-- Managed SSH keys gain user notes and an optional expiry date. Both are
-- metadata only: the private key stays in the encrypted vault, so this
-- migration never touches secrets and needs no re-encryption.
ALTER TABLE ssh_keys ADD COLUMN notes TEXT NOT NULL DEFAULT '';
ALTER TABLE ssh_keys ADD COLUMN expires_at TEXT NOT NULL DEFAULT '';
