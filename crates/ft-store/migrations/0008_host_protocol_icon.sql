-- Hosts carry a transport protocol and an optional display icon so the tree can
-- hold SSH, SFTP, telnet, serial and local shells side by side. Per-protocol
-- extras live in one JSON column, which keeps future protocols to a value
-- change instead of another migration.

ALTER TABLE hosts ADD COLUMN protocol TEXT NOT NULL DEFAULT 'ssh';
ALTER TABLE hosts ADD COLUMN protocol_options TEXT NOT NULL DEFAULT '{}';
ALTER TABLE hosts ADD COLUMN icon TEXT NOT NULL DEFAULT '';
