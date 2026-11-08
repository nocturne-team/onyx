//! Installing Modrinth content into instances, and modpacks as new instances.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use onyx_core::cache::CachePolicy;
use onyx_core::content::{self, InstalledFile, Target, Update};
use onyx_core::download::{self, Download, Progress};
use onyx_core::instance::{self, Instance, NewInstance};
use onyx_core::modpack::{self, Index};
use onyx_core::paths;
use serde::Serialize;
use tauri::{AppHandle, Manager, State};

use crate::AppState;
use crate::instances::{Game, GameState, Phase, emit};

/// Installs a project (and its required dependencies) into an instance.
#[tauri::command]
pub async fn install_to_instance(
    state: State<'_, AppState>,
    id: String,
    project_id: String,
    project_type: String,
) -> Result<Vec<InstalledFile>, String> {
    if state
        .games
        .lock()
        .unwrap()
        .get(&id)
        .is_some_and(|g| g.phase == Some(Phase::Preparing))
    {
        return Err("Wait for the instance to finish installing".into());
    }
    let root = &state.paths.instances;
    let instance = instance::load(root, &id).map_err(|e| e.to_string())?;
    let game_dir = instance::instance_dir(root, &id).map_err(|e| e.to_string())?;
    let target = Target {
        game_dir: &game_dir,
        loader: instance.loader,
        game_version: &instance.game_version,
    };
    content::install_project(&state.modrinth, &target, &project_id, &project_type)
        .await
        .map_err(|e| e.to_string())
}

/// Installed files with newer compatible versions on Modrinth.
#[tauri::command]
pub async fn check_updates(state: State<'_, AppState>, id: String) -> Result<Vec<Update>, String> {
    let root = &state.paths.instances;
    let instance = instance::load(root, &id).map_err(|e| e.to_string())?;
    let game_dir = instance::instance_dir(root, &id).map_err(|e| e.to_string())?;
    let target = Target {
        game_dir: &game_dir,
        loader: instance.loader,
        game_version: &instance.game_version,
    };
    content::check_updates(&state.modrinth, &target)
        .await
        .map_err(|e| e.to_string())
}

/// Replaces an installed file (`<folder>/<file name>`) with another version.
#[tauri::command]
pub async fn apply_update(
    state: State<'_, AppState>,
    id: String,
    key: String,
    version_id: String,
) -> Result<InstalledFile, String> {
    if state.games.lock().unwrap().contains_key(&id) {
        return Err("Stop the game before updating its content".into());
    }
    let game_dir =
        instance::instance_dir(&state.paths.instances, &id).map_err(|e| e.to_string())?;
    content::apply_update(&state.modrinth, &game_dir, &key, &version_id)
        .await
        .map_err(|e| e.to_string())
}

/// What Modrinth knows about an installed file.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KnownFile {
    pub project_id: String,
    pub title: String,
    pub icon_url: Option<String>,
    pub version_number: String,
}

/// Installed files Modrinth recognizes, keyed by `<folder>/<file name>`.
#[tauri::command]
pub async fn identify_content(
    state: State<'_, AppState>,
    id: String,
) -> Result<HashMap<String, KnownFile>, String> {
    let game_dir =
        instance::instance_dir(&state.paths.instances, &id).map_err(|e| e.to_string())?;
    let versions = content::identify(&state.modrinth, &game_dir)
        .await
        .map_err(|e| e.to_string())?;
    let mut ids: Vec<&str> = versions.values().map(|v| v.project_id.as_str()).collect();
    ids.sort_unstable();
    ids.dedup();
    let projects = state
        .modrinth
        .projects(&ids, Some(CachePolicy::LONG))
        .await
        .map_err(|e| e.to_string())?;
    let projects: HashMap<&str, _> = projects.iter().map(|p| (p.id.as_str(), p)).collect();
    Ok(versions
        .iter()
        .filter_map(|(file, version)| {
            let project = projects.get(version.project_id.as_str())?;
            Some((
                file.clone(),
                KnownFile {
                    project_id: project.id.clone(),
                    title: project.title.clone(),
                    icon_url: project.icon_url.clone(),
                    version_number: version.version_number.clone(),
                },
            ))
        })
        .collect())
}

/// Creates an instance from a `.mrpack` on disk. Returns right away; the
/// install reports progress through `instance:state` like a launch does.
#[tauri::command]
pub async fn import_mrpack(
    app: AppHandle,
    state: State<'_, AppState>,
    path: PathBuf,
) -> Result<Instance, String> {
    let read = path.clone();
    let index = tauri::async_runtime::spawn_blocking(move || modpack::read_index(&read))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())?;
    start_pack_install(app, &state, path, None, index)
}

