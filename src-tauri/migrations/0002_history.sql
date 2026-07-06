-- Request history: append-only log of every sent request, response included.
-- req_spec holds the full serialized RequestSpec so any entry can be reopened
-- and resent exactly; the flat columns exist for listing and filtering.
CREATE TABLE history_entries (
    id                  INTEGER PRIMARY KEY AUTOINCREMENT,
    sent_at             TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    method              TEXT NOT NULL,
    url                 TEXT NOT NULL,
    host                TEXT NOT NULL DEFAULT '',
    req_spec            TEXT NOT NULL,
    req_headers         TEXT NOT NULL DEFAULT '[]',
    req_body_text       TEXT,
    status              INTEGER,
    status_text         TEXT,
    error               TEXT,
    http_version        TEXT,
    resp_headers        TEXT,
    resp_body           BLOB,
    resp_body_truncated INTEGER NOT NULL DEFAULT 0,
    resp_size           INTEGER,
    duration_ms         REAL,
    ttfb_ms             REAL
) STRICT;

CREATE INDEX idx_history_sent_at ON history_entries (sent_at DESC);
CREATE INDEX idx_history_method ON history_entries (method);
CREATE INDEX idx_history_host ON history_entries (host);
