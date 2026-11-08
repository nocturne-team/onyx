//! Exported Prism Launcher instances (`.zip`): the instance folder's contents,
//! either at the top of the archive (Prism's own export) or inside one folder
//! (how GT New Horizons ships its Prism/MultiMC download). The icon, if any,
//! is `<iconKey>.<ext>` next to `instance.cfg`.

use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use zip::ZipArchive;

use super::{
    PrismError, PrismInstance, RESERVED, create_instance, io_err, parse_instance, profile,
};
use crate::download::Progress;
use crate::instance::{self, ICON_EXTENSIONS, Instance};

/// An exported instance, read but not imported yet.
#[derive(Debug, Clone)]
pub struct PrismArchive {
    path: PathBuf,
    /// Path of the instance folder inside the zip: `""` or `"Name/"`.
    prefix: String,
    /// `<prefix>minecraft/` or `<prefix>.minecraft/`.
    game_prefix: Option<String>,
    icon_entry: Option<String>,
    pub instance: PrismInstance,
}

fn archive_err(path: &Path) -> impl FnOnce(zip::result::ZipError) -> PrismError + '_ {
    move |e| PrismError::Archive {
        path: path.to_owned(),
        message: e.to_string(),
    }
}

/// Entry names that are valid UTF-8 (the rest can't be Prism files anyway).
fn file_names(zip: &ZipArchive<File>) -> Vec<String> {
    zip.file_names()
        .filter_map(Result::ok)
        .map(|name| name.into_owned())
        .collect()
}

fn open(path: &Path) -> Result<ZipArchive<File>, PrismError> {
    let file = File::open(path).map_err(io_err(path))?;
    ZipArchive::new(file).map_err(archive_err(path))
}

fn read_entry(
    zip: &mut ZipArchive<File>,
    path: &Path,
    name: &str,
) -> Result<Option<Vec<u8>>, PrismError> {
    let mut entry = match zip.by_name(name) {
        Ok(entry) => entry,
        Err(zip::result::ZipError::FileNotFound) => return Ok(None),
        Err(e) => return Err(archive_err(path)(e)),
    };
    let mut bytes = Vec::with_capacity(entry.size().min(64 << 20) as usize);
    entry.read_to_end(&mut bytes).map_err(io_err(path))?;
    Ok(Some(bytes))
}

/// Reads an exported instance. Fails if the zip has no `instance.cfg`; an
/// instance Onyx can't run comes back with `unsupported` set, like in a
/// Prism folder.
pub fn inspect(path: &Path) -> Result<PrismArchive, PrismError> {
    let mut zip = open(path)?;
    let names = file_names(&zip);

    // The shallowest instance.cfg marks the instance folder.
    let prefix = names
        .iter()
        .filter_map(|n| n.strip_suffix("instance.cfg"))
        .filter(|p| p.is_empty() || p.ends_with('/'))
        .min_by_key(|p| p.matches('/').count())
        .map(str::to_owned)
        .ok_or_else(|| PrismError::Archive {
            path: path.to_owned(),
            message: "this isn't an exported Prism Launcher instance (no instance.cfg)".into(),
        })?;
    if prefix.matches('/').count() > 1 {
        return Err(PrismError::Archive {
            path: path.to_owned(),
            message: "instance.cfg is nested too deep to be an exported instance".into(),
        });
    }
    let has = |sub: &str| {
        names
            .iter()
            .any(|n| n.starts_with(&format!("{prefix}{sub}")))
    };
    let game_prefix = ["minecraft/", ".minecraft/"]
        .into_iter()
        .find(|sub| has(sub))
        .map(|sub| format!("{prefix}{sub}"));
    let has_patches = names.iter().any(|n| {
        n.strip_prefix(&format!("{prefix}patches/"))
            .is_some_and(|rest| !rest.contains('/') && rest.ends_with(".json"))
    });

    let cfg = read_entry(&mut zip, path, &format!("{prefix}instance.cfg"))?.unwrap_or_default();
    let pack = read_entry(&mut zip, path, &format!("{prefix}mmc-pack.json"))?;
    let folder = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("Imported")
        .to_owned();
    let instance = parse_instance(
        &folder,
        &String::from_utf8_lossy(&cfg),
        pack.as_deref(),
        game_prefix.is_some(),
        has_patches,
    );
    let icon_entry = instance
        .icon_key
        .as_deref()
        .filter(|key| !key.contains(['/', '\\']) && !key.contains(".."))
        .and_then(|key| {
            ICON_EXTENSIONS
                .iter()
                .map(|ext| format!("{prefix}{key}.{ext}"))
                .find(|name| names.contains(name))
        });
    Ok(PrismArchive {
        path: path.to_owned(),
        prefix,
        game_prefix,
        icon_entry,
        instance,
    })
}

