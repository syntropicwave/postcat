//! AWS Signature Version 4 (header-based) — self-contained implementation.

use hmac::{Hmac, Mac};
use sha2::{Digest, Sha256};

use crate::http_engine::{KeyValue, RequestSpec};

type HmacSha256 = Hmac<Sha256>;

pub struct Credentials<'a> {
    pub access_key: &'a str,
    pub secret_key: &'a str,
    pub session_token: Option<&'a str>,
    pub region: &'a str,
    pub service: &'a str,
}

/// Sign the request in place: adds x-amz-date, x-amz-content-sha256,
/// optional x-amz-security-token, and Authorization headers.
/// `now_override` ("YYYYMMDDTHHMMSSZ") exists for deterministic tests.
pub fn sign(
    spec: &mut RequestSpec,
    creds: &Credentials<'_>,
    now_override: Option<&str>,
) -> Result<(), String> {
    let url = url::Url::parse(&spec.url).map_err(|e| e.to_string())?;
    let host = url.host_str().ok_or("url has no host")?.to_owned();
    let host_hdr = match url.port() {
        Some(p) => format!("{host}:{p}"),
        None => host,
    };

    let amz_date = match now_override {
        Some(s) => s.to_owned(),
        None => amz_now(),
    };
    let date = &amz_date[..8];

    let payload = spec.body_text().unwrap_or_default();
    let payload_hash = hex::encode(Sha256::digest(payload.as_bytes()));

    // Headers included in the signature: host + x-amz-*.
    let mut signed: Vec<(String, String)> = vec![
        ("host".into(), host_hdr.clone()),
        ("x-amz-content-sha256".into(), payload_hash.clone()),
        ("x-amz-date".into(), amz_date.clone()),
    ];
    if let Some(token) = creds.session_token {
        signed.push(("x-amz-security-token".into(), token.to_owned()));
    }
    signed.sort_by(|a, b| a.0.cmp(&b.0));

    let canonical_headers: String = signed
        .iter()
        .map(|(k, v)| format!("{k}:{}\n", v.trim()))
        .collect();
    let signed_names = signed
        .iter()
        .map(|(k, _)| k.as_str())
        .collect::<Vec<_>>()
        .join(";");

    let canonical_query = canonical_query_string(&url);
    let canonical_uri = if url.path().is_empty() {
        "/".to_owned()
    } else {
        uri_encode_path(url.path())
    };

    let canonical_request = format!(
        "{}\n{}\n{}\n{}\n{}\n{}",
        spec.method.to_uppercase(),
        canonical_uri,
        canonical_query,
        canonical_headers,
        signed_names,
        payload_hash
    );

    let scope = format!("{date}/{}/{}/aws4_request", creds.region, creds.service);
    let string_to_sign = format!(
        "AWS4-HMAC-SHA256\n{}\n{}\n{}",
        amz_date,
        scope,
        hex::encode(Sha256::digest(canonical_request.as_bytes()))
    );

    let k_date = hmac(format!("AWS4{}", creds.secret_key).as_bytes(), date);
    let k_region = hmac(&k_date, creds.region);
    let k_service = hmac(&k_region, creds.service);
    let k_signing = hmac(&k_service, "aws4_request");
    let signature = hex::encode(hmac(&k_signing, &string_to_sign));

    let authorization = format!(
        "AWS4-HMAC-SHA256 Credential={}/{}, SignedHeaders={}, Signature={}",
        creds.access_key, scope, signed_names, signature
    );

    // Push headers (skip host — reqwest sets it from the URL).
    for (k, v) in signed.iter().filter(|(k, _)| k != "host") {
        upsert_header(spec, k, v);
    }
    upsert_header(spec, "Authorization", &authorization);
    Ok(())
}

fn upsert_header(spec: &mut RequestSpec, key: &str, value: &str) {
    if let Some(h) = spec
        .headers
        .iter_mut()
        .find(|h| h.key.eq_ignore_ascii_case(key))
    {
        h.value = value.to_owned();
        h.enabled = true;
    } else {
        spec.headers.push(KeyValue {
            key: key.to_owned(),
            value: value.to_owned(),
            enabled: true,
        });
    }
}

fn hmac(key: &[u8], data: &str) -> Vec<u8> {
    let Ok(mut mac) = HmacSha256::new_from_slice(key) else {
        unreachable!("HMAC-SHA256 accepts keys of any length")
    };
    mac.update(data.as_bytes());
    mac.finalize().into_bytes().to_vec()
}

fn canonical_query_string(url: &url::Url) -> String {
    let mut pairs: Vec<(String, String)> = url
        .query_pairs()
        .map(|(k, v)| (aws_uri_encode(&k), aws_uri_encode(&v)))
        .collect();
    pairs.sort();
    pairs
        .iter()
        .map(|(k, v)| format!("{k}={v}"))
        .collect::<Vec<_>>()
        .join("&")
}

fn uri_encode_path(path: &str) -> String {
    path.split('/')
        .map(aws_uri_encode)
        .collect::<Vec<_>>()
        .join("/")
}

/// AWS-style percent encoding: unreserved chars pass, everything else %XX.
fn aws_uri_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

fn amz_now() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;
    // Reuse the civil-date conversion from vars.
    let iso = crate::vars::iso8601_from_unix(secs);
    // 2026-07-06T14:00:00Z -> 20260706T140000Z
    iso.replace(['-', ':'], "")
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use crate::http_engine::BodySpec;

    #[test]
    fn produces_stable_signature() {
        let mut spec = RequestSpec {
            method: "GET".into(),
            url: "https://iam.amazonaws.com/?Action=ListUsers&Version=2010-05-08".into(),
            headers: vec![],
            body: BodySpec::None,
            settings: Default::default(),
            auth: Default::default(),
        };
        let creds = Credentials {
            access_key: "AKIDEXAMPLE",
            secret_key: "wJalrXUtnFEMI/K7MDENG+bPxRfiCYEXAMPLEKEY",
            session_token: None,
            region: "us-east-1",
            service: "iam",
        };
        sign(&mut spec, &creds, Some("20150830T123600Z")).unwrap();

        let auth = spec
            .headers
            .iter()
            .find(|h| h.key == "Authorization")
            .unwrap();
        assert!(auth.value.starts_with(
            "AWS4-HMAC-SHA256 Credential=AKIDEXAMPLE/20150830/us-east-1/iam/aws4_request"
        ));
        assert!(auth
            .value
            .contains("SignedHeaders=host;x-amz-content-sha256;x-amz-date"));
        // Deterministic: same inputs, same signature.
        let sig1 = auth.value.clone();
        let mut spec2 = RequestSpec {
            method: "GET".into(),
            url: "https://iam.amazonaws.com/?Action=ListUsers&Version=2010-05-08".into(),
            headers: vec![],
            body: BodySpec::None,
            settings: Default::default(),
            auth: Default::default(),
        };
        sign(&mut spec2, &creds, Some("20150830T123600Z")).unwrap();
        let sig2 = spec2
            .headers
            .iter()
            .find(|h| h.key == "Authorization")
            .unwrap()
            .value
            .clone();
        assert_eq!(sig1, sig2);
        assert!(spec.headers.iter().any(|h| h.key == "x-amz-date"));
    }
}
