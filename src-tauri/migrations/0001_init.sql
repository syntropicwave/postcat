-- Schema bootstrap. Real domain tables (history, collections, environments)
-- arrive in phase 1+ as separate migrations.
CREATE TABLE meta (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
) STRICT;

INSERT INTO meta (key, value) VALUES ('schema_created_at', strftime('%Y-%m-%dT%H:%M:%fZ', 'now'));
