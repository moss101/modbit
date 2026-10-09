//! `live_trial`: the live paired projection trial (PX-114). It refuses unless
//! `MODBIT_LIVE=1`, `MODBIT_LIVE_API_KEY` and `MODBIT_LIVE_MODEL` say a real
//! provider is configured (see `modbit_bench_context_economics::live`), and
//! on refusal it prints `LIVE: NOT RUN (...)`, writes nothing and exits 2.

use modbit_bench_context_economics::live::live_config;
use modbit_bench_context_economics::projection_trial::{report, run_matrix, validate};
use modbit_bench_context_economics::suite::builtin_suite;

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
    let runs = run_matrix(&cfg.run, &tasks, cfg.repeats);
    let rep = report(&runs, &cfg.run.model, tasks.len(), repeats, true);
    if let Err(e) = validate(&rep, tasks.len(), repeats) {
        eprintln!("the live report is incomplete and is not written: {e}");
        return std::process::ExitCode::from(1);
    }
    let body = serde_json::json!({"report": rep, "runs": runs});
    let text = serde_json::to_string_pretty(&body).unwrap_or_default();
    if let Err(e) = std::fs::write(&cfg.out, text) {
        eprintln!("writing {}: {e}", cfg.out.display());
        return std::process::ExitCode::from(1);
    }
    println!("live report written to {}", cfg.out.display());
    std::process::ExitCode::SUCCESS
}
