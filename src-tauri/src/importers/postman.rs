//! Postman Collection v2.1 import/export and environment import.

use serde_json::{json, Value};

use super::{as_str, spec_default, ImportError, ImportResult};
use crate::collections::{self, Variable};
use crate::http_engine::{BodySpec, FormField, KeyValue, RequestSpec};
use crate::store::Store;

pub fn import_collection(store: &Store, root: &Value) -> Result<ImportResult, ImportError> {
    let name = root
        .pointer("/info/name")
        .map(as_str)
        .unwrap_or_else(|| "Imported collection".into());
    let collection_id = collections::create(store, &name)?;
    if let Some(desc) = root.pointer("/info/description") {
        collections::update(store, collection_id, None, Some(description_text(desc)))?;
    }

    let mut counters = (0u32, 0u32); // (requests, folders)
    if let Some(items) = root.get("item").and_then(Value::as_array) {
        import_items(store, collection_id, None, items, &mut counters)?;
    }

    // Collection-level variables.
    let mut variables = 0u32;
    if let Some(vars) = root.get("variable").and_then(Value::as_array) {
        let list: Vec<Variable> = vars.iter().filter_map(parse_variable).collect();
        variables = list.len() as u32;
        collections::vars_save(store, "collection", Some(collection_id), &list)?;
    }

    Ok(ImportResult {
        collection_id,
        name,
        requests: counters.0,
        folders: counters.1,
        environments: 0,
        variables,
    })
}

pub fn import_environment(store: &Store, root: &Value) -> Result<ImportResult, ImportError> {
    let name = root
        .get("name")
        .map(as_str)
        .unwrap_or_else(|| "Imported environment".into());
    let env_id = collections::env_create(store, &name)?;
    let list: Vec<Variable> = root
        .get("values")
        .and_then(Value::as_array)
        .map(|vals| vals.iter().filter_map(parse_variable).collect())
        .unwrap_or_default();
    collections::vars_save(store, "environment", Some(env_id), &list)?;
    Ok(ImportResult {
        collection_id: 0,
        name,
        requests: 0,
        folders: 0,
        environments: 1,
        variables: list.len() as u32,
    })
}

fn parse_variable(v: &Value) -> Option<Variable> {
    let key = v.get("key").map(as_str)?;
    if key.is_empty() {
        return None;
    }
    Some(Variable {
        key,
        initial_value: v.get("value").map(as_str).unwrap_or_default(),
        current_value: None,
        is_secret: v.get("type").map(as_str).as_deref() == Some("secret"),
        enabled: v.get("disabled").and_then(Value::as_bool) != Some(true),
    })
}

fn import_items(
    store: &Store,
    collection_id: i64,
    parent_id: Option<i64>,
    items: &[Value],
    counters: &mut (u32, u32),
) -> Result<(), ImportError> {
    for item in items {
        let name = item.get("name").map(as_str).unwrap_or_else(|| "?".into());
        if let Some(children) = item.get("item").and_then(Value::as_array) {
            // Folder.
            let folder_id =
                collections::item_create(store, collection_id, parent_id, "folder", &name, None)?;
            if let Some(desc) = item.get("description") {
                collections::item_update(
                    store,
                    folder_id,
                    None,
                    Some(description_text(desc)),
                    None,
                )?;
            }
            counters.1 += 1;
            import_items(store, collection_id, Some(folder_id), children, counters)?;
        } else if let Some(request) = item.get("request") {
            let spec = parse_request(request);
            let item_id = collections::item_create(
                store,
                collection_id,
                parent_id,
                "request",
                &name,
                Some(&spec),
            )?;
            if let Some(desc) = request.get("description") {
                collections::item_update(store, item_id, None, Some(description_text(desc)), None)?;
            }
            counters.0 += 1;
        }
    }
    Ok(())
}

