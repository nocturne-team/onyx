use onyx_core::cache::CachePolicy;
use onyx_core::modrinth::{SearchParams, SearchResults};
use onyx_core::{Settings, config, paths};
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::AppState;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    version: &'static str,
    os: String,
}

#[tauri::command]
pub fn app_info() -> AppInfo {
    let os = os_info::get();
    AppInfo {
        version: config::VERSION,
        os: format!("{} {}", os.os_type(), os.version()),
    }
}

#[tauri::command]
pub fn get_settings(state: State<'_, AppState>) -> Settings {
    state.settings.lock().unwrap().clone()
}

#[tauri::command]
pub fn set_settings(state: State<'_, AppState>, settings: Settings) -> Result<(), String> {
    let path = paths::settings_file().ok_or("could not find a data directory")?;
    settings.save(&path).map_err(|e| e.to_string())?;
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
