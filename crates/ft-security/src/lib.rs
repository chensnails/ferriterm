//! Credential vault: OS keychain first, encrypted file fallback.

use std::collections::BTreeMap;
use std::fmt;
use std::fs;
use std::io;
use std::io::Write;
use std::path::PathBuf;

use ring::aead;
use ring::rand::{SecureRandom, SystemRandom};

const SERVICE: &str = "dev.ferriterm.credentials";
const KEY_FILE: &str = "credentials.key";
const VAULT_FILE: &str = "credentials.vault.json";
const NONCE_LEN: usize = 12;

#[derive(Debug)]
pub enum CredentialError {
    Keyring(String),
    Storage(String),
}

impl fmt::Display for CredentialError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CredentialError::Keyring(msg) => write!(f, "keychain error: {msg}"),
            CredentialError::Storage(msg) => write!(f, "credential storage error: {msg}"),
        }
    }
}

impl std::error::Error for CredentialError {}

impl From<io::Error> for CredentialError {
    fn from(err: io::Error) -> Self {
        CredentialError::Storage(err.to_string())
    }
}

#[derive(Debug, Clone)]
pub struct CredentialStore {
    dir: PathBuf,
    use_keyring: bool,
}

impl CredentialStore {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        // Escape hatch for machines where the keychain backend is unavailable.
        let use_keyring = std::env::var_os("FERRITERM_DISABLE_KEYRING").is_none();
        Self {
            dir: dir.into(),
            use_keyring,
        }
    }

    pub fn get(&self, host_id: &str) -> Result<Option<String>, CredentialError> {
        if self.use_keyring {
            if let Ok(entry) = keyring::Entry::new(SERVICE, host_id) {
                match entry.get_password() {
                    Ok(secret) => return Ok(Some(secret)),
                    Err(keyring::Error::NoEntry) => {}
                    Err(_) => {}
                }
            }
        }
        match self.read_vault()?.get(host_id) {
            Some(blob) => Ok(Some(self.decrypt(blob)?)),
            None => Ok(None),
        }
    }

    pub fn set(&self, host_id: &str, secret: &str) -> Result<(), CredentialError> {
        if self.use_keyring {
            if let Ok(entry) = keyring::Entry::new(SERVICE, host_id) {
                if entry.set_password(secret).is_ok() {
                    self.remove_fallback(host_id)?;
                    return Ok(());
                }
            }
        }
        let mut vault = self.read_vault()?;
        vault.insert(host_id.to_string(), self.encrypt(secret)?);
        self.write_vault(&vault)
    }

    pub fn delete(&self, host_id: &str) -> Result<(), CredentialError> {
        if self.use_keyring {
            if let Ok(entry) = keyring::Entry::new(SERVICE, host_id) {
                match entry.delete_credential() {
                    Ok(()) | Err(keyring::Error::NoEntry) => {}
                    Err(err) => return Err(CredentialError::Keyring(err.to_string())),
                }
            }
        }
        self.remove_fallback(host_id)
    }

    fn remove_fallback(&self, host_id: &str) -> Result<(), CredentialError> {
        let mut vault = self.read_vault()?;
        if vault.remove(host_id).is_some() {
            self.write_vault(&vault)?;
        }
        Ok(())
    }

    fn vault_path(&self) -> PathBuf {
        self.dir.join(VAULT_FILE)
    }

    fn read_vault(&self) -> Result<BTreeMap<String, String>, CredentialError> {
        let path = self.vault_path();
        match fs::read_to_string(&path) {
            Ok(raw) => serde_json::from_str(&raw)
                .map_err(|e| CredentialError::Storage(format!("corrupt vault file: {e}"))),
            Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(BTreeMap::new()),
            Err(err) => Err(err.into()),
        }
    }

    fn write_vault(&self, vault: &BTreeMap<String, String>) -> Result<(), CredentialError> {
        fs::create_dir_all(&self.dir)?;
        let raw = serde_json::to_vec(vault)
            .map_err(|e| CredentialError::Storage(format!("serialize vault: {e}")))?;
        write_private(&self.vault_path(), &raw)
    }

    fn aead_key(&self) -> Result<aead::LessSafeKey, CredentialError> {
        let key = load_or_create_key(&self.dir.join(KEY_FILE))?;
        let unbound = aead::UnboundKey::new(&aead::CHACHA20_POLY1305, &key)
            .map_err(|_| CredentialError::Storage("invalid vault key".to_string()))?;
        Ok(aead::LessSafeKey::new(unbound))
    }

    fn encrypt(&self, plain: &str) -> Result<String, CredentialError> {
        let key = self.aead_key()?;
        let mut nonce_bytes = [0u8; NONCE_LEN];
        SystemRandom::new()
            .fill(&mut nonce_bytes)
            .map_err(|_| CredentialError::Storage("random source unavailable".to_string()))?;
        let nonce = aead::Nonce::assume_unique_for_key(nonce_bytes);
        let mut out = plain.as_bytes().to_vec();
        key.seal_in_place_append_tag(nonce, aead::Aad::empty(), &mut out)
            .map_err(|_| CredentialError::Storage("encrypt failed".to_string()))?;
        let mut blob = nonce_bytes.to_vec();
        blob.extend_from_slice(&out);
        Ok(hex_encode(&blob))
    }

    fn decrypt(&self, blob: &str) -> Result<String, CredentialError> {
        let raw = hex_decode(blob)?;
        if raw.len() <= NONCE_LEN {
            return Err(CredentialError::Storage(
                "truncated vault entry".to_string(),
            ));
        }
        let (nonce_bytes, cipher) = raw.split_at(NONCE_LEN);
        let nonce = aead::Nonce::try_assume_unique_for_key(nonce_bytes)
            .map_err(|_| CredentialError::Storage("invalid nonce".to_string()))?;
        let key = self.aead_key()?;
        let mut buf = cipher.to_vec();
        let plain = key
            .open_in_place(nonce, aead::Aad::empty(), &mut buf)
            .map_err(|_| CredentialError::Storage("decrypt failed".to_string()))?;
        String::from_utf8(plain.to_vec())
            .map_err(|_| CredentialError::Storage("credential is not valid text".to_string()))
    }
}

