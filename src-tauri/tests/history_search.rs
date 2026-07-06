//! Full-text search over history: correctness and (ignored) 100k benchmark.
#![allow(clippy::unwrap_used)]

use postcat_lib::history::{self, Endpoint, SearchFilters};
use postcat_lib::http_engine::{BodySpec, HttpResponseData, RequestSpec, SendSettings};
use postcat_lib::store::Store;

fn spec(method: &str, url: &str, body: Option<&str>) -> RequestSpec {
    RequestSpec {
        method: method.into(),
        url: url.into(),
        headers: vec![],
        body: body
            .map(|text| BodySpec::Raw {
                content_type: "application/json".into(),
                text: text.into(),
            })
            .unwrap_or(BodySpec::None),
        settings: SendSettings::default(),
    }
}

fn response(status: u16, body: &str) -> HttpResponseData {
    HttpResponseData {
        status,
        status_text: "".into(),
        http_version: "HTTP/1.1".into(),
        headers: vec![("content-type".into(), "application/json".into())],
        body: body.as_bytes().to_vec(),
        body_truncated: false,
        size: body.len(),
        duration_ms: 12.0,
        ttfb_ms: 6.0,
    }
}

fn q(text: &str) -> SearchFilters {
    SearchFilters {
        query: Some(text.into()),
        ..Default::default()
    }
}

#[test]
fn finds_by_response_body_content() {
    let store = Store::open_in_memory().unwrap();
    history::record(
        &store,
        &spec("GET", "https://api.shop.dev/orders/42", None),
        Ok(&response(200, r#"{"order_id":"ord_9f31","total":99}"#)),
    )
    .unwrap();
    history::record(
        &store,
        &spec("GET", "https://api.shop.dev/users", None),
        Ok(&response(200, r#"{"users":[]}"#)),
    )
    .unwrap();

    // The Postman pain point: find the request whose RESPONSE contained a value.
    let hits = history::search(&store, &q("ord_9f31"), 50, 0).unwrap();
    assert_eq!(hits.len(), 1);
    assert!(hits[0].url.contains("/orders/42"));
    assert!(hits[0].snippet.as_deref().unwrap_or("").contains("[["));

    // And by request body content.
    history::record(
        &store,
        &spec(
            "POST",
            "https://api.shop.dev/orders",
            Some(r#"{"sku":"WIDGET-7"}"#),
        ),
        Ok(&response(201, "{}")),
    )
    .unwrap();
    let hits = history::search(&store, &q("WIDGET-7"), 50, 0).unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].method, "POST");
}

#[test]
fn url_substring_via_trigram() {
    let store = Store::open_in_memory().unwrap();
    history::record(
        &store,
        &spec("GET", "https://internal.example.com/v2/warehouse", None),
        Ok(&response(200, "{}")),
    )
    .unwrap();

    // "hous" is a mid-token substring — the word index alone can't find it.
    let hits = history::search(&store, &q("hous"), 50, 0).unwrap();
    assert_eq!(hits.len(), 1);
}

#[test]
fn filters_combine_with_text_query() {
    let store = Store::open_in_memory().unwrap();
    for (method, url, status) in [
        ("GET", "https://a.dev/items", 200),
        ("POST", "https://a.dev/items", 500),
        ("GET", "https://b.dev/items", 404),
    ] {
        history::record(
            &store,
            &spec(method, url, None),
            Ok(&response(status, r#"{"items":[]}"#)),
        )
        .unwrap();
    }

    let mut f = q("items");
    f.status_class = Some(5);
    let hits = history::search(&store, &f, 50, 0).unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].status, Some(500));

    let f = SearchFilters {
        method: Some("get".into()),
        host: Some("b.dev".into()),
        ..Default::default()
    };
    let hits = history::search(&store, &f, 50, 0).unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].status, Some(404));

    let f = SearchFilters {
        errors_only: true,
        ..Default::default()
    };
    assert_eq!(history::search(&store, &f, 50, 0).unwrap().len(), 2);
}

#[test]
fn endpoint_grouping_and_drilldown() {
    let store = Store::open_in_memory().unwrap();
    for i in 0..3 {
        history::record(
            &store,
            &spec("GET", &format!("https://a.dev/items?page={i}"), None),
            Ok(&response(200, "{}")),
        )
        .unwrap();
    }
    history::record(
        &store,
        &spec("DELETE", "https://a.dev/items/1", None),
        Ok(&response(204, "")),
    )
    .unwrap();

    let groups = history::endpoints(&store, 100).unwrap();
    assert_eq!(groups.len(), 2);
    let get_group = groups
        .iter()
        .find(|g| g.method == "GET" && g.url_base == "https://a.dev/items")
        .unwrap();
    assert_eq!(get_group.count, 3);

    let f = SearchFilters {
        endpoint: Some(Endpoint {
            method: "GET".into(),
            url_base: "https://a.dev/items".into(),
        }),
        ..Default::default()
    };
    assert_eq!(history::search(&store, &f, 50, 0).unwrap().len(), 3);
}

#[test]
fn label_is_searchable_and_pin_survives_everything() {
    let store = Store::open_in_memory().unwrap();
    let id = history::record(
        &store,
        &spec("GET", "https://a.dev/one", None),
        Ok(&response(200, "{}")),
    )
    .unwrap();
    for _ in 0..5 {
        history::record(
            &store,
            &spec("GET", "https://a.dev/noise", None),
            Ok(&response(200, "{}")),
        )
        .unwrap();
    }

    history::set_pinned(&store, id, true).unwrap();
    history::set_label(&store, id, Some("golden token flow".into())).unwrap();

    // Label is in the full-text index (via the UPDATE trigger).
    let hits = history::search(&store, &q("golden"), 50, 0).unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].id, id);

    // Retention by count keeps the pinned entry regardless.
    history::retention_set(
        &store,
        history::RetentionSettings {
            max_age_days: 0,
            max_entries: 2,
        },
    )
    .unwrap();
    let all = history::search(&store, &SearchFilters::default(), 50, 0).unwrap();
    assert_eq!(all.len(), 3); // 2 newest + the pinned one
    assert!(all.iter().any(|h| h.id == id));

    // Clear keeps pinned too.
    history::clear(&store).unwrap();
    let all = history::search(&store, &SearchFilters::default(), 50, 0).unwrap();
    assert_eq!(all.len(), 1);
    assert_eq!(all[0].id, id);
}

