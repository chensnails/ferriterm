use std::collections::HashSet;
use std::sync::Arc;

use super::session::SshRuntimeState;
use super::vault::{key_passphrase_ref, key_ref, Credentials};
use tauri::State;

/// Bundle format for managed SSH keys. Versioned so an older build can reject
/// a newer layout instead of silently importing partial data. Private keys are
/// only present in the passphrase-sealed variant.
const KEY_BUNDLE_FORMAT: &str = "ferriterm-ssh-keys";
const KEY_BUNDLE_VERSION: u32 = 1;

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct KeyBundleEntry {
    name: String,
    algorithm: String,
    fingerprint: String,
    public_key: String,
    comment: String,
    source_path: String,
    has_passphrase: bool,
    notes: String,
    expires_at: String,
    #[serde(default)]
    private_key: String,
    #[serde(default)]
    passphrase: String,
}

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct KeyBundle {
    format: String,
    version: u32,
    exported_at: String,
    keys: Vec<KeyBundleEntry>,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KeyBundleStats {
    pub imported: usize,
    pub skipped: usize,
}

/// Accepts only a plain ISO date (YYYY-MM-DD) or an empty value, so a typo can
/// never be stored as an unparseable reminder.
pub(crate) fn normalize_expiry(value: &str) -> String {
    let value = value.trim();
    if value.is_empty() {
        return String::new();
    }
    let bytes = value.as_bytes();
    let shape_ok = bytes.len() == 10
        && bytes[4] == b'-'
        && bytes[7] == b'-'
        && bytes
            .iter()
            .enumerate()
            .all(|(i, b)| i == 4 || i == 7 || b.is_ascii_digit());
    if !shape_ok {
        return String::new();
    }
    // Range-check the components: a bundle from another install can carry any
    // string, and "2027-13-45" must not become a stored reminder.
    let month: u32 = value[5..7].parse().unwrap_or(0);
    let day: u32 = value[8..10].parse().unwrap_or(0);
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return String::new();
    }
    value.to_string()
}

fn key_bundle_entries(
    store: &ft_store::Store,
    credentials: Option<&ft_security::CredentialStore>,
) -> Result<Vec<KeyBundleEntry>, String> {
    let keys = store.list_ssh_keys().map_err(|e| e.to_string())?;
    let mut entries = Vec::with_capacity(keys.len());
    for key in keys {
        let (private_key, passphrase) = if let Some(credentials) = credentials {
            let private = credentials
                .get(&key_ref(&key.id))
                .map_err(|e| e.to_string())?
                .unwrap_or_default();
            let passphrase = credentials
                .get(&key_passphrase_ref(&key.id))
                .map_err(|e| e.to_string())?
                .unwrap_or_default();
            (private, passphrase)
        } else {
            (String::new(), String::new())
        };
        let include_private = credentials.is_some();
        // A metadata-only entry is still useful even when the vault is locked;
        // a private-key entry without its key material is not.
        if include_private && private_key.trim().is_empty() {
            continue;
        }
        entries.push(KeyBundleEntry {
            name: key.name,
            algorithm: key.algorithm,
            fingerprint: key.fingerprint,
            public_key: key.public_key,
            comment: key.comment,
            source_path: key.source_path,
            has_passphrase: key.has_passphrase,
            notes: key.notes,
            expires_at: key.expires_at,
            private_key,
            passphrase,
        });
    }
    Ok(entries)
}

fn utc_now() -> String {
    let seconds = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    // Format without pulling in a date crate: day-number to civil date (Howard
    // Hinnant's algorithm), good enough for a human-readable export stamp.
    let days = (seconds / 86_400) as i64;
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let mut year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = mp + if mp < 10 { 3 } else { -9 };
    if month <= 2 {
        year += 1;
    }
    format!("{year:04}-{month:02}-{day:02}")
}

fn key_bundle_payload(
    store: &ft_store::Store,
    credentials: Option<&ft_security::CredentialStore>,
) -> Result<Vec<u8>, String> {
    let bundle = KeyBundle {
        format: KEY_BUNDLE_FORMAT.to_string(),
        version: KEY_BUNDLE_VERSION,
        exported_at: utc_now(),
        keys: key_bundle_entries(store, credentials)?,
    };
    serde_json::to_vec_pretty(&bundle).map_err(|e| e.to_string())
}

/// Plain, unencrypted JSON: public keys plus metadata only. Safe to paste into
/// an issue tracker or a config repo. Never contains private key material.
#[tauri::command]
pub fn ssh_key_export_public(store: State<'_, Arc<ft_store::Store>>) -> Result<String, String> {
    let payload = key_bundle_payload(&store, None)?;
    String::from_utf8(payload).map_err(|e| e.to_string())
}

