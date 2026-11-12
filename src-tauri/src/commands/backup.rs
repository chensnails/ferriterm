use std::sync::Arc;

use super::keys::normalize_expiry;
use super::vault::{infer_host_id, infer_kind, key_passphrase_ref, key_ref, Credentials};
use tauri::State;

/// Backup payload v3. Version 1 bundles (hosts/tags/secrets only) still parse;
/// every added section defaults to empty, so old `.ftbak` files stay importable.
/// `scope` records which of the five data types the bundle actually carries.
const BACKUP_PAYLOAD_VERSION: u32 = 3;

pub const SCOPE_HOSTS: &str = "hosts";
pub const SCOPE_SECRETS: &str = "secrets";
pub const SCOPE_HIGHLIGHTS: &str = "highlights";
pub const SCOPE_MACROS: &str = "macros";
pub const SCOPE_SHORTCUTS: &str = "shortcuts";

pub const ALL_SCOPES: [&str; 5] = [
    SCOPE_HOSTS,
    SCOPE_SECRETS,
    SCOPE_HIGHLIGHTS,
    SCOPE_MACROS,
    SCOPE_SHORTCUTS,
];

/// Per-type counts shown in version records without opening the bundle.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupSummary {
    pub hosts: usize,
    pub tags: usize,
    pub secrets: usize,
    pub ssh_keys: usize,
    pub macros: usize,
    pub highlights: usize,
    pub shortcuts: usize,
}

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct BackupPayload {
    #[serde(default = "payload_version")]
    version: u32,
    #[serde(default)]
    exported_at: u64,
    #[serde(default)]
    scope: Vec<String>,
    #[serde(default)]
    tags: Vec<ft_store::Tag>,
    #[serde(default)]
    hosts: Vec<ft_store::Host>,
    #[serde(default)]
    forward_rules: Vec<ft_store::ForwardRule>,
    #[serde(default)]
    secrets: Vec<BackupSecret>,
    #[serde(default)]
    ssh_keys: Vec<BackupSshKey>,
    /// Named password credential metadata; plaintext travels in `secrets`.
    #[serde(default)]
    password_credentials: Vec<ft_store::PasswordCredential>,
    #[serde(default)]
    command_groups: Vec<ft_store::CommandGroup>,
    /// Raw `keywordHighlights` setting (JSON array of rules).
    #[serde(default)]
    highlights: String,
    /// Raw `shortcuts` setting (JSON object of bindings).
    #[serde(default)]
    shortcuts: String,
    #[serde(default)]
    summary: BackupSummary,
}

fn payload_version() -> u32 {
    1
}

/// Managed SSH key plus the private material, when the vault holds it.
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct BackupSshKey {
    #[serde(flatten)]
    key: ft_store::SshKey,
    #[serde(default)]
    private_key: String,
    #[serde(default)]
    passphrase: String,
}

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct BackupSecret {
    secret_ref: String,
    kind: String,
    label: String,
    host_id: Option<String>,
    secret: String,
}

fn count_json_array(raw: &str) -> usize {
    serde_json::from_str::<serde_json::Value>(raw)
        .ok()
        .and_then(|value| value.as_array().map(|items| items.len()))
        .unwrap_or(0)
}

fn count_json_bindings(raw: &str) -> usize {
    serde_json::from_str::<serde_json::Value>(raw)
        .ok()
        .and_then(|value| {
            value.as_object().map(|map| {
                map.values()
                    .filter(|item| item.as_str().is_some_and(|text| !text.trim().is_empty()))
                    .count()
            })
        })
        .unwrap_or(0)
}
#[tauri::command]
pub fn export_backup(
    backups: State<'_, ft_sync::BackupStore>,
    store: State<'_, Arc<ft_store::Store>>,
    credentials: State<'_, Credentials>,
    passphrase: String,
    scope: Vec<String>,
) -> Result<String, String> {
    let scope = normalize_scope(&scope);
    let (payload, summary) = backup_payload(&store, Some(&credentials), &scope)?;
    let (path, _) = backups
        .export_versioned(
            &passphrase,
            &payload,
            scope,
            serde_json::to_value(&summary).map_err(|e| e.to_string())?,
            "manual export",
        )
        .map_err(|e| e.to_string())?;
    Ok(path.display().to_string())
}

