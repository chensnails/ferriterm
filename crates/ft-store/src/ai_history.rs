//! Persistence for the built-in assistant's conversations.
//!
//! History lives in SQLite rather than a settings blob: a conversation is
//! append-only and can grow to thousands of messages, and the assistant wants
//! to list sessions by recency without parsing every transcript. The provider
//! and model used are recorded on the session so an old conversation keeps its
//! provenance even after the provider is edited.

use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::Store;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiSession {
    pub id: String,
    pub title: String,
    pub provider_id: String,
    pub model: String,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiMessageRow {
    pub role: String,
    pub content: String,
    pub thinking: String,
    pub created_at: i64,
}

fn now_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|value| value.as_millis() as i64)
        .unwrap_or_default()
}

/// Session ids only have to be unique on this machine; a millisecond stamp plus
/// the process-local sequence keeps two quick "new chat" clicks apart.
fn new_id() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static SEQ: AtomicU64 = AtomicU64::new(0);
    format!("s{}-{}", now_millis(), SEQ.fetch_add(1, Ordering::Relaxed))
}

impl Store {
    /// Newest first — the list the panel shows.
    pub fn ai_sessions(&self) -> rusqlite::Result<Vec<AiSession>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, title, provider_id, model, created_at, updated_at
               FROM ai_sessions
              ORDER BY updated_at DESC, id DESC",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(AiSession {
                id: row.get(0)?,
                title: row.get(1)?,
                provider_id: row.get(2)?,
                model: row.get(3)?,
                created_at: row.get(4)?,
                updated_at: row.get(5)?,
            })
        })?;
        rows.collect()
    }

    pub fn ai_session(&self, id: &str) -> rusqlite::Result<Option<AiSession>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, title, provider_id, model, created_at, updated_at
               FROM ai_sessions WHERE id = ?1",
        )?;
        let mut rows = stmt.query([id])?;
        match rows.next()? {
            Some(row) => Ok(Some(AiSession {
                id: row.get(0)?,
                title: row.get(1)?,
                provider_id: row.get(2)?,
                model: row.get(3)?,
                created_at: row.get(4)?,
                updated_at: row.get(5)?,
            })),
            None => Ok(None),
        }
    }

    pub fn ai_session_create(
        &self,
        title: &str,
        provider_id: &str,
        model: &str,
    ) -> rusqlite::Result<AiSession> {
        let session = AiSession {
            id: new_id(),
            title: title.trim().to_string(),
            provider_id: provider_id.to_string(),
            model: model.to_string(),
            created_at: now_millis(),
            updated_at: now_millis(),
        };
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO ai_sessions(id, title, provider_id, model, created_at, updated_at)
             VALUES(?1, ?2, ?3, ?4, ?5, ?6)",
            rusqlite::params![
                session.id,
                session.title,
                session.provider_id,
                session.model,
                session.created_at,
                session.updated_at
            ],
        )?;
        Ok(session)
    }

    pub fn ai_session_messages(&self, session_id: &str) -> rusqlite::Result<Vec<AiMessageRow>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT role, content, thinking, created_at
               FROM ai_messages WHERE session_id = ?1 ORDER BY id",
        )?;
        let rows = stmt.query_map([session_id], |row| {
            Ok(AiMessageRow {
                role: row.get(0)?,
                content: row.get(1)?,
                thinking: row.get(2)?,
                created_at: row.get(3)?,
            })
        })?;
        rows.collect()
    }

    /// Append one message and bump the session's recency. Returns `None` when
    /// the session vanished (deleted in another window), so the caller can stop
    /// writing instead of inserting orphans.
    pub fn ai_session_append(
        &self,
        session_id: &str,
        role: &str,
        content: &str,
        thinking: &str,
    ) -> rusqlite::Result<Option<AiSession>> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        let exists: Option<String> = tx
            .query_row("SELECT id FROM ai_sessions WHERE id = ?1", [session_id], |row| {
                row.get(0)
            })
            .ok();
        if exists.is_none() {
            return Ok(None);
        }
        let stamp = now_millis();
        tx.execute(
            "INSERT INTO ai_messages(session_id, role, content, thinking, created_at)
             VALUES(?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![session_id, role, content, thinking, stamp],
        )?;
        tx.execute(
            "UPDATE ai_sessions SET updated_at = ?2 WHERE id = ?1",
            rusqlite::params![session_id, stamp],
        )?;
        tx.commit()?;
        drop(conn);
        self.ai_session(session_id)
    }

    pub fn ai_session_set_title(&self, id: &str, title: &str) -> rusqlite::Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE ai_sessions SET title = ?2 WHERE id = ?1",
            rusqlite::params![id, title.trim()],
        )?;
        Ok(())
    }

    /// Messages go with the session through `ON DELETE CASCADE`.
    pub fn ai_session_delete(&self, id: &str) -> rusqlite::Result<bool> {
        let conn = self.conn.lock().unwrap();
        let removed = conn.execute("DELETE FROM ai_sessions WHERE id = ?1", [id])?;
        Ok(removed > 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sessions_round_trip_and_cascade() {
        let store = Store::open_in_memory().unwrap();
        let session = store
            .ai_session_create("first chat", "p1", "gpt-x")
            .unwrap();
        assert_eq!(session.title, "first chat");
        store
            .ai_session_append(&session.id, "user", "hello", "")
            .unwrap()
            .expect("session exists");
        store
            .ai_session_append(&session.id, "assistant", "hi", "reasoned")
            .unwrap();

        let messages = store.ai_session_messages(&session.id).unwrap();
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[1].thinking, "reasoned");
        assert_eq!(store.ai_sessions().unwrap().len(), 1);

        store.ai_session_set_title(&session.id, "renamed").unwrap();
        assert_eq!(store.ai_session(&session.id).unwrap().unwrap().title, "renamed");

        assert!(store.ai_session_delete(&session.id).unwrap());
        assert!(store.ai_session_messages(&session.id).unwrap().is_empty());
        assert!(store.ai_session(&session.id).unwrap().is_none());
    }

    #[test]
    fn append_to_missing_session_is_a_no_op() {
        let store = Store::open_in_memory().unwrap();
        assert!(store
            .ai_session_append("nope", "user", "hello", "")
            .unwrap()
            .is_none());
    }
}
