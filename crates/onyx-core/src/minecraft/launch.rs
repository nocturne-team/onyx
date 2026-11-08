//! Builds the `java ...` command line for an installed version.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use super::install::{AssetIndex, copy_assets, log_config_path};
use super::version::{Argument, Environment, Os, VersionInfo};
use super::{InstallError, MetaDirs};

/// The signed-in Minecraft account, as the game sees it.
#[derive(Debug, Clone)]
pub struct LaunchAccount {
    pub name: String,
    /// Without dashes, as Mojang returns it.
    pub uuid: String,
    pub access_token: String,
    pub xuid: String,
}

#[derive(Debug, Clone)]
pub struct LaunchSpec<'a> {
    pub meta: &'a MetaDirs,
    pub version: &'a VersionInfo,
    pub env: &'a Environment,
    pub java: PathBuf,
    pub game_dir: PathBuf,
    pub account: &'a LaunchAccount,
    /// Passed as `--clientId`; the MSA client ID the account signed in with.
    pub client_id: &'a str,
    /// Overrides Mojang's recommended heap size when set.
    pub max_memory_mb: Option<u32>,
    pub launcher_version: &'a str,
}

/// Version-specific setup inside the game directory. Pre-1.6 versions read
/// sounds from `<game dir>/resources`.
pub async fn prepare_game_dir(
    meta: &MetaDirs,
    index: &AssetIndex,
    game_dir: &Path,
) -> Result<(), InstallError> {
    tokio::fs::create_dir_all(game_dir)
        .await
        .map_err(InstallError::io(game_dir))?;
    if index.map_to_resources {
        copy_assets(meta, index, &game_dir.join("resources")).await?;
    }
    Ok(())
}

/// The full argument list after the Java binary: JVM options, main class,
/// then game options.
pub fn arguments(spec: &LaunchSpec) -> Vec<String> {
    let version = spec.version;
    let env = spec.env;
    let vars = variables(spec);
    let substitute = |arg: &str| substitute(arg, &vars);

    let mut args: Vec<String> = Vec::new();

    // Heap size: ours if set, otherwise Mojang's recommendation, otherwise 2 GB.
    let defaults: Vec<&str> = version
        .arguments
        .as_ref()
        .map(|a| {
            a.default_user_jvm
                .iter()
                .flat_map(|arg| arg.values(env))
                .collect()
        })
        .unwrap_or_default();
    match spec.max_memory_mb {
        Some(mb) => {
            args.push(format!("-Xmx{mb}M"));
            args.extend(
                defaults
                    .iter()
                    .filter(|a| !a.starts_with("-Xmx") && !a.starts_with("-Xms"))
                    .map(|a| a.to_string()),
            );
        }
        None if defaults.is_empty() => args.push("-Xmx2G".into()),
        None => args.extend(defaults.iter().map(|a| a.to_string())),
    }

    let jvm = version
        .arguments
        .as_ref()
        .map(|a| a.jvm.as_slice())
        .unwrap_or_default();
    if version.minecraft_arguments.is_some() || jvm.is_empty() {
        // Pre-1.13 versions have no JVM arguments in their JSON; a loader may
        // have added a few, which go after these.
        if env.os == Os::Windows {
            args.push(
                "-XX:HeapDumpPath=MojangTricksIntelDriversForPerformance_javaw.exe_minecraft.exe.heapdump"
                    .into(),
            );
        }
        args.push(substitute("-Djava.library.path=${natives_directory}"));
        args.push(substitute("-Dminecraft.launcher.brand=${launcher_name}"));
        args.push(substitute(
            "-Dminecraft.launcher.version=${launcher_version}",
        ));
        args.push("-cp".into());
        args.push(substitute("${classpath}"));
    }
    args.extend(jvm_or_game(jvm, env).map(substitute));

    // Log4Shell. The property covers log4j 2.10+; 1.7 to 1.11 run 2.0-beta9
    // and need Mojang's patched config instead. Newer configs are skipped
    // because they turn the console output into XML.
    args.push("-Dlog4j2.formatMsgNoLookups=true".into());
    if let Some(logging) = version.logging.as_ref().and_then(|l| l.client.as_ref())
        && logging.file.id == "client-1.7.xml"
    {
        let path = log_config_path(spec.meta, &logging.file.id);
        args.push(logging.argument.replace("${path}", &path.to_string_lossy()));
    }

    args.push(version.main_class.clone());

    match (&version.arguments, &version.minecraft_arguments) {
        (Some(arguments), _) if !arguments.game.is_empty() => {
            args.extend(jvm_or_game(&arguments.game, env).map(substitute));
        }
        (_, Some(legacy)) => args.extend(legacy.split_whitespace().map(substitute)),
        _ => {}
    }
    args
}

