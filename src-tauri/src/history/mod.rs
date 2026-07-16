use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::http_engine::{HttpResponseData, RequestSpec, SendError};
use crate::store::{Store, StoreError};

/// How much response body text is returned to the UI in one piece. The full
/// captured body stays in the DB; the viewer gets a capped slice.
const MAX_UI_BODY: usize = 2 * 1024 * 1024;

/// How much response text feeds the full-text index. Kept well below the
/// 5 MB capture cap so the index stays lean.
const MAX_INDEX_TEXT: usize = 256 * 1024;

/// How many candidates the FTS/trigram indexes contribute before filters and
/// pagination. Generous relative to a screenful; keeps worst-case latency flat.
const FTS_CANDIDATES: usize = 1000;
const TRGM_CANDIDATES: usize = 500;

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
    pub pinned: bool,
    pub label: Option<String>,
    /// Match context with `[[`..`]]` marking the hit; present only for
    /// full-text search results.
    pub snippet: Option<String>,
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
    pub timings: crate::http_engine::Timings,
    /// Failure stage tag (`dns`/`tcp`/`tls`/…) and hint, so reopening a failed
    /// entry rebuilds the same error pipeline. NULL for successes.
    pub error_phase: Option<String>,
    pub error_hint: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
pub struct SearchFilters {
    #[serde(default)]
    pub query: Option<String>,
    #[serde(default)]
    pub method: Option<String>,
    #[serde(default)]
    pub host: Option<String>,
    #[serde(default)]
    pub status_exact: Option<u16>,
    /// 2, 3, 4 or 5 — matches the whole status class.
    #[serde(default)]
    pub status_class: Option<u8>,
    #[serde(default)]
    pub errors_only: bool,
    #[serde(default)]
    pub pinned_only: bool,
    /// ISO timestamps (inclusive from, exclusive to), compared textually.
    #[serde(default)]
    pub date_from: Option<String>,
    #[serde(default)]
    pub date_to: Option<String>,
    /// Endpoint drill-down: method + URL-without-query.
    #[serde(default)]
    pub endpoint: Option<Endpoint>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Endpoint {
    pub method: String,
    pub url_base: String,
}

#[derive(Debug, Serialize)]
pub struct EndpointGroup {
    pub method: String,
    pub url_base: String,
    pub count: i64,
    pub last_sent_at: String,
    pub last_status: Option<u16>,
    pub last_error: Option<String>,
}

#[derive(Debug, Default, Clone, Copy, Serialize, Deserialize)]
pub struct RetentionSettings {
    /// 0 = unlimited.
    pub max_age_days: u32,
    /// 0 = unlimited. Pinned entries never count and are never deleted.
    pub max_entries: u32,
}

/// Record a send into history.
///
/// `original` is the unresolved spec (with `{{var}}` placeholders) — stored
/// as `req_spec` so replay re-resolves against current variables. `display`
/// is the resolved-and-secret-masked spec used for the visible/searchable
/// columns. `secrets` are (value, key) pairs to scrub from indexed response
/// text so secret values are never searchable.
pub fn record(
    store: &Store,
    original: &RequestSpec,
    display: &RequestSpec,
    secrets: &[(String, String)],
    outcome: Result<&HttpResponseData, &SendError>,
) -> Result<i64, StoreError> {
    let spec = display;
    let spec_json = serde_json::to_string(original).unwrap_or_else(|_| "{}".into());
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

    let id = store.with_conn(|conn| {
        match outcome {
            Ok(resp) => {
                let resp_headers =
                    serde_json::to_string(&resp.headers).unwrap_or_else(|_| "[]".into());
                let resp_body_text =
                    index_text(&resp.body).map(|t| crate::vars::mask_str(&t, secrets));
                conn.execute(
                    "INSERT INTO history_entries
                        (method, url, host, req_spec, req_headers, req_body_text,
                         status, status_text, http_version, resp_headers, resp_body,
                         resp_body_text, resp_body_truncated, resp_size, duration_ms, ttfb_ms,
                         dns_ms, connect_ms, tls_ms, server_ms, download_ms, redirects)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16,
                             ?17, ?18, ?19, ?20, ?21, ?22)",
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
                        resp_body_text,
                        resp.body_truncated,
                        resp.size as i64,
                        resp.duration_ms,
                        resp.ttfb_ms,
                        resp.timings.dns_ms,
                        resp.timings.connect_ms,
                        resp.timings.tls_ms,
                        resp.timings.server_ms,
                        resp.timings.download_ms,
                        resp.timings.redirects,
                    ],
                )?;
            }
            Err(error) => {
                conn.execute(
                    "INSERT INTO history_entries
                        (method, url, host, req_spec, req_headers, req_body_text,
                         error, error_phase, error_hint)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                    params![
                        spec.method,
                        spec.url,
                        spec.host(),
                        spec_json,
                        req_headers,
                        req_body_text,
                        error.message,
                        error.phase.as_str(),
                        error.hint,
                    ],
                )?;
            }
        }
        Ok(conn.last_insert_rowid())
    })?;

    apply_retention(store)?;
    Ok(id)
}

