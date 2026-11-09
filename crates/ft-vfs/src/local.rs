use std::path::{Path, PathBuf};

use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt};

use crate::{FileEntry, OpenMode, Vfs, VfsFile, VfsFuture};

#[derive(Default, Clone)]
pub struct LocalFs;

pub struct LocalFile {
    file: tokio::fs::File,
    dirty: bool,
}

impl VfsFile for LocalFile {
    fn read<'a>(&'a mut self, offset: u64, len: u32) -> VfsFuture<'a, Vec<u8>> {
        Box::pin(async move {
            self.file
                .seek(std::io::SeekFrom::Start(offset))
                .await
                .map_err(|e| e.to_string())?;
            let mut buffer = vec![0u8; len as usize];
            let read = self
                .file
                .read(&mut buffer)
                .await
                .map_err(|e| e.to_string())?;
            buffer.truncate(read);
            Ok(buffer)
        })
    }

    fn write<'a>(&'a mut self, offset: u64, data: &'a [u8]) -> VfsFuture<'a, ()> {
        Box::pin(async move {
            self.file
                .seek(std::io::SeekFrom::Start(offset))
                .await
                .map_err(|e| e.to_string())?;
            self.file.write_all(data).await.map_err(|e| e.to_string())?;
            self.dirty = true;
            Ok(())
        })
    }

    fn close<'a>(&'a mut self) -> VfsFuture<'a, ()> {
        Box::pin(async move {
            if self.dirty {
                self.file.flush().await.map_err(|e| e.to_string())?;
            }
            Ok(())
        })
    }
}

impl Vfs for LocalFs {
    fn list<'a>(&'a self, path: &'a str, show_hidden: bool) -> VfsFuture<'a, Vec<FileEntry>> {
        Box::pin(async move {
            let dir = normalize(path);
            let mut reader = tokio::fs::read_dir(&dir)
                .await
                .map_err(|e| format!("failed to read {}: {e}", dir.display()))?;
            let mut entries = Vec::new();
            while let Some(item) = reader.next_entry().await.map_err(|e| e.to_string())? {
                let name = item.file_name().to_string_lossy().into_owned();
                if !show_hidden && name.starts_with('.') {
                    continue;
                }
                let file_type = item.file_type().await.map_err(|e| e.to_string())?;
                let is_symlink = file_type.is_symlink();
                let metadata = match tokio::fs::metadata(item.path()).await {
                    Ok(metadata) => metadata,
                    Err(_) => item.metadata().await.map_err(|e| e.to_string())?,
                };
                let entry = entry_from_metadata(name, item.path(), &metadata, is_symlink);
                entries.push(entry);
            }
            entries.sort_by(|a, b| {
                b.is_dir
                    .cmp(&a.is_dir)
                    .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
            });
            Ok(entries)
        })
    }

    fn stat<'a>(&'a self, path: &'a str) -> VfsFuture<'a, FileEntry> {
        Box::pin(async move {
            let target = normalize(path);
            let metadata = tokio::fs::metadata(&target)
                .await
                .map_err(|e| format!("failed to stat {}: {e}", target.display()))?;
            let name = target
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_else(|| target.display().to_string());
            Ok(entry_from_metadata(name, target, &metadata, false))
        })
    }