/// Passphrase-sealed backup containing private keys. Reuses the same
/// PBKDF2 + ChaCha20-Poly1305 FTBAK1 envelope as the full sync backup, so no
/// new crypto is introduced and the vault cipher stays untouched.
#[tauri::command]
pub fn ssh_key_export_bundle(
    store: State<'_, Arc<ft_store::Store>>,
    credentials: State<'_, Credentials>,
    passphrase: String,
) -> Result<String, String> {
    if passphrase.trim().is_empty() {
        return Err("a passphrase is required to export private keys".to_string());
    }
    let payload = key_bundle_payload(&store, Some(&credentials))?;
    let sealed = ft_sync::seal(&passphrase, &payload).map_err(|e| e.to_string())?;
    let _ = store.append_audit("key.export", "bundle", "");
    Ok(sealed)
}

#[tauri::command]
pub fn ssh_key_import_bundle(
    store: State<'_, Arc<ft_store::Store>>,
    credentials: State<'_, Credentials>,
    payload: String,
    passphrase: String,
) -> Result<KeyBundleStats, String> {
    let plain = ft_sync::open(&passphrase, &payload).map_err(|e| e.to_string())?;
    let bundle: KeyBundle =
        serde_json::from_slice(&plain).map_err(|e| format!("invalid key bundle: {e}"))?;
    if bundle.format != KEY_BUNDLE_FORMAT || bundle.version != KEY_BUNDLE_VERSION {
        return Err("unsupported key bundle format or version".to_string());
    }
    let existing: HashSet<String> = store
        .list_ssh_keys()
        .map_err(|e| e.to_string())?
        .into_iter()
        .map(|key| key.fingerprint)
        .collect();
    let mut imported = 0usize;
    let mut skipped = 0usize;
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    for (index, entry) in bundle.keys.into_iter().enumerate() {
        if entry.private_key.trim().is_empty() || existing.contains(&entry.fingerprint) {
            skipped += 1;
            continue;
        }
        let info = match ft_ssh::inspect_private_key(&entry.private_key) {
            Ok(info) => info,
            Err(_) => {
                skipped += 1;
                continue;
            }
        };
        let id = format!("kb-{stamp}-{index}");
        let key = ft_store::SshKey {
            id: id.clone(),
            name: if entry.name.trim().is_empty() {
                format!("{} key", info.algorithm)
            } else {
                entry.name.trim().to_string()
            },
            algorithm: info.algorithm,
            fingerprint: info.fingerprint,
            public_key: info.public_key,
            comment: info.comment,
            source_path: entry.source_path,
            has_passphrase: info.encrypted,
            created_at: String::new(),
            updated_at: String::new(),
            notes: entry.notes.chars().take(2000).collect(),
            expires_at: normalize_expiry(&entry.expires_at),
        };
        credentials
            .set(&key_ref(&id), &entry.private_key)
            .map_err(|e| e.to_string())?;
        if !entry.passphrase.is_empty() {
            credentials
                .set(&key_passphrase_ref(&id), &entry.passphrase)
                .map_err(|e| e.to_string())?;
        }
        store.save_ssh_key(&key).map_err(|e| e.to_string())?;
        let _ = store.upsert_vault_entry(&key_ref(&id), "key", &key.name, None);
        if key.has_passphrase {
            let _ = store.upsert_vault_entry(
                &key_passphrase_ref(&id),
                "keyPassphrase",
                &key.name,
                None,
            );
        }
        imported += 1;
    }
    let _ = store.append_audit("key.import.bundle", "bundle", &imported.to_string());
    Ok(KeyBundleStats { imported, skipped })
}

#[tauri::command]
pub fn ssh_key_update_metadata(
    store: State<'_, Arc<ft_store::Store>>,
    id: String,
    notes: String,
    expires_at: String,
) -> Result<ft_store::SshKey, String> {
    let notes: String = notes.trim().chars().take(2000).collect();
    let expires_at = normalize_expiry(&expires_at);
    let changed = store
        .update_ssh_key_metadata(&id, &notes, &expires_at)
        .map_err(|e| e.to_string())?;
    if !changed {
        return Err("key not found".to_string());
    }
    let key = store
        .list_ssh_keys()
        .map_err(|e| e.to_string())?
        .into_iter()
        .find(|key| key.id == id)
        .ok_or_else(|| "key not found".to_string())?;
    let _ = store.append_audit("key.metadata", &key.name, &expires_at);
    Ok(key)
}

