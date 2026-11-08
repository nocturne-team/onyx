use std::io;
use std::path::Path;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Theme {
    #[default]
    Dark,
    Light,
    Oled,
    System,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Settings {
    pub theme: Theme,
    /// Blur and other effects that need hardware-accelerated rendering.
    pub advanced_rendering: bool,
    /// Max heap for instances without their own setting. `None` means
    /// Mojang's recommendation for the version (or 2 GB for old versions).
    pub default_memory_mb: Option<u32>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            theme: Theme::Dark,
            advanced_rendering: true,
            default_memory_mb: None,
        }
    }
}

/// Memory settings outside this range are refused, in megabytes.
pub const MEMORY_RANGE_MB: std::ops::RangeInclusive<u32> = 512..=65536;

/// Total physical memory in megabytes, for sizing the memory slider.
/// `None` where we don't know how to read it.
pub fn total_memory_mb() -> Option<u64> {
    #[cfg(target_os = "linux")]
    {
        let meminfo = std::fs::read_to_string("/proc/meminfo").ok()?;
        let kb: u64 = meminfo
            .lines()
            .find_map(|line| line.strip_prefix("MemTotal:"))?
            .trim()
            .trim_end_matches("kB")
            .trim()
            .parse()
            .ok()?;
        Some(kb / 1024)
    }
    #[cfg(not(target_os = "linux"))]
    {
        None
    }
}

#[derive(Debug, thiserror::Error)]
pub enum SettingsError {
    #[error("could not read or write settings file: {0}")]
    Io(#[from] io::Error),
    #[error("settings file is not valid: {0}")]
    Parse(#[from] serde_json::Error),
}

impl Settings {
    /// Loads settings, returning the defaults if the file doesn't exist yet.
    pub fn load(path: &Path) -> Result<Self, SettingsError> {
        match std::fs::read(path) {
            Ok(bytes) => Ok(serde_json::from_slice(&bytes)?),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(e.into()),
        }
    }

    /// Writes to a temp file next to `path` and renames it over, so a crash
    /// mid-write can't leave a half-written settings file.
    pub fn save(&self, path: &Path) -> Result<(), SettingsError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_vec_pretty(self)?)?;
        std::fs::rename(&tmp, path)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_is_readable_here() {
        if cfg!(target_os = "linux") {
            assert!(total_memory_mb().is_some_and(|mb| mb > 256));
        }
    }

    #[test]
    fn missing_file_gives_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let settings = Settings::load(&dir.path().join("settings.json")).unwrap();
        assert_eq!(settings, Settings::default());
    }

    #[test]
    fn save_then_load_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested/settings.json");
        let settings = Settings {
            theme: Theme::Oled,
            advanced_rendering: false,
            default_memory_mb: Some(6144),
        };
        settings.save(&path).unwrap();
        assert_eq!(Settings::load(&path).unwrap(), settings);
        assert!(!path.with_extension("json.tmp").exists());
    }

    #[test]
    fn missing_and_unknown_fields_are_tolerated() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        std::fs::write(&path, r#"{"theme":"light","fromTheFuture":1}"#).unwrap();
        let settings = Settings::load(&path).unwrap();
        assert_eq!(settings.theme, Theme::Light);
        assert!(settings.advanced_rendering);
    }

    #[test]
    fn corrupt_file_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        std::fs::write(&path, "{not json").unwrap();
        assert!(matches!(
            Settings::load(&path),
            Err(SettingsError::Parse(_))
        ));
    }
}
