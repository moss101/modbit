//! REQ-EPR-004 / EPR-E2E-004 / EPR-FI-004: the compiler enumerates bounded
//! plans from the registry, prevalidates every one before anything
//! dispatches, chooses by feasibility then cost, and refuses what cannot be
//! compiled — with a golden corpus of requested and resolved configurations
//! and every exclusion reason.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use ed25519_dalek::{Signer, SigningKey};
use modbit_domain::routing::{Money, PlanScope, Trigger};
use modbit_domain::{RunId, SessionId, TaskId, TenantId};
use modbit_providers::compiler::{CompileInput, CompileRefused, Evidence, compile};
use modbit_providers::feasibility::{LegEvidence, Thresholds};
use modbit_providers::registry::{
    Economics, Governance, Latency, ModelRegistry, Needs, QualityFloor, REGISTRY_SCHEMA_VERSION,
    RegistryDocument, RegistryEntry, SignedRegistry, activate,
};

const NOW: i64 = 1_800_000_000_000;

fn entry(model: &str, roles: &[&str], input_price: u64, output_price: u64) -> RegistryEntry {
    RegistryEntry {
        endpoint: "openai".into(),
        provider: "openai".into(),
        family: "gpt-5".into(),
        model: model.into(),
        roles: roles.iter().map(|r| (*r).to_owned()).collect(),
        input_modalities: vec!["text".into()],
        context_tokens: 400_000,
        max_output_tokens: 64_000,
        tools: true,
        vision: false,
        reasoning: true,
        structured_output: true,
        economics: Economics {
            input_per_mtok_minor: input_price,
            output_per_mtok_minor: output_price,
            currency: "USD".into(),
            scale: 2,
            cached_input_per_mtok_minor: None,
            cache_write_per_mtok_minor: None,
            cache_ttl_ms: None,
        },
        latency: Latency {
            p50_ms: 900,
            p95_ms: 4_200,
        },
        governance: Governance {
            data_residency: "us".into(),
            retains_prompts: false,
            allowed_profiles: vec![],
        },
        revoked: false,
        fallbacks: vec![],
    }
}

fn registry_with(entries: Vec<RegistryEntry>) -> ModelRegistry {
    let key = SigningKey::from_bytes(&[3u8; 32]);
    let doc = RegistryDocument {
        schema_version: REGISTRY_SCHEMA_VERSION,
        registry_generation: "registry-compile-1".into(),
        stats_version: "stats-7".into(),
        issued_at_ms: NOW - 1_000,
        expires_at_ms: NOW + 86_400_000,
        quality_floors: vec![QualityFloor {
            mode: "auto".into(),
            min_quality: 0.72,
            max_cost_minor: 5_000,
            currency: "USD".into(),
            scale: 2,
        }],
        entries,
    };
    let json = serde_json::to_string(&doc).unwrap();
    let signed = SignedRegistry {
        key_id: "ops".into(),
        signature_hex: hex::encode(key.sign(json.as_bytes()).to_bytes()),
        document_json: json,
    };
    let mut trusted = BTreeMap::new();
    trusted.insert("ops".to_owned(), key.verifying_key().to_bytes());
    activate(&signed, &trusted, NOW).expect("activated")
}

fn registry() -> ModelRegistry {
    registry_with(vec![
        entry("gpt-5-mini", &["solver"], 25, 200),
        entry("gpt-5", &["solver", "reviewer"], 125, 1_000),
        entry("gpt-5-pro", &["solver", "reviewer"], 1_500, 12_000),
    ])
}

fn usd(minor: u64) -> Money {
    Money {
        minor_units: minor,
        currency: "USD".into(),
        scale: 2,
    }
}

fn thresholds() -> Thresholds {
    Thresholds {
        mode: "auto".into(),
        tau: 0.72,
        delta: 0.05,
        min_samples: 30,
        switch_cost_minor: 0,
        thresholds_version: "registry-compile-1".into(),
    }
}

fn scope() -> PlanScope {
    PlanScope {
        tenant_id: TenantId::from_bytes([1; 16]),
        session_id: SessionId::from_bytes([2; 16]),
        task_id: TaskId::from_bytes([3; 16]),
        run_id: RunId::from_bytes([4; 16]),
        created_at_ms: NOW,
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

fn input<'a>(
    registry: &'a ModelRegistry,
    evidence: &'a Evidence,
    thresholds: &'a Thresholds,
) -> CompileInput<'a> {
    CompileInput {
        registry,
        evidence,
        thresholds,
        scope: scope(),
        lease_generation: 3,
        routing_epoch: 1,
        needs: Needs {
            tools: true,
            ..Needs::default()
        },
        execution_profile: "local_trusted".into(),
        allowed_residencies: vec![],
        request_cap: usd(5_000),
        verification_reserve: usd(100),
        assurance_available: true,
        manual_pin: None,
        harness: "build-1".into(),
        policy_version: "policy-1".into(),
        profiler_version: "none".into(),
        gate_version: "gate-1".into(),
        risk_version: "risk-1".into(),
        expected_input_tokens: 40_000,
        current_binding: None,
        include_reviewer: false,
        allowed_models: None,
    }
}

