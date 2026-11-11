use std::sync::Arc;

use super::backup::{
    apply_backup_payload, backup_payload, normalize_scope, BackupPayload, RestoreMode, ALL_SCOPES,
};
use super::vault::Credentials;
use tauri::State;

#[derive(serde::Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum RemoteConfig {
    #[serde(rename_all = "camelCase")]
    Gist {
        token: String,
        #[serde(default)]
        gist_id: String,
        #[serde(default)]
        filename: String,
    },
    #[serde(rename_all = "camelCase")]
    Webdav {
        url: String,
        #[serde(default)]
        username: String,
        #[serde(default)]
        password: String,
        #[serde(default)]
        filename: String,
    },
}

impl RemoteConfig {
    /// Fills empty credentials from the vault so the webview never needs to
    /// hold the stored token or password.
    fn resolve_secrets(mut self, credentials: &Credentials) -> Result<Self, String> {
        match &mut self {
            RemoteConfig::Gist { token, .. } if token.trim().is_empty() => {
                *token = credentials
                    .get("sync:gist-token")
                    .map_err(|e| e.to_string())?
                    .unwrap_or_default();
            }
            RemoteConfig::Webdav { password, .. } if password.is_empty() => {
                *password = credentials
                    .get("sync:webdav-password")
                    .map_err(|e| e.to_string())?
                    .unwrap_or_default();
            }
            _ => {}
        }
        Ok(self)
    }

    fn gist(&self) -> Option<ft_sync::GistConfig> {
        match self {
            RemoteConfig::Gist {
                token,
                gist_id,
                filename,
            } => {
                let mut config = ft_sync::GistConfig::new(token, gist_id);
                if !filename.trim().is_empty() {
                    config.filename = filename.trim().to_string();
                }
                Some(config)
            }
            RemoteConfig::Webdav { .. } => None,
        }
    }

    fn webdav(&self) -> Option<ft_sync::WebdavConfig> {
        match self {
            RemoteConfig::Webdav {
                url,
                username,
                password,
                filename,
            } => {
                let mut config = ft_sync::WebdavConfig::new(url, username, password);
                if !filename.trim().is_empty() {
                    config.filename = filename.trim().to_string();
                }
                Some(config)
            }
            RemoteConfig::Gist { .. } => None,
        }
    }
}

/// Settings key holding the ETag of the last WebDAV bundle transfer. Empty
/// means the server publishes no ETags; a missing key means this device has
/// never transferred one.
const SYNC_ETAG_KEY: &str = "sync:webdav-etag";

#[tauri::command]
pub async fn sync_remote_test(
    credentials: State<'_, Credentials>,
    config: RemoteConfig,
) -> Result<String, String> {
    let config = config.resolve_secrets(&credentials)?;
    if let Some(gist) = config.gist() {
        ft_sync::gist_test(&gist)
            .await
            .map(|login| format!("GitHub user {login}"))
            .map_err(|e| e.to_string())
    } else if let Some(webdav) = config.webdav() {
        ft_sync::webdav_test(&webdav)
            .await
            .map_err(|e| e.to_string())
    } else {
        Err("unsupported sync provider".to_string())
    }
}

#[tauri::command]
pub async fn sync_remote_upload(
    backups: State<'_, ft_sync::BackupStore>,
    store: State<'_, Arc<ft_store::Store>>,
    credentials: State<'_, Credentials>,
    config: RemoteConfig,
    passphrase: String,
    scope: Vec<String>,
    force: bool,
) -> Result<ft_sync::UploadOutcome, String> {
    do_remote_upload(
        backups.inner(),
        store.inner(),
        credentials.inner(),
        config,
        passphrase,
        scope,
        force,
    )
    .await
}

/// Automatic runs read the passphrase from the local encrypted vault, so
/// on-change and scheduled syncs need no interaction. The `sync:` prefix is
/// excluded from backup payloads, so the passphrase never leaves this device.
#[tauri::command]
pub async fn sync_auto_upload(
    backups: State<'_, ft_sync::BackupStore>,
    store: State<'_, Arc<ft_store::Store>>,
    credentials: State<'_, Credentials>,
    config: RemoteConfig,
    scope: Vec<String>,
) -> Result<ft_sync::UploadOutcome, String> {
    let passphrase = credentials
        .get("sync:passphrase")
        .map_err(|e| e.to_string())?
        .unwrap_or_default();
    if passphrase.is_empty() {
        return Err("automatic sync passphrase is not saved".to_string());
    }
    do_remote_upload(
        backups.inner(),
        store.inner(),
        credentials.inner(),
        config,
        passphrase,
        scope,
        false,
    )
    .await
}

