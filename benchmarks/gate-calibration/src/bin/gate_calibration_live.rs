//! `gate-calibration-live` (PX-135): the candidate-producing runs of the
//! widened gate calibration against a live provider. It refuses unless
//! `MODBIT_LIVE=1`, `MODBIT_LIVE_API_KEY` and `MODBIT_LIVE_MODEL` say a real
//! provider is configured (see `modbit_bench_context_economics::live`), and
//! on refusal it prints `LIVE: NOT RUN (...)`, writes nothing and exits 2.
//!
//! ```text
//! gate-calibration-live --out <file> --work <dir> [--language rust ...]
//! ```

use std::path::PathBuf;

use modbit_bench_gate_calibration::live::{live_config, run_live};
use modbit_bench_gate_calibration::widened::load;

#[tokio::main]
async fn main() -> std::process::ExitCode {
    let cfg = match live_config(&|k| std::env::var(k).ok()) {
        Ok(c) => c,
        Err(refused) => {
            eprintln!("{refused}");
            return std::process::ExitCode::from(2);
        }
    };
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut work: Option<PathBuf> = None;
    let mut out = cfg.out.clone();
    let mut languages = Vec::new();
    let mut i = 0;
    while i < args.len() {
        let value = args.get(i + 1).cloned();
        match (args[i].as_str(), value) {
            ("--work", Some(v)) => work = Some(PathBuf::from(v)),
            ("--out", Some(v)) => out = PathBuf::from(v),
            ("--language", Some(v)) => languages.push(v),
            _ => {
                eprintln!(
                    "usage: gate-calibration-live --work <dir> [--out <file>] [--language <l>]..."
                );
                return std::process::ExitCode::from(64);
            }
        }
        i += 2;
    }
    let Some(work) = work else {
        eprintln!("usage: gate-calibration-live --work <dir> [--out <file>] [--language <l>]...");
        return std::process::ExitCode::from(64);
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
    let here = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let repo = here.join("../..");
    let corpora = here.join("corpora");
    let corpus = match load(&corpora.join("widened")) {
        Ok(c) => c,
        Err(r) => {
            eprintln!("the widened corpus is unreadable: {r:?}");
            return std::process::ExitCode::from(1);
        }
    };
    let _ = std::fs::create_dir_all(&work);
    match run_live(&cfg, &repo, &corpora, &work, &corpus, &languages).await {
        Ok(report) => {
            let text = serde_json::to_string_pretty(&report).unwrap_or_default();
            if let Err(e) = std::fs::write(&out, text) {
                eprintln!("writing {}: {e}", out.display());
                return std::process::ExitCode::from(1);
            }
            let leaks: usize = report.rows.iter().map(|r| r.leaks.len()).sum();
            println!(
                "live report written to {} ({} runs, {leaks} oracle leaks)",
                out.display(),
                report.rows.len()
            );
            if leaks > 0 {
                return std::process::ExitCode::from(3);
            }
            std::process::ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("the live run is incomplete and is not written: {e}");
            std::process::ExitCode::from(1)
        }
    }
}
