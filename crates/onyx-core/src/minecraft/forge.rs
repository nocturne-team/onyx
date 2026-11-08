//! Forge and NeoForge, via their installer jars ("spec 1" install profiles,
//! Minecraft 1.13 and later).
//!
//! The installer holds `install_profile.json` (libraries plus "processors":
//! small Java programs that patch the vanilla jar) and `version.json` (a
//! Mojang-style version that inherits from vanilla). We download the
//! libraries, run the client-side processors with the game's Java, and merge
//! `version.json` over vanilla like a Fabric profile.

use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::Deserialize;

use super::loader::{LoaderVersion, ProfileArguments, compare_versions, merge_parts};
use super::version::{Environment, Library, maven_path};
use super::{InstallError, MetaDirs, VersionInfo, get_json, write_file};
use crate::download::{self, Download, Progress};
use crate::instance::Loader;

const FORGE_MAVEN: &str = "https://maven.minecraftforge.net";
const FORGE_VERSIONS: &str =
    "https://files.minecraftforge.net/net/minecraftforge/forge/maven-metadata.json";
const FORGE_PROMOTIONS: &str =
    "https://files.minecraftforge.net/net/minecraftforge/forge/promotions_slim.json";
const NEOFORGE_MAVEN: &str = "https://maven.neoforged.net/releases";
const NEOFORGE_VERSIONS: &str =
    "https://maven.neoforged.net/api/maven/versions/releases/net/neoforged/neoforge";
/// Written once the processors have run; its presence means "installed".
const MARKER: &str = ".onyx-installed";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ForgeLoader {
    Forge,
    NeoForge,
}

impl ForgeLoader {
    pub fn from_loader(loader: Loader) -> Option<Self> {
        match loader {
            Loader::Forge => Some(Self::Forge),
            Loader::NeoForge => Some(Self::NeoForge),
            _ => None,
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::Forge => "forge",
            Self::NeoForge => "neoforge",
        }
    }

    fn installer_url(self, game_version: &str, loader_version: &str) -> String {
        match self {
            Self::Forge => {
                let full = format!("{game_version}-{loader_version}");
                format!("{FORGE_MAVEN}/net/minecraftforge/forge/{full}/forge-{full}-installer.jar")
            }
            Self::NeoForge => format!(
                "{NEOFORGE_MAVEN}/net/neoforged/neoforge/{loader_version}/neoforge-{loader_version}-installer.jar"
            ),
        }
    }
}

#[derive(Deserialize)]
struct Promotions {
    promos: HashMap<String, String>,
}

#[derive(Deserialize)]
struct NeoForgeVersions {
    versions: Vec<String>,
}

/// The Minecraft version a NeoForge version is for: `21.1.77` is 1.21.1,
/// `21.0.5` is 1.21, `26.3.0.101` is 26.3, `26.1.1.4` is 26.1.1.
fn neoforge_game_version(version: &str) -> Option<String> {
    let main = version.split('-').next()?;
    let parts: Vec<u32> = main
        .split('.')
        .map(|p| p.parse().ok())
        .collect::<Option<_>>()?;
    match parts.as_slice() {
        [major, minor, patch, _build] if *major >= 26 => Some(if *patch == 0 {
            format!("{major}.{minor}")
        } else {
            format!("{major}.{minor}.{patch}")
        }),
        [major, minor, _build] if (20..26).contains(major) => Some(if *minor == 0 {
            format!("1.{major}")
        } else {
            format!("1.{major}.{minor}")
        }),
        _ => None,
    }
}

/// Game versions with at least one build. Forge before 1.13 used an older
/// installer format that isn't supported, so those are left out.
pub async fn game_versions(
    http: &reqwest::Client,
    loader: ForgeLoader,
) -> Result<Vec<String>, InstallError> {
    match loader {
        ForgeLoader::Forge => {
            let all: HashMap<String, Vec<String>> =
                get_json(http, FORGE_VERSIONS, "the Forge version list").await?;
            Ok(all
                .into_keys()
                .filter(|v| compare_versions(v, "1.13").is_ge() || !v.starts_with("1."))
                .collect())
        }
        ForgeLoader::NeoForge => {
            let list: NeoForgeVersions =
                get_json(http, NEOFORGE_VERSIONS, "the NeoForge version list").await?;
            let mut games: Vec<String> = list
                .versions
                .iter()
                .filter_map(|v| neoforge_game_version(v))
                .collect();
            games.sort();
            games.dedup();
            Ok(games)
        }
    }
}

