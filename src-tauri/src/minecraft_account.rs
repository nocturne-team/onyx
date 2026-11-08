//! Minecraft (Microsoft) account commands. Account names and skins are in
//! `accounts.json`, refresh tokens in the keyring, live tokens only in memory.

use onyx_core::auth::microsoft::{self, MinecraftAccount, MinecraftSession, MsaConfig, MsaError};
use onyx_core::auth::offline::{self, OfflineError};
use onyx_core::auth::{self, Accounts};
use tauri::{AppHandle, State};
use tauri_plugin_opener::OpenerExt;
use tokio::sync::oneshot;

use crate::AppState;
use crate::account::keyring;

fn save_accounts(state: &AppState, accounts: &Accounts) -> Result<(), String> {
    accounts
        .save(&state.paths.accounts)
        .map_err(|e| format!("could not save accounts: {e}"))
}

#[tauri::command]
pub fn minecraft_accounts(state: State<'_, AppState>) -> Accounts {
    state.accounts.lock().unwrap().clone()
}

/// Signs in with Microsoft in the browser, adds the account and makes it active.
#[tauri::command]
pub async fn login_minecraft(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<MinecraftAccount, String> {
    let config = MsaConfig::from_build().ok_or_else(|| MsaError::NotConfigured.to_string())?;
    let (cancel_tx, cancel_rx) = oneshot::channel::<()>();
    // Dropping the previous sender cancels the previous login.
    *state.minecraft_login_cancel.lock().unwrap() = Some(cancel_tx);

    let session = microsoft::login(
        &config,
        state.modrinth.http(),
        |url| {
            app.opener()
                .open_url(url, None::<&str>)
                .map_err(|e| e.to_string())
        },
        async {
            let _ = cancel_rx.await;
        },
    )
    .await
    .map_err(|e| e.to_string())?;

    remember(&state, session).await
}

/// Adds a local offline account and makes it active. Needs at least one
/// Microsoft account to be saved already. The head shown is the real skin of
/// that name if one exists (the game itself will still show its default skin).
#[tauri::command]
pub async fn add_offline_account(
    state: State<'_, AppState>,
    name: String,
) -> Result<MinecraftAccount, String> {
    let name = name.trim();
    offline::validate_name(name).map_err(|e| e.to_string())?;
    if !state.accounts.lock().unwrap().has_microsoft() {
        return Err(OfflineError::NeedsMicrosoft.to_string());
    }
    // The skin of a real account with this name, else the game's default one.
    let real_skin = offline::lookup_skin(state.modrinth.http(), name).await;
    let account = offline::account(name, real_skin).map_err(|e| e.to_string())?;
    let mut accounts = state.accounts.lock().unwrap();
    if !accounts.has_microsoft() {
        return Err(OfflineError::NeedsMicrosoft.to_string());
    }
    if accounts.accounts.iter().any(|a| a.id == account.id) {
        return Err(OfflineError::Duplicate.to_string());
    }
    accounts.upsert(account.clone());
    save_accounts(&state, &accounts)?;
    Ok(account)
}

#[tauri::command]
pub fn cancel_minecraft_login(state: State<'_, AppState>) {
    if let Some(cancel) = state.minecraft_login_cancel.lock().unwrap().take() {
        let _ = cancel.send(());
    }
}

#[tauri::command]
pub fn set_active_minecraft_account(state: State<'_, AppState>, id: String) -> Result<(), String> {
    let mut accounts = state.accounts.lock().unwrap();
    if !accounts.accounts.iter().any(|a| a.id == id) {
        return Err("no such account".into());
    }
    accounts.active = Some(id);
    save_accounts(&state, &accounts)
}

#[tauri::command]
pub async fn remove_minecraft_account(
    state: State<'_, AppState>,
    id: String,
) -> Result<(), String> {
    state.minecraft_sessions.lock().unwrap().remove(&id);
    {
        let mut accounts = state.accounts.lock().unwrap();
        accounts.remove(&id);
        save_accounts(&state, &accounts)?;
    }
    keyring(move || auth::delete_minecraft_refresh_token(&id)).await
}

/// Saves a newly signed-in account and makes it active.
async fn remember(state: &AppState, session: MinecraftSession) -> Result<MinecraftAccount, String> {
    let id = session.account.id.clone();
    let refresh_token = session.refresh_token.clone();
    keyring(move || auth::save_minecraft_refresh_token(&id, &refresh_token)).await?;

    let account = session.account.clone();
    {
        let mut accounts = state.accounts.lock().unwrap();
        accounts.upsert(account.clone());
        save_accounts(state, &accounts)?;
    }
    state
        .minecraft_sessions
        .lock()
        .unwrap()
        .insert(account.id.clone(), session);
    Ok(account)
}

/// Live tokens for the active account, refreshing them if needed.
pub async fn active_session(state: &AppState) -> Result<MinecraftSession, String> {
    let account = state
        .accounts
        .lock()
        .unwrap()
        .active_account()
        .cloned()
        .ok_or("Sign in to Minecraft first")?;

    if account.offline {
        return Ok(MinecraftSession {
            account,
            xuid: "0".into(),
            access_token: "0".into(),
            expires_at: u64::MAX,
            refresh_token: String::new(),
        });
    }

    let cached = state
        .minecraft_sessions
        .lock()
        .unwrap()
        .get(&account.id)
        .cloned();
    if let Some(session) = cached.filter(|s| !s.is_expired_now()) {
        return Ok(session);
    }

    let config = MsaConfig::from_build().ok_or_else(|| MsaError::NotConfigured.to_string())?;
    let id = account.id.clone();
    let refresh_token = keyring(move || auth::load_minecraft_refresh_token(&id))
        .await?
        .ok_or_else(|| sign_in_again(&account))?;
    let session = match microsoft::refresh(&config, state.modrinth.http(), &refresh_token).await {
        Ok(session) => session,
        Err(MsaError::SignInAgain) => return Err(sign_in_again(&account)),
        Err(e) => return Err(e.to_string()),
    };
    if session.account.id != account.id {
        return Err(sign_in_again(&account));
    }
    remember_refreshed(state, session).await
}

/// Saves the rotated refresh token and any name or skin change, and caches
/// the live tokens.
async fn remember_refreshed(
    state: &AppState,
    session: MinecraftSession,
) -> Result<MinecraftSession, String> {
    let id = session.account.id.clone();
    let refresh_token = session.refresh_token.clone();
    keyring(move || auth::save_minecraft_refresh_token(&id, &refresh_token)).await?;
    {
        let mut accounts = state.accounts.lock().unwrap();
        if let Some(existing) = accounts
            .accounts
            .iter_mut()
            .find(|a| a.id == session.account.id)
        {
            *existing = session.account.clone();
        }
        save_accounts(state, &accounts)?;
    }
    state
        .minecraft_sessions
        .lock()
        .unwrap()
        .insert(session.account.id.clone(), session.clone());
    Ok(session)
}

fn sign_in_again(account: &MinecraftAccount) -> String {
    format!(
        "The Microsoft login for {} expired. Sign in again.",
        account.name
    )
}
