-- Host notes for the properties dialog. Credential secrets never live here.

ALTER TABLE hosts ADD COLUMN notes TEXT NOT NULL DEFAULT '';
