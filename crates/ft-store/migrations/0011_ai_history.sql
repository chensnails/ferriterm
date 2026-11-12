-- Assistant conversations. One row per chat, one row per message; the model's
-- reasoning is stored next to its answer so reopening a session restores the
-- thinking panel exactly as it was streamed.

CREATE TABLE ai_sessions (
  id          TEXT PRIMARY KEY,
  title       TEXT NOT NULL DEFAULT '',
  provider_id TEXT NOT NULL DEFAULT '',
  model       TEXT NOT NULL DEFAULT '',
  created_at  INTEGER NOT NULL,
  updated_at  INTEGER NOT NULL
);

CREATE TABLE ai_messages (
  id         INTEGER PRIMARY KEY AUTOINCREMENT,
  session_id TEXT NOT NULL REFERENCES ai_sessions(id) ON DELETE CASCADE,
  role       TEXT NOT NULL,
  content    TEXT NOT NULL DEFAULT '',
  thinking   TEXT NOT NULL DEFAULT '',
  created_at INTEGER NOT NULL
);

CREATE INDEX ai_messages_session ON ai_messages(session_id, id);
CREATE INDEX ai_sessions_updated ON ai_sessions(updated_at DESC);