fn load_or_create_key(path: &PathBuf) -> Result<[u8; 32], CredentialError> {
    match fs::read(path) {
        Ok(bytes) if bytes.len() == 32 => Ok(bytes.try_into().expect("length checked")),
        Ok(_) => Err(CredentialError::Storage(
            "vault key has unexpected length".to_string(),
        )),
        Err(err) if err.kind() == io::ErrorKind::NotFound => {
            let mut key = [0u8; 32];
            SystemRandom::new()
                .fill(&mut key)
                .map_err(|_| CredentialError::Storage("random source unavailable".to_string()))?;
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)?;
            }
            let mut opts = fs::OpenOptions::new();
            opts.write(true).create_new(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                opts.mode(0o600);
            }
            let mut file = opts.open(path)?;
            file.write_all(&key)?;
            Ok(key)
        }
        Err(err) => Err(err.into()),
    }
}

fn write_private(path: &PathBuf, data: &[u8]) -> Result<(), CredentialError> {
    let mut opts = fs::OpenOptions::new();
    opts.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    let mut file = opts.open(path)?;
    file.write_all(data)?;
    file.sync_all()?;
    Ok(())
}

fn hex_encode(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(DIGITS[(byte >> 4) as usize] as char);
        out.push(DIGITS[(byte & 0x0f) as usize] as char);
    }
    out
}

fn hex_decode(text: &str) -> Result<Vec<u8>, CredentialError> {
    if text.len() % 2 != 0 {
        return Err(CredentialError::Storage(
            "vault entry has odd length".to_string(),
        ));
    }
    let mut out = Vec::with_capacity(text.len() / 2);
    let bytes = text.as_bytes();
    for pair in bytes.chunks(2) {
        let hi = hex_digit(pair[0])?;
        let lo = hex_digit(pair[1])?;
        out.push((hi << 4) | lo);
    }
    Ok(out)
}

fn hex_digit(byte: u8) -> Result<u8, CredentialError> {
    match byte {
        b'0'..=b'9' => Ok(byte - b'0'),
        b'a'..=b'f' => Ok(byte - b'a' + 10),
        b'A'..=b'F' => Ok(byte - b'A' + 10),
        _ => Err(CredentialError::Storage(
            "vault entry is not valid hex".to_string(),
        )),
    }
}

#[derive(Debug, Clone)]
pub struct AuditEvent {
    pub actor: String,
    pub action: String,
    pub target: String,
    pub detail: String,
}

pub trait AuditLog: Send + Sync {
    fn record(&self, event: AuditEvent);
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decision {
    Allow,
    Deny,
    Ask,
}

#[derive(Debug, Clone)]
pub struct Action {
    pub kind: String,
    pub target: String,
}

pub trait PermissionEngine: Send + Sync {
    fn check(&self, action: &Action) -> Decision;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HostKeyVerdict {
    Trusted,
    Unknown,
    Mismatch,
}

pub trait HostKeyVerifier: Send + Sync {
    fn verify(&self, host: &str, port: u16, fingerprint: &str) -> HostKeyVerdict;
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TempDir(PathBuf);

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn temp_store(name: &str) -> (TempDir, CredentialStore) {
        let dir = std::env::temp_dir().join(format!(
            "ferriterm-cred-{name}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = fs::remove_dir_all(&dir);
        let store = CredentialStore {
            dir: dir.clone(),
            use_keyring: false,
        };
        (TempDir(dir), store)
    }

    #[test]
    fn encrypted_vault_roundtrip() {
        let (_guard, store) = temp_store("roundtrip");
        store.set("host-1", "s3cret").unwrap();
        assert_eq!(store.get("host-1").unwrap().as_deref(), Some("s3cret"));
        assert_eq!(store.get("missing").unwrap(), None);

        let raw = fs::read_to_string(store.vault_path()).unwrap();
        assert!(!raw.contains("s3cret"));
    }

    #[test]
    fn delete_removes_entry() {
        let (_guard, store) = temp_store("delete");
        store.set("host-1", "s3cret").unwrap();
        store.delete("host-1").unwrap();
        assert_eq!(store.get("host-1").unwrap(), None);
    }

    #[test]
    fn vault_key_is_stable() {
        let (_guard, store) = temp_store("stable");
        store.set("a", "one").unwrap();
        let second = CredentialStore {
            dir: store.dir.clone(),
            use_keyring: false,
        };
        assert_eq!(second.get("a").unwrap().as_deref(), Some("one"));
    }
}
