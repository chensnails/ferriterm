use std::collections::HashMap;
use std::path::Path;
use std::sync::Mutex;

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

const MIGRATIONS: &[&str] = &[
    include_str!("../migrations/0001_init.sql"),
    include_str!("../migrations/0002_host_notes.sql"),
    include_str!("../migrations/0003_commands.sql"),
    include_str!("../migrations/0004_host_key.sql"),
    include_str!("../migrations/0005_host_jump_forward_rules.sql"),
    include_str!("../migrations/0006_transfer_jobs.sql"),
    include_str!("../migrations/0007_vault.sql"),
    include_str!("../migrations/0008_host_protocol_icon.sql"),
];

pub const PROTOCOLS: &[&str] = &["ssh", "sftp", "telnet", "serial", "local"];

fn default_protocol() -> String {
    "ssh".to_string()
}

/// Per-protocol extras. Everything is optional so one JSON column can hold
/// serial, telnet and local settings without a migration per protocol.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ProtocolOptions {
    pub serial_path: String,
    pub baud_rate: u32,
    pub data_bits: u8,
    pub parity: String,
    pub stop_bits: u8,
    pub flow_control: String,
    pub shell: String,
    pub cwd: String,
    pub encoding: String,
}

impl Default for ProtocolOptions {
    fn default() -> Self {
        Self {
            serial_path: String::new(),
            baud_rate: 115_200,
            data_bits: 8,
            parity: "none".to_string(),
            stop_bits: 1,
            flow_control: "none".to_string(),
            shell: String::new(),
            cwd: String::new(),
            encoding: "utf-8".to_string(),
        }
    }
}

fn normalize_protocol(raw: &str) -> String {
    let value = raw.trim().to_ascii_lowercase();
    if PROTOCOLS.contains(&value.as_str()) {
        value
    } else {
        default_protocol()
    }
}

fn normalize_icon(raw: &str) -> String {
    let value = raw.trim();
    if value.len() > 40
        || !value
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
    {
        return String::new();
    }
    value.to_string()
}

fn encode_options(options: &ProtocolOptions) -> String {
    serde_json::to_string(options).unwrap_or_else(|_| "{}".to_string())
}

fn decode_options(raw: &str) -> ProtocolOptions {
    serde_json::from_str(raw).unwrap_or_default()
}

pub struct Store {
    conn: Mutex<Connection>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Host {
    pub id: String,
    pub name: String,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub auth_method: String,
    #[serde(default)]
    pub key_path: String,
    #[serde(default)]
    pub key_id: String,
    #[serde(default)]
    pub jump_host_id: String,
    #[serde(default)]
    pub x11: bool,
    pub parent_id: Option<String>,
    pub is_folder: bool,
    #[serde(default)]
    pub production: bool,
    #[serde(default = "default_protocol")]
    pub protocol: String,
    #[serde(default)]
    pub protocol_options: ProtocolOptions,
    #[serde(default)]
    pub icon: String,
    pub sort_order: i64,
    pub notes: String,
    pub tags: Vec<Tag>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Tag {
    pub id: String,
    pub name: String,
    pub color: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HostInput {
    pub id: String,
    pub name: String,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub auth_method: String,
    #[serde(default)]
    pub key_path: String,
    #[serde(default)]
    pub key_id: String,
    #[serde(default)]
    pub jump_host_id: String,
    #[serde(default)]
    pub x11: bool,
    #[serde(default = "default_protocol")]
    pub protocol: String,
    #[serde(default)]
    pub protocol_options: ProtocolOptions,
    #[serde(default)]
    pub icon: String,
    pub parent_id: Option<String>,
    pub is_folder: bool,
    pub production: bool,
    pub notes: String,
    pub tag_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandItem {
    pub id: String,
    pub group_id: String,
    pub name: String,
    pub template: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandGroup {
    pub id: String,
    pub name: String,
    pub commands: Vec<CommandItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ForwardRule {
    pub id: String,
    pub host_id: String,
    pub kind: String,
    pub bind_host: String,
    pub bind_port: u16,
    pub dest_host: String,
    pub dest_port: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportStats {
    pub hosts_added: usize,
    pub hosts_skipped: usize,
    pub tags_added: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SshKey {
    pub id: String,
    pub name: String,
    pub algorithm: String,
    pub fingerprint: String,
    pub public_key: String,
    #[serde(default)]
    pub comment: String,
    #[serde(default)]
    pub source_path: String,
    #[serde(default)]
    pub has_passphrase: bool,
    #[serde(default)]
    pub created_at: String,
    #[serde(default)]
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VaultEntry {
    pub secret_ref: String,
    pub kind: String,
    pub label: String,
    pub host_id: Option<String>,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuditRecord {
    pub id: i64,
    pub at: String,
    pub action: String,
    pub target: String,
    pub detail: String,
}

#[derive(Debug)]
pub enum StoreError {
    Db(rusqlite::Error),
    NotFound,
    ParentNotFolder,
    Cycle,
}

impl std::fmt::Display for StoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Db(e) => write!(f, "database error: {e}"),
            Self::NotFound => write!(f, "host not found"),
            Self::ParentNotFolder => write!(f, "parent must be a folder"),
            Self::Cycle => write!(f, "cannot move a folder into itself or its descendant"),
        }
    }
}

impl std::error::Error for StoreError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Db(e) => Some(e),
            _ => None,
        }
    }
}

impl From<rusqlite::Error> for StoreError {
    fn from(e: rusqlite::Error) -> Self {
        Self::Db(e)
    }
}

impl Store {
    pub fn open(path: &Path) -> rusqlite::Result<Self> {
        Self::from_connection(Connection::open(path)?)
    }

    pub fn open_in_memory() -> rusqlite::Result<Self> {
        Self::from_connection(Connection::open_in_memory()?)
    }

    fn from_connection(conn: Connection) -> rusqlite::Result<Self> {
        conn.pragma_update(None, "foreign_keys", true)?;
        let store = Self {
            conn: Mutex::new(conn),
        };
        store.migrate()?;
        Ok(store)
    }

    fn migrate(&self) -> rusqlite::Result<()> {
        let mut conn = self.conn.lock().unwrap();
        let version: i64 = conn.pragma_query_value(None, "user_version", |r| r.get(0))?;
        for (i, sql) in MIGRATIONS.iter().enumerate().skip(version as usize) {
            let tx = conn.transaction()?;
            tx.execute_batch(sql)?;
            tx.pragma_update(None, "user_version", (i + 1) as i64)?;
            tx.commit()?;
        }
        Ok(())
    }

    pub fn set_setting(&self, key: &str, value: &str) -> rusqlite::Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO settings(key, value) VALUES(?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            [key, value],
        )?;
        Ok(())
    }

    pub fn get_setting(&self, key: &str) -> rusqlite::Result<Option<String>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT value FROM settings WHERE key = ?1")?;
        let mut rows = stmt.query([key])?;
        match rows.next()? {
            Some(row) => Ok(Some(row.get(0)?)),
            None => Ok(None),
        }
    }

    pub fn list_settings(&self) -> rusqlite::Result<std::collections::BTreeMap<String, String>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT key, value FROM settings")?;
        let rows = stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?;
        rows.collect()
    }

    /// One row per transfer that had not finished when the app closed. The
    /// payload is the job exactly as the queue showed it, so restoring needs no
    /// schema of its own.
    pub fn save_transfer_job(&self, id: &str, payload: &str) -> rusqlite::Result<()> {
        let conn = self.conn.lock().unwrap();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|elapsed| elapsed.as_secs() as i64)
            .unwrap_or(0);
        conn.execute(
            "INSERT INTO transfer_jobs(id, payload, updated_at) VALUES(?1, ?2, ?3)
             ON CONFLICT(id) DO UPDATE SET payload = excluded.payload,
                                           updated_at = excluded.updated_at",
            params![id, payload, now],
        )?;
        Ok(())
    }

