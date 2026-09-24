//! QUAL-EPR-019 / EPR-E2E-019 / EPR-FI-019 (REQ-EPR-019; docs/61): the real
//! verification engine, diff invariants, RealizedRisk rules and Acceptance
//! Gate, measured independently on the committed held-out corpora with
//! hidden oracles; the rates and every case pinned; the release check fails
//! without an approved profile and on an unsafe gate whatever the router
//! saves; a contaminated or mislabeled holdout is refused.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use modbit_bench_gate_calibration::{
    AcceptanceCorpus, Refusal, RiskCorpus, ThresholdProfile, calibrate, check_separation, metrics,
    release_check, run_acceptance_case, run_risk_case, tuning_lineages,
};

fn here() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf()
}

fn repo_root() -> PathBuf {
    here().join("../..").canonicalize().unwrap()
}

fn corpora() -> PathBuf {
    here().join("corpora")
}

fn acceptance() -> AcceptanceCorpus {
    serde_json::from_str(
        &std::fs::read_to_string(corpora().join("acceptance-holdout.json")).unwrap(),
    )
    .unwrap()
}

fn risk() -> RiskCorpus {
    serde_json::from_str(&std::fs::read_to_string(corpora().join("risk-holdout.json")).unwrap())
        .unwrap()
}

/// A profile for this test only: its numbers are fixtures, not approved
/// thresholds (docs/61: the owner approves those).
fn test_profile(limit: f64, min: u32) -> ThresholdProfile {
    ThresholdProfile {
        profile_id: "test-only".into(),
        approved_by: "test fixture (not an approval)".into(),
        approved_at: "2026-09-24".into(),
        method: "wilson-95-upper".into(),
        limits: modbit_bench_gate_calibration::METRICS
            .iter()
            .map(|m| ((*m).to_owned(), limit))
            .collect(),
        min_samples: modbit_bench_gate_calibration::METRICS
            .iter()
            .map(|m| ((*m).to_owned(), min))
            .collect(),
    }
}

