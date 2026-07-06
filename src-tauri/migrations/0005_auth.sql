-- Auth configs (JSON AuthSpec) on collections and items (folders inherit
-- down the tree; requests with auth = inherit walk up).
ALTER TABLE collections ADD COLUMN auth TEXT;
ALTER TABLE collection_items ADD COLUMN auth TEXT;
