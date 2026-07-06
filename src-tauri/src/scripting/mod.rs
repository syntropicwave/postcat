//! JavaScript sandbox for pre-request and test scripts (QuickJS via rquickjs).
//!
//! Capability-scoped by construction: the engine has no filesystem, network
//! or process access — the only door is the injected `__pc_send` host
//! function (pm.sendRequest), which routes through our own HTTP engine.
//! Hard limits: 64 MB heap, 10 s wall clock per script.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::http_engine::RequestSpec;
use crate::store::{Store, StoreError};

const PRELUDE: &str = include_str!("prelude.js");
const MEMORY_LIMIT: usize = 64 * 1024 * 1024;
const TIME_LIMIT: Duration = Duration::from_secs(10);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestResult {
    pub name: String,
    pub passed: bool,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsoleLine {
    pub level: String,
    pub message: String,
}

#[derive(Debug, Serialize)]
pub struct ScriptInput {
    pub request: RequestSpec,
    pub response: Option<serde_json::Value>,
    pub vars: HashMap<String, String>,
    pub data: Option<serde_json::Value>,
    pub iteration: u32,
    pub iteration_count: u32,
    pub request_name: String,
}

#[derive(Debug, Default)]
pub struct ScriptOutput {
    pub tests: Vec<TestResult>,
    pub console: Vec<ConsoleLine>,
    /// Possibly mutated by the script (pre-request).
    pub request: Option<RequestSpec>,
    pub var_ops: Vec<VarOp>,
    /// None = not called; Some(None) = stop; Some(Some(name)) = jump.
    pub next_request: Option<Option<String>>,
    pub skip_requested: bool,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct VarOp {
    pub scope: String,
    pub key: String,
    pub value: Option<String>,
}

/// Callback the sandbox uses for pm.sendRequest — a blocking HTTP call.
pub type SendFn = Arc<dyn Fn(RequestSpec) -> serde_json::Value + Send + Sync>;

/// Execute one script. Runs on the current thread (callers use
/// spawn_blocking); each run gets a fresh runtime, so scripts cannot leak
/// state into each other.
pub fn execute(script: &str, input: &ScriptInput, send: SendFn) -> ScriptOutput {
    let mut output = ScriptOutput::default();

    let runtime = match rquickjs::Runtime::new() {
        Ok(rt) => rt,
        Err(e) => {
            output.error = Some(format!("failed to create JS runtime: {e}"));
            return output;
        }
    };
    runtime.set_memory_limit(MEMORY_LIMIT);
    let deadline = Instant::now() + TIME_LIMIT;
    runtime.set_interrupt_handler(Some(Box::new(move || Instant::now() > deadline)));

    let context = match rquickjs::Context::full(&runtime) {
        Ok(c) => c,
        Err(e) => {
            output.error = Some(format!("failed to create JS context: {e}"));
            return output;
        }
    };

    let input_json = match serde_json::to_string(input) {
        Ok(j) => j,
        Err(e) => {
            output.error = Some(format!("failed to serialize script input: {e}"));
            return output;
        }
    };

    let result_json: Result<String, String> = context.with(|ctx| {
        let globals = ctx.globals();

        let parsed = ctx
            .json_parse(input_json.as_bytes())
            .map_err(|e| format!("input parse: {e}"))?;
        globals
            .set("__pc_input", parsed)
            .map_err(|e| e.to_string())?;

        let send_fn = send.clone();
        globals
            .set(
                "__pc_send",
                rquickjs::Function::new(ctx.clone(), move |spec_json: String| -> String {
                    let spec: RequestSpec = match serde_json::from_str(&spec_json) {
                        Ok(s) => s,
                        Err(e) => {
                            return serde_json::json!({ "error": format!("bad request: {e}") })
                                .to_string()
                        }
                    };
                    send_fn(spec).to_string()
                }),
            )
            .map_err(|e| e.to_string())?;

        ctx.eval::<(), _>(PRELUDE)
            .map_err(|e| pretty_js_error(&ctx, e, "prelude"))?;
        ctx.eval::<(), _>(script.as_bytes())
            .map_err(|e| pretty_js_error(&ctx, e, "script"))?;
        ctx.eval::<String, _>("__pc.result()")
            .map_err(|e| pretty_js_error(&ctx, e, "result"))
    });

    match result_json {
        Err(e) => output.error = Some(e),
        Ok(json) => match serde_json::from_str::<serde_json::Value>(&json) {
            Err(e) => output.error = Some(format!("bad script result: {e}")),
            Ok(v) => {
                if let Some(tests) = v.get("tests") {
                    output.tests = serde_json::from_value::<Vec<serde_json::Value>>(tests.clone())
                        .unwrap_or_default()
                        .into_iter()
                        .map(|t| TestResult {
                            name: t["name"].as_str().unwrap_or("?").to_owned(),
                            passed: t["passed"].as_bool().unwrap_or(false),
                            error: t["error"].as_str().map(str::to_owned),
                        })
                        .collect();
                }
                if let Some(lines) = v.get("console").and_then(|c| c.as_array()) {
                    output.console = lines
                        .iter()
                        .map(|l| ConsoleLine {
                            level: l[0].as_str().unwrap_or("log").to_owned(),
                            message: l[1].as_str().unwrap_or("").to_owned(),
                        })
                        .collect();
                }
                if let Some(req) = v.get("request") {
                    output.request = serde_json::from_value(req.clone()).ok();
                }
                if let Some(ops) = v.get("varOps") {
                    output.var_ops = serde_json::from_value(ops.clone()).unwrap_or_default();
                }
                match v.get("nextRequest").and_then(|n| n.as_str()) {
                    Some("__stop__") => output.next_request = Some(None),
                    Some("__skip__") => output.skip_requested = true,
                    Some(name) => output.next_request = Some(Some(name.to_owned())),
                    None => {}
                }
            }
        },
    }
    output
}

fn pretty_js_error(ctx: &rquickjs::Ctx<'_>, err: rquickjs::Error, stage: &str) -> String {
    if let rquickjs::Error::Exception = err {
        let caught = ctx.catch();
        if let Some(ex) = caught.as_exception() {
            let msg = ex.message().unwrap_or_default();
            let line = ex
                .line()
                .map(|l| format!(" (line {l})"))
                .unwrap_or_default();
            return format!("{stage}: {msg}{line}");
        }
        return format!("{stage}: {caught:?}");
    }
    format!("{stage}: {err}")
}

/// Persist variable mutations from a script run and fold them into the live
/// vars map. `local` ops only touch the map (run-scoped).
pub fn apply_var_ops(
    store: &Store,
    collection_id: Option<i64>,
    ops: &[VarOp],
    vars: &mut HashMap<String, String>,
) -> Result<(), StoreError> {
    use crate::collections::{self, Variable};
    for op in ops {
        match op.value.as_deref() {
            Some(v) => vars.insert(op.key.clone(), v.to_owned()),
            None => vars.remove(&op.key),
        };
        let (scope, owner): (&str, Option<i64>) = match op.scope.as_str() {
            "global" => ("global", None),
            "collection" => match collection_id {
                Some(cid) => ("collection", Some(cid)),
                None => continue,
            },
            "environment" => {
                let active = collections::env_list(store)?
                    .into_iter()
                    .find(|e| e.is_active);
                match active {
                    Some(env) => ("environment", Some(env.id)),
                    None => continue,
                }
            }
            _ => continue, // "local" — run-scoped only
        };
        let mut existing = collections::vars_get(store, scope, owner)?;
        match op.value.as_deref() {
            Some(value) => {
                if let Some(var) = existing.iter_mut().find(|v| v.key == op.key) {
                    var.current_value = Some(value.to_owned());
                } else {
                    existing.push(Variable {
                        key: op.key.clone(),
                        initial_value: value.to_owned(),
                        current_value: None,
                        is_secret: false,
                        enabled: true,
                    });
                }
            }
            None => existing.retain(|v| v.key != op.key),
        }
        collections::vars_save(store, scope, owner, &existing)?;
    }
    Ok(())
}

/// Collect the pre/test script chain from collection down through folders.
/// The request's own scripts are supplied by the caller (they may be
/// unsaved tab state).
pub fn chain_scripts(
    store: &Store,
    collection_id: Option<i64>,
    item_id: Option<i64>,
) -> Result<(Vec<String>, Vec<String>), StoreError> {
    let mut pre = Vec::new();
    let mut test = Vec::new();

    if let Some(cid) = collection_id {
        let row: Option<(Option<String>, Option<String>)> = store.with_conn(|conn| {
            conn.query_row(
                "SELECT pre_request_script, test_script FROM collections WHERE id = ?1",
                rusqlite::params![cid],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .map(Some)
        })?;
        if let Some((p, t)) = row {
            if let Some(p) = p.filter(|s| !s.trim().is_empty()) {
                pre.push(p);
            }
            if let Some(t) = t.filter(|s| !s.trim().is_empty()) {
                test.push(t);
            }
        }
    }

    // Ancestor folders, outermost first (walk up then reverse).
    let mut folder_chain: Vec<(Option<String>, Option<String>)> = Vec::new();
    let mut current = item_id;
    let mut first = true;
    while let Some(id) = current {
        let (p, t, parent): (Option<String>, Option<String>, Option<i64>) =
            store.with_conn(|conn| {
                conn.query_row(
                    "SELECT pre_request_script, test_script, parent_id
                     FROM collection_items WHERE id = ?1",
                    rusqlite::params![id],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                )
            })?;
        if !first {
            folder_chain.push((p, t));
        }
        first = false;
        current = parent;
    }
    for (p, t) in folder_chain.into_iter().rev() {
        if let Some(p) = p.filter(|s| !s.trim().is_empty()) {
            pre.push(p);
        }
        if let Some(t) = t.filter(|s| !s.trim().is_empty()) {
            test.push(t);
        }
    }
    Ok((pre, test))
}

/// A no-network SendFn for contexts where pm.sendRequest is not allowed.
pub fn no_send() -> SendFn {
    Arc::new(|_spec| serde_json::json!({ "error": "pm.sendRequest is not available here" }))
}

/// Blocking SendFn built on the real HTTP engine (call from spawn_blocking).
pub fn blocking_send(app_settings: crate::settings::AppSettings) -> SendFn {
    Arc::new(move |spec: RequestSpec| {
        let client = match reqwest::blocking::Client::builder()
            .timeout(Duration::from_millis(spec.settings.timeout_ms))
            .danger_accept_invalid_certs(!spec.settings.verify_ssl)
            .build()
        {
            Ok(c) => c,
            Err(e) => return serde_json::json!({ "error": e.to_string() }),
        };
        let _ = &app_settings; // proxy/CA support for script sends: later
        let method = match reqwest::Method::from_bytes(spec.method.as_bytes()) {
            Ok(m) => m,
            Err(_) => return serde_json::json!({ "error": "invalid method" }),
        };
        let mut req = client.request(method, &spec.url);
        for h in spec
            .headers
            .iter()
            .filter(|h| h.enabled && !h.key.is_empty())
        {
            req = req.header(&h.key, &h.value);
        }
        if let crate::http_engine::BodySpec::Raw { content_type, text } = &spec.body {
            if !content_type.is_empty() {
                req = req.header("Content-Type", content_type);
            }
            req = req.body(text.clone());
        }
        let started = Instant::now();
        match req.send() {
            Err(e) => serde_json::json!({ "error": e.to_string() }),
            Ok(resp) => {
                let status = resp.status();
                let headers: Vec<(String, String)> = resp
                    .headers()
                    .iter()
                    .map(|(k, v)| {
                        (
                            k.as_str().to_owned(),
                            String::from_utf8_lossy(v.as_bytes()).into_owned(),
                        )
                    })
                    .collect();
                let body = resp.text().unwrap_or_default();
                serde_json::json!({
                    "status": status.as_u16(),
                    "status_text": status.canonical_reason().unwrap_or(""),
                    "headers": headers,
                    "body_text": body,
                    "duration_ms": started.elapsed().as_secs_f64() * 1000.0,
                })
            }
        }
    })
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    fn input(vars: &[(&str, &str)]) -> ScriptInput {
        ScriptInput {
            request: RequestSpec {
                url: "https://api.dev/items?x={{x}}".into(),
                ..Default::default()
            },
            response: None,
            vars: vars
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
            data: None,
            iteration: 0,
            iteration_count: 1,
            request_name: "test".into(),
        }
    }

    fn with_response(body: &str, status: u16) -> ScriptInput {
        let mut i = input(&[]);
        i.response = Some(serde_json::json!({
            "status": status,
            "status_text": "OK",
            "headers": [["content-type", "application/json"]],
            "body_text": body,
            "duration_ms": 42.0,
            "size": body.len(),
        }));
        i
    }

    #[test]
    fn tests_pass_and_fail_with_expect() {
        let out = execute(
            r#"
            pm.test("equality works", function () {
                pm.expect(2 + 2).to.equal(4);
                pm.expect({a: [1, 2]}).to.eql({a: [1, 2]});
                pm.expect("hello world").to.include("world");
                pm.expect([1, 2, 3]).to.have.lengthOf(3);
                pm.expect(5).to.be.above(4).and.below(6);
                pm.expect("abc").to.be.a("string");
                pm.expect(null).to.be.null;
                pm.expect(1).to.not.equal(2);
            });
            pm.test("this one fails", function () {
                pm.expect(1).to.equal(2);
            });
            "#,
            &with_response("{}", 200),
            no_send(),
        );
        assert!(out.error.is_none(), "{:?}", out.error);
        assert_eq!(out.tests.len(), 2);
        assert!(out.tests[0].passed);
        assert!(!out.tests[1].passed);
        assert!(out.tests[1]
            .error
            .as_deref()
            .unwrap()
            .contains("expected 1"));
    }

    #[test]
    fn response_assertions_and_json() {
        let out = execute(
            r#"
            pm.test("status", function () { pm.response.to.have.status(200); });
            pm.test("body", function () {
                const data = pm.response.json();
                pm.expect(data.user.name).to.equal("Ann");
                pm.expect(pm.response.headers.get("content-type")).to.include("json");
                pm.expect(pm.response.responseTime).to.be.below(1000);
            });
            "#,
            &with_response(r#"{"user":{"name":"Ann"}}"#, 200),
            no_send(),
        );
        assert!(out.error.is_none(), "{:?}", out.error);
        assert!(out.tests.iter().all(|t| t.passed), "{:?}", out.tests);
    }

    #[test]
    fn pre_request_mutations_and_vars() {
        let out = execute(
            r#"
            pm.request.headers.upsert({key: "X-Signed", value: "sig-" + pm.variables.get("x")});
            pm.request.method = "post";
            pm.environment.set("token", "tok-123");
            pm.variables.set("run_only", "1");
            console.log("prepared", {x: pm.variables.get("x")});
            "#,
            &input(&[("x", "42")]),
            no_send(),
        );
        assert!(out.error.is_none(), "{:?}", out.error);
        let req = out.request.unwrap();
        assert_eq!(req.method, "POST");
        assert!(req
            .headers
            .iter()
            .any(|h| h.key == "X-Signed" && h.value == "sig-42"));
        assert_eq!(out.var_ops.len(), 2);
        assert_eq!(out.var_ops[0].scope, "environment");
        assert_eq!(out.var_ops[1].scope, "local");
        assert_eq!(out.console.len(), 1);
        assert!(out.console[0].message.contains("prepared"));
    }

    #[test]
    fn set_next_request_and_replace_in() {
        let out = execute(
            r#"
            const filled = pm.variables.replaceIn("go to {{x}} now");
            pm.test("replaceIn", () => pm.expect(filled).to.equal("go to 42 now"));
            pm.execution.setNextRequest("Login");
            "#,
            &input(&[("x", "42")]),
            no_send(),
        );
        assert!(out.error.is_none(), "{:?}", out.error);
        assert_eq!(out.next_request, Some(Some("Login".into())));

        let out = execute("pm.execution.setNextRequest(null);", &input(&[]), no_send());
        assert_eq!(out.next_request, Some(None));
    }

    #[test]
    fn script_errors_are_reported_not_fatal() {
        let out = execute("throw new Error('boom');", &input(&[]), no_send());
        assert!(out.error.as_deref().unwrap().contains("boom"));

        let out = execute("syntax error here", &input(&[]), no_send());
        assert!(out.error.is_some());
    }

    #[test]
    fn infinite_loop_is_interrupted() {
        // NOTE: relies on the 10s interrupt deadline.
        let started = Instant::now();
        let out = execute("while (true) {}", &input(&[]), no_send());
        assert!(out.error.is_some());
        assert!(started.elapsed() < Duration::from_secs(20));
    }

    #[test]
    fn send_request_bridge() {
        let send: SendFn = Arc::new(|spec: RequestSpec| {
            assert_eq!(spec.url, "https://internal.dev/lookup");
            serde_json::json!({
                "status": 200, "status_text": "OK", "headers": [],
                "body_text": "{\"id\": 7}", "duration_ms": 1.0,
            })
        });
        let out = execute(
            r#"
            pm.sendRequest("https://internal.dev/lookup", function (err, res) {
                pm.test("ancillary request", function () {
                    pm.expect(err).to.be.null;
                    pm.expect(res.code).to.equal(200);
                    pm.expect(res.json().id).to.equal(7);
                });
            });
            "#,
            &input(&[]),
            send,
        );
        assert!(out.error.is_none(), "{:?}", out.error);
        assert_eq!(out.tests.len(), 1);
        assert!(out.tests[0].passed, "{:?}", out.tests);
    }
}
