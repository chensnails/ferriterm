//! Parsers for third-party session files.
//!
//! Only publicly observable file formats are implemented; no code or assets
//! from the source projects are used. Credentials found in imported files are
//! intentionally dropped — the importer never reads or stores passwords.

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportedHost {
    pub name: String,
    pub host: String,
    pub port: u16,
    pub username: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImportFormat {
    SshConfig,
    Xshell,
    FinalShell,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportPreview {
    pub format: ImportFormat,
    pub hosts: Vec<ImportedHost>,
}

#[derive(Debug)]
pub enum ImportError {
    UnsupportedFormat,
    Parse(String),
    NoHosts,
}

impl std::fmt::Display for ImportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnsupportedFormat => write!(f, "unsupported import format"),
            Self::Parse(message) => write!(f, "parse error: {message}"),
            Self::NoHosts => write!(f, "no SSH hosts found in the content"),
        }
    }
}

impl std::error::Error for ImportError {}

pub fn parse_format(name: &str) -> Option<ImportFormat> {
    match name.to_ascii_lowercase().as_str() {
        "ssh_config" | "sshconfig" | "openssh" => Some(ImportFormat::SshConfig),
        "xshell" | "xsh" => Some(ImportFormat::Xshell),
        "finalshell" | "final_shell" => Some(ImportFormat::FinalShell),
        _ => None,
    }
}

pub fn detect(content: &str) -> ImportFormat {
    let xshell = content
        .lines()
        .any(|line| line.trim().eq_ignore_ascii_case("[CONNECTION]"));
    if xshell {
        return ImportFormat::Xshell;
    }
    let trimmed = content.trim_start_matches('\u{feff}').trim_start();
    if trimmed.starts_with('{') || trimmed.starts_with('[') {
        return ImportFormat::FinalShell;
    }
    ImportFormat::SshConfig
}

/// Parse with an explicit format, or auto-detect when `format` is `None`.
pub fn preview(format: Option<ImportFormat>, content: &str) -> Result<ImportPreview, ImportError> {
    let format = format.unwrap_or_else(|| detect(content));
    let hosts = match format {
        ImportFormat::SshConfig => parse_ssh_config(content)?,
        ImportFormat::Xshell => parse_xshell(content)?,
        ImportFormat::FinalShell => parse_finalshell(content)?,
    };
    Ok(ImportPreview { format, hosts })
}

pub fn parse(format: ImportFormat, content: &str) -> Result<Vec<ImportedHost>, ImportError> {
    preview(Some(format), content).map(|preview| preview.hosts)
}

fn parse_ssh_config(content: &str) -> Result<Vec<ImportedHost>, ImportError> {
    let mut hosts = Vec::new();
    let mut block: Option<Vec<ImportedHost>> = None;

    for raw in content.lines() {
        let line = strip_comment(raw).trim();
        if line.is_empty() {
            continue;
        }
        let (key, value) = match line.split_once(|c: char| c.is_whitespace() || c == '=') {
            Some((key, value)) => (key.to_ascii_lowercase(), value.trim()),
            None => (line.to_ascii_lowercase(), ""),
        };
        match key.as_str() {
            "host" => {
                if let Some(previous) = block.take() {
                    hosts.extend(previous);
                }
                let mut entries = Vec::new();
                for alias in value.split_whitespace() {
                    let alias = unquote(alias);
                    if alias.is_empty() || alias.contains('*') || alias.contains('?') {
                        continue;
                    }
                    entries.push(ImportedHost {
                        name: alias.clone(),
                        host: alias,
                        port: 22,
                        username: String::new(),
                    });
                }
                block = (!entries.is_empty()).then_some(entries);
            }
            "match" => {
                if let Some(previous) = block.take() {
                    hosts.extend(previous);
                }
            }
            _ => {
                if let Some(entries) = block.as_mut() {
                    apply_ssh_option(entries, &key, value);
                }
            }
        }
    }
    if let Some(previous) = block.take() {
        hosts.extend(previous);
    }
    if hosts.is_empty() {
        return Err(ImportError::NoHosts);
    }
    Ok(hosts)
}

