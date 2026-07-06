//! Instrumented HTTP client: opens the socket ourselves so we can time each
//! phase (DNS → TCP → TLS → server → download) that reqwest hides inside its
//! TTFB. Handles the common case; the caller falls back to reqwest for
//! proxies, custom certificates and multipart bodies.

use std::io;
use std::sync::Arc;
use std::time::{Duration, Instant};

use bytes::Bytes;
use http_body_util::{BodyExt, Full};
use hyper::body::Incoming;
use hyper::{Request as HyperRequest, Response as HyperResponse};
use hyper_util::rt::TokioIo;
use reqwest_cookie_store::CookieStoreMutex;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::TcpStream;

use super::{BodySpec, EngineError, HttpResponseData, RequestSpec, StreamChunkFn, Timings};
use crate::settings::AppSettings;

const MAX_REDIRECT_HOPS: usize = 20;

/// Whether the instrumented path can serve this request. Proxies, custom
/// trust anchors and multipart uploads need reqwest.
pub fn eligible(spec: &RequestSpec, app: &AppSettings) -> bool {
    if app.proxy_mode == "custom"
        || (app.proxy_mode == "system" && has_env_proxy())
        || !app.ca_cert_paths.iter().all(|p| p.trim().is_empty())
        || !app.client_cert_path.trim().is_empty()
    {
        return false;
    }
    if matches!(
        spec.body,
        BodySpec::FormData { .. } | BodySpec::Binary { .. }
    ) {
        // multipart boundaries / large file bodies: let reqwest handle it.
        return false;
    }
    matches!(
        url::Url::parse(&spec.url).ok().map(|u| u.scheme().to_owned()),
        Some(ref s) if s == "http" || s == "https"
    )
}

fn has_env_proxy() -> bool {
    ["HTTP_PROXY", "HTTPS_PROXY", "http_proxy", "https_proxy"]
        .iter()
        .any(|k| std::env::var(k).is_ok())
}

pub async fn execute(
    jar: Arc<CookieStoreMutex>,
    spec: &RequestSpec,
    app: &AppSettings,
    on_stream_chunk: Option<StreamChunkFn>,
) -> Result<HttpResponseData, EngineError> {
    let max_captured = (app.max_captured_body_kb as usize).max(64) * 1024;
    let timeout = if spec.settings.timeout_ms == 0 {
        None
    } else {
        Some(Duration::from_millis(spec.settings.timeout_ms))
    };

    let started = Instant::now();
    let mut url = url::Url::parse(&spec.url).map_err(|e| EngineError::Method(e.to_string()))?;
    let mut method = spec.method.trim().to_uppercase();
    let mut body_bytes = request_body(spec);
    let content_type = request_content_type(spec);

    // Accumulate per-phase time across redirect hops.
    let mut acc = Timings::default();
    let mut hops = 0usize;

    loop {
        let deadline = timeout.map(|t| started + t);
        let hop = one_hop(
            &jar,
            &url,
            &method,
            &spec.headers,
            content_type.as_deref(),
            &body_bytes,
            spec.settings.verify_ssl,
            deadline,
            max_captured,
            on_stream_chunk.as_ref(),
        )
        .await?;

        acc.dns_ms = sum_opt(acc.dns_ms, hop.dns_ms);
        acc.connect_ms = sum_opt(acc.connect_ms, hop.connect_ms);
        acc.tls_ms = sum_opt(acc.tls_ms, hop.tls_ms);
        acc.server_ms += hop.server_ms;
        acc.download_ms += hop.download_ms;

        let is_redirect = matches!(hop.status, 301 | 302 | 303 | 307 | 308);
        let location = hop
            .headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case("location"))
            .map(|(_, v)| v.clone());

        if spec.settings.follow_redirects
            && is_redirect
            && hops < spec.settings.max_redirects.min(MAX_REDIRECT_HOPS)
        {
            if let Some(loc) = location {
                if let Ok(next) = url.join(&loc) {
                    // 303, and 301/302 on POST, become GET without a body.
                    if hop.status == 303 || (matches!(hop.status, 301 | 302) && method != "GET") {
                        method = "GET".into();
                        body_bytes = Vec::new();
                    }
                    url = next;
                    hops += 1;
                    acc.redirects = hops as u32;
                    continue;
                }
            }
        }

        let total_ms = started.elapsed().as_secs_f64() * 1000.0;
        acc.total_ms = total_ms;
        let ttfb_ms = total_ms - hop.download_ms;

        return Ok(HttpResponseData {
            status: hop.status,
            status_text: hop.status_text,
            http_version: hop.http_version,
            headers: hop.headers,
            body: hop.body,
            body_truncated: hop.body_truncated,
            size: hop.size,
            duration_ms: total_ms,
            ttfb_ms,
            timings: acc,
        });
    }
}

