//! Thin HTTP client for the postcat-sync server. All errors surface as a
//! human-readable string for the UI.

use serde::{Deserialize, Serialize};

use crate::crypto::AccountBlob;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Blob {
    pub kind: String,
    pub item_id: String,
    pub rev: i64,
    pub ciphertext: String,
    pub updated_at: String,
    #[serde(default)]
    pub deleted: bool,
    #[serde(default)]
    pub seq: i64,
}

#[derive(Deserialize)]
pub struct LoginResp {
    pub token: String,
    #[serde(default)]
    pub wrapped_by_password: String,
}

#[derive(Deserialize)]
pub struct PushResp {
    pub applied: u32,
    #[allow(dead_code)]
    pub rejected: u32,
    #[allow(dead_code)]
    pub cursor: i64,
}

#[derive(Deserialize)]
pub struct PullResp {
    pub blobs: Vec<Blob>,
    pub cursor: i64,
}

fn client() -> reqwest::Client {
    reqwest::Client::new()
}

fn base(url: &str) -> String {
    url.trim_end_matches('/').to_owned()
}

async fn err_body(resp: reqwest::Response) -> String {
    let status = resp.status();
    let text = resp.text().await.unwrap_or_default();
    serde_json::from_str::<serde_json::Value>(&text)
        .ok()
        .and_then(|v| v["error"].as_str().map(str::to_owned))
        .unwrap_or_else(|| format!("server returned {status}"))
}

pub async fn register(url: &str, email: &str, blob: &AccountBlob) -> Result<(), String> {
    let resp = client()
        .post(format!("{}/v1/register", base(url)))
        .json(&serde_json::json!({ "email": email, "blob": blob }))
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if resp.status().is_success() {
        Ok(())
    } else {
        Err(err_body(resp).await)
    }
}

pub async fn salt(url: &str, email: &str) -> Result<String, String> {
    let resp = client()
        .get(format!("{}/v1/salt", base(url)))
        .query(&[("email", email)])
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(err_body(resp).await);
    }
    let v: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
    v["salt"]
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| "malformed salt response".to_owned())
}

pub async fn login(url: &str, email: &str, auth_verifier: &str) -> Result<LoginResp, String> {
    let resp = client()
        .post(format!("{}/v1/login", base(url)))
        .json(&serde_json::json!({ "email": email, "auth_verifier": auth_verifier }))
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(err_body(resp).await);
    }
    resp.json().await.map_err(|e| e.to_string())
}

// Client for the server's recover-info endpoint. Part of the password-recovery
// flow (see crypto::recover) that isn't wired to a command yet — kept ready.
#[allow(dead_code)]
pub async fn recover_info(url: &str, email: &str) -> Result<(String, String), String> {
    let resp = client()
        .get(format!("{}/v1/recover-info", base(url)))
        .query(&[("email", email)])
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(err_body(resp).await);
    }
    let v: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
    Ok((
        v["recovery_salt"].as_str().unwrap_or_default().to_owned(),
        v["wrapped_by_recovery"]
            .as_str()
            .unwrap_or_default()
            .to_owned(),
    ))
}

pub async fn push(url: &str, token: &str, blobs: &[Blob]) -> Result<PushResp, String> {
    let resp = client()
        .post(format!("{}/v1/push", base(url)))
        .bearer_auth(token)
        .json(&serde_json::json!({ "blobs": blobs }))
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(err_body(resp).await);
    }
    resp.json().await.map_err(|e| e.to_string())
}

pub async fn pull(url: &str, token: &str, since: i64) -> Result<PullResp, String> {
    let resp = client()
        .get(format!("{}/v1/pull", base(url)))
        .bearer_auth(token)
        .query(&[("since", since.to_string())])
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(err_body(resp).await);
    }
    resp.json().await.map_err(|e| e.to_string())
}
