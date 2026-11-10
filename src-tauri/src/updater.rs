//! Self-update against the release `latest.json` manifest.
//!
//! The manifest is fetched over HTTPS, the artifact is verified with
//! `ft_security::update::verify_download` (SHA-256 + size) before anything is
//! handed to the platform installer, and macOS replaces the running bundle
//! in place while Windows/Linux hand off to their installers.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Emitter};

const MANIFEST_URL: &str =
    "https://github.com/chensnails/ferriterm/releases/latest/download/latest.json";
const MAX_ARTIFACT_BYTES: usize = 512 * 1024 * 1024;
const PROGRESS_STEP: usize = 512 * 1024;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    pub current_version: String,
    pub version: String,
    pub notes: String,
    pub available: bool,
    pub size: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct UpdateProgress {
    received: usize,
    total: Option<u64>,
}

pub fn platform_key() -> &'static str {
    if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
        "darwin-aarch64"
    } else if cfg!(all(target_os = "macos", target_arch = "x86_64")) {
        "darwin-x86_64"
    } else if cfg!(all(target_os = "linux", target_arch = "x86_64")) {
        "linux-x86_64"
    } else if cfg!(all(target_os = "windows", target_arch = "x86_64")) {
        "windows-x86_64"
    } else {
        "unsupported"
    }
}

fn version_parts(value: &str) -> Option<Vec<u64>> {
    value.split('.').map(|part| part.parse().ok()).collect()
}

fn version_newer(candidate: &str, current: &str) -> bool {
    let (Some(a), Some(b)) = (version_parts(candidate), version_parts(current)) else {
        return false;
    };
    for index in 0..a.len().max(b.len()) {
        let left = a.get(index).copied().unwrap_or(0);
        let right = b.get(index).copied().unwrap_or(0);
        if left != right {
            return left > right;
        }
    }
    false
}

async fn fetch_manifest() -> Result<ft_security::update::UpdateManifest, String> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|e| e.to_string())?;
    let response = client
        .get(MANIFEST_URL)
        .send()
        .await
        .map_err(|e| format!("update check failed: {e}"))?
        .error_for_status()
        .map_err(|e| format!("update check failed: {e}"))?;
    let bytes = response
        .bytes()
        .await
        .map_err(|e| format!("update manifest download failed: {e}"))?;
    ft_security::update::parse_manifest(&String::from_utf8_lossy(&bytes)).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn update_check(app: AppHandle) -> Result<UpdateInfo, String> {
    let manifest = fetch_manifest().await?;
    let current = app.package_info().version.to_string();
    let target = manifest
        .platforms
        .get(platform_key())
        .ok_or_else(|| format!("no update published for platform {}", platform_key()))?;
    Ok(UpdateInfo {
        current_version: current.clone(),
        version: manifest.version.clone(),
        notes: manifest.notes.clone(),
        available: version_newer(&manifest.version, &current),
        size: target.size,
    })
}

#[tauri::command]
pub async fn update_install(app: AppHandle) -> Result<String, String> {
    let manifest = fetch_manifest().await?;
    let current = app.package_info().version.to_string();
    if !version_newer(&manifest.version, &current) {
        return Err("no newer version is published".to_string());
    }
    let platform = platform_key();
    let target = manifest
        .platforms
        .get(platform)
        .ok_or_else(|| format!("no update published for platform {platform}"))?
        .clone();
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(300))
        .build()
        .map_err(|e| e.to_string())?;
    let mut response = client
        .get(&target.url)
        .send()
        .await
        .map_err(|e| format!("update download failed: {e}"))?
        .error_for_status()
        .map_err(|e| format!("update download failed: {e}"))?;
    let total = target.size.or(response.content_length());
    let mut bytes = Vec::new();
    let mut last_emit = 0usize;
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|e| format!("update download failed: {e}"))?
    {
        bytes.extend_from_slice(&chunk);
        if bytes.len() > MAX_ARTIFACT_BYTES {
            return Err("update artifact exceeds the 512 MiB cap".to_string());
        }
        if bytes.len() - last_emit >= PROGRESS_STEP {
            last_emit = bytes.len();
            let _ = app.emit(
                "update://progress",
                UpdateProgress {
                    received: bytes.len(),
                    total,
                },
            );
        }
    }
    let _ = app.emit(
        "update://progress",
        UpdateProgress {
            received: bytes.len(),
            total,
        },
    );
    ft_security::update::verify_download(&manifest, platform, &bytes).map_err(|e| e.to_string())?;

    let dir = std::env::temp_dir().join(format!("ferriterm-update-{}", manifest.version));
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let name = target
        .url
        .rsplit('/')
        .next()
        .filter(|name| !name.is_empty())
        .ok_or_else(|| "update target url has no file name".to_string())?;
    let artifact = dir.join(name);
    fs::write(&artifact, &bytes).map_err(|e| e.to_string())?;
    install(&app, &artifact, &dir)
}

#[cfg(target_os = "macos")]
fn bundle_path() -> Result<PathBuf, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let bundle = exe
        .parent()
        .and_then(|path| path.parent())
        .and_then(|path| path.parent())
        .ok_or_else(|| "cannot locate the running app bundle".to_string())?;
    if bundle.extension().and_then(|ext| ext.to_str()) != Some("app") {
        return Err("updates install into an .app bundle; run the installed app".to_string());
    }
    Ok(bundle.to_path_buf())
}

