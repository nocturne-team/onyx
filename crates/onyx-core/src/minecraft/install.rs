//! Downloads everything a vanilla version needs to launch.

use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use super::version::{Environment, VersionInfo};
use super::{InstallError, MetaDirs, get_bytes, parse, write_file};
use crate::download::{self, Download, Progress};

const RESOURCES_URL: &str = "https://resources.download.minecraft.net";

#[derive(Debug, Deserialize, Serialize)]
pub struct AssetIndex {
    pub objects: HashMap<String, AssetObject>,
    /// Pre-1.7 versions read assets by name from `assets/virtual/legacy`.
    #[serde(default, rename = "virtual")]
    pub is_virtual: bool,
    /// Pre-1.6 versions read assets from `<game dir>/resources`.
    #[serde(default)]
    pub map_to_resources: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AssetObject {
    pub hash: String,
    pub size: u64,
}

/// Installs the client jar, libraries, natives, assets and logging config for
/// `version`. Safe to run before every launch: files already present are skipped.
pub async fn install_version(
    http: &reqwest::Client,
    meta: &MetaDirs,
    version: &VersionInfo,
    env: &Environment,
    progress: &Arc<Progress>,
) -> Result<(), InstallError> {
    let index = asset_index(http, meta, version).await?;

    let mut downloads = vec![Download {
        url: version.downloads.client.url.clone(),
        path: meta.client_jar(&version.id),
        sha1: version.downloads.client.sha1.clone(),
        size: version.downloads.client.size,
    }];
    downloads.extend(library_downloads(meta, version, env));
    downloads.extend(index.objects.values().map(|object| Download {
        url: format!("{RESOURCES_URL}/{}/{}", &object.hash[..2], object.hash),
        path: object_path(meta, &object.hash),
        sha1: Some(object.hash.clone()),
        size: Some(object.size),
    }));
    if let Some(logging) = version.logging.as_ref().and_then(|l| l.client.as_ref()) {
        downloads.push(Download {
            url: logging.file.url.clone(),
            path: log_config_path(meta, &logging.file.id),
            sha1: Some(logging.file.sha1.clone()),
            size: Some(logging.file.size),
        });
    }

    download::download_all(http, downloads, progress, download::concurrency()).await?;

    extract_natives(meta, version, env).await?;
    if index.is_virtual {
        copy_assets(meta, &index, &meta.assets().join("virtual").join("legacy")).await?;
    }
    Ok(())
}

/// Reads the asset index from disk, downloading it first if needed.
pub async fn asset_index(
    http: &reqwest::Client,
    meta: &MetaDirs,
    version: &VersionInfo,
) -> Result<AssetIndex, InstallError> {
    let path = meta
        .assets()
        .join("indexes")
        .join(format!("{}.json", version.asset_index.id));
    let what = format!("asset index {}", version.asset_index.id);
    if let Ok(bytes) = tokio::fs::read(&path).await
        && let Ok(index) = parse(&bytes, &what)
    {
        return Ok(index);
    }
    let bytes = get_bytes(http, &version.asset_index.url).await?;
    let index = parse(&bytes, &what)?;
    write_file(&path, &bytes).await?;
    Ok(index)
}

pub fn object_path(meta: &MetaDirs, hash: &str) -> PathBuf {
    meta.assets().join("objects").join(&hash[..2]).join(hash)
}

pub fn log_config_path(meta: &MetaDirs, id: &str) -> PathBuf {
    meta.assets().join("log_configs").join(id)
}

fn library_downloads(meta: &MetaDirs, version: &VersionInfo, env: &Environment) -> Vec<Download> {
    let libraries = meta.libraries();
    version
        .libraries
        .iter()
        .filter(|lib| lib.allowed(env))
        .flat_map(|lib| [lib.artifact(), lib.native_artifact(env)])
        .flatten()
        .filter_map(|artifact| {
            Some(Download {
                url: artifact.url.clone(),
                path: libraries.join(artifact.path.as_ref()?),
                sha1: artifact.sha1.clone(),
                size: artifact.size,
            })
        })
        // Some library entries have no URL (provided by the launcher); skip them.
        .filter(|d| !d.url.is_empty())
        .collect()
}

/// Unpacks old-style natives jars (pre-1.19) into the version's natives dir.
/// Newer versions ship natives as normal classpath jars that LWJGL unpacks itself.
async fn extract_natives(
    meta: &MetaDirs,
    version: &VersionInfo,
    env: &Environment,
) -> Result<(), InstallError> {
    let jars: Vec<(PathBuf, Vec<String>)> = version
        .libraries
        .iter()
        .filter(|lib| lib.allowed(env))
        .filter_map(|lib| {
            let artifact = lib.native_artifact(env)?;
            let exclude = lib.extract.clone().unwrap_or_default().exclude;
            Some((meta.libraries().join(artifact.path.as_ref()?), exclude))
        })
        .collect();
    if jars.is_empty() {
        return Ok(());
    }

    let dest = meta.natives(&version.id);
    tokio::task::spawn_blocking(move || {
        for (jar, exclude) in jars {
            extract_jar(&jar, &dest, &exclude)?;
        }
        Ok(())
    })
    .await
    .expect("natives extraction panicked")
}

fn extract_jar(jar: &Path, dest: &Path, exclude: &[String]) -> Result<(), InstallError> {
    let extract_err = |message: String| InstallError::Extract {
        path: jar.to_owned(),
        message,
    };
    let file = std::fs::File::open(jar).map_err(InstallError::io(jar))?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| extract_err(e.to_string()))?;
    for i in 0..archive.len() {
        let mut entry = archive
            .by_index(i)
            .map_err(|e| extract_err(e.to_string()))?;
        let Some(relative) = entry.enclosed_name() else {
            continue;
        };
        let name = relative.to_string_lossy().replace('\\', "/");
        if entry.is_dir()
            || exclude
                .iter()
                .any(|prefix| name.starts_with(prefix.as_str()))
        {
            continue;
        }
        let out = dest.join(&relative);
        if out.exists() {
            continue;
        }
        if let Some(parent) = out.parent() {
            std::fs::create_dir_all(parent).map_err(InstallError::io(parent))?;
        }
        let mut bytes = Vec::with_capacity(entry.size() as usize);
        entry
            .read_to_end(&mut bytes)
            .map_err(|e| extract_err(e.to_string()))?;
        std::fs::write(&out, bytes).map_err(InstallError::io(&out))?;
    }
    Ok(())
}