pub(crate) fn normalize_scope(scope: &[String]) -> Vec<String> {
    scope
        .iter()
        .filter(|item| ALL_SCOPES.contains(&item.as_str()))
        .cloned()
        .collect()
}

/// Builds the sealed-ready JSON payload for the requested scopes, plus the
/// per-type counts used in version records. Unknown scope keys are ignored so
/// an older webview cannot make the export fail.
pub(crate) fn backup_payload(
    store: &ft_store::Store,
    credentials: Option<&ft_security::CredentialStore>,
    scope: &[String],
) -> Result<(Vec<u8>, BackupSummary), String> {
    // Drop unknown keys here as well as in `wants`, so the recorded scope in
    // the bundle always describes exactly the sections it carries.
    let scope = normalize_scope(scope);
    let wants = |key: &str| scope.iter().any(|item| item == key);
    let mut payload = BackupPayload {
        version: BACKUP_PAYLOAD_VERSION,
        exported_at: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0),
        scope: scope.clone(),
        tags: Vec::new(),
        hosts: Vec::new(),
        forward_rules: Vec::new(),
        secrets: Vec::new(),
        ssh_keys: Vec::new(),
        password_credentials: Vec::new(),
        command_groups: Vec::new(),
        highlights: String::new(),
        shortcuts: String::new(),
        summary: BackupSummary::default(),
    };

    if wants(SCOPE_HOSTS) {
        payload.hosts = store.list_hosts().map_err(|e| e.to_string())?;
        payload.tags = store.list_tags().map_err(|e| e.to_string())?;
        payload.forward_rules = store.list_forward_rules(None).map_err(|e| e.to_string())?;
    }

    if wants(SCOPE_SECRETS) {
        if let Some(credentials) = credentials {
            let labels = store
                .list_vault_entries()
                .map_err(|e| e.to_string())?
                .into_iter()
                .map(|entry| (entry.secret_ref.clone(), entry))
                .collect::<std::collections::HashMap<_, _>>();
            for reference in credentials.refs() {
                if reference.starts_with("sync:") {
                    continue;
                }
                let Some(secret) = credentials.get(&reference).map_err(|e| e.to_string())? else {
                    continue;
                };
                let meta = labels.get(&reference);
                payload.secrets.push(BackupSecret {
                    kind: meta
                        .map(|entry| entry.kind.clone())
                        .unwrap_or_else(|| infer_kind(&reference).to_string()),
                    label: meta
                        .map(|entry| entry.label.clone())
                        .unwrap_or_else(|| reference.clone()),
                    host_id: meta
                        .and_then(|entry| entry.host_id.clone())
                        .or_else(|| infer_host_id(&reference)),
                    secret_ref: reference,
                    secret,
                });
            }
            for key in store.list_ssh_keys().map_err(|e| e.to_string())? {
                let private_key = credentials
                    .get(&key_ref(&key.id))
                    .map_err(|e| e.to_string())?
                    .unwrap_or_default();
                let passphrase = credentials
                    .get(&key_passphrase_ref(&key.id))
                    .map_err(|e| e.to_string())?
                    .unwrap_or_default();
                payload.ssh_keys.push(BackupSshKey {
                    key,
                    private_key,
                    passphrase,
                });
            }
            payload.password_credentials = store
                .list_password_credentials()
                .map_err(|e| e.to_string())?;
        }
    }

    if wants(SCOPE_MACROS) {
        payload.command_groups = store.list_command_groups().map_err(|e| e.to_string())?;
    }

    if wants(SCOPE_HIGHLIGHTS) {
        payload.highlights = store
            .get_setting("keywordHighlights")
            .map_err(|e| e.to_string())?
            .unwrap_or_default();
    }

    if wants(SCOPE_SHORTCUTS) {
        payload.shortcuts = store
            .get_setting("shortcuts")
            .map_err(|e| e.to_string())?
            .unwrap_or_default();
    }

    payload.summary = BackupSummary {
        hosts: payload.hosts.len(),
        tags: payload.tags.len(),
        secrets: payload.secrets.len(),
        ssh_keys: payload.ssh_keys.len(),
        macros: payload
            .command_groups
            .iter()
            .map(|group| group.commands.len())
            .sum(),
        highlights: count_json_array(&payload.highlights),
        shortcuts: count_json_bindings(&payload.shortcuts),
    };

    let bytes = serde_json::to_vec(&payload).map_err(|e| e.to_string())?;
    Ok((bytes, payload.summary))
}

