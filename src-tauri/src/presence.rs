//! Keeps Discord rich presence in line with the running instances: the most
//! recently started one is shown, nothing when no game is running.

use onyx_core::cache::CachePolicy;
use onyx_core::discord::{self, Activity, Assets, Timestamps};
use onyx_core::instance::{self, Instance, Loader};
use tauri::{AppHandle, Manager};

use crate::AppState;
use crate::instances::Phase;

/// Art asset of the app icon in the Discord app.
const APP_ICON: &str = "icon";

/// Recomputes the activity. Call after a game starts or stops.
pub fn refresh(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let state = app.state::<AppState>();
        let Some(current) = current_game(&state) else {
            state.presence.set_activity(None);
            return;
        };
        let activity = match instance::load(&state.paths.instances, &current.0) {
            Ok(instance) => Some(activity(&state, &instance, current.1).await),
            Err(e) => {
                tracing::warn!("rich presence: could not load instance {}: {e}", current.0);
                None
            }
        };
        // Building the activity can wait on Modrinth. If the game stopped or
        // another started meanwhile, a newer refresh owns the result.
        if current_game(&state).as_ref() == Some(&current) {
            state.presence.set_activity(activity);
        }
    });
}

/// The running game that started last: its id and start time.
fn current_game(state: &AppState) -> Option<(String, u64)> {
    state
        .games
        .lock()
        .unwrap()
        .iter()
        .filter(|(_, game)| game.phase == Some(Phase::Running))
        .filter_map(|(id, game)| Some((id.clone(), game.running_since?)))
        .max_by_key(|(_, since)| *since)
}

async fn activity(state: &AppState, instance: &Instance, since: u64) -> Activity {
    let (large_image, large_text) = large_image(state, instance).await;
    let (small_image, small_text) = match loader_asset(instance.loader) {
        Some(key) => (
            Some(key.to_owned()),
            Some(discord::text(&match &instance.loader_version {
                Some(version) => format!("{} {version}", loader_name(instance.loader)),
                None => loader_name(instance.loader).to_owned(),
            })),
        ),
        // Vanilla: show the Onyx icon small, unless it's already the big one.
        None if large_image != APP_ICON => (Some(APP_ICON.to_owned()), Some("Onyx".to_owned())),
        None => (None, None),
    };
    Activity {
        details: Some(discord::text(&instance.name)),
        state: Some(discord::text(&format!(
            "{} {}",
            loader_name(instance.loader),
            instance.game_version
        ))),
        timestamps: Some(Timestamps { start: since }),
        assets: Some(Assets {
            large_image: Some(large_image),
            large_text: Some(discord::text(&large_text)),
            small_image,
            small_text,
        }),
    }
}

/// The modpack's icon from Modrinth, else the built-in instance icon (an art
/// asset with the symbol's name), else the app icon. Custom images are local
/// files Discord can't see, so they get the app icon too.
async fn large_image(state: &AppState, instance: &Instance) -> (String, String) {
    if let Some(pack) = &instance.pack {
        match state
            .modrinth
            .projects(&[pack.project_id.as_str()], Some(CachePolicy::LONG))
            .await
        {
            Ok(projects) => {
                if let Some(project) = projects.into_iter().next()
                    && let Some(url) = project.icon_url.filter(|u| u.starts_with("https://"))
                {
                    return (url, project.title);
                }
            }
            Err(e) => tracing::debug!("rich presence: no modpack icon: {e}"),
        }
    }
    let symbol = instance
        .icon_config
        .as_ref()
        .filter(|_| instance.icon.is_none())
        .map(|config| config.symbol.as_str())
        .filter(|s| {
            s.chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        });
    match symbol {
        Some(symbol) => (symbol.to_owned(), instance.name.clone()),
        None => (APP_ICON.to_owned(), instance.name.clone()),
    }
}

fn loader_asset(loader: Loader) -> Option<&'static str> {
    match loader {
        Loader::Vanilla => None,
        Loader::Fabric => Some("fabric"),
        Loader::Quilt => Some("quilt"),
        Loader::Forge => Some("forge"),
        Loader::NeoForge => Some("neoforge"),
    }
}

fn loader_name(loader: Loader) -> &'static str {
    match loader {
        Loader::Vanilla => "Minecraft",
        Loader::Fabric => "Fabric",
        Loader::Quilt => "Quilt",
        Loader::Forge => "Forge",
        Loader::NeoForge => "NeoForge",
    }
}
