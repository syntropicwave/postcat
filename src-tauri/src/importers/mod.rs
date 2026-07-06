//! Import (Postman v2.1, cURL, OpenAPI 3.x, HAR) and export (Postman v2.1).
//!
//! Importers are Value-walkers, not strict schema types: real-world files are
//! messy and a best-effort import that succeeds beats a strict one that fails.

mod curl;
mod har;
mod openapi;
mod postman;

pub use curl::parse_curl;

use serde_json::Value;

use crate::collections;
use crate::http_engine::RequestSpec;
use crate::store::{Store, StoreError};

#[derive(Debug, serde::Serialize)]
pub struct ImportResult {
    pub collection_id: i64,
    pub name: String,
    pub requests: u32,
    pub folders: u32,
    pub environments: u32,
    pub variables: u32,
}

#[derive(Debug, thiserror::Error)]
pub enum ImportError {
    #[error("unrecognized format: expected Postman collection/environment, OpenAPI or HAR")]
    Unrecognized,
    #[error("invalid JSON/YAML: {0}")]
    Parse(String),
    #[error("{0}")]
    Store(#[from] StoreError),
}

/// Auto-detect the payload format and import it.
pub fn import_auto(store: &Store, text: &str) -> Result<ImportResult, ImportError> {
    let trimmed = text.trim_start();
    if trimmed.starts_with("curl ") || trimmed.starts_with("curl.exe ") {
        // cURL imports become a single request in an "Imported" collection.
        let spec = parse_curl(text).map_err(ImportError::Parse)?;
        let collection_id = collections::create(store, "Imported from cURL")?;
        collections::item_create(
            store,
            collection_id,
            None,
            "request",
            &spec.url,
            Some(&spec),
        )?;
        return Ok(ImportResult {
            collection_id,
            name: "Imported from cURL".into(),
            requests: 1,
            folders: 0,
            environments: 0,
            variables: 0,
        });
    }

    let value: Value = if trimmed.starts_with('{') || trimmed.starts_with('[') {
        serde_json::from_str(text).map_err(|e| ImportError::Parse(e.to_string()))?
    } else {
        serde_yaml::from_str(text).map_err(|e| ImportError::Parse(e.to_string()))?
    };

    if value.pointer("/info/schema").is_some() || value.get("item").is_some() {
        return postman::import_collection(store, &value);
    }
    if value.get("values").is_some() && value.get("name").is_some() && value.get("item").is_none() {
        return postman::import_environment(store, &value);
    }
    if value.get("openapi").is_some() || value.get("swagger").is_some() {
        return openapi::import(store, &value);
    }
    if value.pointer("/log/entries").is_some() {
        return har::import(store, &value);
    }
    Err(ImportError::Unrecognized)
}

/// Export a collection as Postman Collection v2.1 JSON.
pub fn export_postman(store: &Store, collection_id: i64) -> Result<String, ImportError> {
    postman::export_collection(store, collection_id)
}

/* ------------ shared helpers for importers ------------ */

pub(crate) fn spec_default() -> RequestSpec {
    RequestSpec {
        method: "GET".into(),
        url: String::new(),
        headers: vec![],
        body: crate::http_engine::BodySpec::None,
        settings: Default::default(),
    }
}

pub(crate) fn as_str(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}
