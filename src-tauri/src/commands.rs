use onyx_core::cache::CachePolicy;
use onyx_core::modrinth::{Category, ProjectPage, SearchParams, SearchResults, Version};
use onyx_core::{Settings, config, paths};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, State};
use tauri_plugin_opener::OpenerExt;

use crate::AppState;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    version: &'static str,
    os: String,
    /// Physical memory, for the memory sliders. `None` if unknown.
    total_memory_mb: Option<u64>,
}

#[tauri::command]
pub fn app_info() -> AppInfo {
    let os = os_info::get();
    AppInfo {
        version: config::VERSION,
        os: format!("{} {}", os.os_type(), os.version()),
        total_memory_mb: onyx_core::settings::total_memory_mb(),
    }
}

#[tauri::command]
pub fn get_settings(state: State<'_, AppState>) -> Settings {
    state.settings.lock().unwrap().clone()
}

#[tauri::command]
pub fn set_settings(state: State<'_, AppState>, settings: Settings) -> Result<(), String> {
    if settings
        .default_memory_mb
        .is_some_and(|mb| !onyx_core::settings::MEMORY_RANGE_MB.contains(&mb))
    {
        return Err("memory must be between 512 MB and 64 GB".into());
    }
    if !onyx_core::settings::CONCURRENT_DOWNLOADS_RANGE.contains(&settings.max_concurrent_downloads)
        || settings.download_limit_kib == Some(0)
    {
        return Err("invalid download limits".into());
    }
    let path = paths::settings_file().ok_or("could not find a data directory")?;
    settings.save(&path).map_err(|e| e.to_string())?;
    settings.apply_download_limits();
    *state.settings.lock().unwrap() = settings;
    Ok(())
}

/// How long the frontend is happy to see cached results for a call.
#[derive(Deserialize, Clone, Copy)]
#[serde(rename_all = "lowercase")]
pub enum CacheMode {
    None,
    Short,
    Long,
}

impl From<CacheMode> for Option<CachePolicy> {
    fn from(mode: CacheMode) -> Self {
        match mode {
            CacheMode::None => None,
            CacheMode::Short => Some(CachePolicy::SHORT),
            CacheMode::Long => Some(CachePolicy::LONG),
        }
    }
}

#[tauri::command]
pub async fn search(
    state: State<'_, AppState>,
    params: SearchParams,
    cache: CacheMode,
) -> Result<SearchResults, String> {
    state
        .modrinth
        .search(&params, cache.into())
        .await
        .map_err(|e| e.to_string())
}

/// Category tags for the Browse filters. Rarely change, so cached for an hour.
#[tauri::command]
pub async fn categories(state: State<'_, AppState>) -> Result<Vec<Category>, String> {
    state
        .modrinth
        .categories(Some(CachePolicy::LONG))
        .await
        .map_err(|e| e.to_string())
}

fn modrinth_token(state: &AppState) -> Option<String> {
    state
        .session
        .lock()
        .unwrap()
        .as_ref()
        .map(|s| s.token.access_token.clone())
}

/// A project's page, as the signed-in user when there is one (so their own
/// drafts and private projects open too). Cached in memory for 5 minutes.
#[tauri::command]
pub async fn project_page(state: State<'_, AppState>, id: String) -> Result<ProjectPage, String> {
    let token = modrinth_token(&state);
    state
        .modrinth
        .project_page(&id, token.as_deref(), Some(CachePolicy::SHORT))
        .await
        .map_err(|e| e.to_string())
}

/// All versions of a project, newest first. Cached for 5 minutes.
#[tauri::command]
pub async fn project_versions(
    state: State<'_, AppState>,
    id: String,
) -> Result<Vec<Version>, String> {
    let token = modrinth_token(&state);
    state
        .modrinth
        .project_versions_as(&id, &[], &[], token.as_deref(), Some(CachePolicy::SHORT))
        .await
        .map_err(|e| e.to_string())
}

/// Opens a web link in the user's browser. Only http(s), since links come
/// from project descriptions written by anyone.
#[tauri::command]
pub fn open_external(app: AppHandle, url: String) -> Result<(), String> {
    let parsed = tauri::Url::parse(&url).map_err(|_| "not a valid link".to_owned())?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return Err("only web links can be opened".into());
    }
    app.opener()
        .open_url(parsed.as_str(), None::<&str>)
        .map_err(|e| e.to_string())
}
