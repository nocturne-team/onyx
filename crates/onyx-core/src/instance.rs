//! Instances: one folder per instance under `<data dir>/instances/`, holding an
//! `instance.json` and the game directory contents (saves, mods, options...).

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::auth::now_secs;

pub const INSTANCE_FILE: &str = "instance.json";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Loader {
    Vanilla,
    Fabric,
    Quilt,
    Forge,
    NeoForge,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Instance {
    /// Folder name, unique among instances. Not shown to the user.
    #[serde(skip_deserializing)]
    pub id: String,
    pub name: String,
    pub game_version: String,
    pub loader: Loader,
    #[serde(default)]
    pub loader_version: Option<String>,
    /// Unix seconds.
    pub created: u64,
    #[serde(default)]
    pub last_played: Option<u64>,
    /// Total seconds the game has been running.
    #[serde(default)]
    pub played_secs: u64,
    #[serde(default)]
    pub max_memory_mb: Option<u32>,
    /// File name of a custom icon inside the instance folder (`icon-<time>.<ext>`;
    /// the name changes on every upload so the UI never shows a stale one).
    #[serde(default)]
    pub icon: Option<String>,
    /// Background + symbol for the built-in icons, used when there's no
    /// custom image. `None` means "pick one from the id".
    #[serde(default)]
    pub icon_config: Option<IconConfig>,
    /// The Modrinth modpack version this instance was installed from, if known.
    #[serde(default)]
    pub pack: Option<PackSource>,
    /// A specific Java binary for this instance, over the Settings choice.
    #[serde(default)]
    pub java_path: Option<PathBuf>,
    /// Launches from a copy of a Prism Launcher profile (`mmc-pack.json` and
    /// component patches in `.onyx/prism/`, see `prism::profile`) instead of
    /// Onyx's own loader setup. `loader` and `loader_version` are then only
    /// for display and content filtering.
    #[serde(default)]
    pub prism_components: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PackSource {
    pub project_id: String,
    pub version_id: String,
    pub version_number: String,
}

/// A built-in icon: ids from the UI's icon catalog (`ui/src/lib/iconCatalog.ts`).
/// The backend only stores them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IconConfig {
    pub background: String,
    pub symbol: String,
}

impl IconConfig {
    fn is_reasonable(&self) -> bool {
        let ok = |s: &str| !s.is_empty() && s.len() <= 64;
        ok(&self.background) && ok(&self.symbol)
    }
}

/// Image types accepted as instance icons.
pub const ICON_EXTENSIONS: &[&str] = &["png", "jpg", "jpeg", "webp", "gif"];
const MAX_ICON_BYTES: usize = 4 * 1024 * 1024;

#[derive(Debug, thiserror::Error)]
pub enum InstanceError {
    #[error("{}: {source}", path.display())]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("{} is not a valid instance file: {source}", path.display())]
    Invalid {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },
    #[error("give the instance a name")]
    EmptyName,
    #[error("no instance called {0}")]
    NotFound(String),
    #[error("icons must be PNG, JPEG, WebP or GIF images under 4 MB")]
    BadIcon,
}

fn io_err(path: &Path) -> impl FnOnce(std::io::Error) -> InstanceError + '_ {
    move |source| InstanceError::Io {
        path: path.to_owned(),
        source,
    }
}

/// All instances, most recently played (or created) first. Folders without a
/// readable `instance.json` are skipped with a warning.
pub fn list(root: &Path) -> Result<Vec<Instance>, InstanceError> {
    let entries = match std::fs::read_dir(root) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(io_err(root)(e)),
    };
    let mut instances: Vec<Instance> = entries
        .filter_map(Result::ok)
        .filter(|entry| entry.path().join(INSTANCE_FILE).is_file())
        .filter_map(|entry| {
            let id = entry.file_name().to_string_lossy().into_owned();
            load(root, &id)
                .inspect_err(|e| tracing::warn!("skipping instance {id}: {e}"))
                .ok()
        })
        .collect();
    instances.sort_by_key(|i| std::cmp::Reverse(i.last_played.unwrap_or(i.created)));
    Ok(instances)
}

pub fn load(root: &Path, id: &str) -> Result<Instance, InstanceError> {
    let path = root.join(id).join(INSTANCE_FILE);
    let bytes = std::fs::read(&path).map_err(|e| match e.kind() {
        std::io::ErrorKind::NotFound => InstanceError::NotFound(id.to_owned()),
        _ => io_err(&path)(e),
    })?;
    let mut instance: Instance =
        serde_json::from_slice(&bytes).map_err(|source| InstanceError::Invalid {
            path: path.clone(),
            source,
        })?;
    instance.id = id.to_owned();
    Ok(instance)
}

