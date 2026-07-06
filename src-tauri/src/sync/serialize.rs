//! (De)serialise a collection or environment to one self-contained JSON blob.
//! Typed structs keep the JSON field order stable so the content hash is
//! deterministic across runs.

use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::collections::{self, Variable};
use crate::store::{Store, StoreError};

#[derive(Debug, Serialize, Deserialize)]
pub struct CollectionContent {
    pub uid: String,
    pub name: String,
    pub description: String,
    pub auth: serde_json::Value,
    pub pre_request_script: Option<String>,
    pub test_script: Option<String>,
    pub variables: Vec<Variable>,
    pub items: Vec<ItemContent>,
}

/// Items are flattened; `local_id`/`parent_local_id` resolve the tree within
/// this blob only (they are not cross-device identities).
#[derive(Debug, Serialize, Deserialize)]
pub struct ItemContent {
    pub local_id: i64,
    pub parent_local_id: Option<i64>,
    pub kind: String,
    pub name: String,
    pub description: String,
    pub sort_order: i64,
    pub req_spec: serde_json::Value,
    pub auth: serde_json::Value,
    pub pre_request_script: Option<String>,
    pub test_script: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct EnvironmentContent {
    pub uid: String,
    pub name: String,
    pub variables: Vec<Variable>,
}

fn json_or_null(s: Option<String>) -> serde_json::Value {
    s.and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or(serde_json::Value::Null)
}

pub fn collection_json(store: &Store, id: i64) -> Result<String, StoreError> {
    let content = store.with_conn(|conn| {
        let (uid, name, description, auth, pre, test): (
            String,
            String,
            String,
            Option<String>,
            Option<String>,
            Option<String>,
        ) = conn.query_row(
            "SELECT uid, name, description, auth, pre_request_script, test_script
             FROM collections WHERE id = ?1",
            params![id],
            |row| {
                Ok((
                    row.get::<_, Option<String>>(0)?.unwrap_or_default(),
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                ))
            },
        )?;

        let mut stmt = conn.prepare(
            "SELECT id, parent_id, kind, name, description, sort_order, req_spec,
                    auth, pre_request_script, test_script
             FROM collection_items WHERE collection_id = ?1
             ORDER BY parent_id NULLS FIRST, sort_order, id",
        )?;
        let items: Vec<ItemContent> = stmt
            .query_map(params![id], |row| {
                Ok(ItemContent {
                    local_id: row.get(0)?,
                    parent_local_id: row.get(1)?,
                    kind: row.get(2)?,
                    name: row.get(3)?,
                    description: row.get(4)?,
                    sort_order: row.get(5)?,
                    req_spec: json_or_null(row.get(6)?),
                    auth: json_or_null(row.get(7)?),
                    pre_request_script: row.get(8)?,
                    test_script: row.get(9)?,
                })
            })?
            .collect::<Result<_, _>>()?;

        Ok((uid, name, description, auth, pre, test, items))
    })?;
    let (uid, name, description, auth, pre, test, items) = content;
    let variables = collections::vars_get(store, "collection", Some(id))?;

    let doc = CollectionContent {
        uid,
        name,
        description,
        auth: json_or_null(auth),
        pre_request_script: pre,
        test_script: test,
        variables,
        items,
    };
    Ok(serde_json::to_string(&doc).unwrap_or_default())
}

pub fn apply_collection(
    store: &Store,
    content: &str,
    rev: i64,
    hash: &str,
) -> Result<(), StoreError> {
    let doc: CollectionContent = serde_json::from_str(content).map_err(|_| StoreError::Poisoned)?;

    store.with_conn(|conn| {
        // Upsert the collection row by uid.
        let existing: Option<i64> = conn
            .query_row(
                "SELECT id FROM collections WHERE uid = ?1",
                params![doc.uid],
                |row| row.get(0),
            )
            .optional()?;
        let cid = match existing {
            Some(id) => {
                conn.execute(
                    "UPDATE collections SET name = ?2, description = ?3, auth = ?4,
                        pre_request_script = ?5, test_script = ?6,
                        deleted = 0, sync_rev = ?7, synced_hash = ?8
                     WHERE id = ?1",
                    params![
                        id,
                        doc.name,
                        doc.description,
                        auth_str(&doc.auth),
                        doc.pre_request_script,
                        doc.test_script,
                        rev,
                        hash,
                    ],
                )?;
                id
            }
            None => {
                conn.execute(
                    "INSERT INTO collections
                        (uid, name, description, auth, pre_request_script, test_script,
                         sort_order, sync_rev, synced_hash)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6,
                             (SELECT coalesce(max(sort_order),0)+1 FROM collections), ?7, ?8)",
                    params![
                        doc.uid,
                        doc.name,
                        doc.description,
                        auth_str(&doc.auth),
                        doc.pre_request_script,
                        doc.test_script,
                        rev,
                        hash,
                    ],
                )?;
                conn.last_insert_rowid()
            }
        };

        // Replace items and collection variables wholesale.
        conn.execute(
            "DELETE FROM collection_items WHERE collection_id = ?1",
            params![cid],
        )?;
        let mut id_map: std::collections::HashMap<i64, i64> = std::collections::HashMap::new();
        for item in &doc.items {
            let parent = item.parent_local_id.and_then(|p| id_map.get(&p).copied());
            conn.execute(
                "INSERT INTO collection_items
                    (collection_id, parent_id, kind, name, description, sort_order,
                     req_spec, auth, pre_request_script, test_script)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                params![
                    cid,
                    parent,
                    item.kind,
                    item.name,
                    item.description,
                    item.sort_order,
                    value_str(&item.req_spec),
                    auth_str(&item.auth),
                    item.pre_request_script,
                    item.test_script,
                ],
            )?;
            id_map.insert(item.local_id, conn.last_insert_rowid());
        }
        Ok(())
    })?;

