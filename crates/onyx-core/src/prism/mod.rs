//! Importing instances from Prism Launcher (and MultiMC, which uses the same
//! format): `instance.cfg` (Qt INI), `mmc-pack.json` (components) and the game
//! folder (`minecraft/` or `.minecraft/`). Exported instances (`.zip`) are
//! in [`archive`]; instances with their own component patches launch through
//! [`profile`].

pub mod archive;
pub mod profile;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;

use serde::{Deserialize, Serialize};

use crate::download::Progress;
use crate::instance::{
    self, ICON_EXTENSIONS, Instance, InstanceError, Loader, NewInstance, PackSource,
};

#[derive(Debug, thiserror::Error)]
pub enum PrismError {
    #[error("{}: {source}", path.display())]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("{} doesn't look like a Prism Launcher or MultiMC folder", .0.display())]
    NotPrism(PathBuf),
    #[error("no Prism instance called {0}")]
    NotFound(String),
    #[error("can't import {name}: {reason}")]
    Unsupported { name: String, reason: String },
    #[error("{0}")]
    Profile(String),
    #[error("{}: {message}", path.display())]
    Archive { path: PathBuf, message: String },
    #[error(transparent)]
    Instance(#[from] InstanceError),
}

pub(crate) fn io_err(path: &Path) -> impl FnOnce(std::io::Error) -> PrismError + '_ {
    move |source| PrismError::Io {
        path: path.to_owned(),
        source,
    }
}

/// Config file names, Prism first.
const CONFIG_FILES: &[&str] = &["prismlauncher.cfg", "multimc.cfg"];

/// Where Prism usually keeps its data, existing folders only. Portable
/// installs aren't found; the user can pick those by hand.
pub fn default_roots() -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(dirs) = directories::BaseDirs::new() {
        // ~/.local/share/PrismLauncher on Linux, %APPDATA%\PrismLauncher on Windows.
        candidates.push(dirs.data_dir().join("PrismLauncher"));
        if cfg!(target_os = "linux") {
            candidates.push(
                dirs.home_dir()
                    .join(".var/app/org.prismlauncher.PrismLauncher/data/PrismLauncher"),
            );
        }
    }
    candidates.into_iter().filter(|dir| is_root(dir)).collect()
}

/// Whether `dir` is a Prism/MultiMC data folder.
pub fn is_root(dir: &Path) -> bool {
    CONFIG_FILES.iter().any(|name| dir.join(name).is_file()) || dir.join("instances").is_dir()
}

/// The launcher's instance and icon folders, from its config when set.
fn launcher_dirs(root: &Path) -> (PathBuf, PathBuf) {
    let config = CONFIG_FILES
        .iter()
        .find_map(|name| std::fs::read_to_string(root.join(name)).ok())
        .map(|text| parse_ini(&text))
        .unwrap_or_default();
    // Relative paths are relative to the data folder.
    let dir = |key: &str, default: &str| {
        let value = config.get(key).map(String::as_str).unwrap_or(default);
        root.join(if value.is_empty() { default } else { value })
    };
    (dir("InstanceDir", "instances"), dir("IconsDir", "icons"))
}

/// One Prism instance, as shown in the import list.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PrismInstance {
    /// Folder name inside Prism's instances folder. This is what gets imported.
    pub folder: String,
    pub name: String,
    pub game_version: Option<String>,
    pub loader: Option<Loader>,
    pub loader_version: Option<String>,
    /// Why this instance can't be imported, if it can't.
    pub unsupported: Option<String>,
    /// A custom icon image from Prism's icon folder.
    pub icon_path: Option<PathBuf>,
    /// Unix seconds.
    pub last_played: Option<u64>,
    pub played_secs: u64,
    /// "CurseForge", "Modrinth", ... for instances installed from a modpack.
    pub pack_platform: Option<String>,
    #[serde(skip)]
    icon_key: Option<String>,
    #[serde(skip)]
    settings: InstanceSettings,
}

