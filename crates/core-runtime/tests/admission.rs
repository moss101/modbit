//! REQ-EPR-014 / EPR-FI-014: what admission lets through, and what it refuses
//! before anything can dispatch.

use modbit_core_runtime::admission::{
    Activation, Refused, RunLedger, admit_activation, admit_plan, check_not_replayed,
};
use modbit_domain::routing::{
    Budget, ConditionalExecutionPlan, Invalid, LegacyExecutionPlan, Money, PlanScope, Provenance,
    ROUTING_SCHEMA_VERSION, Slot, Trigger, decode_legacy,
};
use modbit_domain::{RunId, SessionId, TaskId, TenantId};

fn usd(minor: u64) -> Money {
    Money {
        minor_units: minor,
        currency: "USD".into(),
        scale: 2,
    }
}

fn provenance() -> Provenance {
    Provenance {
        policy_version: "policy-1".into(),
        registry_generation: "registry-1".into(),
        profiler_version: "profiler-1".into(),
        statistics_version: "stats-1".into(),
        compiler_version: "compiler-2".into(),
        gate_version: "gate-1".into(),
        risk_version: "risk-1".into(),
        legacy_decode: None,
    }
}

fn slot(id: &str, predecessor: Option<&str>, trigger: Trigger, reserved: u64) -> Slot {
    Slot {
        slot_id: id.into(),
        predecessor: predecessor.map(str::to_owned),
        trigger,
        max_activations: 1,
        endpoint: "openai".into(),
        model: "gpt-5-mini".into(),
        role: "solver".into(),
        budget: Budget {
            timeout_ms: 120_000,
            max_output_tokens: 4096,
            max_retries: 1,
            reserved: usd(reserved),
        },
    }
}

fn plan(tenant: TenantId, slots: Vec<Slot>) -> ConditionalExecutionPlan {
    ConditionalExecutionPlan {
        schema_version: ROUTING_SCHEMA_VERSION,
        plan_id: "plan-admit-1".into(),
        tenant_id: tenant,
        session_id: SessionId::new(),
        task_id: TaskId::new(),
        run_id: RunId::new(),
        routing_epoch: 3,
        lease_generation: 5,
        created_at_ms: 1_700_000_000_000,
        provenance: provenance(),
        input_digest: "d".repeat(64),
        slots,
        max_total_attempts: 4,
        max_revisions: 1,
        verification_reserve: usd(100),
        total_budget: usd(1000),
        content_digest: String::new(),
    }
    .sealed()
}

fn two_slots(tenant: TenantId) -> ConditionalExecutionPlan {
    plan(
        tenant,
        vec![
            slot("initial", None, Trigger::Initial, 200),
            slot("stronger", Some("initial"), Trigger::QualityRejected, 300),
        ],
    )
}

#[test]
fn a_plan_is_admitted_whole_with_the_digest_of_what_was_validated() {
    let tenant = TenantId::new();
    let p = two_slots(tenant);
    let a = admit_plan(&p, tenant, 3).expect("admitted");
    assert_eq!(a.plan_id, "plan-admit-1");
    assert_eq!(a.routing_epoch, 3);
    assert_eq!(a.lease_generation, 5);
    // Slots plus the verification reserve, in one currency.
    assert_eq!(a.reserved, usd(600));
    assert_eq!(a.validation_digest.len(), 64);
    // The digest is of the plan that was validated: change any of it and the
    // digest no longer matches, so an admitted plan cannot be swapped.
    let mut other = p.clone();
    other.slots[1].model = "gpt-5".into();
    let b = admit_plan(&other.sealed(), tenant, 3).expect("admitted");
    assert_ne!(b.validation_digest, a.validation_digest);
}

#[test]
fn admission_refuses_before_anything_dispatches() {
    let tenant = TenantId::new();
    // A cyclic continuation graph.
    let cyclic = plan(
        tenant,
        vec![
            slot("initial", None, Trigger::Initial, 100),
            slot("a", Some("b"), Trigger::QualityRejected, 100),
            slot("b", Some("a"), Trigger::LegFailed, 100),
        ],
    );
    let err = admit_plan(&cyclic, tenant, 3).unwrap_err();
    assert_eq!(err.code(), "PLAN_INVALID");
    assert!(
        matches!(err, Refused::Invalid(Invalid::UnresolvedReference { .. })),
        "{err:?}"
    );
    // A plan from an older epoch never installs over a newer one.
    let stale = two_slots(tenant);
    let err = admit_plan(&stale, tenant, 4).unwrap_err();
    assert!(
        matches!(err, Refused::Invalid(Invalid::StaleGeneration { .. })),
        "{err:?}"
    );
    // A plan belonging to another tenant.
    let err = admit_plan(&two_slots(tenant), TenantId::new(), 3).unwrap_err();
    assert!(
        matches!(err, Refused::Invalid(Invalid::ForeignTenant { .. })),
        "{err:?}"
    );
    // Slots that reserve more than the request cap allows once verification is
    // held back.
    let greedy = plan(
        tenant,
        vec![
            slot("initial", None, Trigger::Initial, 900),
            slot("stronger", Some("initial"), Trigger::QualityRejected, 300),
        ],
    );
    let err = admit_plan(&greedy, tenant, 3).unwrap_err();
    assert!(
        matches!(err, Refused::Invalid(Invalid::OutOfRange { .. })),
        "{err:?}"
    );
}

