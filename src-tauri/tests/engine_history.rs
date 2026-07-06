//! End-to-end test of the request pipeline without the UI: a local HTTP
//! server, the reqwest engine, and history recording into SQLite.
#![allow(clippy::unwrap_used)]

use std::sync::Arc;

use postcat_lib::settings::AppSettings;
use reqwest_cookie_store::{CookieStore, CookieStoreMutex};

use postcat_lib::history::{self, SearchFilters};
use postcat_lib::http_engine::{self, BodySpec, KeyValue, RequestSpec, SendSettings};
use postcat_lib::store::Store;

/// record() without variables: original == display, no secrets.
fn rec(
    store: &Store,
    spec: &RequestSpec,
    outcome: Result<&postcat_lib::http_engine::HttpResponseData, &str>,
) -> Result<i64, postcat_lib::store::StoreError> {
    history::record(store, spec, spec, &[], outcome)
}

fn spawn_server() -> (tiny_http::Server, String) {
    let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
    let addr = format!("http://{}", server.server_addr());
    (server, addr)
}

#[tokio::test(flavor = "multi_thread")]
async fn request_is_executed_and_recorded() {
    let (server, addr) = spawn_server();
    let server_thread = std::thread::spawn(move || {
        let mut request = server.recv().unwrap();
        assert_eq!(request.method().as_str(), "POST");
        assert_eq!(request.url(), "/echo?x=1");
        let header_val = request
            .headers()
            .iter()
            .find(|h| h.field.equiv("x-postcat-test"))
            .map(|h| h.value.as_str().to_owned());
        assert_eq!(header_val.as_deref(), Some("yes"));

        let mut body = String::new();
        request.as_reader().read_to_string(&mut body).unwrap();
        assert_eq!(body, r#"{"ping":true}"#);

        let response = tiny_http::Response::from_string(r#"{"pong":true}"#).with_header(
            tiny_http::Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap(),
        );
        request.respond(response).unwrap();
    });

    let spec = RequestSpec {
        method: "POST".into(),
        url: format!("{addr}/echo?x=1"),
        headers: vec![KeyValue {
            key: "X-Postcat-Test".into(),
            value: "yes".into(),
            enabled: true,
        }],
        body: BodySpec::Raw {
            content_type: "application/json".into(),
            text: r#"{"ping":true}"#.into(),
        },
        settings: SendSettings::default(),
        auth: Default::default(),
    };

    let jar = Arc::new(CookieStoreMutex::new(CookieStore::default()));
    let resp = http_engine::execute(jar, &spec, &AppSettings::default())
        .await
        .unwrap();
    server_thread.join().unwrap();

    assert_eq!(resp.status, 200);
    assert_eq!(resp.body, br#"{"pong":true}"#);
    assert!(!resp.body_truncated);
    assert!(resp.duration_ms > 0.0);
    assert!(resp.ttfb_ms > 0.0 && resp.ttfb_ms <= resp.duration_ms);

    let store = Store::open_in_memory().unwrap();
    let id = rec(&store, &spec, Ok(&resp)).unwrap();

    let all = SearchFilters::default();
    let list = history::search(&store, &all, 10, 0).unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].id, id);
    assert_eq!(list[0].method, "POST");
    assert_eq!(list[0].status, Some(200));
    assert_eq!(list[0].host, "127.0.0.1");

    let detail = history::get(&store, id).unwrap();
    assert_eq!(detail.resp_body_text.as_deref(), Some(r#"{"pong":true}"#));
    assert_eq!(detail.req_body_text.as_deref(), Some(r#"{"ping":true}"#));
    assert_eq!(detail.req_spec["method"], "POST");

    // Search: URL text matches, junk does not.
    let by_text = |q: &str| {
        let f = SearchFilters {
            query: Some(q.into()),
            ..Default::default()
        };
        history::search(&store, &f, 10, 0).unwrap().len()
    };
    assert_eq!(by_text("echo"), 1);
    assert_eq!(by_text("nomatch"), 0);
}

#[tokio::test(flavor = "multi_thread")]
async fn network_error_is_recorded() {
    // Nothing listens on this port (reserved, never assigned).
    let spec = RequestSpec {
        method: "GET".into(),
        url: "http://127.0.0.1:1/unreachable".into(),
        headers: vec![],
        body: BodySpec::None,
        settings: SendSettings {
            timeout_ms: 3000,
            ..Default::default()
        },
        auth: Default::default(),
    };

    let jar = Arc::new(CookieStoreMutex::new(CookieStore::default()));
    let err = http_engine::execute(jar, &spec, &AppSettings::default())
        .await
        .unwrap_err();

    let store = Store::open_in_memory().unwrap();
    let id = rec(&store, &spec, Err(&err.to_string())).unwrap();

    let detail = history::get(&store, id).unwrap();
    assert!(detail.summary.error.is_some());
    assert_eq!(detail.summary.status, None);
}
