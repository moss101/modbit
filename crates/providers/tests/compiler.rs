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
