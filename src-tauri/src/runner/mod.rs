//! Collection Runner: sequential execution of a collection (or folder) with
//! iterations, data rows, script chains and setNextRequest flow control.
//! UI-agnostic — progress goes through a callback, cancellation through an
//! atomic flag — so the tauri command and the CLI share this code.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use reqwest_cookie_store::CookieStoreMutex;
use serde::{Deserialize, Serialize};

use crate::collections::{self, Variable};
use crate::http_engine::RequestSpec;
use crate::scripting::{self, ConsoleLine, ScriptInput, TestResult};
use crate::store::Store;
use crate::{auth, history, settings, vars};

#[derive(Debug, Clone, Deserialize)]
pub struct RunOptions {
    pub collection_id: i64,
    #[serde(default)]
    pub folder_id: Option<i64>,
    #[serde(default = "one")]
    pub iterations: u32,
    #[serde(default)]
    pub delay_ms: u64,
    /// Data rows (from a CSV/JSON file); row i is used for iteration i.
    #[serde(default)]
    pub data: Option<Vec<serde_json::Value>>,
}

fn one() -> u32 {
    1
}

#[derive(Debug, Clone, Serialize)]
pub struct RequestRunResult {
    pub iteration: u32,
    pub item_id: i64,
    pub name: String,
    pub url: String,
    pub method: String,
    pub status: Option<u16>,
    pub error: Option<String>,
    pub duration_ms: f64,
    pub tests: Vec<TestResult>,
    pub console: Vec<ConsoleLine>,
    pub skipped: bool,
}

#[derive(Debug, Default, Serialize)]
pub struct RunReport {
    pub total_requests: u32,
    pub passed_tests: u32,
    pub failed_tests: u32,
    pub errors: u32,
    pub cancelled: bool,
    pub duration_ms: f64,
    pub results: Vec<RequestRunResult>,
}

struct RunnableRequest {
    item_id: i64,
    name: String,
    spec: RequestSpec,
}

enum NextJump {
    Continue,
    Stop,
    Jump(String),
}

pub async fn run(
    store: &Store,
    jar: Arc<CookieStoreMutex>,
    options: RunOptions,
    cancel: Arc<AtomicBool>,
    progress: impl Fn(&RequestRunResult),
) -> RunReport {
    let started = std::time::Instant::now();
    let mut report = RunReport::default();

    let requests = match ordered_requests(store, options.collection_id, options.folder_id) {
        Ok(r) => r,
        Err(e) => {
            report.errors = 1;
            report.results.push(error_result(0, &e.to_string()));
            return report;
        }
    };
    if requests.is_empty() {
        return report;
    }

    let app_settings = settings::get(store).unwrap_or_default();
    // Run-scoped variables (pm.variables.set) survive across requests.
    let mut run_vars: HashMap<String, String> = HashMap::new();

    'iterations: for iteration in 0..options.iterations {
        let data_row = options
            .data
            .as_ref()
            .filter(|rows| !rows.is_empty())
            .and_then(|rows| rows.get(iteration as usize % rows.len()))
            .cloned();

        let mut idx = 0usize;
        while idx < requests.len() {
            if cancel.load(Ordering::Relaxed) {
                report.cancelled = true;
                break 'iterations;
            }
            let request = &requests[idx];
            let (result, next_jump) = run_one(
                store,
                jar.clone(),
                &app_settings,
                &options,
                request,
                iteration,
                data_row.as_ref(),
                &mut run_vars,
            )
            .await;

            report.total_requests += 1;
            report.passed_tests += result.tests.iter().filter(|t| t.passed).count() as u32;
            report.failed_tests += result.tests.iter().filter(|t| !t.passed).count() as u32;
            if result.error.is_some() {
                report.errors += 1;
            }
            progress(&result);
            report.results.push(result);

            match next_jump {
                NextJump::Continue => idx += 1,
                NextJump::Stop => break,
                NextJump::Jump(name) => match requests.iter().position(|r| r.name == name) {
                    Some(target) => idx = target,
                    None => idx += 1, // unknown name — keep going
                },
            }

            if options.delay_ms > 0 && idx < requests.len() {
                tokio::time::sleep(std::time::Duration::from_millis(options.delay_ms)).await;
            }
        }
    }

    report.duration_ms = started.elapsed().as_secs_f64() * 1000.0;
    report
}

