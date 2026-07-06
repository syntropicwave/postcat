mod migrations;

use std::path::Path;
use std::sync::Mutex;

use rusqlite::Connection;

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("database error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("store lock poisoned")]
    Poisoned,
}

/// Application data store: a single SQLite database behind a mutex.
///
/// Phase 0 keeps the concurrency model trivial (one connection, coarse lock).
/// If profiling ever shows contention, the upgrade path is a read pool +
/// single writer, not an ORM.
pub struct Store {
    conn: Mutex<Connection>,
}

impl Store {
    pub fn open(path: &Path) -> Result<Self, StoreError> {
        let conn = Connection::open(path)?;
        Self::from_connection(conn)
    }

    #[cfg(test)]
    pub fn open_in_memory() -> Result<Self, StoreError> {
        Self::from_connection(Connection::open_in_memory()?)
    }

    fn from_connection(conn: Connection) -> Result<Self, StoreError> {
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        migrations::apply(&conn)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    pub fn with_conn<T>(
        &self,
        f: impl FnOnce(&Connection) -> Result<T, rusqlite::Error>,
    ) -> Result<T, StoreError> {
        let conn = self.conn.lock().map_err(|_| StoreError::Poisoned)?;
        Ok(f(&conn)?)
    }

    pub fn schema_version(&self) -> Result<i64, StoreError> {
        self.with_conn(|conn| conn.query_row("PRAGMA user_version", [], |row| row.get(0)))
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn migrations_apply_on_fresh_db() {
        let store = Store::open_in_memory().unwrap();
        assert_eq!(store.schema_version().unwrap(), migrations::LATEST_VERSION);
        let created_at: String = store
            .with_conn(|conn| {
                conn.query_row(
                    "SELECT value FROM meta WHERE key = 'schema_created_at'",
                    [],
                    |row| row.get(0),
                )
            })
            .unwrap();
        assert!(!created_at.is_empty());
    }

    #[test]
    fn migrations_are_idempotent() {
        let store = Store::open_in_memory().unwrap();
        store
            .with_conn(|conn| migrations::apply(conn).map_err(|_| rusqlite::Error::InvalidQuery))
            .unwrap();
        assert_eq!(store.schema_version().unwrap(), migrations::LATEST_VERSION);
    }

    #[test]
    fn fts5_is_available() {
        // History search (phase 2) depends on FTS5; fail fast if the bundled
        // SQLite was built without it.
        let store = Store::open_in_memory().unwrap();
        store
            .with_conn(|conn| {
                conn.execute_batch(
                    "CREATE VIRTUAL TABLE fts_probe USING fts5(content);
                     INSERT INTO fts_probe (content) VALUES ('hello searchable world');",
                )
            })
            .unwrap();
        let hits: i64 = store
            .with_conn(|conn| {
                conn.query_row(
                    "SELECT count(*) FROM fts_probe WHERE fts_probe MATCH 'searchable'",
                    [],
                    |row| row.get(0),
                )
            })
            .unwrap();
        assert_eq!(hits, 1);
    }
}