fn jvm_or_game<'a>(
    arguments: &'a [Argument],
    env: &'a Environment,
) -> impl Iterator<Item = &'a str> + 'a {
    arguments.iter().flat_map(move |arg| arg.values(env))
}

fn substitute(arg: &str, vars: &HashMap<&str, String>) -> String {
    let mut out = String::with_capacity(arg.len());
    let mut rest = arg;
    while let Some(start) = rest.find("${") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        match after.find('}') {
            Some(end) => {
                let key = &after[..end];
                match vars.get(key) {
                    Some(value) => out.push_str(value),
                    None => {
                        out.push_str(&rest[start..start + 2 + end + 1]);
                    }
                }
                rest = &after[end + 1..];
            }
            None => {
                out.push_str(&rest[start..]);
                rest = "";
            }
        }
    }
    out.push_str(rest);
    out
}

fn variables(spec: &LaunchSpec) -> HashMap<&'static str, String> {
    let meta = spec.meta;
    let version = spec.version;
    let path = |p: &Path| p.to_string_lossy().into_owned();
    let assets_root = meta.assets();
    let game_assets = match version.assets.as_str() {
        "legacy" | "pre-1.6" => assets_root.join("virtual").join("legacy"),
        _ => assets_root.clone(),
    };
    let separator = if spec.env.os == Os::Windows { ";" } else { ":" };

    HashMap::from([
        ("auth_player_name", spec.account.name.clone()),
        ("auth_uuid", spec.account.uuid.clone()),
        ("auth_access_token", spec.account.access_token.clone()),
        (
            "auth_session",
            format!("token:{}:{}", spec.account.access_token, spec.account.uuid),
        ),
        ("auth_xuid", spec.account.xuid.clone()),
        ("clientid", spec.client_id.to_owned()),
        ("user_type", "msa".into()),
        ("user_properties", "{}".into()),
        ("version_name", version.id.clone()),
        ("version_type", version.kind.clone()),
        ("game_directory", path(&spec.game_dir)),
        ("assets_root", path(&assets_root)),
        ("game_assets", path(&game_assets)),
        ("assets_index_name", version.asset_index.id.clone()),
        ("natives_directory", path(&meta.natives(&version.id))),
        ("library_directory", path(&meta.libraries())),
        ("classpath_separator", separator.into()),
        ("classpath", classpath(spec).join(separator)),
        ("launcher_name", "onyx".into()),
        ("launcher_version", spec.launcher_version.to_owned()),
    ])
}

/// Library jars allowed on this OS, then the client jar. Natives jars are
/// included too; newer LWJGL loads its natives from the classpath.
pub fn classpath(spec: &LaunchSpec) -> Vec<String> {
    let libraries = spec.meta.libraries();
    let mut seen = HashSet::new();
    spec.version
        .libraries
        .iter()
        .filter(|lib| lib.allowed(spec.env))
        .filter_map(|lib| lib.artifact()?.path.as_ref())
        .map(|path| libraries.join(path))
        .chain([spec.meta.client_jar(&spec.version.id)])
        .map(|p| p.to_string_lossy().into_owned())
        .filter(|p| seen.insert(p.clone()))
        .collect()
}