/// Loader versions for a game version, newest first. For Forge, "stable" is
/// the promoted "recommended" build (or every build when there is none).
pub async fn loader_versions(
    http: &reqwest::Client,
    loader: ForgeLoader,
    game_version: &str,
) -> Result<Vec<LoaderVersion>, InstallError> {
    let mut versions: Vec<LoaderVersion> = match loader {
        ForgeLoader::Forge => {
            let mut all: HashMap<String, Vec<String>> =
                get_json(http, FORGE_VERSIONS, "the Forge version list").await?;
            let promotions: Promotions =
                get_json(http, FORGE_PROMOTIONS, "the Forge promotions").await?;
            let recommended = promotions
                .promos
                .get(&format!("{game_version}-recommended"))
                .cloned();
            let prefix = format!("{game_version}-");
            all.remove(game_version)
                .unwrap_or_default()
                .into_iter()
                .filter_map(|full| {
                    // Some old builds carry a trailing "-<game version>"; skip those.
                    let version = full.strip_prefix(&prefix)?.to_owned();
                    (!version.contains('-')).then_some(version)
                })
                .map(|version| LoaderVersion {
                    stable: recommended.as_ref().is_none_or(|r| *r == version),
                    version,
                })
                .collect()
        }
        ForgeLoader::NeoForge => {
            let list: NeoForgeVersions =
                get_json(http, NEOFORGE_VERSIONS, "the NeoForge version list").await?;
            list.versions
                .into_iter()
                .filter(|v| neoforge_game_version(v).as_deref() == Some(game_version))
                .map(|version| LoaderVersion {
                    stable: !version.contains("beta") && !version.contains("alpha"),
                    version,
                })
                .collect()
        }
    };
    versions.sort_by(|a, b| compare_versions(&b.version, &a.version));
    Ok(versions)
}

#[derive(Debug, Deserialize)]
struct InstallProfile {
    #[serde(default)]
    data: HashMap<String, SidedValue>,
    #[serde(default)]
    processors: Vec<Processor>,
    #[serde(default)]
    libraries: Vec<Library>,
    /// The old (pre-1.13) format has `versionInfo` instead.
    json: Option<String>,
}

#[derive(Debug, Deserialize)]
struct SidedValue {
    client: String,
}

#[derive(Debug, Deserialize)]
struct Processor {
    sides: Option<Vec<String>>,
    jar: String,
    #[serde(default)]
    classpath: Vec<String>,
    #[serde(default)]
    args: Vec<String>,
    #[serde(default)]
    outputs: HashMap<String, String>,
}

/// `version.json` from the installer.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ForgeVersion {
    pub main_class: String,
    pub arguments: Option<ProfileArguments>,
    #[serde(default)]
    pub libraries: Vec<Library>,
}

/// Vanilla plus the loader's version JSON.
pub fn merge(vanilla: VersionInfo, version: ForgeVersion) -> VersionInfo {
    merge_parts(
        vanilla,
        version.main_class,
        version.arguments,
        version.libraries,
    )
}