#[tauri::command]
pub fn import_backup(
    store: State<'_, Arc<ft_store::Store>>,
    credentials: State<'_, Credentials>,
    payload: String,
    passphrase: String,
) -> Result<ft_store::ImportStats, String> {
    let plain = ft_sync::open(&passphrase, &payload).map_err(|e| e.to_string())?;
    let parsed: BackupPayload =
        serde_json::from_slice(&plain).map_err(|e| format!("invalid backup payload: {e}"))?;
    apply_backup_payload(&store, &credentials, &parsed, RestoreMode::Merge)
}

/// Merge keeps local data and only adds what is missing; Replace (used by
/// version rollback) rewinds each requested scope to the snapshot's state.
#[derive(PartialEq, Clone, Copy)]
pub(crate) enum RestoreMode {
    Merge,
    Replace,
}

pub(crate) fn apply_backup_payload(
    store: &ft_store::Store,
    credentials: &ft_security::CredentialStore,
    parsed: &BackupPayload,
    mode: RestoreMode,
) -> Result<ft_store::ImportStats, String> {
    // A bundle only carries the scopes the user selected. A rollback must
    // rewind exactly those scopes, otherwise restoring a highlights-only
    // snapshot would wipe every host, credential and macro on this install.
    // v1 bundles have no `scope` field and only ever contained hosts + secrets.
    let requested = |key: &str| {
        if parsed.version < 2 {
            key == SCOPE_HOSTS || key == SCOPE_SECRETS
        } else {
            // A v2 bundle with no scopes intentionally changes nothing.
            parsed.scope.iter().any(|item| item == key)
        }
    };
    let replace = mode == RestoreMode::Replace;
    let mut stats = ft_store::ImportStats::default();

    if requested(SCOPE_HOSTS) {
        let host_stats = if replace {
            store
                .replace_hosts(&parsed.hosts, &parsed.tags)
                .map_err(|e| e.to_string())?
        } else if !parsed.hosts.is_empty() || !parsed.tags.is_empty() {
            store
                .import_snapshot(&parsed.hosts, &parsed.tags)
                .map_err(|e| e.to_string())?
        } else {
            ft_store::ImportStats::default()
        };
        stats.hosts_added = host_stats.hosts_added;
        stats.hosts_skipped = host_stats.hosts_skipped;
        stats.hosts_updated = host_stats.hosts_updated;
        stats.hosts_removed = host_stats.hosts_removed;
        stats.tags_added = host_stats.tags_added;

        if replace {
            // Runs after replace_hosts: deleting a host cascades into rules.
            store
                .replace_forward_rules(&parsed.forward_rules)
                .map_err(|e| e.to_string())?;
            stats.forward_rules_imported = parsed.forward_rules.len();
        } else if !parsed.forward_rules.is_empty() {
            stats.forward_rules_imported = store
                .import_forward_rules(&parsed.forward_rules)
                .map_err(|e| e.to_string())?;
        }
    }

    if requested(SCOPE_MACROS) {
        if replace {
            store
                .replace_command_groups(&parsed.command_groups)
                .map_err(|e| e.to_string())?;
            stats.macros_imported = parsed
                .command_groups
                .iter()
                .map(|group| group.commands.len())
                .sum();
        } else {
            for group in &parsed.command_groups {
                let commands: Vec<(String, String, String)> = group
                    .commands
                    .iter()
                    .map(|item| (item.id.clone(), item.name.clone(), item.template.clone()))
                    .collect();
                let inserted = store
                    .import_command_group(&group.id, &group.name, &commands)
                    .map_err(|e| e.to_string())?;
                stats.macros_imported += inserted;
            }
        }
    }

    if requested(SCOPE_SECRETS) {
        stats.secrets_imported = import_backup_secrets(store, credentials, &parsed.secrets, mode)?;
        stats.ssh_keys_imported =
            import_backup_ssh_keys(store, credentials, &parsed.ssh_keys, mode)?;
        import_backup_password_credentials(store, &parsed.password_credentials, mode)?;
    }

    if requested(SCOPE_HIGHLIGHTS) && (!parsed.highlights.trim().is_empty() || replace) {
        store
            .set_setting("keywordHighlights", &parsed.highlights)
            .map_err(|e| e.to_string())?;
        stats.settings_applied += 1;
    }
    if requested(SCOPE_SHORTCUTS) && (!parsed.shortcuts.trim().is_empty() || replace) {
        store
            .set_setting("shortcuts", &parsed.shortcuts)
            .map_err(|e| e.to_string())?;
        stats.settings_applied += 1;
    }

    Ok(stats)
}

