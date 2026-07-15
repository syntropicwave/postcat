mod timed;

use std::sync::Arc;
use std::time::Instant;

use futures_util::StreamExt;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue, CONTENT_TYPE};
use reqwest_cookie_store::CookieStoreMutex;
use serde::{Deserialize, Serialize};

use crate::settings::AppSettings;

/// Default cap on how much of a response body is kept (stored in history and
/// shown in the UI); configurable via app settings.
pub const MAX_CAPTURED_BODY: usize = 5 * 1024 * 1024;

fn default_true() -> bool {
    true
}
fn default_timeout_ms() -> u64 {
    30_000
}
fn default_max_redirects() -> usize {
    10
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyValue {
    pub key: String,
    #[serde(default)]
    pub value: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FormField {
    pub key: String,
    #[serde(default)]
    pub value: String,
    /// When true, `value` is a filesystem path to upload.
    #[serde(default)]
    pub is_file: bool,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum BodySpec {
    #[default]
    None,
    Raw {
        content_type: String,
        text: String,
    },
    UrlEncoded {
        fields: Vec<KeyValue>,
    },
    FormData {
        fields: Vec<FormField>,
    },
    Binary {
        path: String,
    },
    /// GraphQL query + variables (JSON text); sent as application/json.
    Graphql {
        query: String,
        #[serde(default)]
        variables: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SendSettings {
    #[serde(default = "default_timeout_ms")]
    pub timeout_ms: u64,
    #[serde(default = "default_true")]
    pub follow_redirects: bool,
    #[serde(default = "default_max_redirects")]
    pub max_redirects: usize,
    #[serde(default = "default_true")]
    pub verify_ssl: bool,
}

impl Default for SendSettings {
    fn default() -> Self {
        Self {
            timeout_ms: default_timeout_ms(),
            follow_redirects: true,
            max_redirects: default_max_redirects(),
            verify_ssl: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequestSpec {
    pub method: String,
    pub url: String,
    #[serde(default)]
    pub headers: Vec<KeyValue>,
    #[serde(default)]
    pub body: BodySpec,
    #[serde(default)]
    pub settings: SendSettings,
    #[serde(default)]
    pub auth: crate::auth::AuthSpec,
}

impl Default for RequestSpec {
    fn default() -> Self {
        Self {
            method: "GET".into(),
            url: String::new(),
            headers: vec![],
            body: BodySpec::None,
            settings: SendSettings::default(),
            auth: Default::default(),
        }
    }
}

impl RequestSpec {
    pub fn host(&self) -> String {
        url::Url::parse(&self.url)
            .ok()
            .and_then(|u| u.host_str().map(str::to_owned))
            .unwrap_or_default()
    }

    /// Text sent as the request body, for history storage and (later) search
    /// indexing. Files are represented by their paths, not their contents.
    pub fn body_text(&self) -> Option<String> {
        match &self.body {
            BodySpec::None => None,
            BodySpec::Raw { text, .. } => Some(text.clone()),
            BodySpec::UrlEncoded { fields } => Some(
                fields
                    .iter()
                    .filter(|f| f.enabled && !f.key.is_empty())
                    .map(|f| format!("{}={}", f.key, f.value))
                    .collect::<Vec<_>>()
                    .join("&"),
            ),
            BodySpec::FormData { fields } => Some(
                fields
                    .iter()
                    .filter(|f| f.enabled && !f.key.is_empty())
                    .map(|f| {
                        if f.is_file {
                            format!("{}=@{}", f.key, f.value)
                        } else {
                            format!("{}={}", f.key, f.value)
                        }
                    })
                    .collect::<Vec<_>>()
                    .join("\n"),
            ),
            BodySpec::Binary { path } => Some(format!("@{path}")),
            BodySpec::Graphql { query, variables } => {
                if variables.trim().is_empty() {
                    Some(query.clone())
                } else {
                    Some(format!("{query}\n{variables}"))
                }
            }
        }
    }
}

/// Per-phase timing waterfall. Phases before the connection is established
/// (dns/connect/tls) are None when the request reused a pooled connection or
/// ran through the reqwest fallback path.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Timings {
    pub dns_ms: Option<f64>,
    pub connect_ms: Option<f64>,
    pub tls_ms: Option<f64>,
    /// Connection ready → first response byte (request send + server work).
    pub server_ms: f64,
    /// First response byte → last byte.
    pub download_ms: f64,
    pub total_ms: f64,
    pub redirects: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct HttpResponseData {
    pub status: u16,
    pub status_text: String,
    pub http_version: String,
    pub headers: Vec<(String, String)>,
    #[serde(skip)]
    pub body: Vec<u8>,
    pub body_truncated: bool,
    pub size: usize,
    pub duration_ms: f64,
    pub ttfb_ms: f64,
    pub timings: Timings,
}

/// The stage of a request where a failure happened. Drives the UI pipeline
/// (DNS → TCP → TLS → send → receive): every stage before the failing one is
/// shown as passed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Phase {
    Dns,
    Tcp,
    Tls,
    Send,
    Receive,
    Timeout,
    Request,
    Other,
}

/// Structured request failure surfaced to the UI.
#[derive(Debug, serde::Serialize)]
pub struct SendError {
    pub phase: Phase,
    pub message: String,
    pub hint: Option<String>,
}

impl From<String> for SendError {
    fn from(message: String) -> Self {
        SendError {
            phase: Phase::Other,
            message,
            hint: None,
        }
    }
}
impl From<&str> for SendError {
    fn from(m: &str) -> Self {
        m.to_owned().into()
    }
}

#[derive(Debug, thiserror::Error)]
pub enum EngineError {
    #[error("invalid method: {0}")]
    Method(String),
    #[error("invalid header name: {0}")]
    HeaderName(String),
    #[error("invalid header value for {0}")]
    HeaderValue(String),
    #[error("cannot read file {path}: {source}")]
    File {
        path: String,
        source: std::io::Error,
    },
    #[error("invalid certificate: {0}")]
    Certificate(String),
    #[error("invalid proxy: {0}")]
    Proxy(String),
    #[error("{0}")]
    Connect(String),
    #[error("{detail}")]
    Network {
        phase: Phase,
        detail: String,
        hint: Option<String>,
    },
}

impl From<reqwest::Error> for EngineError {
    fn from(e: reqwest::Error) -> Self {
        let (phase, detail, hint) = describe_reqwest(&e);
        EngineError::Network {
            phase,
            detail,
            hint,
        }
    }
}

impl EngineError {
    /// Flatten to the structured error the UI renders.
    pub fn into_send_error(self) -> SendError {
        match self {
            EngineError::Network {
                phase,
                detail,
                hint,
            } => SendError {
                phase,
                message: detail,
                hint,
            },
            EngineError::Connect(m) => SendError {
                phase: Phase::Other,
                message: m,
                hint: None,
            },
            // Method / header / file / certificate / proxy: the request was
            // never sent — a build/config problem.
            e => SendError {
                phase: Phase::Request,
                message: e.to_string(),
                hint: None,
            },
        }
    }
}

/// Join an error with its `source()` chain into a single readable line, keeping
/// each distinct cause. reqwest/hyper hide the actionable cause (DNS, refused,
/// TLS, …) inside this chain; its top-level Display alone is nearly useless.
pub(crate) fn error_chain(err: &dyn std::error::Error) -> String {
    let mut parts: Vec<String> = Vec::new();
    let mut cur: Option<&dyn std::error::Error> = Some(err);
    while let Some(e) = cur {
        let msg = e.to_string();
        // Skip empties and messages already implied by an earlier link.
        if !msg.trim().is_empty() && !parts.iter().any(|p| p == &msg || p.contains(&msg)) {
            parts.push(msg);
        }
        cur = e.source();
    }
    parts.join(": ")
}

/// Classify a reqwest failure into (phase, detail, hint). reqwest lumps
/// DNS/TCP/TLS into one coarse "connect" error, so we refine the phase by
/// scanning the underlying cause chain (which carries the real reason).
fn describe_reqwest(e: &reqwest::Error) -> (Phase, String, Option<String>) {
    use std::error::Error as _;
    // Prefer the cause chain (skipping reqwest's vague top line).
    let detail = match e.source() {
        Some(src) => error_chain(src),
        None => e.to_string(),
    };
    let lower = detail.to_ascii_lowercase();

    let (phase, hint): (Phase, Option<&'static str>) = if e.is_timeout() {
        (
            Phase::Timeout,
            Some("the server took too long — increase the request timeout, or it may be slow/unresponsive"),
        )
    } else if e.is_decode() || e.is_body() {
        (
            Phase::Receive,
            Some("the response could not be read fully — the connection may have dropped mid-transfer"),
        )
    } else if lower.contains("certificate")
        || lower.contains("tls")
        || lower.contains("ssl")
        || lower.contains("handshake")
        || lower.contains("verify")
    {
        (
            Phase::Tls,
            Some("the certificate may be self-signed, expired or untrusted — you can disable SSL verification in request settings, or add the CA in Settings"),
        )
    } else if lower.contains("dns")
        || lower.contains("lookup address")
        || lower.contains("name or service not known")
        || lower.contains("no such host")
        || lower.contains("failed to lookup")
    {
        (
            Phase::Dns,
            Some("the host could not be resolved — check the URL/spelling and your DNS/network"),
        )
    } else if lower.contains("refused") {
        (
            Phase::Tcp,
            Some(
                "the connection was refused — the server may be down or not listening on that port",
            ),
        )
    } else if lower.contains("unreachable") {
        (
            Phase::Tcp,
            Some("the network is unreachable — check your connection/VPN/proxy"),
        )
    } else if e.is_connect() {
        (
            Phase::Tcp,
            Some("check the URL and port, that the server is running and reachable, and any VPN/proxy"),
        )
    } else if e.is_redirect() {
        (Phase::Receive, Some("too many redirects"))
    } else if e.is_request() || e.is_builder() {
        (Phase::Request, None)
    } else {
        (Phase::Other, None)
    };

    (phase, detail, hint.map(str::to_owned))
}

pub async fn execute(
    jar: Arc<CookieStoreMutex>,
    spec: &RequestSpec,
    app: &AppSettings,
) -> Result<HttpResponseData, EngineError> {
    execute_streaming(jar, spec, app, None).await
}

/// Callback invoked per body chunk when the response is a live stream
/// (text/event-stream) — lets the UI show events as they arrive.
pub type StreamChunkFn = Arc<dyn Fn(&str) + Send + Sync>;

pub async fn execute_streaming(
    jar: Arc<CookieStoreMutex>,
    spec: &RequestSpec,
    app: &AppSettings,
    on_stream_chunk: Option<StreamChunkFn>,
) -> Result<HttpResponseData, EngineError> {
    // The instrumented path gives a full per-phase waterfall. It handles the
    // common case (direct connection, default trust, byte bodies). Anything
    // it can't do cleanly falls back to reqwest with a partial breakdown.
    if timed::eligible(spec, app) {
        match timed::execute(jar.clone(), spec, app, on_stream_chunk.clone()).await {
            Ok(data) => return Ok(data),
            // A timeout is a genuine failure — retrying with reqwest would just
            // wait the whole timeout again, so surface it directly.
            Err(
                err @ EngineError::Network {
                    phase: Phase::Timeout,
                    ..
                },
            ) => return Err(err),
            Err(err) => {
                tracing::warn!(%err, "instrumented path failed, falling back to reqwest");
            }
        }
    }

    let client = build_client(jar, &spec.settings, app)?;
    let request = build_request(&client, spec)?;
    let max_captured = (app.max_captured_body_kb as usize).max(64) * 1024;

    let started = Instant::now();
    let response = client.execute(request).await?;
    let ttfb_ms = started.elapsed().as_secs_f64() * 1000.0;

    let status = response.status();
    let http_version = format!("{:?}", response.version());
    let headers: Vec<(String, String)> = response
        .headers()
        .iter()
        .map(|(k, v)| {
            (
                k.as_str().to_owned(),
                String::from_utf8_lossy(v.as_bytes()).into_owned(),
            )
        })
        .collect();

    let is_event_stream = headers
        .iter()
        .any(|(k, v)| k.eq_ignore_ascii_case("content-type") && v.starts_with("text/event-stream"));
    let chunk_cb = on_stream_chunk.filter(|_| is_event_stream);

    let mut body: Vec<u8> = Vec::new();
    let mut body_truncated = false;
    let mut size: usize = 0;
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        size += chunk.len();
        if let Some(cb) = &chunk_cb {
            cb(&String::from_utf8_lossy(&chunk));
        }
        if body.len() < max_captured {
            let take = (max_captured - body.len()).min(chunk.len());
            body.extend_from_slice(&chunk[..take]);
            if take < chunk.len() {
                body_truncated = true;
            }
        } else {
            body_truncated = true;
        }
    }
    let duration_ms = started.elapsed().as_secs_f64() * 1000.0;

    // Partial breakdown: reqwest hides the connection phases inside TTFB.
    let timings = Timings {
        dns_ms: None,
        connect_ms: None,
        tls_ms: None,
        server_ms: ttfb_ms,
        download_ms: (duration_ms - ttfb_ms).max(0.0),
        total_ms: duration_ms,
        redirects: 0,
    };

    Ok(HttpResponseData {
        status: status.as_u16(),
        status_text: status.canonical_reason().unwrap_or("").to_owned(),
        http_version,
        headers,
        body,
        body_truncated,
        size,
        duration_ms,
        ttfb_ms,
        timings,
    })
}

fn build_client(
    jar: Arc<CookieStoreMutex>,
    settings: &SendSettings,
    app: &AppSettings,
) -> Result<reqwest::Client, EngineError> {
    let redirect = if settings.follow_redirects {
        reqwest::redirect::Policy::limited(settings.max_redirects)
    } else {
        reqwest::redirect::Policy::none()
    };
    // A client per request keeps redirect/TLS settings request-scoped; the
    // cookie jar is shared app-wide so sessions survive across requests.
    let mut builder = reqwest::Client::builder()
        .cookie_provider(jar)
        .redirect(redirect)
        .danger_accept_invalid_certs(!settings.verify_ssl);
    // timeout 0 = no timeout (long-lived SSE/streaming connections).
    if settings.timeout_ms > 0 {
        builder = builder.timeout(std::time::Duration::from_millis(settings.timeout_ms));
    }

    builder = match app.proxy_mode.as_str() {
        "none" => builder.no_proxy(),
        "custom" if !app.proxy_url.is_empty() => builder.proxy(
            reqwest::Proxy::all(&app.proxy_url).map_err(|e| EngineError::Proxy(e.to_string()))?,
        ),
        _ => builder, // "system": reqwest picks up the platform/env proxy
    };

    for path in &app.ca_cert_paths {
        if path.trim().is_empty() {
            continue;
        }
        let pem = std::fs::read(path).map_err(|source| EngineError::File {
            path: path.clone(),
            source,
        })?;
        for cert in reqwest::Certificate::from_pem_bundle(&pem)
            .map_err(|e| EngineError::Certificate(e.to_string()))?
        {
            builder = builder.add_root_certificate(cert);
        }
    }

    if !app.client_cert_path.trim().is_empty() {
        let der = std::fs::read(&app.client_cert_path).map_err(|source| EngineError::File {
            path: app.client_cert_path.clone(),
            source,
        })?;
        let identity = reqwest::Identity::from_pkcs12_der(&der, &app.client_cert_password)
            .map_err(|e| EngineError::Certificate(e.to_string()))?;
        builder = builder.identity(identity);
    }

    Ok(builder.build()?)
}

fn build_request(
    client: &reqwest::Client,
    spec: &RequestSpec,
) -> Result<reqwest::Request, EngineError> {
    let method = reqwest::Method::from_bytes(spec.method.trim().as_bytes())
        .map_err(|_| EngineError::Method(spec.method.clone()))?;

    let mut headers = HeaderMap::new();
    for h in spec
        .headers
        .iter()
        .filter(|h| h.enabled && !h.key.is_empty())
    {
        let name = HeaderName::from_bytes(h.key.trim().as_bytes())
            .map_err(|_| EngineError::HeaderName(h.key.clone()))?;
        let value = HeaderValue::from_str(h.value.trim())
            .map_err(|_| EngineError::HeaderValue(h.key.clone()))?;
        headers.append(name, value);
    }

    let mut builder = client.request(method, &spec.url).headers(headers.clone());

    builder = match &spec.body {
        BodySpec::None => builder,
        BodySpec::Raw { content_type, text } => {
            if !content_type.is_empty() && !headers.contains_key(CONTENT_TYPE) {
                builder = builder.header(CONTENT_TYPE, content_type);
            }
            builder.body(text.clone())
        }
        BodySpec::UrlEncoded { fields } => {
            let pairs: Vec<(&str, &str)> = fields
                .iter()
                .filter(|f| f.enabled && !f.key.is_empty())
                .map(|f| (f.key.as_str(), f.value.as_str()))
                .collect();
            builder.form(&pairs)
        }
        BodySpec::FormData { fields } => {
            let mut form = reqwest::multipart::Form::new();
            for f in fields.iter().filter(|f| f.enabled && !f.key.is_empty()) {
                if f.is_file {
                    let bytes = std::fs::read(&f.value).map_err(|source| EngineError::File {
                        path: f.value.clone(),
                        source,
                    })?;
                    let file_name = std::path::Path::new(&f.value)
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_else(|| "file".to_owned());
                    form = form.part(
                        f.key.clone(),
                        reqwest::multipart::Part::bytes(bytes).file_name(file_name),
                    );
                } else {
                    form = form.text(f.key.clone(), f.value.clone());
                }
            }
            builder.multipart(form)
        }
        BodySpec::Binary { path } => {
            let bytes = std::fs::read(path).map_err(|source| EngineError::File {
                path: path.clone(),
                source,
            })?;
            builder.body(bytes)
        }
        BodySpec::Graphql { query, variables } => {
            let vars: serde_json::Value = serde_json::from_str(variables)
                .unwrap_or(serde_json::Value::Object(Default::default()));
            let payload = serde_json::json!({ "query": query, "variables": vars });
            if !headers.contains_key(CONTENT_TYPE) {
                builder = builder.header(CONTENT_TYPE, "application/json");
            }
            builder.body(payload.to_string())
        }
    };

    Ok(builder.build()?)
}