fn parse_request(request: &Value) -> RequestSpec {
    let mut spec = spec_default();

    // A request may be a bare URL string.
    if let Value::String(url) = request {
        spec.url = url.clone();
        return spec;
    }

    spec.method = request
        .get("method")
        .map(as_str)
        .unwrap_or_else(|| "GET".into());
    spec.url = match request.get("url") {
        Some(Value::String(s)) => s.clone(),
        Some(obj) => obj.get("raw").map(as_str).unwrap_or_default(),
        None => String::new(),
    };

    if let Some(headers) = request.get("header").and_then(Value::as_array) {
        spec.headers = headers
            .iter()
            .filter_map(|h| {
                Some(KeyValue {
                    key: h.get("key").map(as_str)?,
                    value: h.get("value").map(as_str).unwrap_or_default(),
                    enabled: h.get("disabled").and_then(Value::as_bool) != Some(true),
                })
            })
            .collect();
    }

    // Auth helpers → headers (basic/bearer/apikey subset).
    if let Some(auth) = request.get("auth") {
        apply_auth(&mut spec, auth);
    }

    if let Some(body) = request.get("body") {
        spec.body = parse_body(body);
    }
    spec
}

fn apply_auth(spec: &mut RequestSpec, auth: &Value) {
    let kind = auth.get("type").map(as_str).unwrap_or_default();
    let param = |section: &str, key: &str| -> Option<String> {
        auth.get(section)?
            .as_array()?
            .iter()
            .find_map(|p| (p.get("key").map(as_str)? == key).then(|| p.get("value").map(as_str))?)
    };
    match kind.as_str() {
        "bearer" => {
            if let Some(token) = param("bearer", "token") {
                spec.headers.push(KeyValue {
                    key: "Authorization".into(),
                    value: format!("Bearer {token}"),
                    enabled: true,
                });
            }
        }
        "apikey" => {
            let key = param("apikey", "key").unwrap_or_default();
            let value = param("apikey", "value").unwrap_or_default();
            if param("apikey", "in").as_deref() == Some("query") {
                let sep = if spec.url.contains('?') { '&' } else { '?' };
                spec.url = format!("{}{}{}={}", spec.url, sep, key, value);
            } else if !key.is_empty() {
                spec.headers.push(KeyValue {
                    key,
                    value,
                    enabled: true,
                });
            }
        }
        // basic with resolved user/pass would need base64 at send time with
        // vars — imported as a placeholder header the user can adjust.
        "basic" => {
            let user = param("basic", "username").unwrap_or_default();
            let pass = param("basic", "password").unwrap_or_default();
            use base64::Engine;
            let b64 = base64::engine::general_purpose::STANDARD.encode(format!("{user}:{pass}"));
            spec.headers.push(KeyValue {
                key: "Authorization".into(),
                value: format!("Basic {b64}"),
                enabled: true,
            });
        }
        _ => {}
    }
}

fn parse_body(body: &Value) -> BodySpec {
    match body.get("mode").map(as_str).as_deref() {
        Some("raw") => {
            let text = body.get("raw").map(as_str).unwrap_or_default();
            let lang = body
                .pointer("/options/raw/language")
                .map(as_str)
                .unwrap_or_else(|| "json".into());
            let content_type = match lang.as_str() {
                "json" => "application/json",
                "xml" => "application/xml",
                "html" => "text/html",
                _ => "text/plain",
            };
            BodySpec::Raw {
                content_type: content_type.into(),
                text,
            }
        }
        Some("urlencoded") => BodySpec::UrlEncoded {
            fields: kv_fields(body.get("urlencoded")),
        },
        Some("formdata") => BodySpec::FormData {
            fields: body
                .get("formdata")
                .and_then(Value::as_array)
                .map(|arr| {
                    arr.iter()
                        .filter_map(|f| {
                            Some(FormField {
                                key: f.get("key").map(as_str)?,
                                is_file: f.get("type").map(as_str).as_deref() == Some("file"),
                                value: f
                                    .get("src")
                                    .or_else(|| f.get("value"))
                                    .map(as_str)
                                    .unwrap_or_default(),
                                enabled: f.get("disabled").and_then(Value::as_bool) != Some(true),
                            })
                        })
                        .collect()
                })
                .unwrap_or_default(),
        },
        Some("file") => BodySpec::Binary {
            path: body.pointer("/file/src").map(as_str).unwrap_or_default(),
        },
        Some("graphql") => BodySpec::Graphql {
            query: body
                .pointer("/graphql/query")
                .map(as_str)
                .unwrap_or_default(),
            variables: body
                .pointer("/graphql/variables")
                .map(as_str)
                .unwrap_or_default(),
        },
        _ => BodySpec::None,
    }
}

