-- Initial schema: host tree, tags, credential references, settings.
-- Credential secrets never live here; credentials_ref holds opaque pointers
-- into the encrypted local vault (ft-security, P1+).

CREATE TABLE hosts (
  id             TEXT PRIMARY KEY,
  name           TEXT NOT NULL,
  host           TEXT NOT NULL,
  port           INTEGER NOT NULL DEFAULT 22,
  username       TEXT NOT NULL DEFAULT '',
  auth_method    TEXT NOT NULL DEFAULT 'password',
  credential_ref TEXT REFERENCES credentials_ref(id) ON DELETE SET NULL,
  parent_id      TEXT REFERENCES hosts(id) ON DELETE CASCADE,
  is_folder      INTEGER NOT NULL DEFAULT 0,
  production     INTEGER NOT NULL DEFAULT 0,
  sort_order     INTEGER NOT NULL DEFAULT 0,
  created_at     TEXT NOT NULL DEFAULT (datetime('now')),
  updated_at     TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE tags (
  id    TEXT PRIMARY KEY,
  name  TEXT NOT NULL UNIQUE,
  color TEXT NOT NULL
);

CREATE TABLE host_tags (
  host_id TEXT NOT NULL REFERENCES hosts(id) ON DELETE CASCADE,
  tag_id  TEXT NOT NULL REFERENCES tags(id) ON DELETE CASCADE,
  PRIMARY KEY (host_id, tag_id)
);

CREATE TABLE credentials_ref (
  id         TEXT PRIMARY KEY,
  kind       TEXT NOT NULL,
  label      TEXT NOT NULL,
  secret_ref TEXT NOT NULL
);

CREATE TABLE settings (
  key   TEXT PRIMARY KEY,
  value TEXT NOT NULL
);

CREATE INDEX idx_hosts_parent ON hosts(parent_id);