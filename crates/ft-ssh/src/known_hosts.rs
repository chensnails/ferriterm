//! Trust-on-first-use host key storage.
//!
//! Keys accepted by the user are appended to a private known_hosts file inside
//! the application data directory. The user's OpenSSH `known_hosts` is also
//! consulted read-only so hosts trusted by the system do not prompt again.

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use data_encoding::BASE64_MIME;
use hmac::{Hmac, KeyInit, Mac};
use russh::keys::ssh_key::PublicKey;
use russh::keys::{check_known_hosts_path, Error as KeyError};
use sha1::Sha1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostKeyStatus {
    Trusted,
    Unknown,
    Mismatch,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HostKeyEntry {
    pub host: String,
    pub port: u16,
    pub algorithm: String,
    pub fingerprint: String,
    /// A hashed line hides the hostname; `host` then carries the hash token
    /// itself, which is what removal must be given.
    pub hashed: bool,
}

pub struct KnownHostsStore {
    path: PathBuf,
    system_path: Option<PathBuf>,
}

impl KnownHostsStore {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        let system_path = home_dir().map(|home| home.join(".ssh").join("known_hosts"));
        Self {
            path: dir.into().join("known_hosts"),
            system_path,
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Lists entries from the app-managed file only; the user's OpenSSH file
    /// stays read-only. Hashed lines are reported with the hash token as the
    /// host because the hostname is not recoverable from them.
    pub fn entries(&self) -> io::Result<Vec<HostKeyEntry>> {
        let raw = match fs::read_to_string(&self.path) {
            Ok(raw) => raw,
            Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(err) => return Err(err),
        };
        let mut entries = Vec::new();
        for line in raw.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }
            let mut parts = trimmed.split_whitespace();
            let Some(names) = parts.next() else { continue };
            let Some(key_type) = parts.next() else {
                continue;
            };
            let Some(key_data) = parts.next() else {
                continue;
            };
            let fingerprint = PublicKey::from_openssh(&format!("{key_type} {key_data}"))
                .map(|key| key.fingerprint(russh::keys::HashAlg::Sha256).to_string())
                .unwrap_or_default();
            for name in names.split(',') {
                let hashed = is_hashed(name);
                let (host, port) = if hashed {
                    (name.to_string(), 22)
                } else {
                    parse_host_field(name)
                };
                entries.push(HostKeyEntry {
                    host,
                    port,
                    algorithm: key_type.to_string(),
                    fingerprint: fingerprint.clone(),
                    hashed,
                });
            }
        }
        Ok(entries)
    }

    /// Drops every app-managed entry for `host`/`port`, including a hashed line
    /// that covers the same target. Passing the hash token reported by
    /// [`Self::entries`] removes that line without knowing its hostname.
    pub fn remove(&self, host: &str, port: u16) -> io::Result<()> {
        let host_field = if is_hashed(host) {
            host.to_string()
        } else {
            host_field(host, port)
        };
        let existing = match fs::read_to_string(&self.path) {
            Ok(raw) => raw,
            Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(()),
            Err(err) => return Err(err),
        };
        let mut kept = String::with_capacity(existing.len());
        for line in existing.lines() {
            if keeps_line(line, &host_field) {
                kept.push_str(line);
                kept.push('\n');
            }
        }
        write_private(&self.path, kept.as_bytes())
    }

    pub fn check(&self, host: &str, port: u16, key: &PublicKey) -> HostKeyStatus {
        let own = check_path(&self.path, host, port, key);
        if own != HostKeyStatus::Unknown {
            return own;
        }
        match &self.system_path {
            Some(system) => check_path(system, host, port, key),
            None => HostKeyStatus::Unknown,
        }
    }

    pub fn remember(&self, host: &str, port: u16, key: &PublicKey) -> io::Result<()> {
        let host_field = host_field(host, port);
        let openssh = key
            .to_openssh()
            .map_err(|e| io::Error::other(format!("serialize host key: {e}")))?;

        let existing = fs::read_to_string(&self.path).unwrap_or_default();
        let mut kept = String::with_capacity(existing.len() + openssh.len() + 32);
        for line in existing.lines() {
            if keeps_line(line, &host_field) {
                kept.push_str(line);
                kept.push('\n');
            }
        }
        kept.push_str(&host_field);
        kept.push(' ');
        kept.push_str(&openssh);
        kept.push('\n');

        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }
        write_private(&self.path, kept.as_bytes())
    }
}

fn check_path(path: &Path, host: &str, port: u16, key: &PublicKey) -> HostKeyStatus {
    match check_known_hosts_path(host, port, key, path) {
        Ok(true) => HostKeyStatus::Trusted,
        Ok(false) => HostKeyStatus::Unknown,
        Err(KeyError::KeyChanged { .. }) => HostKeyStatus::Mismatch,
        Err(_) => HostKeyStatus::Unknown,
    }
}