#[derive(Debug, Clone, Default)]
struct InstanceSettings {
    max_memory_mb: Option<u32>,
    java_path: Option<PathBuf>,
    pack: Option<PackSource>,
    /// Has component patches, so it launches from its Prism profile.
    prism_components: bool,
}

/// Every instance in a Prism data folder, sorted by name. Folders that aren't
/// instances are skipped.
pub fn list(root: &Path) -> Result<Vec<PrismInstance>, PrismError> {
    if !is_root(root) {
        return Err(PrismError::NotPrism(root.to_owned()));
    }
    let (instances_dir, icons_dir) = launcher_dirs(root);
    let entries = match std::fs::read_dir(&instances_dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(io_err(&instances_dir)(e)),
    };
    let mut found: Vec<PrismInstance> = entries
        .filter_map(Result::ok)
        .filter(|entry| entry.path().join("instance.cfg").is_file())
        .filter_map(|entry| {
            let folder = entry.file_name().to_str()?.to_owned();
            read_instance(&entry.path(), &folder, &icons_dir)
                .inspect_err(|e| tracing::warn!("skipping Prism instance {folder}: {e}"))
                .ok()
        })
        .collect();
    found.sort_by_key(|i| i.name.to_lowercase());
    Ok(found)
}

fn read_instance(dir: &Path, folder: &str, icons_dir: &Path) -> Result<PrismInstance, PrismError> {
    let cfg_path = dir.join("instance.cfg");
    let cfg = std::fs::read_to_string(&cfg_path).map_err(io_err(&cfg_path))?;
    let pack = std::fs::read(dir.join("mmc-pack.json")).ok();
    let mut found = parse_instance(
        folder,
        &cfg,
        pack.as_deref(),
        game_folder(dir).is_some(),
        profile::has_patches(dir),
    );
    found.icon_path = found
        .icon_key
        .as_deref()
        .and_then(|key| find_icon(icons_dir, key));
    Ok(found)
}

/// Reads an instance from its `instance.cfg` and `mmc-pack.json` contents.
/// The caller fills in `icon_path`.
fn parse_instance(
    folder: &str,
    cfg: &str,
    pack: Option<&[u8]>,
    has_game_folder: bool,
    has_patches: bool,
) -> PrismInstance {
    let cfg = parse_ini(cfg);
    let get = |key: &str| cfg.get(key).map(String::as_str).filter(|v| !v.is_empty());
    let flag = |key: &str| get(key) == Some("true");

    let name = get("name").unwrap_or(folder).to_owned();
    let mut unsupported = None;
    if get("InstanceType").is_some_and(|t| t != "OneSix") {
        unsupported = Some("this is a legacy MultiMC instance".to_owned());
    }

    let (game_version, loader, loader_version) = match pack {
        Some(bytes) => match serde_json::from_slice::<MmcPack>(bytes) {
            Ok(pack) => match pack.target(has_patches) {
                Ok(target) => target,
                Err(reason) => {
                    unsupported.get_or_insert(reason);
                    (None, None, None)
                }
            },
            Err(e) => {
                unsupported.get_or_insert(format!("mmc-pack.json is unreadable: {e}"));
                (None, None, None)
            }
        },
        None => {
            unsupported.get_or_insert("it has no mmc-pack.json".to_owned());
            (None, None, None)
        }
    };
    if !has_game_folder {
        unsupported.get_or_insert("it has no game folder".to_owned());
    }

    let max_memory_mb = flag("OverrideMemory")
        .then(|| get("MaxMemAlloc")?.parse().ok())
        .flatten();
    // Prism's automatic Java lives in Prism's own folder; Onyx picks its own.
    let java_path = (flag("OverrideJavaLocation") && !flag("AutomaticJava"))
        .then(|| get("JavaPath").map(PathBuf::from))
        .flatten()
        .filter(|path| path.is_file());

    let pack_platform = flag("ManagedPack").then(|| {
        match get("ManagedPackType").unwrap_or_default() {
            "flame" => "CurseForge",
            "modrinth" => "Modrinth",
            "technic" => "Technic",
            "atlauncher" => "ATLauncher",
            "ftb" => "FTB",
            _ => "a modpack",
        }
        .to_owned()
    });
    let pack = (get("ManagedPackType") == Some("modrinth"))
        .then(|| {
            Some(PackSource {
                project_id: get("ManagedPackID")?.to_owned(),
                version_id: get("ManagedPackVersionID")?.to_owned(),
                version_number: get("ManagedPackVersionName").unwrap_or_default().to_owned(),
            })
        })
        .flatten();

    PrismInstance {
        folder: folder.to_owned(),
        name,
        game_version,
        loader,
        loader_version,
        unsupported,
        icon_path: None,
        icon_key: get("iconKey").map(str::to_owned),
        last_played: get("lastLaunchTime")
            .and_then(|ms| ms.parse::<u64>().ok())
            .filter(|&ms| ms > 0)
            .map(|ms| ms / 1000),
        played_secs: get("totalTimePlayed")
            .and_then(|s| s.parse().ok())
            .unwrap_or(0),
        pack_platform,
        settings: InstanceSettings {
            max_memory_mb,
            java_path,
            pack,
            prism_components: has_patches,
        },
    }
}