fn apply_ssh_option(entries: &mut [ImportedHost], key: &str, raw_value: &str) {
    let value = unquote(raw_value);
    if value.is_empty() {
        return;
    }
    match key {
        "hostname" => entries
            .iter_mut()
            .for_each(|entry| entry.host = value.clone()),
        "user" => entries
            .iter_mut()
            .for_each(|entry| entry.username = value.clone()),
        "port" => {
            if let Ok(port) = value.parse::<u16>() {
                entries.iter_mut().for_each(|entry| entry.port = port);
            }
        }
        _ => {}
    }
}

fn unquote(value: &str) -> String {
    let value = value.trim();
    let bytes = value.as_bytes();
    if bytes.len() >= 2 {
        let first = bytes[0];
        let last = bytes[bytes.len() - 1];
        if (first == b'"' && last == b'"') || (first == b'\'' && last == b'\'') {
            return value[1..value.len() - 1].to_string();
        }
    }
    value.to_string()
}

fn strip_comment(line: &str) -> &str {
    let mut quoted = false;
    for (index, ch) in line.char_indices() {
        match ch {
            '"' => quoted = !quoted,
            '#' if !quoted => return &line[..index],
            _ => {}
        }
    }
    line
}

fn parse_xshell(content: &str) -> Result<Vec<ImportedHost>, ImportError> {
    let mut section = String::new();
    let mut name: Option<String> = None;
    let mut host: Option<String> = None;
    let mut port = 22;
    let mut username: Option<String> = None;
    let mut protocol: Option<String> = None;

    for raw in content.lines() {
        let line = raw.trim().trim_start_matches('\u{feff}');
        if line.is_empty() || line.starts_with(';') || line.starts_with('#') {
            continue;
        }
        if let Some(inner) = line
            .strip_prefix('[')
            .and_then(|rest| rest.strip_suffix(']'))
        {
            section = inner.to_ascii_lowercase();
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let key = key.trim().to_ascii_lowercase();
        let value = value.trim();
        match (section.as_str(), key.as_str()) {
            ("connection", "host") => host = Some(value.to_string()),
            ("connection", "port") => {
                if let Ok(parsed) = value.parse::<u16>() {
                    port = parsed;
                }
            }
            ("connection", "protocol") => protocol = Some(value.to_ascii_lowercase()),
            ("connection:authentication", "username" | "user") => {
                username = Some(value.to_string())
            }
            ("session", "name") => name = Some(value.to_string()),
            _ => {}
        }
    }

    if let Some(protocol) = protocol {
        if protocol != "ssh" && protocol != "sftp" {
            return Err(ImportError::NoHosts);
        }
    }
    let host = host
        .filter(|value| !value.is_empty())
        .ok_or(ImportError::NoHosts)?;
    let name = name
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| host.clone());
    Ok(vec![ImportedHost {
        name,
        host,
        port,
        username: username.unwrap_or_default(),
    }])
}

fn parse_finalshell(content: &str) -> Result<Vec<ImportedHost>, ImportError> {
    let value: Value =
        serde_json::from_str(content).map_err(|error| ImportError::Parse(error.to_string()))?;
    let entries = match value {
        Value::Array(items) => items,
        other => vec![other],
    };

    let mut hosts = Vec::new();
    for entry in entries {
        let Value::Object(map) = entry else {
            continue;
        };
        let Some(host) = string_field(&map, &["host", "host_name", "address"]) else {
            continue;
        };
        let name = string_field(&map, &["name", "session_name"]).unwrap_or_else(|| host.clone());
        let username = string_field(&map, &["user_name", "username", "user"]).unwrap_or_default();
        let port = map.get("port").and_then(port_value).unwrap_or(22);
        hosts.push(ImportedHost {
            name,
            host,
            port,
            username,
        });
    }

    if hosts.is_empty() {
        Err(ImportError::NoHosts)
    } else {
        Ok(hosts)
    }
}

fn string_field(map: &serde_json::Map<String, Value>, keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|key| {
        map.get(*key)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
    })
}

