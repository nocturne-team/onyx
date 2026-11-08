//! Invites to someone else's game: an `onyx://join/<address>?...` link, which
//! is also what Discord carries as the join secret.
//!
//! Hosting works through e4mc (modrinth.com/mod/e4mc): with the mod in the
//! instance, "Open to LAN" gets a public `*.e4mc.link` address, which the mod
//! logs. [`log_signal`] picks that out of the game log. Joining needs no mod;
//! e4mc relays plain Minecraft connections.

use reqwest::Url;

use crate::instance::{Instance, Loader};

pub const SCHEME: &str = "onyx";
/// The e4mc mod on Modrinth.
pub const E4MC_PROJECT_ID: &str = "qANg5Jrr";
/// Discord's limit for `secrets.join`.
const MAX_SECRET: usize = 128;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Invite {
    /// `host` or `host:port`.
    pub address: String,
    pub game_version: String,
    pub loader: Loader,
    /// The host's modpack, when their instance came from one.
    pub pack: Option<PackRef>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PackRef {
    pub project_id: String,
    pub version_id: Option<String>,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum InviteError {
    #[error("not an Onyx invite link")]
    NotAnInvite,
    #[error("the invite has an invalid server address")]
    BadAddress,
    #[error("the invite is missing the Minecraft version")]
    BadVersion,
}

impl Invite {
    pub fn to_url(&self) -> String {
        let mut url = Url::parse(&format!("{SCHEME}://join/{}", self.address))
            .expect("addresses are validated");
        {
            let mut query = url.query_pairs_mut();
            query.append_pair("mc", &self.game_version);
            query.append_pair("loader", loader_id(self.loader));
            if let Some(pack) = &self.pack {
                query.append_pair("pack", &pack.project_id);
                if let Some(version) = &pack.version_id {
                    query.append_pair("packVersion", version);
                }
            }
        }
        url.into()
    }

    /// The link, shortened to fit Discord's join secret if needed (the pack
    /// goes first, it's only a hint for picking an instance).
    pub fn to_secret(&self) -> String {
        let url = self.to_url();
        if url.len() <= MAX_SECRET {
            return url;
        }
        Self {
            pack: None,
            ..self.clone()
        }
        .to_url()
    }

    pub fn parse(link: &str) -> Result<Self, InviteError> {
        let url = Url::parse(link.trim()).map_err(|_| InviteError::NotAnInvite)?;
        if url.scheme() != SCHEME || url.host_str() != Some("join") {
            return Err(InviteError::NotAnInvite);
        }
        // From the raw link: the parsed path has `..` segments resolved.
        let raw = link.trim();
        let address = raw[raw.find("//join/").ok_or(InviteError::NotAnInvite)? + 7..]
            .split(['?', '#'])
            .next()
            .unwrap_or_default()
            .trim_end_matches('/');
        if !valid_address(address) {
            return Err(InviteError::BadAddress);
        }
        let (mut game_version, mut loader, mut pack_id, mut pack_version) =
            (None, Loader::Vanilla, None, None);
        for (key, value) in url.query_pairs() {
            match &*key {
                "mc" => game_version = Some(value.into_owned()),
                "loader" => loader = parse_loader(&value).unwrap_or(Loader::Vanilla),
                "pack" if valid_id(&value) => pack_id = Some(value.into_owned()),
                "packVersion" if valid_id(&value) => pack_version = Some(value.into_owned()),
                _ => {}
            }
        }
        let game_version = game_version
            .filter(|v| valid_game_version(v))
            .ok_or(InviteError::BadVersion)?;
        Ok(Self {
            address: address.to_ascii_lowercase(),
            game_version,
            loader,
            pack: pack_id.map(|project_id| PackRef {
                project_id,
                version_id: pack_version,
            }),
        })
    }

    /// `(host, port)` for `--server`/`--port` on versions without quick play.
    pub fn host_port(&self) -> (&str, u16) {
        match self.address.rsplit_once(':') {
            Some((host, port)) => (host, port.parse().unwrap_or(25565)),
            None => (&self.address, 25565),
        }
    }
}

fn loader_id(loader: Loader) -> &'static str {
    match loader {
        Loader::Vanilla => "vanilla",
        Loader::Fabric => "fabric",
        Loader::Quilt => "quilt",
        Loader::Forge => "forge",
        Loader::NeoForge => "neoforge",
    }
}

fn parse_loader(value: &str) -> Option<Loader> {
    serde_json::from_value(serde_json::Value::String(value.to_owned())).ok()
}

/// A hostname or IPv4 address, optionally with `:port`. Nothing that could
/// be read as an option or a path once it's a game argument.
pub fn valid_address(address: &str) -> bool {
    let (host, port) = match address.rsplit_once(':') {
        Some((host, port)) => (host, Some(port)),
        None => (address, None),
    };
    let port_ok = port.is_none_or(|p| p.parse::<u16>().is_ok_and(|p| p > 0));
    let labels_ok = host.split('.').all(|label| {
        !label.is_empty()
            && label.len() <= 63
            && !label.starts_with('-')
            && !label.ends_with('-')
            && label.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
    });
    port_ok && labels_ok && !host.is_empty() && host.len() <= 253
}

fn valid_id(value: &str) -> bool {
    !value.is_empty() && value.len() <= 32 && value.chars().all(|c| c.is_ascii_alphanumeric())
}

fn valid_game_version(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_' | '+' | ' '))
}

/// How well an instance fits an invite, best first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Fit {
    SamePackVersion,
    SamePack,
    SameVersionAndLoader,
    SameVersion,
}

