use std::path::Path;
use std::sync::Mutex;

use rusqlite::Connection;
use serde::{Deserialize, Serialize};

const MIGRATIONS: &[&str] = &[include_str!("../migrations/0001_init.sql")];

pub struct Store {
    conn: Mutex<Connection>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Host {
    pub id: String,
    pub name: String,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub production: bool,
}

impl Store {
    pub fn open(path: &Path) -> rusqlite::Result<Self> {
        Self::from_connection(Connection::open(path)?)
    }

    pub fn open_in_memory() -> rusqlite::Result<Self> {
        Self::from_connection(Connection::open_in_memory()?)
    }

    fn from_connection(conn: Connection) -> rusqlite::Result<Self> {
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

    pub fn insert_host(&self, host: &Host) -> rusqlite::Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO hosts(id, name, host, port, username, production)
             VALUES(?1, ?2, ?3, ?4, ?5, ?6)",
            rusqlite::params![
                host.id,
                host.name,
                host.host,
                host.port,
                host.username,
                host.production,
            ],
        )?;
        Ok(())
    }

    pub fn list_hosts(&self) -> rusqlite::Result<Vec<Host>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, name, host, port, username, production FROM hosts
             WHERE is_folder = 0 ORDER BY sort_order, name",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(Host {
                id: row.get(0)?,
                name: row.get(1)?,
                host: row.get(2)?,
                port: row.get(3)?,
                username: row.get(4)?,
                production: row.get(5)?,
            })
        })?;
        rows.collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn host_roundtrip() {
        let store = Store::open_in_memory().unwrap();
        let host = Host {
            id: "h1".into(),
            name: "router".into(),
            host: "192.168.1.1".into(),
            port: 22,
            username: "root".into(),
            production: true,
        };
        store.insert_host(&host).unwrap();
        let listed = store.list_hosts().unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].id, "h1");
        assert!(listed[0].production);
    }
}
