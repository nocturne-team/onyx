//! Importing instances from Prism Launcher (or MultiMC).

use std::path::PathBuf;

use onyx_core::instance::{self, Instance};
use onyx_core::prism::{self, PrismInstance};
use serde::Serialize;
use tauri::{AppHandle, State};

use crate::AppState;
use crate::content::run_in_background;

/// Prism data folders found in the usual places.
#[tauri::command]
pub fn prism_roots() -> Vec<PathBuf> {
    prism::default_roots()
}

#[tauri::command]
pub async fn prism_instances(root: PathBuf) -> Result<Vec<PrismInstance>, String> {
    tauri::async_runtime::spawn_blocking(move || prism::list(&root))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())
}

/// What `import_prism_instances` created, and what it couldn't.
#[derive(Serialize)]
pub struct PrismImport {
    instances: Vec<Instance>,
    errors: Vec<String>,
}

/// Creates an Onyx instance for each picked Prism instance and copies their
/// game folders in the background (`instance:state` progress, like a modpack
/// install). Returns the new instances right away, plus why any of the
/// picked ones couldn't be created.
#[tauri::command]
pub async fn import_prism_instances(
    app: AppHandle,
    state: State<'_, AppState>,
    root: PathBuf,
    folders: Vec<String>,
) -> Result<PrismImport, String> {
    let all = {
        let root = root.clone();
        tauri::async_runtime::spawn_blocking(move || prism::list(&root))
            .await
            .map_err(|e| e.to_string())?
            .map_err(|e| e.to_string())?
    };
    let onyx_root = state.paths.instances.clone();
    let mut created = Vec::new();
    let mut errors = Vec::new();
    for folder in folders {
        let Some(source) = all.iter().find(|i| i.folder == folder).cloned() else {
            errors.push(format!("no Prism instance called {folder}"));
            continue;
        };
        let instance = match prism::create(&onyx_root, &source) {
            Ok(instance) => instance,
            Err(e) => {
                errors.push(e.to_string());
                continue;
            }
        };
        let (root, onyx_root, id) = (root.clone(), onyx_root.clone(), instance.id.clone());
        let stage = format!("Copying {} from Prism", source.name);
        run_in_background(
            app.clone(),
            &state,
            &instance.id,
            stage,
            "Import failed",
            None,
            move |progress| async move {
                let target = instance::instance_dir(&onyx_root, &id).map_err(|e| e.to_string())?;
                prism::copy_game_folder(&root, &source, &target, &progress)
                    .await
                    .map_err(|e| e.to_string())
            },
        );
        created.push(instance);
    }
    Ok(PrismImport {
        instances: created,
        errors,
    })
}
