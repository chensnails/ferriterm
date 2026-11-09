//! Credential vault: encrypted local file by default, optional master password.
//!
//! Secrets are keyed by opaque reference strings and stored only in an
//! encrypted file under the app data directory; the system keychain is never
//! touched. Without a master password the file is encrypted with a random
//! device key, so protection matches file permissions. In master-password mode
//! the file is encrypted with a PBKDF2-derived key that only exists in memory
//! while the vault is unlocked.

use std::collections::BTreeMap;
use std::fmt;
use std::fs;
use std::io;
use std::io::Write;
use std::num::NonZeroU32;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use ring::aead;
use ring::digest;
use ring::pbkdf2;
use ring::rand::{SecureRandom, SystemRandom};
use serde::{Deserialize, Serialize};

pub mod update;

const KEY_FILE: &str = "credentials.key";
const VAULT_FILE: &str = "credentials.vault.json";
const INDEX_FILE: &str = "credentials.index.json";
const MASTER_FILE: &str = "vault.master.json";
const NONCE_LEN: usize = 12;
const KDF_ITERATIONS: u32 = 600_000;
const MAX_FAILURES_BEFORE_DELAY: u32 = 5;

#[derive(Debug)]
pub enum CredentialError {
    Storage(String),
    Locked,
    WrongPassword,
    NotEnabled,
    AlreadyEnabled,
    TooManyAttempts(u64),
}

impl fmt::Display for CredentialError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CredentialError::Storage(msg) => write!(f, "credential storage error: {msg}"),
            CredentialError::Locked => write!(f, "vault is locked"),
            CredentialError::WrongPassword => write!(f, "master password is incorrect"),
            CredentialError::NotEnabled => write!(f, "master password is not enabled"),
            CredentialError::AlreadyEnabled => write!(f, "master password is already enabled"),
            CredentialError::TooManyAttempts(seconds) => {
                write!(f, "too many attempts, retry in {seconds}s")
            }
        }
    }
}

impl std::error::Error for CredentialError {}

