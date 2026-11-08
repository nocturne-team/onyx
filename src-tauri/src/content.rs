//! Installing Modrinth content into instances, and modpacks as new instances.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use onyx_core::cache::CachePolicy;
use onyx_core::content::{self, InstalledFile, Target, Update};
use onyx_core::download::{self, Download, Progress};
use onyx_core::instance::{self, Instance, NewInstance, PackSource};
use onyx_core::modpack::{self, Index};
use onyx_core::modrinth::Version;
use onyx_core::paths;
use serde::Serialize;
use tauri::{AppHandle, Manager, State};

use crate::AppState;
use crate::instances::{AbortOnDrop, Game, GameState, Phase, emit};

/// Installs a project (and its required dependencies) into an instance.
#[tauri::command]
pub async fn install_to_instance(
    state: State<'_, AppState>,
    id: String,
    project_id: String,
    project_type: String,
    version_id: Option<String>,
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
    content::install_project(
        &state.modrinth,
        &target,
        &project_id,
        &project_type,
        version_id.as_deref(),
    )
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
    let index = read_index(&path).await?;
    // If the file is a pack Modrinth knows, remember which, so it can update.
    let pack = match download::sha1_file(&path).await {
        Ok(sha1) => state
            .modrinth
            .versions_from_hashes(std::slice::from_ref(&sha1))
            .await
            .ok()
            .and_then(|mut found| found.remove(&sha1))
            .map(|v| PackSource {
                project_id: v.project_id,
                version_id: v.id,
                version_number: v.version_number,
            }),
        Err(_) => None,
    };
    let instance = create_pack_instance(&state, &index)?;
    start_pack_install(app, &state, &instance.id, path, None, index, pack);
    Ok(instance)
}

/// Downloads the newest version of a modpack project and installs it as a
/// new instance, like `import_mrpack`.
#[tauri::command]
pub async fn install_modpack(
    app: AppHandle,
    state: State<'_, AppState>,
    project_id: String,
) -> Result<Instance, String> {
    let version = latest_pack_version(&state, &project_id)
        .await?
        .ok_or("this modpack has no versions")?;
    let path = download_pack(&state, &version).await?;
    let index = read_index(&path).await?;
    let instance = create_pack_instance(&state, &index)?;
    start_pack_install(
        app,
        &state,
        &instance.id,
        path.clone(),
        Some(path),
        index,
        Some(pack_source(&version)),
    );
    Ok(use_project_icon(&state, &project_id, &instance)
        .await
        .unwrap_or(instance))
}

/// A newer version of the modpack an instance came from.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PackUpdate {
    pub version_id: String,
    pub version_number: String,
}

#[tauri::command]
pub async fn check_pack_update(
    state: State<'_, AppState>,
    id: String,
) -> Result<Option<PackUpdate>, String> {
    let instance = instance::load(&state.paths.instances, &id).map_err(|e| e.to_string())?;
    let Some(pack) = instance.pack else {
        return Ok(None);
    };
    let latest = latest_pack_version(&state, &pack.project_id).await?;
    Ok(latest
        .filter(|v| v.id != pack.version_id)
        .map(|v| PackUpdate {
            version_id: v.id,
            version_number: v.version_number,
        }))
}

/// Installs another version of the instance's modpack over it. Files the
/// old version brought are swapped; files the user added stay.
#[tauri::command]
pub async fn update_modpack(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
    version_id: String,
) -> Result<(), String> {
    if state.games.lock().unwrap().contains_key(&id) {
        return Err("Stop the game before updating the modpack".into());
    }
    let instance = instance::load(&state.paths.instances, &id).map_err(|e| e.to_string())?;
    let pack = instance
        .pack
        .ok_or("this instance isn't from a Modrinth modpack")?;
    let version = state
        .modrinth
        .version(&version_id, None)
        .await
        .map_err(|e| e.to_string())?;
    if version.project_id != pack.project_id {
        return Err("that version belongs to a different modpack".into());
    }
    let path = download_pack(&state, &version).await?;
    let index = read_index(&path).await?;
    index.target().map_err(|e| e.to_string())?;
    start_pack_install(
        app,
        &state,
        &id,
        path.clone(),
        Some(path),
        index,
        Some(pack_source(&version)),
    );
    Ok(())
}

