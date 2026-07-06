//! Request authentication helpers.
//!
//! An [`AuthSpec`] lives on a request (or on a collection/folder, inherited
//! downward). It is applied to the *resolved* request right before sending;
//! injected sensitive values are reported as secrets so history masks them.

pub mod oauth2;
mod sigv4;

use serde::{Deserialize, Serialize};

use crate::http_engine::{KeyValue, RequestSpec};
use crate::store::{Store, StoreError};

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AuthSpec {
    #[default]
    None,
    /// Walk up: request → ancestor folders → collection.
    Inherit,
    ApiKey {
        key: String,
        value: String,
        /// "header" or "query"
        #[serde(default)]
        in_query: bool,
    },
    Bearer {
        token: String,
    },
    Basic {
        username: String,
        password: String,
    },
    Oauth2(Box<oauth2::OAuth2Config>),
    AwsSigV4 {
        access_key: String,
        secret_key: String,
        region: String,
        service: String,
        #[serde(default)]
        session_token: String,
    },
}

impl AuthSpec {
    /// Substitute {{vars}} in every string field.
    pub fn substituted(&self, sub: &dyn Fn(&str) -> String) -> AuthSpec {
        match self {
            AuthSpec::None => AuthSpec::None,
            AuthSpec::Inherit => AuthSpec::Inherit,
            AuthSpec::ApiKey {
                key,
                value,
                in_query,
            } => AuthSpec::ApiKey {
                key: sub(key),
                value: sub(value),
                in_query: *in_query,
            },
            AuthSpec::Bearer { token } => AuthSpec::Bearer { token: sub(token) },
            AuthSpec::Basic { username, password } => AuthSpec::Basic {
                username: sub(username),
                password: sub(password),
            },
            AuthSpec::Oauth2(cfg) => AuthSpec::Oauth2(Box::new(cfg.substituted(sub))),
            AuthSpec::AwsSigV4 {
                access_key,
                secret_key,
                region,
                service,
                session_token,
            } => AuthSpec::AwsSigV4 {
                access_key: sub(access_key),
                secret_key: sub(secret_key),
                region: sub(region),
                service: sub(service),
                session_token: sub(session_token),
            },
        }
    }
}