impl From<io::Error> for CredentialError {
    fn from(err: io::Error) -> Self {
        CredentialError::Storage(err.to_string())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum VaultMode {
    Local,
    Master,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VaultStatus {
    pub mode: VaultMode,
    pub locked: bool,
    pub entries: usize,
}

#[derive(Debug, Serialize, Deserialize)]
struct MasterFile {
    version: u32,
    salt: String,
    iterations: u32,
    verifier: String,
}

struct Inner {
    key: Option<[u8; 32]>,
    mode: VaultMode,
    locked: bool,
    failures: u32,
    blocked_until: Option<Instant>,
}

pub struct CredentialStore {
    dir: PathBuf,
    inner: Mutex<Inner>,
}

impl fmt::Debug for CredentialStore {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CredentialStore")
            .field("dir", &self.dir)
            .finish_non_exhaustive()
    }
}

impl CredentialStore {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        let dir = dir.into();
        let (mode, locked) = if dir.join(MASTER_FILE).exists() {
            (VaultMode::Master, true)
        } else {
            (VaultMode::Local, false)
        };
        Self {
            dir,
            inner: Mutex::new(Inner {
                key: None,
                mode,
                locked,
                failures: 0,
                blocked_until: None,
            }),
        }
    }

    pub fn status(&self) -> VaultStatus {
        let inner = self.inner.lock().unwrap();
        VaultStatus {
            mode: inner.mode,
            locked: inner.locked,
            entries: self.read_vault().map(|vault| vault.len()).unwrap_or(0),
        }
    }

    pub fn is_locked(&self) -> bool {
        self.inner.lock().unwrap().locked
    }

    pub fn has_ref(&self, id: &str) -> bool {
        self.read_index().iter().any(|item| item == id)
    }

    pub fn refs(&self) -> Vec<String> {
        self.read_index()
    }

    pub fn register_ref(&self, id: &str) {
        if id.is_empty() {
            return;
        }
        let mut index = self.read_index();
        if index.iter().any(|item| item == id) {
            return;
        }
        index.push(id.to_string());
        let _ = self.write_index(&index);
    }

    pub fn get(&self, id: &str) -> Result<Option<String>, CredentialError> {
        let key = self.active_key()?;
        match self.read_vault()?.get(id) {
            Some(blob) => Ok(Some(self.decrypt_with(&key, blob)?)),
            None => Ok(None),
        }
    }

    pub fn set(&self, id: &str, secret: &str) -> Result<(), CredentialError> {
        let key = self.active_key()?;
        let mut vault = self.read_vault()?;
        vault.insert(id.to_string(), self.encrypt_with(&key, secret)?);
        self.write_vault(&vault)?;
        self.register_ref(id);
        Ok(())
    }

    pub fn delete(&self, id: &str) -> Result<(), CredentialError> {
        self.remove_entry(id)?;
        let index: Vec<String> = self
            .read_index()
            .into_iter()
            .filter(|item| item != id)
            .collect();
        self.write_index(&index)
    }

    /// Reports whether a reference exists without decrypting it. Used for
    /// "saved" badges while the vault is locked.
    pub fn exists(&self, id: &str) -> bool {
        self.has_ref(id) || matches!(self.read_vault(), Ok(vault) if vault.contains_key(id))
    }

    pub fn unlock(&self, password: &str) -> Result<(), CredentialError> {
        {
            let inner = self.inner.lock().unwrap();
            if inner.mode != VaultMode::Master {
                return Err(CredentialError::NotEnabled);
            }
            if !inner.locked {
                return Ok(());
            }
            if let Some(until) = inner.blocked_until {
                let now = Instant::now();
                if until > now {
                    return Err(CredentialError::TooManyAttempts(
                        until.duration_since(now).as_secs().max(1),
                    ));
                }
            }
        }
        let master = self.read_master()?.ok_or(CredentialError::NotEnabled)?;
        let key = derive_key(password, &master)?;
        if !verify_key(&key, &master.verifier) {
            let mut inner = self.inner.lock().unwrap();
            inner.failures = inner.failures.saturating_add(1);
            if inner.failures >= MAX_FAILURES_BEFORE_DELAY {
                let delay = failure_delay(inner.failures);
                inner.blocked_until = Some(Instant::now() + Duration::from_secs(delay));
            }
            return Err(CredentialError::WrongPassword);
        }
        let mut inner = self.inner.lock().unwrap();
        inner.key = Some(key);
        inner.locked = false;
        inner.failures = 0;
        inner.blocked_until = None;
        Ok(())
    }

    pub fn lock(&self) {
        let mut inner = self.inner.lock().unwrap();
        if inner.mode == VaultMode::Master {
            inner.key = None;
            inner.locked = true;
        }
    }

    /// Moves every known reference into a master-password vault. The caller
    /// passes the references it knows about because the index only tracks
    /// secrets written by this app.
    pub fn enable_master_password(
        &self,
        password: &str,
        extra_refs: &[String],
    ) -> Result<usize, CredentialError> {
        if self.inner.lock().unwrap().mode == VaultMode::Master {
            return Err(CredentialError::AlreadyEnabled);
        }
        if password.is_empty() {
            return Err(CredentialError::Storage(
                "master password must not be empty".to_string(),
            ));
        }
        let mut refs = self.read_index();
        for item in extra_refs {
            if !item.is_empty() && !refs.iter().any(|existing| existing == item) {
                refs.push(item.clone());
            }
        }
        let stored = self.read_vault()?;
        for id in stored.keys() {
            if !refs.iter().any(|item| item == id) {
                refs.push(id.clone());
            }
        }
        let mut plain: Vec<(String, String)> = Vec::new();
        for id in &refs {
            if let Some(secret) = self.get(id)? {
                plain.push((id.clone(), secret));
            }
        }
        let migrated: Vec<String> = plain.iter().map(|(id, _)| id.clone()).collect();

        let mut salt = [0u8; 16];
        SystemRandom::new()
            .fill(&mut salt)
            .map_err(|_| CredentialError::Storage("random source unavailable".to_string()))?;
        let mut master = MasterFile {
            version: 1,
            salt: hex_encode(&salt),
            iterations: KDF_ITERATIONS,
            verifier: String::new(),
        };
        let key = derive_key(password, &master)?;
        master.verifier = verifier(&key);

        let mut vault = BTreeMap::new();
        for (id, secret) in &plain {
            vault.insert(id.clone(), self.encrypt_with(&key, secret)?);
        }

        let raw = serde_json::to_vec(&master)
            .map_err(|e| CredentialError::Storage(format!("serialize master file: {e}")))?;
        write_private(&self.master_path(), &raw)?;
        self.write_vault(&vault)?;
        self.write_index(&migrated)?;
        let _ = fs::remove_file(self.key_path());

        let mut inner = self.inner.lock().unwrap();
        inner.mode = VaultMode::Master;
        inner.key = Some(key);
        inner.locked = false;
        inner.failures = 0;
        inner.blocked_until = None;
        Ok(plain.len())
    }

    pub fn disable_master_password(&self, password: &str) -> Result<(), CredentialError> {
        if self.inner.lock().unwrap().mode != VaultMode::Master {
            return Err(CredentialError::NotEnabled);
        }
        let master = self.read_master()?.ok_or(CredentialError::NotEnabled)?;
        let key = derive_key(password, &master)?;
        if !verify_key(&key, &master.verifier) {
            return Err(CredentialError::WrongPassword);
        }
        let vault = self.read_vault()?;
        let mut plain: Vec<(String, String)> = Vec::new();
        for (id, blob) in &vault {
            plain.push((id.clone(), self.decrypt_with(&key, blob)?));
        }

        let device_key = self.device_key()?;
        let mut next = BTreeMap::new();
        for (id, secret) in &plain {
            next.insert(id.clone(), self.encrypt_with(&device_key, secret)?);
        }
        self.write_vault(&next)?;
        let stored: Vec<String> = next.keys().cloned().collect();
        self.write_index(&stored)?;
        let _ = fs::remove_file(self.master_path());
        let mut inner = self.inner.lock().unwrap();
        inner.mode = VaultMode::Local;
        inner.key = None;
        inner.locked = false;
        inner.failures = 0;
        inner.blocked_until = None;
        Ok(())
    }

    pub fn change_master_password(&self, current: &str, next: &str) -> Result<(), CredentialError> {
        if next.is_empty() {
            return Err(CredentialError::Storage(
                "master password must not be empty".to_string(),
            ));
        }
        if self.inner.lock().unwrap().mode != VaultMode::Master {
            return Err(CredentialError::NotEnabled);
        }
        let master = self.read_master()?.ok_or(CredentialError::NotEnabled)?;
        let old_key = derive_key(current, &master)?;
        if !verify_key(&old_key, &master.verifier) {
            return Err(CredentialError::WrongPassword);
        }
        let vault = self.read_vault()?;

        let mut salt = [0u8; 16];
        SystemRandom::new()
            .fill(&mut salt)
            .map_err(|_| CredentialError::Storage("random source unavailable".to_string()))?;
        let mut fresh = MasterFile {
            version: 1,
            salt: hex_encode(&salt),
            iterations: KDF_ITERATIONS,
            verifier: String::new(),
        };
        let new_key = derive_key(next, &fresh)?;
        fresh.verifier = verifier(&new_key);

        let mut rotated = BTreeMap::new();
        for (id, blob) in &vault {
            let secret = self.decrypt_with(&old_key, blob)?;
            rotated.insert(id.clone(), self.encrypt_with(&new_key, &secret)?);
        }
        self.write_vault(&rotated)?;
        let raw = serde_json::to_vec(&fresh)
            .map_err(|e| CredentialError::Storage(format!("serialize master file: {e}")))?;
        write_private(&self.master_path(), &raw)?;

        let mut inner = self.inner.lock().unwrap();
        inner.key = Some(new_key);
        inner.locked = false;
        inner.failures = 0;
        inner.blocked_until = None;
        Ok(())
    }

    /// Wipes the vault, its key material and the index. Unrecoverable.
    pub fn reset(&self) -> Result<(), CredentialError> {
        for path in [
            self.vault_path(),
            self.key_path(),
            self.index_path(),
            self.master_path(),
        ] {
            match fs::remove_file(&path) {
                Ok(()) => {}
                Err(err) if err.kind() == io::ErrorKind::NotFound => {}
                Err(err) => return Err(err.into()),
            }
        }
        let mut inner = self.inner.lock().unwrap();
        inner.mode = VaultMode::Local;
        inner.key = None;
        inner.locked = false;
        inner.failures = 0;
        inner.blocked_until = None;
        Ok(())
    }

    fn active_key(&self) -> Result<[u8; 32], CredentialError> {
        let mode_key = {
            let inner = self.inner.lock().unwrap();
            match inner.mode {
                VaultMode::Master => {
                    if inner.locked {
                        return Err(CredentialError::Locked);
                    }
                    Some(inner.key.ok_or(CredentialError::Locked)?)
                }
                VaultMode::Local => None,
            }
        };
        match mode_key {
            Some(key) => Ok(key),
            None => self.device_key(),
        }
    }

    fn remove_entry(&self, id: &str) -> Result<(), CredentialError> {
        let mut vault = self.read_vault()?;
        if vault.remove(id).is_some() {
            self.write_vault(&vault)?;
        }
        Ok(())
    }

    fn master_path(&self) -> PathBuf {
        self.dir.join(MASTER_FILE)
    }

    fn vault_path(&self) -> PathBuf {
        self.dir.join(VAULT_FILE)
    }

    fn key_path(&self) -> PathBuf {
        self.dir.join(KEY_FILE)
    }

    fn index_path(&self) -> PathBuf {
        self.dir.join(INDEX_FILE)
    }

    fn read_master(&self) -> Result<Option<MasterFile>, CredentialError> {
        match fs::read_to_string(self.master_path()) {
            Ok(raw) => serde_json::from_str(&raw)
                .map(Some)
                .map_err(|e| CredentialError::Storage(format!("corrupt master file: {e}"))),
            Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(err) => Err(err.into()),
        }
    }

    fn read_index(&self) -> Vec<String> {
        match fs::read_to_string(self.index_path()) {
            Ok(raw) => serde_json::from_str(&raw).unwrap_or_default(),
            Err(_) => Vec::new(),
        }
    }

    fn write_index(&self, index: &[String]) -> Result<(), CredentialError> {
        fs::create_dir_all(&self.dir)?;
        let raw = serde_json::to_vec(index)
            .map_err(|e| CredentialError::Storage(format!("serialize index: {e}")))?;
        write_private(&self.index_path(), &raw)
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

    fn device_key(&self) -> Result<[u8; 32], CredentialError> {
        load_or_create_key(&self.key_path())
    }

    fn encrypt_with(&self, key: &[u8; 32], plain: &str) -> Result<String, CredentialError> {
        let unbound = aead::UnboundKey::new(&aead::CHACHA20_POLY1305, key)
            .map_err(|_| CredentialError::Storage("invalid vault key".to_string()))?;
        let key = aead::LessSafeKey::new(unbound);
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

    fn decrypt_with(&self, key: &[u8; 32], blob: &str) -> Result<String, CredentialError> {
        let raw = hex_decode(blob)?;
        if raw.len() <= NONCE_LEN {
            return Err(CredentialError::Storage(
                "truncated vault entry".to_string(),
            ));
        }
        let (nonce_bytes, cipher) = raw.split_at(NONCE_LEN);
        let nonce = aead::Nonce::try_assume_unique_for_key(nonce_bytes)
            .map_err(|_| CredentialError::Storage("invalid nonce".to_string()))?;
        let unbound = aead::UnboundKey::new(&aead::CHACHA20_POLY1305, key)
            .map_err(|_| CredentialError::Storage("invalid vault key".to_string()))?;
        let key = aead::LessSafeKey::new(unbound);
        let mut buf = cipher.to_vec();
        let plain = key
            .open_in_place(nonce, aead::Aad::empty(), &mut buf)
            .map_err(|_| CredentialError::Storage("decrypt failed".to_string()))?;
        String::from_utf8(plain.to_vec())
            .map_err(|_| CredentialError::Storage("credential is not valid text".to_string()))
    }
}

fn derive_key(password: &str, master: &MasterFile) -> Result<[u8; 32], CredentialError> {
    let salt = hex_decode(&master.salt)?;
    let iterations = NonZeroU32::new(master.iterations)
        .ok_or_else(|| CredentialError::Storage("invalid KDF iterations".to_string()))?;
    let mut key = [0u8; 32];
    pbkdf2::derive(
        pbkdf2::PBKDF2_HMAC_SHA256,
        iterations,
        &salt,
        password.as_bytes(),
        &mut key,
    );
    Ok(key)
}

fn verifier(key: &[u8; 32]) -> String {
    let mut input = key.to_vec();
    input.extend_from_slice(b"ferriterm.vault.v1");
    hex_encode(digest::digest(&digest::SHA256, &input).as_ref())
}

fn verify_key(key: &[u8; 32], expected: &str) -> bool {
    let actual = verifier(key);
    constant_time_eq(actual.as_bytes(), expected.as_bytes())
}

fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    let mut diff = 0u8;
    for (a, b) in left.iter().zip(right) {
        diff |= a ^ b;
    }
    diff == 0
}

fn failure_delay(failures: u32) -> u64 {
    let step = failures.saturating_sub(MAX_FAILURES_BEFORE_DELAY).min(6);
    1u64 << step
}

fn load_or_create_key(path: &Path) -> Result<[u8; 32], CredentialError> {
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

fn write_private(path: &Path, data: &[u8]) -> Result<(), CredentialError> {
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
            inner: Mutex::new(Inner {
                key: None,
                mode: VaultMode::Local,
                locked: false,
                failures: 0,
                blocked_until: None,
            }),
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
        assert!(store.has_ref("host-1"));
    }

    #[test]
    fn delete_removes_entry() {
        let (_guard, store) = temp_store("delete");
        store.set("host-1", "s3cret").unwrap();
        store.delete("host-1").unwrap();
        assert_eq!(store.get("host-1").unwrap(), None);
        assert!(!store.has_ref("host-1"));
    }

    #[test]
    fn vault_key_is_stable() {
        let (_guard, store) = temp_store("stable");
        store.set("a", "one").unwrap();
        let second = CredentialStore::new(store.dir.clone());
        assert_eq!(second.get("a").unwrap().as_deref(), Some("one"));
    }

    #[test]
    fn master_password_migrates_and_locks() {
        let (guard, store) = temp_store("master");
        store.set("host-1", "pw").unwrap();
        store.set("host:1:passphrase", "pp").unwrap();

        let migrated = store.enable_master_password("hunter2", &[]).unwrap();
        assert_eq!(migrated, 2);
        assert_eq!(store.status().mode, VaultMode::Master);
        assert!(!store.is_locked());
        assert_eq!(store.get("host-1").unwrap().as_deref(), Some("pw"));

        store.lock();
        assert_eq!(
            store.get("host-1").unwrap_err().to_string(),
            "vault is locked"
        );

        let reopened = CredentialStore::new(guard.0.clone());
        assert!(reopened.is_locked());
        assert!(matches!(
            reopened.unlock("wrong"),
            Err(CredentialError::WrongPassword)
        ));
        reopened.unlock("hunter2").unwrap();
        assert_eq!(reopened.get("host-1").unwrap().as_deref(), Some("pw"));
    }

    #[test]
    fn enable_ignores_refs_without_secrets() {
        let (_guard, store) = temp_store("phantom");
        store.set("host-1", "pw").unwrap();
        let extra = vec![
            "host-1".to_string(),
            "host-2".to_string(),
            "sync:gist-token".to_string(),
        ];
        let migrated = store.enable_master_password("hunter2", &extra).unwrap();
        assert_eq!(migrated, 1);
        assert_eq!(store.refs(), vec!["host-1".to_string()]);
        assert_eq!(store.status().entries, 1);

        store.disable_master_password("hunter2").unwrap();
        assert_eq!(store.refs(), vec!["host-1".to_string()]);
        assert_eq!(store.status().entries, 1);
    }

    #[test]
    fn master_password_change_and_disable() {
        let (_guard, store) = temp_store("change");
        store.set("a", "one").unwrap();
        store.enable_master_password("first-pass", &[]).unwrap();
        store.lock();
        store.unlock("first-pass").unwrap();
        store
            .change_master_password("first-pass", "second-pass")
            .unwrap();
        store.lock();
        assert!(store.unlock("first-pass").is_err());
        store.unlock("second-pass").unwrap();
        assert_eq!(store.get("a").unwrap().as_deref(), Some("one"));

        store.disable_master_password("second-pass").unwrap();
        assert_eq!(store.status().mode, VaultMode::Local);
        assert!(!store.is_locked());
        assert_eq!(store.get("a").unwrap().as_deref(), Some("one"));
        assert!(!store.master_path().exists());
    }

    #[test]
    fn reset_wipes_everything() {
        let (_guard, store) = temp_store("reset");
        store.set("a", "one").unwrap();
        store.enable_master_password("pw-pass", &[]).unwrap();
        store.reset().unwrap();
        assert_eq!(store.status().mode, VaultMode::Local);
        assert_eq!(store.status().entries, 0);
        assert!(!store.vault_path().exists());
        assert_eq!(store.get("a").unwrap(), None);
    }

    #[test]
    fn delete_works_while_locked() {
        let (_guard, store) = temp_store("locked-delete");
        store.set("a", "one").unwrap();
        store.enable_master_password("pw-pass", &[]).unwrap();
        store.lock();
        store.delete("a").unwrap();
        store.unlock("pw-pass").unwrap();
        assert_eq!(store.get("a").unwrap(), None);
    }
}