    // Collection variables via the existing helper (own transaction).
    collections::vars_save(
        store,
        "collection",
        Some(cid_by_uid(store, &doc.uid)?),
        &doc.variables,
    )?;
    Ok(())
}

pub fn environment_json(store: &Store, id: i64) -> Result<String, StoreError> {
    let (uid, name): (String, String) = store.with_conn(|conn| {
        conn.query_row(
            "SELECT uid, name FROM environments WHERE id = ?1",
            params![id],
            |row| {
                Ok((
                    row.get::<_, Option<String>>(0)?.unwrap_or_default(),
                    row.get(1)?,
                ))
            },
        )
    })?;
    let variables = collections::vars_get(store, "environment", Some(id))?;
    let doc = EnvironmentContent {
        uid,
        name,
        variables,
    };
    Ok(serde_json::to_string(&doc).unwrap_or_default())
}

pub fn apply_environment(
    store: &Store,
    content: &str,
    rev: i64,
    hash: &str,
) -> Result<(), StoreError> {
    let doc: EnvironmentContent =
        serde_json::from_str(content).map_err(|_| StoreError::Poisoned)?;
    let eid = store.with_conn(|conn| {
        let existing: Option<i64> = conn
            .query_row(
                "SELECT id FROM environments WHERE uid = ?1",
                params![doc.uid],
                |row| row.get(0),
            )
            .optional()?;
        let eid = match existing {
            Some(id) => {
                conn.execute(
                    "UPDATE environments SET name = ?2, deleted = 0, sync_rev = ?3, synced_hash = ?4
                     WHERE id = ?1",
                    params![id, doc.name, rev, hash],
                )?;
                id
            }
            None => {
                conn.execute(
                    "INSERT INTO environments (uid, name, sort_order, sync_rev, synced_hash)
                     VALUES (?1, ?2, (SELECT coalesce(max(sort_order),0)+1 FROM environments), ?3, ?4)",
                    params![doc.uid, doc.name, rev, hash],
                )?;
                conn.last_insert_rowid()
            }
        };
        Ok(eid)
    })?;
    collections::vars_save(store, "environment", Some(eid), &doc.variables)?;
    Ok(())
}