fn no_evidence() -> Evidence {
    Evidence {
        stats_version: "stats-7".into(),
        legs: vec![],
    }
}

fn golden_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/routing/golden")
        .canonicalize()
        .unwrap_or_else(|_| {
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/routing/golden")
        })
}

/// The requested and resolved configuration of a compile, in the shape the
/// golden corpus records.
fn resolved(c: &modbit_providers::compiler::Compiled) -> serde_json::Value {
    serde_json::json!({
        "selected": c.plan.plan_id,
        "code": c.selection.code,
        "target_met": c.selection.target_met,
        "slots": c.plan.slots.iter().map(|s| format!("{}:{}/{}:{:?}", s.slot_id, s.endpoint, s.model, s.trigger)).collect::<Vec<_>>(),
        "total_budget_minor": c.plan.total_budget.minor_units,
        "provenance": {
            "registry_generation": c.plan.provenance.registry_generation,
            "statistics_version": c.plan.provenance.statistics_version,
            "compiler_version": c.plan.provenance.compiler_version,
        },
        "candidates": c.candidates.iter().map(|k| serde_json::json!({
            "plan_id": k.plan_id,
            "bindings": k.bindings,
            "worst_case_cost_minor": k.worst_case_cost_minor,
            "expected_cost_minor": k.expected_cost_minor,
            "hard_eligible": k.hard_eligible,
            "ineligible_reason": k.ineligible_reason,
            "lcb": k.quality.lcb,
            "confident": k.quality.confident,
        })).collect::<Vec<_>>(),
        "exclusions": c.selection.exclusions.iter().map(|e| format!("{}: {}", e.plan_id, e.reason)).collect::<Vec<_>>(),
    })
}

fn check_golden(name: &str, actual: &serde_json::Value) {
    let path = golden_dir().join(format!("{name}.json"));
    if std::env::var("MODBIT_UPDATE_GOLDEN").is_ok() {
        std::fs::create_dir_all(golden_dir()).unwrap();
        std::fs::write(&path, serde_json::to_string_pretty(actual).unwrap()).unwrap();
    }
    let expected: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&path).unwrap_or_else(|e| {
            panic!(
                "golden {} missing ({e}); run with MODBIT_UPDATE_GOLDEN=1",
                path.display()
            )
        }))
        .unwrap();
    assert_eq!(
        actual, &expected,
        "golden {name} differs from the compiler's answer (MODBIT_UPDATE_GOLDEN=1 to record a deliberate change)"
    );
}

#[test]
fn identical_inputs_produce_identical_plans_and_the_golden_cold_start() {
    let r = registry();
    let e = no_evidence();
    let t = thresholds();
    let a = compile(&input(&r, &e, &t)).expect("compiled");
    let b = compile(&input(&r, &e, &t)).expect("compiled");
    assert_eq!(a.plan.plan_id, b.plan.plan_id);
    assert_eq!(a.plan.content_digest, b.plan.content_digest);
    assert_eq!(a.input_digest, b.input_digest);
    assert_eq!(a.candidates, b.candidates);
    // Cold start: nothing has evidence, so nothing is feasible and the best
    // hard-eligible plan runs with the target not claimed. Every candidate
    // was enumerated from parameters and prevalidated first.
    assert_eq!(a.selection.code, "QUALITY_FLOOR_INFEASIBLE");
    assert!(!a.selection.target_met);
    assert!(a.candidates.len() >= 5, "{:?}", a.candidates);
    assert!(
        a.candidates.iter().all(|c| c.hard_eligible),
        "{:?}",
        a.candidates
    );
    assert_eq!(a.plan.provenance.compiler_version, "compiler-2");
    assert_eq!(a.plan.provenance.registry_generation, "registry-compile-1");
    assert_eq!(a.plan.provenance.statistics_version, "stats-7");
    assert_eq!(a.plan.validate(scope().tenant_id, 1), Ok(()));
    check_golden("cold_start", &resolved(&a));
}

