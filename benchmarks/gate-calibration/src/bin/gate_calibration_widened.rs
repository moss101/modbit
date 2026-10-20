//! `gate-calibration-widened` (PX-135): the widened corpora measured under the
//! previous gate and the gate with generated adversarial checks.
//!
//! ```text
//! gate-calibration-widened --out <dir> [--corpora <dir>] [--repo <root>] [--no-legacy]
//! ```
//!
//! Writes `widened.json` into `--out` and prints the per-class before/after
//! table, the per-language rates and the leak search. Exit 0 when the search
//! is clean, 3 when an oracle needle reached the gate, 1 when the holdout is
//! refused. The release check is printed and fails by design without an
//! approved threshold profile; this tool never attests a gate.

use std::path::PathBuf;

use modbit_bench_gate_calibration::widened::{RunOptions, run};

fn usage() -> ! {
    eprintln!(
        "usage: gate-calibration-widened --out <dir> [--corpora <dir>] [--repo <root>] [--no-legacy]"
    );
    std::process::exit(64)
}

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut out: Option<PathBuf> = None;
    let here = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut corpora = here.join("corpora");
    let mut repo = here.join("../..");
    let mut legacy = true;
    let mut i = 0;
    while i < args.len() {
        if args[i] == "--no-legacy" {
            legacy = false;
            i += 1;
            continue;
        }
        let value = args.get(i + 1).cloned().unwrap_or_else(|| usage());
        match args[i].as_str() {
            "--out" => out = Some(PathBuf::from(value)),
            "--corpora" => corpora = PathBuf::from(value),
            "--repo" => repo = PathBuf::from(value),
            _ => usage(),
        }
        i += 2;
    }
    let Some(out) = out else { usage() };
    let work = out.join("work");
    let _ = std::fs::create_dir_all(&work);
    let report = match run(
        &repo,
        &corpora,
        &work,
        &RunOptions {
            include_legacy: legacy,
            ..RunOptions::default()
        },
    )
    .await
    {
        Ok(r) => r,
        Err(refusal) => {
            eprintln!(
                "holdout refused: {}",
                serde_json::to_string(&refusal).unwrap_or_default()
            );
            std::process::exit(1)
        }
    };
    let json = serde_json::to_string_pretty(&report).unwrap_or_default();
    let target = out.join("widened.json");
    if let Err(e) = std::fs::write(&target, &json) {
        eprintln!("writing {}: {e}", target.display());
        std::process::exit(1)
    }
    println!("report digest {}", report.report_digest);
    println!("seeded incorrect candidates accepted (previous gate -> widened gate):");
    for c in &report.before_after {
        println!(
            "  {:10} seeded {:3}  accepted before {:3}  accepted after {:3}  newly rejected {:3}",
            c.class, c.seeded, c.accepted_by_previous, c.accepted_by_widened, c.newly_rejected
        );
    }
    for l in &report.languages {
        println!("{} [{}] cases {}", l.language, l.status, l.cases);
        for s in &l.slices {
            println!(
                "  {:14} {:12} n={:3} (min {:2}) previous {}/{} [{:.3},{:.3}]  widened {}/{} [{:.3},{:.3}]",
                s.slice,
                s.measure,
                s.n,
                s.minimum,
                s.previous.count,
                s.previous.n,
                s.previous.lower,
                s.previous.upper,
                s.widened.count,
                s.widened.n,
                s.widened.lower,
                s.widened.upper,
            );
        }
        if let Some(r) = &l.risk {
            println!(
                "  risk false-negative {}/{} [{:.3},{:.3}]",
                r.false_negative.count,
                r.false_negative.n,
                r.false_negative.lower,
                r.false_negative.upper
            );
        }
    }
    println!(
        "minima met: {}; release check: {} ({}); gates attested: {}",
        report.minima_met,
        report.release.verdict,
        report.release.reasons.first().map_or("", String::as_str),
        report.gates_attested
    );
    if !report.leak.hits.is_empty() {
        eprintln!("ORACLE LEAK: {:?}", report.leak.hits);
        std::process::exit(3)
    }
}