/// Copies hashed objects to their real names, for versions that read assets
/// by path (virtual indexes and `resources/`).
pub async fn copy_assets(
    meta: &MetaDirs,
    index: &AssetIndex,
    dest: &Path,
) -> Result<(), InstallError> {
    for (name, object) in &index.objects {
        let target = dest.join(name);
        if tokio::fs::metadata(&target)
            .await
            .is_ok_and(|m| m.len() == object.size)
        {
            continue;
        }
        if let Some(parent) = target.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(InstallError::io(parent))?;
        }
        let source = object_path(meta, &object.hash);
        tokio::fs::copy(&source, &target)
            .await
            .map_err(InstallError::io(&source))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use super::*;
    use crate::minecraft::version::Os;

    fn version(json: &str) -> VersionInfo {
        serde_json::from_str(json).unwrap()
    }

    const OLD_VERSION: &str = r#"{
        "id": "1.8.9", "type": "release", "mainClass": "net.minecraft.client.main.Main",
        "minecraftArguments": "--username ${auth_player_name}",
        "assetIndex": {"id": "1.8", "sha1": "a", "size": 1, "totalSize": 1, "url": "https://x/1.8.json"},
        "assets": "1.8",
        "downloads": {"client": {"sha1": "c", "size": 3, "url": "https://x/client.jar"}},
        "libraries": [
            {"name": "com.mojang:a:1", "downloads": {"artifact":
                {"path": "com/mojang/a/1/a-1.jar", "sha1": "1", "size": 1, "url": "https://x/a.jar"}}},
            {"name": "org.lwjgl.lwjgl:lwjgl-platform:2.9.4",
             "natives": {"linux": "natives-linux", "windows": "natives-windows-${arch}"},
             "extract": {"exclude": ["META-INF/"]},
             "downloads": {"classifiers": {
                "natives-linux": {"path": "p/natives-linux.jar", "sha1": "2", "size": 2, "url": "https://x/nl.jar"},
                "natives-windows-64": {"path": "p/natives-windows-64.jar", "sha1": "3", "size": 3, "url": "https://x/nw.jar"}}}},
            {"name": "ca.weblite:java-objc-bridge:1.0",
             "rules": [{"action": "allow", "os": {"name": "osx"}}],
             "downloads": {"artifact": {"path": "o.jar", "sha1": "4", "size": 4, "url": "https://x/o.jar"}}}
        ]
    }"#;

    fn env(os: Os) -> Environment {
        Environment {
            os,
            arch: "x86_64",
            os_version: None,
            features: Vec::new(),
        }
    }

    #[test]
    fn library_downloads_follow_rules_and_natives() {
        let meta = MetaDirs::new("/meta");
        let v = version(OLD_VERSION);

        let linux: Vec<String> = library_downloads(&meta, &v, &env(Os::Linux))
            .into_iter()
            .map(|d| d.url)
            .collect();
        assert_eq!(linux, ["https://x/a.jar", "https://x/nl.jar"]);

        let windows: Vec<String> = library_downloads(&meta, &v, &env(Os::Windows))
            .into_iter()
            .map(|d| d.url)
            .collect();
        assert_eq!(windows, ["https://x/a.jar", "https://x/nw.jar"]);
    }

    #[test]
    fn natives_extraction_skips_excluded_entries() {
        let dir = tempfile::tempdir().unwrap();
        let jar = dir.path().join("natives.jar");
        let mut zip = zip::ZipWriter::new(std::fs::File::create(&jar).unwrap());
        let options = zip::write::SimpleFileOptions::default();
        zip.start_file("liblwjgl.so", options).unwrap();
        zip.write_all(b"so").unwrap();
        zip.start_file("META-INF/MANIFEST.MF", options).unwrap();
        zip.write_all(b"mf").unwrap();
        zip.finish().unwrap();

        let out = dir.path().join("out");
        extract_jar(&jar, &out, &["META-INF/".into()]).unwrap();
        assert_eq!(std::fs::read(out.join("liblwjgl.so")).unwrap(), b"so");
        assert!(!out.join("META-INF").exists());
    }
}