#[test]
fn feasibility_then_lowest_expected_complete_cost_decides() {
    let r = registry();
    // The mini solver is well observed but below the floor; the mid solver
    // clears it with confidence; the pro solver clears it too and costs more;
    // and the mini -> mid escalation has real support.
    let e = Evidence {
        stats_version: "stats-7".into(),
        legs: vec![
            leg("solver|gpt-5-mini|none|build-1", 0.55, 0.62, 200),
            leg("solver|gpt-5|none|build-1", 0.80, 0.86, 150),
            leg("solver|gpt-5-pro|none|build-1", 0.93, 0.96, 90),
            leg(
                "escalation|gpt-5-mini|gpt-5|acceptance|workspace|configured",
                0.60,
                0.70,
                80,
            ),
        ],
    };
    let t = thresholds();
    let c = compile(&input(&r, &e, &t)).expect("compiled");
    assert_eq!(c.selection.code, "FEASIBLE", "{:?}", c.selection);
    assert!(c.selection.target_met);
    // The cheapest feasible plan is the mini opener with the mid
    // continuation: 1 - (1-0.55)(1-0.60) = 0.82 clears the floor, and its
    // expected complete cost — the mini leg, plus the mid leg weighted by
    // the 45% of the time the mini leg's bound says it is rejected — is
    // below a plain mid plan's, even though its worst case is above it.
    let chosen = c
        .candidates
        .iter()
        .find(|k| k.plan_id == c.plan.plan_id)
        .unwrap();
    assert_eq!(
        chosen.bindings,
        vec!["openai/gpt-5-mini".to_owned(), "openai/gpt-5".to_owned()]
    );
    assert_eq!(c.plan.slots.len(), 2);
    assert_eq!(c.plan.slots[1].trigger, Trigger::QualityRejected);
    let plain_mid = c
        .candidates
        .iter()
        .find(|k| k.bindings == vec!["openai/gpt-5".to_owned()])
        .unwrap();
    assert!(chosen.worst_case_cost_minor > plain_mid.worst_case_cost_minor);
    assert!(chosen.expected_cost_minor < plain_mid.expected_cost_minor);
    let feasible: Vec<&_> = c
        .candidates
        .iter()
        .filter(|k| c.selection.feasible.contains(&k.plan_id))
        .collect();
    assert!(feasible.len() >= 2, "{feasible:?}");
    assert!(
        feasible
            .iter()
            .all(|k| k.expected_cost_minor >= chosen.expected_cost_minor),
        "the selection is the cheapest feasible by expected cost: {feasible:?}"
    );
    assert!(chosen.quality.lcb >= 0.72);
    check_golden("feasible_selection", &resolved(&c));
    // A manual pin keeps policy: only plans opened by the pin are considered,
    // and the pin's own eligibility still applies.
    let mut pinned = input(&r, &e, &t);
    pinned.manual_pin = Some(("openai".into(), "gpt-5-pro".into()));
    let c = compile(&pinned).expect("compiled");
    assert_eq!(c.plan.slots[0].model, "gpt-5-pro");
    assert!(
        c.selection
            .exclusions
            .iter()
            .any(|x| x.reason.contains("not the manual pin"))
    );
    check_golden("manual_pin", &resolved(&c));
}

#[test]
fn nothing_dispatches_from_what_cannot_be_prevalidated() {
    let t = thresholds();
    let e = no_evidence();
    // Stale statistics: the registry pins stats-7 and something else arrives.
    let stale = Evidence {
        stats_version: "stats-3".into(),
        legs: vec![],
    };
    let r = registry();
    let err = compile(&input(&r, &stale, &t)).unwrap_err();
    assert_eq!(err.code(), "STALE_STATISTICS");
    // Insufficient cap: the reserve does not even fit.
    let mut poor = input(&r, &e, &t);
    poor.request_cap = usd(50);
    assert_eq!(compile(&poor).unwrap_err().code(), "INSUFFICIENT_CAP");
    // A cap that fits the reserve but no plan: every candidate is named
    // ineligible with its worst case against the cap, and nothing runs.
    // (40k input tokens at 25 minor units per million is 1; the cheapest
    // plan is 2 plus the 100 reserve, so a cap of 101 fits the reserve and
    // nothing else.)
    let mut tight = input(&r, &e, &t);
    tight.request_cap = usd(101);
    let err = compile(&tight).unwrap_err();
    assert_eq!(err.code(), "NO_ELIGIBLE_BINDING", "{err:?}");
    assert!(
        matches!(&err, CompileRefused::NoEligibleBinding { exclusions }
            if exclusions.iter().any(|x| x.reason.contains("exceeds the request cap"))),
        "{err:?}"
    );
    // Integer overflow in a price is a refusal, not a wrap.
    let r_overflow = registry_with(vec![
        entry("gpt-5-mini", &["solver"], u64::MAX, 200),
        entry("gpt-5", &["solver", "reviewer"], 125, 1_000),
    ]);
    let err = compile(&input(&r_overflow, &e, &t)).unwrap_err();
    assert_eq!(err.code(), "OVERFLOW", "{err:?}");
    // Disallowed residency excludes the binding with the reason.
    let mut eu_only = input(&r, &e, &t);
    eu_only.allowed_residencies = vec!["eu".into()];
    let err = compile(&eu_only).unwrap_err();
    assert!(
        matches!(&err, CompileRefused::NoEligibleBinding { exclusions }
            if exclusions.iter().all(|x| x.reason.contains("residency"))),
        "{err:?}"
    );
    // Unavailable assurance: no continuation can be triggered honestly, so
    // only single-slot plans are enumerated.
    let mut no_gate = input(&r, &e, &t);
    no_gate.assurance_available = false;
    let c = compile(&no_gate).expect("compiled");
    assert!(
        c.candidates.iter().all(|k| k.bindings.len() == 1),
        "{:?}",
        c.candidates
    );
    check_golden("no_assurance", &resolved(&c));
    // A pin that is revoked is refused as a pin, not silently replaced.
    let mut r_revoked = registry();
    r_revoked.document.entries[0].revoked = true;
    let mut pinned = input(&r_revoked, &e, &t);
    pinned.manual_pin = Some(("openai".into(), "gpt-5-mini".into()));
    let err = compile(&pinned).unwrap_err();
    assert_eq!(err.code(), "PIN_NOT_ELIGIBLE");
    assert!(
        matches!(&err, CompileRefused::PinNotEligible { reason, .. } if reason.contains("revoked"))
    );
    // Every compiled plan passes the same validation admission applies, so
    // cyclic or undeclared slots cannot come out of the compiler at all.
    let c = compile(&input(&r, &e, &t)).unwrap();
    assert_eq!(c.plan.validate(scope().tenant_id, 1), Ok(()));
    assert!(
        c.plan
            .slots
            .iter()
            .all(|s| s.trigger == Trigger::Initial || s.trigger == Trigger::QualityRejected)
    );
}

