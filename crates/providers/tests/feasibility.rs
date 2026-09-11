//! REQ-EPR-016 / EPR-E2E-016 / EPR-FI-016: a plan qualifies on the lower
//! bound of what was observed and only with enough observations; the cheapest
//! feasible plan wins; an empty feasible set says so exactly; and the
//! mutations that would make selection unsafe are visible as such.

use modbit_providers::feasibility::{
    Candidate, LegEvidence, PlanQuality, Thresholds, plan_quality, select,
};

fn thresholds() -> Thresholds {
    Thresholds {
        mode: "auto".into(),
        tau: 0.72,
        delta: 0.05,
        min_samples: 30,
        switch_cost_minor: 100,
        thresholds_version: "thresholds-1".into(),
    }
}

fn leg(key: &str, lcb: f64, mean: f64, samples: u32) -> LegEvidence {
    LegEvidence {
        key_id: key.into(),
        lcb,
        mean,
        samples,
    }
}

fn candidate(id: &str, cost: u64, quality: PlanQuality) -> Candidate {
    Candidate {
        plan_id: id.into(),
        worst_case_cost_minor: cost,
        expected_cost_minor: cost,
        quality,
        hard_eligible: true,
        ineligible_reason: String::new(),
    }
}

fn quality_of(initial: &LegEvidence) -> PlanQuality {
    plan_quality(Some(initial), None, &initial.key_id, None, &thresholds())
}

#[test]
fn a_high_mean_with_a_weak_bound_fails_cheap_eligibility() {
    let t = thresholds();
    // The cheap plan: three for three. A mean of one, and a bound near zero.
    let cheap = candidate("cheap", 300, quality_of(&leg("solver|mini", 0.29, 1.0, 3)));
    // The dear plan: well observed, comfortably above the floor.
    let dear = candidate(
        "dear",
        1_200,
        quality_of(&leg("solver|big", 0.81, 0.88, 140)),
    );
    let s = select(&[cheap.clone(), dear.clone()], &t, None);
    assert_eq!(s.selected.as_deref(), Some("dear"), "{s:?}");
    assert_eq!(s.code, "FEASIBLE");
    assert!(s.target_met);
    assert_eq!(s.feasible, vec!["dear".to_owned()]);
    let why = s
        .exclusions
        .iter()
        .find(|e| e.plan_id == "cheap")
        .map(|e| e.reason.clone())
        .unwrap_or_default();
    assert!(
        why.contains("low confidence") && why.contains("30-sample"),
        "{why}"
    );
    // EPR-FI-016: a mean-only selector would have taken the cheap plan. The
    // mutation is visible: it is the choice this selector refuses to make.
    let mean_only = [&cheap, &dear]
        .iter()
        .filter(|c| c.quality.mean >= t.tau)
        .min_by_key(|c| c.worst_case_cost_minor)
        .map(|c| c.plan_id.clone());
    assert_eq!(mean_only.as_deref(), Some("cheap"), "the unsafe choice");
    assert_ne!(s.selected, mean_only);
    // EPR-FI-016: lowering the sample threshold to nothing makes the cheap
    // plan "feasible" on its perfect mean — with a bound of 0.29 it still
    // fails tau, which is the point of qualifying on the bound.
    let mut lax = t.clone();
    lax.min_samples = 1;
    let s = select(&[cheap.clone(), dear.clone()], &lax, None);
    assert_eq!(s.selected.as_deref(), Some("dear"), "{s:?}");
    // And with enough observations behind the same bound, it is still the
    // bound that decides: a cheap plan at 0.75 over 60 observations does win.
    let cheap_and_good = candidate(
        "cheap",
        300,
        quality_of(&leg("solver|mini", 0.75, 0.84, 60)),
    );
    let s = select(&[cheap_and_good, dear], &t, None);
    assert_eq!(s.selected.as_deref(), Some("cheap"), "{s:?}");
}

#[test]
fn the_minimum_cost_feasible_plan_wins_and_switch_cost_stops_flapping() {
    let t = thresholds();
    let a = candidate("a", 900, quality_of(&leg("solver|a", 0.80, 0.85, 90)));
    let b = candidate("b", 850, quality_of(&leg("solver|b", 0.74, 0.80, 200)));
    let c = candidate("c", 2_000, quality_of(&leg("solver|c", 0.95, 0.97, 400)));
    let s = select(&[a.clone(), b.clone(), c.clone()], &t, None);
    assert_eq!(s.selected.as_deref(), Some("b"), "cheapest feasible: {s:?}");
    assert_eq!(
        s.feasible,
        vec!["b".to_owned(), "a".to_owned(), "c".to_owned()]
    );
    assert!(
        s.exclusions
            .iter()
            .any(|e| e.plan_id == "c" && e.reason.contains("not the lowest expected complete cost"))
    );
    // With `a` in force, `b` saves 50, below the switch cost of 100: `a` is
    // kept rather than flapping to `b` for a marginal saving.
    let s = select(&[a.clone(), b.clone(), c.clone()], &t, Some("a"));
    assert_eq!(s.selected.as_deref(), Some("a"), "{s:?}");
    assert!(
        s.exclusions
            .iter()
            .any(|e| e.plan_id == "b" && e.reason.contains("switch cost")),
        "{s:?}"
    );
    // EPR-FI-016: with the switch cost mutated to zero, the same inputs flap.
    let mut none = t.clone();
    none.switch_cost_minor = 0;
    assert_eq!(
        select(&[a.clone(), b.clone(), c.clone()], &none, Some("a"))
            .selected
            .as_deref(),
        Some("b")
    );
    // A plan in force that is no longer feasible is not kept for hysteresis.
    let a_worse = candidate("a", 900, quality_of(&leg("solver|a", 0.60, 0.70, 90)));
    assert_eq!(
        select(&[a_worse, b, c], &t, Some("a")).selected.as_deref(),
        Some("b")
    );
}

