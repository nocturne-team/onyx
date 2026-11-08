//! Bakes OAuth client IDs from the environment (or the workspace `.env`) into the
//! crate at compile time, and stops release builds that still use a borrowed
//! Microsoft client ID.

use std::path::Path;

const ENV_KEYS: &[&str] = &[
    "ONYX_MODRINTH_CLIENT_ID",
    "ONYX_MODRINTH_CLIENT_SECRET",
    "ONYX_MSA_CLIENT_ID",
];

/// Prism Launcher's Azure client ID. Fine for local dev, never for anything we ship.
const PRISM_MSA_CLIENT_ID: &str = "c36a9fb6-4f2a-41ff-90bd-ae7cc92031eb";

fn main() {
    let env_file = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.env");
    println!("cargo:rerun-if-changed={}", env_file.display());

    let from_file: Vec<(String, String)> = dotenvy::from_path_iter(&env_file)
        .map(|iter| iter.filter_map(Result::ok).collect())
        .unwrap_or_default();

    // Real environment variables win over `.env`.
    let lookup = |key: &str| {
        std::env::var(key).ok().or_else(|| {
            from_file
                .iter()
                .find(|(k, _)| k == key)
                .map(|(_, v)| v.clone())
        })
    };

    for key in ENV_KEYS {
        println!("cargo:rerun-if-env-changed={key}");
        if let Some(value) = lookup(key) {
            println!("cargo:rustc-env={key}={value}");
        }
    }

    let release = std::env::var("PROFILE").as_deref() == Ok("release");
    if release && lookup("ONYX_MSA_CLIENT_ID").as_deref() == Some(PRISM_MSA_CLIENT_ID) {
        panic!(
            "ONYX_MSA_CLIENT_ID is Prism Launcher's client ID. Register Onyx's own Azure app \
             before making a release build."
        );
    }
}
