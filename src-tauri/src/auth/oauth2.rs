//! OAuth 2.0 token acquisition: client_credentials, password, refresh_token,
//! and authorization_code with PKCE via a loopback redirect listener.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct OAuth2Config {
    /// client_credentials | password | authorization_code
    #[serde(default)]
    pub grant_type: String,
    #[serde(default)]
    pub token_url: String,
    #[serde(default)]
    pub auth_url: String,
    #[serde(default)]
    pub client_id: String,
    #[serde(default)]
    pub client_secret: String,
    #[serde(default)]
    pub scope: String,
    #[serde(default)]
    pub username: String,
    #[serde(default)]
    pub password: String,
    /// Send client credentials in the POST body instead of Basic auth.
    #[serde(default)]
    pub credentials_in_body: bool,
    // Acquired state (stored with the request so it survives restarts).
    #[serde(default)]
    pub access_token: String,
    #[serde(default)]
    pub refresh_token: String,
    /// Unix seconds; 0 = unknown.
    #[serde(default)]
    pub expires_at: i64,
}

impl OAuth2Config {
    pub fn substituted(&self, sub: &dyn Fn(&str) -> String) -> OAuth2Config {
        OAuth2Config {
            grant_type: self.grant_type.clone(),
            token_url: sub(&self.token_url),
            auth_url: sub(&self.auth_url),
            client_id: sub(&self.client_id),
            client_secret: sub(&self.client_secret),
            scope: sub(&self.scope),
            username: sub(&self.username),
            password: sub(&self.password),
            credentials_in_body: self.credentials_in_body,
            access_token: sub(&self.access_token),
            refresh_token: self.refresh_token.clone(),
            expires_at: self.expires_at,
        }
    }

    pub fn is_expired(&self) -> bool {
        if self.expires_at == 0 {
            return false;
        }
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;
        now >= self.expires_at - 30 // refresh 30s early
    }
}

#[derive(Debug, Serialize)]
pub struct TokenResult {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_at: i64,
    pub token_type: String,
    pub raw: serde_json::Value,
}

/// Fetch a token using the config's grant type (not authorization_code —
/// that one needs the browser round-trip, see [`authorize_interactive`]).
pub async fn fetch_token(cfg: &OAuth2Config) -> Result<TokenResult, String> {
    let mut form: Vec<(&str, String)> = Vec::new();
    match cfg.grant_type.as_str() {
        "client_credentials" => {
            form.push(("grant_type", "client_credentials".into()));
        }
        "password" => {
            form.push(("grant_type", "password".into()));
            form.push(("username", cfg.username.clone()));
            form.push(("password", cfg.password.clone()));
        }
        other => return Err(format!("unsupported grant type for direct fetch: {other}")),
    }
    if !cfg.scope.is_empty() {
        form.push(("scope", cfg.scope.clone()));
    }
    token_request(cfg, form).await
}

/// Refresh using the stored refresh_token.
pub async fn refresh_token(cfg: &OAuth2Config) -> Result<TokenResult, String> {
    if cfg.refresh_token.is_empty() {
        return Err("no refresh token".into());
    }
    let form = vec![
        ("grant_type", "refresh_token".to_string()),
        ("refresh_token", cfg.refresh_token.clone()),
    ];
    token_request(cfg, form).await
}