/// A user icon in Prism's icon folder: `<key>.<ext>`. Built-in keys like
/// `default` or `fabricmc` have no file there and give `None`.
fn find_icon(icons_dir: &Path, key: &str) -> Option<PathBuf> {
    if key.contains(['/', '\\']) || key.contains("..") {
        return None;
    }
    ICON_EXTENSIONS
        .iter()
        .map(|ext| icons_dir.join(format!("{key}.{ext}")))
        .find(|path| path.is_file())
}

/// `minecraft/` in Prism, `.minecraft/` in older MultiMC instances.
fn game_folder(dir: &Path) -> Option<PathBuf> {
    ["minecraft", ".minecraft"]
        .into_iter()
        .map(|name| dir.join(name))
        .find(|path| path.is_dir())
}

#[derive(Debug, Deserialize)]
struct MmcPack {
    #[serde(default)]
    components: Vec<Component>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Component {
    uid: String,
    #[serde(default)]
    version: Option<String>,
    #[serde(default)]
    cached_name: Option<String>,
    #[serde(default)]
    disabled: bool,
}

/// Components that need nothing from us: LWJGL comes with the version JSON,
/// intermediary/hashed mappings come with the loader profile.
const PASSIVE_COMPONENTS: &[&str] = &[
    "org.lwjgl",
    "org.lwjgl3",
    "net.fabricmc.intermediary",
    "org.quiltmc.hashed",
];

impl MmcPack {
    /// Game version, loader and loader version, or why Onyx can't run it.
    /// With `components` (the instance has its own patches) it launches from
    /// its Prism profile, so components Onyx has no native support for are
    /// fine and the loader is only for display and content filtering.
    #[allow(clippy::type_complexity)]
    fn target(
        &self,
        components: bool,
    ) -> Result<(Option<String>, Option<Loader>, Option<String>), String> {
        let mut game = None;
        let mut loader = None;
        for c in self.components.iter().filter(|c| !c.disabled) {
            let found = match c.uid.as_str() {
                "net.minecraft" => {
                    game = c.version.clone();
                    continue;
                }
                "net.fabricmc.fabric-loader" => Loader::Fabric,
                "org.quiltmc.quilt-loader" => Loader::Quilt,
                "net.minecraftforge" => Loader::Forge,
                "net.neoforged" => Loader::NeoForge,
                uid if PASSIVE_COMPONENTS.contains(&uid) => continue,
                _ if components => continue,
                uid => {
                    let what = c.cached_name.as_deref().unwrap_or(uid);
                    return Err(format!("it uses {what}, which Onyx doesn't support"));
                }
            };
            if loader.is_some() {
                return Err("it has more than one mod loader".to_owned());
            }
            let version = c.version.clone().ok_or("its loader has no version")?;
            loader = Some((found, version));
        }
        let game = game.ok_or("it has no Minecraft version")?;
        if !components
            && loader.as_ref().is_some_and(|(l, _)| *l == Loader::Forge)
            && before_1_13(&game)
        {
            return Err("Forge before Minecraft 1.13 isn't supported yet".to_owned());
        }
        let (loader, version) = match loader {
            Some((loader, version)) => (loader, Some(version)),
            None => (Loader::Vanilla, None),
        };
        Ok((Some(game), Some(loader), version))
    }
}

/// `1.12.2` and older. Snapshots and `26.x` versions count as newer.
fn before_1_13(game: &str) -> bool {
    let mut parts = game.split('.');
    match (
        parts.next(),
        parts.next().and_then(|m| m.parse::<u32>().ok()),
    ) {
        (Some("1"), Some(minor)) => minor < 13,
        _ => false,
    }
}

/// Keys of the `[General]` section (or of a file with no sections, as older
/// MultiMC wrote). Qt-quoted values are unquoted.
fn parse_ini(text: &str) -> HashMap<String, String> {
    let mut values = HashMap::new();
    let mut in_general = true;
    for line in text.lines() {
        let line = line.trim();
        if let Some(section) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            in_general = section == "General";
            continue;
        }
        if !in_general || line.starts_with([';', '#']) {
            continue;
        }
        if let Some((key, value)) = line.split_once('=') {
            values
                .entry(key.trim().to_owned())
                .or_insert_with(|| unquote(value.trim()));
        }
    }
    values
}

