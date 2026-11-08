//! Fabric and Quilt. Both publish a "profile" JSON per loader and game version
//! in Mojang's version format, which inherits from the vanilla version: we
//! merge it over the vanilla `VersionInfo` and install and launch as usual.

use std::cmp::Ordering;
use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use super::version::{Argument, Arguments, Artifact, Library, LibraryDownloads, maven_path};
use super::{InstallError, MetaDirs, VersionInfo, get_bytes, get_json, parse, write_file};
use crate::instance::Loader;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MetaLoader {
    Fabric,
    Quilt,
}

impl MetaLoader {
    pub fn from_loader(loader: Loader) -> Option<Self> {
        match loader {
            Loader::Fabric => Some(Self::Fabric),
            Loader::Quilt => Some(Self::Quilt),
            _ => None,
        }
    }

    fn base(self) -> &'static str {
        match self {
            Self::Fabric => "https://meta.fabricmc.net/v2",
            Self::Quilt => "https://meta.quiltmc.org/v3",
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::Fabric => "fabric",
            Self::Quilt => "quilt",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LoaderVersion {
    pub version: String,
    pub stable: bool,
}

#[derive(Deserialize)]
struct GameEntry {
    version: String,
}

#[derive(Deserialize)]
struct LoaderEntry {
    loader: LoaderInfo,
}

#[derive(Deserialize)]
struct LoaderInfo {
    version: String,
    /// Fabric only. Quilt marks pre-releases in the version string instead.
    stable: Option<bool>,
}

/// The loader's profile JSON: the parts that differ from vanilla.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Profile {
    pub main_class: String,
    #[serde(default)]
    pub arguments: Option<ProfileArguments>,
    #[serde(default)]
    pub libraries: Vec<MavenLibrary>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct ProfileArguments {
    #[serde(default)]
    pub game: Vec<Argument>,
    #[serde(default)]
    pub jvm: Vec<Argument>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MavenLibrary {
    pub name: String,
    /// Maven repository root, with a trailing slash.
    pub url: String,
    pub sha1: Option<String>,
    pub size: Option<u64>,
}

/// Game versions the loader supports.
pub async fn game_versions(
    http: &reqwest::Client,
    loader: MetaLoader,
) -> Result<Vec<String>, InstallError> {
    let url = format!("{}/versions/game", loader.base());
    let entries: Vec<GameEntry> = get_json(http, &url, "the loader's game versions").await?;
    Ok(entries.into_iter().map(|e| e.version).collect())
}

/// Loader versions for a game version, newest first.
pub async fn loader_versions(
    http: &reqwest::Client,
    loader: MetaLoader,
    game_version: &str,
) -> Result<Vec<LoaderVersion>, InstallError> {
    let url = format!("{}/versions/loader/{game_version}", loader.base());
    let entries: Vec<LoaderEntry> = get_json(http, &url, "the loader versions").await?;
    let mut versions: Vec<LoaderVersion> = entries
        .into_iter()
        .map(|e| LoaderVersion {
            stable: e.loader.stable.unwrap_or(!e.loader.version.contains('-')),
            version: e.loader.version,
        })
        .collect();
    // Quilt's list isn't sorted.
    versions.sort_by(|a, b| compare_versions(&b.version, &a.version));
    Ok(versions)
}

/// The profile for one loader + game version, downloaded once and then read
/// from disk so installed instances launch offline.
pub async fn profile(
    http: &reqwest::Client,
    meta: &MetaDirs,
    loader: MetaLoader,
    game_version: &str,
    loader_version: &str,
) -> Result<Profile, InstallError> {
    let id = format!("{}-loader-{loader_version}-{game_version}", loader.name());
    let path = meta.version_json(&id);
    let what = format!("the {id} profile");
    if let Ok(bytes) = tokio::fs::read(&path).await
        && let Ok(profile) = parse(&bytes, &what)
    {
        return Ok(profile);
    }
    let url = format!(
        "{}/versions/loader/{game_version}/{loader_version}/profile/json",
        loader.base()
    );
    let bytes = get_bytes(http, &url).await?;
    let profile = parse(&bytes, &what)?;
    write_file(&path, &bytes).await?;
    Ok(profile)
}

/// Vanilla plus the loader: loader libraries first (replacing vanilla ones
/// with the same group and name), loader main class, and both sets of
/// arguments. The id stays vanilla's so the client jar, natives and
/// `--version` keep pointing at the vanilla version.
pub fn merge(vanilla: VersionInfo, profile: Profile) -> VersionInfo {
    let libraries: Vec<Library> = profile
        .libraries
        .into_iter()
        .filter_map(MavenLibrary::into_library)
        .collect();
    merge_parts(vanilla, profile.main_class, profile.arguments, libraries)
}

/// The shared part of merging a loader's version JSON over vanilla; also used
/// for Forge and NeoForge.
pub(crate) fn merge_parts(
    mut vanilla: VersionInfo,
    main_class: String,
    arguments: Option<ProfileArguments>,
    libraries: Vec<Library>,
) -> VersionInfo {
    let replaced: HashSet<String> = libraries.iter().map(|l| library_key(&l.name)).collect();
    vanilla
        .libraries
        .retain(|lib| !replaced.contains(&library_key(&lib.name)));
    vanilla.libraries.splice(0..0, libraries);

    vanilla.main_class = main_class;
    if let Some(extra) = arguments {
        match &mut vanilla.arguments {
            Some(arguments) => {
                arguments.jvm.extend(extra.jvm);
                arguments.game.extend(extra.game);
            }
            None => {
                // Pre-1.13: game arguments stay in `minecraftArguments`, so
                // the loader's extra game arguments are appended there.
                let extra_game: Vec<String> = extra
                    .game
                    .iter()
                    .filter_map(|a| match a {
                        Argument::Plain(s) => Some(s.clone()),
                        _ => None,
                    })
                    .collect();
                if let Some(legacy) = &mut vanilla.minecraft_arguments {
                    for arg in extra_game {
                        legacy.push(' ');
                        legacy.push_str(&arg);
                    }
                }
                // `launch` adds the usual pre-1.13 JVM flags and then these.
                if !extra.jvm.is_empty() {
                    vanilla.arguments = Some(Arguments {
                        game: Vec::new(),
                        jvm: extra.jvm,
                        default_user_jvm: Vec::new(),
                    });
                }
            }
        }
    }
    vanilla
}

/// `group:name` plus classifier, without the version.
pub(crate) fn library_key(coordinate: &str) -> String {
    let parts: Vec<&str> = coordinate.split(':').collect();
    match parts.as_slice() {
        [group, name, _version, classifier, ..] => format!("{group}:{name}:{classifier}"),
        [group, name, ..] => format!("{group}:{name}"),
        _ => coordinate.to_owned(),
    }
}

impl MavenLibrary {
    fn into_library(self) -> Option<Library> {
        let path = maven_path(&self.name)?;
        let base = if self.url.ends_with('/') {
            self.url
        } else {
            format!("{}/", self.url)
        };
        Some(Library {
            downloads: Some(LibraryDownloads {
                artifact: Some(Artifact {
                    url: format!("{base}{path}"),
                    path: Some(path),
                    sha1: self.sha1,
                    size: self.size,
                }),
                classifiers: None,
            }),
            name: self.name,
            natives: None,
            rules: None,
            extract: None,
        })
    }
}

/// Dotted numeric comparison where a pre-release (`-beta.1`) sorts before the
/// release it precedes.
pub(crate) fn compare_versions(a: &str, b: &str) -> Ordering {
    let split = |v: &str| -> (Vec<u64>, Option<String>) {
        let (main, pre) = match v.split_once('-') {
            Some((main, pre)) => (main, Some(pre.to_owned())),
            None => (v, None),
        };
        let parts = main
            .split(['.', '+'])
            .map(|p| p.parse().unwrap_or(0))
            .collect();
        (parts, pre)
    };
    let (a_parts, a_pre) = split(a);
    let (b_parts, b_pre) = split(b);
    a_parts.cmp(&b_parts).then_with(|| match (a_pre, b_pre) {
        (None, None) => Ordering::Equal,
        (None, Some(_)) => Ordering::Greater,
        (Some(_), None) => Ordering::Less,
        (Some(a), Some(b)) => {
            let num = |s: &str| s.rsplit('.').next().and_then(|n| n.parse::<u64>().ok());
            match (num(&a), num(&b)) {
                (Some(x), Some(y))
                    if a.rsplit_once('.').map(|p| p.0) == b.rsplit_once('.').map(|p| p.0) =>
                {
                    x.cmp(&y)
                }
                _ => a.cmp(&b),
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const VANILLA: &str = r#"{
        "id": "1.21.1", "type": "release", "mainClass": "net.minecraft.client.main.Main",
        "arguments": {"game": ["--username", "${auth_player_name}"], "jvm": ["-cp", "${classpath}"]},
        "assetIndex": {"id": "17", "sha1": "a", "size": 1, "url": "u"},
        "assets": "17",
        "downloads": {"client": {"sha1": "c", "size": 3, "url": "u"}},
        "libraries": [
            {"name": "org.ow2.asm:asm:9.3", "downloads": {"artifact": {"path": "old-asm.jar", "sha1": "1", "size": 1, "url": "u"}}},
            {"name": "com.mojang:brigadier:1.3", "downloads": {"artifact": {"path": "b.jar", "sha1": "2", "size": 1, "url": "u"}}}
        ]
    }"#;

    const FABRIC: &str = r#"{
        "id": "fabric-loader-0.16.0-1.21.1", "inheritsFrom": "1.21.1",
        "mainClass": "net.fabricmc.loader.impl.launch.knot.KnotClient",
        "arguments": {"game": [], "jvm": ["-DFabricMcEmu= net.minecraft.client.main.Main "]},
        "libraries": [
            {"name": "org.ow2.asm:asm:9.7", "url": "https://maven.fabricmc.net/", "sha1": "abc", "size": 5},
            {"name": "net.fabricmc:fabric-loader:0.16.0", "url": "https://maven.fabricmc.net"}
        ]
    }"#;

    #[test]
    fn merge_puts_loader_first_and_replaces_duplicates() {
        let vanilla: VersionInfo = serde_json::from_str(VANILLA).unwrap();
        let profile: Profile = serde_json::from_str(FABRIC).unwrap();
        let merged = merge(vanilla, profile);

        assert_eq!(merged.id, "1.21.1");
        assert_eq!(
            merged.main_class,
            "net.fabricmc.loader.impl.launch.knot.KnotClient"
        );
        let names: Vec<&str> = merged.libraries.iter().map(|l| l.name.as_str()).collect();
        assert_eq!(
            names,
            [
                "org.ow2.asm:asm:9.7",
                "net.fabricmc:fabric-loader:0.16.0",
                "com.mojang:brigadier:1.3"
            ]
        );
        let loader = merged.libraries[1].artifact().unwrap();
        assert_eq!(
            loader.url,
            "https://maven.fabricmc.net/net/fabricmc/fabric-loader/0.16.0/fabric-loader-0.16.0.jar"
        );
        assert_eq!(loader.sha1, None);
        assert_eq!(merged.arguments.unwrap().jvm.len(), 3);
    }

    #[test]
    fn versions_sort_newest_first_with_prereleases_below_releases() {
        let mut versions = vec![
            "0.20.0-beta.9",
            "0.24.0",
            "0.20.0-beta.10",
            "0.20.0",
            "0.9.1",
        ];
        versions.sort_by(|a, b| compare_versions(b, a));
        assert_eq!(
            versions,
            [
                "0.24.0",
                "0.20.0",
                "0.20.0-beta.10",
                "0.20.0-beta.9",
                "0.9.1"
            ]
        );
    }

    #[test]
    fn library_keys_ignore_versions() {
        assert_eq!(library_key("org.ow2.asm:asm:9.7"), "org.ow2.asm:asm");
        assert_eq!(
            library_key("org.lwjgl:lwjgl:3.3.3:natives-linux"),
            "org.lwjgl:lwjgl:natives-linux"
        );
    }
}
