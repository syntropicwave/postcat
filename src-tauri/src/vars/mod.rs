//! `{{variable}}` resolution at send time.
//!
//! The unresolved spec is what gets saved and replayed; resolution happens
//! only in the moment of sending. Secret values are collected so history can
//! mask them before anything hits disk or the search index.

use crate::collections::{effective_vars, Variable};
use crate::http_engine::{BodySpec, RequestSpec};
use crate::store::{Store, StoreError};

pub struct Resolution {
    pub spec: RequestSpec,
    /// (secret value, variable key) pairs that were substituted somewhere.
    pub secrets: Vec<(String, String)>,
}

pub fn resolve(
    store: &Store,
    spec: &RequestSpec,
    collection_id: Option<i64>,
) -> Result<Resolution, StoreError> {
    let vars = effective_vars(store, collection_id)?;
    Ok(resolve_with(spec, &vars))
}

pub fn resolve_with(spec: &RequestSpec, vars: &[Variable]) -> Resolution {
    let mut secrets: Vec<(String, String)> = Vec::new();
    {
        for v in vars.iter().filter(|v| v.is_secret) {
            let val = v.effective_value();
            if !val.is_empty() {
                secrets.push((val.to_owned(), v.key.clone()));
            }
        }
    }

    let sub = |s: &str| substitute(s, vars);

    let mut resolved = spec.clone();
    resolved.url = sub(&resolved.url);
    for h in &mut resolved.headers {
        h.key = sub(&h.key);
        h.value = sub(&h.value);
    }
    resolved.auth = spec.auth.substituted(&sub);
    resolved.body = match &spec.body {
        BodySpec::None => BodySpec::None,
        BodySpec::Raw { content_type, text } => BodySpec::Raw {
            content_type: sub(content_type),
            text: sub(text),
        },
        BodySpec::UrlEncoded { fields } => BodySpec::UrlEncoded {
            fields: fields
                .iter()
                .map(|f| {
                    let mut f = f.clone();
                    f.key = sub(&f.key);
                    f.value = sub(&f.value);
                    f
                })
                .collect(),
        },
        BodySpec::FormData { fields } => BodySpec::FormData {
            fields: fields
                .iter()
                .map(|f| {
                    let mut f = f.clone();
                    f.key = sub(&f.key);
                    if !f.is_file {
                        f.value = sub(&f.value);
                    }
                    f
                })
                .collect(),
        },
        BodySpec::Binary { path } => BodySpec::Binary { path: path.clone() },
        BodySpec::Graphql { query, variables } => BodySpec::Graphql {
            query: sub(query),
            variables: sub(variables),
        },
    };

    // Keep only secrets that actually appear in the resolved request.
    let hay = format!(
        "{}\n{}\n{}",
        resolved.url,
        resolved
            .headers
            .iter()
            .map(|h| format!("{}:{}", h.key, h.value))
            .collect::<Vec<_>>()
            .join("\n"),
        resolved.body_text().unwrap_or_default()
    );
    secrets.retain(|(val, _)| hay.contains(val.as_str()));

    Resolution {
        spec: resolved,
        secrets,
    }
}

/// Replace every `{{token}}` with its variable or dynamic value. Unknown
/// tokens are left as-is.
fn substitute(input: &str, vars: &[Variable]) -> String {
    let mut out = String::with_capacity(input.len());
    let mut rest = input;
    while let Some(start) = rest.find("{{") {
        let Some(end_rel) = rest[start + 2..].find("}}") else {
            break;
        };
        let token = rest[start + 2..start + 2 + end_rel].trim();
        out.push_str(&rest[..start]);
        match lookup(token, vars) {
            Some(value) => out.push_str(&value),
            None => out.push_str(&rest[start..start + 2 + end_rel + 2]),
        }
        rest = &rest[start + 2 + end_rel + 2..];
    }
    out.push_str(rest);
    out
}

fn lookup(token: &str, vars: &[Variable]) -> Option<String> {
    if let Some(dynamic) = token.strip_prefix('$') {
        return dynamic_value(dynamic);
    }
    vars.iter()
        .filter(|v| v.enabled)
        .find(|v| v.key == token)
        .map(|v| v.effective_value().to_owned())
}

/// Uniform pseudo-random index derived from a fresh UUID (no rand crate).
fn rand_u32() -> u32 {
    let bytes = *uuid::Uuid::new_v4().as_bytes();
    u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])
}

fn pick<'a>(list: &[&'a str]) -> &'a str {
    list[rand_u32() as usize % list.len()]
}

