//! `compaction-eval-live`: the live half of PX-138. It refuses (exit 2,
//! nothing written) unless `MODBIT_LIVE=1` and a real provider are
//! configured (see `modbit_bench_context_economics::live`) and a positive
//! spend cap is given. Exit 0 only when every required measurement is
//! present; 1 for an incomplete or failed run (the partial result is still
//! written); 2 for a refusal.
//!
//! ```text
//! compaction-eval-live --out-dir <dir> --max-cost-usd <usd> [--runs 24] [--model <id>]
//! compaction-eval-live --rescore <dir>
//! ```

use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;

use modbit_bench_context_economics::chat::Chat;
use modbit_bench_context_economics::compaction_eval::long_runs;
use modbit_bench_context_economics::compaction_live::{MIN_RUNS, collect, finish, rescore_dir};
use modbit_bench_context_economics::live::live_config;
use modbit_bench_context_economics::spend::SpendMeter;

struct Args {
    out_dir: Option<PathBuf>,
    max_cost: Option<String>,
    runs: usize,
    model: Option<String>,
    rescore: Option<PathBuf>,
}

fn parse(args: &[String]) -> Result<Args, String> {
    let mut a = Args {
        out_dir: None,
        max_cost: None,
        runs: 24,
        model: None,
        rescore: None,
    };
    let mut it = args.iter();
    while let Some(flag) = it.next() {
        let mut value = |name: &str| {
            it.next()
                .cloned()
                .ok_or_else(|| format!("{name} needs a value"))
        };
        match flag.as_str() {
            "--out-dir" => a.out_dir = Some(PathBuf::from(value("--out-dir")?)),
            "--max-cost-usd" => a.max_cost = Some(value("--max-cost-usd")?),
            "--runs" => {
                a.runs = value("--runs")?
                    .parse::<usize>()
                    .ok()
                    .filter(|n| *n >= 1)
                    .ok_or("--runs: a whole number of at least 1")?;
            }
            "--model" => a.model = Some(value("--model")?),
            "--rescore" => a.rescore = Some(PathBuf::from(value("--rescore")?)),
            other => return Err(format!("unknown argument {other}")),
        }
    }
    Ok(a)
}

#[tokio::main]
async fn main() -> ExitCode {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    let args = match parse(&argv) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("compaction-eval-live: {e}");
            return ExitCode::from(2);
        }
    };
    if let Some(dir) = &args.rescore {
        return match rescore_dir(dir) {
            Ok(r) => {
                println!(
                    "digest {} reproduces from the retained exchanges ({} runs scored)",
                    r.digest, r.runs_scored
                );
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("RESCORE FAILED: {e}");
                ExitCode::from(1)
            }
        };
    }
    let cfg = match live_config(&|k| std::env::var(k).ok()) {
        Ok(c) => c,
        Err(refused) => {
            eprintln!("{refused}");
            return ExitCode::from(2);
        }
    };
    let Some(out_dir) = args.out_dir else {
        eprintln!("compaction-eval-live: --out-dir is required");
        return ExitCode::from(2);
    };
    let cap = match args.max_cost.as_deref() {
        Some(v) => v
            .trim()
            .parse::<f64>()
            .ok()
            .filter(|c| c.is_finite() && *c > 0.0),
        None => cfg.max_cost_usd,
    };
    let Some(cap) = cap else {
        eprintln!(
            "LIVE: NOT RUN (a positive --max-cost-usd or MODBIT_LIVE_MAX_COST_USD is required)"
        );
        return ExitCode::from(2);
    };
    let model = args.model.unwrap_or_else(|| cfg.run.model.clone());
    let env = cfg.run.env.clone();
    let lookup = move |k: &str| {
        env.iter()
            .find(|(n, _)| n == k)
            .map(|(_, v)| v.clone())
            .or_else(|| std::env::var(k).ok())
    };
    let chat = match Chat::from_lookup(&lookup, &model) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("LIVE: NOT RUN ({e})");
            return ExitCode::from(2);
        }
    };
    let meter = match SpendMeter::new(cap) {
        Ok(m) => Arc::new(m),
        Err(e) => {
            eprintln!("LIVE: NOT RUN ({e})");
            return ExitCode::from(2);
        }
    };
    if let Err(e) = std::fs::create_dir_all(&out_dir) {
        eprintln!("compaction-eval-live: {}: {e}", out_dir.display());
        return ExitCode::from(1);
    }
    if args.runs < MIN_RUNS {
        eprintln!(
            "compaction-eval-live: --runs {} is below the required {MIN_RUNS}; the result will be incomplete",
            args.runs
        );
    }
    let runs = long_runs(args.runs);
    let collected = match collect(&chat, &meter, &runs, &out_dir).await {
        Ok(c) => c,
        Err(e) => {
            eprintln!("compaction-eval-live: {e}");
            return ExitCode::from(1);
        }
    };
    match finish(&out_dir, &meter, &collected.failures) {
        Ok((report, status)) => {
            println!(
                "{} runs scored; digest {}; {}",
                report.runs_scored,
                report.digest,
                if status.complete {
                    "COMPLETE"
                } else {
                    "INCOMPLETE"
                }
            );
            for m in &status.missing {
                eprintln!("missing: {m}");
            }
            if status.complete {
                ExitCode::SUCCESS
            } else {
                ExitCode::from(1)
            }
        }
        Err(e) => {
            eprintln!("compaction-eval-live: {e}");
            ExitCode::from(1)
        }
    }
}
