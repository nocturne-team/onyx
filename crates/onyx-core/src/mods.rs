//! The `mods/` folder of an instance: listing jars with their metadata,
//! enabling/disabling (`.disabled` suffix, as other launchers do) and removing.

use std::io::Read;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

const DISABLED: &str = ".disabled";

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModFile {
    /// File name as on disk, including any `.disabled` suffix.
    pub file_name: String,
    pub enabled: bool,
    pub size: u64,
    /// From `fabric.mod.json` / `quilt.mod.json`, when the jar has one.
    pub mod_id: Option<String>,
    pub name: Option<String>,
    pub version: Option<String>,
    pub description: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum ModsError {
    #[error("{}: {source}", path.display())]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("{0} is not a mod file in this instance")]
    BadName(String),
}

fn io_err(path: &Path) -> impl FnOnce(std::io::Error) -> ModsError + '_ {
    move |source| ModsError::Io {
        path: path.to_owned(),
        source,
    }
}

/// Every `.jar` (enabled or not) in `<game dir>/mods`, sorted by display name.
pub fn list(game_dir: &Path) -> Result<Vec<ModFile>, ModsError> {
    let dir = game_dir.join("mods");
    let entries = match std::fs::read_dir(&dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(io_err(&dir)(e)),
    };
    let mut mods: Vec<ModFile> = entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let file_name = entry.file_name().to_string_lossy().into_owned();
            let enabled = !file_name.ends_with(DISABLED);
            let base = file_name.strip_suffix(DISABLED).unwrap_or(&file_name);
            if !base.ends_with(".jar") {
                return None;
            }
            let meta = entry.metadata().ok().filter(|m| m.is_file())?;
            let info = read_metadata(&entry.path()).unwrap_or_default();
            Some(ModFile {
                file_name,
                enabled,
                size: meta.len(),
                mod_id: info.id,
                name: info.name,
                version: info.version,
                description: info.description,
            })
        })
        .collect();
    mods.sort_by_cached_key(|m| {
        m.name
            .clone()
            .unwrap_or_else(|| m.file_name.clone())
            .to_lowercase()
    });
    Ok(mods)
}

/// Renames to or from `<file>.disabled`. Returns the new file name.
pub fn set_enabled(game_dir: &Path, file_name: &str, enabled: bool) -> Result<String, ModsError> {
    let from = mod_path(game_dir, file_name)?;
    let base = file_name.strip_suffix(DISABLED).unwrap_or(file_name);
    let new_name = if enabled {
        base.to_owned()
    } else {
        format!("{base}{DISABLED}")
    };
    if new_name != file_name {
        let to = game_dir.join("mods").join(&new_name);
        std::fs::rename(&from, &to).map_err(io_err(&to))?;
    }
    Ok(new_name)
}

pub fn remove(game_dir: &Path, file_name: &str) -> Result<(), ModsError> {
    let path = mod_path(game_dir, file_name)?;
    std::fs::remove_file(&path).map_err(io_err(&path))
}

fn mod_path(game_dir: &Path, file_name: &str) -> Result<PathBuf, ModsError> {
    let bad = || ModsError::BadName(file_name.to_owned());
    if file_name.is_empty()
        || file_name.contains(['/', '\\'])
        || file_name.starts_with('.')
        || !file_name
            .strip_suffix(DISABLED)
            .unwrap_or(file_name)
            .ends_with(".jar")
    {
        return Err(bad());
    }
    let path = game_dir.join("mods").join(file_name);
    if !path.is_file() {
        return Err(bad());
    }
    Ok(path)
}

#[derive(Debug, Default)]
struct Metadata {
    id: Option<String>,
    name: Option<String>,
    version: Option<String>,
    description: Option<String>,
}

#[derive(Deserialize)]
struct FabricModJson {
    id: Option<String>,
    name: Option<String>,
    version: Option<String>,
    description: Option<String>,
}

#[derive(Deserialize)]
struct QuiltModJson {
    quilt_loader: QuiltLoader,
}

#[derive(Deserialize)]
struct QuiltLoader {
    id: Option<String>,
    version: Option<String>,
    metadata: Option<QuiltMetadata>,
}

#[derive(Deserialize)]
struct QuiltMetadata {
    name: Option<String>,
    description: Option<String>,
}