pub fn search(
    store: &Store,
    filters: &SearchFilters,
    limit: u32,
    offset: u32,
) -> Result<Vec<HistorySummary>, StoreError> {
    let query = filters
        .query
        .as_deref()
        .map(str::trim)
        .filter(|q| !q.is_empty());

    store.with_conn(|conn| match query {
        Some(q) => search_text(conn, q, filters, limit, offset),
        None => list_filtered(conn, filters, limit, offset),
    })
}

/// Text-query path: collect candidates from the FTS + trigram indexes, newest
/// first, then hydrate and filter them. Recency beats BM25 for a history tool
/// (and skipping global rank scoring keeps latency flat: `ORDER BY rowid DESC
/// LIMIT n` walks the doclist without scoring the whole corpus).
fn search_text(
    conn: &Connection,
    q: &str,
    filters: &SearchFilters,
    limit: u32,
    offset: u32,
) -> Result<Vec<HistorySummary>, rusqlite::Error> {
    // id -> snippet
    let mut candidates: Vec<(i64, Option<String>)> = Vec::new();

    if let Some(fts_query) = build_fts_query(q) {
        let mut stmt = conn.prepare_cached(
            "SELECT rowid, snippet(history_fts, -1, '[[', ']]', ' … ', 12)
             FROM history_fts WHERE history_fts MATCH ?1
             ORDER BY rowid DESC LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![fts_query, FTS_CANDIDATES as i64], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, Option<String>>(1)?))
        })?;
        for row in rows {
            candidates.push(row?);
        }
    }

    // Substring hits in URLs (trigram wants >= 3 chars).
    if q.chars().count() >= 3 {
        let trgm_query = format!("\"{}\"", q.replace('"', "\"\""));
        let mut stmt = conn.prepare_cached(
            "SELECT rowid FROM history_url_trgm WHERE history_url_trgm MATCH ?1
             ORDER BY rowid DESC LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![trgm_query, TRGM_CANDIDATES as i64], |row| {
            row.get::<_, i64>(0)
        })?;
        for row in rows {
            candidates.push((row?, None));
        }
    }

    // Dedupe (keep any snippet), newest first.
    candidates.sort_by_key(|c| std::cmp::Reverse(c.0));
    candidates.dedup_by(|next, kept| {
        if next.0 == kept.0 {
            if kept.1.is_none() {
                kept.1 = next.1.take();
            }
            true
        } else {
            false
        }
    });

    let mut stmt = conn.prepare_cached(
        "SELECT id, sent_at, method, url, host, status, error, duration_ms, resp_size,
                pinned, label
         FROM history_entries WHERE id = ?1",
    )?;

    let mut out = Vec::new();
    let mut skipped = 0u32;
    for (id, snippet) in candidates {
        if out.len() >= limit as usize {
            break;
        }
        let row = stmt
            .query_row(params![id], |row| {
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
                    pinned: row.get(9)?,
                    label: row.get(10)?,
                    snippet: None,
                })
            })
            .optional()?;
        let Some(mut summary) = row else { continue };
        if !matches_filters(&summary, filters) {
            continue;
        }
        if skipped < offset {
            skipped += 1;
            continue;
        }
        summary.snippet = snippet;
        out.push(summary);
    }
    Ok(out)
}

