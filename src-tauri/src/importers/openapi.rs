//! OpenAPI 3.x (and Swagger 2 basics) → collection with folders per tag.

use serde_json::Value;

use super::{as_str, spec_default, ImportError, ImportResult};
use crate::collections;
use crate::http_engine::{BodySpec, KeyValue};
use crate::store::Store;

const METHODS: &[&str] = &["get", "post", "put", "patch", "delete", "head", "options"];

pub fn import(store: &Store, root: &Value) -> Result<ImportResult, ImportError> {
    let name = root
        .pointer("/info/title")
        .map(as_str)
        .unwrap_or_else(|| "Imported API".into());
    let collection_id = collections::create(store, &name)?;
    if let Some(desc) = root.pointer("/info/description").map(as_str) {
        collections::update(store, collection_id, None, Some(desc))?;
    }

    let base_url = root
        .pointer("/servers/0/url")
        .map(as_str)
        .unwrap_or_else(|| "{{baseUrl}}".into());
    // Base URL as a collection variable so environments can override it.
    collections::vars_save(
        store,
        "collection",
        Some(collection_id),
        &[collections::Variable {
            key: "baseUrl".into(),
            initial_value: base_url,
            current_value: None,
            is_secret: false,
            enabled: true,
        }],
    )?;

    let mut requests = 0u32;
    let mut folders: Vec<(String, i64)> = Vec::new();

    if let Some(paths) = root.get("paths").and_then(Value::as_object) {
        for (path, ops) in paths {
            let Some(ops) = ops.as_object() else { continue };
            for (method, op) in ops {
                if !METHODS.contains(&method.as_str()) {
                    continue;
                }
                let tag = op
                    .pointer("/tags/0")
                    .map(as_str)
                    .unwrap_or_else(|| "default".into());
                let parent_id = match folders.iter().find(|(t, _)| *t == tag) {
                    Some((_, id)) => Some(*id),
                    None => {
                        let id = collections::item_create(
                            store,
                            collection_id,
                            None,
                            "folder",
                            &tag,
                            None,
                        )?;
                        folders.push((tag.clone(), id));
                        Some(id)
                    }
                };

                let mut spec = spec_default();
                spec.method = method.to_uppercase();
                // {petId} path params → :petId style is Postman's; keep {{}}-free
                // literal path so the user sees exactly what to fill.
                spec.url = format!("{{{{baseUrl}}}}{path}");

                // Query params from parameters list.
                let mut query: Vec<String> = Vec::new();
                if let Some(params) = op.get("parameters").and_then(Value::as_array) {
                    for p in params {
                        if p.get("in").map(as_str).as_deref() == Some("query") {
                            if let Some(pname) = p.get("name").map(as_str) {
                                query.push(format!("{pname}="));
                            }
                        }
                    }
                }
                if !query.is_empty() {
                    spec.url = format!("{}?{}", spec.url, query.join("&"));
                }

                // JSON request body: use the example if present, else {}.
                if let Some(content) = op.pointer("/requestBody/content/application~1json") {
                    let example = content
                        .get("example")
                        .cloned()
                        .or_else(|| content.pointer("/examples/0/value").cloned())
                        .unwrap_or(Value::Object(Default::default()));
                    spec.body = BodySpec::Raw {
                        content_type: "application/json".into(),
                        text: serde_json::to_string_pretty(&example).unwrap_or_default(),
                    };
                    spec.headers.push(KeyValue {
                        key: "Content-Type".into(),
                        value: "application/json".into(),
                        enabled: true,
                    });
                }

                let title = op
                    .get("summary")
                    .or_else(|| op.get("operationId"))
                    .map(as_str)
                    .unwrap_or_else(|| format!("{} {path}", spec.method));
                let item_id = collections::item_create(
                    store,
                    collection_id,
                    parent_id,
                    "request",
                    &title,
                    Some(&spec),
                )?;
                if let Some(desc) = op.get("description").map(as_str) {
                    collections::item_update(store, item_id, None, Some(desc), None)?;
                }
                requests += 1;
            }
        }
    }

    Ok(ImportResult {
        collection_id,
        name,
        requests,
        folders: folders.len() as u32,
        environments: 0,
        variables: 1,
    })
}
