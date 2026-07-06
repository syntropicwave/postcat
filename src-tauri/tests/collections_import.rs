//! Collections CRUD, variable scoping, secret masking in history, and
//! import/export round-trips.
#![allow(clippy::unwrap_used)]

use postcat_lib::collections::{self, Variable};
use postcat_lib::history::{self, SearchFilters};
use postcat_lib::http_engine::{BodySpec, HttpResponseData, KeyValue, RequestSpec};
use postcat_lib::importers;
use postcat_lib::store::Store;
use postcat_lib::vars;

fn var(key: &str, value: &str, secret: bool) -> Variable {
    Variable {
        key: key.into(),
        initial_value: value.into(),
        current_value: None,
        is_secret: secret,
        enabled: true,
    }
}

#[test]
fn collection_tree_crud_and_move() {
    let store = Store::open_in_memory().unwrap();
    let cid = collections::create(&store, "API").unwrap();
    let folder = collections::item_create(&store, cid, None, "folder", "Users", None).unwrap();
    let spec = RequestSpec {
        method: "GET".into(),
        url: "https://a.dev/users".into(),
        headers: vec![],
        body: BodySpec::None,
        settings: Default::default(),
        auth: Default::default(),
    };
    let req = collections::item_create(
        &store,
        cid,
        Some(folder),
        "request",
        "List users",
        Some(&spec),
    )
    .unwrap();
    let req2 =
        collections::item_create(&store, cid, None, "request", "Health", Some(&spec)).unwrap();

    let items = collections::items(&store, cid).unwrap();
    assert_eq!(items.len(), 3);

    // Move "Health" into the folder, before "List users".
    collections::item_move(&store, req2, Some(folder), Some(req)).unwrap();
    let items = collections::items(&store, cid).unwrap();
    let health = items.iter().find(|i| i.id == req2).unwrap();
    assert_eq!(health.parent_id, Some(folder));
    let list = items.iter().find(|i| i.id == req).unwrap();
    assert!(health.sort_order <= list.sort_order);

    // Folder cannot move into itself.
    assert!(collections::item_move(&store, folder, Some(folder), None).is_err());

    // Deleting the folder cascades to children.
    collections::item_delete(&store, folder).unwrap();
    assert_eq!(collections::items(&store, cid).unwrap().len(), 0);
}

#[test]
fn variable_scope_precedence() {
    let store = Store::open_in_memory().unwrap();
    let cid = collections::create(&store, "C").unwrap();
    let eid = collections::env_create(&store, "prod").unwrap();

    collections::vars_save(&store, "global", None, &[var("host", "global.dev", false)]).unwrap();
    collections::vars_save(
        &store,
        "collection",
        Some(cid),
        &[var("host", "collection.dev", false)],
    )
    .unwrap();
    collections::vars_save(
        &store,
        "environment",
        Some(eid),
        &[var("host", "env.dev", false)],
    )
    .unwrap();

    // No env active: collection beats global.
    let effective = collections::effective_vars(&store, Some(cid)).unwrap();
    assert_eq!(
        effective
            .iter()
            .find(|v| v.key == "host")
            .unwrap()
            .initial_value,
        "collection.dev"
    );

    // Active environment wins over everything.
    collections::env_set_active(&store, Some(eid)).unwrap();
    let effective = collections::effective_vars(&store, Some(cid)).unwrap();
    assert_eq!(
        effective
            .iter()
            .find(|v| v.key == "host")
            .unwrap()
            .initial_value,
        "env.dev"
    );

    // Without collection scope, env still beats global.
    let effective = collections::effective_vars(&store, None).unwrap();
    assert_eq!(
        effective
            .iter()
            .find(|v| v.key == "host")
            .unwrap()
            .initial_value,
        "env.dev"
    );
}

#[test]
fn secrets_never_reach_history_or_index() {
    let store = Store::open_in_memory().unwrap();
    collections::vars_save(
        &store,
        "global",
        None,
        &[var("apikey", "sk-live-abc123xyz", true)],
    )
    .unwrap();

    let original = RequestSpec {
        method: "GET".into(),
        url: "https://api.dev/data?key={{apikey}}".into(),
        headers: vec![KeyValue {
            key: "X-Key".into(),
            value: "{{apikey}}".into(),
            enabled: true,
        }],
        body: BodySpec::None,
        settings: Default::default(),
        auth: Default::default(),
    };

    let resolution = vars::resolve(&store, &original, None).unwrap();
    assert!(resolution.spec.url.contains("sk-live-abc123xyz"));
    let display = vars::mask_secrets(&resolution.spec, &resolution.secrets);

    // Response echoes the secret back — index text must be scrubbed too.
    let resp = HttpResponseData {
        status: 200,
        status_text: "OK".into(),
        http_version: "HTTP/1.1".into(),
        headers: vec![],
        body: br#"{"granted_to":"sk-live-abc123xyz"}"#.to_vec(),
        body_truncated: false,
        size: 34,
        duration_ms: 1.0,
        ttfb_ms: 1.0,
    };
    let id = history::record(&store, &original, &display, &resolution.secrets, Ok(&resp)).unwrap();

    // The secret VALUE is not searchable anywhere.
    let by = |q: &str| {
        history::search(
            &store,
            &SearchFilters {
                query: Some(q.into()),
                ..Default::default()
            },
            10,
            0,
        )
        .unwrap()
        .len()
    };
    assert_eq!(by("abc123xyz"), 0);
    // But the entry is findable by the variable name (mask leaves {{apikey}}).
    assert!(by("apikey") >= 1);

    // Display fields are masked; replay spec keeps the {{placeholder}}.
    let detail = history::get(&store, id).unwrap();
    assert!(!detail.summary.url.contains("sk-live"));
    assert!(detail.summary.url.contains("{{apikey}}"));
    assert_eq!(
        detail.req_spec["url"],
        "https://api.dev/data?key={{apikey}}"
    );
}

