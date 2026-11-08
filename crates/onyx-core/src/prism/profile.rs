//! Launching from Prism's own component list, for instances that ship their
//! own component patches (`patches/<uid>.json`), like GT New Horizons. Onyx's
//! native loaders can't build those: GTNH swaps LWJGL 2 for LWJGL 3 and starts
//! Forge 1.7.10 through RetroFuturaBootstrap.
//!
//! The format is Prism's "OneSix" version file (see Prism's
//! `launcher/minecraft/OneSixVersionFormat.cpp`). Components are applied in
//! `mmc-pack.json` order: `mainClass`, `minecraftArguments`, `mainJar` and
//! `assetIndex` replace earlier values, `+tweakers` and `+jvmArgs` append, and
//! a library replaces an earlier one with the same group, name and classifier
//! in place. Components without a patch come from Prism's meta server.
//!
//! The result is turned into a Mojang-style [`VersionInfo`] so install and
//! launch work as for any other version.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::Deserialize;
use serde_json::{Value, json};

use super::PrismError;
use crate::minecraft::loader::library_key;
use crate::minecraft::version::{VersionInfo, maven_path};
use crate::minecraft::{InstallError, MetaDirs, get_bytes};

/// Where an instance's copy of the Prism profile lives, inside its folder:
/// `mmc-pack.json`, `patches/` and `libraries/` (jars marked `local`).
pub const PROFILE_DIR: &str = ".onyx/prism";

const META_URL: &str = "https://meta.prismlauncher.org/v1";
const DEFAULT_MAVEN: &str = "https://libraries.minecraft.net/";

pub fn profile_dir(instance_dir: &Path) -> PathBuf {
    instance_dir.join(PROFILE_DIR)
}