/// Undoes QSettings' INI escaping: `"..."` quoting, `\"`, `\\`, `\n`, `\t`,
/// `\r` and `\xHHHH` escapes.
fn unquote(value: &str) -> String {
    let inner = value
        .strip_prefix('"')
        .and_then(|v| v.strip_suffix('"'))
        .unwrap_or(value);
    let mut out = String::with_capacity(inner.len());
    let mut chars = inner.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('r') => out.push('\r'),
            Some('x') => {
                let mut hex = String::new();
                while hex.len() < 4 && chars.peek().is_some_and(char::is_ascii_hexdigit) {
                    hex.push(chars.next().unwrap());
                }
                match u32::from_str_radix(&hex, 16).ok().and_then(char::from_u32) {
                    Some(c) => out.push(c),
                    None => out.push_str(&format!("\\x{hex}")),
                }
            }
            Some(other) => out.push(other),
            None => out.push('\\'),
        }
    }
    out
}

/// Top-level names in the game folder that would clash with Onyx's own files.
const RESERVED: &[&str] = &[instance::INSTANCE_FILE, ".onyx"];

/// Creates an Onyx instance for a Prism instance and copies its icon and
/// settings. The game folder is copied separately with [`copy_game_folder`],
/// so the instance can show up while that runs.
pub fn create(
    root: &Path,
    onyx_root: &Path,
    prism: &PrismInstance,
) -> Result<Instance, PrismError> {
    let icon = prism.icon_path.as_deref().and_then(|path| {
        let extension = path.extension()?.to_str()?.to_owned();
        Some((std::fs::read(path).ok()?, extension))
    });
    let created = create_instance(onyx_root, prism, icon)?;
    if prism.settings.prism_components {
        let (instances_dir, _) = launcher_dirs(root);
        let source = instances_dir.join(&prism.folder);
        let target = profile::profile_dir(&instance::instance_dir(onyx_root, &created.id)?);
        copy_profile(&source, &target)?;
    }
    Ok(created)
}

