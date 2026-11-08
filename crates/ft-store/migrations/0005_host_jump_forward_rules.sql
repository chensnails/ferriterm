ALTER TABLE hosts ADD COLUMN jump_host_id TEXT NOT NULL DEFAULT '';
ALTER TABLE hosts ADD COLUMN x11 INTEGER NOT NULL DEFAULT 0;

CREATE TABLE forward_rules (
  id        TEXT PRIMARY KEY,
  host_id   TEXT NOT NULL REFERENCES hosts(id) ON DELETE CASCADE,
  kind      TEXT NOT NULL DEFAULT 'local',
  bind_host TEXT NOT NULL DEFAULT '127.0.0.1',
  bind_port INTEGER NOT NULL,
  dest_host TEXT NOT NULL DEFAULT '',
  dest_port INTEGER NOT NULL DEFAULT 0
);

CREATE INDEX idx_forward_rules_host ON forward_rules(host_id);
