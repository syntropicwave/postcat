-- Structured failure detail so a history entry can rebuild the error
-- pipeline (which stage broke) and hint when reopened. NULL for successes
-- and for entries recorded before this migration.
ALTER TABLE history_entries ADD COLUMN error_phase TEXT;
ALTER TABLE history_entries ADD COLUMN error_hint TEXT;
