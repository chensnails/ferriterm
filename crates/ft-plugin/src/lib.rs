//! Data-only plugin host. Each package is a JSON manifest (either a single
//! file or `manifest.json` inside a directory) that contributes a language pack,
//! a theme, a command pack, or a sandboxed WASM entry point.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

pub mod wasm;

pub use wasm::{run as run_wasm, WasmLimits, WasmRun};

pub const API_VERSION: u32 = 1;
const MAX_STRING_LEN: usize = 8 * 1024;
const PERMISSIONS: &[&str] = &["network", "filesystem", "clipboard", "serial", "telnet"];
const KINDS: &[&str] = &[
    "language",
    "theme",
    "command-pack",
    "wasm",
    "serial",
    "telnet",
];
const MAX_MARKET_ENTRIES: usize = 200;

/// Index of the community marketplace. It is a plain JSON file served from
/// the repository's default branch, so publishing a plugin needs no server.
pub const DEFAULT_MARKET_URL: &str =
    "https://raw.githubusercontent.com/chensnails/ferriterm-plugins/main/index.json";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LanguagePack {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub locale: String,
    #[serde(default)]
    pub strings: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThemePack {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub kind: String,
    /// Base theme the token overrides apply on top of: `dark` or `light`.
    #[serde(default)]
    pub base: String,
    pub tokens: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandPackCommand {
    pub name: String,
    pub template: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandPackGroup {
    pub name: String,
    #[serde(default)]
    pub commands: Vec<CommandPackCommand>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandPack {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub groups: Vec<CommandPackGroup>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PluginManifest {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub version: String,
    pub kind: String,
    #[serde(default = "default_api_version")]
    pub api_version: u32,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub permissions: Vec<String>,
    #[serde(default)]
    pub entry: String,
}

fn default_api_version() -> u32 {
    API_VERSION
}

/// A plugin whose manifest ships inside the app, so an optional transport can
/// be enabled on a fresh install without the marketplace being reachable.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BuiltinPlugin {
    pub id: String,
    pub name: String,
    pub version: String,
    pub kind: String,
    pub description: String,
    pub permission: String,
}

/// The built-in list is the single source of truth for what can be installed
/// offline; the UI only renders it.
pub fn builtin_plugins() -> Vec<BuiltinPlugin> {
    [
        (
            "serial",
            "Serial Ports",
            "serial",
            "serial",
            "Unlocks serial-port sessions. The transport itself is built into the app; this manifest grants the serial permission.",
        ),
        (
            "telnet",
            "Telnet",
            "telnet",
            "telnet",
            "Unlocks Telnet sessions. The transport itself is built into the app; this manifest grants the telnet permission.",
        ),
    ]
    .into_iter()
    .map(|(id, name, kind, permission, description)| BuiltinPlugin {
        id: id.to_string(),
        name: name.to_string(),
        version: "1.0.0".to_string(),
        kind: kind.to_string(),
        description: description.to_string(),
        permission: permission.to_string(),
    })
    .collect()
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginScan {
    pub plugins: Vec<PluginManifest>,
    pub packs: Vec<LanguagePack>,
    pub themes: Vec<ThemePack>,
    pub command_packs: Vec<CommandPack>,
    pub errors: Vec<String>,
    #[serde(default)]
    pub builtin: Vec<BuiltinPlugin>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MarketEntry {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub version: String,
    pub kind: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub homepage: String,
    pub manifest_url: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MarketIndex {
    #[serde(default = "default_api_version")]
    pub api_version: u32,
    #[serde(default)]
    pub updated: String,
    #[serde(default)]
    pub plugins: Vec<MarketEntry>,
}

/// Parses and validates a marketplace index. Everything that reaches the
/// install path is checked here first: ids, kinds, and https manifest URLs.
pub fn parse_market_index(raw: &str) -> Result<MarketIndex, String> {
    let index: MarketIndex =
        serde_json::from_str(raw).map_err(|e| format!("invalid marketplace index: {e}"))?;
    if index.api_version != API_VERSION {
        return Err(format!(
            "unsupported marketplace apiVersion {} (expected {API_VERSION})",
            index.api_version
        ));
    }
    if index.plugins.len() > MAX_MARKET_ENTRIES {
        return Err("marketplace index has too many entries".to_string());
    }
    let mut ids = BTreeSet::new();
    for entry in &index.plugins {
        validate_id(&entry.id)?;
        if !ids.insert(entry.id.as_str()) {
            return Err(format!("duplicate marketplace id {}", entry.id));
        }
        if !KINDS.contains(&entry.kind.as_str()) {
            return Err(format!("unsupported plugin kind {}", entry.kind));
        }
        if entry.name.trim().is_empty() || entry.name.len() > 128 {
            return Err(format!("marketplace entry {} needs a name", entry.id));
        }
        if !entry.description.is_empty() && entry.description.len() > MAX_STRING_LEN {
            return Err(format!(
                "marketplace entry {} has a long description",
                entry.id
            ));
        }
        validate_https_url(&entry.manifest_url)?;
        if !entry.homepage.is_empty() {
            validate_https_url(&entry.homepage)?;
        }
    }
    Ok(index)
}

fn validate_https_url(url: &str) -> Result<(), String> {
    if url.len() > 2048 {
        return Err("URL is too long".to_string());
    }
    let rest = url
        .strip_prefix("https://")
        .ok_or_else(|| format!("URL must use https: {url}"))?;
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    let host_port = authority
        .rsplit_once('@')
        .map_or(authority, |(_, rest)| rest);
    let host = match host_port.rsplit_once(':') {
        Some((host, port)) if port.chars().all(|ch| ch.is_ascii_digit()) => host,
        _ => host_port,
    };
    let host_lower = host.to_ascii_lowercase();
    let looks_public = !host.is_empty()
        && host
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '-'))
        && !matches!(host_lower.as_str(), "localhost" | "127.0.0.1" | "0.0.0.0")
        && !host_lower.starts_with("127.")
        && !host_lower.ends_with(".local")
        && !host_lower.ends_with(".localhost");
    if looks_public {
        Ok(())
    } else {
        Err(format!("URL must point at a public host: {url}"))
    }
}

pub struct PluginRegistry {
    dir: PathBuf,
}

impl PluginRegistry {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    pub fn scan(&self) -> PluginScan {
        let mut scan = PluginScan {
            builtin: builtin_plugins(),
            ..PluginScan::default()
        };
        let mut ids = BTreeSet::new();
        let mut manifests = Vec::new();
        let entries = match fs::read_dir(&self.dir) {
            Ok(entries) => entries,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => return scan,
            Err(err) => {
                scan.errors.push(format!("{}: {err}", self.dir.display()));
                return scan;
            }
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let manifest_path = if path.is_dir() {
                path.join("manifest.json")
            } else if path.extension().and_then(|ext| ext.to_str()) == Some("json") {
                path.clone()
            } else {
                continue;
            };
            if !manifest_path.is_file() {
                if path.is_dir() {
                    scan.errors
                        .push(format!("{}: missing manifest.json", path.display()));
                }
                continue;
            }
            match load_plugin(&manifest_path) {
                Ok(plugin) => manifests.push((manifest_path, plugin)),
                Err(err) => scan
                    .errors
                    .push(format!("{}: {err}", manifest_path.display())),
            }
        }
        manifests.sort_by(|a, b| a.1.manifest.id.cmp(&b.1.manifest.id));
        for (path, plugin) in manifests {
            if !ids.insert(plugin.manifest.id.clone()) {
                scan.errors.push(format!(
                    "{}: duplicate plugin id {}",
                    path.display(),
                    plugin.manifest.id
                ));
                continue;
            }
            scan.plugins.push(plugin.manifest.clone());
            match plugin.body {
                PluginBody::Language(pack) => scan.packs.push(pack),
                PluginBody::Theme(pack) => scan.themes.push(pack),
                PluginBody::Commands(pack) => scan.command_packs.push(pack),
                PluginBody::Wasm | PluginBody::Serial | PluginBody::Telnet => {}
            }
        }
        scan
    }

    pub fn load_wasm(&self, id: &str) -> Result<(PluginManifest, Vec<u8>), String> {
        let entries = fs::read_dir(&self.dir).map_err(|err| err.to_string())?;
        for entry in entries.flatten() {
            let path = entry.path();
            let manifest_path = if path.is_dir() {
                path.join("manifest.json")
            } else if path.extension().and_then(|ext| ext.to_str()) == Some("json") {
                path.clone()
            } else {
                continue;
            };
            let Ok(loaded) = load_plugin(&manifest_path) else {
                continue;
            };
            if loaded.manifest.id != id || loaded.manifest.kind != "wasm" {
                continue;
            }
            let plugin_dir = manifest_path.parent().unwrap_or(&self.dir);
            let entry = safe_entry(plugin_dir, &loaded.manifest.entry)?;
            let bytes = fs::read(&entry).map_err(|err| format!("{}: {err}", entry.display()))?;
            return Ok((loaded.manifest, bytes));
        }
        Err(format!("plugin {id} not found"))
    }

    /// Validates a manifest fetched from the marketplace and writes it into the
    /// plugin directory as `<id>.json`. The temp file + rename keeps a partial
    /// download from ever looking installed.
    pub fn install(&self, expected_id: &str, raw: &str) -> Result<PluginManifest, String> {
        validate_id(expected_id)?;
        if raw.len() > 256 * 1024 {
            return Err("plugin manifest is too large".to_string());
        }
        fs::create_dir_all(&self.dir).map_err(|e| e.to_string())?;
        let temp = self.dir.join(format!(".{expected_id}.installing"));
        fs::write(&temp, raw).map_err(|e| e.to_string())?;
        let loaded = match load_plugin(&temp) {
            Ok(loaded) => loaded,
            Err(err) => {
                let _ = fs::remove_file(&temp);
                return Err(err);
            }
        };
        if loaded.manifest.id != expected_id {
            let _ = fs::remove_file(&temp);
            return Err(format!(
                "manifest id {} does not match the marketplace entry {expected_id}",
                loaded.manifest.id
            ));
        }
        let target = self.dir.join(format!("{expected_id}.json"));
        if target.is_dir() {
            let _ = fs::remove_file(&temp);
            return Err(format!(
                "{} is a directory; remove it from the plugin folder first",
                target.display()
            ));
        }
        fs::rename(&temp, &target).map_err(|err| {
            let _ = fs::remove_file(&temp);
            err.to_string()
        })?;
        Ok(loaded.manifest)
    }

    /// Writes the manifest for a plugin that ships with the app. The bytes go
    /// through the same `install` validation as a downloaded manifest.
    pub fn install_builtin(&self, id: &str) -> Result<PluginManifest, String> {
        let builtin = builtin_plugins()
            .into_iter()
            .find(|plugin| plugin.id == id)
            .ok_or_else(|| format!("unknown built-in plugin {id}"))?;
        let raw = serde_json::json!({
            "id": builtin.id,
            "name": builtin.name,
            "version": builtin.version,
            "kind": builtin.kind,
            "apiVersion": API_VERSION,
            "description": builtin.description,
            "permissions": [builtin.permission],
        })
        .to_string();
        self.install(id, &raw)
    }

    /// Removes the manifest (file or directory package) whose id matches.
    pub fn remove(&self, id: &str) -> Result<bool, String> {
        validate_id(id)?;
        let entries = match fs::read_dir(&self.dir) {
            Ok(entries) => entries,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(false),
            Err(err) => return Err(err.to_string()),
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let manifest_path = if path.is_dir() {
                path.join("manifest.json")
            } else if path.extension().and_then(|ext| ext.to_str()) == Some("json") {
                path.clone()
            } else {
                continue;
            };
            let Ok(loaded) = load_plugin(&manifest_path) else {
                continue;
            };
            if loaded.manifest.id != id {
                continue;
            }
            if path.is_dir() {
                fs::remove_dir_all(&path).map_err(|e| e.to_string())?;
            } else {
                fs::remove_file(&path).map_err(|e| e.to_string())?;
            }
            return Ok(true);
        }
        Ok(false)
    }
}

struct LoadedPlugin {
    manifest: PluginManifest,
    body: PluginBody,
}

enum PluginBody {
    Language(LanguagePack),
    Theme(ThemePack),
    Commands(CommandPack),
    Wasm,
    Serial,
    Telnet,
}

fn load_plugin(path: &Path) -> Result<LoadedPlugin, String> {
    let raw = fs::read_to_string(path).map_err(|e| e.to_string())?;
    let value: serde_json::Value =
        serde_json::from_str(&raw).map_err(|e| format!("invalid JSON: {e}"))?;
    let kind = value
        .get("kind")
        .and_then(|kind| kind.as_str())
        .ok_or("manifest needs a kind")?;
    let entry = value
        .get("entry")
        .and_then(|v| v.as_str())
        .unwrap_or("plugin.wasm")
        .to_string();
    let body = match kind {
        "language" => {
            let pack: LanguagePack = serde_json::from_value(value.clone())
                .map_err(|e| format!("invalid language pack: {e}"))?;
            if pack.locale.is_empty() || pack.strings.is_empty() {
                return Err("language pack needs locale and strings".to_string());
            }
            validate_strings(&pack.strings)?;
            PluginBody::Language(pack)
        }
        "theme" => {
            let pack: ThemePack = serde_json::from_value(value.clone())
                .map_err(|e| format!("invalid theme pack: {e}"))?;
            if !matches!(pack.base.as_str(), "dark" | "light") {
                return Err("theme base must be dark or light".to_string());
            }
            if pack.tokens.is_empty() {
                return Err("theme pack needs tokens".to_string());
            }
            for (token, token_value) in &pack.tokens {
                if !token.starts_with("--")
                    || token_value.trim().is_empty()
                    || token_value.len() > 128
                {
                    return Err(format!("invalid theme token {token}"));
                }
            }
            PluginBody::Theme(pack)
        }
        "command-pack" => {
            let pack: CommandPack = serde_json::from_value(value.clone())
                .map_err(|e| format!("invalid command pack: {e}"))?;
            if pack.groups.is_empty() {
                return Err("command pack needs groups".to_string());
            }
            for group in &pack.groups {
                if group.name.trim().is_empty() {
                    return Err("command pack group needs a name".to_string());
                }
                for command in &group.commands {
                    if command.name.trim().is_empty() || command.template.trim().is_empty() {
                        return Err("command pack commands need name and template".to_string());
                    }
                    if command.template.len() > MAX_STRING_LEN {
                        return Err("command template is too long".to_string());
                    }
                }
            }
            PluginBody::Commands(pack)
        }
        "wasm" => PluginBody::Wasm,
        // Serial is a built-in transport unlocked by this manifest; the plugin
        // itself contributes no data, only the permission.
        "serial" => {
            if !value
                .get("permissions")
                .and_then(|v| v.as_array())
                .is_some_and(|list| list.iter().any(|item| item.as_str() == Some("serial")))
            {
                return Err("serial plugin needs the serial permission".to_string());
            }
            PluginBody::Serial
        }
        // Telnet works like serial: the transport is built in and the manifest
        // only carries the permission that unlocks it.
        "telnet" => {
            if !value
                .get("permissions")
                .and_then(|v| v.as_array())
                .is_some_and(|list| list.iter().any(|item| item.as_str() == Some("telnet")))
            {
                return Err("telnet plugin needs the telnet permission".to_string());
            }
            PluginBody::Telnet
        }
        other => return Err(format!("unsupported plugin kind {other}")),
    };
    let manifest = manifest_from(&value, entry);
    validate_id(&manifest.id)?;
    validate_manifest(&manifest)?;
    Ok(LoadedPlugin { manifest, body })
}

fn manifest_from(value: &serde_json::Value, entry: String) -> PluginManifest {
    PluginManifest {
        id: value
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string(),
        name: value
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string(),
        version: value
            .get("version")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string(),
        kind: value
            .get("kind")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string(),
        api_version: value
            .get("apiVersion")
            .and_then(|v| v.as_u64())
            .unwrap_or(API_VERSION as u64) as u32,
        description: value
            .get("description")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string(),
        permissions: value
            .get("permissions")
            .and_then(|v| v.as_array())
            .map(|list| {
                list.iter()
                    .filter_map(|item| item.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default(),
        entry,
    }
}

fn validate_manifest(manifest: &PluginManifest) -> Result<(), String> {
    if manifest.name.trim().is_empty() {
        return Err("plugin needs a name".to_string());
    }
    if manifest.api_version != API_VERSION {
        return Err(format!(
            "unsupported apiVersion {} (expected {API_VERSION})",
            manifest.api_version
        ));
    }
    for permission in &manifest.permissions {
        if !PERMISSIONS.contains(&permission.as_str()) {
            return Err(format!("unknown permission {permission}"));
        }
    }
    Ok(())
}

fn validate_id(id: &str) -> Result<(), String> {
    let valid = (1..=64).contains(&id.len())
        && id
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '-'))
        && id
            .chars()
            .next()
            .is_some_and(|ch| ch.is_ascii_alphanumeric());
    if valid {
        Ok(())
    } else {
        Err(format!("invalid plugin id {id:?}"))
    }
}

fn validate_strings(strings: &BTreeMap<String, String>) -> Result<(), String> {
    for (key, value) in strings {
        if key.is_empty() || value.is_empty() {
            return Err("language pack has empty keys or values".to_string());
        }
        if value.len() > MAX_STRING_LEN {
            return Err(format!("language string {key} is too long"));
        }
    }
    Ok(())
}

fn safe_entry(plugin_dir: &Path, entry: &str) -> Result<PathBuf, String> {
    if entry.is_empty() || entry.contains("..") || Path::new(entry).is_absolute() {
        return Err("wasm entry must be a relative path inside the plugin directory".to_string());
    }
    let path = plugin_dir.join(entry);
    if !path.is_file() {
        return Err(format!("wasm entry {} does not exist", path.display()));
    }
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(label: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "ferriterm-plugin-{label}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn scans_language_themes_and_command_packs() {
        let dir = temp_dir("kinds");
        fs::write(
            dir.join("fr.json"),
            r#"{"id":"lang-fr","name":"Français","version":"0.1.0","kind":"language","locale":"fr","strings":{"app.name":"Ferriterm"}}"#,
        )
        .unwrap();
        fs::write(
            dir.join("theme.json"),
            r##"{"id":"theme-ocean","name":"Ocean","kind":"theme","base":"dark","tokens":{"--accent":"#123456"}}"##,
        )
        .unwrap();
        fs::create_dir_all(dir.join("pack")).unwrap();
        fs::write(
            dir.join("pack/manifest.json"),
            r#"{"id":"pack-ops","name":"Ops","kind":"command-pack","groups":[{"name":"Disk","commands":[{"name":"df","template":"df -h"}]}]}"#,
        )
        .unwrap();
        fs::write(dir.join("broken.json"), "{not json").unwrap();

        let scan = PluginRegistry::new(&dir).scan();
        assert_eq!(scan.plugins.len(), 3);
        assert_eq!(scan.packs.len(), 1);
        assert_eq!(scan.packs[0].locale, "fr");
        assert_eq!(scan.themes.len(), 1);
        assert_eq!(scan.themes[0].base, "dark");
        assert_eq!(scan.command_packs.len(), 1);
        assert_eq!(scan.command_packs[0].groups[0].commands[0].name, "df");
        assert_eq!(scan.errors.len(), 1);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn rejects_invalid_ids_and_duplicates() {
        let dir = temp_dir("ids");
        for file in ["a.json", "b.json"] {
            fs::write(
                dir.join(file),
                r#"{"id":"dup","name":"Dup","kind":"language","locale":"en","strings":{"k":"v"}}"#,
            )
            .unwrap();
        }
        fs::write(
            dir.join("bad.json"),
            r#"{"id":"../escape","name":"Bad","kind":"language","locale":"en","strings":{"k":"v"}}"#,
        )
        .unwrap();
        let scan = PluginRegistry::new(&dir).scan();
        assert_eq!(scan.packs.len(), 1);
        assert_eq!(scan.errors.len(), 2);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn validates_wasm_entry_paths() {
        let dir = temp_dir("wasm");
        fs::create_dir_all(dir.join("plug")).unwrap();
        fs::write(
            dir.join("plug/manifest.json"),
            r#"{"id":"plug","name":"Plug","kind":"wasm","entry":"../evil.wasm"}"#,
        )
        .unwrap();
        let scan = PluginRegistry::new(&dir).scan();
        assert_eq!(scan.plugins.len(), 1);
        assert!(PluginRegistry::new(&dir).load_wasm("plug").is_err());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn rejects_unknown_permissions_and_api_versions() {
        let dir = temp_dir("perms");
        fs::write(
            dir.join("a.json"),
            r#"{"id":"a","name":"A","kind":"wasm","permissions":["root"],"entry":"a.wasm"}"#,
        )
        .unwrap();
        fs::write(
            dir.join("b.json"),
            r#"{"id":"b","name":"B","kind":"wasm","apiVersion":99,"entry":"b.wasm"}"#,
        )
        .unwrap();
        let scan = PluginRegistry::new(&dir).scan();
        assert!(scan.plugins.is_empty());
        assert_eq!(scan.errors.len(), 2);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn accepts_transport_plugins_only_with_permission() {
        let dir = temp_dir("serial");
        fs::write(
            dir.join("serial.json"),
            r#"{"id":"serial","name":"Serial Ports","kind":"serial","permissions":["serial"]}"#,
        )
        .unwrap();
        fs::write(
            dir.join("telnet.json"),
            r#"{"id":"telnet","name":"Telnet","kind":"telnet","permissions":["telnet"]}"#,
        )
        .unwrap();
        fs::write(
            dir.join("nope.json"),
            r#"{"id":"nope","name":"Nope","kind":"serial"}"#,
        )
        .unwrap();
        fs::write(
            dir.join("telnet-nope.json"),
            r#"{"id":"telnet-nope","name":"Nope","kind":"telnet"}"#,
        )
        .unwrap();
        let scan = PluginRegistry::new(&dir).scan();
        assert_eq!(scan.plugins.len(), 2);
        assert!(scan.plugins.iter().any(|plugin| plugin.kind == "telnet"));
        assert_eq!(scan.errors.len(), 2);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn installs_builtin_transport_plugins() {
        let dir = temp_dir("builtin");
        let registry = PluginRegistry::new(&dir);
        assert_eq!(registry.scan().builtin.len(), 2);

        let manifest = registry.install_builtin("telnet").unwrap();
        assert_eq!(manifest.kind, "telnet");
        assert_eq!(manifest.permissions, vec!["telnet".to_string()]);
        assert!(dir.join("telnet.json").is_file());
        assert!(registry.install_builtin("unknown").is_err());

        let scan = registry.scan();
        assert!(scan.plugins.iter().any(|plugin| plugin.id == "telnet"));
        assert!(registry.remove("telnet").unwrap());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn parses_and_validates_marketplace_index() {
        let raw = r#"{
          "apiVersion": 1,
          "updated": "2026-11-09",
          "plugins": [{
            "id": "serial",
            "name": "Serial Ports",
            "version": "1.0.0",
            "kind": "serial",
            "manifestUrl": "https://raw.githubusercontent.com/chensnails/ferriterm-plugins/main/plugins/serial/manifest.json"
          }]
        }"#;
        let index = parse_market_index(raw).unwrap();
        assert_eq!(index.plugins.len(), 1);
        assert_eq!(index.plugins[0].id, "serial");

        assert!(parse_market_index(
            r#"{"apiVersion":1,"plugins":[{"id":"a","name":"A","kind":"serial","manifestUrl":"http://example.com/a.json"}]}"#
        )
        .is_err());
        assert!(parse_market_index(
            r#"{"apiVersion":1,"plugins":[{"id":"a","name":"A","kind":"serial","manifestUrl":"https://localhost/a.json"}]}"#
        )
        .is_err());
        assert!(parse_market_index(
            r#"{"apiVersion":1,"plugins":[
                {"id":"a","name":"A","kind":"serial","manifestUrl":"https://example.com/a.json"},
                {"id":"a","name":"B","kind":"serial","manifestUrl":"https://example.com/b.json"}
            ]}"#
        )
        .is_err());
        assert!(parse_market_index(
            r#"{"apiVersion":1,"plugins":[{"id":"a","name":"A","kind":"root","manifestUrl":"https://example.com/a.json"}]}"#
        )
        .is_err());
    }

    #[test]
    fn installs_and_removes_marketplace_manifests() {
        let dir = temp_dir("install");
        let registry = PluginRegistry::new(&dir);
        let manifest =
            r#"{"id":"serial","name":"Serial Ports","kind":"serial","permissions":["serial"]}"#;
        let installed = registry.install("serial", manifest).unwrap();
        assert_eq!(installed.id, "serial");
        assert!(dir.join("serial.json").is_file());
        assert_eq!(registry.scan().plugins.len(), 1);

        assert!(registry.install("other", manifest).is_err());
        assert!(!dir.join("other.json").is_file());
        assert!(registry.install("../escape", manifest).is_err());

        assert!(registry.remove("serial").unwrap());
        assert!(!dir.join("serial.json").exists());
        assert!(!registry.remove("serial").unwrap());
        let _ = fs::remove_dir_all(&dir);
    }
}
