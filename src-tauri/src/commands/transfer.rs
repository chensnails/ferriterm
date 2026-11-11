use std::sync::Arc;

use super::session::SshRuntimeState;
use ft_core::SessionRegistry;
use tauri::State;

async fn resolve_vfs(
    sessions: &SessionRegistry,
    runtime: &SshRuntimeState,
    session_id: Option<&str>,
) -> Result<Arc<dyn ft_vfs::Vfs>, String> {
    match session_id {
        None | Some("") => Ok(Arc::new(ft_vfs::LocalFs)),
        Some(id) => {
            if let Some(mount) = id.strip_prefix('@') {
                return remote_mounts()
                    .get(mount)
                    .ok_or_else(|| format!("remote mount {mount} is not connected"));
            }
            let connection = runtime.sftp.get(sessions, id).await?;
            Ok(Arc::new(ft_vfs::SftpVfs::new(connection)))
        }
    }
}

fn remote_mounts() -> &'static ft_vfs::RemoteMounts {
    static MOUNTS: std::sync::OnceLock<ft_vfs::RemoteMounts> = std::sync::OnceLock::new();
    MOUNTS.get_or_init(ft_vfs::RemoteMounts::default)
}

/// Rebuilds an end point from the session id a job was saved with. This is what
/// lets a job resumed after a restart find its host again.
struct SftpResolver {
    sessions: Arc<ft_core::SessionRegistry>,
    sftp: Arc<ft_vfs::SftpRegistry>,
}

impl ft_transfer::EndpointResolver for SftpResolver {
    fn resolve<'a>(
        &'a self,
        session: Option<&'a str>,
    ) -> ft_vfs::VfsFuture<'a, (Arc<dyn ft_vfs::Vfs>, bool)> {
        Box::pin(async move {
            match session {
                None | Some("") => Ok((Arc::new(ft_vfs::LocalFs) as Arc<dyn ft_vfs::Vfs>, false)),
                Some(id) => {
                    if let Some(mount) = id.strip_prefix('@') {
                        let vfs = remote_mounts()
                            .get(mount)
                            .ok_or_else(|| format!("remote mount {mount} is not connected"))?;
                        return Ok((vfs, true));
                    }
                    let connection = self.sftp.get(self.sessions.as_ref(), id).await?;
                    let vfs: Arc<dyn ft_vfs::Vfs> = Arc::new(ft_vfs::SftpVfs::new(connection));
                    Ok((vfs, true))
                }
            }
        })
    }
}

/// Keeps the queue in the same database as the hosts. Failure is swallowed on
/// purpose: a full disk must not stop a transfer that is already running.
struct StoreJobLog {
    store: Arc<ft_store::Store>,
}

impl ft_transfer::JobLog for StoreJobLog {
    fn save<'a>(&'a self, job: &'a ft_transfer::TransferJob) -> ft_vfs::VfsFuture<'a, ()> {
        Box::pin(async move {
            let Ok(payload) = serde_json::to_string(job) else {
                return Ok(());
            };
            let store = self.store.clone();
            let id = job.id.clone();
            let _ =
                tokio::task::spawn_blocking(move || store.save_transfer_job(&id, &payload)).await;
            Ok(())
        })
    }

    fn delete<'a>(&'a self, id: &'a str) -> ft_vfs::VfsFuture<'a, ()> {
        Box::pin(async move {
            let store = self.store.clone();
            let id = id.to_string();
            let _ = tokio::task::spawn_blocking(move || store.delete_transfer_job(&id)).await;
            Ok(())
        })
    }
}

pub fn transfer_manager(
    store: Arc<ft_store::Store>,
    sessions: ft_core::SessionRegistry,
    sftp: Arc<ft_vfs::SftpRegistry>,
) -> ft_transfer::TransferManager {
    ft_transfer::TransferManager::with_hooks(
        ft_transfer::DEFAULT_CONCURRENCY,
        Some(Arc::new(SftpResolver {
            sessions: Arc::new(sessions),
            sftp,
        })),
        Some(Arc::new(StoreJobLog { store })),
    )
}

