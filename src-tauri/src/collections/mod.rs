use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::http_engine::RequestSpec;
use crate::store::{Store, StoreError};

#[derive(Debug, Serialize)]
pub struct Collection {
    pub id: i64,
    pub name: String,
    pub description: String,
    pub sort_order: i64,
}

#[derive(Debug, Serialize)]
pub struct CollectionItem {
    pub id: i64,
    pub collection_id: i64,
    pub parent_id: Option<i64>,
    pub kind: String, // "folder" | "request"
    pub name: String,
    pub description: String,
    pub sort_order: i64,
    pub req_spec: Option<serde_json::Value>,
    pub pre_request_script: Option<String>,
    pub test_script: Option<String>,
}

pub fn list(store: &Store) -> Result<Vec<Collection>, StoreError> {
    store.with_conn(|conn| {
        let mut stmt = conn.prepare_cached(
            "SELECT id, name, description, sort_order FROM collections
             ORDER BY sort_order, id",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(Collection {
                id: row.get(0)?,
                name: row.get(1)?,
                description: row.get(2)?,
                sort_order: row.get(3)?,
            })
        })?;
        rows.collect()
    })
}

pub fn create(store: &Store, name: &str) -> Result<i64, StoreError> {
    store.with_conn(|conn| {
        conn.execute(
            "INSERT INTO collections (name, sort_order)
             VALUES (?1, (SELECT coalesce(max(sort_order), 0) + 1 FROM collections))",
            params![name],
        )?;
        Ok(conn.last_insert_rowid())
    })
}

pub fn update(
    store: &Store,
    id: i64,
    name: Option<String>,
    description: Option<String>,
) -> Result<(), StoreError> {
    store.with_conn(|conn| {
        conn.execute(
            "UPDATE collections SET
                name = coalesce(?2, name),
                description = coalesce(?3, description)
             WHERE id = ?1",
            params![id, name, description],
        )?;
        Ok(())
    })
}

pub fn delete(store: &Store, id: i64) -> Result<(), StoreError> {
    store.with_conn(|conn| {
        conn.execute("DELETE FROM collections WHERE id = ?1", params![id])?;
        // Collection-scoped variables have no FK (owner_id is polymorphic).
        conn.execute(
            "DELETE FROM variables WHERE scope = 'collection' AND owner_id = ?1",
            params![id],
        )?;
        Ok(())
    })
}

/// All items of a collection, ordered for tree assembly on the frontend.
pub fn items(store: &Store, collection_id: i64) -> Result<Vec<CollectionItem>, StoreError> {
    store.with_conn(|conn| {
        let mut stmt = conn.prepare_cached(
            "SELECT id, collection_id, parent_id, kind, name, description, sort_order, req_spec,
                    pre_request_script, test_script
             FROM collection_items WHERE collection_id = ?1
             ORDER BY parent_id NULLS FIRST, sort_order, id",
        )?;
        let rows = stmt.query_map(params![collection_id], |row| {
            let spec: Option<String> = row.get(7)?;
            Ok(CollectionItem {
                id: row.get(0)?,
                collection_id: row.get(1)?,
                parent_id: row.get(2)?,
                kind: row.get(3)?,
                name: row.get(4)?,
                description: row.get(5)?,
                sort_order: row.get(6)?,
                req_spec: spec.and_then(|s| serde_json::from_str(&s).ok()),
                pre_request_script: row.get(8)?,
                test_script: row.get(9)?,
            })
        })?;
        rows.collect()
    })
}

pub fn item_create(
    store: &Store,
    collection_id: i64,
    parent_id: Option<i64>,
    kind: &str,
    name: &str,
    spec: Option<&RequestSpec>,
) -> Result<i64, StoreError> {
    let spec_json = spec.map(|s| serde_json::to_string(s).unwrap_or_else(|_| "{}".into()));
    store.with_conn(|conn| {
        conn.execute(
            "INSERT INTO collection_items (collection_id, parent_id, kind, name, req_spec, sort_order)
             VALUES (?1, ?2, ?3, ?4, ?5,
                     (SELECT coalesce(max(sort_order), 0) + 1 FROM collection_items
                      WHERE collection_id = ?1 AND parent_id IS ?2))",
            params![collection_id, parent_id, kind, name, spec_json],
        )?;
        Ok(conn.last_insert_rowid())
    })
}

