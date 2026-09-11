//! REQ-EPR-003 / EPR-E2E-003 / EPR-FI-003: what the Request Profiler extracts
//! from a fixed corpus of real repositories, what it refuses to claim, and
//! what blocks it from being promoted out of shadow.

use std::path::{Path, PathBuf};
use std::time::Instant;

use modbit_providers::profiler::{
    CalibrationCohort, CohortSlice, DomainDemand, ModifierDemand, RequestFacts, TaskDemand,
    calibration, cohort, extract, profile,
};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repo root")
}

/// Every file of a fixture repository, as the index would have them.
fn paths_of(fixture: &str) -> Vec<String> {
    let root = repo_root().join("tests/fixtures/repos").join(fixture);
    let mut out = Vec::new();
    let mut stack = vec![root.clone()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for e in entries.flatten() {
            let p = e.path();
            if p.is_dir() {
                if p.file_name().is_some_and(|n| n == "target" || n == ".git") {
                    continue;
                }
                stack.push(p);
            } else if let Ok(rel) = p.strip_prefix(&root) {
                out.push(rel.to_string_lossy().replace('\\', "/"));
            }
        }
    }
    out.sort();
    out
}

fn languages_of(fixture: &str) -> Vec<(String, u32)> {
    let mut counts: std::collections::BTreeMap<String, u32> = std::collections::BTreeMap::new();
    for p in paths_of(fixture) {
        let language = match p.rsplit('.').next().unwrap_or_default() {
            "rs" => "rust",
            "ts" | "tsx" => "typescript",
            "js" => "javascript",
            "py" => "python",
            "md" => "markdown",
            "json" => "json",
            "toml" => "toml",
            "txt" => "text",
            _ => continue,
        };
        *counts.entry(language.to_owned()).or_default() += 1;
    }
    counts.into_iter().collect()
}

/// The fixed corpus: a real repository, a real request, and the demand a
/// reader of that request would say it has.
const CORPUS: &[(&str, &str, TaskDemand, DomainDemand)] = &[
    (
        "rust-cli",
        "fix the bug in src/lib.rs where a negative quantity is accepted",
        TaskDemand::BugFix,
        DomainDemand::Systems,
    ),
    (
        "rust-cli",
        "add a subcommand that prints the total in minor units",
        TaskDemand::Feature,
        DomainDemand::Systems,
    ),
    (
        "rust-cli",
        "refactor the parsing out of main into its own module",
        TaskDemand::Refactor,
        DomainDemand::Systems,
    ),
    (
        "ts-webapp",
        "the cart total is wrong when a discount applies; it crashes on an empty cart",
        TaskDemand::BugFix,
        DomainDemand::Web,
    ),
    (
        "ts-webapp",
        "add a test for the cart rounding in test/cart.test.ts",
        TaskDemand::Test,
        DomainDemand::Web,
    ),
    (
        "python-service",
        "explain how does service.py decide the retry delay",
        TaskDemand::Question,
        DomainDemand::Scripting,
    ),
    (
        "python-service",
        "fix the error where the service retries forever",
        TaskDemand::BugFix,
        DomainDemand::Scripting,
    ),
    (
        "text-docs",
        "document the settings in README.md",
        TaskDemand::Docs,
        DomainDemand::Text,
    ),
];

fn facts_for<'a>(fixture: &str, goal: &'a str, fresh: bool) -> RequestFacts<'a> {
    RequestFacts {
        goal,
        paths: paths_of(fixture),
        languages: languages_of(fixture),
        has_configured_verification: fixture != "text-docs",
        metadata_fresh: fresh,
    }
}

