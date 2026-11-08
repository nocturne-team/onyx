//! Join links (`onyx://join/...`), Discord joins and hosting through e4mc.
//!
//! Links arrive from the OS (deep-link plugin, also forwarded from a second
//! launch by the single-instance plugin) or from Discord as a join secret.
//! Either way the invite is parked in `AppState.pending_invite` and the
//! frontend is told to come and get it, so one sent before the UI loaded
//! isn't lost. Nothing launches without the user picking an instance.

use std::hash::{DefaultHasher, Hash, Hasher};

use onyx_core::discord::{Event, Party, Secrets};
use onyx_core::instance::{self, Instance};
use onyx_core::invite::{self, Fit, Invite, LogSignal, PackRef};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};
use tokio::sync::mpsc;

use crate::AppState;

const RECEIVED_EVENT: &str = "invite:received";
const SHARE_EVENT: &str = "instance:share";
const JOIN_REQUEST_EVENT: &str = "discord:join-request";
/// LAN worlds take 8 players unless a mod changes it.
const LAN_MAX_PLAYERS: u32 = 8;

#[derive(Debug, Clone, Default)]
pub struct Pending {
    pub invite: Option<Invite>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Candidate {
    pub id: String,
    pub fit: Fit,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InviteOffer {
    pub invite: Option<Invite>,
    pub error: Option<String>,
    /// Instances that could join, best first.
    pub candidates: Vec<Candidate>,
}

/// An instance's public address while it hosts a LAN world through e4mc.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Share {
    pub id: String,
    pub address: Option<String>,
    pub link: Option<String>,
    pub players: u32,
    /// Open to LAN; without `address` that means e4mc isn't installed.
    pub lan_open: bool,
}

/// Handles a link from the OS or a Discord join secret.
pub fn receive_link(app: &AppHandle, link: &str) {
    focus(app);
    // Discord launches us through `discord-<app id>://` before it sends the
    // join event; that link carries nothing.
    if link.starts_with("discord-") {
        return;
    }
    let pending = match Invite::parse(link) {
        Ok(invite) => {
            tracing::info!("invite to {} received", invite.address);
            Pending {
                invite: Some(invite),
                error: None,
            }
        }
        Err(e) => {
            tracing::warn!("ignoring invite link: {e}");
            Pending {
                invite: None,
                error: Some(format!("This invite link doesn't work: {e}.")),
            }
        }
    };
    *app.state::<AppState>().pending_invite.lock().unwrap() = Some(pending);
    let _ = app.emit(RECEIVED_EVENT, ());
}

fn focus(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}

/// Registers Discord's `discord-<app id>://` with the OS, which Discord opens
/// to start us when a friend's join is clicked. `onyx://` comes from the
/// installer (NSIS, the .desktop file), so it's only registered here for dev
/// builds and AppImages, where nothing else does it.
pub fn register_schemes(app: &AppHandle) {
    #[cfg(any(windows, target_os = "linux"))]
    {
        use tauri_plugin_deep_link::DeepLinkExt;
        let own = cfg!(debug_assertions) || app.env().appimage.is_some();
        let app = app.clone();
        std::thread::spawn(move || {
            let onyx = own.then(|| invite::SCHEME.to_owned());
            let discord = onyx_core::config::DISCORD_APP_ID.map(|id| format!("discord-{id}"));
            for scheme in onyx.into_iter().chain(discord) {
                let link = app.deep_link();
                if link.is_registered(&scheme).unwrap_or(false) {
                    continue;
                }
                if let Err(e) = link.register(&scheme) {
                    tracing::warn!("could not register {scheme}:// links: {e}");
                }
            }
            #[cfg(target_os = "linux")]
            unquote_handler_exec(&app);
        });
    }
    #[cfg(not(any(windows, target_os = "linux")))]
    let _ = app;
}

/// The deep-link plugin writes `Exec="<path>" %u`. xdg-open's generic mode
/// (desktops it doesn't know, e.g. niri or sway) runs `command -v` on that
/// quoted string, finds nothing and falls back to a browser. Drop the quotes
/// when the path doesn't need them.
#[cfg(target_os = "linux")]
fn unquote_handler_exec(app: &AppHandle) {
    let (Ok(exe), Ok(data)) = (tauri::utils::platform::current_exe(), app.path().data_dir()) else {
        return;
    };
    let Some(name) = exe.file_name() else {
        return;
    };
    let file = data
        .join("applications")
        .join(format!("{}-handler.desktop", name.to_string_lossy()));
    let Ok(text) = std::fs::read_to_string(&file) else {
        return;
    };
    let fixed: Vec<String> = text
        .lines()
        .map(|line| {
            let unquoted = line
                .strip_prefix("Exec=\"")
                .and_then(|rest| rest.strip_suffix("\" %u"))
                .filter(|path| {
                    path.chars()
                        .all(|c| c.is_ascii_alphanumeric() || "/._-+".contains(c))
                });
            match unquoted {
                Some(path) => format!("Exec={path} %u"),
                None => line.to_owned(),
            }
        })
        .collect();
    let fixed = fixed.join("\n") + "\n";
    if fixed != text
        && let Err(e) = std::fs::write(&file, fixed)
    {
        tracing::warn!("could not fix {}: {e}", file.display());
    }
}

/// Forwards what Discord sends: joins become pending invites, join requests
/// go to the frontend to accept or decline.
pub async fn forward_discord_events(app: AppHandle, mut events: mpsc::UnboundedReceiver<Event>) {
    while let Some(event) = events.recv().await {
        match event {
            Event::Join(secret) => receive_link(&app, &secret),
            Event::JoinRequest(request) => {
                let _ = app.emit(JOIN_REQUEST_EVENT, request);
            }
        }
    }
}

#[tauri::command]
pub fn take_invite(state: State<'_, AppState>) -> Option<InviteOffer> {
    let pending = state.pending_invite.lock().unwrap().take()?;
    let candidates = match &pending.invite {
        Some(invite) => {
            let list = instance::list(&state.paths.instances).unwrap_or_default();
            invite::rank(&list, invite)
                .into_iter()
                .map(|(instance, fit)| Candidate {
                    id: instance.id.clone(),
                    fit,
                })
                .collect()
        }
        None => Vec::new(),
    };
    Some(InviteOffer {
        invite: pending.invite,
        error: pending.error,
        candidates,
    })
}

/// Starts `id` and has it join `address` once loaded.
#[tauri::command]
pub async fn join_invite(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
    address: String,
) -> Result<(), String> {
    if !invite::valid_address(&address) {
        return Err("invalid server address".into());
    }
    crate::instances::launch(&app, &state, id, Some(address)).await
}

#[tauri::command]
pub fn answer_join_request(state: State<'_, AppState>, user_id: String, accept: bool) {
    state.presence.answer_join_request(user_id, accept);
}

#[tauri::command]
pub fn game_share(state: State<'_, AppState>, id: String) -> Option<Share> {
    share(&state, &id)
}

fn share(state: &AppState, id: &str) -> Option<Share> {
    let (address, players, lan_open) = {
        let games = state.games.lock().unwrap();
        let game = games.get(id)?;
        (game.share.clone(), game.players, game.lan_open)
    };
    let link = match &address {
        Some(address) => instance::load(&state.paths.instances, id)
            .ok()
            .map(|i| invite_for(&i, address).to_url()),
        None => None,
    };
    Some(Share {
        id: id.to_owned(),
        address,
        link,
        players,
        lan_open,
    })
}

pub fn invite_for(instance: &Instance, address: &str) -> Invite {
    Invite {
        address: address.to_owned(),
        game_version: instance.game_version.clone(),
        loader: instance.loader,
        pack: instance.pack.as_ref().map(|p| PackRef {
            project_id: p.project_id.clone(),
            version_id: Some(p.version_id.clone()),
        }),
    }
}

/// The party and join secret for Discord while `instance` hosts.
pub fn discord_party(instance: &Instance, address: &str, players: u32) -> (Party, Secrets) {
    // The party id goes to everyone who sees the activity; the address only
    // to people who join.
    let mut hasher = DefaultHasher::new();
    address.hash(&mut hasher);
    let party = Party {
        id: format!("onyx-{:016x}", hasher.finish()),
        size: Some([players.clamp(1, LAN_MAX_PLAYERS), LAN_MAX_PLAYERS]),
    };
    let secrets = Secrets {
        join: invite_for(instance, address).to_secret(),
    };
    (party, secrets)
}

/// Called for every game log line: follows e4mc's address and the player
/// count of the integrated server.
pub fn watch_log_line(app: &AppHandle, id: &str, line: &str) {
    let Some(signal) = invite::log_signal(line) else {
        return;
    };
    let state = app.state::<AppState>();
    let changed = {
        let mut games = state.games.lock().unwrap();
        let Some(game) = games.get_mut(id) else {
            return;
        };
        let before = (game.share.clone(), game.players, game.lan_open);
        match signal {
            LogSignal::Domain(domain) => {
                game.share = Some(domain);
                game.lan_open = true;
            }
            LogSignal::LanOpened => game.lan_open = true,
            LogSignal::PlayerJoined => game.players += 1,
            LogSignal::PlayerLeft => game.players = game.players.saturating_sub(1),
            LogSignal::ServerStopped => {
                game.share = None;
                game.players = 0;
                game.lan_open = false;
            }
        }
        // The count only matters to anyone while the world is shared.
        before.0 != game.share
            || before.2 != game.lan_open
            || (game.share.is_some() && before.1 != game.players)
    };
    if changed {
        if let Some(share) = share(&state, id) {
            let _ = app.emit(SHARE_EVENT, share);
        }
        crate::presence::refresh(app);
    }
}
