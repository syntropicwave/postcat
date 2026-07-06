-- Saved host aliases: a short, coloured label shown in place of a host
-- (e.g. "api.example.com" -> "prod") across the URL bar, history and tabs.

CREATE TABLE host_aliases (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    -- Host key including port when present, lowercased ("api.example.com:8080").
    host       TEXT NOT NULL UNIQUE,
    alias      TEXT NOT NULL,
    -- Hex colour ("#7c5cff") or "" for the default accent.
    color      TEXT NOT NULL DEFAULT '',
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
) STRICT;
