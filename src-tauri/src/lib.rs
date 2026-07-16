pub mod auth;
pub mod collections;
pub mod cookies;
pub mod crypto;
pub mod history;
pub mod host_aliases;
pub mod http_engine;
pub mod importers;
pub mod runner;
pub mod scripting;
pub mod settings;
pub mod store;
pub mod sync;
pub mod vars;
pub mod websocket;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;

use tauri::Manager;
use tauri_plugin_opener::OpenerExt;

use auth::AuthSpec;
use http_engine::RequestSpec;
use store::Store;

/// In-flight request cancellation handles, keyed by frontend-generated id.
#[derive(Default)]
struct InflightRequests(Mutex<HashMap<String, tokio::sync::oneshot::Sender<()>>>);

#[derive(serde::Serialize)]
struct AppInfo {
    version: String,
    db_path: String,
    schema_version: i64,
}

#[derive(serde::Serialize)]
struct SendResult {
    history_id: i64,
    status: u16,
    status_text: String,
    http_version: String,
    headers: Vec<(String, String)>,
    body_text: Option<String>,
    body_base64: Option<String>,
    body_truncated: bool,
    size: usize,
    duration_ms: f64,
    ttfb_ms: f64,
    timings: http_engine::Timings,
    tests: Vec<scripting::TestResult>,
    console: Vec<scripting::ConsoleLine>,
    script_error: Option<String>,
}

/// Cancellation flags for collection runs, keyed by collection id.
#[derive(Default)]
struct RunnerCancels(Mutex<HashMap<i64, std::sync::Arc<std::sync::atomic::AtomicBool>>>);