    pub fn delete_transfer_job(&self, id: &str) -> rusqlite::Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM transfer_jobs WHERE id = ?1", [id])?;
        Ok(())
    }

    pub fn list_transfer_jobs(&self) -> rusqlite::Result<Vec<String>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT payload FROM transfer_jobs ORDER BY updated_at")?;
        let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
        rows.collect()
    }

    pub fn list_hosts(&self) -> rusqlite::Result<Vec<Host>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, name, host, port, username, auth_method, key_path, key_id, jump_host_id,
                    x11, parent_id, is_folder, production, sort_order, notes, protocol,
                    protocol_options, icon
             FROM hosts ORDER BY is_folder DESC, sort_order, name",
        )?;
        let mut hosts: Vec<Host> = stmt
            .query_map([], |row| {
                Ok(Host {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    host: row.get(2)?,
                    port: row.get(3)?,
                    username: row.get(4)?,
                    auth_method: row.get(5)?,
                    key_path: row.get(6)?,
                    key_id: row.get(7)?,
                    jump_host_id: row.get(8)?,
                    x11: row.get(9)?,
                    parent_id: row.get(10)?,
                    is_folder: row.get(11)?,
                    production: row.get(12)?,
                    sort_order: row.get(13)?,
                    notes: row.get(14)?,
                    protocol: row.get(15)?,
                    protocol_options: decode_options(&row.get::<_, String>(16)?),
                    icon: row.get(17)?,
                    tags: Vec::new(),
                })
            })?
            .collect::<rusqlite::Result<_>>()?;
        drop(stmt);

        let mut tag_stmt = conn.prepare(
            "SELECT ht.host_id, t.id, t.name, t.color
             FROM host_tags ht JOIN tags t ON t.id = ht.tag_id
             ORDER BY t.name",
        )?;
        let mut rows = tag_stmt.query([])?;
        let mut by_host: HashMap<String, Vec<Tag>> = HashMap::new();
        while let Some(row) = rows.next()? {
            let host_id: String = row.get(0)?;
            by_host.entry(host_id).or_default().push(Tag {
                id: row.get(1)?,
                name: row.get(2)?,
                color: row.get(3)?,
            });
        }
        for host in &mut hosts {
            if let Some(tags) = by_host.remove(&host.id) {
                host.tags = tags;
            }
        }
        Ok(hosts)
    }

    pub fn create_host(&self, input: &HostInput) -> Result<(), StoreError> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        if let Some(parent) = &input.parent_id {
            ensure_folder(&tx, parent)?;
        }
        let sort_order: i64 = tx.query_row(
            "SELECT COALESCE(MAX(sort_order), -1) + 1 FROM hosts WHERE parent_id IS ?1",
            params![input.parent_id],
            |row| row.get(0),
        )?;
        tx.execute(
            "INSERT INTO hosts(id, name, host, port, username, auth_method, key_path, key_id,
                               jump_host_id, x11, parent_id, is_folder, production, sort_order,
                               notes, protocol, protocol_options, icon)
             VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18)",
            params![
                input.id,
                input.name,
                input.host,
                input.port,
                input.username,
                input.auth_method,
                input.key_path,
                input.key_id,
                input.jump_host_id,
                input.x11,
                input.parent_id,
                input.is_folder,
                input.production,
                sort_order,
                input.notes,
                normalize_protocol(&input.protocol),
                encode_options(&input.protocol_options),
                normalize_icon(&input.icon),
            ],
        )?;
        replace_tags(&tx, &input.id, &input.tag_ids)?;
        tx.commit()?;
        Ok(())
    }

    pub fn update_host(&self, input: &HostInput) -> Result<(), StoreError> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        if let Some(parent) = &input.parent_id {
            ensure_folder(&tx, parent)?;
            if input.is_folder && is_self_or_descendant(&tx, parent, &input.id)? {
                return Err(StoreError::Cycle);
            }
        }
        let changed = tx.execute(
            "UPDATE hosts
             SET name = ?2, host = ?3, port = ?4, username = ?5, auth_method = ?6,
                 key_path = ?7, key_id = ?8, jump_host_id = ?9, x11 = ?10, parent_id = ?11,
                 production = ?12, notes = ?13, protocol = ?14, protocol_options = ?15,
                 icon = ?16, updated_at = datetime('now')
             WHERE id = ?1",
            params![
                input.id,
                input.name,
                input.host,
                input.port,
                input.username,
                input.auth_method,
                input.key_path,
                input.key_id,
                input.jump_host_id,
                input.x11,
                input.parent_id,
                input.production,
                input.notes,
                normalize_protocol(&input.protocol),
                encode_options(&input.protocol_options),
                normalize_icon(&input.icon),
            ],
        )?;
        if changed == 0 {
            return Err(StoreError::NotFound);
        }
        replace_tags(&tx, &input.id, &input.tag_ids)?;
        tx.commit()?;
        Ok(())
    }

    pub fn delete_host(&self, id: &str) -> Result<(), StoreError> {
        let conn = self.conn.lock().unwrap();
        let changed = conn.execute("DELETE FROM hosts WHERE id = ?1", [id])?;
        if changed == 0 {
            return Err(StoreError::NotFound);
        }
        Ok(())
    }

    pub fn list_forward_rules(&self, host_id: Option<&str>) -> rusqlite::Result<Vec<ForwardRule>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, host_id, kind, bind_host, bind_port, dest_host, dest_port
             FROM forward_rules
             WHERE ?1 IS NULL OR host_id = ?1
             ORDER BY bind_port",
        )?;
        let rules = stmt
            .query_map([host_id], |row| {
                Ok(ForwardRule {
                    id: row.get(0)?,
                    host_id: row.get(1)?,
                    kind: row.get(2)?,
                    bind_host: row.get(3)?,
                    bind_port: row.get(4)?,
                    dest_host: row.get(5)?,
                    dest_port: row.get(6)?,
                })
            })?
            .collect();
        rules
    }

    pub fn save_forward_rule(&self, rule: &ForwardRule) -> rusqlite::Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO forward_rules(id, host_id, kind, bind_host, bind_port, dest_host, dest_port)
             VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(id) DO UPDATE SET
               kind = excluded.kind,
               bind_host = excluded.bind_host,
               bind_port = excluded.bind_port,
               dest_host = excluded.dest_host,
               dest_port = excluded.dest_port",
            params![
                rule.id,
                rule.host_id,
                rule.kind,
                rule.bind_host,
                rule.bind_port,
                rule.dest_host,
                rule.dest_port,
            ],
        )?;
        Ok(())
    }

    pub fn delete_forward_rule(&self, id: &str) -> rusqlite::Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM forward_rules WHERE id = ?1", [id])?;
        Ok(())
    }

    pub fn list_tags(&self) -> rusqlite::Result<Vec<Tag>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT id, name, color FROM tags ORDER BY name")?;
        let tags = stmt
            .query_map([], |row| {
                Ok(Tag {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    color: row.get(2)?,
                })
            })?
            .collect();
        tags
    }

    pub fn create_tag(&self, tag: &Tag) -> rusqlite::Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO tags(id, name, color) VALUES(?1, ?2, ?3)",
            params![tag.id, tag.name, tag.color],
        )?;
        Ok(())
    }

    pub fn update_tag(&self, tag: &Tag) -> Result<(), StoreError> {
        let conn = self.conn.lock().unwrap();
        let changed = conn.execute(
            "UPDATE tags SET name = ?2, color = ?3 WHERE id = ?1",
            params![tag.id, tag.name, tag.color],
        )?;
        if changed == 0 {
            return Err(StoreError::NotFound);
        }
        Ok(())
    }

    pub fn delete_tag(&self, id: &str) -> Result<(), StoreError> {
        let conn = self.conn.lock().unwrap();
        let changed = conn.execute("DELETE FROM tags WHERE id = ?1", [id])?;
        if changed == 0 {
            return Err(StoreError::NotFound);
        }
        Ok(())
    }

    pub fn list_command_groups(&self) -> rusqlite::Result<Vec<CommandGroup>> {
        let conn = self.conn.lock().unwrap();
        let mut groups_stmt =
            conn.prepare("SELECT id, name FROM command_groups ORDER BY sort_order, name")?;
        let mut commands_stmt = conn.prepare(
            "SELECT id, group_id, name, template FROM commands ORDER BY sort_order, name",
        )?;
        let items: Vec<CommandItem> = commands_stmt
            .query_map([], |row| {
                Ok(CommandItem {
                    id: row.get(0)?,
                    group_id: row.get(1)?,
                    name: row.get(2)?,
                    template: row.get(3)?,
                })
            })?
            .collect::<rusqlite::Result<_>>()?;
        let groups = groups_stmt
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(groups
            .into_iter()
            .map(|(id, name)| {
                let commands = items.iter().filter(|i| i.group_id == id).cloned().collect();
                CommandGroup { id, name, commands }
            })
            .collect())
    }

    pub fn import_command_group(
        &self,
        group_id: &str,
        name: &str,
        commands: &[(String, String, String)],
    ) -> Result<usize, StoreError> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        tx.execute(
            "INSERT OR IGNORE INTO command_groups(id, name, sort_order)
             SELECT ?1, ?2, COALESCE(MAX(sort_order), -1) + 1 FROM command_groups",
            params![group_id, name],
        )?;
        let mut inserted = 0;
        for (index, (id, command_name, template)) in commands.iter().enumerate() {
            inserted += tx.execute(
                "INSERT OR IGNORE INTO commands(id, group_id, name, template, sort_order)
                 VALUES(?1, ?2, ?3, ?4, ?5)",
                params![id, group_id, command_name, template, index as i64],
            )?;
        }
        tx.commit()?;
        Ok(inserted)
    }

    pub fn create_command_group(&self, id: &str, name: &str) -> rusqlite::Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO command_groups(id, name, sort_order)
             SELECT ?1, ?2, COALESCE(MAX(sort_order), -1) + 1 FROM command_groups",
            params![id, name],
        )?;
        Ok(())
    }

    pub fn update_command_group(&self, id: &str, name: &str) -> Result<(), StoreError> {
        let conn = self.conn.lock().unwrap();
        let changed = conn.execute(
            "UPDATE command_groups SET name = ?2 WHERE id = ?1",
            params![id, name],
        )?;
        if changed == 0 {
            return Err(StoreError::NotFound);
        }
        Ok(())
    }

    pub fn delete_command_group(&self, id: &str) -> Result<(), StoreError> {
        let conn = self.conn.lock().unwrap();
        let changed = conn.execute("DELETE FROM command_groups WHERE id = ?1", [id])?;
        if changed == 0 {
            return Err(StoreError::NotFound);
        }
        Ok(())
    }

    pub fn create_command(
        &self,
        id: &str,
        group_id: &str,
        name: &str,
        template: &str,
    ) -> rusqlite::Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO commands(id, group_id, name, template, sort_order)
             SELECT ?1, ?2, ?3, ?4, COALESCE(MAX(sort_order), -1) + 1
             FROM commands WHERE group_id = ?2",
            params![id, group_id, name, template],
        )?;
        Ok(())
    }

    pub fn update_command(&self, id: &str, name: &str, template: &str) -> Result<(), StoreError> {
        let conn = self.conn.lock().unwrap();
        let changed = conn.execute(
            "UPDATE commands SET name = ?2, template = ?3 WHERE id = ?1",
            params![id, name, template],
        )?;
        if changed == 0 {
            return Err(StoreError::NotFound);
        }
        Ok(())
    }

    pub fn delete_command(&self, id: &str) -> Result<(), StoreError> {
        let conn = self.conn.lock().unwrap();
        let changed = conn.execute("DELETE FROM commands WHERE id = ?1", [id])?;
        if changed == 0 {
            return Err(StoreError::NotFound);
        }
        Ok(())
    }

    pub fn import_snapshot(&self, hosts: &[Host], tags: &[Tag]) -> Result<ImportStats, StoreError> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        tx.pragma_update(None, "defer_foreign_keys", true)?;

        let mut tag_ids: HashMap<String, String> = HashMap::new();
        let mut tag_names: HashMap<String, String> = HashMap::new();
        let mut tags_added = 0;
        for tag in tags {
            let existing: Option<String> = tx
                .query_row("SELECT id FROM tags WHERE name = ?1", [&tag.name], |row| {
                    row.get(0)
                })
                .optional()?;
            let local_id = match existing {
                Some(id) => id,
                None => {
                    tx.execute(
                        "INSERT INTO tags(id, name, color) VALUES(?1, ?2, ?3)",
                        params![tag.id, tag.name, tag.color],
                    )?;
                    tags_added += 1;
                    tag.id.clone()
                }
            };
            tag_ids.insert(tag.id.clone(), local_id.clone());
            tag_names.insert(tag.name.clone(), local_id);
        }

        let mut hosts_added = 0;
        let mut hosts_skipped = 0;
        for host in hosts {
            let exists: Option<i64> = tx
                .query_row("SELECT 1 FROM hosts WHERE id = ?1", [&host.id], |row| {
                    row.get(0)
                })
                .optional()?;
            if exists.is_some() {
                hosts_skipped += 1;
                continue;
            }
            tx.execute(
                "INSERT INTO hosts(id, name, host, port, username, auth_method, key_path, key_id,
                                   parent_id, is_folder, production, sort_order, notes, protocol,
                                   protocol_options, icon)
                 VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)",
                params![
                    host.id,
                    host.name,
                    host.host,
                    host.port,
                    host.username,
                    host.auth_method,
                    host.key_path,
                    host.key_id,
                    host.parent_id,
                    host.is_folder,
                    host.production,
                    host.sort_order,
                    host.notes,
                    normalize_protocol(&host.protocol),
                    encode_options(&host.protocol_options),
                    normalize_icon(&host.icon),
                ],
            )?;
            for tag in &host.tags {
                if let Some(local) = tag_ids.get(&tag.id).or_else(|| tag_names.get(&tag.name)) {
                    tx.execute(
                        "INSERT OR IGNORE INTO host_tags(host_id, tag_id) VALUES(?1, ?2)",
                        params![host.id, local],
                    )?;
                }
            }
            hosts_added += 1;
        }

        tx.commit()?;
        Ok(ImportStats {
            hosts_added,
            hosts_skipped,
            tags_added,
        })
    }

    pub fn list_ssh_keys(&self) -> rusqlite::Result<Vec<SshKey>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, name, algorithm, fingerprint, public_key, comment, source_path,
                    has_passphrase, created_at, updated_at
             FROM ssh_keys ORDER BY name",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(SshKey {
                id: row.get(0)?,
                name: row.get(1)?,
                algorithm: row.get(2)?,
                fingerprint: row.get(3)?,
                public_key: row.get(4)?,
                comment: row.get(5)?,
                source_path: row.get(6)?,
                has_passphrase: row.get(7)?,
                created_at: row.get(8)?,
                updated_at: row.get(9)?,
            })
        })?;
        rows.collect()
    }

    pub fn save_ssh_key(&self, key: &SshKey) -> rusqlite::Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO ssh_keys(id, name, algorithm, fingerprint, public_key, comment,
                                  source_path, has_passphrase, created_at, updated_at)
             VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8,
                    COALESCE(NULLIF(?9, ''), datetime('now')), datetime('now'))
             ON CONFLICT(id) DO UPDATE SET
                 name = excluded.name,
                 algorithm = excluded.algorithm,
                 fingerprint = excluded.fingerprint,
                 public_key = excluded.public_key,
                 comment = excluded.comment,
                 source_path = excluded.source_path,
                 has_passphrase = excluded.has_passphrase,
                 updated_at = datetime('now')",
            params![
                key.id,
                key.name,
                key.algorithm,
                key.fingerprint,
                key.public_key,
                key.comment,
                key.source_path,
                key.has_passphrase,
                key.created_at,
            ],
        )?;
        Ok(())
    }

    pub fn delete_ssh_key(&self, id: &str) -> rusqlite::Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute("UPDATE hosts SET key_id = '' WHERE key_id = ?1", [id])?;
        conn.execute("DELETE FROM ssh_keys WHERE id = ?1", [id])?;
        Ok(())
    }

    pub fn list_vault_entries(&self) -> rusqlite::Result<Vec<VaultEntry>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT secret_ref, kind, label, host_id, updated_at
             FROM vault_entries ORDER BY host_id IS NULL, label, kind",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(VaultEntry {
                secret_ref: row.get(0)?,
                kind: row.get(1)?,
                label: row.get(2)?,
                host_id: row.get(3)?,
                updated_at: row.get(4)?,
            })
        })?;
        rows.collect()
    }

    pub fn upsert_vault_entry(
        &self,
        secret_ref: &str,
        kind: &str,
        label: &str,
        host_id: Option<&str>,
    ) -> rusqlite::Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO vault_entries(secret_ref, kind, label, host_id)
             VALUES(?1, ?2, ?3, ?4)
             ON CONFLICT(secret_ref) DO UPDATE SET
                 kind = excluded.kind,
                 label = excluded.label,
                 host_id = excluded.host_id,
                 updated_at = datetime('now')",
            params![secret_ref, kind, label, host_id],
        )?;
        Ok(())
    }

    pub fn delete_vault_entry(&self, secret_ref: &str) -> rusqlite::Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "DELETE FROM vault_entries WHERE secret_ref = ?1",
            [secret_ref],
        )?;
        Ok(())
    }

    pub fn clear_vault_entries(&self) -> rusqlite::Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM vault_entries", [])?;
        Ok(())
    }

    pub fn append_audit(&self, action: &str, target: &str, detail: &str) -> rusqlite::Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO credential_audit(action, target, detail) VALUES(?1, ?2, ?3)",
            params![action, target, detail],
        )?;
        Ok(())
    }

    pub fn list_audit(&self, limit: usize) -> rusqlite::Result<Vec<AuditRecord>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, at, action, target, detail FROM credential_audit
             ORDER BY id DESC LIMIT ?1",
        )?;
        let rows = stmt.query_map([limit as i64], |row| {
            Ok(AuditRecord {
                id: row.get(0)?,
                at: row.get(1)?,
                action: row.get(2)?,
                target: row.get(3)?,
                detail: row.get(4)?,
            })
        })?;
        rows.collect()
    }
}

