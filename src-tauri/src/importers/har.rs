//! HAR (HTTP Archive) → collection, one request per entry.

use serde_json::Value;

use super::{as_str, spec_default, ImportError, ImportResult};
use crate::collections;
use crate::http_engine::{BodySpec, KeyValue};
use crate::store::Store;

pub fn import(store: &Store, root: &Value) -> Result<ImportResult, ImportError> {
    let entries = root
        .pointer("/log/entries")
        .and_then(Value::as_array)
        .ok_or(ImportError::Unrecognized)?;

    let name = "Imported from HAR".to_string();
    let collection_id = collections::create(store, &name)?;
    let mut requests = 0u32;

    for entry in entries {
        let Some(req) = entry.get("request") else {
            continue;
        };
        let mut spec = spec_default();
        spec.method = req
            .get("method")
            .map(as_str)
            .unwrap_or_else(|| "GET".into());
        spec.url = req.get("url").map(as_str).unwrap_or_default();
        if spec.url.is_empty() {
            continue;
        }

        if let Some(headers) = req.get("headers").and_then(Value::as_array) {
            spec.headers = headers
                .iter()
                .filter_map(|h| {
                    let key = h.get("name").map(as_str)?;
                    // Skip pseudo-headers and hop-by-hop noise from browser HARs.
                    if key.starts_with(':') || key.eq_ignore_ascii_case("content-length") {
                        return None;
                    }
                    Some(KeyValue {
                        key,
                        value: h.get("value").map(as_str).unwrap_or_default(),
                        enabled: true,
                    })
                })
                .collect();
        }

        if let Some(post) = req.get("postData") {
            let mime = post.get("mimeType").map(as_str).unwrap_or_default();
            if let Some(text) = post.get("text").map(as_str) {
                spec.body = BodySpec::Raw {
                    content_type: mime.split(';').next().unwrap_or("text/plain").to_string(),
                    text,
                };
            } else if let Some(params) = post.get("params").and_then(Value::as_array) {
                spec.body = BodySpec::UrlEncoded {
                    fields: params
                        .iter()
                        .filter_map(|p| {
                            Some(KeyValue {
                                key: p.get("name").map(as_str)?,
                                value: p.get("value").map(as_str).unwrap_or_default(),
                                enabled: true,
                            })
                        })
                        .collect(),
                };
            }
        }

        let title = short_title(&spec.method, &spec.url);
        collections::item_create(store, collection_id, None, "request", &title, Some(&spec))?;
        requests += 1;
    }

    Ok(ImportResult {
        collection_id,
        name,
        requests,
        folders: 0,
        environments: 0,
        variables: 0,
    })
}

fn short_title(method: &str, url: &str) -> String {
    match url::Url::parse(url) {
        Ok(u) => format!("{method} {}", u.path()),
        Err(_) => format!("{method} {url}"),
    }
}