fn import_backup_secrets(
    store: &ft_store::Store,
    credentials: &ft_security::CredentialStore,
    secrets: &[BackupSecret],
    mode: RestoreMode,
) -> Result<usize, String> {
    if mode == RestoreMode::Replace {
        // Only managed data is rewound; `sync:` refs hold this install's own
        // remote credentials and must survive a rollback untouched.
        let keep: std::collections::HashSet<String> = secrets
            .iter()
            .map(|entry| entry.secret_ref.clone())
            .collect();
        for reference in credentials.refs() {
            if reference.starts_with("sync:") || keep.contains(&reference) {
                continue;
            }
            credentials.delete(&reference).map_err(|e| e.to_string())?;
            let _ = store.delete_vault_entry(&reference);
        }
    }
    let mut imported = 0;
    for entry in secrets {
        if entry.secret.is_empty() {
            continue;
        }
        credentials
            .set(&entry.secret_ref, &entry.secret)
            .map_err(|e| e.to_string())?;
        store
            .upsert_vault_entry(
                &entry.secret_ref,
                &entry.kind,
                &entry.label,
                entry.host_id.as_deref(),
            )
            .map_err(|e| e.to_string())?;
        imported += 1;
    }
    if !secrets.is_empty() {
        let _ = store.append_audit(
            "backup.import.secrets",
            "backup",
            &secrets.len().to_string(),
        );
    }
    Ok(imported)
}