async fn read_index(path: &std::path::Path) -> Result<Index, String> {
    let path = path.to_owned();
    tauri::async_runtime::spawn_blocking(move || modpack::read_index(&path))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())
}

/// Newest release (or newest anything, if there are no releases).
async fn latest_pack_version(
    state: &AppState,
    project_id: &str,
) -> Result<Option<Version>, String> {
    let versions = state
        .modrinth
        .project_versions(project_id, &[], &[], Some(CachePolicy::SHORT))
        .await
        .map_err(|e| e.to_string())?;
    let release = versions.iter().position(|v| v.version_type == "release");
    Ok(versions.into_iter().nth(release.unwrap_or(0)))
}

fn pack_source(version: &Version) -> PackSource {
    PackSource {
        project_id: version.project_id.clone(),
        version_id: version.id.clone(),
        version_number: version.version_number.clone(),
    }
}

/// Downloads a pack version's `.mrpack` into the cache dir.
async fn download_pack(state: &AppState, version: &Version) -> Result<PathBuf, String> {
    let file = version
        .primary_file()
        .ok_or("this modpack version has no file")?;
    let path = paths::cache_dir()
        .ok_or("no cache directory")?
        .join("modpacks")
        .join(format!("{}.mrpack", version.id));
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
    Ok(path)
}

fn create_pack_instance(state: &AppState, index: &Index) -> Result<Instance, String> {
    let target = index.target().map_err(|e| e.to_string())?;
    instance::create(
        &state.paths.instances,
        NewInstance {
            name: index.name.clone(),
            game_version: target.game_version,
            loader: target.loader,
            loader_version: target.loader_version,
            icon_config: None,
        },
    )
    .map_err(|e| e.to_string())
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

/// Installs the pack into an existing instance in the background, then
/// records the pack version and its game/loader versions on the instance.
/// `cleanup` is a downloaded pack file to delete afterwards.
fn start_pack_install(
    app: AppHandle,
    state: &AppState,
    id: &str,
    mrpack: PathBuf,
    cleanup: Option<PathBuf>,
    index: Index,
    pack: Option<PackSource>,
) {
    let root = state.paths.instances.clone();
    let id = id.to_owned();
    let stage = format!("Installing {} {}", index.name, index.version_id);
    state.games.lock().unwrap().insert(
        id.clone(),
        Game {
            phase: Some(Phase::Preparing),
            stage: Some(stage.clone()),
            ..Game::default()
        },
    );

    let work = {
        let (app, id) = (app.clone(), id.clone());
        tokio::spawn(async move {
            let progress = Arc::new(Progress::default());
            let _ticker = AbortOnDrop({
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
            });
            let http = app.state::<AppState>().modrinth.http().clone();
            let game_dir = instance::instance_dir(&root, &id).map_err(|e| e.to_string())?;
            modpack::install(&http, &mrpack, &index, &game_dir, &progress)
                .await
                .map_err(|e| e.to_string())?;
            let target = index.target().map_err(|e| e.to_string())?;
            let mut instance = instance::load(&root, &id).map_err(|e| e.to_string())?;
            instance.game_version = target.game_version;
            instance.loader = target.loader;
            instance.loader_version = target.loader_version;
            instance.pack = pack;
            instance::save(&root, &instance).map_err(|e| e.to_string())
        })
    };
    if let Some(game) = state.games.lock().unwrap().get_mut(&id) {
        game.abort = Some(work.abort_handle());
    }

    tauri::async_runtime::spawn(async move {
        let error = match work.await {
            Ok(result) => result.err(),
            Err(e) if e.is_cancelled() => {
                Some("cancelled; the instance may be incomplete, so update or delete it".to_owned())
            }
            Err(e) => Some(e.to_string()),
        };
        if let Some(path) = cleanup {
            let _ = tokio::fs::remove_file(path).await;
        }
        finish_install(&app, &id, error);
    });
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
