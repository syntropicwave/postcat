//! Headless collection runner (Newman analog) sharing the app's core.
//!
//! Usage:
//!   postcat-cli run --collection <name-or-id> [--db <path>] [--folder <id>]
//!       [--iterations N] [--delay ms] [--data file.json] [--env <name>]
//!
//! Exit code: 0 = all tests passed, 1 = failures or errors.
#![allow(clippy::exit, clippy::print_stdout, clippy::print_stderr)]

use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use postcat_lib::{collections, runner, store::Store};
use reqwest_cookie_store::{CookieStore, CookieStoreMutex};

fn usage() -> ! {
    eprintln!(
        "postcat-cli — headless collection runner\n\n\
         USAGE:\n  postcat-cli run --collection <name-or-id> [options]\n\n\
         OPTIONS:\n  \
         --db <path>         SQLite database (default: the app's database)\n  \
         --folder <id>       run only this folder subtree\n  \
         --iterations <n>    number of iterations (default 1)\n  \
         --delay <ms>        delay between requests\n  \
         --data <file>       JSON array of data rows\n  \
         --env <name>        activate this environment for the run"
    );
    std::process::exit(2);
}

struct Args {
    db: Option<String>,
    collection: String,
    folder: Option<i64>,
    iterations: u32,
    delay: u64,
    data: Option<String>,
    env: Option<String>,
}

fn parse_args() -> Args {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    if argv.first().map(String::as_str) != Some("run") {
        usage();
    }
    let mut args = Args {
        db: None,
        collection: String::new(),
        folder: None,
        iterations: 1,
        delay: 0,
        data: None,
        env: None,
    };
    let mut i = 1;
    while i < argv.len() {
        let value = |i: &mut usize| -> String {
            *i += 1;
            argv.get(*i).cloned().unwrap_or_else(|| usage())
        };
        match argv[i].as_str() {
            "--db" => args.db = Some(value(&mut i)),
            "--collection" => args.collection = value(&mut i),
            "--folder" => args.folder = value(&mut i).parse().ok(),
            "--iterations" => args.iterations = value(&mut i).parse().unwrap_or(1),
            "--delay" => args.delay = value(&mut i).parse().unwrap_or(0),
            "--data" => args.data = Some(value(&mut i)),
            "--env" => args.env = Some(value(&mut i)),
            _ => usage(),
        }
        i += 1;
    }
    if args.collection.is_empty() {
        usage();
    }
    args
}

fn default_db_path() -> std::path::PathBuf {
    // Same location the desktop app uses.
    let base = std::env::var("APPDATA")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| {
            dirs_fallback_home().join(if cfg!(target_os = "macos") {
                "Library/Application Support"
            } else {
                ".local/share"
            })
        });
    base.join("dev.postcat.app").join("postcat.db")
}

fn dirs_fallback_home() -> std::path::PathBuf {
    std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| std::path::PathBuf::from("."))
}

#[tokio::main]
async fn main() {
    let args = parse_args();

    let db_path = args
        .db
        .map(std::path::PathBuf::from)
        .unwrap_or_else(default_db_path);
    let store = match Store::open(&db_path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("cannot open database {}: {e}", db_path.display());
            std::process::exit(2);
        }
    };

    let collection = {
        let list = collections::list(&store).unwrap_or_default();
        let by_id: Option<_> = args
            .collection
            .parse::<i64>()
            .ok()
            .and_then(|id| list.iter().find(|c| c.id == id));
        match by_id.or_else(|| list.iter().find(|c| c.name == args.collection)) {
            Some(c) => c.id,
            None => {
                eprintln!("collection not found: {}", args.collection);
                std::process::exit(2);
            }
        }
    };

    if let Some(env_name) = &args.env {
        let envs = collections::env_list(&store).unwrap_or_default();
        match envs.iter().find(|e| &e.name == env_name) {
            Some(env) => {
                let _ = collections::env_set_active(&store, Some(env.id));
            }
            None => {
                eprintln!("environment not found: {env_name}");
                std::process::exit(2);
            }
        }
    }

    let data = args.data.as_ref().map(|path| {
        let text = std::fs::read_to_string(path).unwrap_or_else(|e| {
            eprintln!("cannot read data file {path}: {e}");
            std::process::exit(2);
        });
        serde_json::from_str::<Vec<serde_json::Value>>(&text).unwrap_or_else(|e| {
            eprintln!("data file must be a JSON array of objects: {e}");
            std::process::exit(2);
        })
    });

    let jar = Arc::new(CookieStoreMutex::new(CookieStore::default()));
    let options = runner::RunOptions {
        collection_id: collection,
        folder_id: args.folder,
        iterations: args.iterations,
        delay_ms: args.delay,
        data,
    };

    let report = runner::run(
        &store,
        jar,
        options,
        Arc::new(AtomicBool::new(false)),
        |r| {
            let status = match (&r.error, r.status) {
                (Some(e), _) => format!("ERROR {e}"),
                (None, Some(s)) => s.to_string(),
                (None, None) => "-".into(),
            };
            let failed = r.tests.iter().filter(|t| !t.passed).count();
            let marker = if r.skipped {
                "SKIP".into()
            } else if failed > 0 {
                format!("FAIL ({}/{})", r.tests.len() - failed, r.tests.len())
            } else if !r.tests.is_empty() {
                format!("PASS ({}/{})", r.tests.len(), r.tests.len())
            } else {
                String::new()
            };
            println!(
                "[iter {}] {} {} -> {} {:.0}ms {}",
                r.iteration + 1,
                r.method,
                r.name,
                status,
                r.duration_ms,
                marker
            );
            for t in r.tests.iter().filter(|t| !t.passed) {
                println!("     ✗ {} — {}", t.name, t.error.as_deref().unwrap_or(""));
            }
        },
    )
    .await;

    println!(
        "\n{} requests, {} tests passed, {} failed, {} errors, {:.1}s{}",
        report.total_requests,
        report.passed_tests,
        report.failed_tests,
        report.errors,
        report.duration_ms / 1000.0,
        if report.cancelled { " (cancelled)" } else { "" }
    );

    if report.failed_tests > 0 || report.errors > 0 {
        std::process::exit(1);
    }
}
