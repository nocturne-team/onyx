//! Installing Modrinth projects (mods, resource packs, shaders) into an
//! instance, with their required dependencies, and recognizing installed files.

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::cache::CachePolicy;
use crate::download::{self, Download, DownloadError};
use crate::instance::Loader;
use crate::modrinth::{ApiError, Client, Version};

#[derive(Debug, thiserror::Error)]
pub enum ContentError {
    #[error(transparent)]
    Api(#[from] ApiError),
    #[error(transparent)]
    Download(#[from] DownloadError),
    #[error("{}: {source}", path.display())]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("vanilla instances can't load mods; create one with a mod loader")]
    VanillaMods,
    #[error("{0} can't be installed into an instance")]
    UnsupportedType(String),
    #[error("no version of this project works with {0}")]
    NoCompatibleVersion(String),
    #[error("this project is already installed")]
    AlreadyInstalled,
}

/// Where content goes, by Modrinth project type.
pub fn folder_for(project_type: &str) -> Result<&'static str, ContentError> {
    match project_type {
        "mod" => Ok("mods"),
        "resourcepack" => Ok("resourcepacks"),
        "shader" => Ok("shaderpacks"),
        other => Err(ContentError::UnsupportedType(other.to_owned())),
    }
}

const SCANNED: &[&str] = &["mods", "resourcepacks", "shaderpacks"];

/// The instance being installed into.
#[derive(Debug, Clone)]
pub struct Target<'a> {
    pub game_dir: &'a Path,
    pub loader: Loader,
    pub game_version: &'a str,
}

impl Target<'_> {
    /// Version loaders to accept for this project type. Quilt runs most Fabric mods.
    fn loaders(&self, project_type: &str) -> Result<Vec<&'static str>, ContentError> {
        if project_type != "mod" {
            return Ok(Vec::new());
        }
        match self.loader {
            Loader::Vanilla => Err(ContentError::VanillaMods),
            Loader::Fabric => Ok(vec!["fabric"]),
            Loader::Quilt => Ok(vec!["quilt", "fabric"]),
            Loader::Forge => Ok(vec!["forge"]),
            Loader::NeoForge => Ok(vec!["neoforge"]),
        }
    }

    fn describe(&self) -> String {
        let loader = match self.loader {
            Loader::Vanilla => "Vanilla",
            Loader::Fabric => "Fabric",
            Loader::Quilt => "Quilt",
            Loader::Forge => "Forge",
            Loader::NeoForge => "NeoForge",
        };
        format!("{loader} {}", self.game_version)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstalledFile {
    pub project_id: String,
    pub file_name: String,
    pub version_number: String,
}

/// Installs the newest compatible version of `project_id` and, recursively,
/// its required dependencies. Dependencies already present are skipped.
pub async fn install_project(
    client: &Client,
    target: &Target<'_>,
    project_id: &str,
    project_type: &str,
) -> Result<Vec<InstalledFile>, ContentError> {
    let folder = folder_for(project_type)?;
    let loaders = target.loaders(project_type)?;
    let installed = identify(client, target.game_dir).await.unwrap_or_else(|e| {
        tracing::warn!("couldn't check installed files: {e}");
        HashMap::new()
    });
    let installed_projects: HashSet<&str> =
        installed.values().map(|v| v.project_id.as_str()).collect();
    let mut done: Vec<InstalledFile> = Vec::new();
    let mut seen: HashSet<String> = HashSet::from([project_id.to_owned()]);
    // (project, exact version if the dependency pins one)
    let mut queue: VecDeque<(String, Option<String>)> =
        VecDeque::from([(project_id.to_owned(), None)]);

    while let Some((project, pinned)) = queue.pop_front() {
        let is_root = project == project_id;
        let version = match pinned {
            Some(version_id) => {
                client
                    .version(&version_id, Some(CachePolicy::SHORT))
                    .await?
            }
            None => match best_version(client, &project, &loaders, target).await? {
                Some(version) => version,
                None if is_root => {
                    return Err(ContentError::NoCompatibleVersion(target.describe()));
                }
                None => {
                    tracing::warn!("skipping dependency {project}: no compatible version");
                    continue;
                }
            },
        };

        // `project_id` may be a slug; compare the resolved id.
        if is_root && installed_projects.contains(version.project_id.as_str()) {
            return Err(ContentError::AlreadyInstalled);
        }

        for dependency in version
            .dependencies
            .iter()
            .filter(|d| d.dependency_type == "required")
        {
            let Some(dep_project) = dependency.project_id.clone() else {
                continue;
            };
            if installed_projects.contains(dep_project.as_str())
                || !seen.insert(dep_project.clone())
            {
                continue;
            }
            queue.push_back((dep_project, dependency.version_id.clone()));
        }

        let Some(file) = version.primary_file() else {
            continue;
        };
        let path = target
            .game_dir
            .join(folder)
            .join(safe_file_name(&file.filename));
        download::fetch(
            client.http(),
            &Download {
                url: file.url.clone(),
                path: path.clone(),
                sha1: Some(file.hashes.sha1.clone()),
                size: Some(file.size),
            },
        )
        .await?;
        done.push(InstalledFile {
            project_id: version.project_id.clone(),
            file_name: path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default(),
            version_number: version.version_number.clone(),
        });
    }
    Ok(done)
}

/// Newest release that fits, else the newest beta or alpha. For Quilt,
/// native Quilt builds win over Fabric ones of the same release.
async fn best_version(
    client: &Client,
    project: &str,
    loaders: &[&str],
    target: &Target<'_>,
) -> Result<Option<Version>, ContentError> {
    let versions = client
        .project_versions(
            project,
            loaders,
            &[target.game_version],
            Some(CachePolicy::SHORT),
        )
        .await?;
    let preferred = loaders.first().copied();
    let rank = |v: &Version| {
        let unstable = v.version_type != "release";
        let not_preferred = preferred.is_some_and(|l| !v.loaders.iter().any(|x| x == l));
        (unstable, not_preferred)
    };
    // Versions come newest first; `min_by_key` keeps the first of equal ranks.
    Ok(versions.into_iter().min_by_key(rank))
}

/// Modrinth's file names come from uploaders; keep them inside the folder.
fn safe_file_name(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| {
            if matches!(c, '/' | '\\' | ':') || c.is_control() {
                '_'
            } else {
                c
            }
        })
        .collect();
    let cleaned = cleaned.trim_start_matches('.');
    if cleaned.is_empty() {
        "download.jar".into()
    } else {
        cleaned.to_owned()
    }
}

