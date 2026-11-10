//! Passphrase-sealed backup bundles: PBKDF2-HMAC-SHA256 + ChaCha20-Poly1305,
//! plus remote transports (GitHub Gist, WebDAV) for the sealed bundles.

pub mod remote;
pub mod versions;

pub use remote::{
    gist_download, gist_test, gist_upload, webdav_download, webdav_test, webdav_upload, GistConfig,
    RemoteError, RemoteUpload, WebdavConfig,
};
pub use versions::{
    VersionEntry, VersionIndex, VersionSource, VersionStore, INDEX_VERSION, MAX_VERSIONS,
};

use std::fmt;
use std::fs;
use std::num::NonZeroU32;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use ring::aead;
use ring::pbkdf2;
use ring::rand::{SecureRandom, SystemRandom};

const MAGIC: &str = "FTBAK1";
const SALT_LEN: usize = 16;
const NONCE_LEN: usize = 12;
const ITERATIONS: u32 = 210_000;

#[derive(Debug)]
pub enum SyncError {
    Crypto(&'static str),
    Io(std::io::Error),
    Format(String),
}

impl fmt::Display for SyncError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SyncError::Crypto(msg) => write!(f, "{msg}"),
            SyncError::Io(err) => write!(f, "backup I/O error: {err}"),
            SyncError::Format(msg) => write!(f, "{msg}"),
        }
    }
}

impl std::error::Error for SyncError {}

impl From<std::io::Error> for SyncError {
    fn from(err: std::io::Error) -> Self {
        SyncError::Io(err)
    }
}

#[derive(Debug, Clone)]
pub struct BackupStore {
    dir: PathBuf,
}

impl BackupStore {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    pub fn export(&self, passphrase: &str, payload: &[u8]) -> Result<PathBuf, SyncError> {
        let sealed = seal(passphrase, payload)?;
        self.write_bundle(&sealed)
    }

    fn write_bundle(&self, sealed: &str) -> Result<PathBuf, SyncError> {
        fs::create_dir_all(&self.dir)?;
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let path = self.dir.join(format!("ferriterm-backup-{stamp}.ftbak"));
        fs::write(&path, sealed)?;
        Ok(path)
    }

    /// Version history lives under the same backups directory as exported
    /// `.ftbak` files, so it follows the app data directory automatically.
    pub fn versions(&self) -> VersionStore {
        VersionStore::new(self.dir.join("versions"))
    }

    /// Seals once, writes the standalone export and records it in the version
    /// history. Used by the manual export button.
    pub fn export_versioned(
        &self,
        passphrase: &str,
        payload: &[u8],
        scope: Vec<String>,
        summary: serde_json::Value,
        note: &str,
    ) -> Result<(PathBuf, VersionEntry), SyncError> {
        let sealed = seal(passphrase, payload)?;
        let path = self.write_bundle(&sealed)?;
        let entry = self.record_version(&sealed, scope, summary, VersionSource::Local, note)?;
        Ok((path, entry))
    }

    /// Records an already-sealed bundle (a remote sync) in local history.
    pub fn record_version(
        &self,
        sealed: &str,
        scope: Vec<String>,
        summary: serde_json::Value,
        source: VersionSource,
        note: &str,
    ) -> Result<VersionEntry, SyncError> {
        self.versions().add(sealed, scope, summary, source, note)
    }
}

pub fn seal(passphrase: &str, plaintext: &[u8]) -> Result<String, SyncError> {
    if passphrase.is_empty() {
        return Err(SyncError::Crypto("passphrase is required"));
    }
    let rng = SystemRandom::new();
    let mut salt = [0u8; SALT_LEN];
    let mut nonce_bytes = [0u8; NONCE_LEN];
    rng.fill(&mut salt)
        .map_err(|_| SyncError::Crypto("random source unavailable"))?;
    rng.fill(&mut nonce_bytes)
        .map_err(|_| SyncError::Crypto("random source unavailable"))?;

    let key = derive(passphrase, &salt)?;
    let nonce = aead::Nonce::assume_unique_for_key(nonce_bytes);
    let mut out = plaintext.to_vec();
    key.seal_in_place_append_tag(nonce, aead::Aad::from(MAGIC.as_bytes()), &mut out)
        .map_err(|_| SyncError::Crypto("seal failed"))?;

    let mut blob = Vec::with_capacity(SALT_LEN + NONCE_LEN + out.len());
    blob.extend_from_slice(&salt);
    blob.extend_from_slice(&nonce_bytes);
    blob.extend_from_slice(&out);
    Ok(format!("{MAGIC}\n{}", hex_encode(&blob)))
}