/// Instances that could join, best fit first, then most recently played.
pub fn rank<'a>(instances: &'a [Instance], invite: &Invite) -> Vec<(&'a Instance, Fit)> {
    let mut fits: Vec<_> = instances
        .iter()
        .filter_map(|instance| fit(instance, invite).map(|f| (instance, f)))
        .collect();
    fits.sort_by(|(a, fa), (b, fb)| fa.cmp(fb).then(b.last_played.cmp(&a.last_played)));
    fits
}

fn fit(instance: &Instance, invite: &Invite) -> Option<Fit> {
    if let (Some(theirs), Some(ours)) = (&invite.pack, &instance.pack)
        && theirs.project_id == ours.project_id
    {
        return Some(
            if theirs.version_id.as_deref() == Some(ours.version_id.as_str()) {
                Fit::SamePackVersion
            } else {
                Fit::SamePack
            },
        );
    }
    if instance.game_version != invite.game_version {
        return None;
    }
    Some(if instance.loader == invite.loader {
        Fit::SameVersionAndLoader
    } else {
        Fit::SameVersion
    })
}

/// Something in the game log that matters for hosting.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LogSignal {
    /// e4mc's public address for the LAN world.
    Domain(String),
    /// The world was opened to LAN (with or without e4mc).
    LanOpened,
    PlayerJoined,
    PlayerLeft,
    /// The integrated server shut down (world closed).
    ServerStopped,
}

