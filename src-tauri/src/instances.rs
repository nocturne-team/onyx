//! Instance commands: create, list, delete, and install + launch with progress
//! and log lines streamed to the frontend as events.

use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use onyx_core::auth::now_secs;
use onyx_core::config;
use onyx_core::download::{Progress, ProgressSnapshot};
use onyx_core::instance::{self, IconConfig, Instance, Loader, NewInstance};
use onyx_core::minecraft::forge::{self, ForgeLoader};
use onyx_core::minecraft::launch::{self, LaunchAccount, LaunchSpec};
use onyx_core::minecraft::loader::{self, LoaderVersion, MetaLoader};
use onyx_core::minecraft::{self, Environment, VersionManifest, install, java};
use onyx_core::mods::{self, ModFile};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_opener::OpenerExt;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::sync::oneshot;

use crate::AppState;
use crate::minecraft_account::active_session;

const STATE_EVENT: &str = "instance:state";
const LOG_EVENT: &str = "instance:log";
/// Lines kept per instance for when the Logs tab opens after launch.
const LOG_LINES: usize = 5000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Phase {
    /// Getting tokens, downloading Java or game files.
    Preparing,
    Running,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GameState {
    pub id: String,
    pub phase: Option<Phase>,
    /// What's happening while preparing, e.g. "Downloading Java 25".
    pub stage: Option<String>,
    pub progress: Option<ProgressSnapshot>,
    /// Set when preparing failed or the game crashed.
    pub error: Option<String>,
}

#[derive(Default)]
pub struct Game {
    pub phase: Option<Phase>,
    pub stage: Option<String>,
    pub kill: Option<oneshot::Sender<()>>,
    pub log: VecDeque<String>,
}

pub type Games = Mutex<HashMap<String, Game>>;

fn root(state: &AppState) -> &PathBuf {
    &state.paths.instances
}

#[tauri::command]
pub async fn list_instances(state: State<'_, AppState>) -> Result<Vec<Instance>, String> {
    let root = root(&state).clone();
    tauri::async_runtime::spawn_blocking(move || instance::list(&root))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn create_instance(
    state: State<'_, AppState>,
    name: String,
    game_version: String,
    loader: Loader,
    loader_version: Option<String>,
    icon_config: Option<IconConfig>,
    icon_path: Option<PathBuf>,
) -> Result<Instance, String> {
    if (loader != Loader::Vanilla) != loader_version.is_some() {
        return Err("pick a loader version".into());
    }
    let instance = instance::create(
        root(&state),
        NewInstance {
            name,
            game_version,
            loader,
            loader_version,
            icon_config,
        },
    )
    .map_err(|e| e.to_string())?;
    match icon_path {
        // The instance exists either way; a bad icon file shouldn't undo that.
        Some(path) => Ok(
            set_icon_from_file(&state, &instance.id, &path).unwrap_or_else(|e| {
                tracing::warn!("ignoring icon {}: {e}", path.display());
                instance
            }),
        ),
        None => Ok(instance),
    }
}

fn set_icon_from_file(
    state: &AppState,
    id: &str,
    path: &std::path::Path,
) -> Result<Instance, String> {
    let extension = path
        .extension()
        .and_then(|e| e.to_str())
        .ok_or("pick a PNG, JPEG, WebP or GIF image")?;
    let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    instance::set_icon(root(state), id, &bytes, extension).map_err(|e| e.to_string())
}

/// Copies an image file in as the instance's icon.
#[tauri::command]
pub fn set_instance_icon(
    state: State<'_, AppState>,
    id: String,
    path: PathBuf,
) -> Result<Instance, String> {
    set_icon_from_file(&state, &id, &path)
}

/// Switches to a built-in icon, dropping any custom image.
#[tauri::command]
pub fn set_instance_icon_config(
    state: State<'_, AppState>,
    id: String,
    config: IconConfig,
) -> Result<Instance, String> {
    instance::set_icon_config(root(&state), &id, config).map_err(|e| e.to_string())
}

/// Preview of an image the user picked, before it's saved anywhere.
#[tauri::command]
pub fn image_preview(path: PathBuf) -> Option<String> {
    instance::image_data_url(&path)
}

/// The instance's custom icon as a data URL, or `None` for generated icons.
#[tauri::command]
pub fn instance_icon(state: State<'_, AppState>, id: String) -> Result<Option<String>, String> {
    let instance = instance::load(root(&state), &id).map_err(|e| e.to_string())?;
    Ok(instance::icon_data_url(root(&state), &instance))
}

/// Game versions a loader supports.
#[tauri::command]
pub async fn loader_game_versions(
    state: State<'_, AppState>,
    loader: Loader,
) -> Result<Vec<String>, String> {
    let http = state.modrinth.http();
    let result = match (
        MetaLoader::from_loader(loader),
        ForgeLoader::from_loader(loader),
    ) {
        (Some(meta_loader), _) => loader::game_versions(http, meta_loader).await,
        (_, Some(forge_loader)) => forge::game_versions(http, forge_loader).await,
        _ => return Err("vanilla has no loader versions".into()),
    };
    result.map_err(|e| e.to_string())
}

/// Loader versions for a game version, newest first.
#[tauri::command]
pub async fn loader_versions(
    state: State<'_, AppState>,
    loader: Loader,
    game_version: String,
) -> Result<Vec<LoaderVersion>, String> {
    let http = state.modrinth.http();
    let result = match (
        MetaLoader::from_loader(loader),
        ForgeLoader::from_loader(loader),
    ) {
        (Some(meta_loader), _) => loader::loader_versions(http, meta_loader, &game_version).await,
        (_, Some(forge_loader)) => forge::loader_versions(http, forge_loader, &game_version).await,
        _ => return Err("vanilla has no loader versions".into()),
    };
    result.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn list_mods(state: State<'_, AppState>, id: String) -> Result<Vec<ModFile>, String> {
    let dir = instance::instance_dir(root(&state), &id).map_err(|e| e.to_string())?;
    tauri::async_runtime::spawn_blocking(move || mods::list(&dir))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn set_mod_enabled(
    state: State<'_, AppState>,
    id: String,
    file_name: String,
    enabled: bool,
) -> Result<String, String> {
    let dir = instance::instance_dir(root(&state), &id).map_err(|e| e.to_string())?;
    mods::set_enabled(&dir, &file_name, enabled).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn remove_mod(state: State<'_, AppState>, id: String, file_name: String) -> Result<(), String> {
    let dir = instance::instance_dir(root(&state), &id).map_err(|e| e.to_string())?;
    mods::remove(&dir, &file_name).map_err(|e| e.to_string())
}

/// Opens a folder inside the instance (e.g. `mods`), creating it if needed.
#[tauri::command]
pub fn open_instance_subfolder(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
    folder: String,
) -> Result<(), String> {
    const ALLOWED: &[&str] = &[
        "mods",
        "resourcepacks",
        "shaderpacks",
        "saves",
        "screenshots",
    ];
    if !ALLOWED.contains(&folder.as_str()) {
        return Err("unknown folder".into());
    }
    let dir = instance::instance_dir(root(&state), &id)
        .map_err(|e| e.to_string())?
        .join(folder);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    app.opener()
        .open_path(dir.to_string_lossy(), None::<&str>)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn delete_instance(state: State<'_, AppState>, id: String) -> Result<(), String> {
    if state.games.lock().unwrap().contains_key(&id) {
        return Err("Stop the game before deleting the instance".into());
    }
    let root = root(&state).clone();
    tauri::async_runtime::spawn_blocking(move || instance::delete(&root, &id))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn open_instance_folder(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
) -> Result<(), String> {
    let dir = instance::instance_dir(root(&state), &id).map_err(|e| e.to_string())?;
    app.opener()
        .open_path(dir.to_string_lossy(), None::<&str>)
        .map_err(|e| e.to_string())
}

/// Every Minecraft version, newest first.
#[tauri::command]
pub async fn game_versions(state: State<'_, AppState>) -> Result<VersionManifest, String> {
    minecraft::version_manifest(state.modrinth.http(), &state.paths.meta)
        .await
        .map_err(|e| e.to_string())
}

/// What's currently preparing or running, for the UI's initial state.
#[tauri::command]
pub fn running_games(state: State<'_, AppState>) -> Vec<GameState> {
    state
        .games
        .lock()
        .unwrap()
        .iter()
        .map(|(id, game)| GameState {
            id: id.clone(),
            phase: game.phase,
            stage: game.stage.clone(),
            progress: None,
            error: None,
        })
        .collect()
}

#[tauri::command]
pub fn game_log(state: State<'_, AppState>, id: String) -> Vec<String> {
    state
        .games
        .lock()
        .unwrap()
        .get(&id)
        .map(|g| g.log.iter().cloned().collect())
        .or_else(|| state.last_logs.lock().unwrap().get(&id).cloned())
        .unwrap_or_default()
}

#[tauri::command]
pub fn stop_game(state: State<'_, AppState>, id: String) {
    if let Some(kill) = state
        .games
        .lock()
        .unwrap()
        .get_mut(&id)
        .and_then(|g| g.kill.take())
    {
        let _ = kill.send(());
    }
}

/// Installs whatever is missing and starts the game. Returns once the game
/// process has started (or preparing failed); everything after that arrives
/// as `instance:state` and `instance:log` events.
#[tauri::command]
pub async fn launch_instance(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
) -> Result<(), String> {
    {
        let mut games = state.games.lock().unwrap();
        if games.contains_key(&id) {
            return Err("This instance is already running".into());
        }
        games.insert(
            id.clone(),
            Game {
                phase: Some(Phase::Preparing),
                ..Game::default()
            },
        );
    }
    state.last_logs.lock().unwrap().remove(&id);

    match prepare_and_spawn(&app, &state, &id).await {
        Ok(()) => Ok(()),
        Err(error) => {
            state.games.lock().unwrap().remove(&id);
            emit(
                &app,
                GameState {
                    id: id.clone(),
                    phase: None,
                    stage: None,
                    progress: None,
                    error: Some(error.clone()),
                },
            );
            Err(error)
        }
    }
}

pub(crate) fn emit(app: &AppHandle, state: GameState) {
    let _ = app.emit(STATE_EVENT, state);
}

fn set_stage(
    app: &AppHandle,
    state: &AppState,
    id: &str,
    stage: &str,
    progress: Option<ProgressSnapshot>,
) {
    if let Some(game) = state.games.lock().unwrap().get_mut(id) {
        game.stage = Some(stage.to_owned());
    }
    emit(
        app,
        GameState {
            id: id.to_owned(),
            phase: Some(Phase::Preparing),
            stage: Some(stage.to_owned()),
            progress,
            error: None,
        },
    );
}

async fn prepare_and_spawn(app: &AppHandle, state: &AppState, id: &str) -> Result<(), String> {
    let root = root(state).clone();
    let mut instance = instance::load(&root, id).map_err(|e| e.to_string())?;
    let game_dir = instance::instance_dir(&root, id).map_err(|e| e.to_string())?;
    let http = state.modrinth.http();
    let meta = &state.paths.meta;
    let env = Environment::current();

    set_stage(app, state, id, "Signing in", None);
    let session = active_session(state).await?;

    set_stage(app, state, id, "Getting version info", None);
    let mut version = minecraft::version_info(http, meta, &instance.game_version)
        .await
        .map_err(|e| e.to_string())?;
    if let (Some(meta_loader), Some(loader_version)) = (
        MetaLoader::from_loader(instance.loader),
        instance.loader_version.as_deref(),
    ) {
        let profile = loader::profile(
            http,
            meta,
            meta_loader,
            &instance.game_version,
            loader_version,
        )
        .await
        .map_err(|e| e.to_string())?;
        version = loader::merge(version, profile);
    }

    // Progress is polled and emitted on a timer rather than per file.
    let progress = Arc::new(Progress::default());
    let stage = Arc::new(Mutex::new(String::new()));
    let ticker = {
        let (app, id, progress, stage) =
            (app.clone(), id.to_owned(), progress.clone(), stage.clone());
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_millis(250)).await;
                let stage = stage.lock().unwrap().clone();
                emit(
                    &app,
                    GameState {
                        id: id.clone(),
                        phase: Some(Phase::Preparing),
                        stage: Some(stage),
                        progress: Some(progress.snapshot()),
                        error: None,
                    },
                );
            }
        })
    };
    let result = async {
        let (component, major) = version
            .java_version
            .as_ref()
            .map_or(("jre-legacy", 8), |j| {
                (j.component.as_str(), j.major_version)
            });
        *stage.lock().unwrap() = format!("Downloading Java {major}");
        let java = java::ensure_runtime(http, meta, component, &progress)
            .await
            .map_err(|e| e.to_string())?;
        progress.reset();
        *stage.lock().unwrap() = format!("Downloading Minecraft {}", version.id);
        install::install_version(http, meta, &version, &env, &progress)
            .await
            .map_err(|e| e.to_string())?;
        let index = install::asset_index(http, meta, &version)
            .await
            .map_err(|e| e.to_string())?;
        launch::prepare_game_dir(meta, &index, &game_dir)
            .await
            .map_err(|e| e.to_string())?;
        let forge_version = match (
            ForgeLoader::from_loader(instance.loader),
            instance.loader_version.as_deref(),
        ) {
            (Some(forge_loader), Some(loader_version)) => {
                progress.reset();
                *stage.lock().unwrap() = format!(
                    "Installing {}",
                    if forge_loader == ForgeLoader::Forge {
                        "Forge"
                    } else {
                        "NeoForge"
                    }
                );
                Some(
                    forge::install(
                        http,
                        meta,
                        forge_loader,
                        &version,
                        loader_version,
                        &java,
                        &env,
                        &progress,
                    )
                    .await
                    .map_err(|e| e.to_string())?,
                )
            }
            _ => None,
        };
        Ok::<_, String>((java, forge_version))
    }
    .await;
    ticker.abort();
    let (java, forge_version) = result?;
    if let Some(forge_version) = forge_version {
        version = forge::merge(version, forge_version);
    }

    set_stage(app, state, id, "Starting", None);
    let account = LaunchAccount {
        name: session.account.name.clone(),
        uuid: session.account.id.clone(),
        access_token: session.access_token.clone(),
        xuid: session.xuid.clone(),
    };
    let spec = LaunchSpec {
        meta,
        version: &version,
        env: &env,
        java,
        game_dir: game_dir.clone(),
        account: &account,
        client_id: config::MSA_CLIENT_ID.unwrap_or_default(),
        max_memory_mb: instance.max_memory_mb,
        launcher_version: config::VERSION,
    };
    let mut child = launch::command(&spec)
        .spawn()
        .map_err(|e| format!("could not start Java: {e}"))?;

    instance.last_played = Some(now_secs());
    if let Err(e) = instance::save(&root, &instance) {
        tracing::warn!("could not save last played time: {e}");
    }

    let (kill_tx, kill_rx) = oneshot::channel();
    if let Some(game) = state.games.lock().unwrap().get_mut(id) {
        game.phase = Some(Phase::Running);
        game.stage = None;
        game.kill = Some(kill_tx);
    }
    emit(
        app,
        GameState {
            id: id.to_owned(),
            phase: Some(Phase::Running),
            stage: None,
            progress: None,
            error: None,
        },
    );

    let stdout = child.stdout.take().expect("stdout is piped");
    let stderr = child.stderr.take().expect("stderr is piped");
    let app = app.clone();
    let id = id.to_owned();
    tauri::async_runtime::spawn(async move {
        let started = Instant::now();
        let mut out = BufReader::new(stdout).lines();
        let mut err = BufReader::new(stderr).lines();
        let mut kill_rx = kill_rx;
        let (mut out_open, mut err_open) = (true, true);
        let (status, killed) = loop {
            tokio::select! {
                line = out.next_line(), if out_open => match line {
                    Ok(Some(line)) => log_line(&app, &id, line),
                    _ => out_open = false,
                },
                line = err.next_line(), if err_open => match line {
                    Ok(Some(line)) => log_line(&app, &id, line),
                    _ => err_open = false,
                },
                status = child.wait(), if !out_open && !err_open => break (status, false),
                _ = &mut kill_rx => {
                    let _ = child.kill().await;
                    break (child.wait().await, true);
                }
            }
        };
        finish(&app, &id, &root, started.elapsed(), status, killed);
    });
    Ok(())
}

fn log_line(app: &AppHandle, id: &str, line: String) {
    let state = app.state::<AppState>();
    if let Some(game) = state.games.lock().unwrap().get_mut(id) {
        if game.log.len() >= LOG_LINES {
            game.log.pop_front();
        }
        game.log.push_back(line.clone());
    }
    let _ = app.emit(LOG_EVENT, (id, line));
}

fn finish(
    app: &AppHandle,
    id: &str,
    root: &std::path::Path,
    played: Duration,
    status: std::io::Result<std::process::ExitStatus>,
    killed: bool,
) {
    let state = app.state::<AppState>();
    if let Some(game) = state.games.lock().unwrap().remove(id) {
        state
            .last_logs
            .lock()
            .unwrap()
            .insert(id.to_owned(), game.log.into_iter().collect());
    }
    if let Ok(mut instance) = instance::load(root, id) {
        instance.played_secs += played.as_secs();
        if let Err(e) = instance::save(root, &instance) {
            tracing::warn!("could not save play time: {e}");
        }
    }

    let error = match status {
        Ok(_) if killed => None,
        Ok(status) if status.success() => None,
        Ok(status) => Some(match status.code() {
            Some(code) => format!("Minecraft exited with code {code}. Check the Logs tab."),
            None => "Minecraft crashed. Check the Logs tab.".to_owned(),
        }),
        Err(e) => Some(format!("lost track of the game process: {e}")),
    };
    emit(
        app,
        GameState {
            id: id.to_owned(),
            phase: None,
            stage: None,
            progress: None,
            error,
        },
    );
}
