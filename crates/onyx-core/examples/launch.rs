//! Installs a version into the real meta dir and launches it with an offline
//! dummy account. For checking the install/launch pipeline without the UI:
//!
//! `cargo run -p onyx-core --example launch -- 26.3 /tmp/game [fabric|quilt|forge|neoforge]`
//!
//! `-- prism <instance dir>` launches an instance imported from Prism with
//! its own components (see the `prism_import` example), from `.onyx/prism/`.

use std::sync::Arc;
use std::time::Duration;

use onyx_core::download::Progress;
use onyx_core::minecraft::forge::{self, ForgeLoader};
use onyx_core::minecraft::launch::{self, LaunchAccount, LaunchSpec};
use onyx_core::minecraft::loader::{self, MetaLoader};
use onyx_core::minecraft::{self, Environment, MetaDirs, install, java};
use onyx_core::{config, paths};
use tokio::io::{AsyncBufReadExt, BufReader};

#[tokio::main]
async fn main() {
    let mut args = std::env::args().skip(1);
    let id = args.next().unwrap_or_else(|| "26.3".into());
    let game_dir = args.next().expect("pass a game directory");
    let loader_name = args.next();
    let meta_loader = match loader_name.as_deref() {
        Some("fabric") => Some(MetaLoader::Fabric),
        Some("quilt") => Some(MetaLoader::Quilt),
        _ => None,
    };
    let forge_loader = match loader_name.as_deref() {
        Some("forge") => Some(ForgeLoader::Forge),
        Some("neoforge") => Some(ForgeLoader::NeoForge),
        _ => None,
    };

    let http = reqwest::Client::builder()
        .user_agent(config::USER_AGENT)
        .build()
        .unwrap();
    let meta = MetaDirs::new(paths::meta_dir().unwrap());
    let env = Environment::current();
    let progress = Arc::new(Progress::default());

    let ticker = {
        let progress = progress.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_secs(2)).await;
                let p = progress.snapshot();
                eprintln!(
                    "  {}/{} files, {}/{} MB",
                    p.done_files,
                    p.total_files,
                    p.done_bytes >> 20,
                    p.total_bytes >> 20
                );
            }
        })
    };

    let mut version = if id == "prism" {
        let dir = onyx_core::prism::profile::profile_dir(game_dir.as_ref());
        onyx_core::prism::profile::resolve(&http, &meta, &dir)
            .await
            .unwrap_or_else(|e| panic!("{e}"))
    } else {
        minecraft::version_info(&http, &meta, &id).await.unwrap()
    };
    if let Some(meta_loader) = meta_loader {
        let versions = loader::loader_versions(&http, meta_loader, &id)
            .await
            .unwrap();
        let pick = versions.iter().find(|v| v.stable).unwrap_or(&versions[0]);
        eprintln!("loader {}", pick.version);
        let profile = loader::profile(&http, &meta, meta_loader, &id, &pick.version)
            .await
            .unwrap();
        version = loader::merge(version, profile);
    }
    let component = version
        .java_version
        .as_ref()
        .map_or("jre-legacy", |j| j.component.as_str())
        .to_owned();
    let component = component.as_str();
    eprintln!("java runtime {component}");
    let java = java::ensure_runtime(&http, &meta, component, &progress)
        .await
        .unwrap();
    progress.reset();
    eprintln!("installing {id}");
    install::install_version(&http, &meta, &version, &env, &progress)
        .await
        .unwrap();
    if let Some(forge_loader) = forge_loader {
        let versions = forge::loader_versions(&http, forge_loader, &id)
            .await
            .unwrap();
        let pick = versions.iter().find(|v| v.stable).unwrap_or(&versions[0]);
        eprintln!("loader {}", pick.version);
        let started = std::time::Instant::now();
        let forge_version = forge::install(
            &http,
            &meta,
            forge_loader,
            &version,
            &pick.version,
            &java,
            &env,
            &progress,
        )
        .await
        .unwrap_or_else(|e| panic!("{e}"));
        eprintln!("loader installed in {:?}", started.elapsed());
        version = forge::merge(version, forge_version);
        install::install_version(&http, &meta, &version, &env, &progress)
            .await
            .unwrap();
    }
    ticker.abort();

    let index = install::asset_index(&http, &meta, &version).await.unwrap();
    launch::prepare_game_dir(&meta, &index, game_dir.as_ref())
        .await
        .unwrap();
    let account = LaunchAccount {
        name: "OnyxTest".into(),
        uuid: "00000000000000000000000000000000".into(),
        access_token: "0".into(),
        xuid: "0".into(),
    };
    let spec = LaunchSpec {
        meta: &meta,
        version: &version,
        env: &env,
        java: java.clone(),
        game_dir: game_dir.into(),
        account: &account,
        client_id: "",
        max_memory_mb: std::env::var("ONYX_MEMORY_MB")
            .ok()
            .and_then(|s| s.parse().ok()),
        launcher_version: config::VERSION,
        join: None,
    };
    let mut child = launch::command(&spec).spawn().unwrap();
    let mut out = BufReader::new(child.stdout.take().unwrap()).lines();
    let mut err = BufReader::new(child.stderr.take().unwrap()).lines();
    let deadline = tokio::time::sleep(Duration::from_secs(
        std::env::var("ONYX_RUN_SECS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(45),
    ));
    tokio::pin!(deadline);
    loop {
        tokio::select! {
            Ok(Some(line)) = out.next_line() => println!("{line}"),
            Ok(Some(line)) = err.next_line() => println!("ERR {line}"),
            status = child.wait() => { println!("exited: {:?}", status); break; }
            () = &mut deadline => { println!("still running, stopping it"); child.kill().await.unwrap(); break; }
        }
    }
}