#[test]
fn an_empty_feasible_set_is_said_exactly_and_never_claims_the_target() {
    let t = thresholds();
    let near = candidate("near", 500, quality_of(&leg("solver|near", 0.69, 0.78, 80)));
    let far = candidate("far", 400, quality_of(&leg("solver|far", 0.40, 0.52, 120)));
    let thin = candidate("thin", 200, quality_of(&leg("solver|thin", 0.10, 1.0, 2)));
    let s = select(&[near.clone(), far.clone(), thin.clone()], &t, None);
    assert_eq!(s.code, "QUALITY_FLOOR_INFEASIBLE");
    assert!(!s.target_met, "the fallback never claims the target");
    assert!(s.feasible.is_empty());
    // The best hard-eligible plan by its bound, not the cheapest.
    assert_eq!(s.selected.as_deref(), Some("near"), "{s:?}");
    assert!((s.selected_lcb - 0.69).abs() < 1e-9);
    let reason = |id: &str| {
        s.exclusions
            .iter()
            .find(|e| e.plan_id == id)
            .map(|e| e.reason.clone())
            .unwrap_or_default()
    };
    assert!(reason("near").starts_with("near:"), "{}", reason("near"));
    assert!(
        reason("far").contains("below tau 0.720"),
        "{}",
        reason("far")
    );
    assert!(
        reason("thin").contains("low confidence"),
        "{}",
        reason("thin")
    );
    // EPR-FI-016: missing data is infeasible, never a fallback beyond the
    // eligible set.
    let unobserved = candidate(
        "unobserved",
        100,
        plan_quality(None, None, "solver|new", None, &t),
    );
    let s = select(&[unobserved], &t, None);
    assert_eq!(s.code, "QUALITY_FLOOR_INFEASIBLE");
    assert!(
        s.exclusions[0]
            .reason
            .contains("no observation for solver|new"),
        "{s:?}"
    );
    assert!(!s.target_met);
    // A plan outside the hard-eligible set is never selected, whatever its
    // bound: the hard-empty set fails.
    let mut over_budget = candidate("over", 50, quality_of(&leg("solver|over", 0.99, 0.99, 500)));
    over_budget.hard_eligible = false;
    over_budget.ineligible_reason = "worst-case cost 5000 exceeds the request cap 1000".into();
    let s = select(&[over_budget], &t, None);
    assert_eq!(s.code, "NO_HARD_ELIGIBLE_PLAN");
    assert_eq!(s.selected, None);
    assert!(
        s.exclusions[0].reason.contains("not hard eligible"),
        "{s:?}"
    );
}

#[test]
fn a_continuation_adds_only_what_its_own_observations_support() {
    let t = thresholds();
    let initial = leg("solver|mini", 0.60, 0.70, 100);
    // No observed escalations: the plan is worth exactly its initial leg, and
    // the joint key is reported missing rather than guessed.
    let alone = plan_quality(
        Some(&initial),
        None,
        "solver|mini",
        Some("escalation|mini|big|gate|repo|cargo"),
        &t,
    );
    assert!((alone.lcb - 0.60).abs() < 1e-9);
    assert_eq!(
        alone.missing,
        vec!["escalation|mini|big|gate|repo|cargo".to_owned()]
    );
    // A thin escalation record contributes nothing either.
    let thin = leg("escalation|mini|big|gate|repo|cargo", 0.5, 0.9, 4);
    let with_thin = plan_quality(Some(&initial), Some(&thin), "solver|mini", None, &t);
    assert!((with_thin.lcb - 0.60).abs() < 1e-9);
    assert_eq!(with_thin.thin, vec![thin.key_id.clone()]);
    // A well-observed escalation composes: the plan succeeds unless both
    // legs fail, and the bound composes the bounds.
    let solid = leg("escalation|mini|big|gate|repo|cargo", 0.50, 0.65, 60);
    let with_solid = plan_quality(Some(&initial), Some(&solid), "solver|mini", None, &t);
    assert!((with_solid.lcb - 0.80).abs() < 1e-9, "{with_solid:?}");
    assert!(with_solid.confident);
    // Nothing subtracts: a continuation never makes a plan worse than its
    // initial leg.
    let bad = leg("escalation|mini|big|gate|repo|cargo", 0.0, 0.1, 60);
    let with_bad = plan_quality(Some(&initial), Some(&bad), "solver|mini", None, &t);
    assert!(with_bad.lcb >= 0.60 - 1e-9);
}