/// The command to start the game. stdout and stderr are piped so the caller
/// can show the log.
pub fn command(spec: &LaunchSpec) -> tokio::process::Command {
    let mut command = tokio::process::Command::new(&spec.java);
    command
        .args(arguments(spec))
        .current_dir(&spec.game_dir)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    #[cfg(windows)]
    {
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    command
}

#[cfg(test)]
mod tests {
    use super::*;

    fn account() -> LaunchAccount {
        LaunchAccount {
            name: "Steve".into(),
            uuid: "0123456789abcdef0123456789abcdef".into(),
            access_token: "token".into(),
            xuid: "42".into(),
        }
    }

    /// The tests build paths from "/m", which `Path::join` extends with `\` on Windows.
    fn unix_paths(args: Vec<String>) -> Vec<String> {
        args.into_iter().map(|a| a.replace('\\', "/")).collect()
    }

    fn env() -> Environment {
        Environment {
            os: Os::Linux,
            arch: "x86_64",
            os_version: None,
            features: Vec::new(),
        }
    }

    const MODERN: &str = r#"{
        "id": "26.3", "type": "release", "mainClass": "net.minecraft.client.main.Main",
        "arguments": {
            "default-user-jvm": [{"value": ["-Xms2G", "-Xmx4G", "-XX:+UseCompactObjectHeaders"]}],
            "jvm": ["-Djava.library.path=${natives_directory}/java", "-cp", "${classpath}",
                {"rules": [{"action": "allow", "os": {"name": "windows"}}], "value": "-XX:HeapDumpPath=x"}],
            "game": ["--username", "${auth_player_name}", "--uuid", "${auth_uuid}",
                "--versionType", "${version_type}", "--unknown", "${not_a_variable}",
                {"rules": [{"action": "allow", "features": {"is_demo_user": true}}], "value": "--demo"}]
        },
        "assetIndex": {"id": "34", "sha1": "a", "size": 1, "url": "https://x/34.json"},
        "assets": "34",
        "downloads": {"client": {"sha1": "c", "size": 3, "url": "https://x/client.jar"}},
        "libraries": [
            {"name": "a:b:1", "downloads": {"artifact": {"path": "a/b/1/b-1.jar", "sha1": "1", "size": 1, "url": "u"}}},
            {"name": "a:b:1", "downloads": {"artifact": {"path": "a/b/1/b-1.jar", "sha1": "1", "size": 1, "url": "u"}}},
            {"name": "c:mac:1", "rules": [{"action": "allow", "os": {"name": "osx"}}],
             "downloads": {"artifact": {"path": "c/mac.jar", "sha1": "2", "size": 1, "url": "u"}}}
        ]
    }"#;

    #[test]
    fn modern_arguments() {
        let version: VersionInfo = serde_json::from_str(MODERN).unwrap();
        let meta = MetaDirs::new("/m");
        let env = env();
        let account = account();
        let spec = LaunchSpec {
            meta: &meta,
            version: &version,
            env: &env,
            java: "/java".into(),
            game_dir: "/game".into(),
            account: &account,
            client_id: "cid",
            max_memory_mb: None,
            launcher_version: "0.1.0",
        };
        let args = unix_paths(arguments(&spec));
        assert_eq!(
            args,
            [
                "-Xms2G",
                "-Xmx4G",
                "-XX:+UseCompactObjectHeaders",
                "-Djava.library.path=/m/natives/26.3/java",
                "-cp",
                "/m/libraries/a/b/1/b-1.jar:/m/versions/26.3/26.3.jar",
                "-Dlog4j2.formatMsgNoLookups=true",
                "net.minecraft.client.main.Main",
                "--username",
                "Steve",
                "--uuid",
                "0123456789abcdef0123456789abcdef",
                "--versionType",
                "release",
                "--unknown",
                "${not_a_variable}",
            ]
        );

        let spec = LaunchSpec {
            max_memory_mb: Some(6144),
            ..spec
        };
        let args = arguments(&spec);
        assert_eq!(&args[..2], ["-Xmx6144M", "-XX:+UseCompactObjectHeaders"]);
    }

    #[test]
    fn legacy_arguments() {
        let version: VersionInfo = serde_json::from_str(
            r#"{
            "id": "1.8.9", "type": "release", "mainClass": "net.minecraft.client.main.Main",
            "minecraftArguments": "--username ${auth_player_name} --userProperties ${user_properties} --assetsDir ${assets_root}",
            "assetIndex": {"id": "1.8", "sha1": "a", "size": 1, "url": "u"},
            "assets": "1.8",
            "downloads": {"client": {"sha1": "c", "size": 3, "url": "u"}},
            "libraries": []
        }"#,
        )
        .unwrap();
        let meta = MetaDirs::new("/m");
        let env = env();
        let account = account();
        let spec = LaunchSpec {
            meta: &meta,
            version: &version,
            env: &env,
            java: "/java".into(),
            game_dir: "/game".into(),
            account: &account,
            client_id: "cid",
            max_memory_mb: None,
            launcher_version: "0.1.0",
        };
        let args = unix_paths(arguments(&spec));
        assert_eq!(
            args,
            [
                "-Xmx2G",
                "-Djava.library.path=/m/natives/1.8.9",
                "-Dminecraft.launcher.brand=onyx",
                "-Dminecraft.launcher.version=0.1.0",
                "-cp",
                "/m/versions/1.8.9/1.8.9.jar",
                "-Dlog4j2.formatMsgNoLookups=true",
                "net.minecraft.client.main.Main",
                "--username",
                "Steve",
                "--userProperties",
                "{}",
                "--assetsDir",
                "/m/assets",
            ]
        );
    }

    #[test]
    fn substitution_handles_odd_input() {
        let vars = HashMap::from([("a", "1".to_owned())]);
        assert_eq!(substitute("x${a}y${a}", &vars), "x1y1");
        assert_eq!(substitute("${a", &vars), "${a");
        assert_eq!(substitute("$a", &vars), "$a");
    }
}