fn read_metadata(jar: &Path) -> Option<Metadata> {
    let file = std::fs::File::open(jar).ok()?;
    let mut archive = zip::ZipArchive::new(file).ok()?;
    let mut read = |name: &str| -> Option<String> {
        let mut entry = archive.by_name(name).ok()?;
        let mut text = String::new();
        entry.read_to_string(&mut text).ok()?;
        Some(text)
    };
    if let Some(text) = read("quilt.mod.json") {
        if let Ok(quilt) = serde_json::from_str::<QuiltModJson>(&text) {
            let meta = quilt.quilt_loader.metadata;
            return Some(Metadata {
                id: quilt.quilt_loader.id,
                version: quilt.quilt_loader.version,
                name: meta.as_ref().and_then(|m| m.name.clone()),
                description: meta.and_then(|m| m.description),
            });
        }
    }
    for toml in ["META-INF/neoforge.mods.toml", "META-INF/mods.toml"] {
        if let Some(text) = read(toml) {
            return Some(forge_metadata(&text));
        }
    }
    // Some mods put raw control characters in descriptions; lenient parsing
    // isn't worth a dependency, so those just show their file name.
    let fabric: FabricModJson = serde_json::from_str(&read("fabric.mod.json")?).ok()?;
    Some(Metadata {
        id: fabric.id,
        name: fabric.name,
        version: fabric.version,
        description: fabric.description,
    })
}

/// The first `[[mods]]` entry of a Forge/NeoForge `mods.toml`. Only simple
/// `key = "value"` lines are read, which covers what the list shows; a TOML
/// parser isn't worth it here.
fn forge_metadata(text: &str) -> Metadata {
    let mut meta = Metadata::default();
    let mut in_mods = false;
    for line in text.lines().map(str::trim) {
        if line.starts_with("[[") || line.starts_with('[') {
            if in_mods && meta.id.is_some() {
                break;
            }
            in_mods = line == "[[mods]]";
            continue;
        }
        if !in_mods {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let value = value.trim();
        let Some(value) = value
            .strip_prefix('"')
            .and_then(|v| v.split_once('"'))
            .map(|(v, _)| v.to_owned())
        else {
            continue;
        };
        match key.trim() {
            "modId" => meta.id = Some(value),
            "displayName" => meta.name = Some(value),
            // Often a "${file.jarVersion}" placeholder, which means nothing to users.
            "version" if !value.starts_with("${") => meta.version = Some(value),
            _ => {}
        }
    }
    meta
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use super::*;

    fn jar(dir: &Path, name: &str, entry: Option<(&str, &str)>) {
        let mut zip = zip::ZipWriter::new(std::fs::File::create(dir.join(name)).unwrap());
        if let Some((file, contents)) = entry {
            zip.start_file(file, zip::write::SimpleFileOptions::default())
                .unwrap();
            zip.write_all(contents.as_bytes()).unwrap();
        }
        zip.finish().unwrap();
    }

    #[test]
    fn lists_toggles_and_removes() {
        let game = tempfile::tempdir().unwrap();
        let mods = game.path().join("mods");
        std::fs::create_dir(&mods).unwrap();
        jar(
            &mods,
            "sodium.jar",
            Some((
                "fabric.mod.json",
                r#"{"id":"sodium","name":"Sodium","version":"0.6.0","description":"Fast"}"#,
            )),
        );
        jar(
            &mods,
            "qsl.jar.disabled",
            Some((
                "quilt.mod.json",
                r#"{"quilt_loader":{"id":"qsl","version":"1.0","metadata":{"name":"QSL"}}}"#,
            )),
        );
        jar(&mods, "plain.jar", None);
        std::fs::write(mods.join("notes.txt"), "x").unwrap();

        let listed = list(game.path()).unwrap();
        let names: Vec<_> = listed.iter().map(|m| m.file_name.as_str()).collect();
        assert_eq!(names, ["plain.jar", "qsl.jar.disabled", "sodium.jar"]);
        assert_eq!(listed[1].name.as_deref(), Some("QSL"));
        assert!(!listed[1].enabled);
        assert_eq!(listed[2].version.as_deref(), Some("0.6.0"));

        assert_eq!(
            set_enabled(game.path(), "qsl.jar.disabled", true).unwrap(),
            "qsl.jar"
        );
        assert_eq!(
            set_enabled(game.path(), "sodium.jar", false).unwrap(),
            "sodium.jar.disabled"
        );
        remove(game.path(), "plain.jar").unwrap();
        let names: Vec<_> = list(game.path())
            .unwrap()
            .into_iter()
            .map(|m| m.file_name)
            .collect();
        assert_eq!(names, ["qsl.jar", "sodium.jar.disabled"]);
    }

    #[test]
    fn forge_mods_toml_is_read() {
        let meta = forge_metadata(
            r#"modLoader="javafml"
loaderVersion="[47,)"
[[mods]]
modId="jei"
version="${file.jarVersion}"
displayName="Just Enough Items" # comment
[[dependencies.jei]]
modId="forge"
"#,
        );
        assert_eq!(meta.id.as_deref(), Some("jei"));
        assert_eq!(meta.name.as_deref(), Some("Just Enough Items"));
        assert_eq!(meta.version, None);
    }

    #[test]
    fn names_cannot_escape_the_mods_folder() {
        let game = tempfile::tempdir().unwrap();
        assert!(remove(game.path(), "../instance.json").is_err());
        assert!(remove(game.path(), "..\\x.jar").is_err());
        assert!(set_enabled(game.path(), "missing.jar", false).is_err());
    }
}
