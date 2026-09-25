//! `gate-calibration` (EPR-019): measure the Acceptance Gate and RealizedRisk
//! on the held-out corpora and check the rates against an approved threshold
//! profile.
//!
//! ```text
//! gate-calibration --out <dir> [--profile <file>] [--router-cost-delta <minor>]
//!                  [--corpora <dir>] [--repo <root>]
//! ```
//!
//! Writes `calibration.json` (the whole bundle) into `--out` and prints the
//! rates and the release check. Exit 0 when the release check passes, 2 when
//! it fails (no approved profile included), 1 when the holdout is refused.

use std::path::PathBuf;

use modbit_bench_gate_calibration::{ThresholdProfile, calibrate};
use sha2::Digest;

fn usage() -> ! {
    eprintln!(
        "usage: gate-calibration --out <dir> [--profile <file>] [--router-cost-delta <minor>] [--corpora <dir>] [--repo <root>]"
    );
    std::process::exit(64)
}

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut out: Option<PathBuf> = None;
    let mut profile: Option<PathBuf> = None;
    let mut delta: i64 = 0;
    let here = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut corpora = here.join("corpora");
    let mut repo = here.join("../..");
    let mut i = 0;
    while i < args.len() {
        let value = args.get(i + 1).cloned().unwrap_or_else(|| usage());
        match args[i].as_str() {
            "--out" => out = Some(PathBuf::from(value)),
            "--profile" => profile = Some(PathBuf::from(value)),
            "--router-cost-delta" => delta = value.parse().unwrap_or_else(|_| usage()),
            "--corpora" => corpora = PathBuf::from(value),
            "--repo" => repo = PathBuf::from(value),
            _ => usage(),
        }
        i += 2;
    }
    let Some(out) = out else { usage() };
    let profile = profile.map(|p| {
        let bytes = std::fs::read(&p).unwrap_or_else(|e| {
            eprintln!("{}: {e}", p.display());
            std::process::exit(1)
        });
        let parsed: ThresholdProfile = serde_json::from_slice(&bytes).unwrap_or_else(|e| {
            eprintln!("{}: not a threshold profile: {e}", p.display());
            std::process::exit(1)
        });
        (parsed, hex::encode(sha2::Sha256::digest(&bytes)))
    });
    let work = out.join("work");
    let _ = std::fs::create_dir_all(&work);
    let bundle = match calibrate(
        &repo,
        &corpora,
        &work,
        profile.as_ref().map(|(p, d)| (p, d.as_str())),
        delta,
    )
    .await
    {
        Ok(b) => b,
        Err(refusal) => {
            eprintln!(
                "holdout refused: {}",
                serde_json::to_string(&refusal).unwrap_or_default()
            );
            std::process::exit(1)
        }
    };
    let json = serde_json::to_string_pretty(&bundle).unwrap_or_default();
    if let Err(e) = std::fs::write(out.join("calibration.json"), &json) {
        eprintln!("writing the bundle: {e}");
        std::process::exit(1)
    }
    for (name, r) in &bundle.metrics.rates {
        println!(
            "{name}: {}/{} = {:.4} (95% {:.4}..{:.4})",
            r.count, r.n, r.rate, r.lower, r.upper
        );
    }
    println!("release check: {}", bundle.release.verdict);
    for reason in &bundle.release.reasons {
        println!("  {reason}");
    }
    std::process::exit(if bundle.release.verdict == "PASS" {
        0
    } else {
        2
    })
}