fn list_filtered(
    conn: &Connection,
    filters: &SearchFilters,
    limit: u32,
    offset: u32,
) -> Result<Vec<HistorySummary>, rusqlite::Error> {
    let endpoint_method = filters.endpoint.as_ref().map(|e| e.method.clone());
    let endpoint_base = filters.endpoint.as_ref().map(|e| e.url_base.clone());
    let mut stmt = conn.prepare_cached(
        "SELECT id, sent_at, method, url, host, status, error, duration_ms, resp_size,
                pinned, label
         FROM history_entries
         WHERE (?1 IS NULL OR upper(method) = upper(?1))
           AND (?2 IS NULL OR host LIKE '%' || ?2 || '%')
           AND (?3 IS NULL OR status = ?3)
           AND (?4 IS NULL OR status / 100 = ?4)
           AND (NOT ?5 OR error IS NOT NULL OR status >= 400)
           AND (NOT ?6 OR pinned = 1)
           AND (?7 IS NULL OR sent_at >= ?7)
           AND (?8 IS NULL OR sent_at < ?8)
           AND (?9 IS NULL OR (upper(method) = upper(?9) AND url_base = ?10))
         ORDER BY id DESC LIMIT ?11 OFFSET ?12",
    )?;
    let rows = stmt.query_map(
        params![
            filters.method,
            filters.host,
            filters.status_exact,
            filters.status_class,
            filters.errors_only,
            filters.pinned_only,
            filters.date_from,
            filters.date_to,
            endpoint_method,
            endpoint_base,
            limit,
            offset,
        ],
        |row| {
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
                pinned: row.get(9)?,
                label: row.get(10)?,
                snippet: None,
            })
        },
    )?;
    rows.collect()
}

/// Filter checks for the text-query path (mirrors the SQL in list_filtered).
fn matches_filters(s: &HistorySummary, f: &SearchFilters) -> bool {
    if let Some(m) = &f.method {
        if !s.method.eq_ignore_ascii_case(m) {
            return false;
        }
    }
    if let Some(h) = &f.host {
        if !s.host.to_lowercase().contains(&h.to_lowercase()) {
            return false;
        }
    }
    if let Some(code) = f.status_exact {
        if s.status != Some(code) {
            return false;
        }
    }
    if let Some(class) = f.status_class {
        if s.status.map(|c| c / 100) != Some(class as u16) {
            return false;
        }
    }
    if f.errors_only && s.error.is_none() && s.status.is_none_or(|c| c < 400) {
        return false;
    }
    if f.pinned_only && !s.pinned {
        return false;
    }
    if let Some(from) = &f.date_from {
        if s.sent_at.as_str() < from.as_str() {
            return false;
        }
    }
    if let Some(to) = &f.date_to {
        if s.sent_at.as_str() >= to.as_str() {
            return false;
        }
    }
    if let Some(ep) = &f.endpoint {
        if !s.method.eq_ignore_ascii_case(&ep.method) || url_base(&s.url) != ep.url_base {
            return false;
        }
    }
    true
}

fn url_base(url: &str) -> &str {
    url.split('?').next().unwrap_or(url)
}

/// Turn free-form user input into an FTS5 query: every whitespace-separated
/// term becomes a quoted phrase, the last one a prefix (live typing).
fn build_fts_query(input: &str) -> Option<String> {
    let terms: Vec<String> = input
        .split_whitespace()
        .map(|t| format!("\"{}\"", t.replace('"', "\"\"")))
        .collect();
    if terms.is_empty() {
        return None;
    }
    let mut parts = terms;
    if let Some(last) = parts.last_mut() {
        last.push('*');
    }
    Some(parts.join(" AND "))
}

pub fn endpoints(store: &Store, limit: u32) -> Result<Vec<EndpointGroup>, StoreError> {
    store.with_conn(|conn| {
        let mut stmt = conn.prepare_cached(
            "SELECT method, url_base, count(*), max(sent_at),
                    (SELECT status FROM history_entries h2
                     WHERE h2.method = h.method AND h2.url_base = h.url_base
                     ORDER BY h2.id DESC LIMIT 1),
                    (SELECT error FROM history_entries h2
                     WHERE h2.method = h.method AND h2.url_base = h.url_base
                     ORDER BY h2.id DESC LIMIT 1)
             FROM history_entries h
             WHERE url_base != ''
             GROUP BY method, url_base
             ORDER BY max(id) DESC LIMIT ?1",
        )?;
        let rows = stmt.query_map(params![limit], |row| {
            Ok(EndpointGroup {
                method: row.get(0)?,
                url_base: row.get(1)?,
                count: row.get(2)?,
                last_sent_at: row.get(3)?,
                last_status: row.get(4)?,
                last_error: row.get(5)?,
            })
        })?;
        rows.collect()
    })
}