#[allow(clippy::too_many_arguments)]
async fn run_one(
    store: &Store,
    jar: Arc<CookieStoreMutex>,
    app_settings: &settings::AppSettings,
    options: &RunOptions,
    request: &RunnableRequest,
    iteration: u32,
    data_row: Option<&serde_json::Value>,
    run_vars: &mut HashMap<String, String>,
) -> (RequestRunResult, NextJump) {
    let mut tests: Vec<TestResult> = Vec::new();
    let mut console: Vec<ConsoleLine> = Vec::new();
    let mut script_error: Option<String> = None;
    let mut next_jump = NextJump::Continue;
    let mut skipped = false;

    // Effective variables: scopes + data row + run-local (strongest last).
    let mut var_list =
        collections::effective_vars(store, Some(options.collection_id)).unwrap_or_default();
    overlay_data(&mut var_list, data_row);
    for (k, v) in run_vars.iter() {
        upsert_var(&mut var_list, k, v);
    }
    let mut vars_map: HashMap<String, String> = var_list
        .iter()
        .map(|v| (v.key.clone(), v.effective_value().to_owned()))
        .collect();

    // Script chains: collection -> folders; then the request's own scripts.
    let (mut pre_chain, mut test_chain) =
        scripting::chain_scripts(store, Some(options.collection_id), Some(request.item_id))
            .unwrap_or_default();
    let own = item_scripts(store, request.item_id);
    if let Some(p) = own.0 {
        pre_chain.push(p);
    }
    if let Some(t) = own.1 {
        test_chain.push(t);
    }

    // Pre-request scripts run against the unresolved spec.
    let mut spec = request.spec.clone();
    let send_fn = scripting::blocking_send(app_settings.clone());
    for script in &pre_chain {
        let input = ScriptInput {
            request: spec.clone(),
            response: None,
            vars: vars_map.clone(),
            data: data_row.cloned(),
            iteration,
            iteration_count: options.iterations,
            request_name: request.name.clone(),
        };
        let script = script.clone();
        let send = send_fn.clone();
        let out = tokio::task::spawn_blocking(move || scripting::execute(&script, &input, send))
            .await
            .unwrap_or_default();
        console.extend(out.console);
        tests.extend(out.tests);
        if let Some(req) = out.request {
            spec = req;
        }
        fold_local_ops(&out.var_ops, run_vars);
        let _ = scripting::apply_var_ops(
            store,
            Some(options.collection_id),
            &out.var_ops,
            &mut vars_map,
        );
        if out.skip_requested {
            skipped = true;
        }
        if let Some(next) = out.next_request {
            next_jump = match next {
                Some(name) => NextJump::Jump(name),
                None => NextJump::Stop,
            };
        }
        if let Some(e) = out.error {
            script_error = Some(format!("pre-request: {e}"));
            break;
        }
    }

    if skipped {
        let result = RequestRunResult {
            iteration,
            item_id: request.item_id,
            name: request.name.clone(),
            url: spec.url.clone(),
            method: spec.method.clone(),
            status: None,
            error: script_error,
            duration_ms: 0.0,
            tests,
            console,
            skipped: true,
        };
        return (result, next_jump);
    }

    // Resolve with the (possibly script-updated) vars.
    let mut updated_vars = var_list;
    for (k, v) in &vars_map {
        upsert_var(&mut updated_vars, k, v);
    }
    let resolution = vars::resolve_with(&spec, &updated_vars);
    let mut secrets = resolution.secrets;
    let mut resolved = resolution.spec;

    let effective = auth::effective_auth(
        store,
        &resolved.auth,
        Some(request.item_id),
        Some(options.collection_id),
    )
    .unwrap_or_default();
    secrets.extend(auth::apply(&mut resolved, &effective));
    let display = vars::mask_secrets(&resolved, &secrets);

    let outcome = crate::http_engine::execute(jar, &resolved, app_settings).await;

    let (status, mut error, duration_ms, response_json) = match &outcome {
        Ok(resp) => {
            let (body_text, _) = history::body_for_ui(&resp.body);
            (
                Some(resp.status),
                None,
                resp.duration_ms,
                Some(serde_json::json!({
                    "status": resp.status,
                    "status_text": resp.status_text,
                    "headers": resp.headers,
                    "body_text": body_text,
                    "duration_ms": resp.duration_ms,
                    "size": resp.size,
                })),
            )
        }
        Err(e) => (None, Some(e.to_string()), 0.0, None),
    };

    // Record into history like a normal send.
    let _ = match &outcome {
        Ok(resp) => history::record(store, &request.spec, &display, &secrets, Ok(resp)),
        Err(e) => history::record(
            store,
            &request.spec,
            &display,
            &secrets,
            Err(&e.to_string()),
        ),
    };

    // Test scripts.
    if response_json.is_some() && script_error.is_none() {
        for script in &test_chain {
            let input = ScriptInput {
                request: spec.clone(),
                response: response_json.clone(),
                vars: vars_map.clone(),
                data: data_row.cloned(),
                iteration,
                iteration_count: options.iterations,
                request_name: request.name.clone(),
            };
            let script = script.clone();
            let send = send_fn.clone();
            let out =
                tokio::task::spawn_blocking(move || scripting::execute(&script, &input, send))
                    .await
                    .unwrap_or_default();
            console.extend(out.console);
            tests.extend(out.tests);
            fold_local_ops(&out.var_ops, run_vars);
            let _ = scripting::apply_var_ops(
                store,
                Some(options.collection_id),
                &out.var_ops,
                &mut vars_map,
            );
            if let Some(next) = out.next_request {
                next_jump = match next {
                    Some(name) => NextJump::Jump(name),
                    None => NextJump::Stop,
                };
            }
            if let Some(e) = out.error {
                script_error = Some(format!("tests: {e}"));
                break;
            }
        }
    }

    error = error.or(script_error);
    let result = RequestRunResult {
        iteration,
        item_id: request.item_id,
        name: request.name.clone(),
        url: display.url.clone(),
        method: display.method.clone(),
        status,
        error,
        duration_ms,
        tests,
        console,
        skipped: false,
    };
    (result, next_jump)
}

