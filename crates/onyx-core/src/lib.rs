//! Everything Onyx does that doesn't need a window: Modrinth API, auth,
//! instances, downloads and launching. The Tauri app is a thin layer over this.

pub mod auth;
pub mod cache;
pub mod config;
pub mod content;
pub mod download;
pub mod icons;
pub mod instance;
pub mod minecraft;
pub mod modpack;
pub mod modrinth;
pub mod mods;
pub mod paths;
pub mod settings;

pub use settings::{Settings, Theme};
