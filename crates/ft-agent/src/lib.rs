//! Ferriterm's local control service.
//!
//! The app listens on loopback for agents and for `ferriterm-cli`, behind a
//! token that lives in a file only this user can read. The port is chosen by
//! the kernel on each launch, so nothing can be aimed at it from a distance,
//! and the token is regenerated then too, so a stale one is worthless.
//!
//! Three pieces, each usable on its own:
//! - [`protocol`] — the JSON shapes on the wire,
//! - [`server`] — the HTTP surface plus the [`server::Backend`] trait the app
//!   implements; the gate itself is *not* here, it is consulted by the backend
//!   so every entry point shares one policy and one audit trail,
//! - [`client`] — what the CLI and scripts use to talk to it.

pub mod client;
pub mod protocol;
pub mod server;

mod base64;

use std::path::{Path, PathBuf};

use ring::rand::{SecureRandom, SystemRandom};
use serde::{Deserialize, Serialize};

/// Keep in step with `identifier` in `src-tauri/tauri.conf.json`: the CLI has
/// to find the settings directory without the app telling it where that is.
pub const APP_IDENTIFIER: &str = "app.ferriterm.desktop";

/// Name of the file the app writes and clients read.
pub const ENDPOINT_FILE: &str = "agent.json";

/// Environment variable that points the CLI at a different endpoint file, for
/// a second profile or a test.
pub const ENDPOINT_ENV: &str = "FERRITERM_AGENT";

/// Where a client should look: the token, the port, and which process owns it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Endpoint {
    /// Application version that wrote the file, for a readable error when the
    /// CLI and the app are out of step.
    pub version: String,
    pub port: u16,
    pub token: String,
    /// Process id, so a client can tell whether the file is stale.
    pub pid: u32,
}

impl Endpoint {
    /// Absolute path of the endpoint file inside an application data directory.
    pub fn path_in(dir: &Path) -> PathBuf {
        dir.join(ENDPOINT_FILE)
    }

    /// Write the file for this user only. On Unix that means mode 0600, set
    /// before the content lands so the token is never briefly world-readable.
    pub fn save(&self, dir: &Path) -> Result<PathBuf, String> {
        let path = Self::path_in(dir);
        let encoded = serde_json::to_string_pretty(self).map_err(|err| err.to_string())?;

        #[cfg(unix)]
        {
            use std::io::Write;
            use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .mode(0o600)
                .open(&path)
                .map_err(|err| format!("cannot write {}: {err}", path.display()))?;
            file.write_all(encoded.as_bytes())
                .map_err(|err| format!("cannot write {}: {err}", path.display()))?;
            let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
        }
        #[cfg(not(unix))]
        {
            std::fs::write(&path, encoded)
                .map_err(|err| format!("cannot write {}: {err}", path.display()))?;
        }
        Ok(path)
    }

    pub fn load(path: &Path) -> Result<Endpoint, String> {
        let raw = std::fs::read_to_string(path)
            .map_err(|err| format!("no agent endpoint at {}: {err}", path.display()))?;
        serde_json::from_str(&raw)
            .map_err(|err| format!("{} is not a valid endpoint: {err}", path.display()))
    }
}

/// Directory Tauri uses for this app's data, resolved the way it does.
pub fn app_data_dir() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("FERRITERM_DATA_DIR") {
        return Some(PathBuf::from(dir));
    }
    #[cfg(windows)]
    {
        std::env::var_os("APPDATA").map(|base| PathBuf::from(base).join(APP_IDENTIFIER))
    }
    #[cfg(target_os = "macos")]
    {
        std::env::var_os("HOME").map(|home| {
            PathBuf::from(home)
                .join("Library/Application Support")
                .join(APP_IDENTIFIER)
        })
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let base = std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .or_else(|| {
                std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/share"))
            })?;
        Some(base.join(APP_IDENTIFIER))
    }
}

/// Endpoint file a client should read, honouring [`ENDPOINT_ENV`] first.
pub fn discover_endpoint() -> Result<(PathBuf, Endpoint), String> {
    if let Some(path) = std::env::var_os(ENDPOINT_ENV) {
        let path = PathBuf::from(path);
        let endpoint = Endpoint::load(&path)?;
        return Ok((path, endpoint));
    }
    let dir =
        app_data_dir().ok_or_else(|| "cannot locate the application data directory".to_string())?;
    let path = Endpoint::path_in(&dir);
    let endpoint = Endpoint::load(&path)?;
    Ok((path, endpoint))
}

/// Encode bytes for a file transfer, exposed so the CLI can build one without
/// depending on this module's layout.
pub fn encode_base64(bytes: &[u8]) -> String {
    base64::encode(bytes)
}

pub fn decode_base64(text: &str) -> Result<Vec<u8>, String> {
    base64::decode(text)
}

/// 32 random bytes as hex. This is the only thing standing between a local
/// process and the machine's SSH connections, so it comes from the OS.
pub fn random_token() -> String {
    let mut bytes = [0u8; 32];
    let rng = SystemRandom::new();
    // A failure here means the OS entropy source is broken, and continuing
    // with a predictable token would be worse than stopping.
    rng.fill(&mut bytes)
        .expect("the operating system random source failed");
    let mut out = String::with_capacity(64);
    for byte in bytes {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokens_are_64_hex_characters_and_unique() {
        let first = random_token();
        let second = random_token();
        assert_eq!(first.len(), 64);
        assert!(first.bytes().all(|byte| byte.is_ascii_hexdigit()));
        assert_ne!(first, second);
    }

    #[test]
    fn an_endpoint_round_trips_through_a_directory() {
        let dir = std::env::temp_dir().join(format!("ft-agent-endpoint-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let endpoint = Endpoint {
            version: "1.0.0".to_string(),
            port: 41234,
            token: "abc".to_string(),
            pid: 99,
        };
        let path = endpoint.save(&dir).unwrap();
        assert_eq!(path, Endpoint::path_in(&dir));
        let loaded = Endpoint::load(&path).unwrap();
        assert_eq!(loaded.port, 41234);
        assert_eq!(loaded.token, "abc");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_missing_endpoint_file_says_where_it_looked() {
        let path = std::env::temp_dir().join("ft-agent-no-such-endpoint.json");
        let error = Endpoint::load(&path).unwrap_err();
        assert!(error.contains("ft-agent-no-such-endpoint.json"), "{error}");
    }
}
