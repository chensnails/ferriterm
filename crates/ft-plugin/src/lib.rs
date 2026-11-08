//! Data-only plugin host. Each package is a JSON manifest (either a single
//! file or `manifest.json` inside a directory) that contributes a language pack,
//! a theme, a command pack, or a sandboxed WASM entry point.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

pub const API_VERSION: u32 = 1;
const MAX_STRING_LEN: usize = 8 * 1024;
const PERMISSIONS: &[&str] = &["network", "filesystem", "clipboard"];

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

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginScan {
    pub plugins: Vec<PluginManifest>,
    pub packs: Vec<LanguagePack>,
    pub themes: Vec<ThemePack>,
    pub command_packs: Vec<CommandPack>,
    pub errors: Vec<String>,
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
        let mut scan = PluginScan::default();
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
                PluginBody::Wasm => {}
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
}
