use std::collections::HashSet;
use std::sync::Arc;

use ft_ssh::{CredentialProvider, SecretKind};
use tauri::State;

pub(crate) type Credentials = Arc<ft_security::CredentialStore>;

/// Reads secrets from the vault inside the Rust process. The webview never
/// receives stored plaintext.
pub(crate) struct VaultProvider {
    pub(crate) credentials: Credentials,
}

impl VaultProvider {
    fn lookup(&self, references: &[String]) -> Option<String> {
        for reference in references {
            if let Ok(Some(secret)) = self.credentials.get(reference) {
                if !secret.is_empty() {
                    return Some(secret);
                }
            }
        }
        None
    }
}

impl CredentialProvider for VaultProvider {
    fn host_secret(&self, host_id: &str, kind: SecretKind) -> Option<String> {
        match kind {
            SecretKind::Password => self.lookup(&[format!("pwd:{host_id}"), host_id.to_string()]),
            SecretKind::Passphrase => self.lookup(&[format!("pp:{host_id}"), host_id.to_string()]),
        }
    }

    fn key_material(&self, key_id: &str) -> Option<(String, Option<String>)> {
        let text = self
            .credentials
            .get(&format!("key:{key_id}"))
            .ok()
            .flatten()?;
        let passphrase = self
            .credentials
            .get(&format!("keypp:{key_id}"))
            .ok()
            .flatten()
            .filter(|value| !value.is_empty());
        Some((text, passphrase))
    }

    fn locked(&self) -> bool {
        self.credentials.is_locked()
    }
}

pub(crate) fn password_ref(host_id: &str) -> String {
    format!("pwd:{host_id}")
}

pub(crate) fn passphrase_ref(host_id: &str) -> String {
    format!("pp:{host_id}")
}

pub(crate) fn key_ref(key_id: &str) -> String {
    format!("key:{key_id}")
}

pub(crate) fn key_passphrase_ref(key_id: &str) -> String {
    format!("keypp:{key_id}")
}

pub(crate) fn infer_kind(reference: &str) -> &'static str {
    if reference.starts_with("pwd:") {
        "password"
    } else if reference.starts_with("pp:") {
        "passphrase"
    } else if reference.starts_with("keypp:") {
        "keyPassphrase"
    } else if reference.starts_with("key:") {
        "key"
    } else if reference.starts_with("sync:") {
        "token"
    } else {
        "password"
    }
}