fn kv_fields(v: Option<&Value>) -> Vec<KeyValue> {
    v.and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(|f| {
                    Some(KeyValue {
                        key: f.get("key").map(as_str)?,
                        value: f.get("value").map(as_str).unwrap_or_default(),
                        enabled: f.get("disabled").and_then(Value::as_bool) != Some(true),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

fn description_text(desc: &Value) -> String {
    match desc {
        Value::String(s) => s.clone(),
        obj => obj.get("content").map(as_str).unwrap_or_default(),
    }
}

/* ---------------- export ---------------- */

pub fn export_collection(store: &Store, collection_id: i64) -> Result<String, ImportError> {
    let meta = collections::list(store)?
        .into_iter()
        .find(|c| c.id == collection_id)
        .ok_or_else(|| ImportError::Parse("collection not found".into()))?;
    let items = collections::items(store, collection_id)?;
    let vars = collections::vars_get(store, "collection", Some(collection_id))?;

    let tree = build_items(&items, None);
    let variable: Vec<Value> = vars
        .iter()
        .map(|v| {
            json!({
                "key": v.key,
                // Secrets export with empty values — they never leave the machine.
                "value": if v.is_secret { "" } else { &v.initial_value },
                "type": if v.is_secret { "secret" } else { "default" },
            })
        })
        .collect();

    let doc = json!({
        "info": {
            "name": meta.name,
            "description": meta.description,
            "schema": "https://schema.getpostman.com/json/collection/v2.1.0/collection.json",
        },
        "item": tree,
        "variable": variable,
    });
    serde_json::to_string_pretty(&doc).map_err(|e| ImportError::Parse(e.to_string()))
}

fn build_items(all: &[collections::CollectionItem], parent: Option<i64>) -> Vec<Value> {
    all.iter()
        .filter(|i| i.parent_id == parent)
        .map(|i| {
            if i.kind == "folder" {
                json!({
                    "name": i.name,
                    "description": i.description,
                    "item": build_items(all, Some(i.id)),
                })
            } else {
                let spec: RequestSpec = i
                    .req_spec
                    .clone()
                    .and_then(|v| serde_json::from_value(v).ok())
                    .unwrap_or_else(spec_default);
                json!({
                    "name": i.name,
                    "request": export_request(&spec, &i.description),
                })
            }
        })
        .collect()
}

fn export_request(spec: &RequestSpec, description: &str) -> Value {
    let headers: Vec<Value> = spec
        .headers
        .iter()
        .map(|h| {
            json!({
                "key": h.key,
                "value": h.value,
                "disabled": !h.enabled,
            })
        })
        .collect();

    let body = match &spec.body {
        BodySpec::None => Value::Null,
        BodySpec::Raw { content_type, text } => json!({
            "mode": "raw",
            "raw": text,
            "options": {"raw": {"language": match content_type.as_str() {
                "application/json" => "json",
                "application/xml" => "xml",
                "text/html" => "html",
                _ => "text",
            }}},
        }),
        BodySpec::UrlEncoded { fields } => json!({
            "mode": "urlencoded",
            "urlencoded": fields.iter().map(|f| json!({
                "key": f.key, "value": f.value, "disabled": !f.enabled,
            })).collect::<Vec<_>>(),
        }),
        BodySpec::FormData { fields } => json!({
            "mode": "formdata",
            "formdata": fields.iter().map(|f| if f.is_file {
                json!({"key": f.key, "type": "file", "src": f.value, "disabled": !f.enabled})
            } else {
                json!({"key": f.key, "type": "text", "value": f.value, "disabled": !f.enabled})
            }).collect::<Vec<_>>(),
        }),
        BodySpec::Binary { path } => json!({
            "mode": "file",
            "file": {"src": path},
        }),
        BodySpec::Graphql { query, variables } => json!({
            "mode": "graphql",
            "graphql": {"query": query, "variables": variables},
        }),
    };

    let mut req = json!({
        "method": spec.method,
        "url": {"raw": spec.url},
        "header": headers,
        "description": description,
    });
    if body != Value::Null {
        req["body"] = body;
    }
    req
}
