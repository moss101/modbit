//! QUAL-EPR-008 / EPR-FI-008 over the rule set itself: every case of the
//! fault corpus (`fixtures/assurance/corpus.json`) is derived under the
//! base policy and compared with the pinned `expected.json` — a mutation of
//! any rule, surface pattern or threshold fails here until the pin is
//! regenerated on purpose (`MODBIT_UPDATE_ASSURANCE_CORPUS=1`). The
//! invariants hold across the corpus: a critical surface always requires a
//! human; no advisory signal changes a diagnosis; a layer only strengthens;
//! the digest binds the facts and not the advisory.

use modbit_policy::{
    Advisory, AssuranceLayer, AssuranceLevel, AssurancePolicy, CandidateFacts, RiskLevel,
    derive_realized_risk, strengthen_only,
};
use serde::Deserialize;

#[derive(Deserialize)]
struct Case {
    name: String,
    facts: CandidateFacts,
}

fn fixture(name: &str) -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/assurance")
        .join(name)
}

fn corpus() -> Vec<Case> {
    serde_json::from_str(&std::fs::read_to_string(fixture("corpus.json")).unwrap()).unwrap()
}

#[test]
fn qual_epr_008_fault_corpus_pins_every_rule_and_no_signal_lowers_assurance() {
    let policy = AssurancePolicy::default();
    let cases = corpus();
    assert!(cases.len() >= 20);
    let actual: serde_json::Map<String, serde_json::Value> = cases
        .iter()
        .map(|c| {
            (
                c.name.clone(),
                serde_json::to_value(derive_realized_risk(&policy, &c.facts)).unwrap(),
            )
        })
        .collect();
    let expected_path = fixture("expected.json");
    if std::env::var_os("MODBIT_UPDATE_ASSURANCE_CORPUS").is_some() {
        std::fs::write(
            &expected_path,
            serde_json::to_string_pretty(&serde_json::Value::Object(actual.clone())).unwrap()
                + "\n",
        )
        .unwrap();
    }
    let expected: serde_json::Map<String, serde_json::Value> =
        serde_json::from_str(&std::fs::read_to_string(&expected_path).unwrap()).unwrap();
    for c in &cases {
        assert_eq!(
            actual[&c.name], expected[&c.name],
            "the diagnosis of `{}` moved; a rule, a surface or a threshold changed — regenerate the pin only on purpose",
            c.name
        );
    }
    assert_eq!(actual.len(), expected.len());

    for c in &cases {
        let r = derive_realized_risk(&policy, &c.facts);
        // The policy minimum is a floor.
        assert!(
            r.minimum_assurance >= policy.minimum_assurance,
            "{}",
            c.name
        );
        // A critical level always needs a human; a human always needs review-grade assurance.
        if r.level == RiskLevel::Critical {
            assert!(r.human_required, "{}", c.name);
            assert_eq!(
                r.minimum_assurance,
                AssuranceLevel::HighAssurance,
                "{}",
                c.name
            );
        }
        if r.human_required {
            assert_eq!(
                r.minimum_assurance,
                AssuranceLevel::HighAssurance,
                "{}",
                c.name
            );
        }
        if r.independent_review_required {
            assert!(
                r.minimum_assurance >= AssuranceLevel::Governed,
                "{}",
                c.name
            );
        }
        // Forged signals change nothing: same level, obligations, reasons, digest.
        for adv in [
            Advisory {
                learned_risk: Some(0.0),
                confidence: Some(1.0),
                checks_passed: Some(true),
            },
            Advisory {
                learned_risk: Some(1.0),
                confidence: Some(0.0),
                checks_passed: Some(false),
            },
        ] {
            let mut forged = c.facts.clone();
            forged.advisory = adv;
            let f = derive_realized_risk(&policy, &forged);
            assert_eq!(
                (
                    f.level,
                    f.minimum_assurance,
                    f.independent_review_required,
                    f.human_required
                ),
                (
                    r.level,
                    r.minimum_assurance,
                    r.independent_review_required,
                    r.human_required
                ),
                "{}: an advisory signal moved the diagnosis",
                c.name
            );
            assert_eq!(f.reasons, r.reasons, "{}", c.name);
            assert_eq!(f.facts_digest, r.facts_digest, "{}", c.name);
        }
        // Post-draft facts only strengthen.
        let benign = derive_realized_risk(
            &policy,
            &CandidateFacts {
                candidate_revision: c.facts.candidate_revision + 1,
                changed: vec![],
                plan_write_set: None,
                requested_effects: vec![],
                advisory: Advisory::default(),
            },
        );
        let joined = strengthen_only(&r, &benign);
        assert!(
            joined.level >= r.level && joined.minimum_assurance >= r.minimum_assurance,
            "{}",
            c.name
        );
        assert!(joined.human_required >= r.human_required, "{}", c.name);
        // A weakening layer is ignored; the result is identical.
        let (weak, ignored) = policy.strengthen_with(&AssuranceLayer {
            minimum_assurance: Some(AssuranceLevel::Fast),
            human_required_at: Some(RiskLevel::Critical),
            review_required_at: Some(RiskLevel::Critical),
            blast_radius: Some(modbit_policy::assurance::BlastRadius {
                medium_files: 1000,
                high_files: 1000,
                high_lines: 1_000_000,
            }),
            ..Default::default()
        });
        assert!(!ignored.is_empty());
        assert_eq!(weak, policy, "{}", c.name);
    }
}

#[test]
fn epr_fi_008_a_stronger_layer_moves_every_case_up_or_keeps_it() {
    let base = AssurancePolicy::default();
    let (strict, ignored) = base.strengthen_with(&AssuranceLayer {
        minimum_assurance: Some(AssuranceLevel::Governed),
        review_required_at: Some(RiskLevel::Medium),
        human_required_at: Some(RiskLevel::High),
        ..Default::default()
    });
    assert!(ignored.is_empty());
    for c in corpus() {
        let a = derive_realized_risk(&base, &c.facts);
        let b = derive_realized_risk(&strict, &c.facts);
        assert!(b.minimum_assurance >= a.minimum_assurance, "{}", c.name);
        assert!(
            b.independent_review_required >= a.independent_review_required,
            "{}",
            c.name
        );
        assert!(b.human_required >= a.human_required, "{}", c.name);
        assert_eq!(
            b.level, a.level,
            "{}: the level is the facts', the layer moves obligations",
            c.name
        );
        assert_ne!(b.policy_version, a.policy_version);
    }
}
