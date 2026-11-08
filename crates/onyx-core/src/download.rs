//! Bounded-concurrency file downloads with hash checks.
//!
//! Every file is written to a temp file next to its destination and renamed
//! into place once its SHA-1 matches, so a half-finished download never looks
//! like a finished one.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use serde::Serialize;
use sha1::{Digest, Sha1};
use tokio::io::AsyncWriteExt;
use tokio::sync::Semaphore;
use tokio::task::JoinSet;

pub const DEFAULT_CONCURRENCY: usize = 16;
const ATTEMPTS: u32 = 3;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Download {
    pub url: String,
    pub path: PathBuf,
    /// Lowercase hex. Files without one are only checked for existence.
    pub sha1: Option<String>,
    pub size: Option<u64>,
}

#[derive(Debug, thiserror::Error)]
pub enum DownloadError {
    #[error("could not download {url}: {source}")]
    Http {
        url: String,
        #[source]
        source: reqwest::Error,
    },
    #[error("{url} returned HTTP {status}")]
    Status { url: String, status: u16 },
    #[error("{url} downloaded with the wrong hash")]
    HashMismatch { url: String },
    #[error("could not write {}: {source}", path.display())]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

impl DownloadError {
    fn io(path: &Path) -> impl FnOnce(std::io::Error) -> Self + '_ {
        move |source| Self::Io {
            path: path.to_owned(),
            source,
        }
    }
}

/// Counters shared between the download tasks and whoever reports progress.
/// Totals grow as work is queued, so read them together via `snapshot`.
#[derive(Debug, Default)]
pub struct Progress {
    pub done_files: AtomicU64,
    pub total_files: AtomicU64,
    pub done_bytes: AtomicU64,
    pub total_bytes: AtomicU64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProgressSnapshot {
    pub done_files: u64,
    pub total_files: u64,
    pub done_bytes: u64,
    pub total_bytes: u64,
}

impl Progress {
    pub fn snapshot(&self) -> ProgressSnapshot {
        ProgressSnapshot {
            done_files: self.done_files.load(Ordering::Relaxed),
            total_files: self.total_files.load(Ordering::Relaxed),
            done_bytes: self.done_bytes.load(Ordering::Relaxed),
            total_bytes: self.total_bytes.load(Ordering::Relaxed),
        }
    }

    pub fn reset(&self) {
        for counter in [
            &self.done_files,
            &self.total_files,
            &self.done_bytes,
            &self.total_bytes,
        ] {
            counter.store(0, Ordering::Relaxed);
        }
    }
}

/// Downloads everything in `items` that isn't already on disk, at most
/// `concurrency` at a time. Stops at the first failure.
///
/// A file already on disk counts as present when its size matches (or no size
/// is known). Hashing every asset on every launch would cost seconds for no
/// real gain; files are hash-checked when they're downloaded.
pub async fn download_all(
    http: &reqwest::Client,
    items: Vec<Download>,
    progress: &Arc<Progress>,
    concurrency: usize,
) -> Result<(), DownloadError> {
    let mut items = items;
    items.sort_by(|a, b| a.path.cmp(&b.path));
    items.dedup_by(|a, b| a.path == b.path);

    progress
        .total_files
        .fetch_add(items.len() as u64, Ordering::Relaxed);
    progress
        .total_bytes
        .fetch_add(items.iter().filter_map(|d| d.size).sum(), Ordering::Relaxed);

    let semaphore = Arc::new(Semaphore::new(concurrency.max(1)));
    let mut tasks = JoinSet::new();
    for item in items {
        let http = http.clone();
        let progress = progress.clone();
        let semaphore = semaphore.clone();
        tasks.spawn(async move {
            let _permit = semaphore
                .acquire_owned()
                .await
                .expect("semaphore is never closed");
            if !is_present(&item).await {
                fetch_with_retries(&http, &item).await?;
            }
            progress.done_files.fetch_add(1, Ordering::Relaxed);
            progress
                .done_bytes
                .fetch_add(item.size.unwrap_or(0), Ordering::Relaxed);
            Ok::<_, DownloadError>(())
        });
    }

    while let Some(result) = tasks.join_next().await {
        match result {
            Ok(Ok(())) => {}
            Ok(Err(e)) => {
                tasks.abort_all();
                return Err(e);
            }
            Err(e) if e.is_panic() => std::panic::resume_unwind(e.into_panic()),
            Err(_) => {}
        }
    }
    Ok(())
}

async fn is_present(item: &Download) -> bool {
    match tokio::fs::metadata(&item.path).await {
        Ok(meta) => meta.is_file() && item.size.is_none_or(|size| size == meta.len()),
        Err(_) => false,
    }
}

async fn fetch_with_retries(http: &reqwest::Client, item: &Download) -> Result<(), DownloadError> {
    let mut attempt = 1;
    loop {
        match fetch(http, item).await {
            Ok(()) => return Ok(()),
            // Disk errors and 404s won't fix themselves.
            Err(e @ (DownloadError::Io { .. } | DownloadError::Status { status: 404, .. })) => {
                return Err(e);
            }
            Err(e) if attempt >= ATTEMPTS => return Err(e),
            Err(e) => {
                tracing::debug!("retrying ({attempt}/{ATTEMPTS}): {e}");
                tokio::time::sleep(Duration::from_millis(500 * u64::from(attempt))).await;
                attempt += 1;
            }
        }
    }
}

/// Downloads one file to a temp path, checks its hash and renames it into place.
pub async fn fetch(http: &reqwest::Client, item: &Download) -> Result<(), DownloadError> {
    let http_err = |source| DownloadError::Http {
        url: item.url.clone(),
        source,
    };
    let mut response = http.get(&item.url).send().await.map_err(http_err)?;
    if !response.status().is_success() {
        return Err(DownloadError::Status {
            url: item.url.clone(),
            status: response.status().as_u16(),
        });
    }

    if let Some(parent) = item.path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(DownloadError::io(parent))?;
    }
    let temp = temp_path(&item.path);
    let mut file = tokio::fs::File::create(&temp)
        .await
        .map_err(DownloadError::io(&temp))?;
    let mut hasher = Sha1::new();
    let result = async {
        while let Some(chunk) = response.chunk().await.map_err(http_err)? {
            hasher.update(&chunk);
            file.write_all(&chunk)
                .await
                .map_err(DownloadError::io(&temp))?;
        }
        file.flush().await.map_err(DownloadError::io(&temp))?;
        drop(file);

        let actual = hex(&hasher.finalize());
        if item
            .sha1
            .as_ref()
            .is_some_and(|expected| !actual.eq_ignore_ascii_case(expected))
        {
            return Err(DownloadError::HashMismatch {
                url: item.url.clone(),
            });
        }
        tokio::fs::rename(&temp, &item.path)
            .await
            .map_err(DownloadError::io(&item.path))
    }
    .await;

