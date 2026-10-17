//! `live_trial`: the live paired projection trial (PX-114). It refuses unless
//! `MODBIT_LIVE=1`, `MODBIT_LIVE_API_KEY` and `MODBIT_LIVE_MODEL` say a real
//! provider is configured (see `modbit_bench_context_economics::live`), and
//! on refusal it prints `LIVE: NOT RUN (...)`, writes nothing and exits 2.
//!
//! Under the `live-core` workflow it runs with a hard spend cap
//! (`MODBIT_LIVE_MAX_COST_USD`, priced from `MODBIT_OPENAI_MODELS`) on
//! `MODBIT_LIVE_PARALLEL` threads, and writes beside the report a
//! `status.json` (complete, missing, spend) and a `summary.md`; a run the cap
//! stopped is reported as partial and exits non-zero. Without a cap it runs
//! the whole matrix as before.

use modbit_bench_context_economics::live::live_config;
use modbit_bench_context_economics::projection_trial::{
    report, run_matrix, run_matrix_capped, validate,
};
use modbit_bench_context_economics::spend::{LiveStatus, SpendMeter, price_from_catalog};
use modbit_bench_context_economics::suite::builtin_suite;

/// What one run may cost before the meter admits it: well above what a
/// 40-turn run on this gateway spends, so the cap is never overshot by more
/// than the runs already in flight.
const RESERVE_PER_RUN_USD: f64 = 0.08;

fn main() -> std::process::ExitCode {
    let cfg = match live_config(&|k| std::env::var(k).ok()) {
        Ok(c) => c,
        Err(refused) => {
            eprintln!("{refused}");
            return std::process::ExitCode::from(2);
        }
    };
    for bin in [&cfg.run.cli, &cfg.run.core] {
        if !bin.exists() {
            eprintln!(
                "LIVE: NOT RUN ({} does not exist; build modbit-cli and modbit-core first)",
                bin.display()
            );
            return std::process::ExitCode::from(2);
        }
    }
    let tasks = builtin_suite();
    let repeats = cfg.repeats as usize;
    let Some(cap) = cfg.max_cost_usd else {
        // No cap: the original behaviour.
        let runs = run_matrix(&cfg.run, &tasks, cfg.repeats);
        return finish(&cfg, &tasks, &runs, None);
    };
    let catalog = std::env::var("MODBIT_OPENAI_MODELS").unwrap_or_default();
    let price = match price_from_catalog(&catalog, &cfg.run.model) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("LIVE: NOT RUN (a capped run needs the model's price in MODBIT_OPENAI_MODELS: {e})");
            return std::process::ExitCode::from(2);
        }
    };
    let meter = match SpendMeter::new(cap) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("LIVE: NOT RUN ({e})");
            return std::process::ExitCode::from(2);
        }
    };
    let parallel = std::env::var("MODBIT_LIVE_PARALLEL")
        .ok()
        .and_then(|v| v.trim().parse::<usize>().ok())
        .filter(|n| *n >= 1)
        .unwrap_or(1);
    let capped = run_matrix_capped(
        &cfg.run,
        &tasks,
        cfg.repeats,
        parallel,
        &meter,
        price,
        RESERVE_PER_RUN_USD,
    );
    let _ = repeats;
    finish(&cfg, &tasks, &capped.runs, Some((capped.planned, capped.spend)))
}

fn finish(
    cfg: &modbit_bench_context_economics::live::LiveConfig,
    tasks: &[modbit_bench_context_economics::suite::TrialTask],
    runs: &[modbit_bench_context_economics::projection_trial::RunRecord],
    capped: Option<(usize, modbit_bench_context_economics::spend::SpendRecord)>,
) -> std::process::ExitCode {
    let repeats = cfg.repeats as usize;
    let dir = cfg
        .out
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .map_or_else(|| std::path::PathBuf::from("."), std::path::Path::to_path_buf);
    let _ = std::fs::create_dir_all(&dir);
    let rep = report(runs, &cfg.run.model, tasks.len(), repeats, true);
    let validated = validate(&rep, tasks.len(), repeats);
    let mut missing: Vec<String> = Vec::new();
    if let Err(e) = &validated {
        missing.push(format!("the report is incomplete: {e}"));
    }
    if let Some((planned, _)) = &capped
        && runs.len() < *planned
    {
        missing.push(format!(
            "{} of {planned} runs were not made (the spend cap stopped the matrix)",
            planned - runs.len()
        ));
    }
    let body = serde_json::json!({"report": rep, "runs": runs});
    let text = serde_json::to_string_pretty(&body).unwrap_or_default();
    // A partial matrix is written as a partial file so it can never be read
    // as the verdict.
    let target = if validated.is_ok() && missing.is_empty() {
        cfg.out.clone()
    } else {
        dir.join("partial-report.json")
    };
    if let Err(e) = std::fs::write(&target, text) {
        eprintln!("writing {}: {e}", target.display());
        return std::process::ExitCode::from(1);
    }
    if let Some((_, spend)) = capped {
        let status = LiveStatus::new("px114", missing.clone(), spend);
        let _ = std::fs::write(
            dir.join("status.json"),
            serde_json::to_string_pretty(&status).unwrap_or_default(),
        );
        let mut summary = format!(
            "### PX-114 paired projection trial (live)\n\n- model: {} (live: true)\n- tasks x repeats x arms: {} x {} x 3, {} runs made\n- spend: ${:.4} of ${:.2}, {} calls\n",
            cfg.run.model,
            tasks.len(),
            repeats,
            runs.len(),
            status.spend.spent_usd,
            status.spend.cap_usd,
            status.spend.calls
        );
        for c in &rep.comparisons {
            summary.push_str(&format!(
                "- {} vs {}: {} pairs, accuracy delta {:.3} [{:.3}, {:.3}], tokens delta {:.0} [{:.0}, {:.0}], verdict {:?}\n",
                c.arm.label(),
                c.against.label(),
                c.pairs,
                c.accuracy_delta.mean,
                c.accuracy_delta.ci95.0,
                c.accuracy_delta.ci95.1,
                c.tokens_delta.mean,
                c.tokens_delta.ci95.0,
                c.tokens_delta.ci95.1,
                c.direction
            ));
        }
        summary.push_str(&format!("- recommendation: {}\n", rep.recommendation));
        let _ = std::fs::write(dir.join("summary.md"), summary);
        if !status.complete {
            eprintln!("the live run is partial: {:?}", status.missing);
            return std::process::ExitCode::from(1);
        }
    } else if let Err(e) = validated {
        eprintln!("the live report is incomplete and is not written: {e}");
        return std::process::ExitCode::from(1);
    }
    println!("live report written to {}", target.display());
    std::process::ExitCode::SUCCESS
}