fn sum_opt(a: Option<f64>, b: Option<f64>) -> Option<f64> {
    match (a, b) {
        (Some(x), Some(y)) => Some(x + y),
        (Some(x), None) | (None, Some(x)) => Some(x),
        (None, None) => None,
    }
}

struct HopResult {
    status: u16,
    status_text: String,
    http_version: String,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
    body_truncated: bool,
    size: usize,
    dns_ms: Option<f64>,
    connect_ms: Option<f64>,
    tls_ms: Option<f64>,
    server_ms: f64,
    download_ms: f64,
}

#[allow(clippy::too_many_arguments)]
async fn one_hop(
    jar: &Arc<CookieStoreMutex>,
    url: &url::Url,
    method: &str,
    extra_headers: &[super::KeyValue],
    content_type: Option<&str>,
    body: &[u8],
    verify_ssl: bool,
    deadline: Option<Instant>,
    max_captured: usize,
    on_stream_chunk: Option<&StreamChunkFn>,
) -> Result<HopResult, EngineError> {
    let host = url
        .host_str()
        .ok_or_else(|| EngineError::Method("url has no host".into()))?
        .to_owned();
    let port = url
        .port_or_known_default()
        .ok_or_else(|| EngineError::Method("url has no port".into()))?;
    let is_https = url.scheme() == "https";

    // ---- DNS ----
    let t = Instant::now();
    let mut addrs = with_deadline(deadline, tokio::net::lookup_host((host.as_str(), port)))
        .await?
        .map_err(|e| EngineError::Connect(format!("dns: {e}")))?;
    let addr = addrs
        .next()
        .ok_or_else(|| EngineError::Connect("dns returned no addresses".into()))?;
    let dns_ms = Some(ms(t));

    // ---- TCP ----
    let t = Instant::now();
    let tcp = with_deadline(deadline, TcpStream::connect(addr))
        .await?
        .map_err(|e| EngineError::Connect(format!("connect: {e}")))?;
    tcp.set_nodelay(true).ok();
    let connect_ms = Some(ms(t));

    // Cookie header from the shared jar.
    let cookie_header = {
        let store = jar
            .lock()
            .map_err(|_| EngineError::Connect("cookie jar".into()))?;
        let pairs: Vec<String> = store
            .get_request_values(url)
            .map(|(k, v)| format!("{k}={v}"))
            .collect();
        (!pairs.is_empty()).then(|| pairs.join("; "))
    };

    if is_https {
        // ---- TLS ----
        let t = Instant::now();
        let connector = native_tls::TlsConnector::builder()
            .danger_accept_invalid_certs(!verify_ssl)
            .danger_accept_invalid_hostnames(!verify_ssl)
            .build()
            .map_err(|e| EngineError::Certificate(e.to_string()))?;
        let connector = tokio_native_tls::TlsConnector::from(connector);
        let tls = with_deadline(deadline, connector.connect(&host, tcp))
            .await?
            .map_err(|e| EngineError::Connect(format!("tls: {e}")))?;
        let tls_ms = Some(ms(t));
        exchange(
            TokioIo::new(tls),
            url,
            &host,
            port,
            method,
            extra_headers,
            content_type,
            cookie_header,
            body,
            jar,
            deadline,
            max_captured,
            on_stream_chunk,
            dns_ms,
            connect_ms,
            tls_ms,
        )
        .await
    } else {
        exchange(
            TokioIo::new(tcp),
            url,
            &host,
            port,
            method,
            extra_headers,
            content_type,
            cookie_header,
            body,
            jar,
            deadline,
            max_captured,
            on_stream_chunk,
            dns_ms,
            connect_ms,
            None,
        )
        .await
    }
}

