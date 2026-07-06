-- Per-phase timing waterfall for each history entry. NULL where a phase
-- wasn't measured (pooled connection or the reqwest fallback path).
ALTER TABLE history_entries ADD COLUMN dns_ms REAL;
ALTER TABLE history_entries ADD COLUMN connect_ms REAL;
ALTER TABLE history_entries ADD COLUMN tls_ms REAL;
ALTER TABLE history_entries ADD COLUMN server_ms REAL;
ALTER TABLE history_entries ADD COLUMN download_ms REAL;
ALTER TABLE history_entries ADD COLUMN redirects INTEGER NOT NULL DEFAULT 0;
