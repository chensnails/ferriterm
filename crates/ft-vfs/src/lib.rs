use std::future::Future;
use std::pin::Pin;

use serde::{Deserialize, Serialize};

mod local;
mod registry;
mod s3;
mod sftp;
mod timeutil;
mod util;
mod webdav;

pub use local::LocalFs;
pub use registry::{RemoteMountInfo, RemoteMounts};
pub use s3::S3Vfs;
pub use sftp::{SftpConnection, SftpRegistry, SftpVfs};
pub use webdav::WebdavVfs;

pub type VfsFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T, String>> + Send + 'a>>;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileEntry {
    pub name: String,
    pub path: String,
    pub size: u64,
    pub mode: u32,
    pub uid: u32,
    pub gid: u32,
    pub mtime: u64,
    pub is_dir: bool,
    pub is_symlink: bool,
    /// Owner name when the host let us read `/etc/passwd`; the numeric id is
    /// what the UI falls back to when it is absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpenMode {
    Read,
    Write { truncate: bool },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TextFile {
    pub content: String,
    pub mtime: u64,
    pub size: u64,
    pub truncated: bool,
}

pub trait VfsFile: Send {
    fn read<'a>(&'a mut self, offset: u64, len: u32) -> VfsFuture<'a, Vec<u8>>;
    fn write<'a>(&'a mut self, offset: u64, data: &'a [u8]) -> VfsFuture<'a, ()>;
    fn close<'a>(&'a mut self) -> VfsFuture<'a, ()>;
}

pub trait Vfs: Send + Sync {
    fn list<'a>(&'a self, path: &'a str, show_hidden: bool) -> VfsFuture<'a, Vec<FileEntry>>;
    fn stat<'a>(&'a self, path: &'a str) -> VfsFuture<'a, FileEntry>;
    fn home<'a>(&'a self) -> VfsFuture<'a, String>;
    fn mkdir<'a>(&'a self, path: &'a str) -> VfsFuture<'a, ()>;
    fn remove_file<'a>(&'a self, path: &'a str) -> VfsFuture<'a, ()>;
    fn remove_dir_all<'a>(&'a self, path: &'a str) -> VfsFuture<'a, ()>;
    fn rename<'a>(&'a self, from: &'a str, to: &'a str) -> VfsFuture<'a, ()>;
    fn chmod<'a>(&'a self, path: &'a str, mode: u32) -> VfsFuture<'a, ()>;
    /// Best-effort timestamp write-back, used when copying keeps the source's
    /// modification time. Backends that cannot store one may do nothing.
    fn set_times<'a>(&'a self, path: &'a str, mtime: u64) -> VfsFuture<'a, ()>;
    fn open<'a>(&'a self, path: &'a str, mode: OpenMode) -> VfsFuture<'a, Box<dyn VfsFile + Send>>;
}

/// Joins a directory and a child name. Both `/` and `\` count as separators
/// because the local side of a transfer may be a Windows path while the remote
/// side is POSIX; the result always uses `/`, which Windows accepts too.
pub fn join_path(base: &str, name: &str) -> String {
    if base.is_empty() {
        return name.to_string();
    }
    if name.is_empty() {
        return base.to_string();
    }
    let trimmed = base.trim_end_matches(['/', '\\']);
    if trimmed.is_empty() {
        return format!("/{name}");
    }
    format!("{trimmed}/{name}")
}

pub fn parent_path(path: &str) -> Option<String> {
    let trimmed = path.trim_end_matches(['/', '\\']);
    if trimmed.is_empty() || is_unc_root(trimmed) {
        return None;
    }
    let index = trimmed.rfind(['/', '\\']);
    match index {
        Some(0) => Some("/".to_string()),
        Some(index) => {
            let parent = &trimmed[..index];
            // `C:` on its own is not a directory; the root is `C:/`.
            if is_drive_prefix(parent) {
                Some(format!("{parent}/"))
            } else {
                Some(parent.to_string())
            }
        }
        None => None,
    }
}

fn is_drive_prefix(value: &str) -> bool {
    let bytes = value.as_bytes();
    bytes.len() == 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':'
}

/// `\\server\share` is the root of a UNC path: climbing out of it would leave
/// the share, so the caller must stop there.
fn is_unc_root(path: &str) -> bool {
    let rest = if let Some(rest) = path.strip_prefix(r"\\") {
        rest
    } else if let Some(rest) = path.strip_prefix("//") {
        rest
    } else {
        return false;
    };
    let mut parts = rest.split(['/', '\\']).filter(|part| !part.is_empty());
    parts.next().is_some() && parts.next().is_some() && parts.next().is_none()
}

