//! Modrinth modpacks (`.mrpack`): a zip with `modrinth.index.json` listing
//! files to download, plus `overrides/` and `client-overrides/` folders that
//! are copied over the instance.
//! Format: https://support.modrinth.com/en/articles/8802351-modrinth-modpack-format-mrpack

use std::collections::HashMap;
use std::io::Read;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::download::{self, Download, DownloadError, Progress};
use crate::instance::Loader;

const INDEX: &str = "modrinth.index.json";
/// Copy of the installed pack's index inside the instance, so an update knows
/// which files came from the pack (and may be removed) and which the user added.
pub const INSTALLED_INDEX: &str = ".onyx/pack-index.json";
/// Hosts the format allows downloads from. Anything else is refused, since a
/// pack could otherwise point the launcher at arbitrary URLs.
const ALLOWED_HOSTS: &[&str] = &[
    "cdn.modrinth.com",
    "github.com",
    "raw.githubusercontent.com",
    "gitlab.com",
];

#[derive(Debug, thiserror::Error)]
pub enum ModpackError {
    #[error("{}: {source}", path.display())]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("not a valid .mrpack: {0}")]
    Invalid(String),
    #[error("the pack lists a file outside the instance: {0}")]
    UnsafePath(String),
    #[error("the pack wants to download from {0}, which modpacks aren't allowed to use")]
    DisallowedHost(String),
    #[error(transparent)]
    Download(#[from] DownloadError),
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Index {
    pub format_version: u32,
    pub game: String,
    pub version_id: String,
    pub name: String,
    pub summary: Option<String>,
    pub files: Vec<IndexFile>,
    pub dependencies: HashMap<String, String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexFile {
    pub path: String,
    pub hashes: HashMap<String, String>,
    pub env: Option<Env>,
    pub downloads: Vec<String>,
    pub file_size: Option<u64>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Env {
    pub client: String,
}

/// What the pack needs: game version and loader.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackTarget {
    pub game_version: String,
    pub loader: Loader,
    pub loader_version: Option<String>,
}

impl Index {
    pub fn target(&self) -> Result<PackTarget, ModpackError> {
        let game_version = self
            .dependencies
            .get("minecraft")
            .cloned()
            .ok_or_else(|| ModpackError::Invalid("no Minecraft version".into()))?;
        let (loader, loader_version) = if let Some(v) = self.dependencies.get("fabric-loader") {
            (Loader::Fabric, Some(v.clone()))
        } else if let Some(v) = self.dependencies.get("quilt-loader") {
            (Loader::Quilt, Some(v.clone()))
        } else if let Some(v) = self.dependencies.get("neoforge") {
            (Loader::NeoForge, Some(v.clone()))
        } else if let Some(v) = self.dependencies.get("forge") {
            (Loader::Forge, Some(v.clone()))
        } else {
            (Loader::Vanilla, None)
        };
        Ok(PackTarget {
            game_version,
            loader,
            loader_version,
        })
    }
}

fn open(mrpack: &Path) -> Result<zip::ZipArchive<std::fs::File>, ModpackError> {
    let file = std::fs::File::open(mrpack).map_err(|source| ModpackError::Io {
        path: mrpack.to_owned(),
        source,
    })?;
    zip::ZipArchive::new(file).map_err(|e| ModpackError::Invalid(e.to_string()))
}

/// Reads `modrinth.index.json` from the pack. Blocking.
pub fn read_index(mrpack: &Path) -> Result<Index, ModpackError> {
    let mut archive = open(mrpack)?;
    let mut entry = archive
        .by_name(INDEX)
        .map_err(|_| ModpackError::Invalid(format!("no {INDEX}")))?;
    let mut text = String::new();
    entry
        .read_to_string(&mut text)
        .map_err(|e| ModpackError::Invalid(e.to_string()))?;
    let index: Index =
        serde_json::from_str(&text).map_err(|e| ModpackError::Invalid(e.to_string()))?;
    if index.game != "minecraft" {
        return Err(ModpackError::Invalid(format!("pack is for {}", index.game)));
    }
    Ok(index)
}

/// A relative path from the pack, checked to stay inside the instance.
fn safe_relative(path: &str) -> Result<PathBuf, ModpackError> {
    let relative = Path::new(path);
    let ok = !path.is_empty()
        && !path.contains('\\')
        && relative
            .components()
            .all(|c| matches!(c, Component::Normal(_)));
    if ok {
        Ok(relative.to_owned())
    } else {
        Err(ModpackError::UnsafePath(path.to_owned()))
    }
}

/// The downloads for client-side files, after path and host checks.
pub fn downloads(index: &Index, game_dir: &Path) -> Result<Vec<Download>, ModpackError> {
    index
        .files
        .iter()
        .filter(|f| f.env.as_ref().is_none_or(|e| e.client != "unsupported"))
        .map(|file| {
            let relative = safe_relative(&file.path)?;
            let url = file
                .downloads
                .first()
                .ok_or_else(|| ModpackError::Invalid(format!("no download for {}", file.path)))?;
            let host = reqwest::Url::parse(url)
                .ok()
                .and_then(|u| (u.scheme() == "https").then(|| u.host_str().map(str::to_owned)))
                .flatten()
                .unwrap_or_default();
            if !ALLOWED_HOSTS.contains(&host.as_str()) {
                return Err(ModpackError::DisallowedHost(if host.is_empty() {
                    url.clone()
                } else {
                    host
                }));
            }
            Ok(Download {
                url: url.clone(),
                path: game_dir.join(relative),
                sha1: Some(
                    file.hashes.get("sha1").cloned().ok_or_else(|| {
                        ModpackError::Invalid(format!("no sha1 for {}", file.path))
                    })?,
                ),
                size: file.file_size,
            })
        })
        .collect()
}

/// Copies `overrides/` then `client-overrides/` into the instance. Blocking.
pub fn extract_overrides(mrpack: &Path, game_dir: &Path) -> Result<(), ModpackError> {
    let mut archive = open(mrpack)?;
    for prefix in ["overrides/", "client-overrides/"] {
        for i in 0..archive.len() {
            let mut entry = archive
                .by_index(i)
                .map_err(|e| ModpackError::Invalid(e.to_string()))?;
            let name = entry.name().to_owned();
            let Some(rest) = name.strip_prefix(prefix) else {
                continue;
            };
            if rest.is_empty() || entry.is_dir() {
                continue;
            }
            let relative = safe_relative(rest.trim_end_matches('/'))?;
            let out = game_dir.join(relative);
            if let Some(parent) = out.parent() {
                std::fs::create_dir_all(parent).map_err(|source| ModpackError::Io {
                    path: parent.to_owned(),
                    source,
                })?;
            }
            let mut file = std::fs::File::create(&out).map_err(|source| ModpackError::Io {
                path: out.clone(),
                source,
            })?;
            std::io::copy(&mut entry, &mut file).map_err(|source| ModpackError::Io {
                path: out.clone(),
                source,
            })?;
        }
    }
    Ok(())
}

/// Downloads the pack's files and applies its overrides to `game_dir`. When
/// the instance already has a pack installed (an update), files the old
/// version listed that the new one drops or changes are removed first;
/// anything the user added is left alone.
pub async fn install(
    http: &reqwest::Client,
    mrpack: &Path,
    index: &Index,
    game_dir: &Path,
    progress: &Arc<Progress>,
) -> Result<(), ModpackError> {
    let items = downloads(index, game_dir)?;
    if let Some(old) = installed_index(game_dir) {
        remove_stale_files(&old, index, game_dir)?;
    }
    download::download_all(http, items, progress, download::DEFAULT_CONCURRENCY).await?;
    let (mrpack, dir) = (mrpack.to_owned(), game_dir.to_owned());
    tokio::task::spawn_blocking(move || extract_overrides(&mrpack, &dir))
        .await
        .expect("override extraction panicked")?;

    let saved = game_dir.join(INSTALLED_INDEX);
    let io = |source| ModpackError::Io {
        path: saved.clone(),
        source,
    };
    std::fs::create_dir_all(saved.parent().expect("has a parent")).map_err(io)?;
    std::fs::write(&saved, serde_json::to_vec(index).expect("index serializes")).map_err(io)
}

/// The index saved by the last install, if any.
pub fn installed_index(game_dir: &Path) -> Option<Index> {
    let bytes = std::fs::read(game_dir.join(INSTALLED_INDEX)).ok()?;
    serde_json::from_slice(&bytes).ok()
}

fn remove_stale_files(old: &Index, new: &Index, game_dir: &Path) -> Result<(), ModpackError> {
    let kept: HashMap<&str, Option<&String>> = new
        .files
        .iter()
        .map(|f| (f.path.as_str(), f.hashes.get("sha1")))
        .collect();
    for file in &old.files {
        let unchanged = kept
            .get(file.path.as_str())
            .is_some_and(|sha1| *sha1 == file.hashes.get("sha1"));
        if unchanged {
            continue;
        }
        // Old indexes come from our own copy, but stay careful anyway.
        let Ok(relative) = safe_relative(&file.path) else {
            continue;
        };
        let path = game_dir.join(relative);
        match std::fs::remove_file(&path) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(source) => return Err(ModpackError::Io { path, source }),
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use super::*;

    fn pack(dir: &Path, index: &str, extra: &[(&str, &str)]) -> PathBuf {
        let path = dir.join("pack.mrpack");
        let mut zip = zip::ZipWriter::new(std::fs::File::create(&path).unwrap());
        let options = zip::write::SimpleFileOptions::default();
        zip.start_file(INDEX, options).unwrap();
        zip.write_all(index.as_bytes()).unwrap();
        for (name, contents) in extra {
            zip.start_file(*name, options).unwrap();
            zip.write_all(contents.as_bytes()).unwrap();
        }
        zip.finish().unwrap();
        path
    }

    const INDEX_JSON: &str = r#"{
        "formatVersion": 1, "game": "minecraft", "versionId": "1.0", "name": "Test Pack",
        "files": [
            {"path": "mods/a.jar", "hashes": {"sha1": "aa", "sha512": "bb"},
             "downloads": ["https://cdn.modrinth.com/data/x/a.jar"], "fileSize": 3},
            {"path": "mods/server-only.jar", "hashes": {"sha1": "cc"}, "env": {"client": "unsupported", "server": "required"},
             "downloads": ["https://cdn.modrinth.com/data/x/s.jar"], "fileSize": 3}
        ],
        "dependencies": {"minecraft": "1.21.1", "fabric-loader": "0.16.5"}
    }"#;

    #[test]
    fn reads_target_and_client_downloads() {
        let dir = tempfile::tempdir().unwrap();
        let index = read_index(&pack(dir.path(), INDEX_JSON, &[])).unwrap();
        assert_eq!(
            index.target().unwrap(),
            PackTarget {
                game_version: "1.21.1".into(),
                loader: Loader::Fabric,
                loader_version: Some("0.16.5".into()),
            }
        );
        let items = downloads(&index, Path::new("/game")).unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].path, Path::new("/game/mods/a.jar"));
        assert_eq!(items[0].sha1.as_deref(), Some("aa"));
    }

    #[test]
    fn rejects_escaping_paths_and_foreign_hosts() {
        let dir = tempfile::tempdir().unwrap();
        let evil = INDEX_JSON.replace("mods/a.jar\"", "../a.jar\"");
        let index = read_index(&pack(dir.path(), &evil, &[])).unwrap();
        assert!(matches!(
            downloads(&index, Path::new("/game")),
            Err(ModpackError::UnsafePath(_))
        ));

        let foreign = INDEX_JSON.replace(
            "https://cdn.modrinth.com/data/x/a.jar",
            "https://evil.example/a.jar",
        );
        let index = read_index(&pack(dir.path(), &foreign, &[])).unwrap();
        assert!(matches!(
            downloads(&index, Path::new("/game")),
            Err(ModpackError::DisallowedHost(host)) if host == "evil.example"
        ));
    }

    #[test]
    fn forge_packs_target_forge() {
        let dir = tempfile::tempdir().unwrap();
        let forge = INDEX_JSON.replace("fabric-loader", "forge");
        let index = read_index(&pack(dir.path(), &forge, &[])).unwrap();
        assert_eq!(index.target().unwrap().loader, Loader::Forge);
    }

    #[test]
    fn updates_remove_dropped_and_changed_files_only() {
        let game = tempfile::tempdir().unwrap();
        let mods = game.path().join("mods");
        std::fs::create_dir_all(&mods).unwrap();
        for name in ["kept.jar", "dropped.jar", "changed.jar", "users-own.jar"] {
            std::fs::write(mods.join(name), name).unwrap();
        }
        let file = |path: &str, sha1: &str| IndexFile {
            path: path.into(),
            hashes: HashMap::from([("sha1".to_owned(), sha1.to_owned())]),
            env: None,
            downloads: vec![],
            file_size: None,
        };
        let index = |files| Index {
            format_version: 1,
            game: "minecraft".into(),
            version_id: "1".into(),
            name: "P".into(),
            summary: None,
            files,
            dependencies: HashMap::new(),
        };
        let old = index(vec![
            file("mods/kept.jar", "a"),
            file("mods/dropped.jar", "b"),
            file("mods/changed.jar", "c"),
            file("../outside.jar", "d"),
        ]);
        let new = index(vec![
            file("mods/kept.jar", "a"),
            file("mods/changed.jar", "c2"),
        ]);
        remove_stale_files(&old, &new, game.path()).unwrap();

        let mut left: Vec<_> = std::fs::read_dir(&mods)
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .collect();
        left.sort();
        assert_eq!(left, ["kept.jar", "users-own.jar"]);
    }

    #[test]
    fn overrides_are_extracted_client_last() {
        let dir = tempfile::tempdir().unwrap();
        let mrpack = pack(
            dir.path(),
            INDEX_JSON,
            &[
                ("overrides/config/a.toml", "common"),
                ("overrides/options.txt", "common"),
                ("client-overrides/options.txt", "client"),
                ("other/ignored.txt", "x"),
            ],
        );
        let game = dir.path().join("game");
        extract_overrides(&mrpack, &game).unwrap();
        assert_eq!(
            std::fs::read_to_string(game.join("config/a.toml")).unwrap(),
            "common"
        );
        assert_eq!(
            std::fs::read_to_string(game.join("options.txt")).unwrap(),
            "client"
        );
        assert!(!game.join("other").exists());
        assert!(!game.join("ignored.txt").exists());
    }
}
