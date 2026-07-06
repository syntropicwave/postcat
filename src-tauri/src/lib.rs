mod store;

use std::path::PathBuf;

use tauri::Manager;

use store::Store;

#[derive(serde::Serialize)]
struct AppInfo {
    version: String,
    db_path: String,
    schema_version: i64,
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
        .setup(|app| {
            let path = db_path(app.handle())?;
            if let Some(dir) = path.parent() {
                std::fs::create_dir_all(dir)?;
            }
            let store = Store::open(&path)?;
            tracing::info!(db = %path.display(), "store opened");
            app.manage(store);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![app_info])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