pub(crate) fn infer_host_id(reference: &str) -> Option<String> {
    reference
        .strip_prefix("pwd:")
        .or_else(|| reference.strip_prefix("pp:"))
        .map(str::to_string)
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CredentialStatus {
    pub password: bool,
    pub passphrase: bool,
    pub key: bool,
    pub key_passphrase: bool,
    pub locked: bool,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VaultEntryView {
    pub secret_ref: String,
    pub kind: String,
    pub label: String,
    pub host_id: Option<String>,
    pub updated_at: String,
}

#[tauri::command]
pub fn credential_status(
    credentials: State<'_, Credentials>,
    host_id: String,
    key_id: Option<String>,
) -> CredentialStatus {
    CredentialStatus {
        password: credentials.exists(&password_ref(&host_id)) || credentials.exists(&host_id),
        passphrase: credentials.exists(&passphrase_ref(&host_id)) || credentials.exists(&host_id),
        key: key_id
            .as_deref()
            .filter(|id| !id.is_empty())
            .is_some_and(|id| credentials.exists(&key_ref(id))),
        key_passphrase: key_id
            .as_deref()
            .filter(|id| !id.is_empty())
            .is_some_and(|id| credentials.exists(&key_passphrase_ref(id))),
        locked: credentials.is_locked(),
    }
}

/// Legacy entry point kept for older payloads; stores a password under the
/// new reference scheme.
#[tauri::command]
pub fn save_credential(
    store: State<'_, Arc<ft_store::Store>>,
    credentials: State<'_, Credentials>,
    host_id: String,
    secret: String,
) -> Result<(), String> {
    apply_host_secrets(&store, &credentials, &host_id, Some(secret), None)
}

#[tauri::command]
pub fn delete_credential(
    store: State<'_, Arc<ft_store::Store>>,
    credentials: State<'_, Credentials>,
    host_id: String,
) -> Result<(), String> {
    for reference in [
        password_ref(&host_id),
        passphrase_ref(&host_id),
        host_id.clone(),
    ] {
        credentials.delete(&reference).map_err(|e| e.to_string())?;
        let _ = store.delete_vault_entry(&reference);
    }
    let _ = store.append_audit("secret.delete", &host_id, "");
    Ok(())
}

/// Saves or clears a host's password/passphrase. `None` leaves the slot
/// untouched; an empty string removes it.
#[tauri::command]
pub fn save_host_credential(
    store: State<'_, Arc<ft_store::Store>>,
    credentials: State<'_, Credentials>,
    host_id: String,
    password: Option<String>,
    passphrase: Option<String>,
) -> Result<(), String> {
    apply_host_secrets(&store, &credentials, &host_id, password, passphrase)
}

pub(crate) fn apply_host_secrets(
    store: &ft_store::Store,
    credentials: &ft_security::CredentialStore,
    host_id: &str,
    password: Option<String>,
    passphrase: Option<String>,
) -> Result<(), String> {
    let host = store
        .list_hosts()
        .ok()
        .and_then(|hosts| hosts.into_iter().find(|host| host.id == host_id));
    let label = host
        .as_ref()
        .map(|host| host.name.clone())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| host_id.to_string());
    let refs = [
        (password_ref(host_id), "password", password),
        (passphrase_ref(host_id), "passphrase", passphrase),
    ];
    for (reference, kind, value) in refs {
        match value {
            None => {}
            Some(secret) if secret.is_empty() => {
                credentials.delete(&reference).map_err(|e| e.to_string())?;
                let _ = store.delete_vault_entry(&reference);
            }
            Some(secret) => {
                credentials
                    .set(&reference, &secret)
                    .map_err(|e| e.to_string())?;
                store
                    .upsert_vault_entry(&reference, kind, &label, Some(host_id))
                    .map_err(|e| e.to_string())?;
            }
        }
    }
    let _ = store.append_audit("secret.save", host_id, "");
    Ok(())
}

#[tauri::command]
pub fn save_secret(
    credentials: State<'_, Credentials>,
    secret_ref: String,
    secret: String,
) -> Result<(), String> {
    credentials
        .set(&secret_ref, &secret)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn secret_saved(credentials: State<'_, Credentials>, secret_ref: String) -> bool {
    credentials.exists(&secret_ref)
}

#[tauri::command]
pub fn delete_secret(
    credentials: State<'_, Credentials>,
    secret_ref: String,
) -> Result<(), String> {
    credentials.delete(&secret_ref).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn vault_status(credentials: State<'_, Credentials>) -> ft_security::VaultStatus {
    credentials.status()
}

#[tauri::command]
pub fn vault_unlock(
    store: State<'_, Arc<ft_store::Store>>,
    credentials: State<'_, Credentials>,
    password: String,
) -> Result<(), String> {
    let result = credentials.unlock(&password);
    let detail = result
        .as_ref()
        .err()
        .map(|e| e.to_string())
        .unwrap_or_default();
    let _ = store.append_audit(
        if result.is_ok() {
            "vault.unlock"
        } else {
            "vault.unlock.failed"
        },
        "vault",
        &detail,
    );
    result.map_err(|e| e.to_string())
}

#[tauri::command]
pub fn vault_lock(
    store: State<'_, Arc<ft_store::Store>>,
    credentials: State<'_, Credentials>,
) -> Result<(), String> {
    credentials.lock();
    let _ = store.append_audit("vault.lock", "vault", "");
    Ok(())
}

#[tauri::command]
pub fn vault_enable_master_password(
    store: State<'_, Arc<ft_store::Store>>,
    credentials: State<'_, Credentials>,
    password: String,
) -> Result<usize, String> {
    let migrated = credentials
        .enable_master_password(&password, &known_secret_refs(&store))
        .map_err(|e| e.to_string())?;
    let _ = store.append_audit("vault.enable", "vault", &format!("{migrated} secrets"));
    Ok(migrated)
}

#[tauri::command]
pub fn vault_disable_master_password(
    store: State<'_, Arc<ft_store::Store>>,
    credentials: State<'_, Credentials>,
    password: String,
) -> Result<(), String> {
    credentials
        .disable_master_password(&password)
        .map_err(|e| e.to_string())?;
    let _ = store.append_audit("vault.disable", "vault", "");
    Ok(())
}

#[tauri::command]
pub fn vault_change_master_password(
    store: State<'_, Arc<ft_store::Store>>,
    credentials: State<'_, Credentials>,
    current: String,
    next: String,
) -> Result<(), String> {
    credentials
        .change_master_password(&current, &next)
        .map_err(|e| e.to_string())?;
    let _ = store.append_audit("vault.password.change", "vault", "");
    Ok(())
}

/// Destroys the master password and every stored secret. Unrecoverable.
#[tauri::command]
pub fn vault_reset(
    store: State<'_, Arc<ft_store::Store>>,
    credentials: State<'_, Credentials>,
) -> Result<(), String> {
    credentials.reset().map_err(|e| e.to_string())?;
    let _ = store.clear_vault_entries();
    let _ = store.append_audit("vault.reset", "vault", "all secrets removed");
    Ok(())
}

#[tauri::command]
pub fn vault_list_entries(
    store: State<'_, Arc<ft_store::Store>>,
    credentials: State<'_, Credentials>,
) -> Result<Vec<VaultEntryView>, String> {
    let hosts = store.list_hosts().unwrap_or_default();
    let keys = store.list_ssh_keys().unwrap_or_default();
    let refs = credentials.refs();
    let mut views: Vec<VaultEntryView> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    for entry in store.list_vault_entries().unwrap_or_default() {
        if !refs.iter().any(|item| item == &entry.secret_ref) {
            continue;
        }
        seen.insert(entry.secret_ref.clone());
        views.push(VaultEntryView {
            secret_ref: entry.secret_ref,
            kind: entry.kind,
            label: entry.label,
            host_id: entry.host_id,
            updated_at: entry.updated_at,
        });
    }
    for reference in refs {
        if !seen.insert(reference.clone()) {
            continue;
        }
        let host_id = infer_host_id(&reference);
        let key_id = reference
            .strip_prefix("keypp:")
            .or_else(|| reference.strip_prefix("key:"))
            .map(str::to_string);
        let label = host_id
            .as_ref()
            .and_then(|id| hosts.iter().find(|host| &host.id == id))
            .map(|host| host.name.clone())
            .or_else(|| {
                key_id
                    .as_ref()
                    .and_then(|id| keys.iter().find(|key| &key.id == id))
                    .map(|key| key.name.clone())
            })
            .unwrap_or_else(|| reference.clone());
        views.push(VaultEntryView {
            kind: infer_kind(&reference).to_string(),
            label,
            host_id,
            updated_at: String::new(),
            secret_ref: reference,
        });
    }
    Ok(views)
}

#[tauri::command]
pub fn vault_delete_entry(
    store: State<'_, Arc<ft_store::Store>>,
    credentials: State<'_, Credentials>,
    secret_ref: String,
) -> Result<(), String> {
    credentials.delete(&secret_ref).map_err(|e| e.to_string())?;
    let _ = store.delete_vault_entry(&secret_ref);
    let _ = store.append_audit("secret.delete", &secret_ref, "");
    Ok(())
}

#[tauri::command]
pub fn vault_clear_entries(
    store: State<'_, Arc<ft_store::Store>>,
    credentials: State<'_, Credentials>,
) -> Result<usize, String> {
    let mut refs = credentials.refs();
    for entry in store.list_vault_entries().unwrap_or_default() {
        if !refs.contains(&entry.secret_ref) {
            refs.push(entry.secret_ref);
        }
    }
    let mut removed = 0;
    for reference in refs {
        if credentials.delete(&reference).is_ok() {
            removed += 1;
        }
    }
    let _ = store.clear_vault_entries();
    let _ = store.append_audit("vault.clear", "vault", &format!("{removed} secrets"));
    Ok(removed)
}

fn known_secret_refs(store: &ft_store::Store) -> Vec<String> {
    let mut refs = Vec::new();
    if let Ok(hosts) = store.list_hosts() {
        for host in hosts {
            refs.push(host.id.clone());
            refs.push(password_ref(&host.id));
            refs.push(passphrase_ref(&host.id));
        }
    }
    if let Ok(keys) = store.list_ssh_keys() {
        for key in keys {
            refs.push(key_ref(&key.id));
            refs.push(key_passphrase_ref(&key.id));
        }
    }
    refs.push("sync:gist-token".to_string());
    refs.push("sync:webdav-password".to_string());
    refs
}

#[tauri::command]
pub fn ssh_key_list(
    store: State<'_, Arc<ft_store::Store>>,
) -> Result<Vec<ft_store::SshKey>, String> {
    store.list_ssh_keys().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn ssh_key_scan(dir: Option<String>) -> Vec<ft_ssh::KeyCandidate> {
    ft_ssh::scan_key_candidates(dir.as_deref())
}

#[tauri::command]
pub fn ssh_key_inspect(text: String) -> Result<ft_ssh::KeyInfo, String> {
    ft_ssh::inspect_private_key(&text)
}

#[tauri::command]
pub fn ssh_key_import(
    store: State<'_, Arc<ft_store::Store>>,
    credentials: State<'_, Credentials>,
    id: String,
    name: String,
    path: Option<String>,
    text: Option<String>,
    passphrase: Option<String>,
) -> Result<ft_store::SshKey, String> {
    let raw = match (path.as_deref(), text.as_deref()) {
        (Some(path), _) if !path.trim().is_empty() => ft_ssh::read_key_file(path)?,
        (_, Some(text)) if !text.trim().is_empty() => text.to_string(),
        _ => return Err("provide a key file path or pasted key content".to_string()),
    };
    let info = ft_ssh::inspect_private_key(&raw)?;
    let fallback_name = path
        .as_deref()
        .and_then(|path| std::path::Path::new(path).file_name())
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| info.fingerprint.chars().take(19).collect());
    let key = ft_store::SshKey {
        id: id.clone(),
        name: if name.trim().is_empty() {
            fallback_name
        } else {
            name.trim().to_string()
        },
        algorithm: info.algorithm,
        fingerprint: info.fingerprint,
        public_key: info.public_key,
        comment: info.comment,
        source_path: path.unwrap_or_default(),
        has_passphrase: info.encrypted,
        created_at: String::new(),
        updated_at: String::new(),
        notes: String::new(),
        expires_at: String::new(),
    };
    credentials
        .set(&key_ref(&id), &raw)
        .map_err(|e| e.to_string())?;
    if let Some(passphrase) = passphrase.filter(|value| !value.is_empty()) {
        credentials
            .set(&key_passphrase_ref(&id), &passphrase)
            .map_err(|e| e.to_string())?;
    }
    store.save_ssh_key(&key).map_err(|e| e.to_string())?;
    store
        .upsert_vault_entry(&key_ref(&id), "key", &key.name, None)
        .map_err(|e| e.to_string())?;
    if key.has_passphrase {
        let _ =
            store.upsert_vault_entry(&key_passphrase_ref(&id), "keyPassphrase", &key.name, None);
    }
    let _ = store.append_audit("key.import", &key.name, &key.fingerprint);
    Ok(key)
}

#[tauri::command]
pub fn ssh_key_generate(
    store: State<'_, Arc<ft_store::Store>>,
    credentials: State<'_, Credentials>,
    id: String,
    name: String,
    algorithm: String,
    passphrase: Option<String>,
    comment: String,
) -> Result<ft_store::SshKey, String> {
    let passphrase = passphrase.filter(|value| !value.is_empty());
    let (private, info) = ft_ssh::generate_key(&algorithm, passphrase.as_deref(), &comment)?;
    let key = ft_store::SshKey {
        id: id.clone(),
        name: if name.trim().is_empty() {
            format!("{} key", info.algorithm)
        } else {
            name.trim().to_string()
        },
        algorithm: info.algorithm,
        fingerprint: info.fingerprint,
        public_key: info.public_key,
        comment: info.comment,
        source_path: String::new(),
        has_passphrase: info.encrypted,
        created_at: String::new(),
        updated_at: String::new(),
        notes: String::new(),
        expires_at: String::new(),
    };
    credentials
        .set(&key_ref(&id), &private)
        .map_err(|e| e.to_string())?;
    if let Some(passphrase) = passphrase {
        credentials
            .set(&key_passphrase_ref(&id), &passphrase)
            .map_err(|e| e.to_string())?;
    }
    store.save_ssh_key(&key).map_err(|e| e.to_string())?;
    let _ = store.upsert_vault_entry(&key_ref(&id), "key", &key.name, None);
    if key.has_passphrase {
        let _ =
            store.upsert_vault_entry(&key_passphrase_ref(&id), "keyPassphrase", &key.name, None);
    }
    let _ = store.append_audit("key.generate", &key.name, &key.algorithm);
    Ok(key)
}

#[tauri::command]
pub fn ssh_key_delete(
    store: State<'_, Arc<ft_store::Store>>,
    credentials: State<'_, Credentials>,
    id: String,
) -> Result<(), String> {
    let label = store
        .list_ssh_keys()
        .ok()
        .and_then(|keys| keys.into_iter().find(|key| key.id == id))
        .map(|key| key.name)
        .unwrap_or_else(|| id.clone());
    for reference in [key_ref(&id), key_passphrase_ref(&id)] {
        if let Err(err) = credentials.delete(&reference) {
            let _ = store.append_audit("key.delete.failed", &label, &err.to_string());
        }
        let _ = store.delete_vault_entry(&reference);
    }
    store.delete_ssh_key(&id).map_err(|e| e.to_string())?;
    let _ = store.append_audit("key.delete", &label, "");
    Ok(())
}