/// Regenerates a managed key in place. The id is deliberately preserved so
/// every host that references `keyId` keeps working; only the vault secret and
/// the public fingerprint change. Requires the new key's passphrase when the
/// old one was encrypted, otherwise the vault would hold an unusable key.
#[tauri::command]
pub fn ssh_key_rotate(
    store: State<'_, Arc<ft_store::Store>>,
    credentials: State<'_, Credentials>,
    id: String,
    passphrase: String,
    comment: String,
) -> Result<ft_store::SshKey, String> {
    let current = store
        .list_ssh_keys()
        .map_err(|e| e.to_string())?
        .into_iter()
        .find(|key| key.id == id)
        .ok_or_else(|| "key not found".to_string())?;
    let algorithm = match current.algorithm.as_str() {
        "ssh-ed25519" | "ed25519" => "ed25519",
        "ssh-rsa" | "rsa-sha2-256" | "rsa-sha2-512" => "rsa",
        "ecdsa-sha2-nistp256" | "ecdsa-sha2-nistp384" | "ecdsa-sha2-nistp521" => "ecdsa",
        other => {
            return Err(format!(
                "rotation is not supported for key algorithm {other}"
            ))
        }
    };
    let new_passphrase = passphrase.trim();
    if current.has_passphrase && new_passphrase.is_empty() {
        return Err(
            "the existing key is encrypted; enter a passphrase for the new key".to_string(),
        );
    }
    let new_comment = if comment.trim().is_empty() {
        current.comment.clone()
    } else {
        comment.trim().to_string()
    };
    let (private, info) = ft_ssh::generate_key(
        algorithm,
        if new_passphrase.is_empty() {
            None
        } else {
            Some(new_passphrase)
        },
        &new_comment,
    )?;
    let mut key = current;
    key.algorithm = info.algorithm;
    key.fingerprint = info.fingerprint;
    key.public_key = info.public_key;
    key.comment = info.comment;
    key.has_passphrase = info.encrypted;
    // Rotation starts a fresh validity period; notes are operator-owned and
    // survive, but an old expiry date would immediately flag the new key.
    key.expires_at = String::new();
    credentials
        .set(&key_ref(&key.id), &private)
        .map_err(|e| e.to_string())?;
    if new_passphrase.is_empty() {
        let _ = credentials.delete(&key_passphrase_ref(&key.id));
        let _ = store.delete_vault_entry(&key_passphrase_ref(&key.id));
    } else {
        credentials
            .set(&key_passphrase_ref(&key.id), new_passphrase)
            .map_err(|e| e.to_string())?;
        let _ = store.upsert_vault_entry(
            &key_passphrase_ref(&key.id),
            "keyPassphrase",
            &key.name,
            None,
        );
    }
    store.save_ssh_key(&key).map_err(|e| e.to_string())?;
    let _ = store.upsert_vault_entry(&key_ref(&key.id), "key", &key.name, None);
    let _ = store.append_audit("key.rotate", &key.name, &key.fingerprint);
    Ok(key)
}

#[tauri::command]
pub fn known_hosts_list(
    runtime: State<'_, SshRuntimeState>,
) -> Result<Vec<ft_ssh::HostKeyEntry>, String> {
    runtime.known_hosts.entries().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn known_hosts_remove(
    store: State<'_, Arc<ft_store::Store>>,
    runtime: State<'_, SshRuntimeState>,
    host: String,
    port: u16,
) -> Result<(), String> {
    runtime
        .known_hosts
        .remove(&host, port)
        .map_err(|e| e.to_string())?;
    let _ = store.append_audit("known_hosts.remove", &host, &port.to_string());
    Ok(())
}
#[cfg(test)]
mod key_bundle_tests {
    use super::*;

    #[test]
    fn expiry_accepts_only_plain_iso_dates() {
        assert_eq!(normalize_expiry(" 2027-01-31 "), "2027-01-31");
        assert_eq!(normalize_expiry("2028-02-29"), "2028-02-29");
        assert_eq!(normalize_expiry(""), "");
        assert_eq!(normalize_expiry("   "), "");
        assert_eq!(normalize_expiry("2027-1-31"), "");
        assert_eq!(normalize_expiry("31/01/2027"), "");
        assert_eq!(normalize_expiry("2027-01-31T00:00:00Z"), "");
        assert_eq!(normalize_expiry("2027-13-01"), "");
        assert_eq!(normalize_expiry("2027-00-10"), "");
        assert_eq!(normalize_expiry("2027-01-32"), "");
        assert_eq!(normalize_expiry("abcd-ef-gh"), "");
    }
}
