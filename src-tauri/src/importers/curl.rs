//! cURL command line → RequestSpec.

use crate::http_engine::{BodySpec, FormField, KeyValue, RequestSpec};

use super::spec_default;

pub fn parse_curl(input: &str) -> Result<RequestSpec, String> {
    let tokens = tokenize(input)?;
    if tokens.is_empty() || !tokens[0].starts_with("curl") {
        return Err("not a curl command".into());
    }

    let mut spec = spec_default();
    let mut method_explicit = false;
    let mut data_parts: Vec<String> = Vec::new();
    let mut form_fields: Vec<FormField> = Vec::new();
    let mut urlencode_parts: Vec<String> = Vec::new();

    let mut i = 1;
    let next = |i: &mut usize| -> Option<String> {
        *i += 1;
        tokens.get(*i).cloned()
    };

    while i < tokens.len() {
        let tok = &tokens[i];
        match tok.as_str() {
            "-X" | "--request" => {
                if let Some(m) = next(&mut i) {
                    spec.method = m.to_uppercase();
                    method_explicit = true;
                }
            }
            "-H" | "--header" => {
                if let Some(h) = next(&mut i) {
                    if let Some((k, v)) = h.split_once(':') {
                        spec.headers.push(KeyValue {
                            key: k.trim().into(),
                            value: v.trim().into(),
                            enabled: true,
                        });
                    }
                }
            }
            "-d" | "--data" | "--data-raw" | "--data-binary" | "--data-ascii" => {
                if let Some(d) = next(&mut i) {
                    data_parts.push(d);
                }
            }
            "--data-urlencode" => {
                if let Some(d) = next(&mut i) {
                    urlencode_parts.push(d);
                }
            }
            "-F" | "--form" => {
                if let Some(f) = next(&mut i) {
                    if let Some((k, v)) = f.split_once('=') {
                        let is_file = v.starts_with('@');
                        form_fields.push(FormField {
                            key: k.into(),
                            value: v.trim_start_matches('@').into(),
                            is_file,
                            enabled: true,
                        });
                    }
                }
            }
            "-u" | "--user" => {
                if let Some(cred) = next(&mut i) {
                    use base64::Engine;
                    let b64 = base64::engine::general_purpose::STANDARD.encode(&cred);
                    spec.headers.push(KeyValue {
                        key: "Authorization".into(),
                        value: format!("Basic {b64}"),
                        enabled: true,
                    });
                }
            }
            "-A" | "--user-agent" => {
                if let Some(ua) = next(&mut i) {
                    spec.headers.push(KeyValue {
                        key: "User-Agent".into(),
                        value: ua,
                        enabled: true,
                    });
                }
            }
            "-e" | "--referer" => {
                if let Some(r) = next(&mut i) {
                    spec.headers.push(KeyValue {
                        key: "Referer".into(),
                        value: r,
                        enabled: true,
                    });
                }
            }
            "-b" | "--cookie" => {
                if let Some(c) = next(&mut i) {
                    spec.headers.push(KeyValue {
                        key: "Cookie".into(),
                        value: c,
                        enabled: true,
                    });
                }
            }
            "-k" | "--insecure" => spec.settings.verify_ssl = false,
            "-L" | "--location" => spec.settings.follow_redirects = true,
            "--url" => {
                if let Some(u) = next(&mut i) {
                    spec.url = u;
                }
            }
            "--compressed" | "-s" | "--silent" | "-v" | "--verbose" | "-i" | "--include" => {}
            "-o" | "--output" | "--max-time" | "--connect-timeout" | "-m" => {
                let _ = next(&mut i); // takes an argument we ignore
            }
            t if !t.starts_with('-') && spec.url.is_empty() => {
                spec.url = t.to_string();
            }
            _ => {}
        }
        i += 1;
    }

    if !form_fields.is_empty() {
        spec.body = BodySpec::FormData {
            fields: form_fields,
        };
    } else if !urlencode_parts.is_empty() {
        spec.body = BodySpec::UrlEncoded {
            fields: urlencode_parts
                .iter()
                .map(|p| {
                    let (k, v) = p.split_once('=').unwrap_or((p.as_str(), ""));
                    KeyValue {
                        key: k.into(),
                        value: v.into(),
                        enabled: true,
                    }
                })
                .collect(),
        };
    } else if !data_parts.is_empty() {
        let text = data_parts.join("&");
        let content_type = spec
            .headers
            .iter()
            .find(|h| h.key.eq_ignore_ascii_case("content-type"))
            .map(|h| h.value.clone())
            .unwrap_or_else(|| {
                if text.trim_start().starts_with('{') || text.trim_start().starts_with('[') {
                    "application/json".into()
                } else {
                    "application/x-www-form-urlencoded".into()
                }
            });
        spec.body = BodySpec::Raw { content_type, text };
    }

    if !method_explicit && !matches!(spec.body, BodySpec::None) {
        spec.method = "POST".into();
    }
    if spec.url.is_empty() {
        return Err("no URL found in curl command".into());
    }
    Ok(spec)
}

/// Shell-ish tokenizer: handles single/double quotes, backslash escapes and
/// line continuations (\ or ^ at end of line).
fn tokenize(input: &str) -> Result<Vec<String>, String> {
    let cleaned = input
        .replace("\\\r\n", " ")
        .replace("\\\n", " ")
        .replace("^\r\n", " ")
        .replace("^\n", " ");
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut chars = cleaned.chars().peekable();
    let mut in_single = false;
    let mut in_double = false;

    while let Some(c) = chars.next() {
        match c {
            '\'' if !in_double => in_single = !in_single,
            '"' if !in_single => in_double = !in_double,
            '\\' if !in_single => {
                if let Some(&next) = chars.peek() {
                    current.push(next);
                    chars.next();
                }
            }
            c if c.is_whitespace() && !in_single && !in_double => {
                if !current.is_empty() {
                    tokens.push(std::mem::take(&mut current));
                }
            }
            c => current.push(c),
        }
    }
    if in_single || in_double {
        return Err("unbalanced quotes".into());
    }
    if !current.is_empty() {
        tokens.push(current);
    }
    Ok(tokens)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    #[test]
    fn parses_typical_curl() {
        let spec = parse_curl(
            r#"curl -X POST 'https://api.dev/users?a=1' \
                -H 'Content-Type: application/json' \
                -H 'Authorization: Bearer tok' \
                --data-raw '{"name":"Ann"}' -k"#,
        )
        .unwrap();
        assert_eq!(spec.method, "POST");
        assert_eq!(spec.url, "https://api.dev/users?a=1");
        assert_eq!(spec.headers.len(), 2);
        assert!(!spec.settings.verify_ssl);
        match &spec.body {
            BodySpec::Raw { content_type, text } => {
                assert_eq!(content_type, "application/json");
                assert_eq!(text, r#"{"name":"Ann"}"#);
            }
            other => panic!("unexpected body {other:?}"),
        }
    }

    #[test]
    fn implicit_post_with_data() {
        let spec = parse_curl("curl https://a.dev -d 'x=1'").unwrap();
        assert_eq!(spec.method, "POST");
    }

    #[test]
    fn form_files() {
        let spec = parse_curl("curl https://a.dev -F 'doc=@C:/tmp/a.pdf' -F 'note=hi'").unwrap();
        match &spec.body {
            BodySpec::FormData { fields } => {
                assert!(fields[0].is_file);
                assert_eq!(fields[0].value, "C:/tmp/a.pdf");
                assert!(!fields[1].is_file);
            }
            other => panic!("unexpected body {other:?}"),
        }
    }
}
