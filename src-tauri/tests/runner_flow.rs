//! End-to-end Collection Runner test against a local HTTP server:
//! script chains, variable passing between requests, iterations with data
//! rows, and setNextRequest flow control.
#![allow(clippy::unwrap_used)]

use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use postcat_lib::collections;
use postcat_lib::http_engine::{BodySpec, RequestSpec};
use postcat_lib::runner::{self, RunOptions};
use postcat_lib::store::Store;
use reqwest_cookie_store::{CookieStore, CookieStoreMutex};

fn spec(method: &str, url: &str) -> RequestSpec {
    RequestSpec {
        method: method.into(),
        url: url.into(),
        body: BodySpec::None,
        ..Default::default()
    }
}

/// Echo server: GET /login -> {"token":"tok-7"}; GET /orders?token=X echoes
/// the token back; runs until the listener thread is dropped.
fn spawn_server() -> (String, std::thread::JoinHandle<Vec<String>>) {
    let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
    let addr = format!("http://{}", server.server_addr());
    let handle = std::thread::spawn(move || {
        let mut seen = Vec::new();
        // The test issues a bounded number of requests; stop when idle.
        while let Ok(Some(request)) = server.recv_timeout(std::time::Duration::from_millis(2500)) {
            seen.push(request.url().to_string());
            let response = if request.url().starts_with("/login") {
                tiny_http::Response::from_string(r#"{"token":"tok-7"}"#)
            } else {
                tiny_http::Response::from_string(format!(r#"{{"echo":"{}"}}"#, request.url()))
            }
            .with_header(
                tiny_http::Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..])
                    .unwrap(),
            );
            let _ = request.respond(response);
        }
        seen
    });
    (addr, handle)
}

#[tokio::test(flavor = "multi_thread")]
async fn runner_chains_scripts_vars_and_data() {
    let (addr, server) = spawn_server();
    let store = Store::open_in_memory().unwrap();
    let cid = collections::create(&store, "Flow").unwrap();

    // Request 1: login — test script extracts the token into an env-less
    // collection variable.
    let login = collections::item_create(
        &store,
        cid,
        None,
        "request",
        "Login",
        Some(&spec("GET", &format!("{addr}/login"))),
    )
    .unwrap();
    store
        .with_conn(|conn| {
            conn.execute(
                "UPDATE collection_items SET test_script = ?2 WHERE id = ?1",
                rusqlite::params![
                    login,
                    r#"
                    pm.test("login ok", () => pm.response.to.have.status(200));
                    pm.collectionVariables.set("token", pm.response.json().token);
                    "#
                ],
            )?;
            Ok(())
        })
        .unwrap();

    // Request 2: uses {{token}} from request 1.
    let orders = collections::item_create(
        &store,
        cid,
        None,
        "request",
        "Orders",
        Some(&spec(
            "GET",
            &format!("{addr}/orders?token={{{{token}}}}&row={{{{row}}}}"),
        )),
    )
    .unwrap();
    store
        .with_conn(|conn| {
            conn.execute(
                "UPDATE collection_items SET test_script = ?2 WHERE id = ?1",
                rusqlite::params![
                    orders,
                    r#"
                    pm.test("token flowed", function () {
                        pm.expect(pm.response.json().echo).to.include("token=tok-7");
                    });
                    "#
                ],
            )?;
            Ok(())
        })
        .unwrap();

    let jar = Arc::new(CookieStoreMutex::new(CookieStore::default()));
    let report = runner::run(
        &store,
        jar,
        RunOptions {
            collection_id: cid,
            folder_id: None,
            iterations: 2,
            delay_ms: 0,
            data: Some(vec![
                serde_json::json!({"row": "alpha"}),
                serde_json::json!({"row": "beta"}),
            ]),
        },
        Arc::new(AtomicBool::new(false)),
        |_| {},
    )
    .await;

    assert_eq!(report.total_requests, 4, "{:?}", report.results);
    assert_eq!(report.errors, 0, "{:?}", report.results);
    assert_eq!(report.failed_tests, 0, "{:?}", report.results);
    assert_eq!(report.passed_tests, 4);

    // Data rows landed in URLs per iteration.
    let urls: Vec<&str> = report.results.iter().map(|r| r.url.as_str()).collect();
    assert!(urls.iter().any(|u| u.contains("row=alpha")));
    assert!(urls.iter().any(|u| u.contains("row=beta")));

    drop(report);
    let seen = server.join().unwrap();
    assert_eq!(seen.len(), 4);
}

#[tokio::test(flavor = "multi_thread")]
async fn set_next_request_stops_iteration() {
    let (addr, server) = spawn_server();
    let store = Store::open_in_memory().unwrap();
    let cid = collections::create(&store, "Stopper").unwrap();

    let first = collections::item_create(
        &store,
        cid,
        None,
        "request",
        "First",
        Some(&spec("GET", &format!("{addr}/login"))),
    )
    .unwrap();
    store
        .with_conn(|conn| {
            conn.execute(
                "UPDATE collection_items SET test_script = ?2 WHERE id = ?1",
                rusqlite::params![first, "pm.execution.setNextRequest(null);"],
            )?;
            Ok(())
        })
        .unwrap();
    collections::item_create(
        &store,
        cid,
        None,
        "request",
        "Never",
        Some(&spec("GET", &format!("{addr}/never"))),
    )
    .unwrap();

    let jar = Arc::new(CookieStoreMutex::new(CookieStore::default()));
    let report = runner::run(
        &store,
        jar,
        RunOptions {
            collection_id: cid,
            folder_id: None,
            iterations: 1,
            delay_ms: 0,
            data: None,
        },
        Arc::new(AtomicBool::new(false)),
        |_| {},
    )
    .await;

    assert_eq!(report.total_requests, 1);
    assert_eq!(report.results[0].name, "First");

    let seen = server.join().unwrap();
    assert_eq!(seen, vec!["/login".to_string()]);
}