pub fn save(root: &Path, instance: &Instance) -> Result<(), InstanceError> {
    let dir = root.join(&instance.id);
    std::fs::create_dir_all(&dir).map_err(io_err(&dir))?;
    let path = dir.join(INSTANCE_FILE);
    let temp = dir.join(format!("{INSTANCE_FILE}.onyx-part"));
    let json = serde_json::to_vec_pretty(instance).expect("instance always serializes");
    std::fs::write(&temp, json).map_err(io_err(&temp))?;
    std::fs::rename(&temp, &path).map_err(io_err(&path))
}

pub struct NewInstance {
    pub name: String,
    pub game_version: String,
    pub loader: Loader,
    pub loader_version: Option<String>,
    pub icon_config: Option<IconConfig>,
}

/// Creates the folder and `instance.json`. The game itself is installed on
/// first launch.
pub fn create(root: &Path, new: NewInstance) -> Result<Instance, InstanceError> {
    let name = new.name.trim().to_owned();
    if name.is_empty() {
        return Err(InstanceError::EmptyName);
    }
    let id = unique_id(root, &name);
    let instance = Instance {
        id,
        name,
        game_version: new.game_version,
        loader: new.loader,
        loader_version: new.loader_version,
        created: now_secs(),
        last_played: None,
        played_secs: 0,
        max_memory_mb: None,
        icon: None,
        icon_config: new.icon_config.filter(IconConfig::is_reasonable),
        pack: None,
        java_path: None,
        prism_components: false,
    };
    save(root, &instance)?;
    Ok(instance)
}

/// The custom icon's path, if the instance has one. Ignores names that
/// don't look like ours, in case `instance.json` was edited by hand.
pub fn icon_path(root: &Path, instance: &Instance) -> Option<PathBuf> {
    let name = instance.icon.as_deref()?;
    let valid = name.starts_with("icon-") && !name.contains(['/', '\\']) && !name.contains("..");
    valid.then(|| root.join(&instance.id).join(name))
}

/// Stores `bytes` as the instance's icon, replacing any previous one.
pub fn set_icon(
    root: &Path,
    id: &str,
    bytes: &[u8],
    extension: &str,
) -> Result<Instance, InstanceError> {
    let extension = extension.to_ascii_lowercase();
    if !ICON_EXTENSIONS.contains(&extension.as_str())
        || bytes.is_empty()
        || bytes.len() > MAX_ICON_BYTES
    {
        return Err(InstanceError::BadIcon);
    }
    let mut instance = load(root, id)?;
    let dir = instance_dir(root, id)?;
    let name = format!("icon-{}.{extension}", now_millis());
    let path = dir.join(&name);
    std::fs::write(&path, bytes).map_err(io_err(&path))?;
    let old = icon_path(root, &instance);
    instance.icon = Some(name);
    save(root, &instance)?;
    if let Some(old) = old {
        let _ = std::fs::remove_file(old);
    }
    Ok(instance)
}

/// Drops the custom image and switches to a built-in icon.
pub fn set_icon_config(
    root: &Path,
    id: &str,
    config: IconConfig,
) -> Result<Instance, InstanceError> {
    if !config.is_reasonable() {
        return Err(InstanceError::BadIcon);
    }
    let mut instance = load(root, id)?;
    let old = icon_path(root, &instance);
    instance.icon = None;
    instance.icon_config = Some(config);
    save(root, &instance)?;
    if let Some(old) = old {
        let _ = std::fs::remove_file(old);
    }
    Ok(instance)
}

/// The custom icon as a `data:` URL, small enough to hand to the webview.
pub fn icon_data_url(root: &Path, instance: &Instance) -> Option<String> {
    image_data_url(&icon_path(root, instance)?)
}

/// Any accepted icon image as a `data:` URL, for previews of files the user
/// picked. `None` for other file types or files over the size limit.
pub fn image_data_url(path: &Path) -> Option<String> {
    use base64::Engine;

    let mime = match path.extension()?.to_str()?.to_ascii_lowercase().as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        "gif" => "image/gif",
        _ => return None,
    };
    if std::fs::metadata(path).ok()?.len() > MAX_ICON_BYTES as u64 {
        return None;
    }
    let bytes = std::fs::read(path).ok()?;
    Some(format!(
        "data:{mime};base64,{}",
        base64::engine::general_purpose::STANDARD.encode(bytes)
    ))
}

fn now_millis() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0)
}

pub fn delete(root: &Path, id: &str) -> Result<(), InstanceError> {
    let dir = instance_dir(root, id)?;
    std::fs::remove_dir_all(&dir).map_err(io_err(&dir))
}