/// Benchmark, not a correctness test: seeds 100k entries and times search.
/// Run manually: cargo test --release --test history_search bench_100k -- --ignored --nocapture
#[test]
#[ignore]
fn bench_100k_search_under_50ms() {
    let dir = std::env::temp_dir().join("postcat-bench");
    std::fs::create_dir_all(&dir).unwrap();
    let db = dir.join("bench.db");
    let _ = std::fs::remove_file(&db);
    let store = Store::open(&db).unwrap();

    let hosts = ["api.shop.dev", "auth.corp.io", "billing.acme.com"];
    let paths = ["/v1/users", "/v1/orders", "/v2/items", "/health", "/login"];
    let started = std::time::Instant::now();
    for i in 0..100_000u32 {
        let host = hosts[(i % 3) as usize];
        let path = paths[(i % 5) as usize];
        let body =
            format!(r#"{{"seq":{i},"token":"tok_{i:06}","note":"payload row {i} for {host}"}}"#);
        history::record(
            &store,
            &spec(
                "GET",
                &format!("https://{host}{path}?page={}", i % 50),
                None,
            ),
            Ok(&response(if i % 17 == 0 { 500 } else { 200 }, &body)),
        )
        .unwrap();
    }
    println!("seeded 100k in {:.1?}", started.elapsed());

    for query in ["tok_042", "payload", "billing", "orders", "row 99321"] {
        let t = std::time::Instant::now();
        let hits = history::search(&store, &q(query), 100, 0).unwrap();
        let elapsed = t.elapsed();
        println!("search '{query}': {} hits in {elapsed:.1?}", hits.len());
        assert!(
            elapsed.as_millis() < 50,
            "search '{query}' took {elapsed:?} (target < 50ms)"
        );
    }
}