/// The queue a previous run left behind. Finished rows are pruned as they are
/// read, so the table only ever holds work that still needs doing.
pub fn restored_transfers(store: &ft_store::Store) -> Vec<ft_transfer::TransferJob> {
    let mut jobs = Vec::new();
    for payload in store.list_transfer_jobs().unwrap_or_default() {
        match serde_json::from_str::<ft_transfer::TransferJob>(&payload) {
            Ok(job) if !job.status.is_finished() => jobs.push(job),
            Ok(job) => {
                let _ = store.delete_transfer_job(&job.id);
            }
            // A row we cannot read is a row we cannot act on either.
            Err(_) => {}
        }
    }
    jobs
}

#[derive(serde::Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum RemoteMountConfig {
    #[serde(rename_all = "camelCase")]
    Webdav {
        url: String,
        #[serde(default)]
        username: String,
        #[serde(default)]
        password: String,
    },
    #[serde(rename = "s3", rename_all = "camelCase")]
    S3 {
        endpoint: String,
        #[serde(default)]
        region: String,
        bucket: String,
        access_key: String,
        secret_key: String,
        #[serde(default)]
        prefix: String,
    },
    #[serde(rename_all = "camelCase")]
    Ftp {
        host: String,
        #[serde(default)]
        port: Option<u16>,
        #[serde(default)]
        username: String,
        #[serde(default)]
        password: String,
        /// Directory that `/` maps to; empty means the login directory.
        #[serde(default)]
        path: String,
    },
}

#[tauri::command]
pub fn remote_mount_add(
    id: String,
    label: String,
    config: RemoteMountConfig,
) -> Result<(), String> {
    let (kind, vfs): (String, Arc<dyn ft_vfs::Vfs>) = match config {
        RemoteMountConfig::Webdav {
            url,
            username,
            password,
        } => (
            "webdav".to_string(),
            Arc::new(ft_vfs::WebdavVfs::new(&url, &username, &password)?),
        ),
        RemoteMountConfig::S3 {
            endpoint,
            region,
            bucket,
            access_key,
            secret_key,
            prefix,
        } => (
            "s3".to_string(),
            Arc::new(ft_vfs::S3Vfs::new(
                &endpoint,
                &region,
                &bucket,
                &access_key,
                &secret_key,
                &prefix,
            )?),
        ),
        RemoteMountConfig::Ftp {
            host,
            port,
            username,
            password,
            path,
        } => (
            "ftp".to_string(),
            // The connection is lazy: a wrong host shows up in the file pane
            // instead of blocking the dialog.
            Arc::new(ft_vfs::FtpVfs::new(
                &host, port, &username, &password, &path,
            )?),
        ),
    };
    remote_mounts().add(
        ft_vfs::RemoteMountInfo {
            id: id.clone(),
            label: if label.trim().is_empty() { id } else { label },
            kind,
        },
        vfs,
    )
}

#[tauri::command]
pub fn remote_mount_remove(id: String) -> bool {
    remote_mounts().remove(&id)
}

#[tauri::command]
pub fn remote_mount_list() -> Vec<ft_vfs::RemoteMountInfo> {
    remote_mounts().list()
}

#[tauri::command]
pub async fn vfs_list(
    sessions: State<'_, SessionRegistry>,
    runtime: State<'_, SshRuntimeState>,
    session_id: Option<String>,
    path: String,
    show_hidden: Option<bool>,
) -> Result<Vec<ft_vfs::FileEntry>, String> {
    let vfs = resolve_vfs(&sessions, &runtime, session_id.as_deref()).await?;
    vfs.list(&path, show_hidden.unwrap_or(false)).await
}

#[tauri::command]
pub async fn vfs_stat(
    sessions: State<'_, SessionRegistry>,
    runtime: State<'_, SshRuntimeState>,
    session_id: Option<String>,
    path: String,
) -> Result<ft_vfs::FileEntry, String> {
    let vfs = resolve_vfs(&sessions, &runtime, session_id.as_deref()).await?;
    vfs.stat(&path).await
}

