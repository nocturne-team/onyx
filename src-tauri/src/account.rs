//! Modrinth sign-in commands. The session lives in `AppState`; the token also
//! lives in the OS keyring so it survives restarts.

use std::time::Duration;

use onyx_core::auth::{self, AuthError, ModrinthToken, OAuthConfig};
use onyx_core::cache::CachePolicy;
use onyx_core::modrinth::{ApiError, Project, User};
use tauri::{AppHandle, Emitter, State};
use tauri_plugin_opener::OpenerExt;
use tokio::sync::oneshot;

use crate::AppState;

/// Emitted when Modrinth rejects the saved token mid-session.
const SIGNED_OUT_EVENT: &str = "account:signed-out";

pub struct Session {
    pub token: ModrinthToken,
    pub user: User,
}

/// Keyring calls block on D-Bus (Linux) or Win32, so keep them off the async runtime.
pub async fn keyring<T: Send + 'static>(
    f: impl FnOnce() -> Result<T, AuthError> + Send + 'static,
) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(f)
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())
}

/// Loads the saved token, if any, and checks it still works.
#[tauri::command]
pub async fn restore_session(state: State<'_, AppState>) -> Result<Option<User>, String> {
    let Some(token) = keyring(auth::load_modrinth_token).await? else {
        return Ok(None);
    };
    if token.is_expired_now() {
        keyring(auth::delete_modrinth_token).await?;
        return Ok(None);
    }
    match state.modrinth.current_user(&token.access_token).await {
        Ok(user) => {
            *state.session.lock().unwrap() = Some(Session {
                token,
                user: user.clone(),
            });
            Ok(Some(user))
        }
        Err(ApiError::Unauthorized) => {
            keyring(auth::delete_modrinth_token).await?;
            Ok(None)
        }
        Err(e) => Err(e.to_string()),
    }
}

/// Runs the OAuth flow in the user's browser. Starting a new login cancels
/// any login still waiting for its redirect.
#[tauri::command]
pub async fn login_modrinth(app: AppHandle, state: State<'_, AppState>) -> Result<User, String> {
    let config = OAuthConfig::from_build().ok_or_else(|| AuthError::NotConfigured.to_string())?;
    let (cancel_tx, cancel_rx) = oneshot::channel::<()>();
    // Dropping the previous sender cancels the previous login.
    *state.login_cancel.lock().unwrap() = Some(cancel_tx);

    let token = auth::login(
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

    finish_login(&state, token).await
}

#[tauri::command]
pub fn cancel_login(state: State<'_, AppState>) {
    if let Some(cancel) = state.login_cancel.lock().unwrap().take() {
        let _ = cancel.send(());
    }
}

/// Signs in with a personal access token from modrinth.com/settings/pats.
#[tauri::command]
pub async fn login_with_token(state: State<'_, AppState>, token: String) -> Result<User, String> {
    let token = ModrinthToken::personal(token);
    if token.access_token.is_empty() {
        return Err("paste a token first".into());
    }
    finish_login(&state, token).await
}

async fn finish_login(state: &AppState, token: ModrinthToken) -> Result<User, String> {
    let user = state
        .modrinth
        .current_user(&token.access_token)
        .await
        .map_err(|e| match e {
            ApiError::Unauthorized => "Modrinth didn't accept that token".to_owned(),
            e => e.to_string(),
        })?;
    let saved = token.clone();
    keyring(move || auth::save_modrinth_token(&saved)).await?;
    *state.session.lock().unwrap() = Some(Session {
        token,
        user: user.clone(),
    });
    Ok(user)
}

#[tauri::command]
pub async fn logout(state: State<'_, AppState>) -> Result<(), String> {
    *state.session.lock().unwrap() = None;
    keyring(auth::delete_modrinth_token).await
}

/// All of the signed-in user's projects. `refresh` skips the 5 minute cache.
#[tauri::command]
pub async fn my_projects(
    app: AppHandle,
    state: State<'_, AppState>,
    refresh: bool,
) -> Result<Vec<Project>, String> {
    let (token, user_id) = {
        let session = state.session.lock().unwrap();
        let session = session.as_ref().ok_or("not signed in")?;
        (session.token.access_token.clone(), session.user.id.clone())
    };
    let policy = if refresh {
        // Always refetch, but keep the result for later non-refresh calls.
        CachePolicy {
            max_age: Duration::ZERO,
            persist: false,
        }
    } else {
        CachePolicy::SHORT
    };

    match state
        .modrinth
        .all_user_projects(&user_id, &token, Some(policy))
        .await
    {
        Ok(projects) => Ok(projects),
        Err(ApiError::Unauthorized) => {
            *state.session.lock().unwrap() = None;
            keyring(auth::delete_modrinth_token).await?;
            let _ = app.emit(SIGNED_OUT_EVENT, ());
            Err(ApiError::Unauthorized.to_string())
        }
        Err(e) => Err(e.to_string()),
    }
}