pub fn item_update(
    store: &Store,
    id: i64,
    name: Option<String>,
    description: Option<String>,
    spec: Option<&RequestSpec>,
) -> Result<(), StoreError> {
    let spec_json = spec.map(|s| serde_json::to_string(s).unwrap_or_else(|_| "{}".into()));
    store.with_conn(|conn| {
        conn.execute(
            "UPDATE collection_items SET
                name = coalesce(?2, name),
                description = coalesce(?3, description),
                req_spec = coalesce(?4, req_spec)
             WHERE id = ?1",
            params![id, name, description, spec_json],
        )?;
        Ok(())
    })
}

/// Move an item to a new parent and/or position among siblings.
pub fn item_move(
    store: &Store,
    id: i64,
    new_parent_id: Option<i64>,
    before_id: Option<i64>,
) -> Result<(), StoreError> {
    store.with_conn(|conn| {
        // Guard: cannot move a folder into its own subtree.
        let mut ancestor = new_parent_id;
        while let Some(a) = ancestor {
            if a == id {
                return Err(rusqlite::Error::InvalidQuery);
            }
            ancestor = conn
                .query_row(
                    "SELECT parent_id FROM collection_items WHERE id = ?1",
                    params![a],
                    |row| row.get(0),
                )
                .optional()?
                .flatten();
        }

        let order: i64 = match before_id {
            Some(before) => {
                let before_order: i64 = conn.query_row(
                    "SELECT sort_order FROM collection_items WHERE id = ?1",
                    params![before],
                    |row| row.get(0),
                )?;
                // Shift everything at/after the anchor down by one.
                conn.execute(
                    "UPDATE collection_items SET sort_order = sort_order + 1
                     WHERE parent_id IS ?1 AND sort_order >= ?2
                       AND collection_id = (SELECT collection_id FROM collection_items WHERE id = ?3)",
                    params![new_parent_id, before_order, id],
                )?;
                before_order
            }
            None => conn.query_row(
                "SELECT coalesce(max(sort_order), 0) + 1 FROM collection_items
                 WHERE parent_id IS ?1
                   AND collection_id = (SELECT collection_id FROM collection_items WHERE id = ?2)",
                params![new_parent_id, id],
                |row| row.get(0),
            )?,
        };
        conn.execute(
            "UPDATE collection_items SET parent_id = ?2, sort_order = ?3 WHERE id = ?1",
            params![id, new_parent_id, order],
        )?;
        Ok(())
    })
}

pub fn item_delete(store: &Store, id: i64) -> Result<(), StoreError> {
    store.with_conn(|conn| {
        conn.execute("DELETE FROM collection_items WHERE id = ?1", params![id])?;
        Ok(())
    })
}

pub fn item_get_spec(store: &Store, id: i64) -> Result<Option<RequestSpec>, StoreError> {
    store.with_conn(|conn| {
        let spec: Option<String> = conn
            .query_row(
                "SELECT req_spec FROM collection_items WHERE id = ?1",
                params![id],
                |row| row.get(0),
            )
            .optional()?
            .flatten();
        Ok(spec.and_then(|s| serde_json::from_str(&s).ok()))
    })
}

/* ---------------- environments & variables ---------------- */