#[tauri::command]
pub async fn vfs_home(
    sessions: State<'_, SessionRegistry>,
    runtime: State<'_, SshRuntimeState>,
    session_id: Option<String>,
) -> Result<String, String> {
    let vfs = resolve_vfs(&sessions, &runtime, session_id.as_deref()).await?;
    vfs.home().await
}

#[tauri::command]
pub async fn vfs_mkdir(
    sessions: State<'_, SessionRegistry>,
    runtime: State<'_, SshRuntimeState>,
    session_id: Option<String>,
    path: String,
) -> Result<(), String> {
    let vfs = resolve_vfs(&sessions, &runtime, session_id.as_deref()).await?;
    vfs.mkdir(&path).await
}

#[tauri::command]
pub async fn vfs_remove(
    sessions: State<'_, SessionRegistry>,
    runtime: State<'_, SshRuntimeState>,
    session_id: Option<String>,
    path: String,
    recursive: bool,
) -> Result<(), String> {
    let vfs = resolve_vfs(&sessions, &runtime, session_id.as_deref()).await?;
    if recursive {
        vfs.remove_dir_all(&path).await
    } else {
        vfs.remove_file(&path).await
    }
}

#[tauri::command]
pub async fn vfs_rename(
    sessions: State<'_, SessionRegistry>,
    runtime: State<'_, SshRuntimeState>,
    session_id: Option<String>,
    from: String,
    to: String,
) -> Result<(), String> {
    let vfs = resolve_vfs(&sessions, &runtime, session_id.as_deref()).await?;
    vfs.rename(&from, &to).await
}

#[tauri::command]
pub async fn vfs_chmod(
    sessions: State<'_, SessionRegistry>,
    runtime: State<'_, SshRuntimeState>,
    session_id: Option<String>,
    path: String,
    mode: u32,
) -> Result<(), String> {
    let vfs = resolve_vfs(&sessions, &runtime, session_id.as_deref()).await?;
    vfs.chmod(&path, mode).await
}

#[tauri::command]
pub async fn vfs_read_text(
    sessions: State<'_, SessionRegistry>,
    runtime: State<'_, SshRuntimeState>,
    session_id: Option<String>,
    path: String,
    max_bytes: Option<u64>,
) -> Result<ft_vfs::TextFile, String> {
    let vfs = resolve_vfs(&sessions, &runtime, session_id.as_deref()).await?;
    ft_vfs::read_text(vfs.as_ref(), &path, max_bytes.unwrap_or(2 * 1024 * 1024)).await
}

