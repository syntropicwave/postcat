//! Auth helpers: application, inheritance through the collection tree,
//! masking in history, and the OAuth2 client_credentials flow against a
//! local token server.
#![allow(clippy::unwrap_used)]

use postcat_lib::auth::{self, oauth2::OAuth2Config, AuthSpec};
use postcat_lib::collections;
use postcat_lib::http_engine::RequestSpec;
use postcat_lib::store::Store;

fn spec(url: &str) -> RequestSpec {
    RequestSpec {
        url: url.into(),
        ..Default::default()
    }
}

#[test]
fn bearer_and_apikey_apply() {
    let mut s = spec("https://api.dev/data");
    let secrets = auth::apply(
        &mut s,
        &AuthSpec::Bearer {
            token: "tok123".into(),
        },
    );
    assert_eq!(s.headers[0].key, "Authorization");
    assert_eq!(s.headers[0].value, "Bearer tok123");
    assert!(secrets.iter().any(|(v, _)| v == "tok123"));

    let mut s = spec("https://api.dev/data?x=1");
    auth::apply(
        &mut s,
        &AuthSpec::ApiKey {
            key: "api_key".into(),
            value: "k-42".into(),
            in_query: true,
        },
    );
    assert_eq!(s.url, "https://api.dev/data?x=1&api_key=k-42");
}

#[test]
fn explicit_header_beats_auth_helper() {
    let mut s = spec("https://api.dev");
    s.headers.push(postcat_lib::http_engine::KeyValue {
        key: "Authorization".into(),
        value: "custom".into(),
        enabled: true,
    });
    auth::apply(
        &mut s,
        &AuthSpec::Bearer {
            token: "tok".into(),
        },
    );
    let auth_headers: Vec<_> = s
        .headers
        .iter()
        .filter(|h| h.key.eq_ignore_ascii_case("authorization"))
        .collect();
    assert_eq!(auth_headers.len(), 1);
    assert_eq!(auth_headers[0].value, "custom");
}

#[test]
fn inheritance_walks_folders_then_collection() {
    let store = Store::open_in_memory().unwrap();
    let cid = collections::create(&store, "C").unwrap();
    let folder = collections::item_create(&store, cid, None, "folder", "F", None).unwrap();
    let req = collections::item_create(
        &store,
        cid,
        Some(folder),
        "request",
        "R",
        Some(&spec("https://a.dev")),
    )
    .unwrap();

    // Collection-level bearer.
    auth::stored_auth_set(
        &store,
        Some(cid),
        None,
        &AuthSpec::Bearer {
            token: "col-token".into(),
        },
    )
    .unwrap();

    let effective = auth::effective_auth(&store, &AuthSpec::Inherit, Some(req), Some(cid)).unwrap();
    assert_eq!(
        effective,
        AuthSpec::Bearer {
            token: "col-token".into()
        }
    );

    // Folder auth overrides collection for its children.
    auth::stored_auth_set(
        &store,
        None,
        Some(folder),
        &AuthSpec::ApiKey {
            key: "X-Key".into(),
            value: "folder-key".into(),
            in_query: false,
        },
    )
    .unwrap();
    let effective = auth::effective_auth(&store, &AuthSpec::Inherit, Some(req), Some(cid)).unwrap();
    assert!(matches!(effective, AuthSpec::ApiKey { .. }));

    // Explicit auth on the request wins outright.
    let explicit = AuthSpec::Basic {
        username: "u".into(),
        password: "p".into(),
    };
    let effective = auth::effective_auth(&store, &explicit, Some(req), Some(cid)).unwrap();
    assert_eq!(effective, explicit);
}

#[tokio::test(flavor = "multi_thread")]
async fn oauth2_client_credentials_against_local_server() {
    let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
    let addr = format!("http://{}", server.server_addr());
    let handle = std::thread::spawn(move || {
        let mut request = server.recv().unwrap();
        let mut body = String::new();
        request.as_reader().read_to_string(&mut body).unwrap();
        assert!(body.contains("grant_type=client_credentials"));
        assert!(body.contains("scope=read"));
        // Basic auth header present (credentials not in body).
        let has_basic = request
            .headers()
            .iter()
            .any(|h| h.field.equiv("authorization") && h.value.as_str().starts_with("Basic "));
        assert!(has_basic);
        let response = tiny_http::Response::from_string(
            r#"{"access_token":"at-1","refresh_token":"rt-1","expires_in":3600,"token_type":"Bearer"}"#,
        )
        .with_header(
            tiny_http::Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap(),
        );
        request.respond(response).unwrap();
    });

    let cfg = OAuth2Config {
        grant_type: "client_credentials".into(),
        token_url: format!("{addr}/token"),
        client_id: "cid".into(),
        client_secret: "sec".into(),
        scope: "read".into(),
        ..Default::default()
    };
    let token = auth::oauth2::fetch_token(&cfg).await.unwrap();
    handle.join().unwrap();

    assert_eq!(token.access_token, "at-1");
    assert_eq!(token.refresh_token, "rt-1");
    assert!(token.expires_at > 0);

    // An expired config with a refresh token reports expired.
    let expired = OAuth2Config {
        access_token: "at-1".into(),
        refresh_token: "rt-1".into(),
        expires_at: 100, // long past
        ..cfg
    };
    assert!(expired.is_expired());
}