/// REQ-EPR-009 (docs/27 §7.6): at a re-evaluation the plan opening with the
/// binding in force is the incumbent. With both solvers confidence-feasible,
/// a warm cached prefix makes staying on the dearer model cheaper than
/// switching once the re-prefill and the cache write are counted, so the
/// incumbent is kept; the same comparison with the prefix cold flips to the
/// cheaper model. With nothing confidence-feasible, the incumbent stays
/// whatever the economics say: a switch needs a feasible alternative.
#[test]
fn qual_epr_009_a_route_switches_only_to_a_feasible_alternative_that_beats_the_switch_cost() {
    use modbit_providers::economics::{CacheState, RemainingDemand, compare};
    let mut mid = entry("gpt-5", &["solver", "reviewer"], 1_000, 3_000);
    mid.economics.cached_input_per_mtok_minor = Some(100);
    mid.economics.cache_ttl_ms = Some(300_000);
    let mut mini = entry("gpt-5-mini", &["solver"], 700, 2_100);
    mini.economics.cache_write_per_mtok_minor = Some(100);
    let r = registry_with(vec![mini.clone(), mid.clone()]);
    let feasible = Evidence {
        stats_version: "stats-7".into(),
        legs: vec![
            leg("solver|gpt-5-mini|none|build-1", 0.80, 0.86, 150),
            leg("solver|gpt-5|none|build-1", 0.85, 0.90, 150),
        ],
    };
    // The compiler's basis: one leg at the expected prompt size and the
    // 4096-token output ceiling.
    let demand = RemainingDemand {
        input_tokens_per_call: 40_000,
        output_tokens_per_call: 4_096,
        calls: 1,
    };
    let warm = CacheState {
        endpoint: "openai".into(),
        model: "gpt-5".into(),
        cached_prefix_tokens: 36_000,
        last_used_at_ms: 1_000,
        prefix_key: "k".into(),
    };
    // Warm: staying on gpt-5 is cheaper; the threshold carries the switch cost.
    let economics_warm = compare(&mid, &mini, Some(&warm), 2_000, demand, 0, 500);
    assert_eq!(economics_warm.decision, "STAY", "{economics_warm:?}");
    let mut t = thresholds();
    t.switch_cost_minor = economics_warm.switch_cost.total_minor;
    let mut i = input(&r, &feasible, &t);
    i.current_binding = Some(("openai".into(), "gpt-5".into()));
    i.assurance_available = false;
    let kept = compile(&i).expect("compiled");
    assert_eq!(kept.selection.code, "FEASIBLE");
    assert_eq!(
        kept.plan.initial_slot().unwrap().model,
        "gpt-5",
        "{:?}",
        kept.selection
    );
    assert!(
        kept.selection
            .exclusions
            .iter()
            .any(|e| e.reason.contains("below the switch cost")),
        "{:?}",
        kept.selection.exclusions
    );
    // Cold (the prefix expired): the cheaper feasible model wins.
    let economics_cold = compare(&mid, &mini, Some(&warm), 2_000 + 600_000, demand, 0, 500);
    assert_eq!(economics_cold.decision, "SWITCH", "{economics_cold:?}");
    let mut t2 = thresholds();
    t2.switch_cost_minor = economics_cold.switch_cost.total_minor;
    let mut i2 = input(&r, &feasible, &t2);
    i2.current_binding = Some(("openai".into(), "gpt-5".into()));
    i2.assurance_available = false;
    let switched = compile(&i2).expect("compiled");
    assert_eq!(
        switched.plan.initial_slot().unwrap().model,
        "gpt-5-mini",
        "{:?}",
        switched.selection
    );
    // A fresh compile with no incumbent is byte-identical to before the
    // field existed: the plan ids do not move.
    let fresh_a = compile(&input(&r, &feasible, &t2)).expect("compiled");
    let fresh_b = compile(&input(&r, &feasible, &t2)).expect("compiled");
    assert_eq!(fresh_a.plan.plan_id, fresh_b.plan.plan_id);
    // Cold start: nothing is confidence-feasible; the incumbent stays even
    // though the alternative is nominally cheaper.
    let none = no_evidence();
    let mut i3 = input(&r, &none, &t2);
    i3.current_binding = Some(("openai".into(), "gpt-5".into()));
    i3.assurance_available = false;
    let stayed = compile(&i3).expect("compiled");
    assert_eq!(stayed.selection.code, "QUALITY_FLOOR_INFEASIBLE");
    assert_eq!(
        stayed.plan.initial_slot().unwrap().model,
        "gpt-5",
        "{:?}",
        stayed.selection
    );
    assert!(!stayed.selection.target_met);
    assert!(
        stayed
            .selection
            .exclusions
            .iter()
            .any(|e| e.plan_id != stayed.plan.plan_id && e.reason.contains("no observation")),
        "the alternative is excluded for want of evidence, not chosen for its price: {:?}",
        stayed.selection.exclusions
    );
    // And with no incumbent at cold start, the fallback picks the best by
    // lower bound and cost, as before.
    let mut i4 = input(&r, &none, &t2);
    i4.assurance_available = false;
    let fallback = compile(&i4).expect("compiled");
    assert_eq!(fallback.plan.initial_slot().unwrap().model, "gpt-5-mini");
}