async fn do_remote_upload(
    backups: &ft_sync::BackupStore,
    store: &Arc<ft_store::Store>,
    credentials: &Credentials,
    config: RemoteConfig,
    passphrase: String,
    scope: Vec<String>,
    force: bool,
) -> Result<ft_sync::UploadOutcome, String> {
    let config = config.resolve_secrets(credentials)?;
    let scope = normalize_scope(&scope);
    let (payload, summary) = backup_payload(store.as_ref(), Some(credentials.as_ref()), &scope)?;
    let sealed = ft_sync::seal(&passphrase, &payload).map_err(|e| e.to_string())?;
    let summary_json = serde_json::to_value(&summary).map_err(|e| e.to_string())?;
    let is_gist = config.gist().is_some();

    // Upload first: a failed sync must not leave a version record claiming
    // the data reached the remote.
    let upload = if let Some(gist) = config.gist() {
        ft_sync::gist_upload(&gist, &sealed)
            .await
            .map_err(|e| e.to_string())?
    } else if let Some(webdav) = config.webdav() {
        let Some(upload) = webdav_upload_guarded(store, &webdav, &sealed, force).await? else {
            return Ok(ft_sync::UploadOutcome::Conflict);
        };
        upload
    } else {
        return Err("unsupported sync provider".to_string());
    };

    let source = if is_gist {
        ft_sync::VersionSource::Gist
    } else {
        ft_sync::VersionSource::Webdav
    };
    let entry = backups
        .record_version(&sealed, scope, summary_json, source, "remote sync")
        .map_err(|e| e.to_string())?;

    // WebDAV can hold many files, so mirror the version and update the remote
    // index. A failure here only costs the *remote* history: the local copy
    // recorded above still makes this version restorable.
    if let Some(webdav) = config.webdav() {
        if let Err(err) = push_webdav_version(backups, &webdav, &entry, &sealed).await {
            tracing::warn!(%err, "remote version index update failed");
        }
    }
    Ok(ft_sync::UploadOutcome::Uploaded(upload))
}

/// Uploads the main bundle, refusing to replace a remote copy another device
/// wrote since this device's last transfer. The last ETag is kept in settings;
/// an empty value means the server publishes none, so uploads stay
/// unconditional. With no record at all we first check the remote: if a bundle
/// is already there, another device owns it and an unattended upload must not
/// silently overwrite that device's data.
async fn webdav_upload_guarded(
    store: &Arc<ft_store::Store>,
    config: &ft_sync::WebdavConfig,
    sealed: &str,
    force: bool,
) -> Result<Option<ft_sync::RemoteUpload>, String> {
    let settings = store.list_settings().map_err(|e| e.to_string())?;
    let baseline = settings.get(SYNC_ETAG_KEY).map(String::as_str);
    let if_match = if force {
        None
    } else {
        baseline.filter(|v| !v.is_empty())
    };
    if !force && baseline.is_none() {
        let existing = ft_sync::webdav_etag(config, &config.filename)
            .await
            .map_err(|e| e.to_string())?;
        if existing.is_some() {
            return Ok(None);
        }
    }
    match ft_sync::webdav_put_conditional(config, &config.filename, sealed, if_match).await {
        Ok((upload, etag)) => {
            store
                .set_setting(SYNC_ETAG_KEY, etag.as_deref().unwrap_or_default())
                .map_err(|e| e.to_string())?;
            Ok(Some(upload))
        }
        Err(ft_sync::RemoteError::Conflict) => {
            tracing::warn!(
                filename = %config.filename,
                "sync refused: the remote bundle changed on another device"
            );
            Ok(None)
        }
        Err(err) => Err(err.to_string()),
    }
}

#[tauri::command]
pub async fn sync_remote_download(
    store: State<'_, Arc<ft_store::Store>>,
    credentials: State<'_, Credentials>,
    config: RemoteConfig,
    passphrase: String,
) -> Result<ft_store::ImportStats, String> {
    let config = config.resolve_secrets(&credentials)?;
    let sealed = if let Some(gist) = config.gist() {
        ft_sync::gist_download(&gist).await
    } else if let Some(webdav) = config.webdav() {
        ft_sync::webdav_download(&webdav).await
    } else {
        return Err("unsupported sync provider".to_string());
    }
    .map_err(|e| e.to_string())?;
    // Pulling the remote makes its current revision the baseline, so the next
    // upload compares against what we actually have here.
    if let Some(webdav) = config.webdav() {
        if let Ok(etag) = ft_sync::webdav_etag(&webdav, &webdav.filename).await {
            let _ = store.set_setting(SYNC_ETAG_KEY, etag.as_deref().unwrap_or_default());
        }
    }
    let plain = ft_sync::open(&passphrase, &sealed).map_err(|e| e.to_string())?;
    let parsed: BackupPayload =
        serde_json::from_slice(&plain).map_err(|e| format!("invalid backup payload: {e}"))?;
    apply_backup_payload(&store, &credentials, &parsed, RestoreMode::Merge)
}

