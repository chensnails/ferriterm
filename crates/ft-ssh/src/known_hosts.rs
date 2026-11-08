//! Trust-on-first-use host key storage.
//!
//! Keys accepted by the user are appended to a private known_hosts file inside
//! the application data directory. The user's OpenSSH `known_hosts` is also
//! consulted read-only so hosts trusted by the system do not prompt again.

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use russh::keys::ssh_key::PublicKey;
use russh::keys::{check_known_hosts_path, Error as KeyError};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostKeyStatus {
    Trusted,
    Unknown,
    Mismatch,
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
            if keeps_line(line, &host_field, host) {
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

fn keeps_line(line: &str, host_field: &str, host: &str) -> bool {
    let trimmed = line.trim();
    if trimmed.is_empty() || trimmed.starts_with('#') {
        return true;
    }
    // Hashed entries cannot be matched against a plain host name; keep them.
    if trimmed.starts_with("|1|") || trimmed.starts_with("|2|") {
        return true;
    }
    match trimmed.split_whitespace().next() {
        Some(names) => !names
            .split(',')
            .any(|name| name == host_field || name == host),
        None => true,
    }
}

fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
}

fn write_private(path: &Path, data: &[u8]) -> io::Result<()> {
    let mut opts = fs::OpenOptions::new();
    opts.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    let mut file = opts.open(path)?;
    file.write_all(data)?;
    file.sync_all()
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
}