#[test]
fn postman_v21_import_export_roundtrip() {
    let store = Store::open_in_memory().unwrap();
    let source = r##"{
      "info": {
        "name": "Petstore",
        "schema": "https://schema.getpostman.com/json/collection/v2.1.0/collection.json"
      },
      "item": [
        {
          "name": "Pets",
          "item": [
            {
              "name": "Create pet",
              "request": {
                "method": "POST",
                "url": {"raw": "{{baseUrl}}/pets"},
                "header": [{"key": "X-Trace", "value": "1"}],
                "auth": {"type": "bearer", "bearer": [{"key": "token", "value": "{{token}}"}]},
                "body": {
                  "mode": "raw",
                  "raw": "{\"name\":\"Cat\"}",
                  "options": {"raw": {"language": "json"}}
                }
              }
            },
            {
              "name": "Search",
              "request": {
                "method": "GET",
                "url": {"raw": "{{baseUrl}}/pets?q=cat"}
              }
            }
          ]
        }
      ],
      "variable": [
        {"key": "baseUrl", "value": "https://petstore.dev"},
        {"key": "token", "value": "", "type": "secret"}
      ]
    }"##;

    let result = importers::import_auto(&store, source).unwrap();
    assert_eq!(result.requests, 2);
    assert_eq!(result.folders, 1);
    assert_eq!(result.variables, 2);

    let items = collections::items(&store, result.collection_id).unwrap();
    let create = items.iter().find(|i| i.name == "Create pet").unwrap();
    let spec: RequestSpec = serde_json::from_value(create.req_spec.clone().unwrap()).unwrap();
    assert_eq!(spec.method, "POST");
    assert_eq!(spec.url, "{{baseUrl}}/pets");
    // bearer auth became a header
    assert!(spec
        .headers
        .iter()
        .any(|h| h.key == "Authorization" && h.value == "Bearer {{token}}"));
    match &spec.body {
        BodySpec::Raw { content_type, text } => {
            assert_eq!(content_type, "application/json");
            assert!(text.contains("Cat"));
        }
        other => panic!("unexpected body {other:?}"),
    }

    // Round-trip: export and re-import.
    let exported = importers::export_postman(&store, result.collection_id).unwrap();
    let round = importers::import_auto(&store, &exported).unwrap();
    assert_eq!(round.requests, 2);
    assert_eq!(round.folders, 1);

    // Secret variable exports with an empty value.
    let doc: serde_json::Value = serde_json::from_str(&exported).unwrap();
    let token = doc["variable"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["key"] == "token")
        .unwrap();
    assert_eq!(token["value"], "");
}

#[test]
fn postman_environment_import() {
    let store = Store::open_in_memory().unwrap();
    let source = r#"{
      "name": "Staging",
      "values": [
        {"key": "baseUrl", "value": "https://staging.dev", "enabled": true},
        {"key": "apiKey", "value": "s3cr3t", "type": "secret"}
      ]
    }"#;
    let result = importers::import_auto(&store, source).unwrap();
    assert_eq!(result.environments, 1);
    assert_eq!(result.variables, 2);

    let envs = collections::env_list(&store).unwrap();
    let env = envs.iter().find(|e| e.name == "Staging").unwrap();
    let vars = collections::vars_get(&store, "environment", Some(env.id)).unwrap();
    assert!(vars.iter().any(|v| v.key == "apiKey" && v.is_secret));
}

#[test]
fn openapi_yaml_import() {
    let store = Store::open_in_memory().unwrap();
    let source = r#"
openapi: 3.0.3
info:
  title: Tiny API
servers:
  - url: https://tiny.dev/v1
paths:
  /users:
    get:
      tags: [users]
      summary: List users
      parameters:
        - name: limit
          in: query
    post:
      tags: [users]
      summary: Create user
      requestBody:
        content:
          application/json:
            example: {"name": "Ann"}
"#;
    let result = importers::import_auto(&store, source).unwrap();
    assert_eq!(result.requests, 2);
    assert_eq!(result.folders, 1);

    let items = collections::items(&store, result.collection_id).unwrap();
    let list = items.iter().find(|i| i.name == "List users").unwrap();
    let spec: RequestSpec = serde_json::from_value(list.req_spec.clone().unwrap()).unwrap();
    assert_eq!(spec.url, "{{baseUrl}}/users?limit=");

    // baseUrl became a collection variable.
    let vars = collections::vars_get(&store, "collection", Some(result.collection_id)).unwrap();
    assert_eq!(vars[0].key, "baseUrl");
    assert_eq!(vars[0].initial_value, "https://tiny.dev/v1");
}

#[test]
fn har_import() {
    let store = Store::open_in_memory().unwrap();
    let source = r#"{
      "log": {
        "entries": [
          {
            "request": {
              "method": "POST",
              "url": "https://api.dev/login",
              "headers": [
                {"name": "content-type", "value": "application/json"},
                {"name": "content-length", "value": "17"}
              ],
              "postData": {"mimeType": "application/json", "text": "{\"user\":\"ann\"}"}
            }
          }
        ]
      }
    }"#;
    let result = importers::import_auto(&store, source).unwrap();
    assert_eq!(result.requests, 1);

    let items = collections::items(&store, result.collection_id).unwrap();
    let spec: RequestSpec = serde_json::from_value(items[0].req_spec.clone().unwrap()).unwrap();
    assert_eq!(spec.method, "POST");
    // content-length dropped, content-type kept
    assert_eq!(spec.headers.len(), 1);
}