fn port_value(value: &Value) -> Option<u16> {
    match value {
        Value::Number(number) => number.as_u64().and_then(|port| u16::try_from(port).ok()),
        Value::String(text) => text.trim().parse().ok(),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SSH_CONFIG: &str = r#"
# global defaults
Host *
    ServerAliveInterval 30

Host web-prod
    HostName 10.0.0.10
    User deploy
    Port 2222

Host db db.local
    HostName "db.internal.example"
    User admin  # inline comment
"#;

    const XSHELL: &str = "\u{feff}[Session]\r\nVersion=7.0\r\nName=prod-web\r\n\
[CONNECTION]\r\nProtocol=SSH\r\nHost=10.0.0.20\r\nPort=22\r\n\
[CONNECTION:AUTHENTICATION]\r\nUserName=root\r\n";

    const FINALSHELL: &str = r#"{
  "name": "fs-box",
  "host": "10.0.0.30",
  "port": "2200",
  "user_name": "ubuntu",
  "password": "must-not-be-imported"
}"#;

    #[test]
    fn parses_ssh_config_blocks() {
        let hosts = parse(ImportFormat::SshConfig, SSH_CONFIG).unwrap();
        assert_eq!(hosts.len(), 3);
        let web = hosts.iter().find(|h| h.name == "web-prod").unwrap();
        assert_eq!(web.host, "10.0.0.10");
        assert_eq!(web.username, "deploy");
        assert_eq!(web.port, 2222);
        let db = hosts.iter().find(|h| h.name == "db").unwrap();
        assert_eq!(db.host, "db.internal.example");
        assert_eq!(db.username, "admin");
        assert!(hosts.iter().all(|h| h.name != "*"));
    }

    #[test]
    fn ssh_config_defaults_to_alias_and_port() {
        let hosts = parse(ImportFormat::SshConfig, "Host plain\n  User me\n").unwrap();
        assert_eq!(
            hosts,
            vec![ImportedHost {
                name: "plain".into(),
                host: "plain".into(),
                port: 22,
                username: "me".into()
            }]
        );
    }

    #[test]
    fn parses_xshell_session() {
        let hosts = parse(ImportFormat::Xshell, XSHELL).unwrap();
        assert_eq!(
            hosts,
            vec![ImportedHost {
                name: "prod-web".into(),
                host: "10.0.0.20".into(),
                port: 22,
                username: "root".into()
            }]
        );
    }

    #[test]
    fn rejects_non_ssh_xshell_session() {
        let content = "[CONNECTION]\nProtocol=Telnet\nHost=10.0.0.1\n";
        let error = parse(ImportFormat::Xshell, content).unwrap_err();
        assert!(matches!(error, ImportError::NoHosts));
    }

    #[test]
    fn parses_finalshell_json() {
        let hosts = parse(ImportFormat::FinalShell, FINALSHELL).unwrap();
        assert_eq!(
            hosts,
            vec![ImportedHost {
                name: "fs-box".into(),
                host: "10.0.0.30".into(),
                port: 2200,
                username: "ubuntu".into()
            }]
        );
    }

    #[test]
    fn parses_finalshell_array() {
        let content =
            r#"[{"host":"a.example"},{"host":"b.example","port":2022,"user_name":"ops"}]"#;
        let hosts = parse(ImportFormat::FinalShell, content).unwrap();
        assert_eq!(hosts.len(), 2);
        assert_eq!(hosts[1].port, 2022);
        assert_eq!(hosts[0].name, "a.example");
    }

    #[test]
    fn auto_detects_each_format() {
        assert_eq!(detect(SSH_CONFIG), ImportFormat::SshConfig);
        assert_eq!(detect(XSHELL), ImportFormat::Xshell);
        assert_eq!(detect(FINALSHELL), ImportFormat::FinalShell);
    }

    #[test]
    fn preview_reports_detected_format() {
        let preview = preview(None, FINALSHELL).unwrap();
        assert_eq!(preview.format, ImportFormat::FinalShell);
        assert_eq!(preview.hosts.len(), 1);
    }

    #[test]
    fn invalid_json_is_a_parse_error() {
        let error = preview(Some(ImportFormat::FinalShell), "{not json").unwrap_err();
        assert!(matches!(error, ImportError::Parse(_)));
    }

    #[test]
    fn empty_content_has_no_hosts() {
        assert!(matches!(
            preview(None, "").unwrap_err(),
            ImportError::NoHosts
        ));
    }

    #[test]
    fn format_names_are_case_insensitive() {
        assert_eq!(parse_format("SSH_CONFIG"), Some(ImportFormat::SshConfig));
        assert_eq!(parse_format("xsh"), Some(ImportFormat::Xshell));
        assert_eq!(parse_format("nope"), None);
    }
}