fn import_backup_ssh_keys(
    store: &ft_store::Store,
    credentials: &ft_security::CredentialStore,
    keys: &[BackupSshKey],
    mode: RestoreMode,
) -> Result<usize, String> {
    if keys.is_empty() {
        return Ok(0);
    }
    if mode == RestoreMode::Replace {
        for existing in store.list_ssh_keys().map_err(|e| e.to_string())? {
            store
                .delete_ssh_key(&existing.id)
                .map_err(|e| e.to_string())?;
            let _ = credentials.delete(&key_ref(&existing.id));
            let _ = credentials.delete(&key_passphrase_ref(&existing.id));
        }
    }
    let mut existing: std::collections::HashSet<String> = store
        .list_ssh_keys()
        .map_err(|e| e.to_string())?
        .into_iter()
        .map(|key| key.fingerprint)
        .collect();
    let mut imported = 0;
    for entry in keys {
        if entry.key.fingerprint.trim().is_empty() || existing.contains(&entry.key.fingerprint) {
            continue;
        }
        let mut key = entry.key.clone();
        key.notes = key.notes.chars().take(2000).collect();
        key.expires_at = normalize_expiry(&key.expires_at);
        credentials
            .set(&key_ref(&key.id), &entry.private_key)
            .map_err(|e| e.to_string())?;
        if !entry.passphrase.is_empty() {
            credentials
                .set(&key_passphrase_ref(&key.id), &entry.passphrase)
                .map_err(|e| e.to_string())?;
        }
        store.save_ssh_key(&key).map_err(|e| e.to_string())?;
        let _ = store.upsert_vault_entry(&key_ref(&key.id), "key", &key.name, None);
        if key.has_passphrase {
            let _ = store.upsert_vault_entry(
                &key_passphrase_ref(&key.id),
                "keyPassphrase",
                &key.name,
                None,
            );
        }
        existing.insert(key.fingerprint.clone());
        imported += 1;
    }
    Ok(imported)
}
fn import_backup_password_credentials(
    store: &ft_store::Store,
    credentials: &[ft_store::PasswordCredential],
    mode: RestoreMode,
) -> Result<usize, String> {
    if mode == RestoreMode::Replace {
        store
            .replace_password_credentials(credentials)
            .map_err(|e| e.to_string())?;
        return Ok(credentials.len());
    }
    let existing: std::collections::HashSet<String> = store
        .list_password_credentials()
        .map_err(|e| e.to_string())?
        .into_iter()
        .map(|credential| credential.id)
        .collect();
    let mut imported = 0;
    for credential in credentials {
        if existing.contains(&credential.id) {
            continue;
        }
        store
            .save_password_credential(credential)
            .map_err(|e| e.to_string())?;
        imported += 1;
    }
    Ok(imported)
}

#[cfg(test)]
mod backup_payload_tests {
    use super::*;

    #[test]
    fn v1_bundle_stays_importable() {
        // v1 wrote only these three sections and had no version/scope fields.
        let raw = r#"{
            "tags": [],
            "hosts": [],
            "secrets": [
                {"secretRef":"pwd:h1","kind":"password","label":"h1","hostId":"h1","secret":"s"}
            ]
        }"#;
        let payload: BackupPayload = serde_json::from_str(raw).unwrap();
        assert_eq!(payload.version, 1);
        assert!(payload.scope.is_empty());
        assert_eq!(payload.secrets.len(), 1);
        assert!(payload.forward_rules.is_empty());
        assert!(payload.command_groups.is_empty());
        assert!(payload.ssh_keys.is_empty());
        assert!(payload.highlights.is_empty());
        assert!(payload.shortcuts.is_empty());
    }

    #[test]
    fn v2_bundle_carries_scope_and_summary() {
        let raw = r#"{
            "version": 2,
            "exportedAt": 123,
            "scope": ["hosts", "highlights"],
            "hosts": [],
            "tags": [],
            "forwardRules": [],
            "secrets": [],
            "sshKeys": [],
            "commandGroups": [],
            "highlights": "[{\"text\":\"err\",\"color\":\"red\"}]",
            "shortcuts": "",
            "summary": {"hosts":0,"tags":0,"secrets":0,"sshKeys":0,
                        "macros":0,"highlights":1,"shortcuts":0}
        }"#;
        let payload: BackupPayload = serde_json::from_str(raw).unwrap();
        assert_eq!(payload.version, 2);
        assert_eq!(payload.scope, vec!["hosts", "highlights"]);
        assert_eq!(payload.summary.highlights, 1);
        assert_eq!(count_json_array(&payload.highlights), 1);
    }

    #[test]
    fn counters_ignore_malformed_settings() {
        assert_eq!(count_json_array("not json"), 0);
        assert_eq!(count_json_array("{\"a\":1}"), 0);
        assert_eq!(
            count_json_bindings("{\"a\":\"ctrl+x\",\"b\":\"\",\"c\":3}"),
            1
        );
    }
}