    fn home<'a>(&'a self) -> VfsFuture<'a, String> {
        Box::pin(async move {
            Ok(std::env::var("HOME")
                .ok()
                .or_else(|| std::env::var("USERPROFILE").ok())
                .unwrap_or_else(|| "/".to_string()))
        })
    }

    fn mkdir<'a>(&'a self, path: &'a str) -> VfsFuture<'a, ()> {
        Box::pin(async move {
            tokio::fs::create_dir(normalize(path))
                .await
                .map_err(|e| e.to_string())
        })
    }

    fn remove_file<'a>(&'a self, path: &'a str) -> VfsFuture<'a, ()> {
        Box::pin(async move {
            tokio::fs::remove_file(normalize(path))
                .await
                .map_err(|e| e.to_string())
        })
    }

    fn remove_dir_all<'a>(&'a self, path: &'a str) -> VfsFuture<'a, ()> {
        Box::pin(async move {
            tokio::fs::remove_dir_all(normalize(path))
                .await
                .map_err(|e| e.to_string())
        })
    }

    fn rename<'a>(&'a self, from: &'a str, to: &'a str) -> VfsFuture<'a, ()> {
        Box::pin(async move {
            tokio::fs::rename(normalize(from), normalize(to))
                .await
                .map_err(|e| e.to_string())
        })
    }

    fn chmod<'a>(&'a self, path: &'a str, mode: u32) -> VfsFuture<'a, ()> {
        Box::pin(async move {
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let permissions = std::fs::Permissions::from_mode(mode & 0o7777);
                tokio::fs::set_permissions(normalize(path), permissions)
                    .await
                    .map_err(|e| e.to_string())
            }
            #[cfg(not(unix))]
            {
                let _ = (path, mode);
                Err("chmod is not supported on this platform".to_string())
            }
        })
    }

    fn set_times<'a>(&'a self, path: &'a str, mtime: u64) -> VfsFuture<'a, ()> {
        Box::pin(async move {
            let target = normalize(path);
            let stamp = std::time::UNIX_EPOCH + std::time::Duration::from_secs(mtime);
            let times = std::fs::FileTimes::new().set_modified(stamp);
            tokio::task::spawn_blocking(move || open_for_times(&target)?.set_times(times))
                .await
                .map_err(|e| e.to_string())?
                .map_err(|e| e.to_string())
        })
    }

    fn open<'a>(&'a self, path: &'a str, mode: OpenMode) -> VfsFuture<'a, Box<dyn VfsFile + Send>> {
        Box::pin(async move {
            let target = normalize(path);
            let mut options = tokio::fs::OpenOptions::new();
            match mode {
                OpenMode::Read => {
                    options.read(true);
                }
                OpenMode::Write { truncate } => {
                    options.write(true).create(true).truncate(truncate);
                }
            }
            let file = options
                .open(&target)
                .await
                .map_err(|e| format!("failed to open {}: {e}", target.display()))?;
            Ok(Box::new(LocalFile { file, dirty: false }) as Box<dyn VfsFile + Send>)
        })
    }
}

fn normalize(path: &str) -> PathBuf {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return PathBuf::from(".");
    }
    if trimmed == "~" {
        if let Ok(home) = std::env::var("HOME") {
            return PathBuf::from(home);
        }
    }
    if let Some(rest) = trimmed.strip_prefix("~/") {
        if let Ok(home) = std::env::var("HOME") {
            return Path::new(&home).join(rest);
        }
    }
    PathBuf::from(trimmed)
}

/// A handle that may write timestamps on files *and* directories. Unix accepts
/// a plain read handle; Windows refuses to open a directory unless the handle
/// asks for FILE_WRITE_ATTRIBUTES with FILE_FLAG_BACKUP_SEMANTICS.
fn open_for_times(path: &Path) -> std::io::Result<std::fs::File> {
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        const FILE_WRITE_ATTRIBUTES: u32 = 0x0100;
        const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;
        std::fs::OpenOptions::new()
            .access_mode(FILE_WRITE_ATTRIBUTES)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS)
            .open(path)
    }
    #[cfg(unix)]
    {
        std::fs::File::open(path)
    }
}

fn entry_from_metadata(
    name: String,
    path: PathBuf,
    metadata: &std::fs::Metadata,
    is_symlink: bool,
) -> FileEntry {
    #[cfg(unix)]
    let (mode, uid, gid) = {
        use std::os::unix::fs::MetadataExt;
        (metadata.mode(), metadata.uid(), metadata.gid())
    };
    #[cfg(not(unix))]
    let (mode, uid, gid) = (
        if metadata.permissions().readonly() {
            0o444
        } else {
            0o644
        },
        0,
        0,
    );
    let mtime = metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|duration| duration.as_secs())
        .unwrap_or(0);
    FileEntry {
        name,
        path: path.to_string_lossy().into_owned(),
        size: metadata.len(),
        mode,
        uid,
        gid,
        mtime,
        is_dir: metadata.is_dir(),
        is_symlink,
        // A local disk has no passwd to read, so the numeric ids stand.
        owner: None,
        group: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn writes_and_reads_local_file() {
        let dir = std::env::temp_dir().join(format!("ft-vfs-test-{}", std::process::id()));
        tokio::fs::create_dir_all(&dir).await.unwrap();
        let file = dir.join("sample.txt");
        let fs = LocalFs;

        let mut handle = fs
            .open(file.to_str().unwrap(), OpenMode::Write { truncate: true })
            .await
            .unwrap();
        handle.write(0, b"hello").await.unwrap();
        handle.close().await.unwrap();

        let mut handle = fs
            .open(file.to_str().unwrap(), OpenMode::Read)
            .await
            .unwrap();
        let data = handle.read(0, 16).await.unwrap();
        assert_eq!(data, b"hello");

        let entry = fs.stat(file.to_str().unwrap()).await.unwrap();
        assert_eq!(entry.size, 5);
        let _ = tokio::fs::remove_dir_all(&dir).await;
    }
}
