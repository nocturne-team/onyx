//! Response cache for API calls that are made often but change rarely, like the
//! home page lists. Entries live in memory, and optionally on disk so they
//! survive restarts.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CachePolicy {
    /// Entries this old or older are refetched; zero means always refetch.
    /// Older entries are still used as a fallback when the request fails.
    pub max_age: Duration,
    /// Also write the entry to disk so it survives restarts.
    pub persist: bool,
}

impl CachePolicy {
    /// Home page lists: popular packs and mods barely move within an hour.
    pub const LONG: Self = Self {
        max_age: Duration::from_secs(60 * 60),
        persist: true,
    };
    /// Browse searches: lets the user flip between tabs and queries without refetching.
    pub const SHORT: Self = Self {
        max_age: Duration::from_secs(5 * 60),
        persist: false,
    };
}

/// Memory entries beyond this are dropped wholesale; cheap and good enough.
const MAX_MEMORY_ENTRIES: usize = 256;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Entry {
    /// Unix seconds.
    fetched_at: u64,
    body: String,
}

pub struct ResponseCache {
    dir: Option<PathBuf>,
    memory: Mutex<HashMap<String, Entry>>,
}

impl ResponseCache {
    /// `dir` is where persisted entries go; `None` keeps everything in memory.
    pub fn new(dir: Option<PathBuf>) -> Self {
        Self {
            dir,
            memory: Mutex::new(HashMap::new()),
        }
    }

    /// Returns the cached body if it's younger than `max_age`.
    pub fn get(&self, key: &str, max_age: Duration) -> Option<String> {
        let entry = self.entry(key)?;
        let age = now_secs().saturating_sub(entry.fetched_at);
        (age < max_age.as_secs()).then_some(entry.body)
    }

    /// Returns the cached body regardless of age, for when the network is down.
    pub fn get_stale(&self, key: &str) -> Option<String> {
        self.entry(key).map(|e| e.body)
    }

    pub fn put(&self, key: &str, body: String, persist: bool) {
        self.put_at(key, body, persist, now_secs());
    }

    fn put_at(&self, key: &str, body: String, persist: bool, fetched_at: u64) {
        let entry = Entry { fetched_at, body };
        if persist {
            if let Some(path) = self.path_for(key) {
                if let Err(e) = write_atomic(&path, &entry) {
                    tracing::warn!("could not persist cache entry {}: {e}", path.display());
                }
            }
        }
        let mut memory = self.memory.lock().unwrap();
        if memory.len() >= MAX_MEMORY_ENTRIES {
            memory.clear();
        }
        memory.insert(key.to_owned(), entry);
    }

    fn entry(&self, key: &str) -> Option<Entry> {
        if let Some(entry) = self.memory.lock().unwrap().get(key) {
            return Some(entry.clone());
        }
        let bytes = std::fs::read(self.path_for(key)?).ok()?;
        let entry: Entry = serde_json::from_slice(&bytes).ok()?;
        self.memory
            .lock()
            .unwrap()
            .insert(key.to_owned(), entry.clone());
        Some(entry)
    }

    fn path_for(&self, key: &str) -> Option<PathBuf> {
        self.dir
            .as_ref()
            .map(|dir| dir.join(format!("{:016x}.json", fnv1a(key))))
    }
}

fn write_atomic(path: &std::path::Path, entry: &Entry) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_vec(entry)?)?;
    std::fs::rename(tmp, path)
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Stable across Rust versions, unlike `DefaultHasher`, so file names stay valid.
fn fnv1a(s: &str) -> u64 {
    s.bytes().fold(0xcbf29ce484222325, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const HOUR: Duration = Duration::from_secs(3600);

    #[test]
    fn fresh_entry_is_returned() {
        let cache = ResponseCache::new(None);
        cache.put("k", "body".into(), false);
        assert_eq!(cache.get("k", HOUR).as_deref(), Some("body"));
    }

    #[test]
    fn expired_entry_is_only_available_as_stale() {
        let cache = ResponseCache::new(None);
        cache.put_at("k", "old".into(), false, now_secs() - 7200);
        assert_eq!(cache.get("k", HOUR), None);
        assert_eq!(cache.get_stale("k").as_deref(), Some("old"));
    }

    #[test]
    fn persisted_entries_survive_a_new_cache() {
        let dir = tempfile::tempdir().unwrap();
        ResponseCache::new(Some(dir.path().into())).put("k", "body".into(), true);
        let reopened = ResponseCache::new(Some(dir.path().into()));
        assert_eq!(reopened.get("k", HOUR).as_deref(), Some("body"));
    }

    #[test]
    fn unpersisted_entries_stay_off_disk() {
        let dir = tempfile::tempdir().unwrap();
        ResponseCache::new(Some(dir.path().into())).put("k", "body".into(), false);
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
    }
}
