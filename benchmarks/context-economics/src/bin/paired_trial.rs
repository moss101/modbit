//! `paired-trial`: the live paired benchmark of DIRECT, CASCADE and CRITIQUE
//! (PX-136) and the leg-sample measurement it feeds (PX-133).
//!
//! ```text
//! paired-trial --row px136 --out-dir <dir> --max-cost-usd <usd> [--parallel 3]
//!              [--model glm-5.3-flash] [--second-endpoint anthropic] [--second-model <id>]
//!              [--arms direct,cascade,critique] [--limit-tasks N] [--max-turns 30]
//! paired-trial --rescore <dir>
//! paired-trial --write-taskset <file>
//! ```
//!
//! It refuses (exit 2, nothing written) unless `MODBIT_LIVE=1` and a real
//! credential, base URL and priced catalog exist for both provider families
//! the registry's two bindings name. It exits 0 only when every required
//! measurement is present and the spend cap did not end the run; the bundle
//! (`result.json`, `status.json`, `summary.md`, the per-trial CLI logs and
//! event logs) is written either way.

use std::path::PathBuf;
use std::time::Duration;

use modbit_bench_context_economics::paired::{
    Arm, Options, PairedConfig, builtin_taskset, execute, live_bindings, rescore_bundle,
};

fn usage() -> std::process::ExitCode {
    eprintln!(
        "usage: paired-trial --row <px136|px133> --out-dir <dir> --max-cost-usd <usd> [--parallel N] [--model <id>] [--second-endpoint <openai|anthropic>] [--second-model <id>] [--arms a,b,c] [--limit-tasks N] [--max-turns N] [--trial-timeout-secs N] | --rescore <dir> | --write-taskset <file>"
    );
    std::process::ExitCode::from(64)
}

fn main() -> std::process::ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut flags = std::collections::BTreeMap::new();
    let mut i = 0;
    while i < args.len() {
        let Some(name) = args[i].strip_prefix("--") else {
            return usage();
        };
        let Some(value) = args.get(i + 1) else {
            return usage();
        };
        flags.insert(name.to_owned(), value.clone());
        i += 2;
    }
    if let Some(dir) = flags.get("rescore") {
        return match rescore_bundle(&PathBuf::from(dir)) {
            Ok(digest) => {
                println!("the report digest reproduces from the retained data: {digest}");
                std::process::ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("rescore failed: {e}");
                std::process::ExitCode::from(1)
            }
        };
    }
    if let Some(file) = flags.get("write-taskset") {
        let text = serde_json::to_string_pretty(&builtin_taskset()).unwrap_or_default();
        return match std::fs::write(file, format!("{text}\n")) {
            Ok(()) => std::process::ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("{file}: {e}");
                std::process::ExitCode::from(1)
            }
        };
    }
    let var = |k: &str| std::env::var(k).ok();
    let (opener, second, environment) = match live_bindings(
        &var,
        flags.get("model").map(String::as_str),
        flags
            .get("second-endpoint")
            .map_or("anthropic", String::as_str),
        flags.get("second-model").map(String::as_str),
    ) {
        Ok(b) => b,
        Err(refused) => {
            eprintln!("{refused}");
            return std::process::ExitCode::from(2);
        }
    };
    let cap = flags
        .get("max-cost-usd")
        .cloned()
        .or_else(|| var("MODBIT_LIVE_MAX_COST_USD"))
        .and_then(|v| v.trim().parse::<f64>().ok())
        .filter(|c| c.is_finite() && *c > 0.0);
    let Some(max_cost_usd) = cap else {
        eprintln!(
            "LIVE: NOT RUN (--max-cost-usd or MODBIT_LIVE_MAX_COST_USD must be a positive number of dollars)"
        );
        return std::process::ExitCode::from(2);
    };
    let Some(out_dir) = flags.get("out-dir").map(PathBuf::from) else {
        return usage();
    };
    let row = flags.get("row").cloned().unwrap_or_else(|| "px136".into());
    let (default_arms, min_tasks, min_esc, min_rev) = match row.as_str() {
        "px136" => ("direct,cascade,critique", 30, 0, 0),
        "px133" => ("cascade,critique", 10, 1, 1),
        _ => return usage(),
    };
    let arms: Vec<Arm> = flags
        .get("arms")
        .map_or(default_arms, String::as_str)
        .split(',')
        .filter_map(|a| Arm::parse(a.trim()))
        .collect();
    if arms.is_empty() {
        return usage();
    }
    let here = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let exe = |name: &str| {
        std::env::current_exe()
            .ok()
            .and_then(|p| {
                p.parent()
                    .map(|d| d.join(format!("{name}{}", std::env::consts::EXE_SUFFIX)))
            })
            .unwrap_or_else(|| PathBuf::from(name))
    };
    let cli = var("MODBIT_LIVE_CLI").map_or_else(|| exe("modbit-cli"), PathBuf::from);
    let core = var("MODBIT_LIVE_CORE").map_or_else(|| exe("modbit-core"), PathBuf::from);
    for bin in [&cli, &core] {
        if !bin.exists() {
            eprintln!(
                "LIVE: NOT RUN ({} does not exist; build modbit-cli and modbit-core first)",
                bin.display()
            );
            return std::process::ExitCode::from(2);
        }
    }
    let number = |k: &str, default: u64| {
        flags
            .get(k)
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(default)
    };
    let cfg = PairedConfig {
        cli,
        core,
        work: std::env::temp_dir().join(format!("modbit-paired-{}", std::process::id())),
        opener,
        second,
        env: Vec::new(),
        max_turns: u32::try_from(number("max-turns", 30)).unwrap_or(30),
        trial_timeout: Duration::from_secs(number("trial-timeout-secs", 1500)),
        parallel: usize::try_from(number("parallel", 3)).unwrap_or(3),
        repeats: u32::try_from(number("repeats", 1)).unwrap_or(1).max(1),
        live: true,
        question_answer: "Use your best judgement and proceed without further questions; keep the change minimal and make the check pass.".into(),
    };
    let limit_tasks = flags
        .get("limit-tasks")
        .and_then(|v| v.parse::<usize>().ok());
    let options = Options {
        row,
        cfg,
        taskset: here.join("paired/taskset.json"),
        arms,
        max_cost_usd,
        out_dir,
        min_tasks,
        limit_tasks,
        min_escalation_samples: min_esc,
        min_reviewer_samples: min_rev,
        environment,
    };
    match execute(&options) {
        Ok(status) => {
            let s = &status.spend;
            println!(
                "{}: complete={} spend=${:.4} of ${:.2} calls={} stopped_by_cap={}",
                status.row, status.complete, s.spent_usd, s.cap_usd, s.calls, s.stopped_by_cap
            );
            for m in &status.missing {
                println!("MISSING: {m}");
            }
            let _ = std::fs::remove_dir_all(&options.cfg.work);
            if status.complete {
                std::process::ExitCode::SUCCESS
            } else {
                std::process::ExitCode::from(1)
            }
        }
        Err(e) => {
            eprintln!("the paired run could not be made: {e}");
            std::process::ExitCode::from(1)
        }
    }
}
