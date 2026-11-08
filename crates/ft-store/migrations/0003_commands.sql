-- Quick command library: groups of reusable command snippets.

CREATE TABLE command_groups (
  id         TEXT PRIMARY KEY,
  name       TEXT NOT NULL,
  sort_order INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE commands (
  id         TEXT PRIMARY KEY,
  group_id   TEXT NOT NULL REFERENCES command_groups(id) ON DELETE CASCADE,
  name       TEXT NOT NULL,
  template   TEXT NOT NULL,
  sort_order INTEGER NOT NULL DEFAULT 0
);

CREATE INDEX idx_commands_group ON commands(group_id);