/// Resolve the effective auth for a request: an explicit spec wins; Inherit
/// walks the item's ancestors and then the collection.
pub fn effective_auth(
    store: &Store,
    spec_auth: &AuthSpec,
    item_id: Option<i64>,
    collection_id: Option<i64>,
) -> Result<AuthSpec, StoreError> {
    if *spec_auth != AuthSpec::Inherit {
        return Ok(spec_auth.clone());
    }
    // Walk item ancestors (the item itself holds the request's saved auth,
    // which equals spec_auth; start from its parent).
    let mut current = item_id;
    let mut first = true;
    while let Some(id) = current {
        let (auth_json, parent): (Option<String>, Option<i64>) = store.with_conn(|conn| {
            conn.query_row(
                "SELECT auth, parent_id FROM collection_items WHERE id = ?1",
                rusqlite::params![id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
        })?;
        if !first {
            if let Some(auth) = parse_auth(auth_json) {
                if auth != AuthSpec::Inherit && auth != AuthSpec::None {
                    return Ok(auth);
                }
            }
        }
        first = false;
        current = parent;
    }
    if let Some(cid) = collection_id {
        let auth_json: Option<String> = store.with_conn(|conn| {
            conn.query_row(
                "SELECT auth FROM collections WHERE id = ?1",
                rusqlite::params![cid],
                |row| row.get(0),
            )
        })?;
        if let Some(auth) = parse_auth(auth_json) {
            if auth != AuthSpec::Inherit {
                return Ok(auth);
            }
        }
    }
    Ok(AuthSpec::None)
}

fn parse_auth(json: Option<String>) -> Option<AuthSpec> {
    json.and_then(|j| serde_json::from_str(&j).ok())
}

/// Stored auth on a collection or a tree item (folder/request).
pub fn stored_auth_get(
    store: &Store,
    collection_id: Option<i64>,
    item_id: Option<i64>,
) -> Result<AuthSpec, StoreError> {
    let json: Option<String> = store.with_conn(|conn| match (collection_id, item_id) {
        (_, Some(id)) => conn.query_row(
            "SELECT auth FROM collection_items WHERE id = ?1",
            rusqlite::params![id],
            |row| row.get(0),
        ),
        (Some(id), None) => conn.query_row(
            "SELECT auth FROM collections WHERE id = ?1",
            rusqlite::params![id],
            |row| row.get(0),
        ),
        _ => Ok(None),
    })?;
    Ok(parse_auth(json).unwrap_or_default())
}

pub fn stored_auth_set(
    store: &Store,
    collection_id: Option<i64>,
    item_id: Option<i64>,
    auth: &AuthSpec,
) -> Result<(), StoreError> {
    let json = serde_json::to_string(auth).unwrap_or_else(|_| "null".into());
    store.with_conn(|conn| {
        match (collection_id, item_id) {
            (_, Some(id)) => conn.execute(
                "UPDATE collection_items SET auth = ?2 WHERE id = ?1",
                rusqlite::params![id, json],
            )?,
            (Some(id), None) => conn.execute(
                "UPDATE collections SET auth = ?2 WHERE id = ?1",
                rusqlite::params![id, json],
            )?,
            _ => 0,
        };
        Ok(())
    })
}

/// Apply auth to a resolved request. Returns (value, label) pairs that must
/// be masked in history.
pub fn apply(spec: &mut RequestSpec, auth: &AuthSpec) -> Vec<(String, String)> {
    let mut secrets: Vec<(String, String)> = Vec::new();
    match auth {
        AuthSpec::None | AuthSpec::Inherit => {}
        AuthSpec::ApiKey {
            key,
            value,
            in_query,
        } => {
            if key.is_empty() {
                return secrets;
            }
            if *in_query {
                let sep = if spec.url.contains('?') { '&' } else { '?' };
                spec.url = format!("{}{}{}={}", spec.url, sep, key, value);
            } else {
                push_header(spec, key, value);
            }
            secrets.push((value.clone(), "api_key".into()));
        }
        AuthSpec::Bearer { token } => {
            if !token.is_empty() {
                push_header(spec, "Authorization", &format!("Bearer {token}"));
                secrets.push((token.clone(), "bearer_token".into()));
            }
        }
        AuthSpec::Basic { username, password } => {
            use base64::Engine;
            let b64 =
                base64::engine::general_purpose::STANDARD.encode(format!("{username}:{password}"));
            push_header(spec, "Authorization", &format!("Basic {b64}"));
            secrets.push((b64, "basic_credentials".into()));
            if !password.is_empty() {
                secrets.push((password.clone(), "basic_password".into()));
            }
        }
        AuthSpec::Oauth2(cfg) => {
            let token = cfg.access_token.trim();
            if !token.is_empty() {
                push_header(spec, "Authorization", &format!("Bearer {token}"));
                secrets.push((token.to_owned(), "oauth2_token".into()));
            }
        }
        AuthSpec::AwsSigV4 {
            access_key,
            secret_key,
            region,
            service,
            session_token,
        } => {
            let creds = sigv4::Credentials {
                access_key,
                secret_key,
                session_token: if session_token.is_empty() {
                    None
                } else {
                    Some(session_token)
                },
                region,
                service,
            };
            if let Err(err) = sigv4::sign(spec, &creds, None) {
                tracing::warn!(%err, "sigv4 signing failed");
            }
            secrets.push((secret_key.clone(), "aws_secret_key".into()));
        }
    }
    secrets
}

fn push_header(spec: &mut RequestSpec, key: &str, value: &str) {
    // Explicit user header wins over the auth helper.
    if spec
        .headers
        .iter()
        .any(|h| h.enabled && h.key.eq_ignore_ascii_case(key))
    {
        return;
    }
    spec.headers.push(KeyValue {
        key: key.to_owned(),
        value: value.to_owned(),
        enabled: true,
    });
}