fn host_field(host: &str, port: u16) -> String {
    if port == 22 {
        host.to_string()
    } else {
        format!("[{host}]:{port}")
    }
}

fn parse_host_field(field: &str) -> (String, u16) {
    if let Some(rest) = field.strip_prefix('[') {
        if let Some((host, port)) = rest.split_once("]:") {
            return (host.to_string(), port.parse().unwrap_or(22));
        }
    }
    (field.to_string(), 22)
}

fn keeps_line(line: &str, host_field: &str) -> bool {
    let trimmed = line.trim();
    if trimmed.is_empty() || trimmed.starts_with('#') {
        return true;
    }
    match trimmed.split_whitespace().next() {
        Some(names) => !names.split(',').any(|name| name_matches(name, host_field)),
        None => true,
    }
}

fn is_hashed(name: &str) -> bool {
    name.starts_with("|1|") || name.starts_with("|2|")
}

/// A line is replaced only when it pins the same target. Hashed names hide the
/// hostname, so the comparison recomputes OpenSSH's HMAC-SHA1 (`|1|salt|hash`)
/// over the candidate; the token itself matches verbatim for removal by hash.
fn name_matches(name: &str, host_field: &str) -> bool {
    if name == host_field {
        return true;
    }
    name.starts_with("|1|") && hashed_name_matches(name, host_field)
}

fn hashed_name_matches(name: &str, host: &str) -> bool {
    let mut parts = name.split('|');
    let (Some(""), Some("1"), Some(salt), Some(expected)) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return false;
    };
    let Ok(salt) = BASE64_MIME.decode(salt.as_bytes()) else {
        return false;
    };
    let Ok(expected) = BASE64_MIME.decode(expected.as_bytes()) else {
        return false;
    };
    let Ok(mut mac) = Hmac::<Sha1>::new_from_slice(&salt) else {
        return false;
    };
    mac.update(host.as_bytes());
    mac.verify_slice(&expected).is_ok()
}

fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
}

