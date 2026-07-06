//! Inspectable, persistent cookie jar shared by all requests.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use reqwest_cookie_store::{CookieStore, CookieStoreMutex};
use serde::Serialize;

pub struct Cookies {
    pub store: Arc<CookieStoreMutex>,
    path: PathBuf,
}

#[derive(Debug, Serialize)]
pub struct CookieInfo {
    pub domain: String,
    pub path: String,
    pub name: String,
    pub value: String,
    pub secure: bool,
    pub expires: Option<String>,
}

impl Cookies {
    /// Load the persisted jar (or start empty).
    pub fn load(path: &Path) -> Self {
        let store = std::fs::File::open(path)
            .ok()
            .and_then(|f| {
                CookieStore::load_all(std::io::BufReader::new(f), |c| serde_json::from_str(c)).ok()
            })
            .unwrap_or_default();
        Self {
            store: Arc::new(CookieStoreMutex::new(store)),
            path: path.to_owned(),
        }
    }

    /// Persist the jar (called after every completed request).
    pub fn save(&self) {
        let Ok(store) = self.store.lock() else {
            return;
        };
        let Ok(file) = std::fs::File::create(&self.path) else {
            return;
        };
        let mut writer = std::io::BufWriter::new(file);
        if let Err(err) =
            store.save_incl_expired_and_nonpersistent(&mut writer, serde_json::to_string)
        {
            tracing::warn!(%err, "failed to persist cookies");
        }
    }

    pub fn list(&self) -> Vec<CookieInfo> {
        let Ok(store) = self.store.lock() else {
            return vec![];
        };
        store
            .iter_any()
            .map(|c| CookieInfo {
                domain: c.domain().unwrap_or_default().to_owned(),
                path: c.path().unwrap_or("/").to_owned(),
                name: c.name().to_owned(),
                value: c.value().to_owned(),
                secure: c.secure().unwrap_or(false),
                expires: c.expires_datetime().map(|dt| dt.to_string()),
            })
            .collect()
    }

    pub fn delete(&self, domain: &str, path: &str, name: &str) {
        if let Ok(mut store) = self.store.lock() {
            store.remove(domain, path, name);
        }
        self.save();
    }

    pub fn clear(&self) {
        if let Ok(mut store) = self.store.lock() {
            store.clear();
        }
        self.save();
    }
}