#[allow(clippy::too_many_arguments)]
async fn exchange<S>(
    io: TokioIo<S>,
    url: &url::Url,
    host: &str,
    port: u16,
    method: &str,
    extra_headers: &[super::KeyValue],
    content_type: Option<&str>,
    cookie_header: Option<String>,
    body: &[u8],
    jar: &Arc<CookieStoreMutex>,
    deadline: Option<Instant>,
    max_captured: usize,
    on_stream_chunk: Option<&StreamChunkFn>,
    dns_ms: Option<f64>,
    connect_ms: Option<f64>,
    tls_ms: Option<f64>,
) -> Result<HopResult, EngineError>
where
    S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    let (mut sender, conn) = hyper::client::conn::http1::handshake(io)
        .await
        .map_err(|e| EngineError::Connect(e.to_string()))?;
    tokio::spawn(async move {
        let _ = conn.await;
    });

    // Host header (with non-default port).
    let host_header =
        if (url.scheme() == "https" && port == 443) || (url.scheme() == "http" && port == 80) {
            host.to_owned()
        } else {
            format!("{host}:{port}")
        };
    let target = match url.query() {
        Some(q) => format!("{}?{}", url.path(), q),
        None => url.path().to_owned(),
    };

    let mut builder = HyperRequest::builder()
        .method(method)
        .uri(target)
        .header("host", host_header);
    for h in extra_headers
        .iter()
        .filter(|h| h.enabled && !h.key.is_empty())
    {
        // Host is derived from the URL; let explicit ones through otherwise.
        builder = builder.header(h.key.as_str(), h.value.as_str());
    }
    let has_ct = extra_headers
        .iter()
        .any(|h| h.enabled && h.key.eq_ignore_ascii_case("content-type"));
    if let (Some(ct), false) = (content_type, has_ct) {
        builder = builder.header("content-type", ct);
    }
    if !body.is_empty() {
        builder = builder.header("content-length", body.len().to_string());
    }
    if let Some(cookie) = cookie_header {
        builder = builder.header("cookie", cookie);
    }
    // Accept-encoding we can actually decode.
    builder = builder.header("accept-encoding", "gzip, deflate, br");

    let request = builder
        .body(Full::new(Bytes::copy_from_slice(body)))
        .map_err(|e| EngineError::HeaderValue(e.to_string()))?;

    // ---- server: send + wait for response head ----
    let t = Instant::now();
    let response: HyperResponse<Incoming> = with_deadline(deadline, sender.send_request(request))
        .await?
        .map_err(|e| EngineError::Connect(e.to_string()))?;
    let server_ms = ms(t);

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

    // Store Set-Cookie back into the jar.
    let set_cookies: Vec<String> = headers
        .iter()
        .filter(|(k, _)| k.eq_ignore_ascii_case("set-cookie"))
        .map(|(_, v)| v.clone())
        .collect();
    if !set_cookies.is_empty() {
        if let Ok(mut store) = jar.lock() {
            for raw in &set_cookies {
                if let Ok(c) = cookie_store::Cookie::parse(raw.as_str(), url) {
                    let _ = store.insert(c.into_owned(), url);
                }
            }
        }
    }

    let is_event_stream = headers
        .iter()
        .any(|(k, v)| k.eq_ignore_ascii_case("content-type") && v.starts_with("text/event-stream"));
    let chunk_cb = on_stream_chunk.filter(|_| is_event_stream);

    // ---- download ----
    let t = Instant::now();
    let mut raw: Vec<u8> = Vec::new();
    let mut size = 0usize;
    let mut truncated = false;
    let mut incoming = response.into_body();
    loop {
        let frame = with_deadline(deadline, incoming.frame()).await?;
        let Some(frame) = frame else { break };
        let frame = frame.map_err(|e| EngineError::Connect(e.to_string()))?;
        if let Ok(chunk) = frame.into_data() {
            size += chunk.len();
            if let Some(cb) = &chunk_cb {
                cb(&String::from_utf8_lossy(&chunk));
            }
            if raw.len() < max_captured {
                let take = (max_captured - raw.len()).min(chunk.len());
                raw.extend_from_slice(&chunk[..take]);
                if take < chunk.len() {
                    truncated = true;
                }
            } else {
                truncated = true;
            }
        }
    }
    let download_ms = ms(t);

    let encoding = headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case("content-encoding"))
        .map(|(_, v)| v.to_lowercase());
    let body = decompress(raw, encoding.as_deref());

    Ok(HopResult {
        status: status.as_u16(),
        status_text: status.canonical_reason().unwrap_or("").to_owned(),
        http_version,
        headers,
        body,
        body_truncated: truncated,
        size,
        dns_ms,
        connect_ms,
        tls_ms,
        server_ms,
        download_ms,
    })
}