/// Reads one game log line. Chat is ignored so players can't fake signals.
pub fn log_signal(line: &str) -> Option<LogSignal> {
    if line.contains("[CHAT]") {
        return None;
    }
    if let Some((_, rest)) = line.split_once("Domain assigned: ") {
        let domain = rest.split_whitespace().next()?.to_ascii_lowercase();
        return (domain.ends_with(".e4mc.link") && valid_address(&domain))
            .then_some(LogSignal::Domain(domain));
    }
    if line.contains("Published LAN server on port") {
        return Some(LogSignal::LanOpened);
    }
    if !line.contains("[Server thread/") {
        return None;
    }
    let line = line.trim_end();
    if line.ends_with(" joined the game") {
        Some(LogSignal::PlayerJoined)
    } else if line.ends_with(" left the game") {
        Some(LogSignal::PlayerLeft)
    } else if line.ends_with("Stopping server") {
        Some(LogSignal::ServerStopped)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::instance::PackSource;

    fn invite() -> Invite {
        Invite {
            address: "quiet-otter.eu.e4mc.link".into(),
            game_version: "1.21.1".into(),
            loader: Loader::Fabric,
            pack: Some(PackRef {
                project_id: "1KVo5zza".into(),
                version_id: Some("AbCd1234".into()),
            }),
        }
    }

    #[test]
    fn url_round_trips() {
        let url = invite().to_url();
        assert_eq!(
            url,
            "onyx://join/quiet-otter.eu.e4mc.link?mc=1.21.1&loader=fabric&pack=1KVo5zza&packVersion=AbCd1234"
        );
        assert_eq!(Invite::parse(&url).unwrap(), invite());
        assert!(invite().to_secret().len() <= MAX_SECRET);
    }

    #[test]
    fn odd_versions_survive() {
        let mut old = invite();
        old.game_version = "1.14 Pre-Release 1".into();
        old.pack = None;
        assert_eq!(Invite::parse(&old.to_url()).unwrap(), old);
    }

    #[test]
    fn parse_rejects_bad_input() {
        assert_eq!(
            Invite::parse("https://join/x.com?mc=1.21"),
            Err(InviteError::NotAnInvite)
        );
        assert_eq!(
            Invite::parse("onyx://join/--demo?mc=1.21"),
            Err(InviteError::BadAddress)
        );
        assert_eq!(
            Invite::parse("onyx://join/a.com:99999?mc=1.21"),
            Err(InviteError::BadAddress)
        );
        assert_eq!(
            Invite::parse("onyx://join/a.com/../x?mc=1.21"),
            Err(InviteError::BadAddress)
        );
        assert_eq!(
            Invite::parse("onyx://join/a.com"),
            Err(InviteError::BadVersion)
        );
        let parsed = Invite::parse("onyx://join/A.com:25566?mc=1.21&loader=nope").unwrap();
        assert_eq!(parsed.address, "a.com:25566");
        assert_eq!(parsed.loader, Loader::Vanilla);
        assert_eq!(parsed.host_port(), ("a.com", 25566));
    }

    fn instance(
        id: &str,
        version: &str,
        loader: Loader,
        pack: Option<&str>,
        played: u64,
    ) -> Instance {
        Instance {
            id: id.into(),
            name: id.into(),
            game_version: version.into(),
            loader,
            loader_version: None,
            created: 0,
            last_played: Some(played),
            played_secs: 0,
            max_memory_mb: None,
            icon: None,
            icon_config: None,
            pack: pack.map(|v| PackSource {
                project_id: "1KVo5zza".into(),
                version_id: v.into(),
                version_number: "1".into(),
            }),
            java_path: None,
            prism_components: false,
        }
    }

    #[test]
    fn ranks_pack_then_loader_then_version() {
        let list = vec![
            instance("vanilla", "1.21.1", Loader::Vanilla, None, 50),
            instance("other", "1.20.1", Loader::Fabric, None, 99),
            instance("fabric-old", "1.21.1", Loader::Fabric, None, 1),
            instance("fabric-new", "1.21.1", Loader::Fabric, None, 2),
            instance("pack-old", "1.21", Loader::Fabric, Some("old"), 3),
            instance("pack", "1.21.1", Loader::Fabric, Some("AbCd1234"), 0),
        ];
        let ids: Vec<_> = rank(&list, &invite())
            .iter()
            .map(|(i, _)| i.id.as_str())
            .collect();
        assert_eq!(
            ids,
            ["pack", "pack-old", "fabric-new", "fabric-old", "vanilla"]
        );
    }

    #[test]
    fn reads_e4mc_and_server_lines() {
        assert_eq!(
            log_signal(
                "[12:01:02] [e4mc-relay/INFO] (e4mc): Domain assigned: Quiet-Otter.eu.e4mc.link"
            ),
            Some(LogSignal::Domain("quiet-otter.eu.e4mc.link".into()))
        );
        assert_eq!(
            log_signal(
                "[12:01:02] [Render thread/INFO]: [CHAT] <bob> Domain assigned: x.e4mc.link"
            ),
            None
        );
        assert_eq!(log_signal("Domain assigned: evil.example.com"), None);
        assert_eq!(
            log_signal("[12:00:00] [Server thread/INFO]: Steve joined the game"),
            Some(LogSignal::PlayerJoined)
        );
        assert_eq!(
            log_signal(
                "[12:00:00] [Server thread/INFO] [minecraft/MinecraftServer]: Alex left the game"
            ),
            Some(LogSignal::PlayerLeft)
        );
        assert_eq!(
            log_signal("[12:00:00] [Server thread/INFO]: Stopping server"),
            Some(LogSignal::ServerStopped)
        );
        assert_eq!(
            log_signal("[12:00:00] [Render thread/INFO]: Steve joined the game"),
            None
        );
        assert_eq!(
            log_signal("[20:24:39] [Render thread/INFO]: Published LAN server on port 45133"),
            Some(LogSignal::LanOpened)
        );
    }
}