const FIRST_NAMES: &[&str] = &[
    "Anna", "Boris", "Clara", "David", "Elena", "Felix", "Greta", "Hugo", "Iris", "Jonas", "Kira",
    "Leo", "Marta", "Nikita", "Olga", "Pavel", "Rita", "Simon", "Tanya", "Viktor",
];
const LAST_NAMES: &[&str] = &[
    "Smirnov", "Miller", "Weber", "Novak", "Garcia", "Rossi", "Kim", "Tanaka", "Larsen", "Kovacs",
    "Petrov", "Schmidt", "Silva", "Moreau", "Janssen", "Olsen",
];
const CITIES: &[&str] = &[
    "Berlin", "Lisbon", "Tallinn", "Osaka", "Toronto", "Prague", "Oslo", "Valencia", "Tbilisi",
    "Vienna", "Riga", "Porto",
];
const COUNTRIES: &[&str] = &[
    "Germany", "Portugal", "Estonia", "Japan", "Canada", "Czechia", "Norway", "Spain", "Georgia",
    "Austria",
];
const WORDS: &[&str] = &[
    "amber", "breeze", "cedar", "delta", "ember", "falcon", "granite", "harbor", "indigo",
    "juniper", "krypton", "lagoon", "meadow", "nimbus", "onyx", "pixel", "quartz", "raven",
    "summit", "tundra",
];
const COMPANIES: &[&str] = &[
    "Northwind",
    "Acme Corp",
    "Globex",
    "Initech",
    "Umbrella Labs",
    "Stark Industries",
    "Wayne Enterprises",
    "Hooli",
    "Aperture",
    "Vandelay",
];

/// Postman-compatible dynamic variables (the commonly used subset).
fn dynamic_value(name: &str) -> Option<String> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    match name {
        "guid" | "randomUUID" => Some(uuid::Uuid::new_v4().to_string()),
        "timestamp" => Some(now.as_secs().to_string()),
        "isoTimestamp" => {
            // Seconds precision is enough here; avoid a chrono dependency.
            let secs = now.as_secs() as i64;
            Some(iso8601_from_unix(secs))
        }
        "randomInt" => Some((rand_u32() % 1001).to_string()),
        "randomBoolean" => Some(rand_u32().is_multiple_of(2).to_string()),
        "randomFirstName" => Some(pick(FIRST_NAMES).to_owned()),
        "randomLastName" => Some(pick(LAST_NAMES).to_owned()),
        "randomFullName" => Some(format!("{} {}", pick(FIRST_NAMES), pick(LAST_NAMES))),
        "randomUserName" => Some(format!(
            "{}{}",
            pick(FIRST_NAMES).to_lowercase(),
            rand_u32() % 1000
        )),
        "randomEmail" => Some(format!(
            "{}.{}{}@example.com",
            pick(FIRST_NAMES).to_lowercase(),
            pick(LAST_NAMES).to_lowercase(),
            rand_u32() % 100
        )),
        "randomCity" => Some(pick(CITIES).to_owned()),
        "randomCountry" => Some(pick(COUNTRIES).to_owned()),
        "randomStreetAddress" => Some(format!("{} {} St", rand_u32() % 900 + 1, pick(WORDS))),
        "randomPhoneNumber" => Some(format!(
            "+1-{:03}-{:03}-{:04}",
            rand_u32() % 800 + 200,
            rand_u32() % 1000,
            rand_u32() % 10000
        )),
        "randomWord" => Some(pick(WORDS).to_owned()),
        "randomWords" => Some(format!("{} {} {}", pick(WORDS), pick(WORDS), pick(WORDS))),
        "randomCompanyName" => Some(pick(COMPANIES).to_owned()),
        "randomUrl" => Some(format!("https://{}.example.com", pick(WORDS))),
        "randomIP" => Some(format!(
            "{}.{}.{}.{}",
            rand_u32() % 223 + 1,
            rand_u32() % 256,
            rand_u32() % 256,
            rand_u32() % 254 + 1
        )),
        "randomPort" => Some((rand_u32() % 64512 + 1024).to_string()),
        "randomColor" => Some(format!("#{:06x}", rand_u32() % 0x1000000)),
        "randomAlphaNumeric" => Some(uuid::Uuid::new_v4().simple().to_string()[..12].to_owned()),
        "randomPassword" => Some(format!(
            "{}{}{}!",
            pick(WORDS),
            rand_u32() % 100,
            pick(WORDS)
        )),
        _ => None,
    }
}

