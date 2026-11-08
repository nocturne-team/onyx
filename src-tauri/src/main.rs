// Hide the console window on Windows release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod account;
mod commands;
mod content;
mod icons;
mod instances;
mod minecraft_account;
mod presence;
mod prism;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use account::Session;
use onyx_core::auth::Accounts;
use onyx_core::auth::microsoft::MinecraftSession;
use onyx_core::cache::ResponseCache;
use onyx_core::discord::Presence;
use onyx_core::icons::IconCache;
use onyx_core::minecraft::MetaDirs;
use onyx_core::{Settings, modrinth, paths};
use tokio::sync::oneshot;
use tracing_subscriber::EnvFilter;

pub struct Paths {
    pub meta: MetaDirs,
    pub instances: PathBuf,
    pub accounts: PathBuf,
}

pub struct AppState {
    pub settings: Mutex<Settings>,
    pub paths: Paths,
    pub modrinth: modrinth::Client,
    pub session: Mutex<Option<Session>>,
    /// Cancels the Modrinth login that's waiting for its browser redirect.
    pub login_cancel: Mutex<Option<oneshot::Sender<()>>>,
    pub accounts: Mutex<Accounts>,
    /// Live Minecraft tokens by account ID. Never written to disk.
    pub minecraft_sessions: Mutex<HashMap<String, MinecraftSession>>,
    pub minecraft_login_cancel: Mutex<Option<oneshot::Sender<()>>>,
    /// Instances that are preparing or running.
    pub games: instances::Games,
    /// Log of each instance's last finished run.
    pub last_logs: Mutex<HashMap<String, Vec<String>>>,
    pub presence: Presence,
}

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let settings = paths::settings_file()
        .map(|path| {
            Settings::load(&path).unwrap_or_else(|e| {
                tracing::warn!("using default settings: {e}");
                Settings::default()
            })
        })
        .unwrap_or_default();
    settings.apply_download_limits();
    let (presence, presence_task) = Presence::new(
        onyx_core::config::DISCORD_APP_ID,
        settings.discord_rich_presence,
    );
    tauri::async_runtime::spawn(presence_task);

    let data_dir = paths::data_dir().expect("no home directory to keep Onyx's data in");
    let paths = Paths {
        meta: MetaDirs::new(paths::meta_dir().expect("data dir exists")),
        instances: data_dir.join("instances"),
        accounts: data_dir.join("accounts.json"),
    };
    let accounts = Accounts::load(&paths.accounts);

    let modrinth = modrinth::Client::new(ResponseCache::new(
        paths::cache_dir().map(|dir| dir.join("api")),
    ));
    let icons = Arc::new(IconCache::new(
        paths::cache_dir().map(|dir| dir.join("icons")),
        modrinth.http().clone(),
    ));

    tauri::Builder::default()
        .register_asynchronous_uri_scheme_protocol("onyx-icon", icons::protocol(icons))
        .manage(AppState {
            settings: Mutex::new(settings),
            paths,
            modrinth,
            session: Mutex::new(None),
            login_cancel: Mutex::new(None),
            accounts: Mutex::new(accounts),
            minecraft_sessions: Mutex::new(HashMap::new()),
            minecraft_login_cancel: Mutex::new(None),
            games: Mutex::new(HashMap::new()),
            last_logs: Mutex::new(HashMap::new()),
            presence,
        })
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            commands::app_info,
            commands::get_settings,
            commands::set_settings,
            commands::search,
            commands::categories,
            commands::project_page,
            commands::project_versions,
            commands::open_external,
            account::restore_session,
            account::login_modrinth,
            account::cancel_login,
            account::login_with_token,
            account::logout,
            account::my_projects,
            minecraft_account::minecraft_accounts,
            minecraft_account::login_minecraft,
            minecraft_account::cancel_minecraft_login,
            minecraft_account::set_active_minecraft_account,
            minecraft_account::remove_minecraft_account,
            instances::list_instances,
            instances::create_instance,
            instances::set_instance_icon,
            instances::set_instance_memory,
            instances::set_instance_java,
            instances::detect_java,
            instances::required_java,
            instances::probe_java,
            instances::set_instance_icon_config,
            instances::instance_icon,
            instances::image_preview,
            instances::loader_game_versions,
            instances::loader_versions,
            instances::list_content,
            instances::set_content_enabled,
            instances::remove_content,
            instances::open_instance_subfolder,
            instances::delete_instance,
            instances::open_instance_folder,
            instances::game_versions,
            instances::running_games,
            instances::game_log,
            instances::stop_game,
            instances::launch_instance,
            content::install_to_instance,
            content::identify_content,
            content::cached_content_info,
            content::check_updates,
            content::apply_update,
            content::import_mrpack,
            prism::prism_roots,
            prism::prism_instances,
            prism::import_prism_instances,
            content::install_modpack,
            content::check_pack_update,
            content::update_modpack,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Onyx");
}
