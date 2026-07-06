pub mod history;
pub mod http_engine;
pub mod store;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use tauri::Manager;

use http_engine::RequestSpec;
use store::Store;

/// In-flight request cancellation handles, keyed by frontend-generated id.
#[derive(Default)]
struct InflightRequests(Mutex<HashMap<String, tokio::sync::oneshot::Sender<()>>>);

struct CookieJar(Arc<reqwest::cookie::Jar>);

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
}

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
async fn send_request(
    store: tauri::State<'_, Store>,
    jar: tauri::State<'_, CookieJar>,
    inflight: tauri::State<'_, InflightRequests>,
    request_id: String,
    spec: RequestSpec,
) -> Result<SendResult, String> {
    let (cancel_tx, cancel_rx) = tokio::sync::oneshot::channel::<()>();
    if let Ok(mut map) = inflight.0.lock() {
        map.insert(request_id.clone(), cancel_tx);
    }

    let outcome = tokio::select! {
        res = http_engine::execute(jar.0.clone(), &spec) => Some(res),
        _ = cancel_rx => None,
    };

    if let Ok(mut map) = inflight.0.lock() {
        map.remove(&request_id);
    }

    match outcome {
        None => {
            // Cancelled by the user: record the attempt, report as error.
            let _ = history::record(&store, &spec, Err("cancelled"));
            Err("cancelled".into())
        }
        Some(Err(err)) => {
            let msg = err.to_string();
            let _ = history::record(&store, &spec, Err(&msg));
            Err(msg)
        }
        Some(Ok(resp)) => {
            let history_id =
                history::record(&store, &spec, Ok(&resp)).map_err(|e| e.to_string())?;
            let (body_text, body_base64) = history::body_for_ui(&resp.body);
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

fn db_path(app: &tauri::AppHandle) -> Result<PathBuf, tauri::Error> {
    Ok(app.path().app_data_dir()?.join("postcat.db"))
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
#[allow(clippy::expect_used)] // there is nothing better to do if the app cannot start
pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let path = db_path(app.handle())?;
            if let Some(dir) = path.parent() {
                std::fs::create_dir_all(dir)?;
            }
            let store = Store::open(&path)?;
            tracing::info!(db = %path.display(), "store opened");
            app.manage(store);
            app.manage(CookieJar(Arc::new(reqwest::cookie::Jar::default())));
            app.manage(InflightRequests::default());
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
            retention_set
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
