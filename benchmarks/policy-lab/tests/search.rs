//! The Policy Lab's search rules (REQ-EPR-012) on materialized snapshots.
//! The evaluator here is a stand-in for the router's compiler — two
//! bindings, the cheaper selected when its lower bound clears the floor —
//! so these tests pin the search's own rules; the Core's qualification runs
//! the same search through the real compiler.

use modbit_bench_outcome_statistics::{Materialization, Sample, Snapshot, StatKey, materialize};
use modbit_bench_policy_lab::{
    Evaluation, FEASIBLE, HOLDOUT_REGRESSION, NO_FEASIBLE_CONFIGURATION, RequestShape, Spec,
    search, verify,
};

fn snapshot(version: &str, source: &str, cheap: (u32, u32), dear: (u32, u32)) -> Snapshot {
    let mut samples = Vec::new();
    for (model, (ok, n)) in [("cheap", cheap), ("dear", dear)] {
        for i in 0..n {
            samples.push(Sample {
                outcome_id: format!("{source}:{model}:{i}"),
                key: StatKey::Solver {
                    model: model.into(),
                    skill: "none".into(),
                    harness: "h".into(),
                },
                success: i < ok,
                cost_minor: None,
                wall_ms: 10,
                attributed_to: source.into(),
            });
        }
    }
    materialize(
        &Materialization {
            stats_version: version,
            tenant_id: "t",
            source_versions: vec![],
            created_at_ms: 0,
            declared_samples: None,
        },
        &samples,
    )
    .unwrap()
}

/// Cheapest binding whose lower bound clears the floor, as the compiler
/// would select it; the dearer one costs 10x.
fn evaluator(floor: f64, s: &Snapshot) -> Evaluation {
    let lcb = |m: &str| {
        s.aggregates
            .iter()
            .find(|a| a.key_id.contains(m))
            .filter(|a| !a.low_confidence)
            .map_or(0.0, |a| a.interval.0)
    };
    for (model, cost) in [("cheap", 10), ("dear", 100)] {
        if lcb(model) >= floor {
            return Evaluation {
                floor,
                code: FEASIBLE.into(),
                selected: vec![format!("openai/{model}")],
                selected_lcb: lcb(model),
                expected_cost_minor: cost,
                target_met: true,
            };
        }
    }
    Evaluation {
        floor,
        code: "QUALITY_FLOOR_INFEASIBLE".into(),
        selected: vec!["openai/dear".into()],
        selected_lcb: lcb("dear"),
        expected_cost_minor: 100,
        target_met: false,
    }
}

fn spec(target: f64, floors: &[f64]) -> Spec {
    Spec {
        candidate_generation: "policy-b".into(),
        target,
        floors: floors.to_vec(),
        train_stats_version: "train".into(),
        holdout_stats_version: "holdout".into(),
        request: RequestShape::default(),
    }
}

#[test]
fn stage_a_then_stage_b_then_the_holdout_confirms() {
    // cheap: 38/40 (lcb ~0.83), dear: 40/40 (lcb ~0.91).
    let train = snapshot("train", "bundle-a", (38, 40), (40, 40));
    let holdout = snapshot("holdout", "bundle-b", (39, 40), (40, 40));
    // At a 0.9 target only the dear plan reaches it; floors that select the
    // cheap plan fail Stage A even though the compiler calls them feasible.
    let r = search(&spec(0.9, &[0.5, 0.8, 0.9]), &train, &holdout, evaluator).unwrap();
    assert_eq!(r.verdict, FEASIBLE, "{r:#?}");
    assert_eq!(r.feasible, vec![0.9]);
    assert_eq!(
        r.chosen.as_ref().unwrap().selected,
        vec!["openai/dear".to_owned()]
    );
    assert!(verify(&r));
    // At a 0.8 target both reach it and Stage B takes the cheaper, the
    // higher floor breaking a tie.
    let r = search(&spec(0.8, &[0.5, 0.8, 0.9]), &train, &holdout, evaluator).unwrap();
    assert_eq!(r.verdict, FEASIBLE, "{r:#?}");
    let c = r.chosen.unwrap();
    assert_eq!(
        (c.floor, c.selected),
        (0.8, vec!["openai/cheap".to_owned()])
    );
}

#[test]
fn a_configuration_that_does_not_hold_on_the_holdout_is_not_promotable() {
    let train = snapshot("train", "bundle-a", (38, 40), (40, 40));
    // On the holdout the cheap plan regresses (30/40, lcb ~0.6).
    let holdout = snapshot("holdout", "bundle-b", (30, 40), (40, 40));
    let r = search(&spec(0.8, &[0.8]), &train, &holdout, evaluator).unwrap();
    assert_eq!(r.verdict, HOLDOUT_REGRESSION, "{r:#?}");
    assert!(
        r.reasons.iter().any(|x| x.starts_with("holdout:")),
        "{r:#?}"
    );
}

#[test]
fn thin_or_absent_evidence_is_no_feasible_configuration() {
    let train = snapshot("train", "bundle-a", (3, 3), (0, 0));
    let holdout = snapshot("holdout", "bundle-b", (3, 3), (0, 0));
    let r = search(&spec(0.5, &[0.5, 0.7]), &train, &holdout, evaluator).unwrap();
    assert_eq!(r.verdict, NO_FEASIBLE_CONFIGURATION, "{r:#?}");
    assert!(r.chosen.is_none() && r.holdout_check.is_none());
}

#[test]
fn the_holdout_is_untouched_and_the_spec_is_bounded() {
    let train = snapshot("train", "bundle-a", (38, 40), (40, 40));
    let shared = snapshot("holdout", "bundle-a", (38, 40), (40, 40));
    let e = search(&spec(0.8, &[0.8]), &train, &shared, evaluator).unwrap_err();
    assert_eq!(e.code(), "SEARCH_PARTITION_INVALID", "{e:?}");
    let other = snapshot("holdout", "bundle-b", (38, 40), (40, 40));
    // The pinned versions are the ones searched.
    let mut s = spec(0.8, &[0.8]);
    s.train_stats_version = "train-2".into();
    assert_eq!(
        search(&s, &train, &other, evaluator).unwrap_err().code(),
        "SEARCH_PARTITION_INVALID"
    );
    for bad in [
        spec(0.0, &[0.8]),
        spec(0.8, &[]),
        spec(0.8, &[0.8, 0.8]),
        spec(0.8, &[1.5]),
        spec(
            0.8,
            &(0..40).map(|i| f64::from(i) / 40.0).collect::<Vec<_>>(),
        ),
    ] {
        assert_eq!(
            search(&bad, &train, &other, evaluator).unwrap_err().code(),
            "SEARCH_SPEC_INVALID",
            "{bad:?}"
        );
    }
}

#[test]
fn a_tampered_report_does_not_verify() {
    let train = snapshot("train", "bundle-a", (38, 40), (40, 40));
    let holdout = snapshot("holdout", "bundle-b", (39, 40), (40, 40));
    let mut r = search(&spec(0.8, &[0.8, 0.9]), &train, &holdout, evaluator).unwrap();
    assert!(verify(&r));
    r.chosen.as_mut().unwrap().floor = 0.5;
    assert!(!verify(&r));
}
