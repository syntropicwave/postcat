//! postcat-sync — self-hostable, end-to-end-encrypted sync server.
//!
//! The server never sees plaintext: it stores opaque ciphertext blobs and
//! verifies logins by comparing a SHA-256 of the client-presented auth
//! verifier. Run it and point postcat's sync settings at its URL.
//!
//!   POSTCAT_SYNC_DB=./sync.db POSTCAT_SYNC_ADDR=0.0.0.0:8787 postcat-sync

mod db;

use std::sync::Arc;

use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::routing::{get, post};
use axum::{Json, Router};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use serde_json::json;

use db::{AccountBlob, Blob, Db};

const SESSION_TTL_SECS: i64 = 30 * 24 * 3600;

type Shared = Arc<Db>;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();

    let db_path = std::env::var("POSTCAT_SYNC_DB").unwrap_or_else(|_| "postcat-sync.db".into());
    let addr = std::env::var("POSTCAT_SYNC_ADDR").unwrap_or_else(|_| "0.0.0.0:8787".into());

    let db = match Db::open(std::path::Path::new(&db_path)) {
        Ok(db) => Arc::new(db),
        Err(err) => {
            eprintln!("cannot open database {db_path}: {err}");
            std::process::exit(1);
        }
    };

    let app = router(db);
    let listener = match tokio::net::TcpListener::bind(&addr).await {
        Ok(l) => l,
        Err(err) => {
            eprintln!("cannot bind {addr}: {err}");
            std::process::exit(1);
        }
    };
    tracing::info!(%addr, db = %db_path, "postcat-sync listening");
    if let Err(err) = axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
    {
        eprintln!("server error: {err}");
    }
}

fn router(db: Shared) -> Router {
    Router::new()
        .route("/health", get(|| async { "ok" }))
        .route("/v1/register", post(register))
        .route("/v1/salt", get(salt))
        .route("/v1/recover-info", get(recover_info))
        .route("/v1/login", post(login))
        .route("/v1/push", post(push))
        .route("/v1/pull", get(pull))
        .with_state(db)
}

async fn shutdown_signal() {
    let _ = tokio::signal::ctrl_c().await;
}

/* ---------------- accounts ---------------- */

#[derive(Deserialize)]
struct RegisterReq {
    email: String,
    blob: AccountBlob,
}

async fn register(
    State(db): State<Shared>,
    Json(req): Json<RegisterReq>,
) -> Result<StatusCode, ApiError> {
    let email = normalize_email(&req.email)?;
    if db.account_exists(&email)? {
        return Err(ApiError::new(
            StatusCode::CONFLICT,
            "email already registered",
        ));
    }
    db.create_account(&email, &req.blob)?;
    Ok(StatusCode::CREATED)
}

#[derive(Deserialize)]
struct EmailQuery {
    email: String,
}

/// The password salt is not secret — the client needs it to derive keys
/// before it can even attempt a login.
async fn salt(
    State(db): State<Shared>,
    Query(q): Query<EmailQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let email = normalize_email(&q.email)?;
    let salt = db
        .account_field(&email, "salt")?
        .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "no such account"))?;
    Ok(Json(json!({ "salt": salt })))
}

/// Data needed to attempt recovery. Useless without the recovery code, which
/// is required to actually decrypt `wrapped_by_recovery`.
async fn recover_info(
    State(db): State<Shared>,
    Query(q): Query<EmailQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let email = normalize_email(&q.email)?;
    let recovery_salt = db
        .account_field(&email, "recovery_salt")?
        .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "no such account"))?;
    let wrapped = db
        .account_field(&email, "wrapped_by_recovery")?
        .unwrap_or_default();
    Ok(Json(
        json!({ "recovery_salt": recovery_salt, "wrapped_by_recovery": wrapped }),
    ))
}

#[derive(Deserialize)]
struct LoginReq {
    email: String,
    /// The client's HKDF("auth") value, base64. The server compares its
    /// SHA-256 to the stored hash — it never learns the password.
    auth_verifier: String,
}

async fn login(
    State(db): State<Shared>,
    Json(req): Json<LoginReq>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let email = normalize_email(&req.email)?;
    let verifier_hash = sha256_hex(req.auth_verifier.as_bytes());
    let token = random_token();
    match db.login(&email, &verifier_hash, &token, SESSION_TTL_SECS)? {
        None => Err(ApiError::new(
            StatusCode::UNAUTHORIZED,
            "invalid credentials",
        )),
        Some(_) => {
            let wrapped = db
                .account_field(&email, "wrapped_by_password")?
                .unwrap_or_default();
            Ok(Json(
                json!({ "token": token, "wrapped_by_password": wrapped }),
            ))
        }
    }
}

/* ---------------- sync ---------------- */

#[derive(Deserialize)]
struct PushReq {
    blobs: Vec<Blob>,
}

#[derive(Serialize)]
struct PushResp {
    applied: usize,
    rejected: usize,
    cursor: i64,
}

async fn push(
    State(db): State<Shared>,
    headers: HeaderMap,
    Json(req): Json<PushReq>,
) -> Result<Json<PushResp>, ApiError> {
    let account = authenticate(&db, &headers)?;
    let mut applied = 0;
    let mut rejected = 0;
    for blob in &req.blobs {
        if db.push_blob(account, blob)? {
            applied += 1;
        } else {
            rejected += 1;
        }
    }
    // Report the account's current max cursor so the client can advance.
    let cursor = db
        .pull_since(account, -1)?
        .iter()
        .map(|b| b.seq)
        .max()
        .unwrap_or(0);
    Ok(Json(PushResp {
        applied,
        rejected,
        cursor,
    }))
}

