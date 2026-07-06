//! Live network check for the instrumented engine (TLS + gzip). Ignored by
//! default; run with: cargo test --test timing_live -- --ignored --nocapture
#![allow(clippy::unwrap_used)]

use std::sync::Arc;

use postcat_lib::http_engine::{self, RequestSpec};
use postcat_lib::settings::AppSettings;
use reqwest_cookie_store::{CookieStore, CookieStoreMutex};

#[tokio::test(flavor = "multi_thread")]
#[ignore]
async fn https_full_waterfall() {
    let spec = RequestSpec {
        url: "https://httpbin.org/gzip".into(),
        ..Default::default()
    };
    let jar = Arc::new(CookieStoreMutex::new(CookieStore::default()));
    let resp = http_engine::execute(jar, &spec, &AppSettings::default())
        .await
        .unwrap();

    let t = &resp.timings;
    println!(
        "dns={:?} connect={:?} tls={:?} server={:.1} download={:.1} total={:.1} redirects={}",
        t.dns_ms, t.connect_ms, t.tls_ms, t.server_ms, t.download_ms, t.total_ms, t.redirects
    );
    assert_eq!(resp.status, 200);
    assert!(t.dns_ms.is_some());
    assert!(t.connect_ms.is_some());
    assert!(t.tls_ms.is_some(), "TLS phase must be measured for https");
    assert!(t.server_ms > 0.0);
    // gzip body was transparently decompressed to JSON.
    assert!(
        String::from_utf8_lossy(&resp.body).contains("\"gzipped\": true"),
        "decompressed gzip body"
    );
}
