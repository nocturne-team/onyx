//! Installing and launching Minecraft: version metadata, libraries, assets,
//! Java runtimes and the launch command.
//!
//! Everything shared between instances lives under one "meta" directory:
//!
//! ```text
//! meta/
//!   version_manifest_v2.json
//!   versions/<id>/<id>.json, <id>.jar
//!   libraries/<maven path>
//!   assets/indexes, assets/objects, assets/log_configs, assets/virtual
//!   natives/<version id>/
//!   runtimes/<component>/
//! ```

pub mod forge;
pub mod install;
pub mod java;
pub mod launch;
pub mod loader;
pub mod version;

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use serde::de::DeserializeOwned;

use crate::download::DownloadError;
pub use version::{Environment, ManifestEntry, VersionInfo, VersionManifest};

const MANIFEST_MAX_AGE: Duration = Duration::from_secs(60 * 60);

#[derive(Debug, thiserror::Error)]
pub enum InstallError {
    #[error(transparent)]
    Download(#[from] DownloadError),
    #[error("could not reach {url}: {source}")]
    Http {
        url: String,
        #[source]
        source: reqwest::Error,
    },
    #[error("{url} returned HTTP {status}")]
    Status { url: String, status: u16 },
    #[error("could not read {what}: {source}")]
    Json {
        what: String,
        #[source]
        source: serde_json::Error,
    },
    #[error("{}: {source}", path.display())]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("Minecraft {0} doesn't exist")]
    UnknownVersion(String),
    #[error(
        "Mojang has no Java {0} runtime for this platform; install Java yourself and set it in Settings"
    )]
    NoJavaRuntime(String),
    #[error("could not read {}: {message}", path.display())]
    Extract { path: PathBuf, message: String },
    #[error("the loader installer step {jar} failed:\n{output}")]
    Processor { jar: String, output: String },
}

impl InstallError {
    pub(crate) fn io(path: &Path) -> impl FnOnce(std::io::Error) -> Self + '_ {
        move |source| Self::Io {
            path: path.to_owned(),
            source,
        }
    }
}

/// Paths inside the shared meta directory.
#[derive(Debug, Clone)]
pub struct MetaDirs {
    pub root: PathBuf,
}

impl MetaDirs {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn manifest_file(&self) -> PathBuf {
        self.root.join("version_manifest_v2.json")
    }

    pub fn version_dir(&self, id: &str) -> PathBuf {
        self.root.join("versions").join(id)
    }

    pub fn version_json(&self, id: &str) -> PathBuf {
        self.version_dir(id).join(format!("{id}.json"))
    }

    pub fn client_jar(&self, id: &str) -> PathBuf {
        self.version_dir(id).join(format!("{id}.jar"))
    }

    pub fn libraries(&self) -> PathBuf {
        self.root.join("libraries")
    }

    pub fn assets(&self) -> PathBuf {
        self.root.join("assets")
    }

    pub fn natives(&self, version_id: &str) -> PathBuf {
        self.root.join("natives").join(version_id)
    }

    pub fn runtime(&self, component: &str) -> PathBuf {
        self.root.join("runtimes").join(component)
    }
}

/// The version list, refreshed at most hourly. Falls back to the copy on disk
/// (of any age) when Mojang can't be reached.
pub async fn version_manifest(
    http: &reqwest::Client,
    meta: &MetaDirs,
) -> Result<VersionManifest, InstallError> {
    version_manifest_from(http, meta, version::MANIFEST_URL).await
}

pub(crate) async fn version_manifest_from(
    http: &reqwest::Client,
    meta: &MetaDirs,
    url: &str,
) -> Result<VersionManifest, InstallError> {
    let path = meta.manifest_file();
    let cached = tokio::fs::read(&path).await.ok();
    let fresh = tokio::fs::metadata(&path)
        .await
        .and_then(|m| m.modified())
        .ok()
        .and_then(|modified| SystemTime::now().duration_since(modified).ok())
        .is_some_and(|age| age < MANIFEST_MAX_AGE);

    if let (true, Some(bytes)) = (fresh, &cached) {
        if let Ok(manifest) = serde_json::from_slice(bytes) {
            return Ok(manifest);
        }
    }

    match get_bytes(http, url).await {
        Ok(bytes) => {
            let manifest = parse(&bytes, "the Minecraft version list")?;
            write_file(&path, &bytes).await?;
            Ok(manifest)
        }
        Err(e) => match cached.and_then(|bytes| serde_json::from_slice(&bytes).ok()) {
            Some(manifest) => {
                tracing::warn!("using saved version list: {e}");
                Ok(manifest)
            }
            None => Err(e),
        },
    }
}