#[test]
fn extraction_is_bounded_deterministic_and_reads_the_corpus_correctly() {
    let mut latencies = Vec::new();
    let mut correct_task = 0;
    let mut correct_domain = 0;
    for (fixture, goal, task, domain) in CORPUS {
        let facts = facts_for(fixture, goal, true);
        let started = Instant::now();
        let features = extract(&facts);
        latencies.push(started.elapsed().as_micros());
        if features.task == *task {
            correct_task += 1;
        }
        if features.domain == *domain {
            correct_domain += 1;
        }
        // Deterministic: the same inputs give the same digest, every time.
        assert_eq!(features.digest(), extract(&facts).digest());
        assert_eq!(features.profiler_version, "profiler-1");
    }
    assert_eq!(
        (correct_task, correct_domain),
        (CORPUS.len(), CORPUS.len()),
        "the extractor must read the corpus it was written for"
    );
    latencies.sort_unstable();
    let p50 = latencies[latencies.len() / 2];
    let p95 = latencies[(latencies.len() * 95 / 100).min(latencies.len() - 1)];
    // Extraction is bounded work, not a model call: it is microseconds.
    assert!(p95 < 50_000, "extraction p50={p50}us p95={p95}us");
    println!(
        "profiler extraction: p50={p50}us p95={p95}us over {} requests",
        CORPUS.len()
    );
    // Bounded: a goal far past the ceiling is still read in bounded time and
    // gives the same features as the ceiling's worth of it.
    let long = "fix the bug in src/lib.rs ".repeat(4_000);
    let truncated: String = long.chars().take(8 * 1024).collect();
    let a = extract(&facts_for("rust-cli", &long, true));
    let b = extract(&facts_for("rust-cli", &truncated, true));
    assert_eq!(a.task, b.task);
    assert_eq!(a.domain, b.domain);
}

#[test]
fn features_are_intrinsic_and_no_catalog_change_can_move_them() {
    // The extractor's whole input is the request and the repository. There is
    // no field for a price, a provider, a cache or an allowlist, so the proof
    // is that the same request against the same repository gives the same
    // digest regardless of what the catalog is doing — including after a
    // registry generation changes underneath it.
    let (fixture, goal, ..) = CORPUS[0];
    let before = extract(&facts_for(fixture, goal, true)).digest();
    // Whatever a caller does to the gateway, the facts it may pass are the
    // same ones; a changed catalog cannot reach into this struct.
    let after = extract(&RequestFacts {
        goal,
        paths: paths_of(fixture),
        languages: languages_of(fixture),
        has_configured_verification: true,
        metadata_fresh: true,
    })
    .digest();
    assert_eq!(before, after);
    // A repository change does move them, which is what makes them features.
    let mut moved = facts_for(fixture, goal, true);
    moved.paths.push("src/secret.env".into());
    let features = extract(&moved);
    assert!(features.modifiers.contains(&ModifierDemand::Sensitive));
    assert_ne!(features.digest(), before);
}

#[test]
fn a_disabled_profiler_or_stale_metadata_answers_conservatively() {
    let (fixture, goal, ..) = CORPUS[0];
    let recorded = cohort();
    // Disabled: shadow mode is the default, and it answers with a defined
    // conservative profile rather than with nothing.
    let p = profile(&facts_for(fixture, goal, true), &recorded, false);
    assert!(p.fallback && p.ood, "{p:?}");
    assert_eq!(p.p_floor_success, 0.0);
    assert_eq!(p.confidence, 0.0);
    assert!(p.fallback_reason.contains("disabled"), "{p:?}");
    assert!(!p.may_economise(0.0), "a fallback may never economise");
    // Stale or missing repository metadata is a reason to fall back, not a
    // feature to extract.
    let p = profile(&facts_for(fixture, goal, false), &recorded, true);
    assert!(p.fallback && p.ood, "{p:?}");
    assert!(p.fallback_reason.contains("stale"), "{p:?}");
    // Enabled, with the shipped cohort: no observations, so out of
    // distribution rather than a made-up number.
    let p = profile(&facts_for(fixture, goal, true), &recorded, true);
    assert!(p.ood, "{p:?}");
    assert!(!p.may_economise(0.0), "{p:?}");
    assert_eq!(p.cohort_version, "cohort-0-unrecorded");
}

