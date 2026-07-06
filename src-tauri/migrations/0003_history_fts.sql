-- History becomes searchable: FTS5 full-text index over URL, headers and
-- bodies, plus a trigram index for substring matches in URLs. New columns:
-- pinned/label (user curation, exempt from retention) and resp_body_text
-- (capped text extraction of the response body that feeds the index).

ALTER TABLE history_entries ADD COLUMN pinned INTEGER NOT NULL DEFAULT 0;
ALTER TABLE history_entries ADD COLUMN label TEXT;
ALTER TABLE history_entries ADD COLUMN resp_body_text TEXT;

-- Endpoint identity: URL without the query string. VIRTUAL because SQLite
-- cannot add STORED generated columns via ALTER TABLE.
ALTER TABLE history_entries ADD COLUMN url_base TEXT
    GENERATED ALWAYS AS (
        CASE WHEN instr(url, '?') > 0 THEN substr(url, 1, instr(url, '?') - 1) ELSE url END
    ) VIRTUAL;

CREATE INDEX idx_history_endpoint ON history_entries (method, url_base);
CREATE INDEX idx_history_pinned ON history_entries (pinned) WHERE pinned = 1;

-- Backfill text extraction for pre-FTS rows (best-effort byte cast).
UPDATE history_entries
SET resp_body_text = CAST(substr(resp_body, 1, 262144) AS TEXT)
WHERE resp_body IS NOT NULL;

-- Main full-text index (external content: text is stored once, in
-- history_entries; the index reads it from there).
CREATE VIRTUAL TABLE history_fts USING fts5(
    url,
    req_headers,
    req_body_text,
    resp_body_text,
    label,
    content='history_entries',
    content_rowid='id'
);

-- Trigram index over URLs: substring search ("ample" finds "example.com").
CREATE VIRTUAL TABLE history_url_trgm USING fts5(
    url,
    content='history_entries',
    content_rowid='id',
    tokenize='trigram'
);

CREATE TRIGGER history_fts_ai AFTER INSERT ON history_entries BEGIN
    INSERT INTO history_fts(rowid, url, req_headers, req_body_text, resp_body_text, label)
    VALUES (new.id, new.url, new.req_headers, new.req_body_text, new.resp_body_text, new.label);
    INSERT INTO history_url_trgm(rowid, url) VALUES (new.id, new.url);
END;

CREATE TRIGGER history_fts_ad AFTER DELETE ON history_entries BEGIN
    INSERT INTO history_fts(history_fts, rowid, url, req_headers, req_body_text, resp_body_text, label)
    VALUES ('delete', old.id, old.url, old.req_headers, old.req_body_text, old.resp_body_text, old.label);
    INSERT INTO history_url_trgm(history_url_trgm, rowid, url) VALUES ('delete', old.id, old.url);
END;

CREATE TRIGGER history_fts_au AFTER UPDATE ON history_entries BEGIN
    INSERT INTO history_fts(history_fts, rowid, url, req_headers, req_body_text, resp_body_text, label)
    VALUES ('delete', old.id, old.url, old.req_headers, old.req_body_text, old.resp_body_text, old.label);
    INSERT INTO history_fts(rowid, url, req_headers, req_body_text, resp_body_text, label)
    VALUES (new.id, new.url, new.req_headers, new.req_body_text, new.resp_body_text, new.label);
    INSERT INTO history_url_trgm(history_url_trgm, rowid, url) VALUES ('delete', old.id, old.url);
    INSERT INTO history_url_trgm(rowid, url) VALUES (new.id, new.url);
END;

-- Index existing rows.
INSERT INTO history_fts(rowid, url, req_headers, req_body_text, resp_body_text, label)
SELECT id, url, req_headers, req_body_text, resp_body_text, label FROM history_entries;

INSERT INTO history_url_trgm(rowid, url) SELECT id, url FROM history_entries;