#[derive(Debug, Deserialize)]
pub(super) struct PackFile {
    #[serde(default)]
    pub components: Vec<PackComponent>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct PackComponent {
    pub uid: String,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub cached_name: Option<String>,
    #[serde(default)]
    pub disabled: bool,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Patch {
    #[serde(default)]
    name: Option<String>,
    main_class: Option<String>,
    minecraft_arguments: Option<String>,
    #[serde(default, rename = "+tweakers")]
    tweakers: Vec<String>,
    #[serde(default, rename = "+jvmArgs")]
    jvm_args: Vec<String>,
    #[serde(default)]
    libraries: Vec<PrismLibrary>,
    #[serde(default, rename = "+libraries")]
    plus_libraries: Vec<PrismLibrary>,
    main_jar: Option<PrismLibrary>,
    asset_index: Option<Value>,
    #[serde(rename = "type")]
    kind: Option<String>,
    compatible_java_majors: Option<Vec<u32>>,
    compatible_java_name: Option<String>,
    #[serde(default)]
    jar_mods: Vec<Value>,
    #[serde(default)]
    maven_files: Vec<Value>,
    #[serde(default, rename = "+agents")]
    agents: Vec<Value>,
}

#[derive(Debug, Clone, Deserialize)]
struct PrismLibrary {
    name: String,
    #[serde(default)]
    downloads: Option<PrismDownloads>,
    /// Maven repository base, used when there's no `downloads`.
    url: Option<String>,
    /// `local`: the jar is in the instance's `libraries/` folder.
    #[serde(rename = "MMC-hint")]
    hint: Option<String>,
    natives: Option<HashMap<String, String>>,
    rules: Option<Value>,
    extract: Option<Value>,
}

#[derive(Debug, Clone, Deserialize)]
struct PrismDownloads {
    artifact: Option<PrismArtifact>,
    classifiers: Option<HashMap<String, PrismArtifact>>,
}

#[derive(Debug, Clone, Deserialize)]
struct PrismArtifact {
    url: String,
    sha1: Option<String>,
    size: Option<u64>,
}

/// Whether a Prism instance folder (or its copy in an Onyx instance) has
/// component patches, which is what makes Onyx launch it from components.
pub fn has_patches(dir: &Path) -> bool {
    std::fs::read_dir(dir.join("patches")).is_ok_and(|entries| {
        entries
            .filter_map(Result::ok)
            .any(|e| e.path().extension().is_some_and(|ext| ext == "json"))
    })
}

/// Builds the launch profile from the Prism files in `dir` (an instance's
/// [`PROFILE_DIR`]). Local libraries are copied into the shared libraries
/// folder so install and launch find them like any other.
pub async fn resolve(
    http: &reqwest::Client,
    meta: &MetaDirs,
    dir: &Path,
) -> Result<VersionInfo, PrismError> {
    let pack_path = dir.join("mmc-pack.json");
    let bytes = tokio::fs::read(&pack_path)
        .await
        .map_err(super::io_err(&pack_path))?;
    let pack: PackFile = serde_json::from_slice(&bytes)
        .map_err(|e| PrismError::Profile(format!("mmc-pack.json is unreadable: {e}")))?;

    let mut patches = Vec::new();
    for component in pack.components.iter().filter(|c| !c.disabled) {
        patches.push((component, load_patch(http, meta, dir, component).await?));
    }
    let version = build(&patches, &pack, dir, meta)?;
    serde_json::from_value(version)
        .map_err(|e| PrismError::Profile(format!("the merged profile is incomplete: {e}")))
}

/// `patches/<uid>.json`, else Prism's meta for that component version (cached
/// under `meta/prism/`).
async fn load_patch(
    http: &reqwest::Client,
    meta: &MetaDirs,
    dir: &Path,
    component: &PackComponent,
) -> Result<Patch, PrismError> {
    let uid = &component.uid;
    if uid.is_empty()
        || !uid
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'))
        || uid.starts_with('.')
    {
        return Err(PrismError::Profile(format!("bad component name {uid:?}")));
    }
    let local = dir.join("patches").join(format!("{uid}.json"));
    let bytes = match tokio::fs::read(&local).await {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            let version = component
                .version
                .as_deref()
                .ok_or_else(|| PrismError::Profile(format!("component {uid} has no version")))?;
            if version.contains(['/', '\\']) || version.contains("..") {
                return Err(PrismError::Profile(format!(
                    "bad version {version:?} for {uid}"
                )));
            }
            let cached = meta
                .root
                .join("prism")
                .join(uid)
                .join(format!("{version}.json"));
            match tokio::fs::read(&cached).await {
                Ok(bytes) => bytes,
                Err(_) => {
                    let bytes =
                        get_bytes(http, &format!("{META_URL}/{uid}/{version}.json")).await?;
                    crate::minecraft::write_file(&cached, &bytes).await?;
                    bytes
                }
            }
        }
        Err(e) => return Err(super::io_err(&local)(e)),
    };
    serde_json::from_slice(&bytes).map_err(|e| {
        PrismError::Profile(format!(
            "the {} component is unreadable: {e}",
            component.cached_name.as_deref().unwrap_or(uid)
        ))
    })
}

fn build(
    patches: &[(&PackComponent, Patch)],
    pack: &PackFile,
    dir: &Path,
    meta: &MetaDirs,
) -> Result<Value, PrismError> {
    let mut main_class = None;
    let mut game_args: Option<String> = None;
    let mut main_jar = None;
    let mut asset_index = None;
    let mut kind = None;
    let mut java: Option<(Option<String>, Vec<u32>)> = None;
    let mut tweakers = Vec::new();
    let mut jvm_args = Vec::new();
    let mut libraries: Vec<PrismLibrary> = Vec::new();

    for (component, patch) in patches {
        let what = patch
            .name
            .as_deref()
            .or(component.cached_name.as_deref())
            .unwrap_or(&component.uid);
        let unsupported = |feature: &str| {
            PrismError::Profile(format!(
                "{what} uses {feature}, which Onyx can't launch yet"
            ))
        };
        if !patch.jar_mods.is_empty() {
            return Err(unsupported("jar mods"));
        }
        if !patch.maven_files.is_empty() {
            return Err(unsupported("installer files (mavenFiles)"));
        }
        if !patch.agents.is_empty() {
            return Err(unsupported("Java agents"));
        }
        if let Some(value) = &patch.main_class {
            main_class = Some(value.clone());
        }
        if let Some(value) = &patch.minecraft_arguments {
            game_args = Some(value.clone());
        }
        if let Some(value) = &patch.main_jar {
            main_jar = Some(value.clone());
        }
        if let Some(value) = &patch.asset_index {
            asset_index = Some(value.clone());
        }
        if component.uid == "net.minecraft"
            && let Some(value) = &patch.kind
        {
            kind = Some(value.clone());
        }
        if patch.compatible_java_name.is_some() || patch.compatible_java_majors.is_some() {
            java = Some((
                patch.compatible_java_name.clone(),
                patch.compatible_java_majors.clone().unwrap_or_default(),
            ));
        }
        tweakers.extend(patch.tweakers.iter().cloned());
        jvm_args.extend(patch.jvm_args.iter().cloned());
        for library in patch.libraries.iter().chain(&patch.plus_libraries) {
            let key = library_key(&library.name);
            match libraries.iter().position(|l| library_key(&l.name) == key) {
                Some(index) => libraries[index] = library.clone(),
                None => libraries.push(library.clone()),
            }
        }
    }

    let missing = |what: &str| PrismError::Profile(format!("no component sets {what}"));
    let game_version = pack
        .components
        .iter()
        .find(|c| c.uid == "net.minecraft")
        .and_then(|c| c.version.clone())
        .ok_or_else(|| missing("the Minecraft version"))?;
    let main_class = main_class.ok_or_else(|| missing("a main class"))?;
    let mut game_args = game_args.ok_or_else(|| missing("game arguments"))?;
    for tweaker in tweakers {
        game_args.push_str(" --tweakClass ");
        game_args.push_str(&tweaker);
    }
    let main_jar = main_jar.ok_or_else(|| missing("the game jar"))?;
    let client = main_jar
        .downloads
        .as_ref()
        .and_then(|d| d.artifact.as_ref())
        .ok_or_else(|| missing("a download for the game jar"))?;
    let asset_index = asset_index.ok_or_else(|| missing("an asset index"))?;
    let assets = asset_index
        .get("id")
        .and_then(Value::as_str)
        .ok_or_else(|| missing("an asset index id"))?
        .to_owned();

    let libraries = libraries
        .into_iter()
        .map(|library| convert_library(library, dir, meta))
        .collect::<Result<Vec<_>, _>>()?;

    let mut version = json!({
        "id": game_version,
        "type": kind.unwrap_or_else(|| "release".to_owned()),
        "mainClass": main_class,
        "minecraftArguments": game_args,
        "arguments": { "game": [], "jvm": jvm_args },
        "assetIndex": asset_index,
        "assets": assets,
        "downloads": { "client": {
            "url": client.url,
            "sha1": client.sha1,
            "size": client.size,
        }},
        "libraries": libraries,
    });
    if let Some((component, major)) = java.and_then(|(name, majors)| java_runtime(name, &majors)) {
        version["javaVersion"] = json!({ "component": component, "majorVersion": major });
    }
    Ok(version)
}

/// Mojang runtime component and Java major for Prism's
/// `compatibleJavaName`/`compatibleJavaMajors`.
fn java_runtime(name: Option<String>, majors: &[u32]) -> Option<(String, u32)> {
    const RUNTIMES: &[(&str, u32)] = &[
        ("jre-legacy", 8),
        ("java-runtime-alpha", 16),
        ("java-runtime-gamma", 17),
        ("java-runtime-delta", 21),
        ("java-runtime-epsilon", 25),
    ];
    if let Some(name) = name {
        let major = RUNTIMES
            .iter()
            .find(|(n, _)| *n == name)
            .map(|(_, m)| *m)
            .or_else(|| majors.first().copied())?;
        return Some((name, major));
    }
    majors.iter().find_map(|major| {
        RUNTIMES
            .iter()
            .find(|(_, m)| m == major)
            .map(|(n, m)| ((*n).to_owned(), *m))
    })
}

/// A Prism library in Mojang's form: every artifact gets its Maven `path`,
/// libraries without `downloads` get a URL from `url` (or Mojang's maven),
/// and `local` ones are copied out of the instance's `libraries/` folder.
fn convert_library(
    library: PrismLibrary,
    dir: &Path,
    meta: &MetaDirs,
) -> Result<Value, PrismError> {
    let bad = || PrismError::Profile(format!("bad library name {:?}", library.name));
    let path = maven_path(&library.name).ok_or_else(bad)?;
    let mut out = json!({ "name": library.name });
    if let Some(rules) = &library.rules {
        out["rules"] = rules.clone();
    }
    if let Some(extract) = &library.extract {
        out["extract"] = extract.clone();
    }
    if let Some(natives) = &library.natives {
        out["natives"] = json!(natives);
    }

    if library.hint.as_deref() == Some("local") {
        let file = path.rsplit('/').next().ok_or_else(bad)?;
        let source = dir.join("libraries").join(file);
        let target = meta.libraries().join(&path);
        let same = |a: &Path, b: &Path| {
            matches!((std::fs::metadata(a), std::fs::metadata(b)),
                (Ok(a), Ok(b)) if a.len() == b.len())
        };
        if !same(&source, &target) {
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent).map_err(super::io_err(parent))?;
            }
            std::fs::copy(&source, &target).map_err(super::io_err(&source))?;
        }
        out["downloads"] = json!({ "artifact": { "path": path, "url": "" } });
        return Ok(out);
    }

    let artifact = |a: &PrismArtifact, path: &str| json!({ "path": path, "url": a.url, "sha1": a.sha1, "size": a.size });
    let mut downloads = json!({});
    match &library.downloads {
        Some(d) => {
            if let Some(a) = &d.artifact {
                downloads["artifact"] = artifact(a, &path);
            }
            if let Some(classifiers) = &d.classifiers {
                let mut map = serde_json::Map::new();
                for (classifier, a) in classifiers {
                    let path =
                        maven_path(&format!("{}:{classifier}", library.name)).ok_or_else(bad)?;
                    map.insert(classifier.clone(), artifact(a, &path));
                }
                downloads["classifiers"] = Value::Object(map);
            }
        }
        // Old-style natives with no downloads aren't worth guessing at.
        None if library.natives.is_none() => {
            let base = library.url.as_deref().unwrap_or(DEFAULT_MAVEN);
            let base = base.trim_end_matches('/');
            downloads["artifact"] = json!({ "path": path, "url": format!("{base}/{path}") });
        }
        None => {}
    }
    out["downloads"] = downloads;
    Ok(out)
}