/// Copies `mmc-pack.json`, `patches/*.json` and `libraries/*` (flat, files
/// only) from a Prism instance folder.
fn copy_profile(source: &Path, target: &Path) -> Result<(), PrismError> {
    for sub in ["patches", "libraries"] {
        let to = target.join(sub);
        std::fs::create_dir_all(&to).map_err(io_err(&to))?;
        let from = source.join(sub);
        let Ok(entries) = std::fs::read_dir(&from) else {
            continue;
        };
        for entry in entries.filter_map(Result::ok) {
            if entry.file_type().is_ok_and(|t| t.is_file()) {
                let path = entry.path();
                std::fs::copy(&path, to.join(entry.file_name())).map_err(io_err(&path))?;
            }
        }
    }
    let pack = source.join("mmc-pack.json");
    std::fs::copy(&pack, target.join("mmc-pack.json")).map_err(io_err(&pack))?;
    Ok(())
}

/// The instance itself, with its icon (bytes and extension) and settings.
fn create_instance(
    onyx_root: &Path,
    prism: &PrismInstance,
    icon: Option<(Vec<u8>, String)>,
) -> Result<Instance, PrismError> {
    if let Some(reason) = &prism.unsupported {
        return Err(PrismError::Unsupported {
            name: prism.name.clone(),
            reason: reason.clone(),
        });
    }
    let unsupported = |reason: &str| PrismError::Unsupported {
        name: prism.name.clone(),
        reason: reason.to_owned(),
    };
    let created = instance::create(
        onyx_root,
        NewInstance {
            name: prism.name.clone(),
            game_version: prism
                .game_version
                .clone()
                .ok_or_else(|| unsupported("no game version"))?,
            loader: prism.loader.ok_or_else(|| unsupported("no loader"))?,
            loader_version: prism.loader_version.clone(),
            icon_config: None,
        },
    )?;
    let mut created = match icon {
        Some((bytes, extension)) => instance::set_icon(onyx_root, &created.id, &bytes, &extension)
            .inspect_err(|e| tracing::warn!("not importing the icon of {}: {e}", prism.name))
            .unwrap_or(created),
        None => created,
    };
    created.last_played = prism.last_played;
    created.played_secs = prism.played_secs;
    created.max_memory_mb = prism.settings.max_memory_mb;
    created.java_path = prism.settings.java_path.clone();
    // A pack update would install Onyx's own loader setup over a profile it
    // can't change, so component instances don't track their pack.
    if !prism.settings.prism_components {
        created.pack = prism.settings.pack.clone();
    }
    created.prism_components = prism.settings.prism_components;
    instance::save(onyx_root, &created)?;
    Ok(created)
}