    if result.is_err() {
        let _ = tokio::fs::remove_file(&temp).await;
    }
    result
}

fn temp_path(path: &Path) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(".onyx-part");
    path.with_file_name(name)
}

pub(crate) fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// SHA-1 of a file on disk, as lowercase hex.
pub async fn sha1_file(path: &Path) -> std::io::Result<String> {
    let bytes = tokio::fs::read(path).await?;
    Ok(hex(&Sha1::digest(&bytes)))
}

#[cfg(test)]
mod tests {
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;

    const BODY: &[u8] = b"hello onyx";

    fn body_sha1() -> String {
        hex(&Sha1::digest(BODY))
    }

    async fn serve(server: &MockServer, at: &str, body: &'static [u8]) {
        Mock::given(method("GET"))
            .and(path(at))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(body))
            .mount(server)
            .await;
    }

    #[tokio::test]
    async fn downloads_verifies_and_skips_present_files() {
        let server = MockServer::start().await;
        serve(&server, "/a", BODY).await;
        let dir = tempfile::tempdir().unwrap();
        let item = Download {
            url: format!("{}/a", server.uri()),
            path: dir.path().join("nested/a.bin"),
            sha1: Some(body_sha1()),
            size: Some(BODY.len() as u64),
        };
        let progress = Arc::new(Progress::default());
        let http = reqwest::Client::new();

        download_all(&http, vec![item.clone(), item.clone()], &progress, 4)
            .await
            .unwrap();
        assert_eq!(std::fs::read(&item.path).unwrap(), BODY);
        // Duplicates are only fetched (and counted) once.
        assert_eq!(progress.snapshot().total_files, 1);
        assert_eq!(server.received_requests().await.unwrap().len(), 1);

        download_all(&http, vec![item], &progress, 4).await.unwrap();
        assert_eq!(server.received_requests().await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn wrong_hash_leaves_nothing_behind() {
        let server = MockServer::start().await;
        serve(&server, "/a", BODY).await;
        let dir = tempfile::tempdir().unwrap();
        let item = Download {
            url: format!("{}/a", server.uri()),
            path: dir.path().join("a.bin"),
            sha1: Some("0000000000000000000000000000000000000000".into()),
            size: None,
        };
        let err = download_all(
            &reqwest::Client::new(),
            vec![item.clone()],
            &Arc::new(Progress::default()),
            1,
        )
        .await
        .unwrap_err();
        assert!(matches!(err, DownloadError::HashMismatch { .. }));
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
    }

    #[tokio::test]
    async fn missing_files_fail_without_retrying() {
        let server = MockServer::start().await;
        let dir = tempfile::tempdir().unwrap();
        let item = Download {
            url: format!("{}/missing", server.uri()),
            path: dir.path().join("a.bin"),
            sha1: None,
            size: None,
        };
        let err = download_all(
            &reqwest::Client::new(),
            vec![item],
            &Arc::new(Progress::default()),
            1,
        )
        .await
        .unwrap_err();
        assert!(matches!(err, DownloadError::Status { status: 404, .. }));
        assert_eq!(server.received_requests().await.unwrap().len(), 1);
    }
}
