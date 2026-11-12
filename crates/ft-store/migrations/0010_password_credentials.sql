-- Named password credentials. The plaintext password lives in the vault under
-- `pwcred:{id}`; this table holds the operator-facing metadata and the host
-- link so a host reuses the credential instead of asking again.

CREATE TABLE password_credentials (
  id         TEXT PRIMARY KEY,
  name       TEXT NOT NULL,
  username   TEXT NOT NULL DEFAULT '',
  notes      TEXT NOT NULL DEFAULT '',
  created_at TEXT NOT NULL DEFAULT (datetime('now')),
  updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

ALTER TABLE hosts ADD COLUMN password_id TEXT NOT NULL DEFAULT '';