/// The version's JSON, downloaded once and then read from disk, so installed
/// versions launch offline.
pub async fn version_info(
    http: &reqwest::Client,
    meta: &MetaDirs,
    id: &str,
) -> Result<VersionInfo, InstallError> {
    let path = meta.version_json(id);
    if let Ok(bytes) = tokio::fs::read(&path).await {
        match serde_json::from_slice(&bytes) {
            Ok(info) => return Ok(info),
            Err(e) => tracing::warn!("re-downloading unreadable {}: {e}", path.display()),
        }
    }

    let manifest = version_manifest(http, meta).await?;
    let entry = manifest
        .versions
        .iter()
        .find(|v| v.id == id)
        .ok_or_else(|| InstallError::UnknownVersion(id.to_owned()))?;
    let bytes = get_bytes(http, &entry.url).await?;
    if crate::download::hex(&<sha1::Sha1 as sha1::Digest>::digest(&bytes)) != entry.sha1 {
        return Err(DownloadError::HashMismatch {
            url: entry.url.clone(),
        }
        .into());
    }
    let info = parse(&bytes, &format!("the Minecraft {id} metadata"))?;
    write_file(&path, &bytes).await?;
    Ok(info)
}

pub(crate) async fn get_bytes(http: &reqwest::Client, url: &str) -> Result<Vec<u8>, InstallError> {
    let http_err = |source| InstallError::Http {
        url: url.to_owned(),
        source,
    };
    let response = http.get(url).send().await.map_err(http_err)?;
    if !response.status().is_success() {
        return Err(InstallError::Status {
            url: url.to_owned(),
            status: response.status().as_u16(),
        });
    }
    Ok(response.bytes().await.map_err(http_err)?.to_vec())
}

pub(crate) async fn get_json<T: DeserializeOwned>(
    http: &reqwest::Client,
    url: &str,
    what: &str,
) -> Result<T, InstallError> {
    parse(&get_bytes(http, url).await?, what)
}

pub(crate) fn parse<T: DeserializeOwned>(bytes: &[u8], what: &str) -> Result<T, InstallError> {
    serde_json::from_slice(bytes).map_err(|source| InstallError::Json {
        what: what.to_owned(),
        source,
    })
}

/// Writes via a temp file so readers never see half a file.
pub(crate) async fn write_file(path: &Path, bytes: &[u8]) -> Result<(), InstallError> {
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(InstallError::io(parent))?;
    }
    let temp = path.with_extension("onyx-part");
    tokio::fs::write(&temp, bytes)
        .await
        .map_err(InstallError::io(&temp))?;
    tokio::fs::rename(&temp, path)
        .await
        .map_err(InstallError::io(path))
}

#[cfg(test)]
mod tests {
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;

    const MANIFEST: &str = r#"{"latest":{"release":"1.0","snapshot":"1.1-pre"},
        "versions":[{"id":"1.0","type":"release","url":"https://x/1.0.json",
        "time":"t","releaseTime":"2011-11-18T22:00:00+00:00","sha1":"abc","complianceLevel":0}]}"#;

    #[tokio::test]
    async fn manifest_is_cached_and_served_stale_when_offline() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/manifest"))
            .respond_with(ResponseTemplate::new(200).set_body_string(MANIFEST))
            .expect(1)
            .mount(&server)
            .await;
        let dir = tempfile::tempdir().unwrap();
        let meta = MetaDirs::new(dir.path());
        let http = reqwest::Client::new();
        let url = format!("{}/manifest", server.uri());

        let manifest = version_manifest_from(&http, &meta, &url).await.unwrap();
        assert_eq!(manifest.latest.release, "1.0");
        // Second call is served from disk (the mock expects exactly one hit).
        version_manifest_from(&http, &meta, &url).await.unwrap();

        // Stale and offline: still served.
        let old = SystemTime::now() - Duration::from_secs(2 * 60 * 60);
        std::fs::File::options()
            .write(true)
            .open(meta.manifest_file())
            .unwrap()
            .set_modified(old)
            .unwrap();
        let manifest = version_manifest_from(&http, &meta, "http://127.0.0.1:1/offline")
            .await
            .unwrap();
        assert_eq!(manifest.versions[0].kind, "release");
    }
}