pub fn open(passphrase: &str, text: &str) -> Result<Vec<u8>, SyncError> {
    let body = text
        .strip_prefix(MAGIC)
        .and_then(|rest| rest.strip_prefix('\n'))
        .ok_or_else(|| SyncError::Format("not a Ferriterm backup bundle".to_string()))?;
    let raw = hex_decode(body.trim())?;
    if raw.len() <= SALT_LEN + NONCE_LEN {
        return Err(SyncError::Format("truncated backup bundle".to_string()));
    }
    let (salt, rest) = raw.split_at(SALT_LEN);
    let (nonce_bytes, cipher) = rest.split_at(NONCE_LEN);
    let key = derive(passphrase, salt)?;
    let nonce = aead::Nonce::try_assume_unique_for_key(nonce_bytes)
        .map_err(|_| SyncError::Format("invalid nonce".to_string()))?;
    let mut buf = cipher.to_vec();
    let plain = key
        .open_in_place(nonce, aead::Aad::from(MAGIC.as_bytes()), &mut buf)
        .map_err(|_| SyncError::Crypto("wrong passphrase or corrupted backup"))?;
    Ok(plain.to_vec())
}

fn derive(passphrase: &str, salt: &[u8]) -> Result<aead::LessSafeKey, SyncError> {
    let mut key = [0u8; 32];
    pbkdf2::derive(
        pbkdf2::PBKDF2_HMAC_SHA256,
        NonZeroU32::new(ITERATIONS).expect("nonzero iterations"),
        salt,
        passphrase.as_bytes(),
        &mut key,
    );
    let unbound = aead::UnboundKey::new(&aead::CHACHA20_POLY1305, &key)
        .map_err(|_| SyncError::Crypto("invalid bundle key"))?;
    Ok(aead::LessSafeKey::new(unbound))
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

fn hex_decode(text: &str) -> Result<Vec<u8>, SyncError> {
    if text.len() % 2 != 0 {
        return Err(SyncError::Format(
            "bundle payload has odd length".to_string(),
        ));
    }
    let mut out = Vec::with_capacity(text.len() / 2);
    for pair in text.as_bytes().chunks(2) {
        let hi = hex_digit(pair[0])?;
        let lo = hex_digit(pair[1])?;
        out.push((hi << 4) | lo);
    }
    Ok(out)
}

fn hex_digit(byte: u8) -> Result<u8, SyncError> {
    match byte {
        b'0'..=b'9' => Ok(byte - b'0'),
        b'a'..=b'f' => Ok(byte - b'a' + 10),
        b'A'..=b'F' => Ok(byte - b'A' + 10),
        _ => Err(SyncError::Format(
            "bundle payload is not valid hex".to_string(),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seal_open_roundtrip() {
        let sealed = seal("correct horse", b"{\"hosts\":[]}").unwrap();
        assert!(sealed.starts_with("FTBAK1\n"));
        assert_eq!(open("correct horse", &sealed).unwrap(), b"{\"hosts\":[]}");
    }

    #[test]
    fn wrong_passphrase_fails() {
        let sealed = seal("one", b"payload").unwrap();
        assert!(open("two", &sealed).is_err());
    }

    #[test]
    fn tampered_bundle_fails() {
        let sealed = seal("pass", b"payload").unwrap();
        let mut tampered = sealed.clone().into_bytes();
        let last = tampered.len() - 1;
        tampered[last] = if tampered[last] == b'a' { b'b' } else { b'a' };
        let text = String::from_utf8(tampered).unwrap();
        assert!(open("pass", &text).is_err());
    }

    #[test]
    fn rejects_foreign_payload() {
        assert!(open("pass", "not-a-bundle").is_err());
    }
}
