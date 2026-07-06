//! SQLite persistence for the sync server. The server is E2E-blind: every
//! `ciphertext` is opaque, and login is verified only by comparing a SHA-256
//! of the client-presented auth verifier to the stored hash.

use std::path::Path;
use std::sync::Mutex;

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

pub struct Db {
    conn: Mutex<Connection>,
}

#[derive(Debug, thiserror::Error)]
pub enum DbError {
    #[error(transparent)]
    Sqlite(#[from] rusqlite::Error),
    #[error("lock poisoned")]
    Lock,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AccountBlob {
    pub salt: String,
    pub recovery_salt: String,
    pub auth_verifier_hash: String,
    pub wrapped_by_password: String,
    pub wrapped_by_recovery: String,
}

/// One encrypted item as pushed/pulled by clients.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Blob {
    pub kind: String,
    pub item_id: String,
    pub rev: i64,
    pub ciphertext: String,
    pub updated_at: String,
    #[serde(default)]
    pub deleted: bool,
    /// Server-assigned monotonic cursor (set on pull, ignored on push).
    #[serde(default)]
    pub seq: i64,
}

impl Db {
    pub fn open(path: &Path) -> Result<Self, DbError> {
        let conn = Connection::open(path)?;
        Self::init(conn)
    }

    #[cfg(test)]
    pub fn open_in_memory() -> Result<Self, DbError> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(conn: Connection) -> Result<Self, DbError> {
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS accounts (
                id                  INTEGER PRIMARY KEY AUTOINCREMENT,
                email               TEXT NOT NULL UNIQUE,
                salt                TEXT NOT NULL,
                recovery_salt       TEXT NOT NULL,
                auth_verifier_hash  TEXT NOT NULL,
                wrapped_by_password TEXT NOT NULL,
                wrapped_by_recovery TEXT NOT NULL,
                created_at          TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
            );
            CREATE TABLE IF NOT EXISTS sessions (
                token       TEXT PRIMARY KEY,
                account_id  INTEGER NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
                expires_at  INTEGER NOT NULL
            );
            -- Server-wide monotonic counter feeds the pull cursor.
            CREATE TABLE IF NOT EXISTS seq_counter (id INTEGER PRIMARY KEY CHECK (id = 1), value INTEGER NOT NULL);
            INSERT OR IGNORE INTO seq_counter (id, value) VALUES (1, 0);
            CREATE TABLE IF NOT EXISTS blobs (
                account_id  INTEGER NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
                kind        TEXT NOT NULL,
                item_id     TEXT NOT NULL,
                rev         INTEGER NOT NULL,
                ciphertext  TEXT NOT NULL,
                updated_at  TEXT NOT NULL,
                deleted     INTEGER NOT NULL DEFAULT 0,
                seq         INTEGER NOT NULL,
                PRIMARY KEY (account_id, kind, item_id)
            );
            CREATE INDEX IF NOT EXISTS idx_blobs_seq ON blobs (account_id, seq);",
        )?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    fn with<T>(
        &self,
        f: impl FnOnce(&Connection) -> Result<T, rusqlite::Error>,
    ) -> Result<T, DbError> {
        let conn = self.conn.lock().map_err(|_| DbError::Lock)?;
        Ok(f(&conn)?)
    }

    pub fn account_exists(&self, email: &str) -> Result<bool, DbError> {
        self.with(|conn| {
            conn.query_row(
                "SELECT 1 FROM accounts WHERE email = ?1",
                params![email],
                |_| Ok(()),
            )
            .optional()
            .map(|o| o.is_some())
        })
    }

    pub fn create_account(&self, email: &str, blob: &AccountBlob) -> Result<(), DbError> {
        self.with(|conn| {
            conn.execute(
                "INSERT INTO accounts
                    (email, salt, recovery_salt, auth_verifier_hash,
                     wrapped_by_password, wrapped_by_recovery)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    email,
                    blob.salt,
                    blob.recovery_salt,
                    blob.auth_verifier_hash,
                    blob.wrapped_by_password,
                    blob.wrapped_by_recovery,
                ],
            )?;
            Ok(())
        })
    }

    pub fn account_field(&self, email: &str, column: &str) -> Result<Option<String>, DbError> {
        // column is from a fixed internal allowlist (never user input).
        let sql = format!("SELECT {column} FROM accounts WHERE email = ?1");
        self.with(|conn| {
            conn.query_row(&sql, params![email], |row| row.get(0))
                .optional()
        })
    }

