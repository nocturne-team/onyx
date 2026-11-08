//! Values baked in at build time by `build.rs`. Each is `None` when the
//! matching env var was missing during the build.

pub const MODRINTH_CLIENT_ID: Option<&str> = option_env!("ONYX_MODRINTH_CLIENT_ID");
pub const MODRINTH_CLIENT_SECRET: Option<&str> = option_env!("ONYX_MODRINTH_CLIENT_SECRET");
pub const MSA_CLIENT_ID: Option<&str> = option_env!("ONYX_MSA_CLIENT_ID");
/// Discord application for rich presence; its art assets are the instance
/// icons (`ui/src/assets/instance-icons/@512x512/`) plus `icon` (the app icon).
pub const DISCORD_APP_ID: Option<&str> = option_env!("ONYX_DISCORD_APP_ID");

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Sent with every request to Modrinth, which blocks generic user agents and
/// may refuse requests without contact info. Format follows their docs:
/// `github_username/project_name/version (contact)`.
pub const USER_AGENT: &str = concat!(
    "nocturne-team/onyx/",
    env!("CARGO_PKG_VERSION"),
    " (contact@kkaii.xyz; +https://github.com/nocturne-team/onyx)"
);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_agent_names_project_version_and_contact() {
        assert!(USER_AGENT.starts_with(&format!("nocturne-team/onyx/{VERSION} (")));
        assert!(USER_AGENT.contains("contact@kkaii.xyz"));
    }
}