/// Copies a Prism instance's game folder (saves, mods, config, ...) into an
/// Onyx instance folder, reporting files and bytes on `progress`. Symlinks are
/// skipped. Existing files are overwritten.
pub async fn copy_game_folder(
    root: &Path,
    prism: &PrismInstance,
    target: &Path,
    progress: &Progress,
) -> Result<(), PrismError> {
    let (instances_dir, _) = launcher_dirs(root);
    let source = game_folder(&instances_dir.join(&prism.folder))
        .ok_or_else(|| PrismError::NotFound(prism.folder.clone()))?;

    // List everything first so progress has a total.
    let mut files = Vec::new();
    let mut dirs = vec![PathBuf::new()];
    while let Some(relative) = dirs.pop() {
        let dir = source.join(&relative);
        let mut entries = tokio::fs::read_dir(&dir).await.map_err(io_err(&dir))?;
        while let Some(entry) = entries.next_entry().await.map_err(io_err(&dir))? {
            let name = entry.file_name();
            if relative.as_os_str().is_empty() && RESERVED.iter().any(|r| name.to_str() == Some(r))
            {
                continue;
            }
            let path = relative.join(&name);
            let kind = entry.file_type().await.map_err(io_err(&entry.path()))?;
            if kind.is_dir() {
                // Created now so empty folders (resourcepacks/, a world's
                // datapacks/) come along too.
                let to = target.join(&path);
                tokio::fs::create_dir_all(&to).await.map_err(io_err(&to))?;
                dirs.push(path);
            } else if kind.is_file() {
                let size = entry.metadata().await.map(|m| m.len()).unwrap_or(0);
                files.push((path, size));
            } else {
                tracing::warn!(
                    "not importing {}: symlink or special file",
                    entry.path().display()
                );
            }
        }
    }

    progress.reset();
    progress
        .total_files
        .store(files.len() as u64, Ordering::Relaxed);
    progress
        .total_bytes
        .store(files.iter().map(|(_, size)| size).sum(), Ordering::Relaxed);
    for (relative, size) in files {
        let to = target.join(&relative);
        if let Some(parent) = to.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(io_err(parent))?;
        }
        let from = source.join(&relative);
        tokio::fs::copy(&from, &to).await.map_err(io_err(&from))?;
        progress.done_files.fetch_add(1, Ordering::Relaxed);
        progress.done_bytes.fetch_add(size, Ordering::Relaxed);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[allow(clippy::type_complexity)]
    fn pack(json: &str) -> Result<(Option<String>, Option<Loader>, Option<String>), String> {
        serde_json::from_str::<MmcPack>(json).unwrap().target(false)
    }

    #[test]
    fn reads_components() {
        let fabric = pack(
            r#"{"formatVersion":1,"components":[
                {"uid":"org.lwjgl3","version":"3.4.3"},
                {"uid":"net.minecraft","version":"26.3"},
                {"uid":"net.fabricmc.intermediary","version":"26.3"},
                {"uid":"net.fabricmc.fabric-loader","version":"0.19.5"}]}"#,
        );
        assert_eq!(
            fabric,
            Ok((
                Some("26.3".into()),
                Some(Loader::Fabric),
                Some("0.19.5".into())
            ))
        );
        let vanilla = pack(r#"{"components":[{"uid":"net.minecraft","version":"1.8.9"}]}"#);
        assert_eq!(
            vanilla,
            Ok((Some("1.8.9".into()), Some(Loader::Vanilla), None))
        );
    }

    #[test]
    fn refuses_what_onyx_cant_run() {
        let old_forge = pack(
            r#"{"components":[{"uid":"net.minecraft","version":"1.12.2"},
                {"uid":"net.minecraftforge","version":"14.23.5.2860"}]}"#,
        );
        assert!(old_forge.unwrap_err().contains("1.13"));
        let lite = pack(
            r#"{"components":[{"uid":"net.minecraft","version":"1.12.2"},
                {"uid":"com.mumfrey.liteloader","version":"1.12.2","cachedName":"LiteLoader"}]}"#,
        );
        assert!(lite.unwrap_err().contains("LiteLoader"));
        assert!(pack(r#"{"components":[]}"#).is_err());
    }

    #[test]
    fn component_instances_allow_what_native_ones_cant() {
        let gtnh: MmcPack = serde_json::from_str(
            r#"{"components":[{"uid":"org.lwjgl3","version":"3.4.3"},
                {"uid":"net.minecraft","version":"1.7.10"},
                {"uid":"me.eigenraven.lwjgl3ify.forgepatches"},
                {"uid":"net.minecraftforge","version":"10.13.4.1614"}]}"#,
        )
        .unwrap();
        assert!(gtnh.target(false).is_err());
        assert_eq!(
            gtnh.target(true),
            Ok((
                Some("1.7.10".into()),
                Some(Loader::Forge),
                Some("10.13.4.1614".into())
            ))
        );
    }

    #[test]
    fn version_cutoff() {
        assert!(before_1_13("1.12.2"));
        assert!(before_1_13("1.7.10"));
        assert!(!before_1_13("1.13"));
        assert!(!before_1_13("1.20.1"));
        assert!(!before_1_13("26.3"));
        assert!(!before_1_13("24w10a"));
    }

    #[test]
    fn parses_general_section() {
        let ini = "[General]\nname=\"Create: New World, \\\"pack\\\"\"\niconKey=quiltmc\n\
                   OverrideMemory=true\n\n[UI]\nname=wrong\niconKey=default\n";
        let values = parse_ini(ini);
        assert_eq!(values["name"], "Create: New World, \"pack\"");
        assert_eq!(values["iconKey"], "quiltmc");
        // Old MultiMC files have no sections.
        assert_eq!(parse_ini("name=Old\n")["name"], "Old");
        assert_eq!(unquote("caf\\xe9"), "café");
    }

    #[tokio::test]
    async fn imports_an_instance() {
        let prism = tempfile::tempdir().unwrap();
        let onyx = tempfile::tempdir().unwrap();
        let root = prism.path();
        std::fs::write(
            root.join("prismlauncher.cfg"),
            "[General]\nIconsDir=icons\n",
        )
        .unwrap();
        let dir = root.join("instances/My Pack");
        std::fs::create_dir_all(dir.join("minecraft/saves/World")).unwrap();
        std::fs::write(dir.join("minecraft/saves/World/level.dat"), b"level").unwrap();
        std::fs::write(dir.join("minecraft/options.txt"), b"fov:90").unwrap();
        std::fs::write(dir.join("minecraft/instance.json"), b"not ours").unwrap();
        std::fs::create_dir_all(dir.join("minecraft/resourcepacks")).unwrap();
        std::fs::write(
            dir.join("instance.cfg"),
            "[General]\nInstanceType=OneSix\nname=My Pack\niconKey=mine\n\
             OverrideMemory=true\nMaxMemAlloc=6144\ntotalTimePlayed=120\n\
             lastLaunchTime=1792333751904\nManagedPack=true\nManagedPackType=modrinth\n\
             ManagedPackID=abc\nManagedPackVersionID=def\nManagedPackVersionName=1.0\n",
        )
        .unwrap();
        std::fs::write(
            dir.join("mmc-pack.json"),
            r#"{"components":[{"uid":"net.minecraft","version":"1.21.1"},
                {"uid":"net.neoforged","version":"21.1.259"}]}"#,
        )
        .unwrap();
        std::fs::create_dir_all(root.join("icons")).unwrap();
        // Smallest valid-looking PNG header is enough; set_icon only checks the extension.
        std::fs::write(root.join("icons/mine.png"), b"\x89PNG\r\n\x1a\n").unwrap();

        let found = list(root).unwrap();
        assert_eq!(found.len(), 1);
        let prism_instance = &found[0];
        assert_eq!(prism_instance.unsupported, None);
        assert_eq!(prism_instance.pack_platform.as_deref(), Some("Modrinth"));

        let created = create(root, onyx.path(), prism_instance).unwrap();
        assert_eq!(created.loader, Loader::NeoForge);
        assert_eq!(created.loader_version.as_deref(), Some("21.1.259"));
        assert_eq!(created.max_memory_mb, Some(6144));
        assert_eq!(created.played_secs, 120);
        assert_eq!(created.last_played, Some(1792333751));
        assert_eq!(created.pack.as_ref().unwrap().version_id, "def");
        assert!(created.icon.is_some());

        let target = onyx.path().join(&created.id);
        let progress = Progress::default();
        copy_game_folder(root, prism_instance, &target, &progress)
            .await
            .unwrap();
        assert_eq!(
            std::fs::read(target.join("saves/World/level.dat")).unwrap(),
            b"level"
        );
        assert_eq!(
            std::fs::read(target.join("options.txt")).unwrap(),
            b"fov:90"
        );
        // Our instance.json wasn't replaced by the one in the game folder.
        assert!(instance::load(onyx.path(), &created.id).is_ok());
        assert_eq!(progress.snapshot().done_files, 2);
    }
}