#[derive(Debug, Serialize)]
pub struct Environment {
    pub id: i64,
    pub name: String,
    pub is_active: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Variable {
    #[serde(default)]
    pub key: String,
    #[serde(default)]
    pub initial_value: String,
    #[serde(default)]
    pub current_value: Option<String>,
    #[serde(default)]
    pub is_secret: bool,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

fn default_true() -> bool {
    true
}

impl Variable {
    pub fn effective_value(&self) -> &str {
        self.current_value.as_deref().unwrap_or(&self.initial_value)
    }
}

pub fn env_list(store: &Store) -> Result<Vec<Environment>, StoreError> {
    store.with_conn(|conn| {
        let mut stmt = conn.prepare_cached(
            "SELECT id, name, is_active FROM environments ORDER BY sort_order, id",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(Environment {
                id: row.get(0)?,
                name: row.get(1)?,
                is_active: row.get(2)?,
            })
        })?;
        rows.collect()
    })
}

pub fn env_create(store: &Store, name: &str) -> Result<i64, StoreError> {
    store.with_conn(|conn| {
        conn.execute(
            "INSERT INTO environments (name, sort_order)
             VALUES (?1, (SELECT coalesce(max(sort_order), 0) + 1 FROM environments))",
            params![name],
        )?;
        Ok(conn.last_insert_rowid())
    })
}

pub fn env_rename(store: &Store, id: i64, name: &str) -> Result<(), StoreError> {
    store.with_conn(|conn| {
        conn.execute(
            "UPDATE environments SET name = ?2 WHERE id = ?1",
            params![id, name],
        )?;
        Ok(())
    })
}

pub fn env_delete(store: &Store, id: i64) -> Result<(), StoreError> {
    store.with_conn(|conn| {
        conn.execute("DELETE FROM environments WHERE id = ?1", params![id])?;
        conn.execute(
            "DELETE FROM variables WHERE scope = 'environment' AND owner_id = ?1",
            params![id],
        )?;
        Ok(())
    })
}

/// Activate one environment (or none with id = None).
pub fn env_set_active(store: &Store, id: Option<i64>) -> Result<(), StoreError> {
    store.with_conn(|conn| {
        conn.execute("UPDATE environments SET is_active = 0", [])?;
        if let Some(id) = id {
            conn.execute(
                "UPDATE environments SET is_active = 1 WHERE id = ?1",
                params![id],
            )?;
        }
        Ok(())
    })
}

pub fn vars_get(
    store: &Store,
    scope: &str,
    owner_id: Option<i64>,
) -> Result<Vec<Variable>, StoreError> {
    store.with_conn(|conn| {
        let mut stmt = conn.prepare_cached(
            "SELECT key, initial_value, current_value, is_secret, enabled
             FROM variables WHERE scope = ?1 AND owner_id IS ?2
             ORDER BY sort_order, id",
        )?;
        let rows = stmt.query_map(params![scope, owner_id], |row| {
            Ok(Variable {
                key: row.get(0)?,
                initial_value: row.get(1)?,
                current_value: row.get(2)?,
                is_secret: row.get(3)?,
                enabled: row.get(4)?,
            })
        })?;
        rows.collect()
    })
}

/// Replace the whole variable set of one scope (the UI edits it as a grid).
pub fn vars_save(
    store: &Store,
    scope: &str,
    owner_id: Option<i64>,
    vars: &[Variable],
) -> Result<(), StoreError> {
    store.with_conn(|conn| {
        conn.execute(
            "DELETE FROM variables WHERE scope = ?1 AND owner_id IS ?2",
            params![scope, owner_id],
        )?;
        let mut stmt = conn.prepare_cached(
            "INSERT INTO variables
                (scope, owner_id, key, initial_value, current_value, is_secret, enabled, sort_order)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        )?;
        for (i, v) in vars.iter().filter(|v| !v.key.trim().is_empty()).enumerate() {
            stmt.execute(params![
                scope,
                owner_id,
                v.key.trim(),
                v.initial_value,
                v.current_value,
                v.is_secret,
                v.enabled,
                i as i64,
            ])?;
        }
        Ok(())
    })
}

/// Effective variable map for a request: global < collection < environment.
pub fn effective_vars(
    store: &Store,
    collection_id: Option<i64>,
) -> Result<Vec<Variable>, StoreError> {
    let mut merged: Vec<Variable> = Vec::new();
    let mut push_all = |vars: Vec<Variable>| {
        for v in vars.into_iter().filter(|v| v.enabled) {
            if let Some(existing) = merged.iter_mut().find(|m| m.key == v.key) {
                *existing = v;
            } else {
                merged.push(v);
            }
        }
    };

    push_all(vars_get(store, "global", None)?);
    if let Some(cid) = collection_id {
        push_all(vars_get(store, "collection", Some(cid))?);
    }
    let active_env: Option<i64> = store.with_conn(|conn| {
        conn.query_row(
            "SELECT id FROM environments WHERE is_active = 1 LIMIT 1",
            [],
            |row| row.get(0),
        )
        .optional()
    })?;
    if let Some(eid) = active_env {
        push_all(vars_get(store, "environment", Some(eid))?);
    }
    Ok(merged)
}