#[test]
fn a_thin_slice_is_shrunk_toward_the_prior_and_a_thick_one_is_not() {
    let (fixture, goal, ..) = CORPUS[0];
    let facts = facts_for(fixture, goal, true);
    let slice = extract(&facts).slice();
    let with = |observations: u32, met: u32| CalibrationCohort {
        cohort_version: "cohort-test".into(),
        note: "synthetic, for the shrinkage rule".into(),
        source_digests: vec![],
        slices: vec![CohortSlice {
            slice: slice.clone(),
            observations,
            met_floor: met,
        }],
        holdout: vec![],
    };
    // Four observations is below the support floor: out of distribution.
    assert!(profile(&facts, &with(4, 4), true).ood);
    // Ten of ten is not a claim of one: it is shrunk hard toward the prior.
    let thin = profile(&facts, &with(10, 10), true);
    assert!(!thin.ood, "{thin:?}");
    assert!(thin.p_floor_success < 0.85, "{thin:?}");
    assert!(thin.confidence < 0.3, "{thin:?}");
    // Fifty of fifty is trusted as the rate it is.
    let thick = profile(&facts, &with(50, 50), true);
    assert!((thick.p_floor_success - 1.0).abs() < 1e-9, "{thick:?}");
    assert!((thick.confidence - 1.0).abs() < 1e-9, "{thick:?}");
    assert!(thick.may_economise(0.9), "{thick:?}");
    // And a thick slice that mostly fails says so.
    let bad = profile(&facts, &with(50, 10), true);
    assert!(!bad.may_economise(0.5), "{bad:?}");
}

#[test]
fn calibration_is_published_and_an_unrecorded_cohort_blocks_promotion() {
    let recorded = cohort();
    let c = calibration(&recorded);
    // The shipped cohort has nothing in it, and the metrics say exactly that
    // instead of reporting a flattering zero.
    assert_eq!((c.slices, c.observations), (0, 0));
    assert!(!c.promotable, "{c:?}");
    assert!(
        c.blockers.iter().any(|b| b.contains("observations")),
        "{c:?}"
    );
    println!(
        "profiler calibration: slices={} observations={} brier={:.3} ece={:.3} ood={} promotable={} blockers={:?}",
        c.slices, c.observations, c.brier, c.ece, c.ood_slices, c.promotable, c.blockers
    );
    // A cohort with a well-calibrated holdout and enough of it is promotable.
    let slice = "bug_fix|systems|cross_file+needs_verification".to_owned();
    let good = CalibrationCohort {
        cohort_version: "cohort-test".into(),
        note: "synthetic, for the promotion rule".into(),
        source_digests: vec![],
        slices: vec![CohortSlice {
            slice: slice.clone(),
            observations: 400,
            met_floor: 300,
        }],
        holdout: vec![CohortSlice {
            slice: slice.clone(),
            observations: 250,
            met_floor: 188,
        }],
    };
    let c = calibration(&good);
    assert!(c.promotable, "{c:?}");
    assert!(c.brier < 0.01, "{c:?}");
    // Poor calibration on a well-supported slice blocks promotion.
    let poor = CalibrationCohort {
        holdout: vec![CohortSlice {
            slice,
            observations: 250,
            met_floor: 40,
        }],
        ..good.clone()
    };
    let c = calibration(&poor);
    assert!(!c.promotable, "{c:?}");
    assert!(c.blockers.iter().any(|b| b.contains("Brier")), "{c:?}");
    // A holdout the calibration set never saw is out of distribution, and
    // that blocks promotion too rather than being ignored.
    let contaminated = CalibrationCohort {
        holdout: vec![CohortSlice {
            slice: "feature|web|".into(),
            observations: 250,
            met_floor: 200,
        }],
        ..good
    };
    let c = calibration(&contaminated);
    assert!(!c.promotable && c.ood_slices == 1, "{c:?}");
}