/// Apply the overall request timeout to an await, if one is set.
async fn with_deadline<F: std::future::Future>(
    deadline: Option<Instant>,
    fut: F,
) -> Result<F::Output, EngineError> {
    match deadline {
        None => Ok(fut.await),
        Some(at) => {
            let now = Instant::now();
            if now >= at {
                return Err(EngineError::Connect("request timed out".into()));
            }
            tokio::time::timeout(at - now, fut)
                .await
                .map_err(|_| EngineError::Connect("request timed out".into()))
        }
    }
}

fn ms(t: Instant) -> f64 {
    t.elapsed().as_secs_f64() * 1000.0
}

fn request_body(spec: &RequestSpec) -> Vec<u8> {
    match &spec.body {
        BodySpec::None => Vec::new(),
        BodySpec::Raw { text, .. } => text.clone().into_bytes(),
        BodySpec::UrlEncoded { fields } => fields
            .iter()
            .filter(|f| f.enabled && !f.key.is_empty())
            .map(|f| format!("{}={}", urlencode(&f.key), urlencode(&f.value)))
            .collect::<Vec<_>>()
            .join("&")
            .into_bytes(),
        BodySpec::Graphql { query, variables } => {
            let vars: serde_json::Value = serde_json::from_str(variables).unwrap_or_default();
            serde_json::json!({ "query": query, "variables": vars })
                .to_string()
                .into_bytes()
        }
        // Excluded by eligible().
        BodySpec::FormData { .. } | BodySpec::Binary { .. } => Vec::new(),
    }
}

fn request_content_type(spec: &RequestSpec) -> Option<String> {
    match &spec.body {
        BodySpec::Raw { content_type, .. } if !content_type.is_empty() => {
            Some(content_type.clone())
        }
        BodySpec::UrlEncoded { .. } => Some("application/x-www-form-urlencoded".into()),
        BodySpec::Graphql { .. } => Some("application/json".into()),
        _ => None,
    }
}

fn urlencode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

fn decompress(raw: Vec<u8>, encoding: Option<&str>) -> Vec<u8> {
    use std::io::Read;
    match encoding {
        Some(e) if e.contains("gzip") => {
            let mut out = Vec::new();
            if flate2::read::GzDecoder::new(&raw[..])
                .read_to_end(&mut out)
                .is_ok()
            {
                out
            } else {
                raw
            }
        }
        Some(e) if e.contains("deflate") => {
            let mut out = Vec::new();
            if flate2::read::ZlibDecoder::new(&raw[..])
                .read_to_end(&mut out)
                .is_ok()
            {
                out
            } else {
                raw
            }
        }
        Some(e) if e.contains("br") => {
            let mut out = Vec::new();
            let mut r = brotli::Decompressor::new(&raw[..], 4096);
            if io::Read::read_to_end(&mut r, &mut out).is_ok() {
                out
            } else {
                raw
            }
        }
        _ => raw,
    }
}