/// Files in the content folders that Modrinth recognizes, keyed by
/// `<folder>/<file name>`. `.disabled` files are included.
pub async fn identify(
    client: &Client,
    game_dir: &Path,
) -> Result<HashMap<String, Version>, ContentError> {
    let mut by_hash = hash_files(game_dir).await?;
    let hashes: Vec<String> = by_hash.keys().cloned().collect();
    let versions = client.versions_from_hashes(&hashes).await?;
    Ok(versions
        .into_iter()
        .filter_map(|(hash, version)| Some((by_hash.remove(&hash)?, version)))
        .collect())
}

/// SHA-1 of every file in the content folders, mapped to `<folder>/<name>`.
async fn hash_files(game_dir: &Path) -> Result<HashMap<String, String>, ContentError> {
    let mut by_hash: HashMap<String, String> = HashMap::new();
    for folder in SCANNED {
        let dir = game_dir.join(folder);
        let Ok(mut entries) = tokio::fs::read_dir(&dir).await else {
            continue;
        };
        while let Ok(Some(entry)) = entries.next_entry().await {
            if !entry.file_type().await.is_ok_and(|t| t.is_file()) {
                continue;
            }
            let path = entry.path();
            match download::sha1_file(&path).await {
                Ok(hash) => {
                    by_hash.insert(
                        hash,
                        format!("{folder}/{}", entry.file_name().to_string_lossy()),
                    );
                }
                Err(source) => return Err(ContentError::Io { path, source }),
            }
        }
    }
    Ok(by_hash)
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Update {
    /// `<folder>/<file name>` of the installed file.
    pub key: String,
    pub project_id: String,
    pub current: String,
    pub latest: Version,
}

/// Installed files with a newer compatible version on Modrinth. One request
/// per content folder.
pub async fn check_updates(
    client: &Client,
    target: &Target<'_>,
) -> Result<Vec<Update>, ContentError> {
    let by_hash = hash_files(target.game_dir).await?;
    let hashes: Vec<String> = by_hash.keys().cloned().collect();
    let current = client.versions_from_hashes(&hashes).await?;

    let mut updates = Vec::new();
    for (folder, project_type) in [
        ("mods", "mod"),
        ("resourcepacks", "resourcepack"),
        ("shaderpacks", "shader"),
    ] {
        let folder_hashes: Vec<String> = by_hash
            .iter()
            .filter(|(hash, key)| {
                key.starts_with(&format!("{folder}/")) && current.contains_key(*hash)
            })
            .map(|(hash, _)| hash.clone())
            .collect();
        if folder_hashes.is_empty() {
            continue;
        }
        let loaders = match project_type {
            "mod" => target.loaders(project_type).unwrap_or_default(),
            // Resource packs are tagged with the "minecraft" loader on Modrinth.
            "resourcepack" => vec!["minecraft"],
            _ => Vec::new(),
        };
        let latest = client
            .latest_versions_from_hashes(&folder_hashes, &loaders, &[target.game_version])
            .await?;
        for (hash, latest) in latest {
            let (Some(installed), Some(key)) = (current.get(&hash), by_hash.get(&hash)) else {
                continue;
            };
            if latest.id != installed.id && latest.date_published > installed.date_published {
                updates.push(Update {
                    key: key.clone(),
                    project_id: installed.project_id.clone(),
                    current: installed.version_number.clone(),
                    latest,
                });
            }
        }
    }
    updates.sort_by(|a, b| a.key.cmp(&b.key));
    Ok(updates)
}

/// Replaces the installed file `key` (`<folder>/<file name>`) with
/// `version_id`'s primary file, keeping it disabled if it was.
pub async fn apply_update(
    client: &Client,
    game_dir: &Path,
    key: &str,
    version_id: &str,
) -> Result<InstalledFile, ContentError> {
    let (folder, file_name) = key
        .split_once('/')
        .filter(|(folder, name)| {
            SCANNED.contains(folder) && !name.contains(['/', '\\']) && !name.starts_with('.')
        })
        .ok_or_else(|| ContentError::UnsupportedType(key.to_owned()))?;
    let old = game_dir.join(folder).join(file_name);
    if !old.is_file() {
        return Err(ContentError::UnsupportedType(key.to_owned()));
    }
    let version = client.version(version_id, None).await?;
    let file = version
        .primary_file()
        .ok_or_else(|| ContentError::NoCompatibleVersion("that version has no file".into()))?;
    let mut new_name = safe_file_name(&file.filename);
    if file_name.ends_with(".disabled") {
        new_name.push_str(".disabled");
    }
    let new = game_dir.join(folder).join(&new_name);
    download::fetch(
        client.http(),
        &Download {
            url: file.url.clone(),
            path: new.clone(),
            sha1: Some(file.hashes.sha1.clone()),
            size: Some(file.size),
        },
    )
    .await?;
    if new != old {
        tokio::fs::remove_file(&old)
            .await
            .map_err(|source| ContentError::Io {
                path: old.clone(),
                source,
            })?;
    }
    Ok(InstalledFile {
        project_id: version.project_id,
        file_name: new_name,
        version_number: version.version_number,
    })
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use sha1::{Digest, Sha1};
    use wiremock::matchers::{method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;
    use crate::cache::ResponseCache;

    fn version(
        id: &str,
        project: &str,
        kind: &str,
        loaders: &[&str],
        deps: serde_json::Value,
        server: &str,
    ) -> serde_json::Value {
        let body = format!("jar of {id}");
        json!({
            "id": id, "project_id": project, "name": id, "version_number": id,
            "version_type": kind, "game_versions": ["26.3"], "loaders": loaders,
            "date_published": "2026-11-01T00:00:00Z",
            "dependencies": deps,
            "files": [{
                "hashes": {"sha1": crate::download::hex(&Sha1::digest(body.as_bytes()))},
                "url": format!("{server}/files/{id}.jar"),
                "filename": format!("{id}.jar"),
                "primary": true,
                "size": body.len()
            }]
        })
    }

    async fn serve_file(server: &MockServer, id: &str) {
        Mock::given(method("GET"))
            .and(path(format!("/files/{id}.jar")))
            .respond_with(ResponseTemplate::new(200).set_body_string(format!("jar of {id}")))
            .mount(server)
            .await;
    }

    #[tokio::test]
    async fn installs_required_dependencies_and_prefers_releases() {
        let server = MockServer::start().await;
        let uri = server.uri();
        Mock::given(method("GET"))
            .and(path("/v2/project/modmenu/version"))
            .and(query_param("loaders", r#"["fabric"]"#))
            .and(query_param("game_versions", r#"["26.3"]"#))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!([
                version("mm-beta", "modmenu", "beta", &["fabric"], json!([]), &uri),
                version(
                    "mm-1",
                    "modmenu",
                    "release",
                    &["fabric"],
                    json!([
                        {"project_id": "fapi", "dependency_type": "required"},
                        {"project_id": "optional-thing", "dependency_type": "optional"}
                    ]),
                    &uri
                ),
            ])))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/v2/project/fapi/version"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!([version(
                "fapi-1",
                "fapi",
                "release",
                &["fabric"],
                json!([]),
                &uri
            )])))
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/v2/version_files"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
            .mount(&server)
            .await;
        serve_file(&server, "mm-1").await;
        serve_file(&server, "fapi-1").await;

        let client = Client::with_base(server.uri(), ResponseCache::new(None));
        let game = tempfile::tempdir().unwrap();
        let target = Target {
            game_dir: game.path(),
            loader: Loader::Fabric,
            game_version: "26.3",
        };
        let installed = install_project(&client, &target, "modmenu", "mod")
            .await
            .unwrap();
        let files: Vec<&str> = installed.iter().map(|f| f.file_name.as_str()).collect();
        assert_eq!(files, ["mm-1.jar", "fapi-1.jar"]);
        assert!(game.path().join("mods/fapi-1.jar").is_file());
        assert!(!game.path().join("mods/optional-thing.jar").exists());
    }

    #[tokio::test]
    async fn vanilla_refuses_mods_and_no_versions_is_explained() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/v2/project/x/version"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!([])))
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/v2/version_files"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
            .mount(&server)
            .await;
        let client = Client::with_base(server.uri(), ResponseCache::new(None));
        let game = tempfile::tempdir().unwrap();
        let vanilla = Target {
            game_dir: game.path(),
            loader: Loader::Vanilla,
            game_version: "26.3",
        };
        assert!(matches!(
            install_project(&client, &vanilla, "x", "mod").await,
            Err(ContentError::VanillaMods)
        ));
        assert!(matches!(
            install_project(&client, &vanilla, "x", "resourcepack").await,
            Err(ContentError::NoCompatibleVersion(_))
        ));
    }

    #[tokio::test]
    async fn updates_are_found_and_applied_keeping_disabled_state() {
        let server = MockServer::start().await;
        let uri = server.uri();
        let old = version(
            "sodium-1",
            "sodium",
            "release",
            &["fabric"],
            json!([]),
            &uri,
        );
        let new = version(
            "sodium-2",
            "sodium",
            "release",
            &["fabric"],
            json!([]),
            &uri,
        );
        let mut new_later = new.clone();
        new_later["date_published"] = json!("2026-11-05T00:00:00Z");
        let old_hash = old["files"][0]["hashes"]["sha1"]
            .as_str()
            .unwrap()
            .to_owned();

        Mock::given(method("POST"))
            .and(path("/v2/version_files"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(json!({ old_hash.clone(): old })),
            )
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/v2/version_files/update"))
            .and(wiremock::matchers::body_partial_json(
                json!({"loaders": ["fabric"], "game_versions": ["26.3"]}),
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({ old_hash: new_later })))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/v2/version/sodium-2"))
            .respond_with(ResponseTemplate::new(200).set_body_json(new_later.clone()))
            .mount(&server)
            .await;
        serve_file(&server, "sodium-2").await;

        let game = tempfile::tempdir().unwrap();
        std::fs::create_dir(game.path().join("mods")).unwrap();
        std::fs::write(
            game.path().join("mods/sodium-1.jar.disabled"),
            "jar of sodium-1",
        )
        .unwrap();
        let client = Client::with_base(server.uri(), ResponseCache::new(None));
        let target = Target {
            game_dir: game.path(),
            loader: Loader::Fabric,
            game_version: "26.3",
        };

        let updates = check_updates(&client, &target).await.unwrap();
        assert_eq!(updates.len(), 1);
        assert_eq!(updates[0].key, "mods/sodium-1.jar.disabled");
        assert_eq!(updates[0].current, "sodium-1");

        let installed = apply_update(&client, game.path(), &updates[0].key, &updates[0].latest.id)
            .await
            .unwrap();
        assert_eq!(installed.file_name, "sodium-2.jar.disabled");
        assert!(game.path().join("mods/sodium-2.jar.disabled").is_file());
        assert!(!game.path().join("mods/sodium-1.jar.disabled").exists());

        assert!(
            apply_update(&client, game.path(), "../instance.json", "sodium-2")
                .await
                .is_err()
        );
        assert!(
            apply_update(&client, game.path(), "mods/../x.jar", "sodium-2")
                .await
                .is_err()
        );
    }

    #[test]
    fn file_names_stay_in_the_folder() {
        assert_eq!(safe_file_name("../evil.jar"), "_evil.jar");
        assert_eq!(safe_file_name("a\\b.jar"), "a_b.jar");
        assert_eq!(safe_file_name("..."), "download.jar");
    }
}
