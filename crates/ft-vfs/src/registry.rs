//! In-memory registry of remote mounts (WebDAV/S3) exposed as VFS targets.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};

use crate::Vfs;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteMountInfo {
    pub id: String,
    pub label: String,
    pub kind: String,
}

#[derive(Default)]
pub struct RemoteMounts {
    inner: Mutex<BTreeMap<String, MountedVfs>>,
}

type MountedVfs = (RemoteMountInfo, Arc<dyn Vfs>);

impl RemoteMounts {
    pub fn add(&self, info: RemoteMountInfo, vfs: Arc<dyn Vfs>) -> Result<(), String> {
        let valid = (1..=64).contains(&info.id.len())
            && info
                .id
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_'));
        if !valid {
            return Err(format!("invalid mount id {:?}", info.id));
        }
        let mut mounts = self
            .inner
            .lock()
            .map_err(|_| "mount registry is unavailable".to_string())?;
        if mounts.contains_key(&info.id) {
            return Err(format!("mount {} already exists", info.id));
        }
        mounts.insert(info.id.clone(), (info, vfs));
        Ok(())
    }

    pub fn remove(&self, id: &str) -> bool {
        self.inner
            .lock()
            .map(|mut mounts| mounts.remove(id).is_some())
            .unwrap_or(false)
    }

    pub fn get(&self, id: &str) -> Option<Arc<dyn Vfs>> {
        self.inner
            .lock()
            .ok()
            .and_then(|mounts| mounts.get(id).map(|(_, vfs)| Arc::clone(vfs)))
    }

    pub fn list(&self) -> Vec<RemoteMountInfo> {
        self.inner
            .lock()
            .map(|mounts| mounts.values().map(|(info, _)| info.clone()).collect())
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::LocalFs;

    #[test]
    fn registers_and_looks_up_mounts() {
        let mounts = RemoteMounts::default();
        mounts
            .add(
                RemoteMountInfo {
                    id: "dav".to_string(),
                    label: "Team DAV".to_string(),
                    kind: "webdav".to_string(),
                },
                Arc::new(LocalFs),
            )
            .unwrap();
        assert!(mounts.get("dav").is_some());
        assert_eq!(mounts.list().len(), 1);
        assert!(mounts
            .add(
                RemoteMountInfo {
                    id: "dav".to_string(),
                    label: "Dup".to_string(),
                    kind: "webdav".to_string(),
                },
                Arc::new(LocalFs),
            )
            .is_err());
        assert!(mounts
            .add(
                RemoteMountInfo {
                    id: "../bad".to_string(),
                    label: "Bad".to_string(),
                    kind: "s3".to_string(),
                },
                Arc::new(LocalFs),
            )
            .is_err());
        assert!(mounts.remove("dav"));
        assert!(mounts.get("dav").is_none());
    }
}