/// Authorization Code + PKCE: starts a loopback listener, returns the URL the
/// caller must open in the browser, then waits for the redirect and exchanges
/// the code. One command does the whole dance; timeout 180 s.
pub async fn authorize_interactive(
    cfg: &OAuth2Config,
    open_url: impl Fn(&str),
) -> Result<TokenResult, String> {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .map_err(|e| e.to_string())?;
    let port = listener.local_addr().map_err(|e| e.to_string())?.port();
    let redirect_uri = format!("http://127.0.0.1:{port}/callback");

    let verifier = format!(
        "{}{}",
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple()
    );
    use base64::Engine;
    let challenge = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .encode(Sha256::digest(verifier.as_bytes()));
    let state = uuid::Uuid::new_v4().simple().to_string();

    let mut auth_url = url::Url::parse(&cfg.auth_url).map_err(|e| format!("auth url: {e}"))?;
    auth_url
        .query_pairs_mut()
        .append_pair("response_type", "code")
        .append_pair("client_id", &cfg.client_id)
        .append_pair("redirect_uri", &redirect_uri)
        .append_pair("code_challenge", &challenge)
        .append_pair("code_challenge_method", "S256")
        .append_pair("state", &state);
    if !cfg.scope.is_empty() {
        auth_url.query_pairs_mut().append_pair("scope", &cfg.scope);
    }

    open_url(auth_url.as_str());

    let code = tokio::time::timeout(
        std::time::Duration::from_secs(180),
        wait_for_code(listener, &state),
    )
    .await
    .map_err(|_| "timed out waiting for the browser redirect (180 s)".to_string())??;

    let form = vec![
        ("grant_type", "authorization_code".to_string()),
        ("code", code),
        ("redirect_uri", redirect_uri),
        ("code_verifier", verifier),
    ];
    token_request(cfg, form).await
}

async fn wait_for_code(listener: tokio::net::TcpListener, state: &str) -> Result<String, String> {
    loop {
        let (mut stream, _) = listener.accept().await.map_err(|e| e.to_string())?;
        let mut buf = vec![0u8; 4096];
        let n = stream.read(&mut buf).await.map_err(|e| e.to_string())?;
        let request = String::from_utf8_lossy(&buf[..n]);
        let Some(path) = request.split_whitespace().nth(1) else {
            continue;
        };
        let Ok(url) = url::Url::parse(&format!("http://localhost{path}")) else {
            continue;
        };
        if url.path() != "/callback" {
            let _ = stream.write_all(b"HTTP/1.1 404 Not Found\r\n\r\n").await;
            continue;
        }
        let q: std::collections::HashMap<_, _> = url.query_pairs().into_owned().collect();
        let body = "<html><body style='font-family:sans-serif'><h2>postcat</h2><p>You can close this window and return to the app.</p></body></html>";
        let _ = stream
            .write_all(
                format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\n\r\n{}",
                    body.len(),
                    body
                )
                .as_bytes(),
            )
            .await;

        if q.get("state").map(String::as_str) != Some(state) {
            return Err("state mismatch in OAuth redirect".into());
        }
        if let Some(err) = q.get("error") {
            return Err(format!(
                "authorization failed: {err} {}",
                q.get("error_description").cloned().unwrap_or_default()
            ));
        }
        return q
            .get("code")
            .cloned()
            .ok_or_else(|| "no code in redirect".into());
    }
}

async fn token_request(
    cfg: &OAuth2Config,
    mut form: Vec<(&str, String)>,
) -> Result<TokenResult, String> {
    let client = reqwest::Client::new();
    let mut req = client.post(&cfg.token_url);
    if cfg.credentials_in_body {
        form.push(("client_id", cfg.client_id.clone()));
        if !cfg.client_secret.is_empty() {
            form.push(("client_secret", cfg.client_secret.clone()));
        }
    } else if !cfg.client_id.is_empty() {
        req = req.basic_auth(&cfg.client_id, Some(&cfg.client_secret));
    }

    let resp = req
        .form(&form)
        .header("Accept", "application/json")
        .send()
        .await
        .map_err(|e| e.to_string())?;
    let status = resp.status();
    let body = resp.text().await.map_err(|e| e.to_string())?;
    if !status.is_success() {
        return Err(format!("token endpoint returned {status}: {body}"));
    }
    let json: serde_json::Value =
        serde_json::from_str(&body).map_err(|e| format!("invalid token response: {e}"))?;

    let access_token = json["access_token"]
        .as_str()
        .ok_or("token response has no access_token")?
        .to_owned();
    let expires_at = json["expires_in"].as_i64().map_or(0, |sec| {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64
            + sec
    });
    Ok(TokenResult {
        access_token,
        refresh_token: json["refresh_token"].as_str().unwrap_or("").to_owned(),
        expires_at,
        token_type: json["token_type"].as_str().unwrap_or("Bearer").to_owned(),
        raw: json,
    })
}
