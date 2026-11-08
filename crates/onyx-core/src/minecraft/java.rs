//! Java runtimes from Mojang's own runtime manifest, the same builds the
//! official launcher uses. Each runtime is a flat list of files with hashes,
//! so there is no archive to unpack.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use super::version::Os;
use super::{InstallError, MetaDirs, get_json, write_file};
use crate::download::{self, Download, Progress};

const RUNTIMES_URL: &str = "https://launchermeta.mojang.com/v1/products/java-runtime/2ec0cc96c44e5a76b9c8b7c39df7210883d12871/all.json";
/// Written after a runtime installs completely; its presence means the
/// runtime can be used without going online.
const MARKER: &str = ".onyx-runtime.json";

#[derive(Debug, Deserialize)]
struct RuntimeEntry {
    manifest: ManifestRef,
    version: RuntimeVersion,
}

#[derive(Debug, Deserialize)]
struct ManifestRef {
    sha1: String,
    url: String,
}

#[derive(Debug, Deserialize, Serialize)]
struct RuntimeVersion {
    name: String,
}

#[derive(Debug, Deserialize)]
struct RuntimeManifest {
    files: HashMap<String, RuntimeFile>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
enum RuntimeFile {
    File {
        #[serde(default)]
        executable: bool,
        downloads: RawDownloads,
    },
    Directory,
    Link {
        target: String,
    },
}

#[derive(Debug, Deserialize)]
struct RawDownloads {
    raw: RawFile,
}

#[derive(Debug, Deserialize)]
struct RawFile {
    sha1: String,
    size: u64,
    url: String,
}

#[derive(Debug, Deserialize, Serialize)]
struct Marker {
    manifest_sha1: String,
    version: String,
}

/// Mojang's name for this platform in the runtime manifest.
fn platform(os: Os, arch: &str) -> Option<&'static str> {
    Some(match (os, arch) {
        (Os::Linux, "x86_64") => "linux",
        (Os::Linux, "x86") => "linux-i386",
        (Os::Windows, "x86_64") => "windows-x64",
        (Os::Windows, "x86") => "windows-x86",
        (Os::Windows, "aarch64") => "windows-arm64",
        (Os::MacOs, "x86_64") => "mac-os",
        (Os::MacOs, "aarch64") => "mac-os-arm64",
        _ => return None,
    })
}

/// The `java` (or `javaw.exe`) binary inside a runtime directory.
pub fn java_binary(runtime_dir: &Path, os: Os) -> PathBuf {
    match os {
        Os::Windows => runtime_dir.join("bin").join("javaw.exe"),
        Os::MacOs => runtime_dir.join("jre.bundle/Contents/Home/bin/java"),
        Os::Linux => runtime_dir.join("bin").join("java"),
    }
}

/// Returns the Java binary for `component` (e.g. `java-runtime-delta`),
/// downloading the runtime first if it isn't installed.
pub async fn ensure_runtime(
    http: &reqwest::Client,
    meta: &MetaDirs,
    component: &str,
    progress: &Arc<Progress>,
) -> Result<PathBuf, InstallError> {
    let os = Os::current();
    let dir = meta.runtime(component);
    let java = java_binary(&dir, os);
    if dir.join(MARKER).is_file() && java.is_file() {
        return Ok(java);
    }

    let platform = platform(os, std::env::consts::ARCH)
        .ok_or_else(|| InstallError::NoJavaRuntime(component.to_owned()))?;
    let all: HashMap<String, HashMap<String, Vec<RuntimeEntry>>> =
        get_json(http, RUNTIMES_URL, "the Java runtime list").await?;
    let entry = all
        .get(platform)
        .and_then(|components| components.get(component))
        .and_then(|entries| entries.first())
        .ok_or_else(|| InstallError::NoJavaRuntime(component.to_owned()))?;
    let manifest: RuntimeManifest = get_json(
        http,
        &entry.manifest.url,
        &format!("the {component} file list"),
    )
    .await?;

    let mut downloads = Vec::new();
    let mut executables = Vec::new();
    let mut links = Vec::new();
    for (name, file) in &manifest.files {
        let path = dir.join(name);
        match file {
            RuntimeFile::File {
                executable,
                downloads: files,
            } => {
                if *executable {
                    executables.push(path.clone());
                }
                downloads.push(Download {
                    url: files.raw.url.clone(),
                    path,
                    sha1: Some(files.raw.sha1.clone()),
                    size: Some(files.raw.size),
                });
            }
            RuntimeFile::Directory => {
                tokio::fs::create_dir_all(&path)
                    .await
                    .map_err(InstallError::io(&path))?;
            }
            RuntimeFile::Link { target } => links.push((path, target.clone())),
        }
    }
    download::download_all(http, downloads, progress, download::DEFAULT_CONCURRENCY).await?;
    finish_files(&executables, &links)?;

    let marker = Marker {
        manifest_sha1: entry.manifest.sha1.clone(),
        version: entry.version.name.clone(),
    };
    write_file(
        &dir.join(MARKER),
        &serde_json::to_vec(&marker).expect("marker always serializes"),
    )
    .await?;
    Ok(java)
}

#[cfg(unix)]
fn finish_files(executables: &[PathBuf], links: &[(PathBuf, String)]) -> Result<(), InstallError> {
    use std::os::unix::fs::PermissionsExt;

    for path in executables {
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755))
            .map_err(InstallError::io(path))?;
    }
    for (path, target) in links {
        if path.symlink_metadata().is_ok() {
            continue;
        }
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(InstallError::io(parent))?;
        }
        std::os::unix::fs::symlink(target, path).map_err(InstallError::io(path))?;
    }
    Ok(())
}

/// Windows runtimes have no links and no executable bit.
#[cfg(not(unix))]
fn finish_files(_: &[PathBuf], _: &[(PathBuf, String)]) -> Result<(), InstallError> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runtime_manifest_entries_parse() {
        let manifest: RuntimeManifest = serde_json::from_str(
            r#"{"files": {
                "bin": {"type": "directory"},
                "bin/java": {"type": "file", "executable": true, "downloads": {
                    "raw": {"sha1": "a", "size": 1, "url": "https://x/java"},
                    "lzma": {"sha1": "b", "size": 1, "url": "https://x/java.lzma"}}},
                "legal/java.base/LICENSE": {"type": "link", "target": "../java.base/LICENSE"}
            }}"#,
        )
        .unwrap();
        assert!(matches!(
            manifest.files["bin/java"],
            RuntimeFile::File {
                executable: true,
                ..
            }
        ));
        assert!(matches!(manifest.files["bin"], RuntimeFile::Directory));
        assert!(matches!(
            manifest.files["legal/java.base/LICENSE"],
            RuntimeFile::Link { .. }
        ));
    }

    #[test]
    fn platforms() {
        assert_eq!(platform(Os::Linux, "x86_64"), Some("linux"));
        assert_eq!(platform(Os::Windows, "x86_64"), Some("windows-x64"));
        assert_eq!(platform(Os::Linux, "aarch64"), None);
    }
}
