use super::{AuthError, ModrinthToken};

const SERVICE: &str = "onyx";
const MODRINTH_ACCOUNT: &str = "modrinth";

fn entry() -> Result<keyring::Entry, AuthError> {
    keyring::Entry::new(SERVICE, MODRINTH_ACCOUNT).map_err(|e| AuthError::Keyring(e.to_string()))
}

pub fn load_modrinth_token() -> Result<Option<ModrinthToken>, AuthError> {
    match entry()?.get_password() {
        Ok(json) => match serde_json::from_str(&json) {
            Ok(token) => Ok(Some(token)),
            Err(e) => {
                tracing::warn!("discarding unreadable Modrinth token from keyring: {e}");
                delete_modrinth_token()?;
                Ok(None)
            }
        },
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(AuthError::Keyring(e.to_string())),
    }
}

pub fn save_modrinth_token(token: &ModrinthToken) -> Result<(), AuthError> {
    let json = serde_json::to_string(token).expect("token always serializes");
    entry()?
        .set_password(&json)
        .map_err(|e| AuthError::Keyring(e.to_string()))
}

pub fn delete_modrinth_token() -> Result<(), AuthError> {
    match entry()?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(AuthError::Keyring(e.to_string())),
    }
}

fn minecraft_entry(account_id: &str) -> Result<keyring::Entry, AuthError> {
    keyring::Entry::new(SERVICE, &format!("minecraft:{account_id}"))
        .map_err(|e| AuthError::Keyring(e.to_string()))
}

/// The account's Microsoft refresh token. Stored as raw bytes: Windows caps
/// passwords at 2560 bytes of UTF-16, which long refresh tokens can exceed.
pub fn load_minecraft_refresh_token(account_id: &str) -> Result<Option<String>, AuthError> {
    match minecraft_entry(account_id)?.get_secret() {
        Ok(bytes) => Ok(String::from_utf8(bytes).ok()),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(AuthError::Keyring(e.to_string())),
    }
}

pub fn save_minecraft_refresh_token(account_id: &str, token: &str) -> Result<(), AuthError> {
    minecraft_entry(account_id)?
        .set_secret(token.as_bytes())
        .map_err(|e| AuthError::Keyring(e.to_string()))
}

pub fn delete_minecraft_refresh_token(account_id: &str) -> Result<(), AuthError> {
    match minecraft_entry(account_id)?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(AuthError::Keyring(e.to_string())),
    }
}
