//! Application-wide settings, stored as JSON in the meta table.

use rusqlite::OptionalExtension;
use serde::{Deserialize, Serialize};

use crate::store::{Store, StoreError};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AppSettings {
    /// "system" | "none" | "custom"
    pub proxy_mode: String,
    pub proxy_url: String,
    /// Paths to extra CA certificates (PEM).
    pub ca_cert_paths: Vec<String>,
    /// Client certificate (PKCS#12 / .pfx) + passphrase.
    pub client_cert_path: String,
    pub client_cert_password: String,
    /// Response capture cap in kilobytes.
    pub max_captured_body_kb: u32,
    /// Default timeout for new requests, milliseconds.
    pub default_timeout_ms: u64,

    // ----- Defaults applied to newly created requests -----
    /// Verify TLS certificates by default.
    pub default_verify_ssl: bool,
    /// Follow HTTP redirects by default.
    pub default_follow_redirects: bool,
    /// Redirect hop limit for new requests.
    pub default_max_redirects: u32,

    // ----- Appearance / editor (UI only) -----
    /// "system" | "light" | "dark"
    pub theme: String,
    /// Response pane placement: "bottom" | "right".
    pub response_layout: String,
    /// Editor/response font size in pixels.
    pub editor_font_size: u32,
    /// Wrap long response bodies by default.
    pub wrap_response: bool,
    /// Accent palette id: "bronze" | "sapphire" | "indigo" | "teal" |
    /// "burgundy" | "amethyst". Drives the --accent CSS variables.
    pub accent: String,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            proxy_mode: "system".into(),
            proxy_url: String::new(),
            ca_cert_paths: vec![],
            client_cert_path: String::new(),
            client_cert_password: String::new(),
            max_captured_body_kb: 5 * 1024,
            default_timeout_ms: 30_000,
            default_verify_ssl: true,
            default_follow_redirects: true,
            default_max_redirects: 10,
            theme: "system".into(),
            response_layout: "bottom".into(),
            editor_font_size: 13,
            wrap_response: true,
            accent: "bronze".into(),
        }
    }
}

pub fn get(store: &Store) -> Result<AppSettings, StoreError> {
    store.with_conn(|conn| {
        let json: Option<String> = conn
            .query_row(
                "SELECT value FROM meta WHERE key = 'app_settings'",
                [],
                |row| row.get(0),
            )
            .optional()?;
        Ok(json
            .and_then(|j| serde_json::from_str(&j).ok())
            .unwrap_or_default())
    })
}

pub fn set(store: &Store, settings: &AppSettings) -> Result<(), StoreError> {
    store.with_conn(|conn| {
        let json = serde_json::to_string(settings).unwrap_or_else(|_| "{}".into());
        conn.execute(
            "INSERT INTO meta (key, value) VALUES ('app_settings', ?1)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            rusqlite::params![json],
        )?;
        Ok(())
    })
}