pub async fn read_text(vfs: &dyn Vfs, path: &str, max_bytes: u64) -> Result<TextFile, String> {
    let entry = vfs.stat(path).await?;
    let mut file = vfs.open(path, OpenMode::Read).await?;
    let mut buffer = Vec::new();
    let mut offset = 0u64;
    let mut truncated = false;
    loop {
        let chunk = file.read(offset, 64 * 1024).await?;
        if chunk.is_empty() {
            break;
        }
        if buffer.len() as u64 + chunk.len() as u64 > max_bytes {
            let remaining = max_bytes.saturating_sub(buffer.len() as u64) as usize;
            buffer.extend_from_slice(&chunk[..remaining]);
            truncated = true;
            break;
        }
        offset += chunk.len() as u64;
        buffer.extend_from_slice(&chunk);
    }
    let _ = file.close().await;
    Ok(TextFile {
        content: String::from_utf8_lossy(&buffer).into_owned(),
        mtime: entry.mtime,
        size: entry.size,
        truncated,
    })
}

pub async fn write_text(
    vfs: &dyn Vfs,
    path: &str,
    content: &str,
    expected_mtime: Option<u64>,
) -> Result<u64, String> {
    if let Some(expected) = expected_mtime {
        if let Ok(current) = vfs.stat(path).await {
            if current.mtime != expected {
                return Err(
                    "the remote file changed since it was opened; reload before saving".to_string(),
                );
            }
        }
    }
    let mut file = vfs.open(path, OpenMode::Write { truncate: true }).await?;
    file.write(0, content.as_bytes()).await?;
    file.close().await?;
    Ok(vfs.stat(path).await?.mtime)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn joins_paths() {
        assert_eq!(join_path("/tmp", "a"), "/tmp/a");
        assert_eq!(join_path("/tmp/", "a"), "/tmp/a");
        assert_eq!(join_path("", "a"), "a");
        assert_eq!(join_path("/", "a"), "/a");
        // The local side of a transfer may be a Windows path.
        assert_eq!(join_path(r"C:\Users\me", "a"), "C:\\Users\\me/a");
        assert_eq!(join_path(r"C:\Users\me\", "a"), "C:\\Users\\me/a");
    }

    #[test]
    fn finds_parent() {
        assert_eq!(parent_path("/tmp/a"), Some("/tmp".to_string()));
        assert_eq!(parent_path("/tmp"), Some("/".to_string()));
        assert_eq!(parent_path("/"), None);
        assert_eq!(parent_path("a"), None);
        assert_eq!(parent_path(r"C:\Users\me"), Some(r"C:\Users".to_string()));
        assert_eq!(parent_path("C:\\Users"), Some("C:/".to_string()));
        assert_eq!(parent_path("C:/"), None);
    }

    #[test]
    fn unc_share_is_a_root() {
        assert_eq!(
            parent_path(r"\\server\share\dir"),
            Some(r"\\server\share".to_string())
        );
        assert_eq!(parent_path(r"\\server\share"), None);
        assert_eq!(parent_path(r"\\server\share\"), None);
    }

    /// The editor's "overwrite" button has no backend of its own: it saves with
    /// no expectation at all, and that is what must skip the stamp check.
    #[tokio::test]
    async fn a_stored_buffer_can_be_forced_over_a_newer_file() {
        let fs = LocalFs;
        let path =
            std::env::temp_dir().join(format!("ft-vfs-overwrite-{}.txt", std::process::id()));
        let path = path.to_string_lossy().into_owned();

        write_text(&fs, &path, "opened", None).await.unwrap();
        let opened_at = fs.stat(&path).await.unwrap().mtime;

        // Only equality matters, so any other stamp stands in for a file that
        // someone else saved in the meantime.
        let stale = opened_at.wrapping_add(1);
        let err = write_text(&fs, &path, "mine", Some(stale))
            .await
            .unwrap_err();
        assert!(err.contains("changed since"), "{err}");

        write_text(&fs, &path, "mine", None).await.unwrap();
        let text = read_text(&fs, &path, 1024).await.unwrap();
        assert_eq!(text.content, "mine");

        let _ = std::fs::remove_file(&path);
    }
}