/// Downloads the newest version of a modpack project and installs it as a
/// new instance, like `import_mrpack`.
#[tauri::command]
pub async fn install_modpack(
    app: AppHandle,
    state: State<'_, AppState>,
    project_id: String,
) -> Result<Instance, String> {
    let versions = state
        .modrinth
        .project_versions(&project_id, &[], &[], Some(CachePolicy::SHORT))
        .await
        .map_err(|e| e.to_string())?;
    let version = versions
        .iter()
        .find(|v| v.version_type == "release")
        .or_else(|| versions.first())
        .ok_or("this modpack has no versions")?;
    let file = version
        .primary_file()
        .ok_or("this modpack version has no file")?;

    let dir = paths::cache_dir()
        .ok_or("no cache directory")?
        .join("modpacks");
    let path = dir.join(format!("{}.mrpack", version.id));
    download::fetch(
        state.modrinth.http(),
        &Download {
            url: file.url.clone(),
            path: path.clone(),
            sha1: Some(file.hashes.sha1.clone()),
            size: Some(file.size),
        },
    )
    .await
    .map_err(|e| e.to_string())?;

    let read = path.clone();
    let index = tauri::async_runtime::spawn_blocking(move || modpack::read_index(&read))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())?;
    let instance = start_pack_install(app, &state, path.clone(), Some(path), index)?;
    Ok(use_project_icon(&state, &project_id, &instance)
        .await
        .unwrap_or(instance))
}

/// Gives a modpack instance its Modrinth project icon. Failing is fine: the
/// instance keeps its generated icon.
async fn use_project_icon(
    state: &AppState,
    project_id: &str,
    instance: &Instance,
) -> Option<Instance> {
    let project = state
        .modrinth
        .projects(&[project_id], Some(CachePolicy::LONG))
        .await
        .ok()?
        .into_iter()
        .next()?;
    let url = project.icon_url?;
    let extension = url.rsplit('.').next()?.split(['?', '#']).next()?.to_owned();
    let bytes = state
        .modrinth
        .http()
        .get(&url)
        .send()
        .await
        .ok()?
        .error_for_status()
        .ok()?
        .bytes()
        .await
        .ok()?;
    instance::set_icon(&state.paths.instances, &instance.id, &bytes, &extension)
        .inspect_err(|e| tracing::warn!("no icon for {}: {e}", instance.name))
        .ok()
}

/// Creates the instance and installs the pack in the background.
/// `cleanup` is a downloaded pack file to delete afterwards.
fn start_pack_install(
    app: AppHandle,
    state: &AppState,
    mrpack: PathBuf,
    cleanup: Option<PathBuf>,
    index: Index,
) -> Result<Instance, String> {
    let target = index.target().map_err(|e| e.to_string())?;
    let root = state.paths.instances.clone();
    let instance = instance::create(
        &root,
        NewInstance {
            name: index.name.clone(),
            game_version: target.game_version,
            loader: target.loader,
            loader_version: target.loader_version,
            icon_config: None,
        },
    )
    .map_err(|e| e.to_string())?;
    let game_dir = instance::instance_dir(&root, &instance.id).map_err(|e| e.to_string())?;

    let id = instance.id.clone();
    let stage = format!("Installing {}", index.name);
    state.games.lock().unwrap().insert(
        id.clone(),
        Game {
            phase: Some(Phase::Preparing),
            stage: Some(stage.clone()),
            ..Game::default()
        },
    );

    tauri::async_runtime::spawn(async move {
        let progress = Arc::new(Progress::default());
        let ticker = {
            let (app, id, progress, stage) =
                (app.clone(), id.clone(), progress.clone(), stage.clone());
            tokio::spawn(async move {
                loop {
                    emit(
                        &app,
                        GameState {
                            id: id.clone(),
                            phase: Some(Phase::Preparing),
                            stage: Some(stage.clone()),
                            progress: Some(progress.snapshot()),
                            error: None,
                        },
                    );
                    tokio::time::sleep(Duration::from_millis(250)).await;
                }
            })
        };
        let http = app.state::<AppState>().modrinth.http().clone();
        let result = modpack::install(&http, &mrpack, &index, &game_dir, &progress).await;
        ticker.abort();
        if let Some(path) = cleanup {
            let _ = tokio::fs::remove_file(path).await;
        }
        finish_install(&app, &id, result.err().map(|e| e.to_string()));
    });
    Ok(instance)
}

fn finish_install(app: &AppHandle, id: &str, error: Option<String>) {
    app.state::<AppState>().games.lock().unwrap().remove(id);
    emit(
        app,
        GameState {
            id: id.to_owned(),
            phase: None,
            stage: None,
            progress: None,
            error: error.map(|e| format!("Modpack install failed: {e}")),
        },
    );
}