/// Creates the Onyx instance with its icon, settings and (for component
/// instances) Prism profile. The game folder is extracted separately with
/// [`extract_game_folder`].
pub fn create(onyx_root: &Path, archive: &PrismArchive) -> Result<Instance, PrismError> {
    let mut zip = open(&archive.path)?;
    let icon = match &archive.icon_entry {
        Some(name) => read_entry(&mut zip, &archive.path, name)?.and_then(|bytes| {
            let extension = name.rsplit('.').next()?.to_owned();
            Some((bytes, extension))
        }),
        None => None,
    };
    let created = create_instance(onyx_root, &archive.instance, icon)?;
    if archive.instance.settings.prism_components {
        let target = profile::profile_dir(&instance::instance_dir(onyx_root, &created.id)?);
        extract_profile(&mut zip, archive, &target)?;
    }
    Ok(created)
}

/// `mmc-pack.json`, `patches/*.json` and `libraries/*`, flat.
fn extract_profile(
    zip: &mut ZipArchive<File>,
    archive: &PrismArchive,
    target: &Path,
) -> Result<(), PrismError> {
    let prefix = &archive.prefix;
    let wanted: Vec<(String, PathBuf)> = file_names(zip)
        .into_iter()
        .filter_map(|name| {
            let rest = name.strip_prefix(prefix.as_str())?;
            let (sub, file) = rest.split_once('/').unwrap_or(("", rest));
            let ok_name =
                !file.is_empty() && !file.contains(['/', '\\']) && file != "." && file != "..";
            match sub {
                "" if file == "mmc-pack.json" => Some(target.join(file)),
                "patches" if ok_name && file.ends_with(".json") => {
                    Some(target.join(sub).join(file))
                }
                "libraries" if ok_name => Some(target.join(sub).join(file)),
                _ => None,
            }
            .map(|to| (name.clone(), to))
        })
        .collect();
    for dir in [target.join("patches"), target.join("libraries")] {
        std::fs::create_dir_all(&dir).map_err(io_err(&dir))?;
    }
    for (name, to) in wanted {
        let mut entry = zip.by_name(&name).map_err(archive_err(&archive.path))?;
        if !entry.is_file() {
            continue;
        }
        let mut out = File::create(&to).map_err(io_err(&to))?;
        std::io::copy(&mut entry, &mut out).map_err(io_err(&to))?;
    }
    Ok(())
}

