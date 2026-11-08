//! Signing in to Modrinth (OAuth or a pasted personal access token) and to
//! Minecraft (Microsoft OAuth), keeping tokens in the OS keyring.

mod accounts;
mod loopback;
pub mod microsoft;
mod oauth;
pub mod offline;
mod store;

pub use accounts::Accounts;
pub use oauth::{OAuthConfig, login};
pub use store::{
    delete_minecraft_refresh_token, delete_modrinth_token, load_minecraft_refresh_token,
    load_modrinth_token, save_minecraft_refresh_token, save_modrinth_token,
};

use serde::{Deserialize, Serialize};

/// Read-only on purpose: the client secret ships inside the binary, so a
/// leaked secret should not be able to change anything on anyone's account.
pub const MODRINTH_SCOPES: &[&str] = &[
    "USER_READ",
    "PROJECT_READ",
    "VERSION_READ",
    "COLLECTION_READ",
    "ORGANIZATION_READ",
    "NOTIFICATION_READ",
];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModrinthToken {
    pub access_token: String,
    /// Unix seconds. `None` for personal access tokens, whose expiry we don't know.
    pub expires_at: Option<u64>,
}

impl ModrinthToken {
    pub fn personal(access_token: String) -> Self {
        Self {
            access_token: access_token.trim().to_owned(),
            expires_at: None,
        }
    }

    /// Treats tokens within a minute of expiring as already expired.
    pub fn is_expired(&self, now: u64) -> bool {
        self.expires_at.is_some_and(|at| now + 60 >= at)
    }

    pub fn is_expired_now(&self) -> bool {
        self.is_expired(now_secs())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum AuthError {
    #[error("this build has no Modrinth OAuth client configured")]
    NotConfigured,
    #[error(
        "could not listen for the login redirect on ports {0:?}; close whatever is using them and try again"
    )]
    PortsInUse(Vec<u16>),
    #[error("could not open the browser: {0}")]
    Browser(String),
    #[error("the login was refused: {0}")]
    Denied(String),
    #[error("login redirect didn't match this login attempt")]
    StateMismatch,
    #[error("login timed out")]
    TimedOut,
    #[error("login cancelled")]
    Cancelled,
    #[error("login redirect failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("could not reach Modrinth: {0}")]
    Http(#[from] reqwest::Error),
    #[error("Modrinth rejected the login code ({status}): {body}")]
    TokenExchange { status: u16, body: String },
    #[error("could not access the system keyring: {0}")]
    Keyring(String),
}

/// Current Unix time in seconds.
pub fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn personal_tokens_never_expire_and_are_trimmed() {
        let token = ModrinthToken::personal("  mrp_abc \n".into());
        assert_eq!(token.access_token, "mrp_abc");
        assert!(!token.is_expired(u64::MAX - 100));
    }

    #[test]
    fn tokens_expire_a_minute_early() {
        let token = ModrinthToken {
            access_token: "x".into(),
            expires_at: Some(1000),
        };
        assert!(!token.is_expired(900));
        assert!(token.is_expired(950));
    }
}
