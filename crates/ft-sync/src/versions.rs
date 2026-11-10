//! Local version history for sealed backup bundles.
//!
//! Every sync run (manual, on-change or scheduled) leaves a sealed snapshot and
//! an index entry here. This is the fallback history for Gist, which can only
//! hold one file, and the safety net for a WebDAV index that cannot be read.
//! The newest [`MAX_VERSIONS`] bundles are kept; older files are pruned.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::SyncError;

pub const MAX_VERSIONS: usize = 20;
pub const INDEX_FILE: &str = "versions.json";
pub const INDEX_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum VersionSource {
    /// A local export or a sync run whose provider has no multi-version support.
    Local,
    /// A version file stored on WebDAV.
    Webdav,
    /// The single-file Gist upload; the bundle itself lives locally.
    Gist,
    /// Snapshot taken automatically right before a rollback.
    RestorePoint,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VersionEntry {
    /// Also the version file name (`{id}.ftbak`).
    pub id: String,
    /// Unix seconds, used for the "when" column in version records.
    pub created_at: u64,
    /// The scopes this bundle carries, so a rollback rewinds only these.
    pub scope: Vec<String>,
    /// Opaque per-type counts (`BackupSummary` JSON) shown without decrypting.
    pub summary: serde_json::Value,
    pub source: VersionSource,
    pub bytes: usize,
    #[serde(default)]
    pub note: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VersionIndex {
    #[serde(default)]
    pub version: u32,
    #[serde(default)]
    pub entries: Vec<VersionEntry>,
}

impl VersionIndex {
    /// Newest first. Shared by the local index and the WebDAV index so both
    /// providers show the same ordering.
    pub fn sort_newest_first(&mut self) {
        self.entries.sort_by(|a, b| {
            b.created_at
                .cmp(&a.created_at)
                .then_with(|| b.id.cmp(&a.id))
        });
    }

    pub fn keep_newest(&mut self, limit: usize) -> Vec<String> {
        self.sort_newest_first();
        self.entries
            .drain(limit.min(self.entries.len())..)
            .map(|entry| entry.id)
            .collect()
    }
}

fn now() -> (u64, u128) {
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    (duration.as_secs(), duration.as_nanos())
}

fn new_id() -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let (_, nanos) = now();
    let seq = COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("v{:x}-{:x}-{:x}", nanos / 1_000_000, nanos % 1_000_000, seq)
}

/// Only plain, short ids are accepted, so an index entry can never be used to
/// read a file outside the versions directory.
fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 96
        && !id.contains("..")
        && id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
}

#[derive(Debug, Clone)]
pub struct VersionStore {
    dir: PathBuf,
}

impl VersionStore {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    fn index_path(&self) -> PathBuf {
        self.dir.join(INDEX_FILE)
    }

    fn bundle_path(&self, id: &str) -> Result<PathBuf, SyncError> {
        if !valid_id(id) {
            return Err(SyncError::Format("invalid version id".to_string()));
        }
        Ok(self.dir.join(format!("{id}.ftbak")))
    }

    /// A damaged or missing index is treated as empty rather than breaking
    /// sync entirely; the bundles on disk can still be opened by hand.
    pub fn index(&self) -> VersionIndex {
        let Ok(raw) = fs::read(self.index_path()) else {
            return VersionIndex::default();
        };
        let mut index: VersionIndex = serde_json::from_slice(&raw).unwrap_or_default();
        index.sort_newest_first();
        index
    }

    pub fn list(&self) -> Vec<VersionEntry> {
        self.index().entries
    }

    pub fn write_index(&self, index: &VersionIndex) -> Result<(), SyncError> {
        fs::create_dir_all(&self.dir)?;
        let bytes = serde_json::to_vec_pretty(index)
            .map_err(|err| SyncError::Format(err.to_string()))?;
        let target = self.index_path();
        let temp = self.dir.join(format!("{INDEX_FILE}.tmp"));
        fs::write(&temp, bytes)?;
        // Windows refuses to rename over an existing file.
        if fs::rename(&temp, &target).is_err() {
            let _ = fs::remove_file(&target);
            fs::rename(&temp, &target)?;
        }
        Ok(())
    }

    pub fn read(&self, id: &str) -> Result<String, SyncError> {
        Ok(fs::read_to_string(self.bundle_path(id)?)?)
    }

