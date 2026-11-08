//! The list of Minecraft accounts (`<data dir>/accounts.json`). Only names,
//! UUIDs and skins live here; refresh tokens are in the keyring.

use std::path::Path;

use serde::{Deserialize, Serialize};

use super::microsoft::MinecraftAccount;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Accounts {
    pub active: Option<String>,
    pub accounts: Vec<MinecraftAccount>,
}

impl Accounts {
    /// Missing or unreadable files mean "no accounts".
    pub fn load(path: &Path) -> Self {
        match std::fs::read(path) {
            Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_else(|e| {
                tracing::warn!("ignoring unreadable {}: {e}", path.display());
                Self::default()
            }),
            Err(_) => Self::default(),
        }
    }

    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let temp = path.with_extension("onyx-part");
        std::fs::write(&temp, serde_json::to_vec_pretty(self)?)?;
        std::fs::rename(&temp, path)
    }

    pub fn active_account(&self) -> Option<&MinecraftAccount> {
        let id = self.active.as_deref()?;
        self.accounts.iter().find(|a| a.id == id)
    }

    /// Adds or updates the account and makes it active.
    pub fn upsert(&mut self, account: MinecraftAccount) {
        self.active = Some(account.id.clone());
        match self.accounts.iter_mut().find(|a| a.id == account.id) {
            Some(existing) => *existing = account,
            None => self.accounts.push(account),
        }
    }

    /// Removes the account; if it was active, the first remaining one becomes active.
    pub fn remove(&mut self, id: &str) {
        self.accounts.retain(|a| a.id != id);
        if self.active.as_deref() == Some(id) {
            self.active = self.accounts.first().map(|a| a.id.clone());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn account(id: &str) -> MinecraftAccount {
        MinecraftAccount {
            id: id.into(),
            name: format!("name-{id}"),
            skin_url: None,
        }
    }

    #[test]
    fn upsert_remove_and_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("accounts.json");
        let mut accounts = Accounts::load(&path);
        assert_eq!(accounts, Accounts::default());

        accounts.upsert(account("a"));
        accounts.upsert(account("b"));
        let mut renamed = account("a");
        renamed.name = "renamed".into();
        accounts.upsert(renamed);
        assert_eq!(accounts.accounts.len(), 2);
        assert_eq!(accounts.active_account().unwrap().name, "renamed");

        accounts.remove("a");
        assert_eq!(accounts.active.as_deref(), Some("b"));
        accounts.save(&path).unwrap();
        assert_eq!(Accounts::load(&path), accounts);
    }
}