/// REQ-EV-0030: an opener's approved fallback chain is compiled into every
/// plan as prevalidated `LegFailed` slots in order — worst-case budgeted like
/// any reachable slot, left out of the expected cost (only an outage pays
/// them) — and a fallback the request cannot use is excluded with its reason.
#[test]
fn an_approved_fallback_chain_is_compiled_as_prevalidated_leg_failed_slots() {
    let with_chain = |fallbacks: Vec<&str>, pro_tools: bool| {
        let mut mini = entry("gpt-5-mini", &["solver"], 25, 200);
        mini.fallbacks = fallbacks.into_iter().map(str::to_owned).collect();
        let mut pro = entry("gpt-5-pro", &["solver", "reviewer"], 1_500, 12_000);
        pro.tools = pro_tools;
        registry_with(vec![
            mini,
            entry("gpt-5", &["solver", "reviewer"], 125, 1_000),
            pro,
        ])
    };
    let e = no_evidence();
    let t = thresholds();
    let plain = compile(&input(&with_chain(vec![], true), &e, &t)).expect("compiled");
    let chained = compile(&input(
        &with_chain(vec!["openai/gpt-5", "openai/gpt-5-pro"], true),
        &e,
        &t,
    ))
    .expect("compiled");
    let fb: Vec<(&str, Option<&str>, Trigger, &str)> = chained
        .plan
        .slots
        .iter()
        .filter(|s| s.trigger == Trigger::LegFailed)
        .map(|s| {
            (
                s.slot_id.as_str(),
                s.predecessor.as_deref(),
                s.trigger,
                s.model.as_str(),
            )
        })
        .collect();
    assert_eq!(
        fb,
        vec![
            ("fallback-1", Some("initial"), Trigger::LegFailed, "gpt-5"),
            (
                "fallback-2",
                Some("fallback-1"),
                Trigger::LegFailed,
                "gpt-5-pro"
            ),
        ],
        "{:#?}",
        chained.plan.slots
    );
    assert_eq!(chained.plan.validate(scope().tenant_id, 1), Ok(()));
    let cost = |c: &modbit_providers::compiler::Compiled| {
        c.candidates
            .iter()
            .find(|k| k.plan_id == c.plan.plan_id)
            .map(|k| (k.expected_cost_minor, k.worst_case_cost_minor))
            .unwrap()
    };
    let (exp_plain, worst_plain) = cost(&plain);
    let (exp_chain, worst_chain) = cost(&chained);
    assert_eq!(exp_chain, exp_plain, "an outage is not expected");
    assert!(worst_chain > worst_plain, "but it is budgeted");
    // A fallback without tools cannot serve a tool-using request.
    let excluded =
        compile(&input(&with_chain(vec!["openai/gpt-5-pro"], false), &e, &t)).expect("compiled");
    assert!(
        !excluded
            .plan
            .slots
            .iter()
            .any(|s| s.trigger == Trigger::LegFailed),
        "{:#?}",
        excluded.plan.slots
    );
    assert!(
        excluded
            .selection
            .exclusions
            .iter()
            .any(|x| x.reason.starts_with("fallback openai/gpt-5-pro")),
        "{:?}",
        excluded.selection.exclusions
    );
}

