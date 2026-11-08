//! Data-only plugin host. Language packs are the first package type: a single
//! JSON manifest with its ICU strings, dropped into the plugins directory.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

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
pub struct PluginScan {
    pub packs: Vec<LanguagePack>,
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
        let mut packs = Vec::new();
        let mut errors = Vec::new();
        let entries = match fs::read_dir(&self.dir) {
            Ok(entries) => entries,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                return PluginScan { packs, errors };
            }
            Err(err) => {
                errors.push(format!("{}: {err}", self.dir.display()));
                return PluginScan { packs, errors };
            }
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
                continue;
            }
            match load_pack(&path) {
                Ok(Some(pack)) => packs.push(pack),
                Ok(None) => {}
                Err(err) => errors.push(format!("{}: {err}", path.display())),
            }
        }
        packs.sort_by(|a, b| a.name.cmp(&b.name));
        PluginScan { packs, errors }
    }
}

fn load_pack(path: &Path) -> Result<Option<LanguagePack>, String> {
    let raw = fs::read_to_string(path).map_err(|e| e.to_string())?;
    let pack: LanguagePack = serde_json::from_str(&raw).map_err(|e| e.to_string())?;
    if pack.kind != "language" {
        return Ok(None);
    }
    if pack.id.is_empty() || pack.locale.is_empty() || pack.strings.is_empty() {
        return Err("language pack needs id, locale and strings".to_string());
    }
    Ok(Some(pack))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scans_language_packs_and_reports_errors() {
        let dir = std::env::temp_dir().join(format!(
            "ferriterm-plugin-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join("fr.json"),
            r#"{"id":"lang-fr","name":"Français","version":"0.1.0","kind":"language","locale":"fr","strings":{"app.name":"Ferriterm"}}"#,
        )
        .unwrap();
        fs::write(dir.join("broken.json"), "{not json").unwrap();
        fs::write(
            dir.join("other.json"),
            r#"{"id":"x","name":"X","kind":"theme"}"#,
        )
        .unwrap();

        let scan = PluginRegistry::new(&dir).scan();
        assert_eq!(scan.packs.len(), 1);
        assert_eq!(scan.packs[0].locale, "fr");
        assert_eq!(scan.errors.len(), 1);
        let _ = fs::remove_dir_all(&dir);
    }
}