#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn qual_epr_019_gates_are_measured_independently_on_the_holdout_and_an_unsafe_gate_fails_release()
 {
    let work = tempfile::tempdir().unwrap();
    let bundle = calibrate(&repo_root(), &corpora(), work.path(), None, 0)
        .await
        .expect("the holdout is held out and labelled");
    // Versions and digests pin what was measured.
    assert_eq!(bundle.suite_version, "gate-calibration/1");
    assert_eq!(bundle.gate_version, "gate-1");
    assert_eq!(bundle.risk_version, "risk-rules-2");
    for d in [
        &bundle.corpora.acceptance,
        &bundle.corpora.risk,
        &bundle.corpora.oracles,
        &bundle.corpora.tuning,
    ] {
        assert_eq!(d.len(), 64);
    }
    // Every acceptance case: the real gate's verdict and the hidden oracle's.
    let acc: BTreeMap<&str, (&str, bool)> = bundle
        .acceptance
        .iter()
        .map(|a| (a.id.as_str(), (a.verdict.as_str(), a.oracle_correct)))
        .collect();
    let expected_acc = [
        ("zero-correct", ("ACCEPT", true)),
        ("zero-overfitted", ("ACCEPT", false)),
        ("zero-breaks-format", ("REJECT", false)),
        ("zero-correct-escalated", ("ACCEPT", true)),
        ("max-correct", ("ACCEPT", true)),
        ("max-off-by-one", ("ACCEPT", false)),
        ("max-breaks-format", ("REJECT", false)),
        ("max-correct-escalated", ("ACCEPT", true)),
    ];
    assert_eq!(
        acc,
        expected_acc.into_iter().collect(),
        "{:#?}",
        bundle.acceptance
    );
    // The false accepts are the candidates the visible tests cannot tell
    // apart from correct ones: the gate's real error, measured.
    let fa: Vec<&str> = bundle
        .acceptance
        .iter()
        .filter(|a| a.false_accept())
        .map(|a| a.id.as_str())
        .collect();
    assert_eq!(fa, vec!["zero-overfitted", "max-off-by-one"]);
    // Every risk case against its oracle. `env-production` was a critical
    // miss under risk-rules-1 (`.env` matched only as a suffix); the rules
    // were fixed in risk-rules-2 (EPR-008 regression), so that case informed
    // a rule and no longer counts as independent evidence for the secret
    // surface.
    let misses: BTreeMap<&str, (bool, bool, bool)> = bundle
        .risk
        .iter()
        .filter(|r| r.false_negative || r.false_positive || r.critical_miss)
        .map(|r| {
            (
                r.id.as_str(),
                (r.false_negative, r.false_positive, r.critical_miss),
            )
        })
        .collect();
    assert_eq!(
        misses,
        [
            ("dependency-bump", (true, false, false)),
            ("payments-charge", (true, false, false)),
        ]
        .into_iter()
        .collect(),
        "{:#?}",
        bundle.risk
    );
    // The five rates, each on its own denominator.
    let rate = |name: &str| {
        let r = &bundle.metrics.rates[name];
        (r.count, r.n)
    };
    assert_eq!(rate("acceptance_false_accept_rate"), (2, 4));
    assert_eq!(rate("acceptance_false_reject_rate"), (0, 4));
    assert_eq!(rate("realized_risk_false_negative_rate"), (2, 13));
    assert_eq!(rate("realized_risk_false_positive_rate"), (0, 5));
    assert_eq!(rate("critical_surface_miss_rate"), (0, 6));
    for r in bundle.metrics.rates.values() {
        assert!(r.lower <= r.rate && r.rate <= r.upper, "{r:?}");
    }
    // EPR-FI-019: a critical miss is still measured when the rules make one.
    // The same held-out case under the rules without `.env.*` (risk-rules-1's
    // secret surface) is a false negative and a critical miss, and the rate
    // counts it.
    let mut before_fix = modbit_policy::AssurancePolicy::default();
    for s in &mut before_fix.protected_surfaces {
        s.patterns.retain(|p| p != ".env.*");
    }
    assert_ne!(before_fix, modbit_policy::AssurancePolicy::default());
    let env_production = risk()
        .cases
        .into_iter()
        .find(|c| c.id == "env-production")
        .unwrap();
    let missed = run_risk_case(&env_production, &before_fix);
    assert!(missed.false_negative && missed.critical_miss, "{missed:?}");
    let m = metrics(&[], &[missed]);
    let r = &m.rates["critical_surface_miss_rate"];
    assert_eq!((r.count, r.n), (1, 1));
    // Attribution per leg: an escalation's correct candidate never improves
    // the initial leg's record.
    let (initial_fa, _) = &bundle.metrics.per_leg["initial"];
    let (escalation_fa, _) = &bundle.metrics.per_leg["escalation"];
    assert_eq!((initial_fa.count, initial_fa.n), (2, 3));
    assert_eq!((escalation_fa.count, escalation_fa.n), (0, 1));

    // No approved profile: the release check fails.
    assert_eq!(bundle.release.verdict, "FAIL");
    assert!(
        bundle.release.reasons[0].starts_with("MISSING_THRESHOLD_PROFILE"),
        "{:?}",
        bundle.release
    );
    // A strict profile and a router that saves money: still an unsafe gate —
    // no critical miss was observed, but six cases cannot bound the rate
    // under the limit (Wilson upper bound 0.39).
    let strict = test_profile(0.20, 30);
    let check = release_check(&bundle.metrics, Some((&strict, "digest")), -5_000);
    assert_eq!(check.verdict, "FAIL");
    assert_eq!(check.router_cost_delta_minor, -5_000);
    for needle in [
        "UNSAFE_GATE: acceptance_false_accept_rate",
        "UNSAFE_GATE: critical_surface_miss_rate",
        "INSUFFICIENT_SAMPLES: acceptance_false_accept_rate",
    ] {
        assert!(
            check.reasons.iter().any(|r| r.starts_with(needle)),
            "{needle}: {check:?}"
        );
    }
    // A profile that bounds nothing it should is incomplete, not permissive.
    let mut partial = test_profile(1.0, 1);
    partial.limits.remove("critical_surface_miss_rate");
    let check = release_check(&bundle.metrics, Some((&partial, "digest")), 0);
    assert!(
        check
            .reasons
            .iter()
            .any(|r| r == "MISSING_THRESHOLD: critical_surface_miss_rate"),
        "{check:?}"
    );
    // The check can pass: every rate under a (test) limit with its samples.
    let permissive = test_profile(1.0, 1);
    let check = release_check(&bundle.metrics, Some((&permissive, "digest")), 0);
    assert_eq!(check.verdict, "PASS", "{check:?}");
}

#[test]
fn a_contaminated_holdout_is_refused() {
    let tuning = tuning_lineages(&repo_root());
    assert!(tuning.contains("rust-cli/reject-negative"), "{tuning:?}");
    assert!(check_separation(&acceptance(), &risk(), &tuning).is_ok());
    // A holdout case that shares a lineage with the competence suite.
    let mut acc = acceptance();
    acc.cases[0].lineage = "rust-cli/reject-negative".into();
    assert!(matches!(
        check_separation(&acc, &risk(), &tuning),
        Err(Refusal::Contaminated { .. })
    ));
    // The two holdouts share no lineage either.
    let mut r = risk();
    r.cases[0].lineage = acceptance().cases[0].lineage.clone();
    assert!(matches!(
        check_separation(&acceptance(), &r, &tuning),
        Err(Refusal::SharedLineage { .. })
    ));
}

#[tokio::test]
async fn a_mislabeled_case_is_refused_by_its_oracle() {
    let work = tempfile::tempdir().unwrap();
    let mut case = acceptance()
        .cases
        .into_iter()
        .find(|c| c.id == "zero-overfitted")
        .unwrap();
    // Declared correct, but the hidden oracle rejects it.
    case.declared = "correct".into();
    let policy = modbit_policy::AssurancePolicy::default();
    let r = run_acceptance_case(&case, &repo_root(), &corpora(), work.path(), &policy).await;
    assert!(
        matches!(&r, Err(Refusal::Mislabeled { observed, .. }) if observed == "incorrect"),
        "{r:?}"
    );
}
