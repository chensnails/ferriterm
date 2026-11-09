-- Managed SSH keys, credential metadata and access audit.
-- Private key material lives in the ft-security vault; this table only keeps
-- the public half and the vault reference used to look up the secret.

CREATE TABLE ssh_keys (
  id             TEXT PRIMARY KEY,
  name           TEXT NOT NULL,
  algorithm      TEXT NOT NULL,
  fingerprint    TEXT NOT NULL,
  public_key     TEXT NOT NULL,
  comment        TEXT NOT NULL DEFAULT '',
  source_path    TEXT NOT NULL DEFAULT '',
  has_passphrase INTEGER NOT NULL DEFAULT 0,
  created_at     TEXT NOT NULL DEFAULT (datetime('now')),
  updated_at     TEXT NOT NULL DEFAULT (datetime('now'))
);

ALTER TABLE hosts ADD COLUMN key_id TEXT NOT NULL DEFAULT '';

CREATE TABLE vault_entries (
  secret_ref TEXT PRIMARY KEY,
  kind       TEXT NOT NULL,
  label      TEXT NOT NULL,
  host_id    TEXT,
  updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX idx_vault_entries_host ON vault_entries(host_id);

CREATE TABLE credential_audit (
  id     INTEGER PRIMARY KEY AUTOINCREMENT,
  at     TEXT NOT NULL DEFAULT (datetime('now')),
  action TEXT NOT NULL,
  target TEXT NOT NULL,
  detail TEXT NOT NULL DEFAULT ''
);

CREATE INDEX idx_credential_audit_at ON credential_audit(at);

CREATE TRIGGER trg_hosts_vault_entries AFTER DELETE ON hosts
BEGIN
  DELETE FROM vault_entries WHERE host_id = OLD.id;
END;
