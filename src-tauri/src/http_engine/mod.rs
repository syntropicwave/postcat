use std::sync::Arc;
use std::time::Instant;

use futures_util::StreamExt;
use reqwest::cookie::Jar;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue, CONTENT_TYPE};
use serde::{Deserialize, Serialize};

/// Hard cap on how much of a response body is kept (stored in history and
/// shown in the UI). Anything past this is discarded and the entry is marked
/// truncated. Made configurable in settings later.
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
        }
    }
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
    #[error("{0}")]
    Request(#[from] reqwest::Error),
}

pub async fn execute(jar: Arc<Jar>, spec: &RequestSpec) -> Result<HttpResponseData, EngineError> {
    let client = build_client(jar, &spec.settings)?;
    let request = build_request(&client, spec)?;

    let started = Instant::now();
    let response = client.execute(request).await?;
    let ttfb_ms = started.elapsed().as_secs_f64() * 1000.0;

    let status = response.status();
    let http_version = format!("{:?}", response.version());
    let headers = response
        .headers()
        .iter()
        .map(|(k, v)| {
            (
                k.as_str().to_owned(),
                String::from_utf8_lossy(v.as_bytes()).into_owned(),
            )
        })
        .collect();

    let mut body: Vec<u8> = Vec::new();
    let mut body_truncated = false;
    let mut size: usize = 0;
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        size += chunk.len();
        if body.len() < MAX_CAPTURED_BODY {
            let take = (MAX_CAPTURED_BODY - body.len()).min(chunk.len());
            body.extend_from_slice(&chunk[..take]);
            if take < chunk.len() {
                body_truncated = true;
            }
        } else {
            body_truncated = true;
        }
    }
    let duration_ms = started.elapsed().as_secs_f64() * 1000.0;

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
    })
}

fn build_client(jar: Arc<Jar>, settings: &SendSettings) -> Result<reqwest::Client, EngineError> {
    let redirect = if settings.follow_redirects {
        reqwest::redirect::Policy::limited(settings.max_redirects)
    } else {
        reqwest::redirect::Policy::none()
    };
    // A client per request keeps redirect/TLS settings request-scoped; the
    // cookie jar is shared app-wide so sessions survive across requests.
    Ok(reqwest::Client::builder()
        .cookie_provider(jar)
        .redirect(redirect)
        .danger_accept_invalid_certs(!settings.verify_ssl)
        .timeout(std::time::Duration::from_millis(settings.timeout_ms))
        .build()?)
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
    };

    Ok(builder.build()?)
}
