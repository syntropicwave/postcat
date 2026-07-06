use rusqlite::params;
use serde::Serialize;

use crate::http_engine::{HttpResponseData, RequestSpec};
use crate::store::{Store, StoreError};

/// How much response body text is returned to the UI in one piece. The full
/// captured body stays in the DB; the viewer gets a capped slice.
const MAX_UI_BODY: usize = 2 * 1024 * 1024;

#[derive(Debug, Serialize)]
pub struct HistorySummary {
    pub id: i64,
    pub sent_at: String,
    pub method: String,
    pub url: String,
    pub host: String,
    pub status: Option<u16>,
    pub error: Option<String>,
    pub duration_ms: Option<f64>,
    pub resp_size: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct HistoryDetail {
    #[serde(flatten)]
    pub summary: HistorySummary,
    /// Full RequestSpec JSON — lets the UI reopen the entry as a draft.
    pub req_spec: serde_json::Value,
    pub req_headers: serde_json::Value,
    pub req_body_text: Option<String>,
    pub status_text: Option<String>,
    pub http_version: Option<String>,
    pub resp_headers: serde_json::Value,
    pub resp_body_text: Option<String>,
    pub resp_body_base64: Option<String>,
    pub resp_body_truncated: bool,
    pub ttfb_ms: Option<f64>,
}

pub fn record(
    store: &Store,
    spec: &RequestSpec,
    outcome: Result<&HttpResponseData, &str>,
) -> Result<i64, StoreError> {
    let spec_json = serde_json::to_string(spec).unwrap_or_else(|_| "{}".into());
    let req_headers = serde_json::to_string(
        &spec
            .headers
            .iter()
            .filter(|h| h.enabled && !h.key.is_empty())
            .map(|h| (h.key.clone(), h.value.clone()))
            .collect::<Vec<_>>(),
    )
    .unwrap_or_else(|_| "[]".into());
    let req_body_text = spec.body_text();

    store.with_conn(|conn| {
        match outcome {
            Ok(resp) => {
                let resp_headers =
                    serde_json::to_string(&resp.headers).unwrap_or_else(|_| "[]".into());
                conn.execute(
                    "INSERT INTO history_entries
                        (method, url, host, req_spec, req_headers, req_body_text,
                         status, status_text, http_version, resp_headers, resp_body,
                         resp_body_truncated, resp_size, duration_ms, ttfb_ms)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
                    params![
                        spec.method,
                        spec.url,
                        spec.host(),
                        spec_json,
                        req_headers,
                        req_body_text,
                        resp.status,
                        resp.status_text,
                        resp.http_version,
                        resp_headers,
                        resp.body,
                        resp.body_truncated,
                        resp.size as i64,
                        resp.duration_ms,
                        resp.ttfb_ms,
                    ],
                )?;
            }
            Err(error) => {
                conn.execute(
                    "INSERT INTO history_entries
                        (method, url, host, req_spec, req_headers, req_body_text, error)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                    params![
                        spec.method,
                        spec.url,
                        spec.host(),
                        spec_json,
                        req_headers,
                        req_body_text,
                        error,
                    ],
                )?;
            }
        }
        Ok(conn.last_insert_rowid())
    })
}

pub fn list(
    store: &Store,
    limit: u32,
    offset: u32,
    query: Option<&str>,
) -> Result<Vec<HistorySummary>, StoreError> {
    store.with_conn(|conn| {
        // Phase 1: substring match on URL/method. Replaced by FTS5 in phase 2.
        let raw = query.map(str::trim).filter(|q| !q.is_empty());
        let like = raw.map(|q| format!("%{}%", q.replace('%', "\\%").replace('_', "\\_")));
        let sql = "SELECT id, sent_at, method, url, host, status, error, duration_ms, resp_size
             FROM history_entries
             WHERE (?1 IS NULL OR url LIKE ?1 ESCAPE '\\' OR upper(method) = upper(?2))
             ORDER BY id DESC LIMIT ?3 OFFSET ?4";
        let mut stmt = conn.prepare(sql)?;
        let rows = stmt.query_map(params![like, raw, limit, offset], |row| {
            Ok(HistorySummary {
                id: row.get(0)?,
                sent_at: row.get(1)?,
                method: row.get(2)?,
                url: row.get(3)?,
                host: row.get(4)?,
                status: row.get(5)?,
                error: row.get(6)?,
                duration_ms: row.get(7)?,
                resp_size: row.get(8)?,
            })
        })?;
        rows.collect()
    })
}

pub fn get(store: &Store, id: i64) -> Result<HistoryDetail, StoreError> {
    store.with_conn(|conn| {
        conn.query_row(
            "SELECT id, sent_at, method, url, host, status, error, duration_ms, resp_size,
                    req_spec, req_headers, req_body_text, status_text, http_version,
                    resp_headers, resp_body, resp_body_truncated, ttfb_ms
             FROM history_entries WHERE id = ?1",
            params![id],
            |row| {
                let resp_body: Option<Vec<u8>> = row.get(15)?;
                let (resp_body_text, resp_body_base64) = match &resp_body {
                    None => (None, None),
                    Some(bytes) => body_for_ui(bytes),
                };
                Ok(HistoryDetail {
                    summary: HistorySummary {
                        id: row.get(0)?,
                        sent_at: row.get(1)?,
                        method: row.get(2)?,
                        url: row.get(3)?,
                        host: row.get(4)?,
                        status: row.get(5)?,
                        error: row.get(6)?,
                        duration_ms: row.get(7)?,
                        resp_size: row.get(8)?,
                    },
                    req_spec: parse_json(row.get::<_, String>(9)?),
                    req_headers: parse_json(row.get::<_, String>(10)?),
                    req_body_text: row.get(11)?,
                    status_text: row.get(12)?,
                    http_version: row.get(13)?,
                    resp_headers: row
                        .get::<_, Option<String>>(14)?
                        .map(parse_json)
                        .unwrap_or(serde_json::Value::Null),
                    resp_body_text,
                    resp_body_base64,
                    resp_body_truncated: row.get(16)?,
                    ttfb_ms: row.get(17)?,
                })
            },
        )
    })
}

pub fn delete(store: &Store, id: i64) -> Result<(), StoreError> {
    store.with_conn(|conn| {
        conn.execute("DELETE FROM history_entries WHERE id = ?1", params![id])?;
        Ok(())
    })
}

pub fn clear(store: &Store) -> Result<(), StoreError> {
    store.with_conn(|conn| {
        conn.execute("DELETE FROM history_entries", [])?;
        Ok(())
    })
}

fn parse_json(s: String) -> serde_json::Value {
    serde_json::from_str(&s).unwrap_or(serde_json::Value::Null)
}

/// Split a stored body into what the UI needs: lossy text for text-ish
/// payloads, base64 for binary ones (images etc.).
pub fn body_for_ui(bytes: &[u8]) -> (Option<String>, Option<String>) {
    let looks_binary = bytes.iter().take(8192).any(|&b| b == 0);
    if looks_binary {
        use base64::Engine;
        let b64 = base64::engine::general_purpose::STANDARD.encode(bytes);
        (None, Some(b64))
    } else {
        let capped = &bytes[..bytes.len().min(MAX_UI_BODY)];
        (Some(String::from_utf8_lossy(capped).into_owned()), None)
    }
}
