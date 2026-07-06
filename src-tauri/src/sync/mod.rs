//! Client sync engine: serialises each collection/environment into one
//! self-contained JSON blob keyed by a stable `uid`, encrypts it end-to-end
//! and pushes/pulls it to a `postcat-sync` server. Change detection is by
//! content hash at sync time, so ordinary edits need no instrumentation.

mod api;
mod serialize;

use std::sync::Mutex;

use rusqlite::{params, OptionalExtension};
use serde::Serialize;

use crate::crypto::{self, AccountBlob, DataKey};
use crate::store::{Store, StoreError};

pub use serialize::{CollectionContent, EnvironmentContent};

/// In-memory session: the data key (never persisted) plus the server token.
#[derive(Default)]
pub struct SyncSession(pub Mutex<Option<Session>>);

pub struct Session {
    pub url: String,
    pub email: String,
    pub token: String,
    pub data_key: DataKey,
}

#[derive(Debug, thiserror::Error)]
pub enum SyncError {
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error(transparent)]
    Crypto(#[from] crypto::CryptoError),
    #[error("{0}")]
    Server(String),
    #[error("not signed in")]
    NotSignedIn,
}

#[derive(Debug, Serialize)]
pub struct SyncStatus {
    pub signed_in: bool,
    pub url: String,
    pub email: String,
    pub last_cursor: i64,
    pub pending: u32,
}

/* ---------------- account lifecycle ---------------- */

/// Create a server account and sign in. Returns the one-time recovery code.
pub async fn register(
    store: &Store,
    session: &SyncSession,
    url: &str,
    email: &str,
    password: &str,
) -> Result<String, SyncError> {
    let reg = crypto::register(password)?;
    api::register(url, email, &reg.blob)
        .await
        .map_err(SyncError::Server)?;
    let token = api::login(url, email, &reg.auth_verifier)
        .await
        .map_err(SyncError::Server)?
        .token;
    set_config(store, url, email)?;
    *session.0.lock().map_err(|_| SyncError::NotSignedIn)? = Some(Session {
        url: url.to_owned(),
        email: email.to_owned(),
        token,
        data_key: reg.data_key,
    });
    Ok(reg.recovery_code)
}

pub async fn login(
    store: &Store,
    session: &SyncSession,
    url: &str,
    email: &str,
    password: &str,
) -> Result<(), SyncError> {
    let salt = api::salt(url, email).await.map_err(SyncError::Server)?;
    // Reconstruct the blob fields the client needs from server responses.
    let verifier = crypto::login_auth_verifier(password, &salt)?;
    let login = api::login(url, email, &verifier)
        .await
        .map_err(SyncError::Server)?;
    let blob = AccountBlob {
        salt,
        recovery_salt: String::new(),
        auth_verifier_hash: String::new(),
        wrapped_by_password: login.wrapped_by_password,
        wrapped_by_recovery: String::new(),
    };
    let (data_key, _) = crypto::login(password, &blob)?;
    set_config(store, url, email)?;
    *session.0.lock().map_err(|_| SyncError::NotSignedIn)? = Some(Session {
        url: url.to_owned(),
        email: email.to_owned(),
        token: login.token,
        data_key,
    });
    Ok(())
}

pub fn logout(session: &SyncSession) {
    if let Ok(mut s) = session.0.lock() {
        *s = None;
    }
}

pub fn status(store: &Store, session: &SyncSession) -> Result<SyncStatus, SyncError> {
    let signed_in = session.0.lock().map(|s| s.is_some()).unwrap_or(false);
    let url = meta_get(store, "sync_url")?.unwrap_or_default();
    let email = meta_get(store, "sync_email")?.unwrap_or_default();
    let last_cursor = meta_get(store, "sync_cursor")?
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    let pending = count_pending(store)?;
    Ok(SyncStatus {
        signed_in,
        url,
        email,
        last_cursor,
        pending,
    })
}

/* ---------------- push / pull ---------------- */

#[derive(Debug, Serialize)]
pub struct SyncReport {
    pub pushed: u32,
    pub pulled: u32,
    pub cursor: i64,
}

pub async fn sync_now(store: &Store, session: &SyncSession) -> Result<SyncReport, SyncError> {
    let (url, token, data_key) = {
        let guard = session.0.lock().map_err(|_| SyncError::NotSignedIn)?;
        let s = guard.as_ref().ok_or(SyncError::NotSignedIn)?;
        (s.url.clone(), s.token.clone(), s.data_key.clone())
    };

    // ---- push local changes ----
    let mut blobs: Vec<api::Blob> = Vec::new();
    let mut to_mark: Vec<(String, String, i64, String)> = Vec::new(); // table, uid, rev, hash
    for (table, kind) in [
        ("collections", "collection"),
        ("environments", "environment"),
    ] {
        for row in dirty_rows(store, table)? {
            let content = match kind {
                "collection" => serialize::collection_json(store, row.id)?,
                _ => serialize::environment_json(store, row.id)?,
            };
            let hash = hash_str(&content);
            if !row.deleted && Some(&hash) == row.synced_hash.as_ref() {
                continue; // unchanged
            }
            let rev = row.sync_rev.max(row.server_rev) + 1;
            let ciphertext = if row.deleted {
                String::new()
            } else {
                data_key.seal_str(&content)?
            };
            blobs.push(api::Blob {
                kind: kind.to_owned(),
                item_id: row.uid.clone(),
                rev,
                ciphertext,
                updated_at: now(),
                deleted: row.deleted,
                seq: 0,
            });
            to_mark.push((table.to_owned(), row.uid, rev, hash));
        }
    }
    let mut pushed = 0;
    if !blobs.is_empty() {
        let resp = api::push(&url, &token, &blobs)
            .await
            .map_err(SyncError::Server)?;
        pushed = resp.applied;
        for (table, uid, rev, hash) in &to_mark {
            mark_synced(store, table, uid, *rev, hash)?;
        }
    }

    // ---- pull remote changes ----
    let cursor = meta_get(store, "sync_cursor")?
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    let pull = api::pull(&url, &token, cursor)
        .await
        .map_err(SyncError::Server)?;
    let mut pulled = 0;
    for blob in &pull.blobs {
        if apply_pulled(store, &data_key, blob)? {
            pulled += 1;
        }
    }
    if pull.cursor > cursor {
        meta_set(store, "sync_cursor", &pull.cursor.to_string())?;
    }

    Ok(SyncReport {
        pushed,
        pulled,
        cursor: pull.cursor.max(cursor),
    })
}