/// Depth-first, sort-ordered list of runnable requests, optionally scoped to
/// a folder subtree.
fn ordered_requests(
    store: &Store,
    collection_id: i64,
    folder_id: Option<i64>,
) -> Result<Vec<RunnableRequest>, crate::store::StoreError> {
    let items = collections::items(store, collection_id)?;
    fn walk(
        items: &[collections::CollectionItem],
        parent: Option<i64>,
        out: &mut Vec<RunnableRequest>,
    ) {
        let mut children: Vec<_> = items.iter().filter(|i| i.parent_id == parent).collect();
        children.sort_by_key(|i| (i.sort_order, i.id));
        for child in children {
            if child.kind == "request" {
                if let Some(spec) = child
                    .req_spec
                    .clone()
                    .and_then(|v| serde_json::from_value(v).ok())
                {
                    out.push(RunnableRequest {
                        item_id: child.id,
                        name: child.name.clone(),
                        spec,
                    });
                }
            } else {
                walk(items, Some(child.id), out);
            }
        }
    }
    let mut out = Vec::new();
    walk(&items, folder_id, &mut out);
    Ok(out)
}

fn item_scripts(store: &Store, item_id: i64) -> (Option<String>, Option<String>) {
    store
        .with_conn(|conn| {
            conn.query_row(
                "SELECT pre_request_script, test_script FROM collection_items WHERE id = ?1",
                rusqlite::params![item_id],
                |row| {
                    Ok((
                        row.get::<_, Option<String>>(0)?,
                        row.get::<_, Option<String>>(1)?,
                    ))
                },
            )
        })
        .map(|(p, t)| {
            (
                p.filter(|s| !s.trim().is_empty()),
                t.filter(|s| !s.trim().is_empty()),
            )
        })
        .unwrap_or((None, None))
}

/// Only `pm.variables.set` (local scope) survives across requests of a run —
/// other scopes are persisted to the DB and re-read each request.
fn fold_local_ops(ops: &[crate::scripting::VarOp], run_vars: &mut HashMap<String, String>) {
    for op in ops.iter().filter(|o| o.scope == "local") {
        match op.value.as_deref() {
            Some(v) => run_vars.insert(op.key.clone(), v.to_owned()),
            None => run_vars.remove(&op.key),
        };
    }
}

fn overlay_data(vars: &mut Vec<Variable>, data_row: Option<&serde_json::Value>) {
    let Some(row) = data_row.and_then(|r| r.as_object()) else {
        return;
    };
    for (k, v) in row {
        let value = match v {
            serde_json::Value::String(s) => s.clone(),
            other => other.to_string(),
        };
        upsert_var(vars, k, &value);
    }
}

use crate::vars::upsert_var;

fn error_result(iteration: u32, message: &str) -> RequestRunResult {
    RequestRunResult {
        iteration,
        item_id: 0,
        name: "run".into(),
        url: String::new(),
        method: String::new(),
        status: None,
        error: Some(message.to_owned()),
        duration_ms: 0.0,
        tests: vec![],
        console: vec![],
        skipped: false,
    }
}