pub fn get(store: &Store, id: i64) -> Result<HistoryDetail, StoreError> {
    store.with_conn(|conn| {
        conn.query_row(
            "SELECT id, sent_at, method, url, host, status, error, duration_ms, resp_size,
                    pinned, label,
                    req_spec, req_headers, req_body_text, status_text, http_version,
                    resp_headers, resp_body, resp_body_truncated, ttfb_ms,
                    dns_ms, connect_ms, tls_ms, server_ms, download_ms, redirects,
                    error_phase, error_hint
             FROM history_entries WHERE id = ?1",
            params![id],
            |row| {
                let resp_body: Option<Vec<u8>> = row.get(17)?;
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
                        pinned: row.get(9)?,
                        label: row.get(10)?,
                        snippet: None,
                    },
                    req_spec: parse_json(row.get::<_, String>(11)?),
                    req_headers: parse_json(row.get::<_, String>(12)?),
                    req_body_text: row.get(13)?,
                    status_text: row.get(14)?,
                    http_version: row.get(15)?,
                    resp_headers: row
                        .get::<_, Option<String>>(16)?
                        .map(parse_json)
                        .unwrap_or(serde_json::Value::Null),
                    resp_body_text,
                    resp_body_base64,
                    resp_body_truncated: row.get(18)?,
                    ttfb_ms: row.get(19)?,
                    timings: crate::http_engine::Timings {
                        dns_ms: row.get(20)?,
                        connect_ms: row.get(21)?,
                        tls_ms: row.get(22)?,
                        server_ms: row.get::<_, Option<f64>>(23)?.unwrap_or(0.0),
                        download_ms: row.get::<_, Option<f64>>(24)?.unwrap_or(0.0),
                        total_ms: row.get::<_, Option<f64>>(7)?.unwrap_or(0.0),
                        redirects: row.get::<_, i64>(25)? as u32,
                    },
                    error_phase: row.get(26)?,
                    error_hint: row.get(27)?,
                })
            },
        )
    })
}

pub fn set_pinned(store: &Store, id: i64, pinned: bool) -> Result<(), StoreError> {
    store.with_conn(|conn| {
        conn.execute(
            "UPDATE history_entries SET pinned = ?2 WHERE id = ?1",
            params![id, pinned],
        )?;
        Ok(())
    })
}

pub fn set_label(store: &Store, id: i64, label: Option<String>) -> Result<(), StoreError> {
    store.with_conn(|conn| {
        conn.execute(
            "UPDATE history_entries SET label = ?2 WHERE id = ?1",
            params![id, label.filter(|l| !l.trim().is_empty())],
        )?;
        Ok(())
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
        conn.execute("DELETE FROM history_entries WHERE pinned = 0", [])?;
        Ok(())
    })
}

pub fn retention_get(store: &Store) -> Result<RetentionSettings, StoreError> {
    store.with_conn(|conn| {
        let json: Option<String> = conn
            .query_row(
                "SELECT value FROM meta WHERE key = 'retention'",
                [],
                |row| row.get(0),
            )
            .optional()?;
        Ok(json
            .and_then(|j| serde_json::from_str(&j).ok())
            .unwrap_or_default())
    })
}

pub fn retention_set(store: &Store, settings: RetentionSettings) -> Result<(), StoreError> {
    store.with_conn(|conn| {
        let json = serde_json::to_string(&settings).unwrap_or_else(|_| "{}".into());
        conn.execute(
            "INSERT INTO meta (key, value) VALUES ('retention', ?1)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![json],
        )?;
        Ok(())
    })?;
    apply_retention(store)
}

/// Delete entries beyond the retention policy. Pinned entries are exempt.
pub fn apply_retention(store: &Store) -> Result<(), StoreError> {
    let settings = retention_get(store)?;
    store.with_conn(|conn| {
        if settings.max_age_days > 0 {
            conn.execute(
                "DELETE FROM history_entries
                 WHERE pinned = 0
                   AND sent_at < strftime('%Y-%m-%dT%H:%M:%fZ', 'now', ?1)",
                params![format!("-{} days", settings.max_age_days)],
            )?;
        }
        if settings.max_entries > 0 {
            conn.execute(
                "DELETE FROM history_entries
                 WHERE pinned = 0
                   AND id NOT IN (
                       SELECT id FROM history_entries WHERE pinned = 0
                       ORDER BY id DESC LIMIT ?1
                   )",
                params![settings.max_entries],
            )?;
        }
        Ok(())
    })
}

fn parse_json(s: String) -> serde_json::Value {
    serde_json::from_str(&s).unwrap_or(serde_json::Value::Null)
}

/// Text extraction for the FTS index: capped, and skipped entirely for
/// binary payloads.
fn index_text(bytes: &[u8]) -> Option<String> {
    if bytes.is_empty() || bytes.iter().take(8192).any(|&b| b == 0) {
        return None;
    }
    let capped = &bytes[..bytes.len().min(MAX_INDEX_TEXT)];
    Some(String::from_utf8_lossy(capped).into_owned())
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
