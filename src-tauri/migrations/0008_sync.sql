-- Sync bookkeeping on the two top-level sync units (collections and
-- environments). Each syncs as one self-contained encrypted blob keyed by a
-- stable uid. Changes are detected by comparing the content hash at sync
-- time, so ordinary mutations need no instrumentation. Deletes leave a
-- tombstone (deleted = 1) so the removal propagates to other devices.

ALTER TABLE collections ADD COLUMN uid TEXT;
ALTER TABLE collections ADD COLUMN sync_rev INTEGER NOT NULL DEFAULT 0;
ALTER TABLE collections ADD COLUMN synced_hash TEXT;
ALTER TABLE collections ADD COLUMN deleted INTEGER NOT NULL DEFAULT 0;
ALTER TABLE collections ADD COLUMN updated_at TEXT;

ALTER TABLE environments ADD COLUMN uid TEXT;
ALTER TABLE environments ADD COLUMN sync_rev INTEGER NOT NULL DEFAULT 0;
ALTER TABLE environments ADD COLUMN synced_hash TEXT;
ALTER TABLE environments ADD COLUMN deleted INTEGER NOT NULL DEFAULT 0;
ALTER TABLE environments ADD COLUMN updated_at TEXT;

-- Backfill uids for rows that predate sync (hex of randomblob).
UPDATE collections SET uid = lower(hex(randomblob(16))) WHERE uid IS NULL;
UPDATE environments SET uid = lower(hex(randomblob(16))) WHERE uid IS NULL;

CREATE UNIQUE INDEX idx_collections_uid ON collections (uid);
CREATE UNIQUE INDEX idx_environments_uid ON environments (uid);