    /// Adds a fresh local version and prunes older bundles.
    pub fn add(
        &self,
        sealed: &str,
        scope: Vec<String>,
        summary: serde_json::Value,
        source: VersionSource,
        note: impl Into<String>,
    ) -> Result<VersionEntry, SyncError> {
        let (created_at, _) = now();
        let entry = VersionEntry {
            id: new_id(),
            created_at,
            scope,
            summary,
            source,
            bytes: sealed.len(),
            note: note.into(),
        };
        self.insert(entry.clone(), sealed)?;
        Ok(entry)
    }

    /// Stores a version that already has an id (a remote index entry or the
    /// same bundle uploaded to more than one place).
    pub fn insert(&self, entry: VersionEntry, sealed: &str) -> Result<(), SyncError> {
        fs::create_dir_all(&self.dir)?;
        fs::write(self.bundle_path(&entry.id)?, sealed)?;
        let mut index = self.index();
        index.entries.retain(|item| item.id != entry.id);
        index.entries.push(entry);
        index.version = INDEX_VERSION;
        let removed = index.keep_newest(MAX_VERSIONS);
        self.write_index(&index)?;
        for id in removed {
            if let Ok(path) = self.bundle_path(&id) {
                let _ = fs::remove_file(path);
            }
        }
        Ok(())
    }

    /// Drops a bundle and its index entry, used after a remote prune.
    pub fn remove(&self, id: &str) -> Result<(), SyncError> {
        let _ = fs::remove_file(self.bundle_path(id)?);
        let mut index = self.index();
        index.entries.retain(|entry| entry.id != id);
        self.write_index(&index)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicU64;

    fn temp_dir() -> PathBuf {
        static SEQ: AtomicU64 = AtomicU64::new(0);
        let dir = std::env::temp_dir().join(format!(
            "ft-versions-{}-{}",
            std::process::id(),
            SEQ.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    fn entry(id: &str, at: u64) -> VersionEntry {
        VersionEntry {
            id: id.into(),
            created_at: at,
            scope: vec!["hosts".into()],
            summary: serde_json::json!({"hosts": 1}),
            source: VersionSource::Local,
            bytes: 12,
            note: String::new(),
        }
    }

    #[test]
    fn add_writes_bundle_and_index_newest_first() {
        let store = VersionStore::new(temp_dir());
        let first = store
            .add("SEALED-1", vec!["hosts".into()], serde_json::json!({}), VersionSource::Local, "")
            .unwrap();
        let second = store
            .add("SEALED-2", vec!["macros".into()], serde_json::json!({}), VersionSource::Webdav, "")
            .unwrap();
        assert_eq!(store.read(&first.id).unwrap(), "SEALED-1");
        assert_eq!(store.read(&second.id).unwrap(), "SEALED-2");

        let listed = store.list();
        assert_eq!(listed.len(), 2);
        assert!(listed[0].created_at >= listed[1].created_at);
        assert_eq!(listed.iter().filter(|e| e.scope == ["macros"]).count(), 1);
        assert_eq!(store.index().version, INDEX_VERSION);
    }

    #[test]
    fn keeps_only_the_newest_bundles() {
        let store = VersionStore::new(temp_dir());
        for index in 0..(MAX_VERSIONS + 5) {
            store
                .insert(entry(&format!("v{index}"), index as u64), "SEALED")
                .unwrap();
        }
        let listed = store.list();
        assert_eq!(listed.len(), MAX_VERSIONS);
        assert_eq!(listed[0].id, format!("v{}", MAX_VERSIONS + 4));
        assert!(store.read("v0").is_err());
        assert_eq!(store.read(&format!("v{}", MAX_VERSIONS + 4)).unwrap(), "SEALED");
    }

    #[test]
    fn rejects_path_traversal() {
        let store = VersionStore::new(temp_dir());
        assert!(store.read("../secret").is_err());
        assert!(store.read("").is_err());
        assert!(store.read("a/b").is_err());
        assert!(store
            .insert(entry("../escape", 1), "SEALED")
            .is_err());
    }

    #[test]
    fn corrupt_index_falls_back_to_empty() {
        let store = VersionStore::new(temp_dir());
        fs::create_dir_all(store.dir()).unwrap();
        fs::write(store.dir().join(INDEX_FILE), "not json").unwrap();
        assert!(store.list().is_empty());
        store
            .add("SEALED", vec![], serde_json::json!({}), VersionSource::Local, "")
            .unwrap();
        assert_eq!(store.list().len(), 1);
    }
}
