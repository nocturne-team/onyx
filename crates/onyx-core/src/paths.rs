use std::path::PathBuf;

use directories::ProjectDirs;

/// Root folder for everything Onyx stores: settings, instances, caches.
///
/// `~/.local/share/onyx` on Linux, `%APPDATA%\onyx\data` on Windows.
pub fn data_dir() -> Option<PathBuf> {
    ProjectDirs::from("", "", "onyx").map(|dirs| dirs.data_dir().to_path_buf())
}

/// Shared game files: versions, libraries, assets, Java runtimes.
pub fn meta_dir() -> Option<PathBuf> {
    data_dir().map(|dir| dir.join("meta"))
}

pub fn settings_file() -> Option<PathBuf> {
    data_dir().map(|dir| dir.join("settings.json"))
}

/// Disposable cached data: `~/.cache/onyx` on Linux, `%LOCALAPPDATA%\onyx\cache` on Windows.
pub fn cache_dir() -> Option<PathBuf> {
    ProjectDirs::from("", "", "onyx").map(|dirs| dirs.cache_dir().to_path_buf())
}