fn ensure_folder(conn: &Connection, parent_id: &str) -> Result<(), StoreError> {
    let is_folder: Option<bool> = conn
        .query_row(
            "SELECT is_folder FROM hosts WHERE id = ?1",
            [parent_id],
            |row| row.get(0),
        )
        .optional()?;
    match is_folder {
        Some(true) => Ok(()),
        Some(false) => Err(StoreError::ParentNotFolder),
        None => Err(StoreError::NotFound),
    }
}

fn is_self_or_descendant(conn: &Connection, candidate: &str, id: &str) -> rusqlite::Result<bool> {
    let hits: i64 = conn.query_row(
        "WITH RECURSIVE ancestors(id, parent_id) AS (
             SELECT id, parent_id FROM hosts WHERE id = ?1
             UNION ALL
             SELECT h.id, h.parent_id FROM hosts h JOIN ancestors a ON h.id = a.parent_id
         )
         SELECT COUNT(*) FROM ancestors WHERE id = ?2",
        params![candidate, id],
        |row| row.get(0),
    )?;
    Ok(hits > 0)
}

fn replace_tags(conn: &Connection, host_id: &str, tag_ids: &[String]) -> rusqlite::Result<()> {
    conn.execute("DELETE FROM host_tags WHERE host_id = ?1", [host_id])?;
    for tag_id in tag_ids {
        conn.execute(
            "INSERT OR IGNORE INTO host_tags(host_id, tag_id) VALUES(?1, ?2)",
            params![host_id, tag_id],
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn host_input(id: &str, name: &str, parent: Option<&str>) -> HostInput {
        HostInput {
            id: id.into(),
            name: name.into(),
            host: "127.0.0.1".into(),
            port: 22,
            username: "test".into(),
            auth_method: "password".into(),
            key_path: String::new(),
            key_id: String::new(),
            jump_host_id: String::new(),
            x11: false,
            protocol: "ssh".into(),
            protocol_options: ProtocolOptions::default(),
            icon: String::new(),
            parent_id: parent.map(Into::into),
            is_folder: false,
            production: false,
            notes: String::new(),
            tag_ids: Vec::new(),
        }
    }

    fn folder_input(id: &str, name: &str, parent: Option<&str>) -> HostInput {
        HostInput {
            id: id.into(),
            name: name.into(),
            host: String::new(),
            port: 22,
            username: String::new(),
            auth_method: "password".into(),
            key_path: String::new(),
            key_id: String::new(),
            jump_host_id: String::new(),
            x11: false,
            protocol: "ssh".into(),
            protocol_options: ProtocolOptions::default(),
            icon: String::new(),
            parent_id: parent.map(Into::into),
            is_folder: true,
            production: false,
            notes: String::new(),
            tag_ids: Vec::new(),
        }
    }

    #[test]
    fn protocol_options_and_icon_roundtrip() {
        let store = Store::open_in_memory().unwrap();
        let mut host = host_input("h1", "console", None);
        host.protocol = "serial".into();
        host.icon = "huawei".into();
        host.protocol_options = ProtocolOptions {
            serial_path: "/dev/tty.usbserial".into(),
            baud_rate: 9_600,
            ..ProtocolOptions::default()
        };
        store.create_host(&host).unwrap();

        let listed = store.list_hosts().unwrap();
        let stored = listed.iter().find(|h| h.id == "h1").unwrap();
        assert_eq!(stored.protocol, "serial");
        assert_eq!(stored.icon, "huawei");
        assert_eq!(stored.protocol_options.serial_path, "/dev/tty.usbserial");
        assert_eq!(stored.protocol_options.baud_rate, 9_600);
        assert_eq!(stored.protocol_options.data_bits, 8);
        assert_eq!(stored.protocol_options.parity, "none");

        let mut broken = host.clone();
        broken.protocol = "raw".into();
        broken.icon = "Bad Icon!".into();
        store.update_host(&broken).unwrap();
        let listed = store.list_hosts().unwrap();
        let stored = listed.iter().find(|h| h.id == "h1").unwrap();
        assert_eq!(stored.protocol, "ssh");
        assert_eq!(stored.icon, "");
    }

    #[test]
    fn migrate_is_idempotent() {
        let store = Store::open_in_memory().unwrap();
        store.migrate().unwrap();
        let conn = store.conn.lock().unwrap();
        let version: i64 = conn
            .pragma_query_value(None, "user_version", |r| r.get(0))
            .unwrap();
        assert_eq!(version, MIGRATIONS.len() as i64);
    }

    #[test]
    fn settings_roundtrip() {
        let store = Store::open_in_memory().unwrap();
        assert_eq!(store.get_setting("language").unwrap(), None);
        store.set_setting("language", "zh-CN").unwrap();
        store.set_setting("language", "en").unwrap();
        assert_eq!(store.get_setting("language").unwrap(), Some("en".into()));
    }

    #[test]
    fn command_library_roundtrip() {
        let store = Store::open_in_memory().unwrap();
        store.create_command_group("g1", "System").unwrap();
        store.create_command_group("g2", "Network").unwrap();
        store
            .create_command("c1", "g1", "Disk usage", "df -h")
            .unwrap();
        store
            .create_command("c2", "g1", "Memory", "free -m")
            .unwrap();
        store
            .create_command("c3", "g2", "Listeners", "ss -ltnp")
            .unwrap();

        let groups = store.list_command_groups().unwrap();
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].name, "System");
        assert_eq!(groups[0].commands.len(), 2);
        assert_eq!(groups[0].commands[0].name, "Disk usage");
        assert_eq!(groups[1].commands[0].template, "ss -ltnp");

        store.delete_command("c1").unwrap();
        store.delete_command_group("g2").unwrap();

        let imported = store
            .import_command_group(
                "plugin-ops",
                "Plugin",
                &[("cmd-1".into(), "Uptime".into(), "uptime".into())],
            )
            .unwrap();
        assert_eq!(imported, 1);
        let again = store
            .import_command_group(
                "plugin-ops",
                "Plugin",
                &[("cmd-1".into(), "Uptime".into(), "uptime".into())],
            )
            .unwrap();
        assert_eq!(again, 0);
        let groups = store.list_command_groups().unwrap();
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].commands.len(), 1);
        assert_eq!(groups[0].commands[0].name, "Memory");
        let plugin_group = groups
            .iter()
            .find(|group| group.id == "plugin-ops")
            .expect("imported group");
        assert_eq!(plugin_group.commands[0].name, "Uptime");
    }

    #[test]
    fn import_snapshot_merges_tags_and_skips_existing() {
        let store = Store::open_in_memory().unwrap();
        store
            .create_tag(&Tag {
                id: "local-t".into(),
                name: "web".into(),
                color: "blue".into(),
            })
            .unwrap();

        let tags = vec![Tag {
            id: "snap-t".into(),
            name: "web".into(),
            color: "blue".into(),
        }];
        let hosts = vec![
            Host {
                id: "f1".into(),
                name: "prod".into(),
                host: String::new(),
                port: 22,
                username: String::new(),
                auth_method: "password".into(),
                key_path: String::new(),
                key_id: String::new(),
                jump_host_id: String::new(),
                x11: false,
                parent_id: None,
                is_folder: true,
                production: false,
                protocol: "ssh".into(),
                protocol_options: ProtocolOptions::default(),
                icon: String::new(),
                sort_order: 0,
                notes: String::new(),
                tags: vec![],
            },
            Host {
                id: "h1".into(),
                name: "web-1".into(),
                host: "10.0.0.1".into(),
                port: 22,
                username: "root".into(),
                auth_method: "password".into(),
                key_path: String::new(),
                key_id: String::new(),
                jump_host_id: String::new(),
                x11: false,
                parent_id: Some("f1".into()),
                is_folder: false,
                production: true,
                protocol: "ssh".into(),
                protocol_options: ProtocolOptions::default(),
                icon: String::new(),
                sort_order: 0,
                notes: String::new(),
                tags: vec![tags[0].clone()],
            },
        ];

        let stats = store.import_snapshot(&hosts, &tags).unwrap();
        assert_eq!(stats.hosts_added, 2);
        assert_eq!(stats.hosts_skipped, 0);
        assert_eq!(stats.tags_added, 0);

        let stored = store.list_hosts().unwrap();
        let web = stored.iter().find(|h| h.id == "h1").unwrap();
        assert_eq!(web.tags[0].id, "local-t");

        let stats = store.import_snapshot(&hosts, &tags).unwrap();
        assert_eq!(stats.hosts_added, 0);
        assert_eq!(stats.hosts_skipped, 2);
    }

    #[test]
    fn host_tree_and_tags_roundtrip() {
        let store = Store::open_in_memory().unwrap();
        store
            .create_host(&folder_input("f1", "prod", None))
            .unwrap();

        let tag = Tag {
            id: "t1".into(),
            name: "web".into(),
            color: "blue".into(),
        };
        store.create_tag(&tag).unwrap();

        let mut host = host_input("h1", "web-1", Some("f1"));
        host.production = true;
        host.notes = "primary".into();
        host.auth_method = "key".into();
        host.key_path = "~/.ssh/id_ed25519".into();
        host.tag_ids = vec!["t1".into()];
        store.create_host(&host).unwrap();

        let listed = store.list_hosts().unwrap();
        assert_eq!(listed.len(), 2);
        let folder = listed.iter().find(|h| h.id == "f1").unwrap();
        assert!(folder.is_folder);
        assert_eq!(folder.parent_id, None);

        let stored = listed.iter().find(|h| h.id == "h1").unwrap();
        assert_eq!(stored.parent_id.as_deref(), Some("f1"));
        assert!(stored.production);
        assert_eq!(stored.notes, "primary");
        assert_eq!(stored.auth_method, "key");
        assert_eq!(stored.key_path, "~/.ssh/id_ed25519");
        assert_eq!(stored.tags, vec![tag]);
    }

    #[test]
    fn update_moves_host_and_replaces_tags() {
        let store = Store::open_in_memory().unwrap();
        store.create_host(&folder_input("a", "a", None)).unwrap();
        store.create_host(&folder_input("b", "b", None)).unwrap();
        store
            .create_host(&host_input("h1", "host", Some("a")))
            .unwrap();
        store
            .create_tag(&Tag {
                id: "t1".into(),
                name: "one".into(),
                color: "red".into(),
            })
            .unwrap();
        store
            .create_tag(&Tag {
                id: "t2".into(),
                name: "two".into(),
                color: "green".into(),
            })
            .unwrap();

        let mut update = host_input("h1", "host renamed", Some("b"));
        update.tag_ids = vec!["t2".into()];
        store.update_host(&update).unwrap();

        let listed = store.list_hosts().unwrap();
        let stored = listed.iter().find(|h| h.id == "h1").unwrap();
        assert_eq!(stored.name, "host renamed");
        assert_eq!(stored.parent_id.as_deref(), Some("b"));
        assert_eq!(stored.tags.len(), 1);
        assert_eq!(stored.tags[0].id, "t2");
    }

    #[test]
    fn delete_folder_cascades_children() {
        let store = Store::open_in_memory().unwrap();
        store
            .create_host(&folder_input("f1", "group", None))
            .unwrap();
        store
            .create_host(&host_input("h1", "child", Some("f1")))
            .unwrap();
        store
            .create_tag(&Tag {
                id: "t1".into(),
                name: "tag".into(),
                color: "cyan".into(),
            })
            .unwrap();
        let mut host = host_input("h1", "child", Some("f1"));
        host.tag_ids = vec!["t1".into()];
        store.update_host(&host).unwrap();

        store.delete_host("f1").unwrap();
        assert!(store.list_hosts().unwrap().is_empty());
        let conn = store.conn.lock().unwrap();
        let links: i64 = conn
            .query_row("SELECT COUNT(*) FROM host_tags", [], |row| row.get(0))
            .unwrap();
        assert_eq!(links, 0);
    }

    #[test]
    fn rejects_folder_cycle() {
        let store = Store::open_in_memory().unwrap();
        store.create_host(&folder_input("a", "a", None)).unwrap();
        store
            .create_host(&folder_input("b", "b", Some("a")))
            .unwrap();

        let mut move_a = folder_input("a", "a", Some("b"));
        move_a.tag_ids.clear();
        let err = store.update_host(&move_a).unwrap_err();
        assert!(matches!(err, StoreError::Cycle));
    }

    #[test]
    fn rejects_non_folder_parent() {
        let store = Store::open_in_memory().unwrap();
        store.create_host(&host_input("h1", "host", None)).unwrap();
        let err = store.create_host(&host_input("h2", "child", Some("h1")));
        assert!(matches!(err, Err(StoreError::ParentNotFolder)));
    }

    #[test]
    fn transfer_jobs_round_trip() {
        let store = Store::open_in_memory().unwrap();
        store.save_transfer_job("a", "{\"id\":\"a\"}").unwrap();
        store.save_transfer_job("b", "{\"id\":\"b\"}").unwrap();
        // Saving the same id again must replace, not duplicate.
        store
            .save_transfer_job("a", "{\"id\":\"a\",\"x\":1}")
            .unwrap();
        assert_eq!(
            store.list_transfer_jobs().unwrap(),
            vec![
                "{\"id\":\"a\",\"x\":1}".to_string(),
                "{\"id\":\"b\"}".to_string()
            ]
        );
        store.delete_transfer_job("a").unwrap();
        assert_eq!(
            store.list_transfer_jobs().unwrap(),
            vec!["{\"id\":\"b\"}".to_string()]
        );
    }

    #[test]
    fn tag_crud() {
        let store = Store::open_in_memory().unwrap();
        let mut tag = Tag {
            id: "t1".into(),
            name: "web".into(),
            color: "blue".into(),
        };
        store.create_tag(&tag).unwrap();
        tag.name = "web servers".into();
        tag.color = "amber".into();
        store.update_tag(&tag).unwrap();
        assert_eq!(store.list_tags().unwrap(), vec![tag]);
        store.delete_tag("t1").unwrap();
        assert!(store.list_tags().unwrap().is_empty());
    }

    #[test]
    fn managed_keys_vault_metadata_and_audit() {
        let store = Store::open_in_memory().unwrap();
        let key = SshKey {
            id: "k1".into(),
            name: "work".into(),
            algorithm: "ssh-ed25519".into(),
            fingerprint: "SHA256:abc".into(),
            public_key: "ssh-ed25519 AAAA work".into(),
            comment: "work".into(),
            source_path: "~/.ssh/id_ed25519".into(),
            has_passphrase: false,
            created_at: String::new(),
            updated_at: String::new(),
        };
        store.save_ssh_key(&key).unwrap();
        let listed = store.list_ssh_keys().unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].name, "work");
        store.delete_ssh_key("k1").unwrap();
        assert!(store.list_ssh_keys().unwrap().is_empty());

        let host = host_input("h1", "web", None);
        store.create_host(&host).unwrap();
        store
            .upsert_vault_entry("pwd:h1", "password", "web", Some("h1"))
            .unwrap();
        store
            .upsert_vault_entry("key:k1", "key", "work", None)
            .unwrap();
        let entries = store.list_vault_entries().unwrap();
        assert_eq!(entries.len(), 2);
        assert!(entries.iter().any(|entry| entry.secret_ref == "key:k1"));
        store.delete_vault_entry("key:k1").unwrap();
        assert_eq!(store.list_vault_entries().unwrap().len(), 1);

        store.append_audit("read", "pwd:h1", "connect").unwrap();
        store.append_audit("write", "pwd:h1", "saved").unwrap();
        let audit = store.list_audit(10).unwrap();
        assert_eq!(audit.len(), 2);
        assert_eq!(audit[0].action, "write");

        // Deleting the host removes its metadata through the trigger.
        store.delete_host("h1").unwrap();
        assert!(store.list_vault_entries().unwrap().is_empty());
    }
}
