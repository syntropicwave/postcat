//! Saved host aliases — a short, coloured label shown in place of a host.

use serde::{Deserialize, Serialize};

use crate::store::{Store, StoreError};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HostAlias {
    pub id: i64,
    pub host: String,
    pub alias: String,
    pub color: String,
}

pub fn list(store: &Store) -> Result<Vec<HostAlias>, StoreError> {
    store.with_conn(|conn| {
        let mut stmt =
            conn.prepare("SELECT id, host, alias, color FROM host_aliases ORDER BY alias")?;
        let rows = stmt
            .query_map([], |r| {
                Ok(HostAlias {
                    id: r.get(0)?,
                    host: r.get(1)?,
                    alias: r.get(2)?,
                    color: r.get(3)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    })
}

/// Insert or update the alias/colour for `host` (matched case-insensitively),
/// returning the stored row.
pub fn upsert(
    store: &Store,
    host: &str,
    alias: &str,
    color: &str,
) -> Result<HostAlias, StoreError> {
    let host = host.trim().to_lowercase();
    store.with_conn(|conn| {
        conn.execute(
            "INSERT INTO host_aliases (host, alias, color) VALUES (?1, ?2, ?3)
             ON CONFLICT(host) DO UPDATE SET alias = excluded.alias, color = excluded.color",
            rusqlite::params![host, alias.trim(), color],
        )?;
        conn.query_row(
            "SELECT id, host, alias, color FROM host_aliases WHERE host = ?1",
            [&host],
            |r| {
                Ok(HostAlias {
                    id: r.get(0)?,
                    host: r.get(1)?,
                    alias: r.get(2)?,
                    color: r.get(3)?,
                })
            },
        )
    })
}

pub fn delete(store: &Store, id: i64) -> Result<(), StoreError> {
    store.with_conn(|conn| {
        conn.execute("DELETE FROM host_aliases WHERE id = ?1", [id])?;
        Ok(())
    })
}