#[test]
fn a_legacy_template_label_authorizes_no_slot() {
    let tenant = TenantId::new();
    let legacy = LegacyExecutionPlan {
        version: 1,
        template: "CASCADE".into(),
        endpoint: "openai".into(),
        model: "gpt-5-mini".into(),
        timeout_ms: Some(60_000),
    };
    let decoded = decode_legacy(
        &legacy,
        "plan-legacy-1",
        PlanScope {
            tenant_id: tenant,
            session_id: SessionId::new(),
            task_id: TaskId::new(),
            run_id: RunId::new(),
            created_at_ms: 1_700_000_000_000,
        },
        provenance(),
        usd(1000),
    );
    // The label is kept as provenance, and the decoded plan holds exactly the
    // one leg the legacy record described.
    let d = decoded
        .provenance
        .legacy_decode
        .as_ref()
        .expect("provenance");
    assert_eq!(d.template_label, "CASCADE");
    assert_eq!(decoded.slots.len(), 1);
    assert!(admit_plan(&decoded, tenant, 0).is_ok());
    // The CASCADE label buys no second slot: a plan that leans on it is
    // refused rather than admitted.
    let mut smuggled = decoded.clone();
    smuggled.slots.push(slot(
        "stronger",
        Some("initial"),
        Trigger::QualityRejected,
        0,
    ));
    let err = admit_plan(&smuggled.sealed(), tenant, 0).unwrap_err();
    assert_eq!(err.code(), "LEGACY_TEMPLATE_AUTHORIZES_NOTHING");
    assert!(
        matches!(err, Refused::LegacyTemplate { ref label } if label == "CASCADE"),
        "{err:?}"
    );
}

#[test]
fn the_runtime_activates_slots_and_never_adds_one() {
    let tenant = TenantId::new();
    let p = two_slots(tenant);
    let mut ledger = RunLedger::empty("USD", 2);
    // The initial slot activates once.
    assert_eq!(
        admit_activation(&p, &ledger, "initial", Trigger::Initial).unwrap(),
        Activation {
            slot_id: "initial".into(),
            activation: 1,
            reserved: usd(200),
        }
    );
    // A continuation the plan does not contain is unavailable, not synthesised.
    let err = admit_activation(&p, &ledger, "reviewer", Trigger::ReviewRequired).unwrap_err();
    assert_eq!(err.code(), "REQUIRED_CONTINUATION_UNAVAILABLE");
    assert!(
        matches!(err, Refused::RequiredContinuationUnavailable { ref admitted, .. }
            if admitted == &vec!["initial".to_owned(), "stronger".to_owned()]),
        "{err:?}"
    );
    // A slot the plan does contain, on a trigger it does not answer, is
    // equally unavailable.
    let err = admit_activation(&p, &ledger, "stronger", Trigger::LegFailed).unwrap_err();
    assert_eq!(err.code(), "REQUIRED_CONTINUATION_UNAVAILABLE");
    // Once the initial slot has used its single activation, it is exhausted.
    ledger.activations.push(("initial".into(), 1));
    let err = admit_activation(&p, &ledger, "initial", Trigger::Initial).unwrap_err();
    assert_eq!(err.code(), "ACTIVATIONS_EXHAUSTED");
    // The continuation is still available, and it reserves its own money.
    assert_eq!(
        admit_activation(&p, &ledger, "stronger", Trigger::QualityRejected)
            .unwrap()
            .reserved,
        usd(300)
    );
}

#[test]
fn no_slot_receives_a_new_budget_and_a_restart_recovers_its_activation() {
    let tenant = TenantId::new();
    let p = two_slots(tenant);
    let mut ledger = RunLedger::empty("USD", 2);
    ledger.activations.push(("initial".into(), 1));
    // Spent plus in flight plus the verification reserve bounds what is left:
    // 1000 - 550 - 250 - 100 = 100, which does not cover the 300 the
    // continuation reserves.
    ledger.spent = usd(550);
    ledger.in_flight = usd(250);
    let err = admit_activation(&p, &ledger, "stronger", Trigger::QualityRejected).unwrap_err();
    assert_eq!(err.code(), "ALLOWANCE_EXCEEDED");
    assert!(
        matches!(err, Refused::AllowanceExceeded { ref needs, ref allowance, .. }
            if needs == &usd(300) && allowance == &usd(100)),
        "{err:?}"
    );
    // With the in-flight reservation settled, the same slot fits.
    ledger.in_flight = usd(0);
    assert!(admit_activation(&p, &ledger, "stronger", Trigger::QualityRejected).is_ok());
    // The transaction's own attempt ceiling still applies.
    ledger.attempts = 4;
    let err = admit_activation(&p, &ledger, "stronger", Trigger::QualityRejected).unwrap_err();
    assert_eq!(err.code(), "ATTEMPTS_EXHAUSTED");
    // A restart that replays an activation it already recorded recovers it
    // rather than opening a second one.
    assert_eq!(
        check_not_replayed(&ledger, "initial", 1)
            .unwrap_err()
            .code(),
        "DUPLICATE_ACTIVATION"
    );
    assert!(check_not_replayed(&ledger, "stronger", 1).is_ok());
}