fn cid_by_uid(store: &Store, uid: &str) -> Result<i64, StoreError> {
    store.with_conn(|conn| {
        conn.query_row(
            "SELECT id FROM collections WHERE uid = ?1",
            params![uid],
            |row| row.get(0),
        )
    })
}

fn auth_str(v: &serde_json::Value) -> Option<String> {
    if v.is_null() {
        None
    } else {
        Some(v.to_string())
    }
}

fn value_str(v: &serde_json::Value) -> Option<String> {
    if v.is_null() {
        None
    } else {
        Some(v.to_string())
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use crate::http_engine::RequestSpec;

    fn var(key: &str, value: &str, secret: bool) -> Variable {
        Variable {
            key: key.into(),
            initial_value: value.into(),
            current_value: None,
            is_secret: secret,
            enabled: true,
        }
    }

    #[test]
    fn collection_roundtrips_across_stores() {
        // Source store: a collection with a folder, a request and vars.
        let a = Store::open_in_memory().unwrap();
        let cid = collections::create(&a, "Petstore").unwrap();
        let folder = collections::item_create(&a, cid, None, "folder", "Pets", None).unwrap();
        let spec = RequestSpec {
            method: "POST".into(),
            url: "https://api.dev/pets".into(),
            ..Default::default()
        };
        collections::item_create(&a, cid, Some(folder), "request", "Create", Some(&spec)).unwrap();
        collections::vars_save(
            &a,
            "collection",
            Some(cid),
            &[var("base", "https://api.dev", false)],
        )
        .unwrap();

        let json = collection_json(&a, cid).unwrap();
        let hash = "h".to_string();

        // Apply into a fresh store: same tree appears under the same uid.
        let b = Store::open_in_memory().unwrap();
        apply_collection(&b, &json, 1, &hash).unwrap();

        let cols = collections::list(&b).unwrap();
        assert_eq!(cols.len(), 1);
        assert_eq!(cols[0].name, "Petstore");
        let items = collections::items(&b, cols[0].id).unwrap();
        assert_eq!(items.len(), 2);
        let f = items.iter().find(|i| i.kind == "folder").unwrap();
        let r = items.iter().find(|i| i.kind == "request").unwrap();
        assert_eq!(r.parent_id, Some(f.id)); // tree preserved
        assert_eq!(r.req_spec.as_ref().unwrap()["url"], "https://api.dev/pets");
        let vars = collections::vars_get(&b, "collection", Some(cols[0].id)).unwrap();
        assert_eq!(vars[0].key, "base");

        // Serialising the applied copy yields identical content (stable hash).
        let json_b = collection_json(&b, cols[0].id).unwrap();
        assert_eq!(
            super::super::hash_str(&json),
            super::super::hash_str(&json_b)
        );
    }

    #[test]
    fn apply_is_idempotent_and_updates_in_place() {
        let a = Store::open_in_memory().unwrap();
        let cid = collections::create(&a, "One").unwrap();
        let json = collection_json(&a, cid).unwrap();

        let b = Store::open_in_memory().unwrap();
        apply_collection(&b, &json, 1, "h1").unwrap();
        apply_collection(&b, &json, 2, "h2").unwrap();
        // Still exactly one collection (upsert by uid, not a duplicate).
        assert_eq!(collections::list(&b).unwrap().len(), 1);
    }

    #[test]
    fn environment_roundtrips() {
        let a = Store::open_in_memory().unwrap();
        let eid = collections::env_create(&a, "prod").unwrap();
        collections::vars_save(
            &a,
            "environment",
            Some(eid),
            &[var("host", "prod.dev", false), var("key", "s", true)],
        )
        .unwrap();

        let json = environment_json(&a, eid).unwrap();
        let b = Store::open_in_memory().unwrap();
        apply_environment(&b, &json, 1, "h").unwrap();

        let envs = collections::env_list(&b).unwrap();
        assert_eq!(envs.len(), 1);
        assert_eq!(envs[0].name, "prod");
        let vars = collections::vars_get(&b, "environment", Some(envs[0].id)).unwrap();
        assert!(vars.iter().any(|v| v.key == "key" && v.is_secret));
    }
}