/// Apply one pulled blob if it is newer than what we have locally.
fn apply_pulled(store: &Store, data_key: &DataKey, blob: &api::Blob) -> Result<bool, SyncError> {
    let table = match blob.kind.as_str() {
        "collection" => "collections",
        "environment" => "environments",
        _ => return Ok(false),
    };
    let local: Option<(i64, i64)> = store.with_conn(|conn| {
        conn.query_row(
            &format!("SELECT id, sync_rev FROM {table} WHERE uid = ?1"),
            params![blob.item_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
    })?;
    if let Some((_, local_rev)) = local {
        if blob.rev <= local_rev {
            return Ok(false); // we already have this or newer
        }
    }
    if blob.deleted {
        store.with_conn(|conn| {
            conn.execute(
                &format!("UPDATE {table} SET deleted = 1, sync_rev = ?2 WHERE uid = ?1"),
                params![blob.item_id, blob.rev],
            )?;
            Ok(())
        })?;
        return Ok(true);
    }
    let content = data_key.open_str(&blob.ciphertext)?;
    let hash = hash_str(&content);
    match blob.kind.as_str() {
        "collection" => serialize::apply_collection(store, &content, blob.rev, &hash)?,
        "environment" => serialize::apply_environment(store, &content, blob.rev, &hash)?,
        _ => {}
    }
    Ok(true)
}

/* ---------------- change detection helpers ---------------- */

struct DirtyRow {
    id: i64,
    uid: String,
    sync_rev: i64,
    server_rev: i64,
    synced_hash: Option<String>,
    deleted: bool,
}

/// Rows that might need pushing: everything (we hash to decide) plus
/// tombstones. Cheap at our scale (dozens of collections).
fn dirty_rows(store: &Store, table: &str) -> Result<Vec<DirtyRow>, StoreError> {
    store.with_conn(|conn| {
        let mut stmt = conn.prepare(&format!(
            "SELECT id, uid, sync_rev, synced_hash, deleted FROM {table}"
        ))?;
        let rows = stmt.query_map([], |row| {
            Ok(DirtyRow {
                id: row.get(0)?,
                uid: row.get::<_, Option<String>>(1)?.unwrap_or_default(),
                sync_rev: row.get(2)?,
                server_rev: row.get(2)?,
                synced_hash: row.get(3)?,
                deleted: row.get::<_, i64>(4)? != 0,
            })
        })?;
        rows.collect()
    })
}

fn count_pending(store: &Store) -> Result<u32, StoreError> {
    let mut pending = 0u32;
    for (table, kind) in [
        ("collections", "collection"),
        ("environments", "environment"),
    ] {
        for row in dirty_rows(store, table)? {
            let content = match kind {
                "collection" => serialize::collection_json(store, row.id).ok(),
                _ => serialize::environment_json(store, row.id).ok(),
            };
            let changed = match &content {
                Some(c) => Some(hash_str(c)) != row.synced_hash,
                None => false,
            };
            if row.deleted || changed || row.synced_hash.is_none() {
                pending += 1;
            }
        }
    }
    Ok(pending)
}

fn mark_synced(
    store: &Store,
    table: &str,
    uid: &str,
    rev: i64,
    hash: &str,
) -> Result<(), StoreError> {
    store.with_conn(|conn| {
        conn.execute(
            &format!("UPDATE {table} SET sync_rev = ?2, synced_hash = ?3 WHERE uid = ?1"),
            params![uid, rev, hash],
        )?;
        Ok(())
    })
}

fn set_config(store: &Store, url: &str, email: &str) -> Result<(), StoreError> {
    meta_set(store, "sync_url", url)?;
    meta_set(store, "sync_email", email)?;
    Ok(())
}

fn meta_get(store: &Store, key: &str) -> Result<Option<String>, StoreError> {
    store.with_conn(|conn| {
        conn.query_row("SELECT value FROM meta WHERE key = ?1", params![key], |r| {
            r.get(0)
        })
        .optional()
    })
}

fn meta_set(store: &Store, key: &str, value: &str) -> Result<(), StoreError> {
    store.with_conn(|conn| {
        conn.execute(
            "INSERT INTO meta (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )?;
        Ok(())
    })
}

fn hash_str(s: &str) -> String {
    use sha2::{Digest, Sha256};
    hex::encode(Sha256::digest(s.as_bytes()))
}

fn now() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    crate::vars::iso8601_from_unix(secs)
}
