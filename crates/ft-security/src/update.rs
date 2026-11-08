//! Update manifest verification.
//!
//! Releases publish a `latest.json` in the Tauri updater shape, extended with a
//! SHA-256 digest per platform. The digest is checked before an artifact is
//! handed to the installer so a corrupted or swapped download is rejected.

use std::collections::BTreeMap;
use std::fmt;

use ring::digest::{digest, SHA256};
use serde::{Deserialize, Serialize};

pub const MAX_MANIFEST_BYTES: usize = 256 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct UpdateManifest {
    pub version: String,
    #[serde(default)]
    pub notes: String,
    #[serde(default)]
    pub pub_date: String,
    #[serde(default)]
    pub platforms: BTreeMap<String, UpdateTarget>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct UpdateTarget {
    pub url: String,
    #[serde(default)]
    pub signature: String,
    #[serde(default)]
    pub sha256: String,
    #[serde(default)]
    pub size: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpdateError {
    TooLarge,
    InvalidManifest(String),
    UnsupportedPlatform(String),
    InsecureUrl,
    MissingChecksum,
    ChecksumMismatch { expected: String, actual: String },
    SizeMismatch { expected: u64, actual: u64 },
}

impl fmt::Display for UpdateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            UpdateError::TooLarge => {
                write!(f, "update manifest exceeds {MAX_MANIFEST_BYTES} bytes")
            }
            UpdateError::InvalidManifest(msg) => write!(f, "invalid update manifest: {msg}"),
            UpdateError::UnsupportedPlatform(platform) => {
                write!(f, "no update published for platform {platform}")
            }
            UpdateError::InsecureUrl => write!(f, "update artifacts must use https"),
            UpdateError::MissingChecksum => write!(f, "update target has no sha256 digest"),
            UpdateError::ChecksumMismatch { expected, actual } => {
                write!(f, "checksum mismatch: expected {expected}, got {actual}")
            }
            UpdateError::SizeMismatch { expected, actual } => {
                write!(f, "size mismatch: expected {expected} bytes, got {actual}")
            }
        }
    }
}

impl std::error::Error for UpdateError {}

pub fn parse_manifest(raw: &str) -> Result<UpdateManifest, UpdateError> {
    if raw.len() > MAX_MANIFEST_BYTES {
        return Err(UpdateError::TooLarge);
    }
    let manifest: UpdateManifest =
        serde_json::from_str(raw).map_err(|err| UpdateError::InvalidManifest(err.to_string()))?;
    if manifest.version.trim().is_empty() {
        return Err(UpdateError::InvalidManifest("missing version".into()));
    }
    Ok(manifest)
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    let digest = digest(&SHA256, bytes);
    let mut out = String::with_capacity(64);
    for byte in digest.as_ref() {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

fn is_sha256_hex(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

pub fn verify_download<'a>(
    manifest: &'a UpdateManifest,
    platform: &str,
    bytes: &[u8],
) -> Result<&'a UpdateTarget, UpdateError> {
    let target = manifest
        .platforms
        .get(platform)
        .ok_or_else(|| UpdateError::UnsupportedPlatform(platform.to_string()))?;
    if !target
        .url
        .trim()
        .to_ascii_lowercase()
        .starts_with("https://")
    {
        return Err(UpdateError::InsecureUrl);
    }
    if target.sha256.trim().is_empty() {
        return Err(UpdateError::MissingChecksum);
    }
    let expected = target.sha256.trim();
    if !is_sha256_hex(expected) {
        return Err(UpdateError::MissingChecksum);
    }
    if let Some(size) = target.size {
        if size != bytes.len() as u64 {
            return Err(UpdateError::SizeMismatch {
                expected: size,
                actual: bytes.len() as u64,
            });
        }
    }
    let actual = sha256_hex(bytes);
    if !actual.eq_ignore_ascii_case(expected) {
        return Err(UpdateError::ChecksumMismatch {
            expected: expected.to_string(),
            actual,
        });
    }
    Ok(target)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest() -> UpdateManifest {
        let mut platforms = BTreeMap::new();
        platforms.insert(
            "darwin-aarch64".to_string(),
            UpdateTarget {
                url: "https://example.com/Ferriterm.app.tar.gz".into(),
                signature: "sig".into(),
                sha256: sha256_hex(b"release artifact"),
                size: Some(b"release artifact".len() as u64),
            },
        );
        UpdateManifest {
            version: "0.2.0".into(),
            notes: "notes".into(),
            pub_date: "2026-11-08T00:00:00Z".into(),
            platforms,
        }
    }

    #[test]
    fn parses_tauri_style_manifest() {
        let raw = serde_json::to_string(&manifest()).unwrap();
        let parsed = parse_manifest(&raw).unwrap();
        assert_eq!(parsed.version, "0.2.0");
        assert!(parsed.platforms.contains_key("darwin-aarch64"));
    }

    #[test]
    fn rejects_oversized_manifest() {
        let raw = "{}".repeat(MAX_MANIFEST_BYTES);
        assert_eq!(parse_manifest(&raw), Err(UpdateError::TooLarge));
    }

    #[test]
    fn rejects_manifest_without_version() {
        let mut value = serde_json::to_value(manifest()).unwrap();
        value["version"] = serde_json::Value::String(String::new());
        let err = parse_manifest(&value.to_string()).unwrap_err();
        assert!(matches!(err, UpdateError::InvalidManifest(_)));
    }

    #[test]
    fn accepts_matching_digest() {
        let manifest = manifest();
        let target = verify_download(&manifest, "darwin-aarch64", b"release artifact").unwrap();
        assert_eq!(target.sha256, sha256_hex(b"release artifact"));
    }

    #[test]
    fn rejects_tampered_artifact() {
        let manifest = manifest();
        let err = verify_download(&manifest, "darwin-aarch64", b"release artifacts").unwrap_err();
        assert!(matches!(err, UpdateError::SizeMismatch { .. }));
        let err = verify_download(&manifest, "darwin-aarch64", b"release artifacT").unwrap_err();
        assert!(matches!(err, UpdateError::ChecksumMismatch { .. }));
    }

    #[test]
    fn rejects_unknown_platform() {
        let manifest = manifest();
        assert_eq!(
            verify_download(&manifest, "windows-x86_64", b"release artifact"),
            Err(UpdateError::UnsupportedPlatform("windows-x86_64".into()))
        );
    }

    #[test]
    fn rejects_insecure_url() {
        let mut manifest = manifest();
        manifest.platforms.get_mut("darwin-aarch64").unwrap().url =
            "http://example.com/app.tar.gz".into();
        assert_eq!(
            verify_download(&manifest, "darwin-aarch64", b"release artifact"),
            Err(UpdateError::InsecureUrl)
        );
    }

    #[test]
    fn rejects_missing_or_malformed_checksum() {
        let mut manifest = manifest();
        {
            let target = manifest.platforms.get_mut("darwin-aarch64").unwrap();
            target.sha256 = String::new();
        }
        assert_eq!(
            verify_download(&manifest, "darwin-aarch64", b"release artifact"),
            Err(UpdateError::MissingChecksum)
        );
        {
            let target = manifest.platforms.get_mut("darwin-aarch64").unwrap();
            target.sha256 = "not-a-digest".into();
        }
        assert_eq!(
            verify_download(&manifest, "darwin-aarch64", b"release artifact"),
            Err(UpdateError::MissingChecksum)
        );
    }

    #[test]
    fn hashes_known_vector() {
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }
}
