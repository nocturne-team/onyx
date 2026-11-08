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
    download::download_all(http, downloads, progress, download::concurrency()).await?;
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

/// A Java runtime found on the system.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JavaInstall {
    /// The `java` / `javaw.exe` binary.
    pub path: PathBuf,
    /// Full version, e.g. `21.0.5`.
    pub version: String,
    /// `8` for `1.8.0_xxx`, otherwise the first number.
    pub major: u32,
}

/// `1.8.0_402` is Java 8; `21.0.5` is Java 21.
fn major_of(version: &str) -> Option<u32> {
    let mut parts = version.split(['.', '_', '+', '-']);
    match parts.next()?.parse().ok()? {
        1 => parts.next()?.parse().ok(),
        major => Some(major),
    }
}

/// Reads a Java binary's version from the `release` file next to its `bin`
/// directory, or by running `java -version` when there isn't one.
pub async fn probe(java: &Path) -> Option<JavaInstall> {
    let java = std::fs::canonicalize(java).ok()?;
    if !java.is_file() {
        return None;
    }
    let home = java.parent()?.parent()?;
    let from_release = std::fs::read_to_string(home.join("release"))
        .ok()
        .and_then(|text| {
            text.lines()
                .find_map(|line| line.strip_prefix("JAVA_VERSION="))
                .map(|v| v.trim_matches('"').to_owned())
        });
    let version = match from_release {
        Some(version) => version,
        None => {
            let mut command = tokio::process::Command::new(&java);
            command.arg("-version").stdin(std::process::Stdio::null());
            #[cfg(windows)]
            {
                const CREATE_NO_WINDOW: u32 = 0x0800_0000;
                command.creation_flags(CREATE_NO_WINDOW);
            }
            let output = tokio::time::timeout(std::time::Duration::from_secs(10), command.output())
                .await
                .ok()?
                .ok()?;
            // `openjdk version "21.0.5" 2024-10-15` (on stderr).
            let text = String::from_utf8_lossy(&output.stderr);
            text.lines().next()?.split('"').nth(1)?.to_owned()
        }
    };
    Some(JavaInstall {
        major: major_of(&version)?,
        version,
        path: java,
    })
}

/// Java installs in the usual places, plus `JAVA_HOME` and `PATH`.
/// Onyx's own Mojang runtimes are left out; those are the automatic choice.
pub async fn detect() -> Vec<JavaInstall> {
    let os = Os::current();
    let binary = |home: &Path| java_binary(home, os);
    let mut candidates: Vec<PathBuf> = Vec::new();

    if let Some(home) = std::env::var_os("JAVA_HOME") {
        candidates.push(binary(Path::new(&home)));
    }
    let name = if os == Os::Windows {
        "javaw.exe"
    } else {
        "java"
    };
    if let Some(path) = std::env::var_os("PATH") {
        candidates.extend(std::env::split_paths(&path).map(|dir| dir.join(name)));
    }

    let roots: Vec<PathBuf> = match os {
        Os::Windows => ["ProgramFiles", "ProgramFiles(x86)"]
            .iter()
            .filter_map(std::env::var_os)
            .flat_map(|base| {
                [
                    "Java",
                    "Eclipse Adoptium",
                    "Microsoft",
                    "Zulu",
                    "BellSoft",
                    "Amazon Corretto",
                ]
                .map(|vendor| Path::new(&base).join(vendor))
            })
            .collect(),
        _ => vec![
            "/usr/lib/jvm".into(),
            "/usr/lib64/jvm".into(),
            "/usr/java".into(),
            "/opt".into(),
            directories::BaseDirs::new()
                .map(|d| d.home_dir().join(".sdkman/candidates/java"))
                .unwrap_or_default(),
        ],
    };
    for root in roots {
        let Ok(entries) = std::fs::read_dir(&root) else {
            continue;
        };
        for entry in entries.filter_map(Result::ok) {
            candidates.push(binary(&entry.path()));
        }
    }

    let mut found: Vec<JavaInstall> = Vec::new();
    for candidate in candidates {
        if !candidate.is_file() {
            continue;
        }
        if let Some(install) = probe(&candidate).await {
            if !found.iter().any(|f| f.path == install.path) {
                found.push(install);
            }
        }
    }
    found.sort_by(|a, b| b.major.cmp(&a.major).then_with(|| a.path.cmp(&b.path)));
    found
}

#[cfg(test)]
mod detect_tests {
    use super::*;

    #[test]
    fn majors() {
        assert_eq!(major_of("1.8.0_402"), Some(8));
        assert_eq!(major_of("21.0.5"), Some(21));
        assert_eq!(major_of("25"), Some(25));
        assert_eq!(major_of("17.0.15+6"), Some(17));
        assert_eq!(major_of("garbage"), None);
    }

    #[tokio::test]
    async fn finds_system_javas_here() {
        // Only meaningful on a machine with Java installed (the dev box has 17 and 21).
        let found = detect().await;
        if Path::new("/usr/lib/jvm/java-21-openjdk/bin/java").exists() {
            assert!(found.iter().any(|j| j.major == 21), "{found:?}");
        }
    }
}
