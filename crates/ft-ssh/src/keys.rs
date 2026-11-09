//! Private key inspection, generation and discovery for the key manager.

use std::fs;
use std::path::{Path, PathBuf};

use russh::keys::ssh_key::LineEnding;
use russh::keys::{Algorithm, EcdsaCurve, HashAlg, PrivateKey};
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KeyInfo {
    pub algorithm: String,
    pub fingerprint: String,
    pub public_key: String,
    pub comment: String,
    pub encrypted: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KeyCandidate {
    pub path: String,
    pub name: String,
    pub algorithm: String,
    pub fingerprint: String,
    pub comment: String,
    pub encrypted: bool,
}

pub fn inspect_private_key(text: &str) -> Result<KeyInfo, String> {
    let key = PrivateKey::from_openssh(text)
        .map_err(|err| format!("not an OpenSSH private key: {err}"))?;
    Ok(info(&key))
}

pub fn decrypt_for_use(text: &str, passphrase: Option<&str>) -> Result<PrivateKey, String> {
    let key = PrivateKey::from_openssh(text)
        .map_err(|err| format!("failed to parse private key: {err}"))?;
    if !key.is_encrypted() {
        return Ok(key);
    }
    let passphrase = passphrase
        .ok_or_else(|| "private key is encrypted; a passphrase is required".to_string())?;
    key.decrypt(passphrase)
        .map_err(|err| format!("failed to decrypt private key: {err}"))
}

/// Generates a new key. Returns the OpenSSH-encoded private key and its public
/// metadata; the private key is encrypted when a passphrase is supplied.
pub fn generate_key(
    algorithm: &str,
    passphrase: Option<&str>,
    comment: &str,
) -> Result<(String, KeyInfo), String> {
    let mut rng = rand::rng();
    let algorithm = match algorithm {
        "rsa" => Algorithm::Rsa { hash: None },
        "ecdsa" => Algorithm::Ecdsa {
            curve: EcdsaCurve::NistP256,
        },
        "ed25519" | "" => Algorithm::Ed25519,
        other => return Err(format!("unsupported key algorithm: {other}")),
    };
    let mut key =
        PrivateKey::random(&mut rng, algorithm).map_err(|err| format!("keygen failed: {err}"))?;
    if !comment.trim().is_empty() {
        key.set_comment(comment.trim());
    }
    let key = match passphrase.filter(|value| !value.is_empty()) {
        Some(passphrase) => key
            .encrypt(&mut rng, passphrase)
            .map_err(|err| format!("failed to encrypt new key: {err}"))?,
        None => key,
    };
    let info = info(&key);
    let encoded = key
        .to_openssh(LineEnding::LF)
        .map_err(|err| err.to_string())?;
    Ok((encoded.to_string(), info))
}

/// Lists private keys inside `~/.ssh` (or an explicit directory).
pub fn scan_key_candidates(dir: Option<&str>) -> Vec<KeyCandidate> {
    let dir = match dir {
        Some(value) if !value.trim().is_empty() => PathBuf::from(expand_home(value.trim())),
        _ => match home_dir() {
            Some(home) => home.join(".ssh"),
            None => return Vec::new(),
        },
    };
    let entries = match fs::read_dir(&dir) {
        Ok(entries) => entries,
        Err(_) => return Vec::new(),
    };
    let mut candidates = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let Some(name) = path.file_name().and_then(|value| value.to_str()) else {
            continue;
        };
        if name == "known_hosts"
            || name == "known_hosts.old"
            || name == "config"
            || name == "authorized_keys"
            || name.ends_with(".pub")
        {
            continue;
        }
        let Ok(text) = fs::read_to_string(&path) else {
            continue;
        };
        let Ok(info) = inspect_private_key(&text) else {
            continue;
        };
        candidates.push(KeyCandidate {
            path: path.display().to_string(),
            name: name.to_string(),
            algorithm: info.algorithm,
            fingerprint: info.fingerprint,
            comment: info.comment,
            encrypted: info.encrypted,
        });
    }
    candidates.sort_by(|a, b| a.name.cmp(&b.name));
    candidates
}

pub fn read_key_file(path: &str) -> Result<String, String> {
    let path = expand_home(path.trim());
    let metadata = fs::metadata(&path).map_err(|err| format!("cannot read {path}: {err}"))?;
    if !metadata.is_file() {
        return Err(format!("{path} is not a file"));
    }
    if metadata.len() > 1024 * 1024 {
        return Err("key file is larger than 1 MiB".to_string());
    }
    fs::read_to_string(&path).map_err(|err| format!("cannot read {path}: {err}"))
}

fn info(key: &PrivateKey) -> KeyInfo {
    let public = key.public_key();
    KeyInfo {
        algorithm: public.algorithm().as_str().to_string(),
        fingerprint: public.fingerprint(HashAlg::Sha256).to_string(),
        public_key: public.to_openssh().unwrap_or_default(),
        comment: public.comment().to_string(),
        encrypted: key.is_encrypted(),
    }
}

pub fn expand_home(path: &str) -> String {
    if let Some(rest) = path.strip_prefix("~/") {
        if let Some(home) = home_dir() {
            return home.join(rest).display().to_string();
        }
    }
    path.to_string()
}

fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
}

#[allow(dead_code)]
fn _assert_path(_: &Path) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_key_roundtrips_and_reports_fingerprint() {
        let (private, info) = generate_key("ed25519", None, "test@ferriterm").unwrap();
        assert_eq!(info.algorithm, "ssh-ed25519");
        assert!(info.fingerprint.starts_with("SHA256:"));
        assert!(info.public_key.starts_with("ssh-ed25519 "));
        assert!(!info.encrypted);

        let parsed = inspect_private_key(&private).unwrap();
        assert_eq!(parsed.fingerprint, info.fingerprint);
        assert!(decrypt_for_use(&private, None).is_ok());
    }

    #[test]
    fn encrypted_key_requires_passphrase() {
        let (private, info) = generate_key("ed25519", Some("secret"), "locked").unwrap();
        assert!(info.encrypted);
        assert!(decrypt_for_use(&private, None).is_err());
        assert!(decrypt_for_use(&private, Some("secret")).is_ok());
    }

    #[test]
    fn rejects_non_key_text() {
        assert!(inspect_private_key("hello").is_err());
    }
}