/// Installs the loader (once) and returns its version JSON. Needs the vanilla
/// client jar installed already, since the processors patch it. `java` runs
/// the processors.
#[allow(clippy::too_many_arguments)]
pub async fn install(
    http: &reqwest::Client,
    meta: &MetaDirs,
    loader: ForgeLoader,
    vanilla: &VersionInfo,
    loader_version: &str,
    java: &Path,
    env: &Environment,
    progress: &Arc<Progress>,
) -> Result<ForgeVersion, InstallError> {
    let id = format!("{}-{}-{loader_version}", loader.name(), vanilla.id);
    let dir = meta.version_dir(&id);
    let version_path = dir.join("version.json");
    if dir.join(MARKER).is_file()
        && let Ok(bytes) = tokio::fs::read(&version_path).await
        && let Ok(version) = serde_json::from_slice(&bytes)
    {
        return Ok(version);
    }

    let installer = dir.join("installer.jar");
    let url = loader.installer_url(&vanilla.id, loader_version);
    // Both mavens publish a .sha1 next to each file; use it when available.
    let sha1 = super::get_bytes(http, &format!("{url}.sha1"))
        .await
        .ok()
        .and_then(|b| String::from_utf8(b).ok())
        .map(|s| s.trim().to_owned())
        .filter(|s| s.len() == 40);
    download::fetch(
        http,
        &Download {
            url,
            path: installer.clone(),
            sha1,
            size: None,
        },
    )
    .await?;

    let (profile, version_bytes) = {
        let installer = installer.clone();
        let libraries = meta.libraries();
        tokio::task::spawn_blocking(move || read_installer(&installer, &libraries))
            .await
            .expect("reading the installer panicked")?
    };
    let version: ForgeVersion = super::parse(&version_bytes, "the loader's version.json")?;

    // Libraries for the processors and for the game. Ones without a URL are
    // produced by the processors (or were in the installer's maven/ folder).
    let libraries = meta.libraries();
    let downloads: Vec<Download> = profile
        .libraries
        .iter()
        .chain(&version.libraries)
        .filter(|lib| lib.allowed(env))
        .filter_map(|lib| {
            let artifact = lib.artifact()?;
            if artifact.url.is_empty() {
                return None;
            }
            Some(Download {
                url: artifact.url.clone(),
                path: libraries.join(artifact.path.as_ref()?),
                sha1: artifact.sha1.clone(),
                size: artifact.size,
            })
        })
        .collect();
    download::download_all(http, downloads, progress, download::concurrency()).await?;

    run_processors(meta, &profile, vanilla, &installer, &dir, java).await?;

    write_file(&version_path, &version_bytes).await?;
    write_file(&dir.join(MARKER), b"").await?;
    Ok(version)
}

/// Reads both JSON files and unpacks the installer's bundled `maven/` folder
/// into the libraries dir. Blocking.
fn read_installer(
    installer: &Path,
    libraries: &Path,
) -> Result<(InstallProfile, Vec<u8>), InstallError> {
    let invalid = |message: String| InstallError::Extract {
        path: installer.to_owned(),
        message,
    };
    let file = std::fs::File::open(installer).map_err(InstallError::io(installer))?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| invalid(e.to_string()))?;
    let mut read = |name: &str| -> Result<Vec<u8>, InstallError> {
        let mut entry = archive
            .by_name(name)
            .map_err(|_| invalid(format!("no {name} in the installer")))?;
        let mut bytes = Vec::new();
        entry
            .read_to_end(&mut bytes)
            .map_err(|e| invalid(e.to_string()))?;
        Ok(bytes)
    };

    let profile_bytes = read("install_profile.json")?;
    let profile: InstallProfile = super::parse(&profile_bytes, "the installer's profile")?;
    let json = profile
        .json
        .clone()
        .ok_or_else(|| invalid("this is a pre-1.13 installer, which isn't supported".into()))?;
    let version_bytes = read(json.trim_start_matches('/'))?;

    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(|e| invalid(e.to_string()))?;
        let Some(relative) = entry.enclosed_name() else {
            continue;
        };
        let Ok(rest) = relative.strip_prefix("maven") else {
            continue;
        };
        if entry.is_dir() || rest.as_os_str().is_empty() {
            continue;
        }
        let out = libraries.join(rest);
        if out.exists() {
            continue;
        }
        if let Some(parent) = out.parent() {
            std::fs::create_dir_all(parent).map_err(InstallError::io(parent))?;
        }
        let mut bytes = Vec::new();
        entry
            .read_to_end(&mut bytes)
            .map_err(|e| invalid(e.to_string()))?;
        std::fs::write(&out, bytes).map_err(InstallError::io(&out))?;
    }
    Ok((profile, version_bytes))
}