fn write_private(path: &Path, data: &[u8]) -> io::Result<()> {
    // Atomic replace: a crash mid-write must never truncate the trust store.
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
    fs::rename(&tmp, path)?;
    #[cfg(unix)]
    if let Some(parent) = path.parent() {
        if let Ok(dir) = fs::File::open(parent) {
            let _ = dir.sync_all();
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    struct TempDir(PathBuf);

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn temp_store(name: &str) -> (TempDir, KnownHostsStore) {
        let dir = std::env::temp_dir().join(format!(
            "ferriterm-known-hosts-{name}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = fs::remove_dir_all(&dir);
        let store = KnownHostsStore {
            path: dir.join("known_hosts"),
            system_path: None,
        };
        (TempDir(dir), store)
    }

    fn key(seed: u64) -> PublicKey {
        let openssh = match seed {
            1 => "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIDvdTSWBad30om/6Qvu6qerBssLm1RHEeoULtFb3ZPC/",
            _ => "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAILOIWRygPppfNNnXY8f/Gt392k/RpOxrZH+BCOPz+fMv",
        };
        PublicKey::from_openssh(openssh).unwrap()
    }

    #[test]
    fn unknown_host_then_trusted_after_remember() {
        let (_guard, store) = temp_store("remember");
        let key = key(1);
        assert_eq!(store.check("example.com", 22, &key), HostKeyStatus::Unknown);

        store.remember("example.com", 22, &key).unwrap();
        assert_eq!(store.check("example.com", 22, &key), HostKeyStatus::Trusted);
    }

    #[test]
    fn non_standard_port_uses_brackets() {
        let (_guard, store) = temp_store("port");
        let key = key(1);
        store.remember("example.com", 2222, &key).unwrap();

        let raw = fs::read_to_string(store.path()).unwrap();
        assert!(raw.starts_with("[example.com]:2222 "));
        assert_eq!(
            store.check("example.com", 2222, &key),
            HostKeyStatus::Trusted
        );
        assert_eq!(store.check("example.com", 22, &key), HostKeyStatus::Unknown);
    }

    #[test]
    fn changed_key_is_detected_and_replaced() {
        let (_guard, store) = temp_store("mismatch");
        let first = key(1);
        let second = key(2);
        store.remember("example.com", 22, &first).unwrap();
        assert_eq!(
            store.check("example.com", 22, &second),
            HostKeyStatus::Mismatch
        );

        store.remember("example.com", 22, &second).unwrap();
        assert_eq!(
            store.check("example.com", 22, &second),
            HostKeyStatus::Trusted
        );
        assert_eq!(
            store.check("example.com", 22, &first),
            HostKeyStatus::Mismatch
        );
    }

    #[test]
    fn comments_and_other_hosts_are_preserved() {
        let (_guard, store) = temp_store("preserve");
        let key = key(1);
        fs::create_dir_all(store.path().parent().unwrap()).unwrap();
        fs::write(
            store.path(),
            "# managed\nexample.com ssh-ed25519 OLD_KEY_FOR_EXAMPLE\nother.test,alias.test ssh-ed25519 OLD_KEY_FOR_OTHER\n",
        )
        .unwrap();

        store.remember("example.com", 22, &key).unwrap();
        let raw = fs::read_to_string(store.path()).unwrap();
        assert!(raw.contains("# managed"));
        assert!(raw.contains("other.test,alias.test"));
        assert!(!raw.contains("OLD_KEY_FOR_EXAMPLE"));
    }

    const HASHED_LINE: &str = "|1|O33ESRMWPVkMYIwJ1Uw+n877jTo=|nuuC5vEqXlEZ/8BXQR7m619W6Ak= ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAILIG2T/B0l0gaqj3puu510tu9N1OkQ4znY3LYuEm5zCF";

    fn hashed_key() -> PublicKey {
        PublicKey::from_openssh(
            "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAILIG2T/B0l0gaqj3puu510tu9N1OkQ4znY3LYuEm5zCF",
        )
        .unwrap()
    }

    fn write_lines(store: &KnownHostsStore, lines: &str) {
        fs::create_dir_all(store.path().parent().unwrap()).unwrap();
        fs::write(store.path(), lines).unwrap();
    }

    #[test]
    fn hashed_entries_are_listed_checked_and_replaced() {
        let (_guard, store) = temp_store("hashed");
        write_lines(&store, &format!("{HASHED_LINE}\n"));

        let entries = store.entries().unwrap();
        assert_eq!(entries.len(), 1);
        assert!(entries[0].hashed);
        assert_eq!(entries[0].port, 22);
        assert!(entries[0].host.starts_with("|1|"));
        assert!(entries[0].fingerprint.starts_with("SHA256:"));

        // russh resolves the hash when checking; the entry is real trust.
        assert_eq!(
            store.check("example.com", 22, &hashed_key()),
            HostKeyStatus::Trusted
        );

        store.remember("example.com", 22, &key(2)).unwrap();
        let raw = fs::read_to_string(store.path()).unwrap();
        assert!(!raw.contains("|1|"), "{raw}");
        assert!(raw.starts_with("example.com "), "{raw}");
        assert_eq!(
            store.check("example.com", 22, &key(2)),
            HostKeyStatus::Trusted
        );
    }

    #[test]
    fn hashed_entries_can_be_removed_by_token() {
        let (_guard, store) = temp_store("hashed-remove");
        write_lines(
            &store,
            &format!("{HASHED_LINE}\nother.test ssh-ed25519 OLD\n"),
        );
        let token = store.entries().unwrap()[0].host.clone();

        store.remove(&token, 22).unwrap();
        let raw = fs::read_to_string(store.path()).unwrap();
        assert!(!raw.contains("|1|"), "{raw}");
        assert!(raw.contains("other.test"), "{raw}");
    }

    #[test]
    fn removing_another_host_keeps_the_hashed_line() {
        let (_guard, store) = temp_store("hashed-keep");
        write_lines(
            &store,
            &format!("{HASHED_LINE}\nother.test ssh-ed25519 OLD\n"),
        );

        store.remove("other.test", 22).unwrap();
        let raw = fs::read_to_string(store.path()).unwrap();
        assert!(raw.contains("|1|"), "{raw}");
        assert!(!raw.contains("other.test"), "{raw}");
    }

    #[test]
    fn entries_and_remove_target_one_host() {
        let (_guard, store) = temp_store("entries");
        store.remember("example.com", 22, &key(1)).unwrap();
        store.remember("example.com", 2222, &key(1)).unwrap();

        let entries = store.entries().unwrap();
        assert_eq!(entries.len(), 2);
        assert!(entries.iter().all(|entry| entry.algorithm == "ssh-ed25519"));
        assert!(entries
            .iter()
            .all(|entry| entry.fingerprint.starts_with("SHA256:")));
        assert!(entries
            .iter()
            .any(|entry| entry.host == "example.com" && entry.port == 2222));

        store.remove("example.com", 2222).unwrap();
        let entries = store.entries().unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].port, 22);

        store.remove("example.com", 22).unwrap();
        assert!(store.entries().unwrap().is_empty());
        assert_eq!(
            store.check("example.com", 22, &key(1)),
            HostKeyStatus::Unknown
        );
    }
}
