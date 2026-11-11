//! Credential vault: encrypted local file by default, optional master password.
//!
//! Secrets are keyed by opaque reference strings and stored only in an
//! encrypted file under the app data directory; the system keychain is never
//! touched. Without a master password the file is encrypted with a random
//! device key, so protection matches file permissions. In master-password mode
//! the file is encrypted with a PBKDF2-derived key that only exists in memory
//! while the vault is unlocked.
//!
//! Every private file is replaced atomically (tmp + fsync + rename) and keeps
//! its previous generation in a `.bak` sidecar, so a crash mid-write or midway
//! through a mode switch never leaves an undecryptable vault: startup and
//! unlock roll back to the last consistent generation.

use std::collections::BTreeMap;
use std::fmt;
use std::fs;
use std::io;
use std::io::Write;
use std::num::NonZeroU32;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

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
const THROTTLE_FILE: &str = "vault.throttle.json";
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

/// Persisted password-throttle state. Restarting the app must not hand an
/// attacker back a clean budget after five rejected guesses.
#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ThrottleFile {
    #[serde(default)]
    failures: u32,
    /// Absolute unix milliseconds; wall-clock so it survives reboots.
    #[serde(default)]
    blocked_until: Option<u64>,
}

/// A derived vault key that wipes its bytes when dropped, so plaintext key
/// material does not linger in freed heap memory.
#[derive(Clone)]
struct SecureKey([u8; 32]);

impl std::ops::Deref for SecureKey {
    type Target = [u8; 32];
    fn deref(&self) -> &[u8; 32] {
        &self.0
    }
}

impl Drop for SecureKey {
    fn drop(&mut self) {
        for byte in self.0.iter_mut() {
            // SAFETY: unique borrow through &mut self; volatile prevents the optimizer eliding the wipe.
            unsafe { std::ptr::write_volatile(byte, 0) };
        }
    }
}

