//! Offline accounts: a name and a locally derived UUID, no sign-in. They work
//! for singleplayer, LAN and servers in offline mode only.

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use md5::{Digest, Md5};
use serde::Deserialize;

use super::microsoft::MinecraftAccount;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum OfflineError {
    #[error("Names must be 3 to 16 characters: letters, digits and underscores")]
    InvalidName,
    #[error("Sign in with a Microsoft account before adding an offline one")]
    NeedsMicrosoft,
    #[error("An account with that name already exists")]
    Duplicate,
}

pub fn validate_name(name: &str) -> Result<(), OfflineError> {
    let ok = (3..=16).contains(&name.len())
        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
    if ok {
        Ok(())
    } else {
        Err(OfflineError::InvalidName)
    }
}

/// The UUID vanilla servers in offline mode give a player: a version 3 UUID
/// of `OfflinePlayer:<name>`, returned without dashes.
pub fn uuid(name: &str) -> String {
    let mut bytes: [u8; 16] = Md5::digest(format!("OfflinePlayer:{name}").as_bytes()).into();
    bytes[6] = (bytes[6] & 0x0f) | 0x30;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// The default skins in the order the game indexes them (1.19.3+): slim
/// variants first, then wide.
const DEFAULT_SKINS: [&str; 18] = [
    "slim/alex",
    "slim/ari",
    "slim/efe",
    "slim/kai",
    "slim/makena",
    "slim/noor",
    "slim/steve",
    "slim/sunny",
    "slim/zuri",
    "wide/alex",
    "wide/ari",
    "wide/efe",
    "wide/kai",
    "wide/makena",
    "wide/noor",
    "wide/steve",
    "wide/sunny",
    "wide/zuri",
];

/// Prefix of `skin_url` values that name one of the bundled default skins
/// (`ui/src/assets/default-skins/`) instead of a URL.
pub const DEFAULT_SKIN_PREFIX: &str = "default:";

/// The skin the game shows for this UUID when it has none:
/// `floorMod(UUID.hashCode(), 18)`, where Java's `UUID.hashCode()` folds
/// `msb ^ lsb` into 32 bits. Returned as `default:<model>/<name>`.
pub fn default_skin(uuid: &str) -> Option<String> {
    let value = u128::from_str_radix(uuid, 16).ok()?;
    let folded = (value >> 64) as u64 ^ value as u64;
    let hash = (folded >> 32) as u32 ^ folded as u32;
    let index = (hash as i32).rem_euclid(DEFAULT_SKINS.len() as i32) as usize;
    Some(format!("{DEFAULT_SKIN_PREFIX}{}", DEFAULT_SKINS[index]))
}

#[derive(Deserialize)]
struct IdLookup {
    id: String,
}

#[derive(Deserialize)]
struct SessionProfile {
    #[serde(default)]
    properties: Vec<Property>,
}

#[derive(Deserialize)]
struct Property {
    name: String,
    value: String,
}

#[derive(Deserialize)]
struct Textures {
    textures: TextureSet,
}

#[derive(Deserialize)]
struct TextureSet {
    #[serde(rename = "SKIN")]
    skin: Option<Skin>,
}

#[derive(Deserialize)]
struct Skin {
    url: String,
}

fn skin_from_profile(profile: &SessionProfile) -> Option<String> {
    let property = profile.properties.iter().find(|p| p.name == "textures")?;
    let bytes = STANDARD.decode(&property.value).ok()?;
    let textures: Textures = serde_json::from_slice(&bytes).ok()?;
    let url = textures.textures.skin?.url;
    Some(url.replacen("http://", "https://", 1))
}

/// The skin of the real Minecraft account with this name, if there is one.
/// Any failure (no such name, offline, rate limited) is "none".
pub async fn lookup_skin(http: &reqwest::Client, name: &str) -> Option<String> {
    let ask = async |url: String| -> Option<Vec<u8>> {
        let response = http.get(url).send().await.ok()?;
        if !response.status().is_success() {
            return None;
        }
        response.bytes().await.ok().map(|b| b.to_vec())
    };
    let found = ask(format!(
        "https://api.mojang.com/users/profiles/minecraft/{name}"
    ))
    .await?;
    let IdLookup { id } = serde_json::from_slice(&found).ok()?;
    if id.len() != 32 || !id.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let profile = ask(format!(
        "https://sessionserver.mojang.com/session/minecraft/profile/{id}"
    ))
    .await?;
    skin_from_profile(&serde_json::from_slice(&profile).ok()?)
}

/// `skin_url` is the real skin of that name when `lookup_skin` found one.
pub fn account(name: &str, real_skin: Option<String>) -> Result<MinecraftAccount, OfflineError> {
    validate_name(name)?;
    let id = uuid(name);
    Ok(MinecraftAccount {
        skin_url: real_skin.or_else(|| default_skin(&id)),
        id,
        name: name.to_owned(),
        offline: true,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uuid_matches_vanilla() {
        // java.util.UUID.nameUUIDFromBytes("OfflinePlayer:Notch")
        assert_eq!(uuid("Notch"), "b50ad385829d3141a2167e7d7539ba7f");
    }

    #[test]
    fn default_skin_follows_java_hash() {
        // Zero UUID hashes to 0 -> slim/alex.
        assert_eq!(
            default_skin("00000000000000000000000000000000").as_deref(),
            Some("default:slim/alex")
        );
        // msb ^ lsb = 0x00000000_00000013 -> hash 19 -> 19 mod 18 = 1.
        assert_eq!(
            default_skin("00000000000000000000000000000013").as_deref(),
            Some("default:slim/ari")
        );
        // Hash 0xffffffff is -1 as a Java int: floorMod(-1, 18) = 17 -> wide/zuri.
        assert_eq!(
            default_skin("000000000000000000000000ffffffff").as_deref(),
            Some("default:wide/zuri")
        );
        assert_eq!(default_skin("not hex"), None);
    }

    #[test]
    fn reads_skin_from_session_profile() {
        let textures =
            r#"{"textures":{"SKIN":{"url":"http://textures.minecraft.net/texture/abc"}}}"#;
        let profile = SessionProfile {
            properties: vec![Property {
                name: "textures".into(),
                value: STANDARD.encode(textures),
            }],
        };
        assert_eq!(
            skin_from_profile(&profile).as_deref(),
            Some("https://textures.minecraft.net/texture/abc")
        );
        let none = SessionProfile { properties: vec![] };
        assert_eq!(skin_from_profile(&none), None);
    }

    #[test]
    fn names() {
        assert!(validate_name("Steve_01").is_ok());
        for bad in ["ab", "seventeen_chars_xx", "has space", "né_name", ""] {
            assert_eq!(validate_name(bad), Err(OfflineError::InvalidName), "{bad}");
        }
    }
}
