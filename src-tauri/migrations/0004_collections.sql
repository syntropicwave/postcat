-- Collections (folders + saved requests), environments and variables.

CREATE TABLE collections (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    name        TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    sort_order  INTEGER NOT NULL DEFAULT 0,
    created_at  TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
) STRICT;

-- Tree of folders and requests inside a collection. parent_id NULL = root.
CREATE TABLE collection_items (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    collection_id INTEGER NOT NULL REFERENCES collections(id) ON DELETE CASCADE,
    parent_id     INTEGER REFERENCES collection_items(id) ON DELETE CASCADE,
    kind          TEXT NOT NULL CHECK (kind IN ('folder', 'request')),
    name          TEXT NOT NULL,
    description   TEXT NOT NULL DEFAULT '',
    sort_order    INTEGER NOT NULL DEFAULT 0,
    -- JSON RequestSpec, present for kind = 'request'.
    req_spec      TEXT
) STRICT;

CREATE INDEX idx_items_collection ON collection_items (collection_id, parent_id, sort_order);

CREATE TABLE environments (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    name       TEXT NOT NULL,
    is_active  INTEGER NOT NULL DEFAULT 0,
    sort_order INTEGER NOT NULL DEFAULT 0
) STRICT;

-- Variables in three scopes. owner_id references environments.id or
-- collections.id depending on scope (NULL for global). current_value is the
-- local override — with future sync/export it never leaves this machine;
-- initial_value is what gets shared.
CREATE TABLE variables (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    scope         TEXT NOT NULL CHECK (scope IN ('global', 'environment', 'collection')),
    owner_id      INTEGER,
    key           TEXT NOT NULL,
    initial_value TEXT NOT NULL DEFAULT '',
    current_value TEXT,
    is_secret     INTEGER NOT NULL DEFAULT 0,
    enabled       INTEGER NOT NULL DEFAULT 1,
    sort_order    INTEGER NOT NULL DEFAULT 0
) STRICT;

CREATE INDEX idx_variables_scope ON variables (scope, owner_id);