/// The instance's folder, which is also its game directory. Rejects ids that
/// would point outside `root`.
pub fn instance_dir(root: &Path, id: &str) -> Result<PathBuf, InstanceError> {
    if id.is_empty() || id != folder_name(id) || id == "." || id == ".." {
        return Err(InstanceError::NotFound(id.to_owned()));
    }
    let dir = root.join(id);
    if !dir.join(INSTANCE_FILE).is_file() {
        return Err(InstanceError::NotFound(id.to_owned()));
    }
    Ok(dir)
}

/// A folder name safe on Windows and Linux.
fn folder_name(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            c if c.is_control() => '_',
            c => c,
        })
        .collect();
    // Windows rejects trailing dots and spaces.
    cleaned.trim_end_matches(['.', ' ']).trim_start().to_owned()
}

fn unique_id(root: &Path, name: &str) -> String {
    let mut base = folder_name(name);
    if base.is_empty() || base.starts_with('.') {
        base = format!("instance{base}");
    }
    const RESERVED: &[&str] = &["CON", "PRN", "AUX", "NUL", "COM1", "LPT1"];
    if RESERVED.contains(&base.to_ascii_uppercase().as_str()) {
        base.push('_');
    }
    let mut id = base.clone();
    let mut n = 2;
    while root.join(&id).exists() {
        id = format!("{base} ({n})");
        n += 1;
    }
    id
}

#[cfg(test)]
mod tests {
    use super::*;

    fn new(name: &str) -> NewInstance {
        NewInstance {
            name: name.into(),
            game_version: "26.3".into(),
            loader: Loader::Vanilla,
            loader_version: None,
            icon_config: None,
        }
    }

    #[test]
    fn create_list_and_delete() {
        let dir = tempfile::tempdir().unwrap();
        let a = create(dir.path(), new("Vanilla 26.3")).unwrap();
        let b = create(dir.path(), new("Vanilla 26.3")).unwrap();
        assert_eq!(a.id, "Vanilla 26.3");
        assert_eq!(b.id, "Vanilla 26.3 (2)");
        assert_eq!(b.name, "Vanilla 26.3");

        let mut listed = list(dir.path()).unwrap();
        assert_eq!(listed.len(), 2);
        listed.sort_by(|x, y| x.id.cmp(&y.id));
        assert_eq!(listed[0], a);

        delete(dir.path(), &a.id).unwrap();
        assert_eq!(list(dir.path()).unwrap(), vec![b]);
    }

    #[test]
    fn unsafe_names_become_safe_folders() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(create(dir.path(), new("a/b:c?")).unwrap().id, "a_b_c_");
        assert_eq!(create(dir.path(), new("..")).unwrap().id, "instance");
        assert_eq!(create(dir.path(), new("con")).unwrap().id, "con_");
        assert!(matches!(
            create(dir.path(), new("  ")),
            Err(InstanceError::EmptyName)
        ));
    }

    #[test]
    fn ids_cannot_escape_the_root() {
        let dir = tempfile::tempdir().unwrap();
        assert!(instance_dir(dir.path(), "../x").is_err());
        assert!(instance_dir(dir.path(), "..").is_err());
        assert!(instance_dir(dir.path(), "").is_err());
    }

    #[test]
    fn icons_replace_each_other_and_reject_junk() {
        let dir = tempfile::tempdir().unwrap();
        let instance = create(dir.path(), new("a")).unwrap();
        let first = set_icon(dir.path(), &instance.id, b"png", "PNG").unwrap();
        let first_path = icon_path(dir.path(), &first).unwrap();
        assert!(first_path.is_file());
        std::thread::sleep(std::time::Duration::from_millis(2));
        let second = set_icon(dir.path(), &instance.id, b"jpg", "jpg").unwrap();
        assert!(!first_path.exists());
        assert!(icon_path(dir.path(), &second).unwrap().is_file());

        assert!(matches!(
            set_icon(dir.path(), &instance.id, b"x", "exe"),
            Err(InstanceError::BadIcon)
        ));
        let config = IconConfig {
            background: "blue".into(),
            symbol: "chest".into(),
        };
        let seeded = set_icon_config(dir.path(), &instance.id, config.clone()).unwrap();
        assert_eq!(
            (seeded.icon.clone(), seeded.icon_config.clone()),
            (None, Some(config))
        );
        assert_eq!(std::fs::read_dir(dir.path().join("a")).unwrap().count(), 1);

        let mut tampered = seeded;
        tampered.icon = Some("../../etc/passwd".into());
        assert_eq!(icon_path(dir.path(), &tampered), None);
    }

    #[test]
    fn played_instances_sort_first() {
        let dir = tempfile::tempdir().unwrap();
        let mut old = create(dir.path(), new("old")).unwrap();
        create(dir.path(), new("new")).unwrap();
        old.last_played = Some(now_secs() + 10);
        save(dir.path(), &old).unwrap();
        assert_eq!(list(dir.path()).unwrap()[0].id, "old");
    }
}