#[tauri::command]
pub async fn vfs_write_text(
    sessions: State<'_, SessionRegistry>,
    runtime: State<'_, SshRuntimeState>,
    session_id: Option<String>,
    path: String,
    content: String,
    expected_mtime: Option<u64>,
) -> Result<u64, String> {
    let vfs = resolve_vfs(&sessions, &runtime, session_id.as_deref()).await?;
    ft_vfs::write_text(vfs.as_ref(), &path, &content, expected_mtime).await
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransferRequest {
    pub id: String,
    #[serde(default)]
    pub source_session: Option<String>,
    pub source_path: String,
    #[serde(default)]
    pub target_session: Option<String>,
    pub target_path: String,
    #[serde(default)]
    pub directory: bool,
}

fn is_remote(session: &Option<String>) -> bool {
    session.as_deref().is_some_and(|id| !id.is_empty())
}

#[tauri::command]
pub async fn transfer_enqueue(
    sessions: State<'_, SessionRegistry>,
    runtime: State<'_, SshRuntimeState>,
    request: TransferRequest,
) -> Result<String, String> {
    let source_remote = is_remote(&request.source_session);
    let target_remote = is_remote(&request.target_session);
    let mut source = ft_transfer::Endpoint::new(
        resolve_vfs(&sessions, &runtime, request.source_session.as_deref()).await?,
        request.source_path,
    )
    .with_session(request.source_session.clone());
    if source_remote {
        source = source.as_remote();
    }
    let mut target = ft_transfer::Endpoint::new(
        resolve_vfs(&sessions, &runtime, request.target_session.as_deref()).await?,
        request.target_path,
    )
    .with_session(request.target_session.clone());
    if target_remote {
        target = target.as_remote();
    }
    if request.directory {
        runtime
            .transfers
            .enqueue_tree(request.id, source, target)
            .await
    } else {
        let size = source.bound()?.stat(&source.path).await?.size;
        Ok(runtime
            .transfers
            .enqueue_file(request.id, source, target, size)
            .await)
    }
}

#[tauri::command]
pub async fn transfer_set_concurrency(
    runtime: State<'_, SshRuntimeState>,
    concurrency: usize,
) -> Result<usize, String> {
    runtime.transfers.set_concurrency(concurrency);
    Ok(runtime.transfers.concurrency())
}

#[tauri::command]
pub async fn transfer_list(
    runtime: State<'_, SshRuntimeState>,
) -> Result<Vec<ft_transfer::TransferJob>, String> {
    Ok(runtime.transfers.list().await)
}

#[tauri::command]
pub async fn transfer_pause(runtime: State<'_, SshRuntimeState>, id: String) -> Result<(), String> {
    runtime.transfers.pause(&id).await;
    Ok(())
}

#[tauri::command]
pub async fn transfer_resume(
    runtime: State<'_, SshRuntimeState>,
    id: String,
) -> Result<(), String> {
    runtime.transfers.resume(&id).await;
    Ok(())
}

#[tauri::command]
pub async fn transfer_cancel(
    runtime: State<'_, SshRuntimeState>,
    id: String,
) -> Result<(), String> {
    runtime.transfers.cancel(&id).await;
    Ok(())
}

#[tauri::command]
pub async fn transfer_retry(runtime: State<'_, SshRuntimeState>, id: String) -> Result<(), String> {
    runtime.transfers.retry(&id).await;
    Ok(())
}

#[tauri::command]
pub async fn transfer_remove(
    runtime: State<'_, SshRuntimeState>,
    id: String,
) -> Result<(), String> {
    runtime.transfers.remove(&id).await;
    Ok(())
}

#[tauri::command]
pub async fn transfer_clear_finished(runtime: State<'_, SshRuntimeState>) -> Result<(), String> {
    runtime.transfers.clear_finished().await;
    Ok(())
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn sync_preview(
    sessions: State<'_, SessionRegistry>,
    runtime: State<'_, SshRuntimeState>,
    source_session: Option<String>,
    source_path: String,
    target_session: Option<String>,
    target_path: String,
    two_way: bool,
    delete_extra: bool,
) -> Result<ft_transfer::SyncPlan, String> {
    let source = resolve_vfs(&sessions, &runtime, source_session.as_deref()).await?;
    let target = resolve_vfs(&sessions, &runtime, target_session.as_deref()).await?;
    if two_way {
        return ft_transfer::preview_sync_two_way(
            source.as_ref(),
            &source_path,
            target.as_ref(),
            &target_path,
        )
        .await;
    }
    ft_transfer::preview_sync(
        source.as_ref(),
        &source_path,
        target.as_ref(),
        &target_path,
        delete_extra,
    )
    .await
}

#[tauri::command]
pub async fn sync_apply(
    sessions: State<'_, SessionRegistry>,
    runtime: State<'_, SshRuntimeState>,
    source_session: Option<String>,
    target_session: Option<String>,
    plan: ft_transfer::SyncPlan,
) -> Result<ft_transfer::SyncPlan, String> {
    let source = ft_transfer::Endpoint::new(
        resolve_vfs(&sessions, &runtime, source_session.as_deref()).await?,
        plan.source_root.clone(),
    )
    .with_remote(source_session.as_deref().is_some_and(|id| !id.is_empty()))
    .with_session(source_session.clone());
    let target = ft_transfer::Endpoint::new(
        resolve_vfs(&sessions, &runtime, target_session.as_deref()).await?,
        plan.target_root.clone(),
    )
    .with_remote(target_session.as_deref().is_some_and(|id| !id.is_empty()))
    .with_session(target_session.clone());
    let prefix = format!(
        "sync-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0)
    );
    ft_transfer::apply_sync(&runtime.transfers, &source, &target, &plan, &prefix).await?;
    Ok(plan)
}