    /// Verify the presented auth verifier and, on success, mint a session.
    pub fn login(
        &self,
        email: &str,
        auth_verifier_hash: &str,
        token: &str,
        ttl_secs: i64,
    ) -> Result<Option<i64>, DbError> {
        self.with(|conn| {
            let row: Option<(i64, String)> = conn
                .query_row(
                    "SELECT id, auth_verifier_hash FROM accounts WHERE email = ?1",
                    params![email],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .optional()?;
            let Some((id, stored)) = row else {
                return Ok(None);
            };
            if stored != auth_verifier_hash {
                return Ok(None);
            }
            let expires = now_secs() + ttl_secs;
            conn.execute(
                "INSERT INTO sessions (token, account_id, expires_at) VALUES (?1, ?2, ?3)",
                params![token, id, expires],
            )?;
            Ok(Some(id))
        })
    }

    pub fn session_account(&self, token: &str) -> Result<Option<i64>, DbError> {
        self.with(|conn| {
            conn.query_row(
                "SELECT account_id FROM sessions WHERE token = ?1 AND expires_at > ?2",
                params![token, now_secs()],
                |row| row.get(0),
            )
            .optional()
        })
    }

    /// Upsert an item, last-writer-wins on `rev`. Returns true if applied.
    pub fn push_blob(&self, account_id: i64, blob: &Blob) -> Result<bool, DbError> {
        self.with(|conn| {
            let existing_rev: Option<i64> = conn
                .query_row(
                    "SELECT rev FROM blobs WHERE account_id = ?1 AND kind = ?2 AND item_id = ?3",
                    params![account_id, blob.kind, blob.item_id],
                    |row| row.get(0),
                )
                .optional()?;
            if let Some(rev) = existing_rev {
                if blob.rev <= rev {
                    return Ok(false); // stale write
                }
            }
            let seq: i64 = conn.query_row(
                "UPDATE seq_counter SET value = value + 1 WHERE id = 1 RETURNING value",
                [],
                |row| row.get(0),
            )?;
            conn.execute(
                "INSERT INTO blobs (account_id, kind, item_id, rev, ciphertext, updated_at, deleted, seq)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
                 ON CONFLICT(account_id, kind, item_id) DO UPDATE SET
                    rev = excluded.rev, ciphertext = excluded.ciphertext,
                    updated_at = excluded.updated_at, deleted = excluded.deleted,
                    seq = excluded.seq",
                params![
                    account_id,
                    blob.kind,
                    blob.item_id,
                    blob.rev,
                    blob.ciphertext,
                    blob.updated_at,
                    blob.deleted,
                    seq,
                ],
            )?;
            Ok(true)
        })
    }

    /// All items changed strictly after the given server cursor.
    pub fn pull_since(&self, account_id: i64, since: i64) -> Result<Vec<Blob>, DbError> {
        self.with(|conn| {
            let mut stmt = conn.prepare(
                "SELECT kind, item_id, rev, ciphertext, updated_at, deleted, seq
                 FROM blobs WHERE account_id = ?1 AND seq > ?2 ORDER BY seq",
            )?;
            let rows = stmt.query_map(params![account_id, since], |row| {
                Ok(Blob {
                    kind: row.get(0)?,
                    item_id: row.get(1)?,
                    rev: row.get(2)?,
                    ciphertext: row.get(3)?,
                    updated_at: row.get(4)?,
                    deleted: row.get::<_, i64>(5)? != 0,
                    seq: row.get(6)?,
                })
            })?;
            rows.collect()
        })
    }
}

fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    fn blob(email_id: &str, rev: i64) -> Blob {
        Blob {
            kind: "collection".into(),
            item_id: email_id.into(),
            rev,
            ciphertext: format!("cipher-{rev}"),
            updated_at: "2026-07-06T00:00:00Z".into(),
            deleted: false,
            seq: 0,
        }
    }

    fn sample_blob() -> AccountBlob {
        AccountBlob {
            salt: "s".into(),
            recovery_salt: "r".into(),
            auth_verifier_hash: "hash".into(),
            wrapped_by_password: "wp".into(),
            wrapped_by_recovery: "wr".into(),
        }
    }

    #[test]
    fn account_and_login_flow() {
        let db = Db::open_in_memory().unwrap();
        assert!(!db.account_exists("a@b.c").unwrap());
        db.create_account("a@b.c", &sample_blob()).unwrap();
        assert!(db.account_exists("a@b.c").unwrap());

        // salt is public (needed to derive keys before login).
        assert_eq!(
            db.account_field("a@b.c", "salt").unwrap().as_deref(),
            Some("s")
        );

        // wrong verifier -> no session; right verifier -> session.
        assert!(db.login("a@b.c", "nope", "t1", 3600).unwrap().is_none());
        let acct = db.login("a@b.c", "hash", "t2", 3600).unwrap().unwrap();
        assert_eq!(db.session_account("t2").unwrap(), Some(acct));
        assert!(db.session_account("bogus").unwrap().is_none());
    }

    #[test]
    fn push_lww_and_pull_cursor() {
        let db = Db::open_in_memory().unwrap();
        db.create_account("a@b.c", &sample_blob()).unwrap();
        let acct = db.login("a@b.c", "hash", "t", 3600).unwrap().unwrap();

        assert!(db.push_blob(acct, &blob("x", 1)).unwrap());
        assert!(db.push_blob(acct, &blob("y", 1)).unwrap());
        // stale rev rejected, newer rev applied.
        assert!(!db.push_blob(acct, &blob("x", 1)).unwrap());
        assert!(db.push_blob(acct, &blob("x", 2)).unwrap());

        // full pull from 0 returns both items, x at its latest rev.
        let all = db.pull_since(acct, 0).unwrap();
        assert_eq!(all.len(), 2);
        let x = all.iter().find(|b| b.item_id == "x").unwrap();
        assert_eq!(x.rev, 2);

        // incremental pull returns only what changed after a cursor.
        let cursor = all.iter().map(|b| b.seq).max().unwrap();
        assert!(db.pull_since(acct, cursor).unwrap().is_empty());
        db.push_blob(acct, &blob("z", 1)).unwrap();
        let delta = db.pull_since(acct, cursor).unwrap();
        assert_eq!(delta.len(), 1);
        assert_eq!(delta[0].item_id, "z");
    }
}