/// Runs the client-side processors in order. Processors whose outputs all
/// exist with the expected hashes are skipped.
async fn run_processors(
    meta: &MetaDirs,
    profile: &InstallProfile,
    vanilla: &VersionInfo,
    installer: &Path,
    work_dir: &Path,
    java: &Path,
) -> Result<(), InstallError> {
    let libraries = meta.libraries();
    let library =
        |coordinate: &str| -> Option<PathBuf> { Some(libraries.join(maven_path(coordinate)?)) };

    // Data values: `[coord]` is a library path, `'x'` a literal, `/path` a
    // file inside the installer (extracted on demand).
    let extracted = work_dir.join("data");
    let mut data: HashMap<String, String> = HashMap::new();
    for (key, value) in &profile.data {
        let value = &value.client;
        let resolved = if let Some(coordinate) =
            value.strip_prefix('[').and_then(|v| v.strip_suffix(']'))
        {
            path_string(&library(coordinate).ok_or_else(|| bad_profile(installer, value))?)
        } else if let Some(literal) = value.strip_prefix('\'').and_then(|v| v.strip_suffix('\'')) {
            literal.to_owned()
        } else if let Some(inner) = value.strip_prefix('/') {
            let out = extracted.join(inner);
            extract_entry(installer, inner, &out)?;
            path_string(&out)
        } else {
            value.clone()
        };
        data.insert(key.clone(), resolved);
    }
    data.insert("SIDE".into(), "client".into());
    data.insert(
        "MINECRAFT_JAR".into(),
        path_string(&meta.client_jar(&vanilla.id)),
    );
    data.insert("MINECRAFT_VERSION".into(), vanilla.id.clone());
    data.insert("ROOT".into(), path_string(&meta.root));
    data.insert("INSTALLER".into(), path_string(installer));
    data.insert("LIBRARY_DIR".into(), path_string(&libraries));

    let substitute = |arg: &str| -> Result<String, InstallError> {
        if let Some(coordinate) = arg.strip_prefix('[').and_then(|a| a.strip_suffix(']')) {
            return Ok(path_string(
                &library(coordinate).ok_or_else(|| bad_profile(installer, arg))?,
            ));
        }
        let mut out = arg.to_owned();
        for (key, value) in &data {
            out = out.replace(&format!("{{{key}}}"), value);
        }
        Ok(out)
    };

    let separator = if cfg!(windows) { ";" } else { ":" };
    for processor in &profile.processors {
        if processor
            .sides
            .as_ref()
            .is_some_and(|sides| !sides.iter().any(|s| s == "client"))
        {
            continue;
        }

        let outputs: Vec<(PathBuf, String)> = processor
            .outputs
            .iter()
            .map(|(path, sha1)| {
                Ok((
                    PathBuf::from(substitute(path)?),
                    substitute(sha1)?.trim_matches('\'').to_owned(),
                ))
            })
            .collect::<Result<_, InstallError>>()?;
        if !outputs.is_empty() && outputs_match(&outputs).await {
            continue;
        }

        let jar = library(&processor.jar).ok_or_else(|| bad_profile(installer, &processor.jar))?;
        let main_class = main_class(&jar)?;
        let mut classpath = vec![path_string(&jar)];
        for coordinate in &processor.classpath {
            classpath.push(path_string(
                &library(coordinate).ok_or_else(|| bad_profile(installer, coordinate))?,
            ));
        }
        let args = processor
            .args
            .iter()
            .map(|a| substitute(a))
            .collect::<Result<Vec<_>, _>>()?;

        tracing::info!("running {} ({main_class})", processor.jar);
        let mut command = tokio::process::Command::new(java);
        command
            .arg("-cp")
            .arg(classpath.join(separator))
            .arg(&main_class)
            .args(&args)
            .current_dir(work_dir)
            .stdin(std::process::Stdio::null())
            // A cancelled install shouldn't leave a processor running.
            .kill_on_drop(true);
        #[cfg(windows)]
        {
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            command.creation_flags(CREATE_NO_WINDOW);
        }
        let output = command.output().await.map_err(InstallError::io(java))?;
        if !output.status.success() {
            let log = String::from_utf8_lossy(&output.stderr);
            let stdout = String::from_utf8_lossy(&output.stdout);
            let tail: Vec<&str> = log.lines().chain(stdout.lines()).rev().take(8).collect();
            return Err(InstallError::Processor {
                jar: processor.jar.clone(),
                output: tail.into_iter().rev().collect::<Vec<_>>().join("\n"),
            });
        }
        // Expected hashes come from the machine that built the installer.
        // Re-compressing jars with another Java/zlib build gives different
        // bytes for the same content (seen with Forge 1.20.1's jarsplitter),
        // so a mismatch after a successful run is only logged.
        if !outputs.is_empty() && !outputs_match(&outputs).await {
            tracing::warn!(
                "{}: output hashes differ from the installer's",
                processor.jar
            );
        }
    }
    let _ = tokio::fs::remove_dir_all(&extracted).await;
    Ok(())
}