/// Mask secret values in a resolved spec before it is stored/displayed:
/// every occurrence of a secret value becomes `{{its_key}}`.
pub fn mask_secrets(spec: &RequestSpec, secrets: &[(String, String)]) -> RequestSpec {
    if secrets.is_empty() {
        return spec.clone();
    }
    let mask = |s: &str| mask_str(s, secrets);
    let mut masked = spec.clone();
    masked.url = mask(&masked.url);
    for h in &mut masked.headers {
        h.value = mask(&h.value);
    }
    masked.body = match &spec.body {
        BodySpec::Raw { content_type, text } => BodySpec::Raw {
            content_type: content_type.clone(),
            text: mask(text),
        },
        BodySpec::UrlEncoded { fields } => BodySpec::UrlEncoded {
            fields: fields
                .iter()
                .map(|f| {
                    let mut f = f.clone();
                    f.value = mask(&f.value);
                    f
                })
                .collect(),
        },
        BodySpec::FormData { fields } => BodySpec::FormData {
            fields: fields
                .iter()
                .map(|f| {
                    let mut f = f.clone();
                    if !f.is_file {
                        f.value = mask(&f.value);
                    }
                    f
                })
                .collect(),
        },
        BodySpec::Graphql { query, variables } => BodySpec::Graphql {
            query: mask(query),
            variables: mask(variables),
        },
        other => other.clone(),
    };
    masked
}

pub fn mask_str(input: &str, secrets: &[(String, String)]) -> String {
    let mut out = input.to_owned();
    for (value, key) in secrets {
        // Values shorter than 3 chars would mask half the text.
        if value.len() >= 3 {
            out = out.replace(value.as_str(), &format!("{{{{{key}}}}}"));
        }
    }
    out
}

/// Add or override a variable in a scope list (strongest-wins semantics).
pub fn upsert_var(vars: &mut Vec<Variable>, key: &str, value: &str) {
    if let Some(existing) = vars.iter_mut().find(|v| v.key == key) {
        existing.current_value = Some(value.to_owned());
        existing.enabled = true;
    } else {
        vars.push(Variable {
            key: key.to_owned(),
            initial_value: value.to_owned(),
            current_value: None,
            is_secret: false,
            enabled: true,
        });
    }
}

pub fn iso8601_from_unix(secs: i64) -> String {
    // Days-to-civil conversion (Howard Hinnant's algorithm), UTC.
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    let (h, m, s) = (rem / 3600, (rem % 3600) / 60, rem % 60);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let mo = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if mo <= 2 { y + 1 } else { y };
    format!("{y:04}-{mo:02}-{d:02}T{h:02}:{m:02}:{s:02}Z")
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use crate::http_engine::KeyValue;

    fn var(key: &str, value: &str, secret: bool) -> Variable {
        Variable {
            key: key.into(),
            initial_value: value.into(),
            current_value: None,
            is_secret: secret,
            enabled: true,
        }
    }

    #[test]
    fn substitutes_and_leaves_unknown() {
        let vars = vec![var("base", "https://api.dev", false)];
        assert_eq!(
            substitute("{{base}}/users?x={{missing}}", &vars),
            "https://api.dev/users?x={{missing}}"
        );
    }

    #[test]
    fn current_value_overrides_initial() {
        let mut v = var("k", "shared", false);
        v.current_value = Some("local".into());
        assert_eq!(substitute("{{k}}", &[v]), "local");
    }

    #[test]
    fn dynamic_vars_produce_values() {
        let guid = substitute("{{$guid}}", &[]);
        assert_eq!(guid.len(), 36);
        let ts: i64 = substitute("{{$timestamp}}", &[]).parse().unwrap();
        assert!(ts > 1_700_000_000);
        let iso = substitute("{{$isoTimestamp}}", &[]);
        assert!(iso.ends_with('Z') && iso.contains('T'));
    }

    #[test]
    fn secrets_are_collected_and_masked() {
        let vars = vec![
            var("token", "sk-very-secret-123", true),
            var("host", "api.dev", false),
        ];
        let spec = RequestSpec {
            method: "GET".into(),
            url: "https://{{host}}/data".into(),
            headers: vec![KeyValue {
                key: "Authorization".into(),
                value: "Bearer {{token}}".into(),
                enabled: true,
            }],
            body: BodySpec::None,
            settings: Default::default(),
            auth: Default::default(),
        };
        let res = resolve_with(&spec, &vars);
        assert_eq!(res.spec.headers[0].value, "Bearer sk-very-secret-123");
        assert_eq!(res.secrets.len(), 1);

        let masked = mask_secrets(&res.spec, &res.secrets);
        assert_eq!(masked.headers[0].value, "Bearer {{token}}");
        assert_eq!(masked.url, "https://api.dev/data"); // non-secret resolved stays
    }
}
