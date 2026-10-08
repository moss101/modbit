//! `projection_trial`: run the paired projection trial (PX-114) against a
//! provider the owner configures, and write the report.
//!
//! ```text
//! projection_trial --cli target/debug/modbit-cli --core target/debug/modbit-core \
//!     --model <model id> --repeats 1 --out report.json [--live] [--tasks N] [--env K=V]...
//! ```
//!
//! The provider comes from the environment the Core inherits
//! (`OPENAI_API_KEY`, `MODBIT_OPENAI_BASE_URL`, `MODBIT_OPENAI_MODELS`...), the
//! way any Core is configured. Pass `--live` only when that provider is a
//! real model: without it the report says it decides nothing. At least 30
//! tasks are needed for a verdict (the built-in suite has 30).

use modbit_bench_context_economics::projection_trial::{RunConfig, report, run_matrix, validate};
use modbit_bench_context_economics::suite::builtin_suite;

fn main() {
    let mut args = std::env::args().skip(1);
    let (mut cli, mut core, mut model, mut out) = (None, None, None, None);
    let (mut repeats, mut limit, mut live, mut turns) = (1u32, usize::MAX, false, 40u32);
    let mut env: Vec<(String, String)> = Vec::new();
    while let Some(a) = args.next() {
        match a.as_str() {
            "--cli" => cli = args.next(),
            "--core" => core = args.next(),
            "--model" => model = args.next(),
            "--out" => out = args.next(),
            "--repeats" => repeats = args.next().and_then(|v| v.parse().ok()).unwrap_or(1),
            "--tasks" => {
                limit = args
                    .next()
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(usize::MAX)
            }
            "--max-turns" => turns = args.next().and_then(|v| v.parse().ok()).unwrap_or(40),
            "--live" => live = true,
            "--env" => {
                if let Some((k, v)) = args.next().and_then(|kv| {
                    kv.split_once('=')
                        .map(|(k, v)| (k.to_owned(), v.to_owned()))
                }) {
                    env.push((k, v));
                }
            }
            other => {
                eprintln!("unknown argument {other}");
                std::process::exit(2);
            }
        }
    }
    let (Some(cli), Some(core), Some(model)) = (cli, core, model) else {
        eprintln!(
            "usage: projection_trial --cli <modbit-cli> --core <modbit-core> --model <id> [--repeats N] [--tasks N] [--max-turns N] [--out file] [--live] [--env K=V]..."
        );
        std::process::exit(2);
    };
    let tasks: Vec<_> = builtin_suite().into_iter().take(limit).collect();
    let cfg = RunConfig {
        cli: cli.into(),
        core: core.into(),
        model: model.clone(),
        env,
        max_turns: turns,
        live,
    };
    let runs = run_matrix(&cfg, &tasks, repeats);
    let rep = report(&runs, &model, tasks.len(), repeats as usize, live);
    if let Err(e) = validate(&rep, tasks.len(), repeats as usize) {
        eprintln!("the report is incomplete: {e}");
        std::process::exit(1);
    }
    let json = serde_json::json!({"report": rep, "runs": runs});
    let text = serde_json::to_string_pretty(&json).unwrap_or_default();
    match out {
        Some(path) => std::fs::write(path, text).expect("write report"),
        None => println!("{text}"),
    }
}