/// Errors from building a profile that came from the network.
impl From<InstallError> for PrismError {
    fn from(e: InstallError) -> Self {
        PrismError::Profile(e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::minecraft::version::Environment;

    fn write(dir: &Path, name: &str, text: &str) {
        let path = dir.join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    /// A cut-down GTNH: LWJGL 3, Minecraft with LWJGL 3, a local library,
    /// Forge with a tweaker and a duplicate guava, and a main class override.
    #[tokio::test]
    async fn merges_components_in_order() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("prism");
        let meta = MetaDirs::new(tmp.path().join("meta"));
        write(
            &dir,
            "mmc-pack.json",
            r#"{"components":[
                {"uid":"org.lwjgl3","version":"3.4.3"},
                {"uid":"net.minecraft","version":"1.7.10"},
                {"uid":"early.cp"},
                {"uid":"net.minecraftforge","version":"10.13.4.1614"},
                {"uid":"launch.args"},
                {"uid":"turned.off","disabled":true}]}"#,
        );
        write(
            &dir,
            "patches/org.lwjgl3.json",
            r#"{"libraries":[
                {"name":"org.lwjgl:lwjgl:3.4.3","downloads":{"artifact":{"url":"https://x/lwjgl.jar","sha1":"aa","size":1}}},
                {"name":"org.lwjgl:lwjgl-natives-windows:3.4.3","rules":[{"action":"allow","os":{"name":"windows"}}],
                 "downloads":{"artifact":{"url":"https://x/win.jar","sha1":"bb","size":1}}}]}"#,
        );
        write(
            &dir,
            "patches/net.minecraft.json",
            r#"{"mainClass":"net.minecraft.client.main.Main",
                "minecraftArguments":"--username ${auth_player_name}",
                "assetIndex":{"id":"1.7.10","sha1":"cc","size":1,"totalSize":2,"url":"https://x/1.7.10.json"},
                "mainJar":{"name":"com.mojang:minecraft:1.7.10:client","downloads":{"artifact":{"url":"https://x/client.jar","sha1":"dd","size":5}}},
                "compatibleJavaMajors":[17,21,25],"compatibleJavaName":"java-runtime-epsilon",
                "type":"release",
                "libraries":[
                  {"name":"com.google.guava:guava:15.0","downloads":{"artifact":{"url":"https://x/guava15.jar"}}},
                  {"name":"tv.twitch:twitch-platform:5.16","natives":{"linux":"natives-linux"},
                   "extract":{"exclude":["META-INF/"]},
                   "downloads":{"classifiers":{"natives-linux":{"url":"https://x/tp.jar","sha1":"ee","size":1}}}}]}"#,
        );
        write(
            &dir,
            "patches/early.cp.json",
            r#"{"+jvmArgs":["-Dfoo=bar"],"libraries":[{"name":"com.example:early:1.0:patches","MMC-hint":"local"}]}"#,
        );
        write(&dir, "libraries/early-1.0-patches.jar", "jar");
        write(
            &dir,
            "patches/net.minecraftforge.json",
            r#"{"mainClass":"com.example.Bootstrap","+tweakers":["cpw.mods.fml.common.launcher.FMLTweaker"],
                "libraries":[{"name":"com.google.guava:guava:17.0","downloads":{"artifact":{"url":"https://x/guava17.jar"}}},
                             {"name":"lzma:lzma:0.0.1","url":"https://maven.example/"}]}"#,
        );
        write(
            &dir,
            "patches/launch.args.json",
            r#"{"mainClass":"com.example.FirstThread"}"#,
        );
        write(&dir, "patches/turned.off.json", "not even json");

        let version = resolve(&reqwest::Client::new(), &meta, &dir).await.unwrap();
        assert_eq!(version.id, "1.7.10");
        assert_eq!(version.main_class, "com.example.FirstThread");
        assert_eq!(
            version.minecraft_arguments.as_deref(),
            Some(
                "--username ${auth_player_name} --tweakClass cpw.mods.fml.common.launcher.FMLTweaker"
            )
        );
        assert_eq!(version.assets, "1.7.10");
        assert_eq!(version.downloads.client.url, "https://x/client.jar");
        let java = version.java_version.as_ref().unwrap();
        assert_eq!(
            (java.component.as_str(), java.major_version),
            ("java-runtime-epsilon", 25)
        );

        let names: Vec<&str> = version.libraries.iter().map(|l| l.name.as_str()).collect();
        assert_eq!(
            names,
            [
                "org.lwjgl:lwjgl:3.4.3",
                "org.lwjgl:lwjgl-natives-windows:3.4.3",
                "com.google.guava:guava:17.0",
                "tv.twitch:twitch-platform:5.16",
                "com.example:early:1.0:patches",
                "lzma:lzma:0.0.1",
            ]
        );
        let lzma = version.libraries[5].artifact().unwrap();
        assert_eq!(
            lzma.url,
            "https://maven.example/lzma/lzma/0.0.1/lzma-0.0.1.jar"
        );
        assert_eq!(lzma.path.as_deref(), Some("lzma/lzma/0.0.1/lzma-0.0.1.jar"));

        let env = Environment {
            os: crate::minecraft::version::Os::Linux,
            ..Environment::current()
        };
        assert!(!version.libraries[1].allowed(&env));
        let natives = version.libraries[3].native_artifact(&env).unwrap();
        assert_eq!(
            natives.path.as_deref(),
            Some("tv/twitch/twitch-platform/5.16/twitch-platform-5.16-natives-linux.jar")
        );

        // The local jar was copied into the shared libraries folder.
        let local = meta
            .libraries()
            .join("com/example/early/1.0/early-1.0-patches.jar");
        assert_eq!(std::fs::read(local).unwrap(), b"jar");
        let jvm: Vec<&str> = version
            .arguments
            .as_ref()
            .unwrap()
            .jvm
            .iter()
            .flat_map(|a| a.values(&env))
            .collect();
        assert_eq!(jvm, ["-Dfoo=bar"]);
    }

    #[tokio::test]
    async fn refuses_jar_mods_and_bad_names() {
        let tmp = tempfile::tempdir().unwrap();
        let meta = MetaDirs::new(tmp.path().join("meta"));
        let dir = tmp.path().join("p");
        write(
            &dir,
            "mmc-pack.json",
            r#"{"components":[{"uid":"net.minecraft","version":"1.7.10"}]}"#,
        );
        write(
            &dir,
            "patches/net.minecraft.json",
            r#"{"jarMods":[{"name":"x:y:1"}]}"#,
        );
        let err = resolve(&reqwest::Client::new(), &meta, &dir)
            .await
            .unwrap_err();
        assert!(err.to_string().contains("jar mods"), "{err}");

        write(
            &dir,
            "mmc-pack.json",
            r#"{"components":[{"uid":"../evil","version":"1"}]}"#,
        );
        let err = resolve(&reqwest::Client::new(), &meta, &dir)
            .await
            .unwrap_err();
        assert!(err.to_string().contains("bad component"), "{err}");
    }
}