#[tauri::command]
fn app_info(app: tauri::AppHandle, store: tauri::State<'_, Store>) -> Result<AppInfo, String> {
    let schema_version = store.schema_version().map_err(|e| e.to_string())?;
    Ok(AppInfo {
        version: app.package_info().version.to_string(),
        db_path: db_path(&app)
            .map_err(|e| e.to_string())?
            .display()
            .to_string(),
        schema_version,
    })
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
async fn send_request(
    app: tauri::AppHandle,
    store: tauri::State<'_, Store>,
    jar: tauri::State<'_, cookies::Cookies>,
    inflight: tauri::State<'_, InflightRequests>,
    request_id: String,
    spec: RequestSpec,
    collection_id: Option<i64>,
    item_id: Option<i64>,
    pre_request_script: Option<String>,
    test_script: Option<String>,
) -> Result<SendResult, http_engine::SendError> {
    let app_settings = settings::get(&store).map_err(|e| e.to_string())?;

    // Script chains: collection -> folders (from the tree) + the tab's own.
    let (mut pre_chain, mut test_chain) =
        scripting::chain_scripts(&store, collection_id, item_id).map_err(|e| e.to_string())?;
    if let Some(p) = pre_request_script.filter(|s| !s.trim().is_empty()) {
        pre_chain.push(p);
    }
    if let Some(t) = test_script.filter(|s| !s.trim().is_empty()) {
        test_chain.push(t);
    }

    let mut var_list =
        collections::effective_vars(&store, collection_id).map_err(|e| e.to_string())?;
    let mut vars_map: HashMap<String, String> = var_list
        .iter()
        .filter(|v| v.enabled)
        .map(|v| (v.key.clone(), v.effective_value().to_owned()))
        .collect();

    let mut tests: Vec<scripting::TestResult> = Vec::new();
    let mut console: Vec<scripting::ConsoleLine> = Vec::new();
    let mut script_error: Option<String> = None;

    // Pre-request scripts mutate the unresolved spec and the vars map.
    let mut working_spec = spec.clone();
    if !pre_chain.is_empty() {
        let send_fn = scripting::blocking_send(app_settings.clone());
        for script in &pre_chain {
            let input = scripting::ScriptInput {
                request: working_spec.clone(),
                response: None,
                vars: vars_map.clone(),
                data: None,
                iteration: 0,
                iteration_count: 1,
                request_name: String::new(),
            };
            let script = script.clone();
            let send = send_fn.clone();
            let out =
                tokio::task::spawn_blocking(move || scripting::execute(&script, &input, send))
                    .await
                    .unwrap_or_default();
            console.extend(out.console);
            tests.extend(out.tests);
            if let Some(req) = out.request {
                working_spec = req;
            }
            let _ = scripting::apply_var_ops(&store, collection_id, &out.var_ops, &mut vars_map);
            if let Some(e) = out.error {
                script_error = Some(format!("pre-request: {e}"));
                break;
            }
        }
        for (k, v) in &vars_map {
            vars::upsert_var(&mut var_list, k, v);
        }
    }

    // Resolve {{vars}}; the ORIGINAL spec is what history replays.
    let resolution = vars::resolve_with(&working_spec, &var_list);
    let mut secrets = resolution.secrets;
    let mut resolved = resolution.spec;

    // Effective auth: explicit on the request, or inherited from the tree.
    let mut effective = auth::effective_auth(&store, &resolved.auth, item_id, collection_id)
        .map_err(|e| e.to_string())?;
    if let AuthSpec::Oauth2(cfg) = &effective {
        if cfg.is_expired() && !cfg.refresh_token.is_empty() {
            if let Ok(token) = auth::oauth2::refresh_token(cfg).await {
                let mut updated = cfg.clone();
                updated.access_token = token.access_token;
                if !token.refresh_token.is_empty() {
                    updated.refresh_token = token.refresh_token;
                }
                updated.expires_at = token.expires_at;
                effective = AuthSpec::Oauth2(updated);
            }
        }
    }
    secrets.extend(auth::apply(&mut resolved, &effective));

    let display = vars::mask_secrets(&resolved, &secrets);

    let (cancel_tx, cancel_rx) = tokio::sync::oneshot::channel::<()>();
    if let Ok(mut map) = inflight.0.lock() {
        map.insert(request_id.clone(), cancel_tx);
    }

    // Live streaming (SSE): forward chunks to the UI as they arrive.
    let stream_cb: http_engine::StreamChunkFn = {
        use tauri::Emitter;
        let app = app.clone();
        let event = format!("stream:{request_id}");
        std::sync::Arc::new(move |chunk: &str| {
            let _ = app.emit(&event, chunk);
        })
    };

    let outcome = tokio::select! {
        res = http_engine::execute_streaming(jar.store.clone(), &resolved, &app_settings, Some(stream_cb)) => Some(res),
        _ = cancel_rx => None,
    };

    if let Ok(mut map) = inflight.0.lock() {
        map.remove(&request_id);
    }
    jar.save();

    match outcome {
        None => {
            // Cancelled by the user: record the attempt, report as error.
            let _ = history::record(&store, &spec, &display, &secrets, Err("cancelled"));
            Err("cancelled".into())
        }
        Some(Err(err)) => {
            let se = err.into_send_error();
            let _ = history::record(&store, &spec, &display, &secrets, Err(&se.message));
            Err(se)
        }
        Some(Ok(resp)) => {
            let history_id = history::record(&store, &spec, &display, &secrets, Ok(&resp))
                .map_err(|e| e.to_string())?;
            let (body_text, body_base64) = history::body_for_ui(&resp.body);

            // Test scripts run against the response.
            if !test_chain.is_empty() && script_error.is_none() {
                let response_json = serde_json::json!({
                    "status": resp.status,
                    "status_text": resp.status_text,
                    "headers": resp.headers,
                    "body_text": body_text,
                    "duration_ms": resp.duration_ms,
                    "size": resp.size,
                });
                let send_fn = scripting::blocking_send(app_settings.clone());
                for script in &test_chain {
                    let input = scripting::ScriptInput {
                        request: working_spec.clone(),
                        response: Some(response_json.clone()),
                        vars: vars_map.clone(),
                        data: None,
                        iteration: 0,
                        iteration_count: 1,
                        request_name: String::new(),
                    };
                    let script = script.clone();
                    let send = send_fn.clone();
                    let out = tokio::task::spawn_blocking(move || {
                        scripting::execute(&script, &input, send)
                    })
                    .await
                    .unwrap_or_default();
                    console.extend(out.console);
                    tests.extend(out.tests);
                    let _ = scripting::apply_var_ops(
                        &store,
                        collection_id,
                        &out.var_ops,
                        &mut vars_map,
                    );
                    if let Some(e) = out.error {
                        script_error = Some(format!("tests: {e}"));
                        break;
                    }
                }
            }

            Ok(SendResult {
                history_id,
                status: resp.status,
                status_text: resp.status_text,
                http_version: resp.http_version,
                headers: resp.headers,
                body_text,
                body_base64,
                body_truncated: resp.body_truncated,
                size: resp.size,
                duration_ms: resp.duration_ms,
                ttfb_ms: resp.ttfb_ms,
                timings: resp.timings.clone(),
                tests,
                console,
                script_error,
            })
        }
    }
}

#[tauri::command]
fn cancel_request(inflight: tauri::State<'_, InflightRequests>, request_id: String) {
    if let Ok(mut map) = inflight.0.lock() {
        if let Some(tx) = map.remove(&request_id) {
            let _ = tx.send(());
        }
    }
}

#[tauri::command]
fn history_search(
    store: tauri::State<'_, Store>,
    filters: history::SearchFilters,
    limit: Option<u32>,
    offset: Option<u32>,
) -> Result<Vec<history::HistorySummary>, String> {
    history::search(
        &store,
        &filters,
        limit.unwrap_or(100).min(500),
        offset.unwrap_or(0),
    )
    .map_err(|e| e.to_string())
}

#[tauri::command]
fn history_endpoints(
    store: tauri::State<'_, Store>,
    limit: Option<u32>,
) -> Result<Vec<history::EndpointGroup>, String> {
    history::endpoints(&store, limit.unwrap_or(200).min(1000)).map_err(|e| e.to_string())
}

#[tauri::command]
fn history_set_pinned(store: tauri::State<'_, Store>, id: i64, pinned: bool) -> Result<(), String> {
    history::set_pinned(&store, id, pinned).map_err(|e| e.to_string())
}

#[tauri::command]
fn history_set_label(
    store: tauri::State<'_, Store>,
    id: i64,
    label: Option<String>,
) -> Result<(), String> {
    history::set_label(&store, id, label).map_err(|e| e.to_string())
}

#[tauri::command]
fn retention_get(store: tauri::State<'_, Store>) -> Result<history::RetentionSettings, String> {
    history::retention_get(&store).map_err(|e| e.to_string())
}

#[tauri::command]
fn retention_set(
    store: tauri::State<'_, Store>,
    settings: history::RetentionSettings,
) -> Result<(), String> {
    history::retention_set(&store, settings).map_err(|e| e.to_string())
}

#[tauri::command]
fn history_get(store: tauri::State<'_, Store>, id: i64) -> Result<history::HistoryDetail, String> {
    history::get(&store, id).map_err(|e| e.to_string())
}

#[tauri::command]
fn history_delete(store: tauri::State<'_, Store>, id: i64) -> Result<(), String> {
    history::delete(&store, id).map_err(|e| e.to_string())
}

#[tauri::command]
fn history_clear(store: tauri::State<'_, Store>) -> Result<(), String> {
    history::clear(&store).map_err(|e| e.to_string())
}

/* ---------------- collections ---------------- */

#[tauri::command]
fn collections_list(
    store: tauri::State<'_, Store>,
) -> Result<Vec<collections::Collection>, String> {
    collections::list(&store).map_err(|e| e.to_string())
}

#[tauri::command]
fn collection_create(store: tauri::State<'_, Store>, name: String) -> Result<i64, String> {
    collections::create(&store, &name).map_err(|e| e.to_string())
}

#[tauri::command]
fn collection_update(
    store: tauri::State<'_, Store>,
    id: i64,
    name: Option<String>,
    description: Option<String>,
) -> Result<(), String> {
    collections::update(&store, id, name, description).map_err(|e| e.to_string())
}

#[tauri::command]
fn collection_delete(store: tauri::State<'_, Store>, id: i64) -> Result<(), String> {
    collections::delete(&store, id).map_err(|e| e.to_string())
}

#[tauri::command]
fn collection_items(
    store: tauri::State<'_, Store>,
    collection_id: i64,
) -> Result<Vec<collections::CollectionItem>, String> {
    collections::items(&store, collection_id).map_err(|e| e.to_string())
}

#[tauri::command]
fn item_create(
    store: tauri::State<'_, Store>,
    collection_id: i64,
    parent_id: Option<i64>,
    kind: String,
    name: String,
    spec: Option<RequestSpec>,
) -> Result<i64, String> {
    collections::item_create(
        &store,
        collection_id,
        parent_id,
        &kind,
        &name,
        spec.as_ref(),
    )
    .map_err(|e| e.to_string())
}

#[tauri::command]
fn item_update(
    store: tauri::State<'_, Store>,
    id: i64,
    name: Option<String>,
    description: Option<String>,
    spec: Option<RequestSpec>,
) -> Result<(), String> {
    collections::item_update(&store, id, name, description, spec.as_ref())
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn item_move(
    store: tauri::State<'_, Store>,
    id: i64,
    new_parent_id: Option<i64>,
    before_id: Option<i64>,
) -> Result<(), String> {
    collections::item_move(&store, id, new_parent_id, before_id).map_err(|e| e.to_string())
}

#[tauri::command]
fn item_delete(store: tauri::State<'_, Store>, id: i64) -> Result<(), String> {
    collections::item_delete(&store, id).map_err(|e| e.to_string())
}

#[tauri::command]
fn item_duplicate(store: tauri::State<'_, Store>, id: i64) -> Result<i64, String> {
    collections::item_duplicate(&store, id).map_err(|e| e.to_string())
}

#[tauri::command]
fn env_export_file(store: tauri::State<'_, Store>, id: i64, path: String) -> Result<(), String> {
    let json = collections::env_export(&store, id).map_err(|e| e.to_string())?;
    std::fs::write(&path, json).map_err(|e| e.to_string())
}

#[tauri::command]
fn env_duplicate(store: tauri::State<'_, Store>, id: i64) -> Result<i64, String> {
    collections::env_duplicate(&store, id).map_err(|e| e.to_string())
}

/// Write a history entry's response body to a file (full, uncapped).
#[tauri::command]
fn history_save_body(store: tauri::State<'_, Store>, id: i64, path: String) -> Result<(), String> {
    let body: Option<Vec<u8>> = store
        .with_conn(|conn| {
            conn.query_row(
                "SELECT resp_body FROM history_entries WHERE id = ?1",
                rusqlite::params![id],
                |row| row.get(0),
            )
        })
        .map_err(|e| e.to_string())?;
    std::fs::write(&path, body.unwrap_or_default()).map_err(|e| e.to_string())
}

/* ---------------- environments & variables ---------------- */

#[tauri::command]
fn env_list(store: tauri::State<'_, Store>) -> Result<Vec<collections::Environment>, String> {
    collections::env_list(&store).map_err(|e| e.to_string())
}

#[tauri::command]
fn env_create(store: tauri::State<'_, Store>, name: String) -> Result<i64, String> {
    collections::env_create(&store, &name).map_err(|e| e.to_string())
}

#[tauri::command]
fn env_rename(store: tauri::State<'_, Store>, id: i64, name: String) -> Result<(), String> {
    collections::env_rename(&store, id, &name).map_err(|e| e.to_string())
}

#[tauri::command]
fn env_delete(store: tauri::State<'_, Store>, id: i64) -> Result<(), String> {
    collections::env_delete(&store, id).map_err(|e| e.to_string())
}

#[tauri::command]
fn env_set_active(store: tauri::State<'_, Store>, id: Option<i64>) -> Result<(), String> {
    collections::env_set_active(&store, id).map_err(|e| e.to_string())
}

#[tauri::command]
fn vars_get(
    store: tauri::State<'_, Store>,
    scope: String,
    owner_id: Option<i64>,
) -> Result<Vec<collections::Variable>, String> {
    collections::vars_get(&store, &scope, owner_id).map_err(|e| e.to_string())
}

#[tauri::command]
fn vars_save(
    store: tauri::State<'_, Store>,
    scope: String,
    owner_id: Option<i64>,
    vars: Vec<collections::Variable>,
) -> Result<(), String> {
    collections::vars_save(&store, &scope, owner_id, &vars).map_err(|e| e.to_string())
}

/// Effective variables for autocomplete; secret values are masked.
#[tauri::command]
fn vars_effective(
    store: tauri::State<'_, Store>,
    collection_id: Option<i64>,
) -> Result<Vec<collections::Variable>, String> {
    let mut vars = collections::effective_vars(&store, collection_id).map_err(|e| e.to_string())?;
    for v in &mut vars {
        if v.is_secret {
            v.initial_value = "••••••".into();
            v.current_value = None;
        }
    }
    Ok(vars)
}

/* ---------------- import / export ---------------- */

#[tauri::command]
fn import_text(
    store: tauri::State<'_, Store>,
    text: String,
) -> Result<importers::ImportResult, String> {
    importers::import_auto(&store, &text).map_err(|e| e.to_string())
}

#[tauri::command]
fn import_file(
    store: tauri::State<'_, Store>,
    path: String,
) -> Result<importers::ImportResult, String> {
    let text = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
    importers::import_auto(&store, &text).map_err(|e| e.to_string())
}

#[tauri::command]
fn export_collection_file(
    store: tauri::State<'_, Store>,
    collection_id: i64,
    path: String,
) -> Result<(), String> {
    let json = importers::export_postman(&store, collection_id).map_err(|e| e.to_string())?;
    std::fs::write(&path, json).map_err(|e| e.to_string())
}

#[tauri::command]
fn parse_curl_command(text: String) -> Result<RequestSpec, String> {
    importers::parse_curl(&text)
}

/* ---------------- auth ---------------- */

#[tauri::command]
fn auth_stored_get(
    store: tauri::State<'_, Store>,
    collection_id: Option<i64>,
    item_id: Option<i64>,
) -> Result<AuthSpec, String> {
    auth::stored_auth_get(&store, collection_id, item_id).map_err(|e| e.to_string())
}

#[tauri::command]
fn auth_stored_set(
    store: tauri::State<'_, Store>,
    collection_id: Option<i64>,
    item_id: Option<i64>,
    auth: AuthSpec,
) -> Result<(), String> {
    auth::stored_auth_set(&store, collection_id, item_id, &auth).map_err(|e| e.to_string())
}

#[tauri::command]
async fn oauth2_fetch_token(
    config: auth::oauth2::OAuth2Config,
) -> Result<auth::oauth2::TokenResult, String> {
    auth::oauth2::fetch_token(&config).await
}

#[tauri::command]
async fn oauth2_refresh_token(
    config: auth::oauth2::OAuth2Config,
) -> Result<auth::oauth2::TokenResult, String> {
    auth::oauth2::refresh_token(&config).await
}

#[tauri::command]
async fn oauth2_authorize(
    app: tauri::AppHandle,
    config: auth::oauth2::OAuth2Config,
) -> Result<auth::oauth2::TokenResult, String> {
    auth::oauth2::authorize_interactive(&config, |url| {
        if let Err(err) = app.opener().open_url(url, None::<String>) {
            tracing::warn!(%err, "failed to open browser for OAuth flow");
        }
    })
    .await
}

/* ---------------- scripts & runner ---------------- */

#[tauri::command]
fn collection_scripts_get(
    store: tauri::State<'_, Store>,
    id: i64,
) -> Result<(Option<String>, Option<String>), String> {
    store
        .with_conn(|conn| {
            conn.query_row(
                "SELECT pre_request_script, test_script FROM collections WHERE id = ?1",
                rusqlite::params![id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
        })
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn collection_scripts_set(
    store: tauri::State<'_, Store>,
    id: i64,
    pre_request_script: Option<String>,
    test_script: Option<String>,
) -> Result<(), String> {
    store
        .with_conn(|conn| {
            conn.execute(
                "UPDATE collections SET pre_request_script = ?2, test_script = ?3 WHERE id = ?1",
                rusqlite::params![id, pre_request_script, test_script],
            )?;
            Ok(())
        })
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn item_scripts_get(
    store: tauri::State<'_, Store>,
    id: i64,
) -> Result<(Option<String>, Option<String>), String> {
    store
        .with_conn(|conn| {
            conn.query_row(
                "SELECT pre_request_script, test_script FROM collection_items WHERE id = ?1",
                rusqlite::params![id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
        })
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn item_scripts_set(
    store: tauri::State<'_, Store>,
    id: i64,
    pre_request_script: Option<String>,
    test_script: Option<String>,
) -> Result<(), String> {
    store
        .with_conn(|conn| {
            conn.execute(
                "UPDATE collection_items SET pre_request_script = ?2, test_script = ?3 WHERE id = ?1",
                rusqlite::params![id, pre_request_script, test_script],
            )?;
            Ok(())
        })
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn run_collection(
    app: tauri::AppHandle,
    store: tauri::State<'_, Store>,
    jar: tauri::State<'_, cookies::Cookies>,
    cancels: tauri::State<'_, RunnerCancels>,
    options: runner::RunOptions,
) -> Result<runner::RunReport, String> {
    use tauri::Emitter;
    let cancel = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    if let Ok(mut map) = cancels.0.lock() {
        map.insert(options.collection_id, cancel.clone());
    }
    let collection_id = options.collection_id;

    let report = runner::run(&store, jar.store.clone(), options, cancel, |result| {
        let _ = app.emit("runner:progress", result);
    })
    .await;

    if let Ok(mut map) = cancels.0.lock() {
        map.remove(&collection_id);
    }
    jar.save();
    Ok(report)
}

/// Read a small text file (data files for the runner, etc.).
#[tauri::command]
fn read_text_file(path: String) -> Result<String, String> {
    std::fs::read_to_string(&path).map_err(|e| e.to_string())
}

/* ---------------- sync ---------------- */

#[tauri::command]
async fn sync_register(
    store: tauri::State<'_, Store>,
    session: tauri::State<'_, sync::SyncSession>,
    url: String,
    email: String,
    password: String,
) -> Result<String, String> {
    sync::register(&store, &session, &url, &email, &password)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn sync_login(
    store: tauri::State<'_, Store>,
    session: tauri::State<'_, sync::SyncSession>,
    url: String,
    email: String,
    password: String,
) -> Result<(), String> {
    sync::login(&store, &session, &url, &email, &password)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn sync_logout(session: tauri::State<'_, sync::SyncSession>) {
    sync::logout(&session);
}

#[tauri::command]
fn sync_status(
    store: tauri::State<'_, Store>,
    session: tauri::State<'_, sync::SyncSession>,
) -> Result<sync::SyncStatus, String> {
    sync::status(&store, &session).map_err(|e| e.to_string())
}

#[tauri::command]
async fn sync_now(
    store: tauri::State<'_, Store>,
    session: tauri::State<'_, sync::SyncSession>,
) -> Result<sync::SyncReport, String> {
    sync::sync_now(&store, &session)
        .await
        .map_err(|e| e.to_string())
}

/* ---------------- GraphQL & WebSocket ---------------- */

const INTROSPECTION_QUERY: &str = r#"
query IntrospectionQuery {
  __schema {
    queryType { name }
    mutationType { name }
    subscriptionType { name }
    types {
      kind name description
      fields(includeDeprecated: false) {
        name description
        args { name description type { ...TypeRef } defaultValue }
        type { ...TypeRef }
      }
      inputFields { name type { ...TypeRef } }
      enumValues(includeDeprecated: false) { name description }
    }
  }
}
fragment TypeRef on __Type {
  kind name
  ofType { kind name ofType { kind name ofType { kind name } } }
}
"#;

/// Fetch the GraphQL schema of an endpoint (vars/auth of the tab apply).
#[tauri::command]
async fn graphql_introspect(
    store: tauri::State<'_, Store>,
    jar: tauri::State<'_, cookies::Cookies>,
    url: String,
    headers: Vec<http_engine::KeyValue>,
    collection_id: Option<i64>,
) -> Result<serde_json::Value, String> {
    let spec = RequestSpec {
        method: "POST".into(),
        url,
        headers,
        body: http_engine::BodySpec::Graphql {
            query: INTROSPECTION_QUERY.into(),
            variables: String::new(),
        },
        ..Default::default()
    };
    let resolution = vars::resolve(&store, &spec, collection_id).map_err(|e| e.to_string())?;
    let app_settings = settings::get(&store).map_err(|e| e.to_string())?;
    let resp = http_engine::execute(jar.store.clone(), &resolution.spec, &app_settings)
        .await
        .map_err(|e| e.to_string())?;
    if resp.status >= 400 {
        return Err(format!("introspection returned {}", resp.status));
    }
    serde_json::from_slice(&resp.body).map_err(|e| format!("bad introspection response: {e}"))
}

#[tauri::command]
async fn ws_connect(
    app: tauri::AppHandle,
    store: tauri::State<'_, Store>,
    sessions: tauri::State<'_, websocket::WsSessions>,
    conn_id: String,
    url: String,
    headers: Vec<http_engine::KeyValue>,
    collection_id: Option<i64>,
) -> Result<(), String> {
    use tauri::Emitter;
    // Resolve {{vars}} in URL and headers before connecting.
    let spec = RequestSpec {
        method: "WS".into(),
        url,
        headers,
        ..Default::default()
    };
    let resolution = vars::resolve(&store, &spec, collection_id).map_err(|e| e.to_string())?;

    // The session task needs its own DB connection to record history later.
    let task_store =
        Store::open(&db_path(&app).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;

    let emit_app = app.clone();
    websocket::connect(
        &sessions,
        task_store,
        conn_id,
        resolution.spec.url,
        resolution.spec.headers,
        move |event| {
            let _ = emit_app.emit("ws:event", &event);
        },
    )
    .await
}

#[tauri::command]
fn ws_send(
    app: tauri::AppHandle,
    sessions: tauri::State<'_, websocket::WsSessions>,
    conn_id: String,
    text: String,
) -> Result<(), String> {
    use tauri::Emitter;
    websocket::send(&sessions, &conn_id, text, |event| {
        let _ = app.emit("ws:event", &event);
    })
}

#[tauri::command]
fn ws_close(sessions: tauri::State<'_, websocket::WsSessions>, conn_id: String) {
    websocket::close(&sessions, &conn_id);
}

#[tauri::command]
fn runner_cancel(cancels: tauri::State<'_, RunnerCancels>, collection_id: i64) {
    if let Ok(map) = cancels.0.lock() {
        if let Some(flag) = map.get(&collection_id) {
            flag.store(true, std::sync::atomic::Ordering::Relaxed);
        }
    }
}

/* ---------------- cookies ---------------- */

#[tauri::command]
fn cookies_list(jar: tauri::State<'_, cookies::Cookies>) -> Vec<cookies::CookieInfo> {
    jar.list()
}

#[tauri::command]
fn cookie_delete(
    jar: tauri::State<'_, cookies::Cookies>,
    domain: String,
    path: String,
    name: String,
) {
    jar.delete(&domain, &path, &name);
}

#[tauri::command]
fn cookies_clear(jar: tauri::State<'_, cookies::Cookies>) {
    jar.clear();
}

/* ---------------- app settings ---------------- */

#[tauri::command]
fn app_settings_get(store: tauri::State<'_, Store>) -> Result<settings::AppSettings, String> {
    settings::get(&store).map_err(|e| e.to_string())
}

#[tauri::command]
fn app_settings_set(
    store: tauri::State<'_, Store>,
    settings_value: settings::AppSettings,
) -> Result<(), String> {
    settings::set(&store, &settings_value).map_err(|e| e.to_string())
}

/* ---------------- host aliases ---------------- */

#[tauri::command]
fn host_aliases_list(
    store: tauri::State<'_, Store>,
) -> Result<Vec<host_aliases::HostAlias>, String> {
    host_aliases::list(&store).map_err(|e| e.to_string())
}

#[tauri::command]
fn host_alias_upsert(
    store: tauri::State<'_, Store>,
    host: String,
    alias: String,
    color: String,
) -> Result<host_aliases::HostAlias, String> {
    host_aliases::upsert(&store, &host, &alias, &color).map_err(|e| e.to_string())
}

#[tauri::command]
fn host_alias_delete(store: tauri::State<'_, Store>, id: i64) -> Result<(), String> {
    host_aliases::delete(&store, id).map_err(|e| e.to_string())
}

/// Base directory for all app data. Overridable with the `POSTCAT_DATA_DIR`
/// environment variable so dev/test runs use a throwaway directory instead of
/// the real user profile.
fn data_dir(app: &tauri::AppHandle) -> Result<PathBuf, tauri::Error> {
    match std::env::var("POSTCAT_DATA_DIR") {
        Ok(dir) if !dir.trim().is_empty() => Ok(PathBuf::from(dir)),
        _ => app.path().app_data_dir(),
    }
}

fn db_path(app: &tauri::AppHandle) -> Result<PathBuf, tauri::Error> {
    Ok(data_dir(app)?.join("postcat.db"))
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
#[allow(clippy::expect_used)] // there is nothing better to do if the app cannot start
/// Register with the Windows Restart Manager so the OS relaunches us after an
/// update-triggered restart (but not after a crash/hang). Our state is restored
/// from disk on launch, so no command-line args are needed. No-op elsewhere.
#[cfg(windows)]
fn register_app_restart() {
    use windows::core::PCWSTR;
    use windows::Win32::System::Recovery::{
        RegisterApplicationRestart, RESTART_NO_CRASH, RESTART_NO_HANG,
    };
    // SAFETY: a plain FFI call; a null command line relaunches the exe as-is.
    unsafe {
        let _ = RegisterApplicationRestart(PCWSTR::null(), RESTART_NO_CRASH | RESTART_NO_HANG);
    }
}

#[cfg(not(windows))]
fn register_app_restart() {}

pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();

    #[allow(unused_mut)]
    let mut builder = tauri::Builder::default();
    // Auto-updater (desktop only): fetches a signed release from GitHub.
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    {
        builder = builder
            .plugin(tauri_plugin_updater::Builder::new().build())
            .plugin(tauri_plugin_process::init());
    }

    builder
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_decorum::init())
        // Remembers window size/position/maximized/fullscreen across launches.
        // Exclude DECORATIONS/VISIBLE — decorum owns the (undecorated) chrome.
        .plugin(
            tauri_plugin_window_state::Builder::default()
                .with_state_flags(
                    tauri_plugin_window_state::StateFlags::SIZE
                        | tauri_plugin_window_state::StateFlags::POSITION
                        | tauri_plugin_window_state::StateFlags::MAXIMIZED
                        | tauri_plugin_window_state::StateFlags::FULLSCREEN,
                )
                .build(),
        )
        .setup(|app| {
            // Relaunch after a Windows Update–triggered restart.
            register_app_restart();

            let path = db_path(app.handle())?;
            if let Some(dir) = path.parent() {
                std::fs::create_dir_all(dir)?;
            }
            let store = Store::open(&path)?;
            tracing::info!(db = %path.display(), "store opened");
            app.manage(store);
            let cookie_path = data_dir(app.handle())?.join("cookies.json");
            app.manage(cookies::Cookies::load(&cookie_path));
            app.manage(InflightRequests::default());
            app.manage(RunnerCancels::default());
            app.manage(websocket::WsSessions::default());
            app.manage(sync::SyncSession::default());

            // Custom window chrome: drop the native title bar and let decorum
            // draw native-style controls (with Windows Snap Layouts). Our own
            // title bar (the tab strip) lives in the reclaimed space.
            {
                use tauri_plugin_decorum::WebviewWindowExt;
                if let Some(win) = app.get_webview_window("main") {
                    let _ = win.create_overlay_titlebar();
                }
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            app_info,
            send_request,
            cancel_request,
            history_search,
            history_endpoints,
            history_get,
            history_set_pinned,
            history_set_label,
            history_delete,
            history_clear,
            retention_get,
            retention_set,
            collections_list,
            collection_create,
            collection_update,
            collection_delete,
            collection_items,
            item_create,
            item_update,
            item_move,
            item_delete,
            env_list,
            env_create,
            env_rename,
            env_delete,
            env_set_active,
            vars_get,
            vars_save,
            vars_effective,
            import_text,
            import_file,
            export_collection_file,
            parse_curl_command,
            auth_stored_get,
            auth_stored_set,
            oauth2_fetch_token,
            oauth2_refresh_token,
            oauth2_authorize,
            cookies_list,
            cookie_delete,
            cookies_clear,
            app_settings_get,
            app_settings_set,
            host_aliases_list,
            host_alias_upsert,
            host_alias_delete,
            collection_scripts_get,
            collection_scripts_set,
            item_scripts_get,
            item_scripts_set,
            run_collection,
            runner_cancel,
            read_text_file,
            graphql_introspect,
            ws_connect,
            ws_send,
            ws_close,
            item_duplicate,
            env_export_file,
            env_duplicate,
            history_save_body,
            sync_register,
            sync_login,
            sync_logout,
            sync_status,
            sync_now
        ])
        .run(tauri::generate_context!())
        .unwrap_or_else(|e| {
            tracing::error!("fatal: error while running tauri application: {e}");
            std::process::exit(1);
        });
}