/// Mirrors a local version entry to WebDAV: one timestamped bundle plus the
/// shared JSON index. Older remote bundles are pruned to the same limit as the
/// local history.
async fn push_webdav_version(
    backups: &ft_sync::BackupStore,
    config: &ft_sync::WebdavConfig,
    entry: &ft_sync::VersionEntry,
    sealed: &str,
) -> Result<(), String> {
    use ft_sync::remote;

    remote::webdav_put_version(config, &entry.id, sealed)
        .await
        .map_err(|e| e.to_string())?;
    // A missing or damaged remote index is rebuilt from local history rather
    // than wiping versions another device may have uploaded.
    let mut index = remote::webdav_read_index(config)
        .await
        .unwrap_or_else(|_| backups.versions().index());
    index.version = ft_sync::INDEX_VERSION;
    index.entries.retain(|item| item.id != entry.id);
    index.entries.push(entry.clone());
    let removed = index.keep_newest(ft_sync::MAX_VERSIONS);
    remote::webdav_write_index(config, &index)
        .await
        .map_err(|e| e.to_string())?;
    for id in removed {
        let _ = remote::webdav_delete_version(config, &id).await;
    }
    Ok(())
}

/// Version records for the history list. WebDAV returns its remote index (so
/// a fresh install can see every version); Gist and an unreachable WebDAV fall
/// back to the local history.
#[tauri::command]
pub async fn sync_versions_list(
    backups: State<'_, ft_sync::BackupStore>,
    credentials: State<'_, Credentials>,
    config: RemoteConfig,
) -> Result<Vec<ft_sync::VersionEntry>, String> {
    let config = config.resolve_secrets(&credentials)?;
    let local = backups.versions().list();
    if let Some(webdav) = config.webdav() {
        // The settings pane opens this on mount; an offline server must not
        // hold it for the transport's full 30s timeout.
        match tokio::time::timeout(
            std::time::Duration::from_secs(5),
            ft_sync::remote::webdav_read_index(&webdav),
        )
        .await
        {
            Ok(Ok(index)) if !index.entries.is_empty() => Ok(index.entries),
            _ => Ok(local),
        }
    } else {
        Ok(local)
    }
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VersionRestoreResult {
    stats: ft_store::ImportStats,
    restore_point_id: String,
    restore_point_at: u64,
}

/// Rewinds managed data to a chosen version. Before touching anything the
/// current state is sealed as a restore-point version, so the rollback itself
/// is reversible from the same list.
#[tauri::command]
pub async fn sync_version_restore(
    backups: State<'_, ft_sync::BackupStore>,
    store: State<'_, Arc<ft_store::Store>>,
    credentials: State<'_, Credentials>,
    config: RemoteConfig,
    passphrase: String,
    version_id: String,
) -> Result<VersionRestoreResult, String> {
    let config = config.resolve_secrets(&credentials)?;
    let versions = backups.versions();
    // A one-click rollback should not require retyping the passphrase when it
    // is already stored for automatic sync.
    let passphrase = if passphrase.is_empty() {
        credentials
            .get("sync:passphrase")
            .map_err(|e| e.to_string())?
            .unwrap_or_default()
    } else {
        passphrase
    };
    if passphrase.is_empty() {
        return Err("backup passphrase is required".to_string());
    }

    let mut entry = versions
        .list()
        .into_iter()
        .find(|item| item.id == version_id);
    if entry.is_none() {
        if let Some(webdav) = config.webdav() {
            if let Ok(index) = ft_sync::remote::webdav_read_index(&webdav).await {
                entry = index.entries.into_iter().find(|item| item.id == version_id);
            }
        }
    }
    let entry = entry.ok_or_else(|| "version not found".to_string())?;

    let sealed = match versions.read(&entry.id) {
        Ok(text) => text,
        Err(_) => {
            let webdav = config
                .webdav()
                .ok_or_else(|| "version bundle is not available locally".to_string())?;
            ft_sync::remote::webdav_get_version(&webdav, &entry.id)
                .await
                .map_err(|e| e.to_string())?
        }
    };

    // Validate the bundle before snapshotting: a wrong passphrase must not
    // leave a stray restore point behind.
    let plain = ft_sync::open(&passphrase, &sealed).map_err(|e| e.to_string())?;
    let parsed: BackupPayload =
        serde_json::from_slice(&plain).map_err(|e| format!("invalid backup payload: {e}"))?;

    let all: Vec<String> = ALL_SCOPES.iter().map(|item| item.to_string()).collect();
    let (current, current_summary) = backup_payload(&store, Some(&credentials), &all)?;
    let current_sealed = ft_sync::seal(&passphrase, &current).map_err(|e| e.to_string())?;
    let point = versions
        .add(
            &current_sealed,
            all,
            serde_json::to_value(&current_summary).map_err(|e| e.to_string())?,
            ft_sync::VersionSource::RestorePoint,
            format!("before {}", entry.id),
        )
        .map_err(|e| e.to_string())?;

    let stats = apply_backup_payload(&store, &credentials, &parsed, RestoreMode::Replace)?;
    Ok(VersionRestoreResult {
        stats,
        restore_point_id: point.id,
        restore_point_at: point.created_at,
    })
}
