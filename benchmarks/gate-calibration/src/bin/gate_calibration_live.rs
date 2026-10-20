//! `gate-calibration-live` (PX-135): the candidate-producing runs of the
//! widened gate calibration against a live provider. It refuses unless
//! `MODBIT_LIVE=1`, `MODBIT_LIVE_API_KEY` and `MODBIT_LIVE_MODEL` say a real
//! provider is configured (see `modbit_bench_context_economics::live`), and
//! on refusal it prints `LIVE: NOT RUN (...)`, writes nothing and exits 2.
//!
//! ```text
//! gate-calibration-live --out-dir <dir> --max-cost-usd <usd> [--repeats 2]
//!                       [--language rust ...] [--lineage discount ...]
//! gate-calibration-live --verify <dir>      # rescore the retained candidates
//! ```
//!
//! Writes `result.json`, `summary.md`, `status.json` and `runs/<id>/...`.
//! Exit 0 only when every required measurement is present; 1 incomplete; 2
//! refused; 3 an oracle needle reached the model or the gate.

use std::path::PathBuf;

#[tokio::main]
async fn main() -> std::process::ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let code = modbit_bench_gate_calibration::live::execute(
        &args,
        &|k| std::env::var(k).ok(),
        &PathBuf::from(env!("CARGO_MANIFEST_DIR")),
    )
    .await;
    std::process::ExitCode::from(code)
}
