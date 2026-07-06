//! WebSocket sessions: connect/send/close with live events to the UI and a
//! history record on close — sent messages become the request body, received
//! messages the response body, so both are full-text searchable.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use futures_util::{SinkExt, StreamExt};
use serde::Serialize;
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::Message;

use crate::history;
use crate::http_engine::{BodySpec, HttpResponseData, KeyValue, RequestSpec};
use crate::store::Store;

#[derive(Debug, Clone, Serialize)]
pub struct WsEvent {
    pub conn_id: String,
    /// "open" | "in" | "out" | "closed" | "error"
    pub kind: String,
    pub text: String,
}

struct Session {
    sender: mpsc::UnboundedSender<Message>,
}

#[derive(Default)]
struct SessionLog {
    url: String,
    sent: Vec<String>,
    received: Vec<String>,
    started: Option<std::time::Instant>,
}

#[derive(Default)]
pub struct WsSessions(Mutex<HashMap<String, Session>>);

pub async fn connect(
    sessions: &WsSessions,
    store: Store,
    conn_id: String,
    url: String,
    headers: Vec<KeyValue>,
    emit: impl Fn(WsEvent) + Send + Sync + 'static,
) -> Result<(), String> {
    let mut request = url
        .clone()
        .into_client_request()
        .map_err(|e| e.to_string())?;
    for h in headers.iter().filter(|h| h.enabled && !h.key.is_empty()) {
        let name: tokio_tungstenite::tungstenite::http::HeaderName =
            h.key.parse().map_err(|_| format!("bad header {}", h.key))?;
        let value = h
            .value
            .parse()
            .map_err(|_| format!("bad value for {}", h.key))?;
        request.headers_mut().insert(name, value);
    }

    let (stream, _resp) = tokio_tungstenite::connect_async(request)
        .await
        .map_err(|e| e.to_string())?;
    let (mut write, mut read) = stream.split();

    let (tx, mut rx) = mpsc::unbounded_channel::<Message>();
    let log = Arc::new(Mutex::new(SessionLog {
        url: url.clone(),
        started: Some(std::time::Instant::now()),
        ..Default::default()
    }));

    if let Ok(mut map) = sessions.0.lock() {
        map.insert(conn_id.clone(), Session { sender: tx });
    }
    emit(WsEvent {
        conn_id: conn_id.clone(),
        kind: "open".into(),
        text: url.clone(),
    });

    let emit = Arc::new(emit);

    // Writer: forwards queued outgoing messages.
    let writer_log = log.clone();
    let writer = tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            if let Message::Text(t) = &msg {
                if let Ok(mut l) = writer_log.lock() {
                    l.sent.push(t.to_string());
                }
            }
            let is_close = matches!(msg, Message::Close(_));
            if write.send(msg).await.is_err() || is_close {
                break;
            }
        }
        let _ = write.close().await;
    });

    // Reader: emits incoming messages until the peer closes.
    let reader_emit = emit.clone();
    let reader_log = log.clone();
    let reader_conn = conn_id.clone();
    tokio::spawn(async move {
        while let Some(msg) = read.next().await {
            match msg {
                Ok(Message::Text(t)) => {
                    if let Ok(mut l) = reader_log.lock() {
                        l.received.push(t.to_string());
                    }
                    reader_emit(WsEvent {
                        conn_id: reader_conn.clone(),
                        kind: "in".into(),
                        text: t.to_string(),
                    });
                }
                Ok(Message::Binary(b)) => {
                    let text = format!("<binary {} bytes>", b.len());
                    if let Ok(mut l) = reader_log.lock() {
                        l.received.push(text.clone());
                    }
                    reader_emit(WsEvent {
                        conn_id: reader_conn.clone(),
                        kind: "in".into(),
                        text,
                    });
                }
                Ok(Message::Close(_)) | Err(_) => break,
                Ok(_) => {} // ping/pong handled by the library
            }
        }
        writer.abort();

        // Session over: record it into history.
        record_session(&store, &reader_log);
        reader_emit(WsEvent {
            conn_id: reader_conn.clone(),
            kind: "closed".into(),
            text: String::new(),
        });
    });

    Ok(())
}

pub fn send(
    sessions: &WsSessions,
    conn_id: &str,
    text: String,
    emit: impl Fn(WsEvent),
) -> Result<(), String> {
    let map = sessions.0.lock().map_err(|_| "lock poisoned")?;
    let session = map.get(conn_id).ok_or("connection not found")?;
    session
        .sender
        .send(Message::Text(text.clone()))
        .map_err(|_| "connection is closed".to_string())?;
    emit(WsEvent {
        conn_id: conn_id.to_owned(),
        kind: "out".into(),
        text,
    });
    Ok(())
}

pub fn close(sessions: &WsSessions, conn_id: &str) {
    if let Ok(mut map) = sessions.0.lock() {
        if let Some(session) = map.remove(conn_id) {
            let _ = session.sender.send(Message::Close(None));
        }
    }
}

fn record_session(store: &Store, log: &Arc<Mutex<SessionLog>>) {
    let Ok(log) = log.lock() else { return };
    let duration_ms = log
        .started
        .map(|s| s.elapsed().as_secs_f64() * 1000.0)
        .unwrap_or(0.0);

    let spec = RequestSpec {
        method: "WS".into(),
        url: log.url.clone(),
        body: if log.sent.is_empty() {
            BodySpec::None
        } else {
            BodySpec::Raw {
                content_type: "text/plain".into(),
                text: log.sent.join("\n"),
            }
        },
        ..Default::default()
    };
    let received = log.received.join("\n");
    let resp = HttpResponseData {
        status: 101,
        status_text: "WebSocket session".into(),
        http_version: "WS".into(),
        headers: vec![],
        size: received.len(),
        body: received.into_bytes(),
        body_truncated: false,
        duration_ms,
        ttfb_ms: 0.0,
        timings: Default::default(),
    };
    if let Err(err) = history::record(store, &spec, &spec, &[], Ok(&resp)) {
        tracing::warn!(%err, "failed to record websocket session");
    }
}