/// Extracts the game folder (saves, mods, config, ...) into an Onyx instance
/// folder, reporting files and bytes on `progress`. Blocking; checks `stop`
/// between files. Symlinks and paths that would leave the folder are skipped.
pub fn extract_game_folder(
    archive: &PrismArchive,
    target: &Path,
    progress: &Progress,
    stop: &AtomicBool,
) -> Result<(), PrismError> {
    let Some(game_prefix) = &archive.game_prefix else {
        return Ok(());
    };
    let mut zip = open(&archive.path)?;
    let base = Path::new(game_prefix.trim_end_matches('/'));

    // Index, relative path, size, is dir. Listed first so progress has a total.
    let mut entries = Vec::new();
    for index in 0..zip.len() {
        let entry = zip
            .by_index_raw(index)
            .map_err(archive_err(&archive.path))?;
        let name = String::from_utf8_lossy(entry.name_raw()).into_owned();
        if !name.starts_with(game_prefix.as_str()) {
            continue;
        }
        let Some(relative) = entry
            .enclosed_name()
            .and_then(|p| p.strip_prefix(base).ok().map(Path::to_owned))
        else {
            tracing::warn!("not importing {name}: unsafe path");
            continue;
        };
        let Some(first) = relative.components().next() else {
            continue;
        };
        if RESERVED
            .iter()
            .any(|r| first.as_os_str() == std::ffi::OsStr::new(r))
        {
            continue;
        }
        if entry.is_symlink() {
            tracing::warn!("not importing {name}: symlink");
            continue;
        }
        entries.push((index, relative, entry.size(), entry.is_dir()));
    }

    progress.reset();
    let files = entries.iter().filter(|(.., dir)| !dir);
    progress
        .total_files
        .store(files.clone().count() as u64, Ordering::Relaxed);
    progress
        .total_bytes
        .store(files.map(|(_, _, size, _)| size).sum(), Ordering::Relaxed);

    for (index, relative, size, is_dir) in entries {
        if stop.load(Ordering::Relaxed) {
            return Ok(());
        }
        let to = target.join(&relative);
        if is_dir {
            std::fs::create_dir_all(&to).map_err(io_err(&to))?;
            continue;
        }
        if let Some(parent) = to.parent() {
            std::fs::create_dir_all(parent).map_err(io_err(parent))?;
        }
        let mut entry = zip.by_index(index).map_err(archive_err(&archive.path))?;
        let mut out = File::create(&to).map_err(io_err(&to))?;
        std::io::copy(&mut entry, &mut out).map_err(|e| PrismError::Archive {
            path: archive.path.clone(),
            message: format!("{}: {e}", relative.display()),
        })?;
        progress.done_files.fetch_add(1, Ordering::Relaxed);
        progress.done_bytes.fetch_add(size, Ordering::Relaxed);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use zip::write::SimpleFileOptions;

    use super::*;
    use crate::instance::Loader;

    fn zip_with(path: &Path, files: &[(&str, &[u8])]) {
        let mut zip = zip::ZipWriter::new(File::create(path).unwrap());
        for (name, bytes) in files {
            if name.ends_with('/') {
                zip.add_directory(*name, SimpleFileOptions::default())
                    .unwrap();
            } else {
                zip.start_file(*name, SimpleFileOptions::default()).unwrap();
                zip.write_all(bytes).unwrap();
            }
        }
        zip.finish().unwrap();
    }

    const GTNH_PACK: &str = r#"{"components":[
        {"uid":"org.lwjgl3","version":"3.4.3"},
        {"uid":"net.minecraft","version":"1.7.10"},
        {"uid":"me.eigenraven.lwjgl3ify.forgepatches","cachedName":"LWJGL3ify Early Classpath"},
        {"uid":"net.minecraftforge","version":"10.13.4.1614"}]}"#;

    #[test]
    fn imports_a_gtnh_style_zip() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("GTNH.zip");
        zip_with(
            &path,
            &[
                (
                    "GTNH/instance.cfg",
                    b"InstanceType=OneSix\nname=GTNH 2.9.0\niconKey=gtnh_icon\n",
                ),
                ("GTNH/mmc-pack.json", GTNH_PACK.as_bytes()),
                ("GTNH/patches/net.minecraft.json", b"{}"),
                ("GTNH/patches/nested/evil.json", b"{}"),
                ("GTNH/libraries/lwjgl3ify-forgePatches.jar", b"jar"),
                ("GTNH/gtnh_icon.png", b"\x89PNG\r\n\x1a\n"),
                ("GTNH/.minecraft/", b""),
                ("GTNH/.minecraft/mods/a.jar", b"mod"),
                ("GTNH/.minecraft/config/", b""),
                ("GTNH/.minecraft/instance.json", b"not ours"),
                ("GTNH/.minecraft/../../escape.txt", b"no"),
            ],
        );

        let archive = inspect(&path).unwrap();
        assert_eq!(archive.instance.unsupported, None);
        assert_eq!(archive.instance.name, "GTNH 2.9.0");
        assert_eq!(archive.instance.loader, Some(Loader::Forge));
        assert_eq!(archive.instance.game_version.as_deref(), Some("1.7.10"));

        let onyx = tmp.path().join("onyx");
        let created = create(&onyx, &archive).unwrap();
        assert!(created.prism_components);
        assert!(created.icon.is_some());
        let dir = onyx.join(&created.id);
        let prism = profile::profile_dir(&dir);
        assert!(prism.join("mmc-pack.json").is_file());
        assert!(prism.join("patches/net.minecraft.json").is_file());
        assert!(!prism.join("patches/evil.json").exists());
        assert!(prism.join("libraries/lwjgl3ify-forgePatches.jar").is_file());

        let progress = Progress::default();
        extract_game_folder(&archive, &dir, &progress, &AtomicBool::new(false)).unwrap();
        assert_eq!(std::fs::read(dir.join("mods/a.jar")).unwrap(), b"mod");
        assert!(dir.join("config").is_dir());
        assert!(!tmp.path().join("escape.txt").exists());
        assert!(instance::load(&onyx, &created.id).unwrap().prism_components);
        assert_eq!(progress.snapshot().done_files, 1);
    }

    #[test]
    fn reads_a_flat_prism_export_and_rejects_other_zips() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("Fabric.zip");
        zip_with(
            &path,
            &[
                ("instance.cfg", b"[General]\nname=Fabric\n"),
                (
                    "mmc-pack.json",
                    br#"{"components":[{"uid":"net.minecraft","version":"26.3"},
                        {"uid":"net.fabricmc.fabric-loader","version":"0.19.5"}]}"#,
                ),
                ("minecraft/options.txt", b"fov:90"),
            ],
        );
        let archive = inspect(&path).unwrap();
        assert_eq!(archive.instance.unsupported, None);
        assert_eq!(archive.instance.loader, Some(Loader::Fabric));
        assert!(!archive.instance.settings.prism_components);
        assert_eq!(archive.game_prefix.as_deref(), Some("minecraft/"));

        let other = tmp.path().join("other.zip");
        zip_with(&other, &[("readme.txt", b"hi")]);
        assert!(
            inspect(&other)
                .unwrap_err()
                .to_string()
                .contains("instance.cfg")
        );
    }
}