#[derive(Deserialize)]
struct PullQuery {
    #[serde(default)]
    since: i64,
}

#[derive(Serialize)]
struct PullResp {
    blobs: Vec<Blob>,
    cursor: i64,
}

async fn pull(
    State(db): State<Shared>,
    headers: HeaderMap,
    Query(q): Query<PullQuery>,
) -> Result<Json<PullResp>, ApiError> {
    let account = authenticate(&db, &headers)?;
    let blobs = db.pull_since(account, q.since)?;
    let cursor = blobs.iter().map(|b| b.seq).max().unwrap_or(q.since);
    Ok(Json(PullResp { blobs, cursor }))
}

/* ---------------- helpers ---------------- */

fn authenticate(db: &Db, headers: &HeaderMap) -> Result<i64, ApiError> {
    let token = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .ok_or_else(|| ApiError::new(StatusCode::UNAUTHORIZED, "missing bearer token"))?;
    db.session_account(token)?
        .ok_or_else(|| ApiError::new(StatusCode::UNAUTHORIZED, "invalid or expired session"))
}

fn normalize_email(email: &str) -> Result<String, ApiError> {
    let e = email.trim().to_lowercase();
    if e.is_empty() || !e.contains('@') {
        return Err(ApiError::new(StatusCode::BAD_REQUEST, "invalid email"));
    }
    Ok(e)
}

fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::Digest;
    hex::encode(sha2::Sha256::digest(bytes))
}

fn random_token() -> String {
    let mut buf = [0u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut buf);
    hex::encode(buf)
}

/// Error type that renders as a JSON body with a status code.
struct ApiError {
    status: StatusCode,
    message: String,
}

impl ApiError {
    fn new(status: StatusCode, message: &str) -> Self {
        Self {
            status,
            message: message.to_owned(),
        }
    }
}

impl From<db::DbError> for ApiError {
    fn from(err: db::DbError) -> Self {
        tracing::error!(%err, "db error");
        ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "internal error")
    }
}

impl axum::response::IntoResponse for ApiError {
    fn into_response(self) -> axum::response::Response {
        (self.status, Json(json!({ "error": self.message }))).into_response()
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use axum::body::Body;
    use axum::http::Request;
    use tower::ServiceExt; // oneshot

    fn body_json(bytes: &[u8]) -> serde_json::Value {
        serde_json::from_slice(bytes).unwrap()
    }

    async fn call(app: &Router, req: Request<Body>) -> (StatusCode, serde_json::Value) {
        let resp = app.clone().oneshot(req).await.unwrap();
        let status = resp.status();
        let bytes = axum::body::to_bytes(resp.into_body(), 1 << 20)
            .await
            .unwrap();
        let value = if bytes.is_empty() {
            serde_json::Value::Null
        } else {
            body_json(&bytes)
        };
        (status, value)
    }

    #[tokio::test]
    async fn full_register_login_push_pull() {
        let app = router(Arc::new(Db::open_in_memory().unwrap()));

        // register
        let reg = json!({
            "email": "user@example.com",
            "blob": {
                "salt": "s", "recovery_salt": "r",
                "auth_verifier_hash": sha256_hex(b"verifier"),
                "wrapped_by_password": "wp", "wrapped_by_recovery": "wr"
            }
        });
        let (st, _) = call(
            &app,
            Request::post("/v1/register")
                .header("content-type", "application/json")
                .body(Body::from(reg.to_string()))
                .unwrap(),
        )
        .await;
        assert_eq!(st, StatusCode::CREATED);

        // salt is public
        let (st, v) = call(
            &app,
            Request::get("/v1/salt?email=user@example.com")
                .body(Body::empty())
                .unwrap(),
        )
        .await;
        assert_eq!(st, StatusCode::OK);
        assert_eq!(v["salt"], "s");

        // wrong verifier rejected
        let (st, _) = call(
            &app,
            Request::post("/v1/login")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({"email":"user@example.com","auth_verifier":"wrong"}).to_string(),
                ))
                .unwrap(),
        )
        .await;
        assert_eq!(st, StatusCode::UNAUTHORIZED);

        // correct verifier -> token + wrapped key
        let (st, v) = call(
            &app,
            Request::post("/v1/login")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({"email":"user@example.com","auth_verifier":"verifier"}).to_string(),
                ))
                .unwrap(),
        )
        .await;
        assert_eq!(st, StatusCode::OK);
        assert_eq!(v["wrapped_by_password"], "wp");
        let token = v["token"].as_str().unwrap().to_owned();

        // push two blobs
        let push = json!({"blobs":[
            {"kind":"collection","item_id":"c1","rev":1,"ciphertext":"AAA","updated_at":"2026-07-06T00:00:00Z"},
            {"kind":"environment","item_id":"e1","rev":1,"ciphertext":"BBB","updated_at":"2026-07-06T00:00:00Z"}
        ]});
        let (st, v) = call(
            &app,
            Request::post("/v1/push")
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {token}"))
                .body(Body::from(push.to_string()))
                .unwrap(),
        )
        .await;
        assert_eq!(st, StatusCode::OK);
        assert_eq!(v["applied"], 2);

        // pull from 0 returns both, still encrypted
        let (st, v) = call(
            &app,
            Request::get("/v1/pull?since=0")
                .header("authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await;
        assert_eq!(st, StatusCode::OK);
        assert_eq!(v["blobs"].as_array().unwrap().len(), 2);
        assert_eq!(v["blobs"][0]["ciphertext"], "AAA");

        // pull without token is rejected
        let (st, _) = call(
            &app,
            Request::get("/v1/pull?since=0")
                .body(Body::empty())
                .unwrap(),
        )
        .await;
        assert_eq!(st, StatusCode::UNAUTHORIZED);
    }
}