#[cfg(unix)]
fn extract_archive(artifact: &Path, dir: &Path, suffix: &str) -> Result<PathBuf, String> {
    let extracted = dir.join("extracted");
    fs::create_dir_all(&extracted).map_err(|e| e.to_string())?;
    let status = std::process::Command::new("tar")
        .arg("-xzf")
        .arg(artifact)
        .arg("-C")
        .arg(&extracted)
        .status()
        .map_err(|e| format!("failed to run tar: {e}"))?;
    if !status.success() {
        return Err("failed to extract the update archive".to_string());
    }
    fs::read_dir(&extracted)
        .map_err(|e| e.to_string())?
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .find(|path| {
            path.extension()
                .and_then(|ext| ext.to_str())
                .map(|ext| ext == suffix)
                .unwrap_or(false)
        })
        .ok_or_else(|| format!("update archive contains no .{suffix}"))
}

#[cfg(target_os = "macos")]
fn install(app: &AppHandle, artifact: &Path, dir: &Path) -> Result<String, String> {
    let new_app = extract_archive(artifact, dir, "app")?;
    let bundle = bundle_path()?;
    let status = std::process::Command::new("ditto")
        .arg(new_app)
        .arg(&bundle)
        .status()
        .map_err(|e| format!("failed to run ditto: {e}"))?;
    if !status.success() {
        return Err("failed to replace the app bundle".to_string());
    }
    app.restart();
}

#[cfg(target_os = "linux")]
fn install(app: &AppHandle, artifact: &Path, dir: &Path) -> Result<String, String> {
    let new_image = extract_archive(artifact, dir, "AppImage")?;
    if let Some(current) = std::env::var_os("APPIMAGE").map(PathBuf::from) {
        if fs::copy(&new_image, &current).is_ok() {
            use std::os::unix::fs::PermissionsExt;
            let _ = fs::set_permissions(&current, fs::Permissions::from_mode(0o755));
            app.restart();
        }
    }
    let home = std::env::var_os("HOME").ok_or_else(|| "HOME is not set".to_string())?;
    let dest = PathBuf::from(home).join(".local/bin/ferriterm.AppImage");
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    fs::copy(&new_image, &dest).map_err(|e| e.to_string())?;
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(&dest, fs::Permissions::from_mode(0o755)).map_err(|e| e.to_string())?;
    Ok(format!(
        "installed to {}; relaunch Ferriterm from there",
        dest.display()
    ))
}

#[cfg(target_os = "windows")]
fn install(app: &AppHandle, artifact: &Path, dir: &Path) -> Result<String, String> {
    let extracted = dir.join("extracted");
    fs::create_dir_all(&extracted).map_err(|e| e.to_string())?;
    let script = format!(
        "Expand-Archive -LiteralPath '{}' -DestinationPath '{}' -Force",
        artifact.display().to_string().replace('\'', "''"),
        extracted.display().to_string().replace('\'', "''"),
    );
    let status = std::process::Command::new("powershell")
        .args(["-NoProfile", "-Command", &script])
        .status()
        .map_err(|e| format!("failed to expand the update archive: {e}"))?;
    if !status.success() {
        return Err("failed to expand the update archive".to_string());
    }
    let installer = fs::read_dir(&extracted)
        .map_err(|e| e.to_string())?
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .find(|path| {
            path.extension()
                .and_then(|ext| ext.to_str())
                .map(|ext| ext.eq_ignore_ascii_case("exe"))
                .unwrap_or(false)
        })
        .ok_or_else(|| "update archive contains no installer".to_string())?;
    // The wizard needs the running bundle unlocked, so hand off and exit.
    std::process::Command::new(installer)
        .spawn()
        .map_err(|e| format!("failed to launch the installer: {e}"))?;
    app.exit(0);
    Ok("installer launched".to_string())
}

#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
fn install(_app: &AppHandle, _artifact: &Path, _dir: &Path) -> Result<String, String> {
    Err("self-update is not supported on this platform".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_comparison_is_numeric() {
        assert!(version_newer("0.2.0", "0.1.0"));
        assert!(version_newer("0.10.0", "0.9.9"));
        assert!(version_newer("1.0.0", "0.99.99"));
        assert!(!version_newer("0.1.0", "0.1.0"));
        assert!(!version_newer("0.1.0", "0.2.0"));
        assert!(!version_newer("0.2", "0.2.0"));
        assert!(!version_newer("nightly", "0.1.0"));
    }

    #[test]
    fn platform_key_matches_the_build_target() {
        let expected = if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
            "darwin-aarch64"
        } else if cfg!(target_os = "macos") {
            "darwin-x86_64"
        } else if cfg!(target_os = "windows") {
            "windows-x86_64"
        } else if cfg!(target_os = "linux") {
            "linux-x86_64"
        } else {
            "unsupported"
        };
        assert_eq!(platform_key(), expected);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn bundle_path_rejects_non_bundle_executables() {
        // The test harness runs from target/debug/deps, not an .app bundle.
        assert!(bundle_path().is_err());
    }

    #[cfg(unix)]
    #[test]
    fn extract_archive_finds_the_app() {
        let dir =
            std::env::temp_dir().join(format!("ferriterm-update-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let src = dir.join("src");
        fs::create_dir_all(src.join("Fake.app/Contents/MacOS")).unwrap();
        fs::write(src.join("Fake.app/Contents/MacOS/fake"), b"bin").unwrap();
        let artifact = dir.join("update.tar.gz");
        let status = std::process::Command::new("tar")
            .arg("-czf")
            .arg(&artifact)
            .arg("-C")
            .arg(&src)
            .arg("Fake.app")
            .status()
            .unwrap();
        assert!(status.success());
        let found = extract_archive(&artifact, &dir, "app").unwrap();
        assert!(found.ends_with("Fake.app"));
        let _ = fs::remove_dir_all(&dir);
    }
}