struct Inner {
    key: Option<SecureKey>,
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
        let store = Self {
            dir,
            inner: Mutex::new(Inner {
                key: None,
                mode,
                locked,
                failures: 0,
                blocked_until: None,
            }),
        };
        let throttle = store.read_throttle();
        let blocked_until = throttle.blocked_until.and_then(|until| {
            let now = now_millis();
            (until > now).then(|| Instant::now() + Duration::from_millis(until - now))
        });
        {
            let mut inner = store.inner.lock().unwrap();
            inner.failures = throttle.failures;
            inner.blocked_until = blocked_until;
        }
        store.recover();
        store
    }

    /// Repairs crash windows left by interrupted writes or mode switches.
    /// Master-mode windows need the password and are repaired in `unlock`.
    fn recover(&self) {
        for base in [VAULT_FILE, INDEX_FILE, MASTER_FILE, KEY_FILE] {
            for suffix in ["tmp", "bak.tmp"] {
                let _ = fs::remove_file(self.dir.join(format!("{base}.{suffix}")));
            }
        }
        if self.inner.lock().unwrap().mode != VaultMode::Local {
            return;
        }
        if !self.vault_path().exists() {
            return;
        }
        let Ok(device) = self.read_device_key() else {
            return;
        };
        if self.vault_decrypts_with(&device) {
            return;
        }
        // Interrupted enable: the vault already holds master-key ciphertext
        // while the master file was never written. The backup still decrypts
        // with the device key, so roll back to it.
        if self.bak_decrypts_with(&device) {
            let _ = self.restore_vault_from_bak();
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
        }
        self.check_throttle()?;
        let master = self.read_master()?.ok_or(CredentialError::NotEnabled)?;
        let key = derive_key(password, &master)?;
        if !verify_key(&key, &master.verifier) {
            self.record_failure();
            return Err(CredentialError::WrongPassword);
        }
        if !self.vault_decrypts_with(&key) {
            self.recover_master_window(&key)?;
            let inner = self.inner.lock().unwrap();
            if inner.mode == VaultMode::Local {
                return Ok(());
            }
        }
        {
            let mut inner = self.inner.lock().unwrap();
            inner.key = Some(key);
            inner.locked = false;
        }
        self.clear_failures();
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
        // Vault first: a crash before the master file lands leaves a Local-mode
        // vault whose backup still decrypts with the device key (see recover).
        self.write_vault(&vault)?;
        write_private(&self.master_path(), &raw)?;
        self.write_index(&migrated)?;
        let _ = fs::remove_file(self.key_path());

        {
            let mut inner = self.inner.lock().unwrap();
            inner.mode = VaultMode::Master;
            inner.key = Some(key);
            inner.locked = false;
        }
        self.clear_failures();
        Ok(plain.len())
    }

    pub fn disable_master_password(&self, password: &str) -> Result<(), CredentialError> {
        if self.inner.lock().unwrap().mode != VaultMode::Master {
            return Err(CredentialError::NotEnabled);
        }
        self.check_throttle()?;
        let master = self.read_master()?.ok_or(CredentialError::NotEnabled)?;
        let key = derive_key(password, &master)?;
        if !verify_key(&key, &master.verifier) {
            self.record_failure();
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
        {
            let mut inner = self.inner.lock().unwrap();
            inner.mode = VaultMode::Local;
            inner.key = None;
            inner.locked = false;
        }
        self.clear_failures();
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
        self.check_throttle()?;
        let master = self.read_master()?.ok_or(CredentialError::NotEnabled)?;
        let old_key = derive_key(current, &master)?;
        if !verify_key(&old_key, &master.verifier) {
            self.record_failure();
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

        {
            let mut inner = self.inner.lock().unwrap();
            inner.key = Some(new_key);
            inner.locked = false;
        }
        self.clear_failures();
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
        for base in [VAULT_FILE, INDEX_FILE, MASTER_FILE, KEY_FILE] {
            for suffix in ["tmp", "bak.tmp", "bak"] {
                let _ = fs::remove_file(self.dir.join(format!("{base}.{suffix}")));
            }
        }
        let _ = fs::remove_file(self.throttle_path());
        let mut inner = self.inner.lock().unwrap();
        inner.mode = VaultMode::Local;
        inner.key = None;
        inner.locked = false;
        inner.failures = 0;
        inner.blocked_until = None;
        Ok(())
    }

    fn active_key(&self) -> Result<SecureKey, CredentialError> {
        let mode_key = {
            let inner = self.inner.lock().unwrap();
            match inner.mode {
                VaultMode::Master => {
                    if inner.locked {
                        return Err(CredentialError::Locked);
                    }
                    Some(inner.key.clone().ok_or(CredentialError::Locked)?)
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

    fn throttle_path(&self) -> PathBuf {
        self.dir.join(THROTTLE_FILE)
    }

    fn read_throttle(&self) -> ThrottleFile {
        fs::read_to_string(self.throttle_path())
            .ok()
            .and_then(|raw| serde_json::from_str(&raw).ok())
            .unwrap_or_default()
    }

    /// Persists the live throttle counters using wall-clock milliseconds so the
    /// block survives both app restarts and reboots without truncating the
    /// remaining window.
    fn write_throttle(&self) {
        let (failures, blocked_until) = {
            let inner = self.inner.lock().unwrap();
            let until = inner.blocked_until.map(|instant| {
                now_millis()
                    + instant
                        .saturating_duration_since(Instant::now())
                        .as_millis() as u64
            });
            (inner.failures, until)
        };
        let raw = match serde_json::to_vec(&ThrottleFile {
            failures,
            blocked_until,
        }) {
            Ok(raw) => raw,
            Err(_) => return,
        };
        let _ = write_private(&self.throttle_path(), &raw);
    }

    fn check_throttle(&self) -> Result<(), CredentialError> {
        let inner = self.inner.lock().unwrap();
        if let Some(until) = inner.blocked_until {
            let now = Instant::now();
            if until > now {
                return Err(CredentialError::TooManyAttempts(
                    until.duration_since(now).as_secs().max(1),
                ));
            }
        }
        Ok(())
    }

    fn record_failure(&self) {
        {
            let mut inner = self.inner.lock().unwrap();
            inner.failures = inner.failures.saturating_add(1);
            if inner.failures >= MAX_FAILURES_BEFORE_DELAY {
                let delay = failure_delay(inner.failures);
                inner.blocked_until = Some(Instant::now() + Duration::from_secs(delay));
            }
        }
        self.write_throttle();
    }

    fn clear_failures(&self) {
        {
            let mut inner = self.inner.lock().unwrap();
            inner.failures = 0;
            inner.blocked_until = None;
        }
        let _ = fs::remove_file(self.throttle_path());
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
        self.read_vault_at(&self.vault_path())
    }

    fn read_vault_at(&self, path: &Path) -> Result<BTreeMap<String, String>, CredentialError> {
        match fs::read_to_string(path) {
            Ok(raw) => serde_json::from_str(&raw)
                .map_err(|e| CredentialError::Storage(format!("corrupt vault file: {e}"))),
            Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(BTreeMap::new()),
            Err(err) => Err(err.into()),
        }
    }

    fn bak_path(&self) -> PathBuf {
        PathBuf::from(format!("{}.bak", self.vault_path().display()))
    }

    fn read_device_key(&self) -> Result<SecureKey, CredentialError> {
        let bytes = fs::read(self.key_path())?;
        bytes
            .try_into()
            .map(SecureKey)
            .map_err(|_| CredentialError::Storage("vault key has unexpected length".to_string()))
    }

    fn vault_decrypts_with(&self, key: &[u8; 32]) -> bool {
        self.map_decrypts_with(&self.read_vault(), key)
    }

    fn bak_decrypts_with(&self, key: &[u8; 32]) -> bool {
        self.map_decrypts_with(&self.read_vault_at(&self.bak_path()), key)
    }

    fn map_decrypts_with(
        &self,
        vault: &Result<BTreeMap<String, String>, CredentialError>,
        key: &[u8; 32],
    ) -> bool {
        match vault {
            Ok(map) => map
                .values()
                .next()
                .is_none_or(|blob| self.decrypt_with(key, blob).is_ok()),
            Err(_) => false,
        }
    }

    fn restore_vault_from_bak(&self) -> Result<(), CredentialError> {
        let raw = fs::read(self.bak_path())?;
        atomic_replace(&self.vault_path(), &raw, false)?;
        let _ = fs::remove_file(self.bak_path());
        Ok(())
    }

    /// Repairs Master-mode crash windows once the password proved correct:
    /// a rotated vault whose master file never landed rolls back to the
    /// backup; a device-key vault left under a master file completes the
    /// interrupted disable instead of staying undecryptable.
    fn recover_master_window(&self, key: &[u8; 32]) -> Result<(), CredentialError> {
        if self.bak_decrypts_with(key) {
            return self.restore_vault_from_bak();
        }
        if let Ok(device) = self.read_device_key() {
            let vault_ok = self.vault_decrypts_with(&device);
            let bak_ok = self.bak_decrypts_with(&device);
            if vault_ok || bak_ok {
                if !vault_ok {
                    self.restore_vault_from_bak()?;
                }
                let _ = fs::remove_file(self.master_path());
                let _ = fs::remove_file(self.throttle_path());
                let mut inner = self.inner.lock().unwrap();
                inner.mode = VaultMode::Local;
                inner.key = None;
                inner.locked = false;
                inner.failures = 0;
                inner.blocked_until = None;
                return Ok(());
            }
        }
        Err(CredentialError::Storage(
            "vault is not decryptable with any known key; restore a backup".to_string(),
        ))
    }

    fn write_vault(&self, vault: &BTreeMap<String, String>) -> Result<(), CredentialError> {
        fs::create_dir_all(&self.dir)?;
        let raw = serde_json::to_vec(vault)
            .map_err(|e| CredentialError::Storage(format!("serialize vault: {e}")))?;
        write_private(&self.vault_path(), &raw)
    }

    fn device_key(&self) -> Result<SecureKey, CredentialError> {
        load_or_create_key(&self.key_path()).map(SecureKey)
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

fn derive_key(password: &str, master: &MasterFile) -> Result<SecureKey, CredentialError> {
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
    let derived = SecureKey(key);
    for byte in key.iter_mut() {
        // SAFETY: unique borrow of the local buffer; volatile write stops the
        // optimizer eliding the wipe before the stack frame is popped.
        unsafe { std::ptr::write_volatile(byte, 0) };
    }
    Ok(derived)
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

fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as u64)
        .unwrap_or_default()
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
            let tmp = PathBuf::from(format!("{}.tmp", path.display()));
            let mut opts = fs::OpenOptions::new();
            opts.write(true).create_new(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                opts.mode(0o600);
            }
            let mut file = opts.open(&tmp)?;
            file.write_all(&key)?;
            file.sync_all()?;
            drop(file);
            fs::rename(&tmp, path)?;
            sync_dir(path);
            Ok(key)
        }
        Err(err) => Err(err.into()),
    }
}

fn write_private(path: &Path, data: &[u8]) -> Result<(), CredentialError> {
    atomic_replace(path, data, true)
}

/// tmp + fsync + rename. With `keep_backup` the previous generation is first
/// copied to `<name>.bak` (itself replaced atomically) so recovery always has
/// a consistent generation to fall back to after a crash.
fn atomic_replace(path: &Path, data: &[u8], keep_backup: bool) -> Result<(), CredentialError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let tmp = PathBuf::from(format!("{}.tmp", path.display()));
    let mut opts = fs::OpenOptions::new();
    opts.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    let mut file = opts.open(&tmp)?;
    file.write_all(data)?;
    file.sync_all()?;
    drop(file);
    if keep_backup && path.exists() {
        let bak = PathBuf::from(format!("{}.bak", path.display()));
        let bak_tmp = PathBuf::from(format!("{}.bak.tmp", path.display()));
        fs::copy(path, &bak_tmp)?;
        // FlushFileBuffers needs a write handle; a read-only open fails with
        // access denied on Windows.
        fs::OpenOptions::new()
            .write(true)
            .open(&bak_tmp)?
            .sync_all()?;
        fs::rename(&bak_tmp, &bak)?;
    }
    fs::rename(&tmp, path)?;
    sync_dir(path);
    Ok(())
}

fn sync_dir(path: &Path) {
    #[cfg(unix)]
    if let Some(parent) = path.parent() {
        if let Ok(dir) = fs::File::open(parent) {
            let _ = dir.sync_all();
        }
    }
    #[cfg(not(unix))]
    let _ = path;
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

    #[test]
    fn backup_keeps_previous_generation() {
        let (_guard, store) = temp_store("backup");
        store.set("a", "one").unwrap();
        store.set("a", "two").unwrap();
        let bak: BTreeMap<String, String> =
            serde_json::from_str(&fs::read_to_string(store.bak_path()).unwrap()).unwrap();
        let key = store.read_device_key().unwrap();
        assert_eq!(store.decrypt_with(&key, &bak["a"]).unwrap(), "one");
        assert_eq!(store.get("a").unwrap().as_deref(), Some("two"));
    }

    #[test]
    fn local_recovery_restores_backup_after_corrupt_vault() {
        let (guard, store) = temp_store("corrupt");
        store.set("a", "one").unwrap();
        store.set("b", "two").unwrap();
        fs::write(store.vault_path(), b"{ truncated").unwrap();
        let reopened = CredentialStore::new(guard.0.clone());
        assert_eq!(reopened.get("a").unwrap().as_deref(), Some("one"));
    }

    #[test]
    fn enable_crash_window_rolls_back_to_local() {
        let (guard, store) = temp_store("enable-crash");
        store.set("a", "one").unwrap();
        // Crash after the master-key vault landed but before the master file:
        // vault ciphertext under an unknown key, backup still device-keyed.
        fs::copy(store.vault_path(), store.bak_path()).unwrap();
        let mut unknown = BTreeMap::new();
        unknown.insert(
            "a".to_string(),
            store.encrypt_with(&[7u8; 32], "one").unwrap(),
        );
        fs::write(store.vault_path(), serde_json::to_vec(&unknown).unwrap()).unwrap();
        let reopened = CredentialStore::new(guard.0.clone());
        assert_eq!(reopened.status().mode, VaultMode::Local);
        assert_eq!(reopened.get("a").unwrap().as_deref(), Some("one"));
    }

    #[test]
    fn disable_crash_window_rolls_back_to_master() {
        let (guard, store) = temp_store("disable-crash");
        store.set("a", "one").unwrap();
        store.enable_master_password("pw-pass", &[]).unwrap();
        // Crash after the device-key vault landed but before the master file
        // was removed: write_vault keeps the master-keyed vault as backup.
        let device = store.device_key().unwrap();
        let mut next = BTreeMap::new();
        next.insert("a".to_string(), store.encrypt_with(&device, "one").unwrap());
        store.write_vault(&next).unwrap();
        let reopened = CredentialStore::new(guard.0.clone());
        assert_eq!(reopened.status().mode, VaultMode::Master);
        reopened.unlock("pw-pass").unwrap();
        assert_eq!(reopened.get("a").unwrap().as_deref(), Some("one"));
    }

    #[test]
    fn change_crash_window_restores_old_password() {
        let (guard, store) = temp_store("change-crash");
        store.set("a", "one").unwrap();
        store.enable_master_password("first-pass", &[]).unwrap();
        let old_master = fs::read(store.master_path()).unwrap();
        store
            .change_master_password("first-pass", "second-pass")
            .unwrap();
        // Crash after the rotated vault landed but before the master file.
        fs::write(store.master_path(), old_master).unwrap();
        let reopened = CredentialStore::new(guard.0.clone());
        reopened.unlock("first-pass").unwrap();
        assert_eq!(reopened.get("a").unwrap().as_deref(), Some("one"));
        reopened.lock();
        assert!(matches!(
            reopened.unlock("second-pass"),
            Err(CredentialError::WrongPassword)
        ));
    }

    #[test]
    #[ignore = "spawned by crash_injection_during_vault_writes"]
    fn vault_write_loop() {
        let dir = std::env::var_os("FERRITERM_CRASH_DIR").expect("crash dir env");
        let store = CredentialStore::new(PathBuf::from(dir));
        let mut n = 0u64;
        loop {
            n += 1;
            store.set("k", &format!("value-{n}")).unwrap();
        }
    }

    #[test]
    fn crash_injection_during_vault_writes() {
        let dir = std::env::temp_dir().join(format!("ferriterm-crash-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let _guard = TempDir(dir.clone());
        let mut child = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--ignored",
                "--exact",
                "tests::vault_write_loop",
                "--nocapture",
            ])
            .env("FERRITERM_CRASH_DIR", &dir)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .unwrap();
        let store = CredentialStore::new(dir.clone());
        // A slow runner may need longer than a fixed pause to reach the first
        // write; wait for the vault to appear, then kill the writer mid-flight.
        let vault = store.vault_path();
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        while !vault.exists() {
            assert!(
                std::time::Instant::now() < deadline,
                "the writer never produced a vault"
            );
            std::thread::sleep(Duration::from_millis(20));
        }
        std::thread::sleep(Duration::from_millis(50));
        let _ = child.kill();
        let _ = child.wait();
        let value = store.get("k").unwrap().expect("entry survived the crash");
        assert!(value.starts_with("value-"));
        assert_eq!(store.status().entries, 1);
    }

    #[test]
    fn throttle_survives_restart() {
        let (guard, store) = temp_store("throttle-restart");
        store.set("a", "one").unwrap();
        store.enable_master_password("hunter2", &[]).unwrap();
        store.lock();
        for _ in 0..MAX_FAILURES_BEFORE_DELAY {
            assert!(matches!(
                store.unlock("wrong"),
                Err(CredentialError::WrongPassword)
            ));
        }
        drop(store);

        let reopened = CredentialStore::new(guard.0.clone());
        // Even the right password is refused while the persisted block is live.
        assert!(matches!(
            reopened.unlock("hunter2"),
            Err(CredentialError::TooManyAttempts(_))
        ));
        std::thread::sleep(Duration::from_secs(2));
        reopened.unlock("hunter2").unwrap();
        assert_eq!(reopened.get("a").unwrap().as_deref(), Some("one"));
        assert!(!reopened.throttle_path().exists());
    }

    #[test]
    fn change_and_disable_share_the_throttle() {
        let (_guard, store) = temp_store("throttle-change");
        store.enable_master_password("hunter2", &[]).unwrap();
        for _ in 0..MAX_FAILURES_BEFORE_DELAY {
            assert!(matches!(
                store.change_master_password("wrong", "x"),
                Err(CredentialError::WrongPassword)
            ));
        }
        assert!(matches!(
            store.change_master_password("hunter2", "x"),
            Err(CredentialError::TooManyAttempts(_))
        ));
        assert!(matches!(
            store.disable_master_password("hunter2"),
            Err(CredentialError::TooManyAttempts(_))
        ));
    }
}