fn bad_profile(installer: &Path, what: &str) -> InstallError {
    InstallError::Extract {
        path: installer.to_owned(),
        message: format!("can't resolve {what}"),
    }
}

fn path_string(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

async fn outputs_match(outputs: &[(PathBuf, String)]) -> bool {
    for (path, sha1) in outputs {
        match download::sha1_file(path).await {
            Ok(actual) if actual.eq_ignore_ascii_case(sha1) => {}
            _ => return false,
        }
    }
    true
}

fn extract_entry(installer: &Path, name: &str, out: &Path) -> Result<(), InstallError> {
    let invalid = |message: String| InstallError::Extract {
        path: installer.to_owned(),
        message,
    };
    let file = std::fs::File::open(installer).map_err(InstallError::io(installer))?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| invalid(e.to_string()))?;
    let mut entry = archive
        .by_name(name)
        .map_err(|_| invalid(format!("no {name} in the installer")))?;
    if let Some(parent) = out.parent() {
        std::fs::create_dir_all(parent).map_err(InstallError::io(parent))?;
    }
    let mut file = std::fs::File::create(out).map_err(InstallError::io(out))?;
    std::io::copy(&mut entry, &mut file).map_err(InstallError::io(out))?;
    Ok(())
}

/// `Main-Class` from a jar's manifest.
fn main_class(jar: &Path) -> Result<String, InstallError> {
    let invalid = |message: String| InstallError::Extract {
        path: jar.to_owned(),
        message,
    };
    let file = std::fs::File::open(jar).map_err(InstallError::io(jar))?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| invalid(e.to_string()))?;
    let mut manifest = String::new();
    archive
        .by_name("META-INF/MANIFEST.MF")
        .map_err(|_| invalid("no manifest".into()))?
        .read_to_string(&mut manifest)
        .map_err(|e| invalid(e.to_string()))?;
    manifest
        .lines()
        .find_map(|line| line.strip_prefix("Main-Class:"))
        .map(|class| class.trim().to_owned())
        .ok_or_else(|| invalid("no Main-Class in the manifest".into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn neoforge_versions_map_to_game_versions() {
        assert_eq!(neoforge_game_version("21.1.77").as_deref(), Some("1.21.1"));
        assert_eq!(
            neoforge_game_version("21.0.5-beta").as_deref(),
            Some("1.21")
        );
        assert_eq!(neoforge_game_version("20.4.237").as_deref(), Some("1.20.4"));
        assert_eq!(neoforge_game_version("26.3.0.101").as_deref(), Some("26.3"));
        assert_eq!(neoforge_game_version("26.1.1.4").as_deref(), Some("26.1.1"));
        assert_eq!(neoforge_game_version("0.25w14craftmine.3-beta"), None);
    }

    #[test]
    fn installer_urls() {
        assert_eq!(
            ForgeLoader::Forge.installer_url("1.20.1", "47.4.28"),
            "https://maven.minecraftforge.net/net/minecraftforge/forge/1.20.1-47.4.28/forge-1.20.1-47.4.28-installer.jar"
        );
        assert_eq!(
            ForgeLoader::NeoForge.installer_url("26.3", "26.3.0.101"),
            "https://maven.neoforged.net/releases/net/neoforged/neoforge/26.3.0.101/neoforge-26.3.0.101-installer.jar"
        );
    }
}