/// One case of the QUAL-EV-0029 corpus: a request's demands, the policy it
/// runs under, and what the hard filters and the selector must make of them.
struct RoutingCase {
    name: &'static str,
    profile: &'static str,
    min_context_tokens: u32,
    residencies: &'static [&'static str],
    models: Option<&'static [&'static str]>,
    /// Binding → the start of the reason it must be excluded with.
    excluded: &'static [(&'static str, &'static str)],
    /// The binding the plan must open with; `None` = fail closed.
    chosen: Option<&'static str>,
}

/// QUAL-EV-0029 (REQ-EV-0029; docs/15 "Routing"): a benchmark corpus of
/// routing cases replayed through the compiler. Hard policy and capability
/// filters decide first — a binding that cannot call tools, one whose context
/// cannot hold the request, one the model policy refuses, a revoked one, one
/// in a residency the request may not use and one governance keeps from the
/// profile are removed before anything is weighed, the cheapest bindings
/// among them — and only then do the eval-gated quality floor and the
/// expected cost choose among what is left. Replaying the corpus reproduces
/// every exclusion, every score and every plan exactly; every score
/// recomputes from the registry prices and the evidence; a request nothing
/// can serve fails closed with every reason.
#[test]
fn qual_ev_0029_hard_filters_decide_first_and_the_corpus_replays_exactly() {
    let mut nano = entry("gpt-5-nano", &["solver"], 5, 40);
    nano.tools = false;
    let mut tiny = entry("gpt-5-tiny", &["solver"], 10, 80);
    tiny.context_tokens = 32_000;
    let mut old = entry("gpt-4-legacy", &["solver"], 20, 150);
    old.revoked = true;
    let mut eu = entry("gpt-5-eu", &["solver"], 60, 500);
    eu.governance.data_residency = "eu".into();
    let mut sandboxed = entry("gpt-5-sandboxed", &["solver"], 80, 600);
    sandboxed.governance.allowed_profiles = vec!["sandboxed".into()];
    let entries = vec![
        nano,
        tiny,
        old,
        entry("gpt-5-mini", &["solver"], 25, 200),
        eu,
        sandboxed,
        entry("gpt-5", &["solver"], 125, 1_000),
        // The registry binds a reviewer; this corpus compiles without one.
        entry("gpt-5-pro", &["solver", "reviewer"], 1_500, 12_000),
    ];
    let r = registry_with(entries.clone());
    // Eval evidence: the two cheapest bindings are the best measured, and
    // are excluded anyway when the request cannot use them.
    let e = Evidence {
        stats_version: "stats-7".into(),
        legs: vec![
            leg("solver|gpt-5-nano|none|build-1", 0.95, 0.97, 400),
            leg("solver|gpt-5-tiny|none|build-1", 0.90, 0.93, 300),
            leg("solver|gpt-5-mini|none|build-1", 0.55, 0.62, 200),
            leg("solver|gpt-5-eu|none|build-1", 0.78, 0.83, 120),
            leg("solver|gpt-5-sandboxed|none|build-1", 0.75, 0.80, 100),
            leg("solver|gpt-5|none|build-1", 0.80, 0.86, 150),
            leg("solver|gpt-5-pro|none|build-1", 0.93, 0.96, 90),
        ],
    };
    let t = thresholds();
    let corpus = [
        RoutingCase {
            name: "a plain request",
            profile: "local_trusted",
            min_context_tokens: 40_000,
            residencies: &[],
            models: None,
            excluded: &[
                ("openai/gpt-5-nano", "cannot call tools"),
                (
                    "openai/gpt-5-tiny",
                    "context of 32000 tokens is below the 40000",
                ),
                ("openai/gpt-4-legacy", "revoked in the active registry"),
                (
                    "openai/gpt-5-sandboxed",
                    "governance does not allow the local_trusted profile",
                ),
            ],
            chosen: Some("openai/gpt-5-eu"),
        },
        RoutingCase {
            name: "a request that must stay in the us",
            profile: "local_trusted",
            min_context_tokens: 40_000,
            residencies: &["us"],
            models: None,
            excluded: &[
                ("openai/gpt-5-nano", "cannot call tools"),
                (
                    "openai/gpt-5-tiny",
                    "context of 32000 tokens is below the 40000",
                ),
                ("openai/gpt-4-legacy", "revoked in the active registry"),
                (
                    "openai/gpt-5-eu",
                    "data residency eu is not allowed for this request",
                ),
                (
                    "openai/gpt-5-sandboxed",
                    "governance does not allow the local_trusted profile",
                ),
            ],
            chosen: Some("openai/gpt-5"),
        },
        RoutingCase {
            name: "an organization that allows two models",
            profile: "local_trusted",
            min_context_tokens: 40_000,
            residencies: &[],
            models: Some(&["gpt-5-pro", "openai/gpt-5-mini"]),
            excluded: &[
                (
                    "openai/gpt-5-nano",
                    "not allowed by the model policy in force",
                ),
                (
                    "openai/gpt-5-tiny",
                    "not allowed by the model policy in force",
                ),
                ("openai/gpt-4-legacy", "revoked in the active registry"),
                (
                    "openai/gpt-5-eu",
                    "not allowed by the model policy in force",
                ),
                (
                    "openai/gpt-5-sandboxed",
                    "not allowed by the model policy in force",
                ),
                ("openai/gpt-5", "not allowed by the model policy in force"),
            ],
            // The mini is allowed and cheaper, and below the floor.
            chosen: Some("openai/gpt-5-pro"),
        },
        RoutingCase {
            name: "a sandboxed request in the us",
            profile: "sandboxed",
            min_context_tokens: 40_000,
            residencies: &["us"],
            models: None,
            excluded: &[
                ("openai/gpt-5-nano", "cannot call tools"),
                (
                    "openai/gpt-5-tiny",
                    "context of 32000 tokens is below the 40000",
                ),
                ("openai/gpt-4-legacy", "revoked in the active registry"),
                (
                    "openai/gpt-5-eu",
                    "data residency eu is not allowed for this request",
                ),
            ],
            chosen: Some("openai/gpt-5-sandboxed"),
        },
        RoutingCase {
            name: "a small request",
            profile: "local_trusted",
            min_context_tokens: 16_000,
            residencies: &[],
            models: None,
            excluded: &[
                ("openai/gpt-5-nano", "cannot call tools"),
                ("openai/gpt-4-legacy", "revoked in the active registry"),
                (
                    "openai/gpt-5-sandboxed",
                    "governance does not allow the local_trusted profile",
                ),
            ],
            chosen: Some("openai/gpt-5-tiny"),
        },
        RoutingCase {
            name: "a request no binding can hold",
            profile: "local_trusted",
            min_context_tokens: 500_000,
            residencies: &[],
            models: None,
            excluded: &[
                ("openai/gpt-5-nano", "cannot call tools"),
                (
                    "openai/gpt-5-tiny",
                    "context of 32000 tokens is below the 500000",
                ),
                ("openai/gpt-4-legacy", "revoked in the active registry"),
                (
                    "openai/gpt-5-mini",
                    "context of 400000 tokens is below the 500000",
                ),
                (
                    "openai/gpt-5-eu",
                    "context of 400000 tokens is below the 500000",
                ),
                (
                    "openai/gpt-5-sandboxed",
                    "governance does not allow the local_trusted profile",
                ),
                (
                    "openai/gpt-5",
                    "context of 400000 tokens is below the 500000",
                ),
                (
                    "openai/gpt-5-pro",
                    "context of 400000 tokens is below the 500000",
                ),
            ],
            chosen: None,
        },
    ];
    let reason_for = |excluded: &[(String, String)], binding: &str| {
        excluded
            .iter()
            .find(|(b, _)| b == binding)
            .map(|(_, r)| r.clone())
    };
    let mut digests = std::collections::BTreeSet::new();
    for case in &corpus {
        let mut i = input(&r, &e, &t);
        i.assurance_available = false;
        i.execution_profile = case.profile.into();
        i.needs.min_context_tokens = case.min_context_tokens;
        i.allowed_residencies = case.residencies.iter().map(|s| (*s).to_owned()).collect();
        i.allowed_models = case
            .models
            .map(|m| m.iter().map(|s| (*s).to_owned()).collect());
        let first = compile(&i);
        // The replay: the same inputs, compiled again, are the same result,
        // exclusions and scores included.
        let again = compile(&i);
        assert_eq!(first, again, "{}: the replay differs", case.name);
        let excluded: Vec<(String, String)> = match &first {
            Ok(c) => c
                .hard_exclusions
                .iter()
                .map(|x| (x.plan_id.clone(), x.reason.clone()))
                .collect(),
            Err(CompileRefused::NoEligibleBinding { exclusions }) => exclusions
                .iter()
                .map(|x| (x.plan_id.clone(), x.reason.clone()))
                .collect(),
            Err(other) => panic!("{}: {other:?}", case.name),
        };
        // Exactly the expected bindings are removed, each for its reason.
        assert_eq!(
            excluded.len(),
            case.excluded.len(),
            "{}: {excluded:#?}",
            case.name
        );
        for (binding, why) in case.excluded {
            let reason = reason_for(&excluded, binding)
                .unwrap_or_else(|| panic!("{}: {binding} was not excluded", case.name));
            assert!(
                reason.starts_with(why),
                "{}: {binding} excluded for `{reason}`, not `{why}`",
                case.name
            );
        }
        let Some(chosen) = case.chosen else {
            assert!(
                matches!(first, Err(CompileRefused::NoEligibleBinding { .. })),
                "{}: fails closed",
                case.name
            );
            continue;
        };
        let c = first.expect("compiled");
        assert_eq!(
            format!("{}/{}", c.plan.slots[0].endpoint, c.plan.slots[0].model),
            chosen,
            "{}",
            case.name
        );
        // Nothing excluded is a candidate: the filters came before the
        // weighing, not after it.
        for k in &c.candidates {
            for b in &k.bindings {
                assert!(
                    reason_for(&excluded, b).is_none(),
                    "{}: excluded {b} became a candidate",
                    case.name
                );
            }
        }
        // Every score is auditable: it recomputes from the registry prices
        // and the evidence the compile was given.
        for k in &c.candidates {
            let model = k.bindings[0].trim_start_matches("openai/");
            let entry = entries.iter().find(|x| x.model == model).unwrap();
            let leg_minor = (40_000 * entry.economics.input_per_mtok_minor).div_ceil(1_000_000)
                + (4_096 * entry.economics.output_per_mtok_minor).div_ceil(1_000_000);
            assert_eq!(
                k.expected_cost_minor,
                leg_minor + 100,
                "{}: {} expected cost",
                case.name,
                k.plan_id
            );
            let evidence = e
                .legs
                .iter()
                .find(|l| l.key_id == format!("solver|{model}|none|build-1"))
                .unwrap();
            assert!(
                (k.quality.lcb - evidence.lcb).abs() < 1e-9,
                "{}: {} lcb {} vs evidence {}",
                case.name,
                k.plan_id,
                k.quality.lcb,
                evidence.lcb
            );
            assert!(k.quality.lcb <= k.quality.mean);
            assert!(k.worst_case_cost_minor >= k.expected_cost_minor);
        }
        // Then quality, then cost: the chosen plan clears the floor, and no
        // feasible plan is cheaper.
        let picked = c
            .candidates
            .iter()
            .find(|k| k.plan_id == c.plan.plan_id)
            .unwrap();
        assert!(picked.quality.lcb >= t.tau, "{}", case.name);
        assert!(
            c.candidates
                .iter()
                .filter(|k| k.quality.lcb >= t.tau && k.hard_eligible)
                .all(|k| k.expected_cost_minor >= picked.expected_cost_minor),
            "{}",
            case.name
        );
        // The demands read as the case states them, and differ where the
        // case differs.
        assert!(c.demands.contains(&"tools".to_owned()), "{:?}", c.demands);
        assert!(
            c.demands
                .contains(&format!("context>={}", case.min_context_tokens))
        );
        assert!(c.demands.contains(&format!("profile={}", case.profile)));
        if let Some(m) = case.models {
            let mut m: Vec<&str> = m.to_vec();
            m.sort_unstable();
            assert!(
                c.demands.contains(&format!("model in [{}]", m.join(","))),
                "{:?}",
                c.demands
            );
        }
        assert!(digests.insert(c.demands_digest.clone()), "{}", case.name);
    }
    // The policy is part of a replay's identity: a policy that allows every
    // binding chooses what an unrestricted compile chooses, under a digest
    // of its own.
    let mut i = input(&r, &e, &t);
    i.assurance_available = false;
    let unrestricted = compile(&i).unwrap();
    i.allowed_models = Some(entries.iter().map(|x| x.model.clone()).collect());
    let everything = compile(&i).unwrap();
    assert_eq!(everything.plan.slots, unrestricted.plan.slots);
    assert_ne!(everything.input_digest, unrestricted.input_digest);
    // A policy that allows nothing admits nothing.
    i.allowed_models = Some(vec![]);
    assert!(
        matches!(compile(&i), Err(CompileRefused::NoEligibleBinding { .. })),
        "a policy that allows nothing admits nothing"
    );
}
