//! Mojang's version manifest and per-version JSON, plus the rule system those
//! files use to switch libraries and arguments on per OS and feature.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

pub const MANIFEST_URL: &str = "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json";

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct VersionManifest {
    pub latest: Latest,
    pub versions: Vec<ManifestEntry>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Latest {
    pub release: String,
    pub snapshot: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ManifestEntry {
    pub id: String,
    /// `release`, `snapshot`, `old_beta` or `old_alpha`.
    #[serde(rename = "type")]
    pub kind: String,
    pub url: String,
    pub release_time: String,
    pub sha1: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VersionInfo {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub main_class: String,
    /// 1.13 and later.
    pub arguments: Option<Arguments>,
    /// Before 1.13: one space-separated string of game arguments.
    pub minecraft_arguments: Option<String>,
    pub asset_index: AssetIndexRef,
    pub assets: String,
    pub downloads: VersionDownloads,
    #[serde(default)]
    pub libraries: Vec<Library>,
    pub java_version: Option<JavaVersion>,
    pub logging: Option<Logging>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Arguments {
    #[serde(default)]
    pub game: Vec<Argument>,
    #[serde(default)]
    pub jvm: Vec<Argument>,
    /// Memory and GC flags Mojang recommends (26.x and later).
    #[serde(default, rename = "default-user-jvm")]
    pub default_user_jvm: Vec<Argument>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum Argument {
    Plain(String),
    Ruled {
        /// Missing in some `default-user-jvm` entries, meaning "always".
        #[serde(default)]
        rules: Vec<Rule>,
        value: ArgValue,
    },
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum ArgValue {
    One(String),
    Many(Vec<String>),
}

impl Argument {
    /// The argument's values if its rules allow it here.
    pub fn values(&self, env: &Environment) -> Vec<&str> {
        match self {
            Argument::Plain(value) => vec![value],
            Argument::Ruled { rules, value } if rules_allow(rules, env) => match value {
                ArgValue::One(value) => vec![value],
                ArgValue::Many(values) => values.iter().map(String::as_str).collect(),
            },
            Argument::Ruled { .. } => Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetIndexRef {
    pub id: String,
    pub sha1: String,
    pub size: u64,
    pub total_size: Option<u64>,
    pub url: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct VersionDownloads {
    pub client: Artifact,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Artifact {
    /// Only set for library artifacts.
    pub path: Option<String>,
    /// Always set by Mojang; some loader libraries (Quilt) have neither.
    pub sha1: Option<String>,
    pub size: Option<u64>,
    pub url: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Library {
    pub name: String,
    pub downloads: Option<LibraryDownloads>,
    /// Pre-1.19 natives: OS name to classifier, e.g. `"natives-windows-${arch}"`.
    pub natives: Option<HashMap<String, String>>,
    pub rules: Option<Vec<Rule>>,
    pub extract: Option<Extract>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LibraryDownloads {
    pub artifact: Option<Artifact>,
    pub classifiers: Option<HashMap<String, Artifact>>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct Extract {
    #[serde(default)]
    pub exclude: Vec<String>,
}

impl Library {
    pub fn allowed(&self, env: &Environment) -> bool {
        self.rules
            .as_ref()
            .is_none_or(|rules| rules_allow(rules, env))
    }

    /// The jar that goes on the classpath, if any.
    pub fn artifact(&self) -> Option<&Artifact> {
        self.downloads.as_ref()?.artifact.as_ref()
    }

    /// The old-style natives jar for this OS, to be extracted before launch.
    pub fn native_artifact(&self, env: &Environment) -> Option<&Artifact> {
        let classifier = self.natives.as_ref()?.get(env.os.mojang_name())?;
        let classifier = classifier.replace("${arch}", if env.is_64_bit() { "64" } else { "32" });
        self.downloads
            .as_ref()?
            .classifiers
            .as_ref()?
            .get(&classifier)
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JavaVersion {
    /// Name of the Mojang Java runtime, e.g. `java-runtime-delta`.
    pub component: String,
    pub major_version: u32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Logging {
    pub client: Option<LoggingConfig>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LoggingConfig {
    /// JVM argument with a `${path}` placeholder.
    pub argument: String,
    pub file: LoggingFile,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LoggingFile {
    pub id: String,
    pub sha1: String,
    pub size: u64,
    pub url: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Rule {
    pub action: RuleAction,
    pub os: Option<OsRule>,
    pub features: Option<HashMap<String, bool>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RuleAction {
    Allow,
    Disallow,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OsRule {
    pub name: Option<String>,
    pub arch: Option<String>,
    pub version_range: Option<VersionRange>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct VersionRange {
    pub min: Option<String>,
    pub max: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Os {
    Linux,
    Windows,
    MacOs,
}

impl Os {
    pub fn current() -> Self {
        match std::env::consts::OS {
            "windows" => Os::Windows,
            "macos" => Os::MacOs,
            _ => Os::Linux,
        }
    }

    pub fn mojang_name(self) -> &'static str {
        match self {
            Os::Linux => "linux",
            Os::Windows => "windows",
            Os::MacOs => "osx",
        }
    }
}

/// What rules are evaluated against.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Environment {
    pub os: Os,
    /// `std::env::consts::ARCH` naming: `x86_64`, `x86`, `aarch64`.
    pub arch: &'static str,
    /// Dotted OS version, compared against `versionRange` rules. Only Windows
    /// rules use these so far.
    pub os_version: Option<String>,
    /// Feature flags such as `has_custom_resolution`. Missing ones are off.
    pub features: Vec<&'static str>,
}

impl Environment {
    pub fn current() -> Self {
        Self {
            os: Os::current(),
            arch: std::env::consts::ARCH,
            os_version: None,
            features: Vec::new(),
        }
    }

    pub fn is_64_bit(&self) -> bool {
        matches!(self.arch, "x86_64" | "aarch64")
    }

    fn arch_matches(&self, rule: &str) -> bool {
        match self.arch {
            "x86_64" => matches!(rule, "x86_64" | "amd64" | "x64"),
            "aarch64" => matches!(rule, "arm64" | "aarch64" | "aarch_64"),
            "x86" => rule == "x86",
            other => other == rule,
        }
    }
}

/// Mojang's semantics: with no rules everything is allowed; otherwise start
/// from "disallowed" and let the last matching rule decide.
pub fn rules_allow(rules: &[Rule], env: &Environment) -> bool {
    let mut allowed = rules.is_empty();
    for rule in rules {
        if rule_matches(rule, env) {
            allowed = rule.action == RuleAction::Allow;
        }
    }
    allowed
}

fn rule_matches(rule: &Rule, env: &Environment) -> bool {
    if let Some(os) = &rule.os {
        if os
            .name
            .as_deref()
            .is_some_and(|name| name != env.os.mojang_name())
        {
            return false;
        }
        if os
            .arch
            .as_deref()
            .is_some_and(|arch| !env.arch_matches(arch))
        {
            return false;
        }
        if let Some(range) = &os.version_range {
            // Unknown version: assume a current OS, i.e. at least `min` and
            // not below `max`.
            let in_range = match &env.os_version {
                Some(version) => {
                    range
                        .min
                        .as_deref()
                        .is_none_or(|min| compare_versions(version, min).is_ge())
                        && range
                            .max
                            .as_deref()
                            .is_none_or(|max| compare_versions(version, max).is_lt())
                }
                None => range.max.is_none(),
            };
            if !in_range {
                return false;
            }
        }
    }
    if let Some(features) = &rule.features {
        let all_match = features
            .iter()
            .all(|(name, wanted)| env.features.contains(&name.as_str()) == *wanted);
        if !all_match {
            return false;
        }
    }
    true
}

fn compare_versions(a: &str, b: &str) -> std::cmp::Ordering {
    let parts = |v: &str| -> Vec<u64> { v.split('.').map(|p| p.parse().unwrap_or(0)).collect() };
    parts(a).cmp(&parts(b))
}

/// `group:name:version[:classifier]` to the Maven path under the libraries dir.
pub fn maven_path(coordinate: &str) -> Option<String> {
    let (coordinate, extension) = coordinate.split_once('@').unwrap_or((coordinate, "jar"));
    let mut parts = coordinate.split(':');
    let group = parts.next()?;
    let name = parts.next()?;
    let version = parts.next()?;
    let classifier = parts.next();
    let file = match classifier {
        Some(classifier) => format!("{name}-{version}-{classifier}.{extension}"),
        None => format!("{name}-{version}.{extension}"),
    };
    Some(format!(
        "{}/{name}/{version}/{file}",
        group.replace('.', "/")
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env(os: Os, arch: &'static str) -> Environment {
        Environment {
            os,
            arch,
            os_version: None,
            features: Vec::new(),
        }
    }

    fn rules(json: &str) -> Vec<Rule> {
        serde_json::from_str(json).unwrap()
    }

    #[test]
    fn last_matching_rule_wins() {
        // The classic "everything but macOS" library rule.
        let r = rules(r#"[{"action":"allow"},{"action":"disallow","os":{"name":"osx"}}]"#);
        assert!(rules_allow(&r, &env(Os::Linux, "x86_64")));
        assert!(rules_allow(&r, &env(Os::Windows, "x86_64")));
        assert!(!rules_allow(&r, &env(Os::MacOs, "aarch64")));
        assert!(rules_allow(&[], &env(Os::MacOs, "aarch64")));
    }

    #[test]
    fn arch_and_feature_rules() {
        let netty = rules(r#"[{"action":"allow","os":{"name":"linux","arch":"aarch_64"}}]"#);
        assert!(!rules_allow(&netty, &env(Os::Linux, "x86_64")));
        assert!(rules_allow(&netty, &env(Os::Linux, "aarch64")));

        let demo = rules(r#"[{"action":"allow","features":{"is_demo_user":true}}]"#);
        let mut with_demo = env(Os::Linux, "x86_64");
        assert!(!rules_allow(&demo, &with_demo));
        with_demo.features.push("is_demo_user");
        assert!(rules_allow(&demo, &with_demo));
    }

    #[test]
    fn windows_version_ranges_assume_a_current_os() {
        let zgc = rules(
            r#"[{"action":"allow","os":{"name":"windows","versionRange":{"min":"10.0.17134"}}}]"#,
        );
        let g1 = rules(
            r#"[{"action":"allow","os":{"name":"windows","versionRange":{"max":"10.0.17134"}}}]"#,
        );
        let mut windows = env(Os::Windows, "x86_64");
        assert!(rules_allow(&zgc, &windows));
        assert!(!rules_allow(&g1, &windows));

        windows.os_version = Some("10.0.10586".into());
        assert!(!rules_allow(&zgc, &windows));
        assert!(rules_allow(&g1, &windows));
    }

    /// Every version JSON Mojang has published must parse. Run with
    /// `cargo test -p onyx-core -- --ignored real_version_jsons` (network).
    #[tokio::test]
    #[ignore = "downloads from Mojang"]
    async fn real_version_jsons_parse() {
        let http = reqwest::Client::new();
        let manifest: VersionManifest = http
            .get(MANIFEST_URL)
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        let picks = [
            "26.3",
            "1.21.4",
            "1.12.2",
            "1.8.9",
            "1.5.2",
            "b1.7.3",
            "rd-132211",
        ];
        for id in picks.into_iter().chain([manifest.latest.snapshot.as_str()]) {
            let entry = manifest.versions.iter().find(|v| v.id == id).unwrap();
            let text = http
                .get(&entry.url)
                .send()
                .await
                .unwrap()
                .text()
                .await
                .unwrap();
            let info: VersionInfo =
                serde_json::from_str(&text).unwrap_or_else(|e| panic!("{id}: {e}"));
            assert_eq!(info.id, id);
        }
    }

    #[test]
    fn maven_paths() {
        assert_eq!(
            maven_path("org.lwjgl:lwjgl:3.4.3:natives-linux").unwrap(),
            "org/lwjgl/lwjgl/3.4.3/lwjgl-3.4.3-natives-linux.jar"
        );
        assert_eq!(
            maven_path("net.fabricmc:intermediary:1.21").unwrap(),
            "net/fabricmc/intermediary/1.21/intermediary-1.21.jar"
        );
        assert_eq!(
            maven_path("de.oceanlabs.mcp:mcp_config:1.20@zip").unwrap(),
            "de/oceanlabs/mcp/mcp_config/1.20/mcp_config-1.20.zip"
        );
    }

    #[test]
    fn mixed_arguments_parse() {
        let args: Vec<Argument> = serde_json::from_str(
            r#"["--username", {"rules":[{"action":"allow","features":{"has_custom_resolution":true}}],
               "value":["--width","${resolution_width}"]},
               {"rules":[{"action":"allow","os":{"name":"windows"}}],"value":"-XX:HeapDumpPath=x"}]"#,
        )
        .unwrap();
        let linux = env(Os::Linux, "x86_64");
        let values: Vec<&str> = args.iter().flat_map(|a| a.values(&linux)).collect();
        assert_eq!(values, ["--username"]);
        let windows = env(Os::Windows, "x86_64");
        let values: Vec<&str> = args.iter().flat_map(|a| a.values(&windows)).collect();
        assert_eq!(values, ["--username", "-XX:HeapDumpPath=x"]);
    }
}
