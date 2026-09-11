//! REQ-EPR-001: what a routing contract must satisfy before anything can run
//! from it, and what it refuses.
use modbit_domain::routing::{
    Budget, ConditionalExecutionPlan, DIRECT_COMPILER_VERSION, DIRECT_NOT_CONSULTED, DIRECT_NOTE,
    DirectPath, Invalid, LegacyExecutionPlan, Money, PlanScope, Probability, Provenance,
    ROUTING_SCHEMA_VERSION, Slot, Trigger, decode_legacy, plan_digest,
};
use modbit_domain::{RunId, SessionId, TaskId, TenantId};

fn provenance() -> Provenance {
    Provenance {
        policy_version: "policy-1".into(),
        registry_generation: "registry-7".into(),
        profiler_version: "profiler-1".into(),
        statistics_version: "stats-3".into(),
        compiler_version: "compiler-2".into(),
        gate_version: "gate-1".into(),
        risk_version: "risk-1".into(),
        legacy_decode: None,
    }
}

fn usd(minor: u64) -> Money {
    Money {
        minor_units: minor,
        currency: "USD".into(),
        scale: 2,
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
            max_retries: 0,
            reserved: usd(reserved),
        },
    }
}

fn plan(tenant: TenantId, slots: Vec<Slot>) -> ConditionalExecutionPlan {
    ConditionalExecutionPlan {
        schema_version: ROUTING_SCHEMA_VERSION,
        plan_id: "plan-1".into(),
        tenant_id: tenant,
        session_id: SessionId::new(),
        task_id: TaskId::new(),
        run_id: RunId::new(),
        routing_epoch: 3,
        lease_generation: 2,
        created_at_ms: 1_700_000_000_000,
        provenance: provenance(),
        input_digest: "a".repeat(64),
        slots,
        max_total_attempts: 3,
        max_revisions: 1,
        verification_reserve: usd(100),
        total_budget: usd(1000),
        content_digest: String::new(),
    }
    .sealed()
}

#[test]
fn a_valid_plan_is_sealed_by_its_own_content() {
    let tenant = TenantId::new();
    let p = plan(
        tenant,
        vec![
            slot("initial", None, Trigger::Initial, 300),
            slot("stronger", Some("initial"), Trigger::QualityRejected, 400),
        ],
    );
    assert_eq!(p.validate(tenant, 3), Ok(()));
    assert_eq!(p.content_digest, plan_digest(&p));
    assert_eq!(p.initial_slot().unwrap().slot_id, "initial");
    // A plan compiled in a newer epoch is fine; an older one is stale.
    assert_eq!(p.validate(tenant, 2), Ok(()));
    assert_eq!(
        p.validate(tenant, 4),
        Err(Invalid::StaleGeneration {
            field: "routing_epoch".into(),
            offered: 3,
            current: 4
        })
    );
    // Editing a sealed plan breaks its digest.
    let mut edited = p.clone();
    edited.max_total_attempts = 9;
    assert!(matches!(
        edited.validate(tenant, 3),
        Err(Invalid::OutOfRange { ref field, .. }) if field == "content_digest"
    ));
}

#[test]
fn a_plan_that_cannot_be_executed_is_refused_before_anything_dispatches() {
    let tenant = TenantId::new();
    let other = TenantId::new();
    // Foreign tenant.
    let p = plan(other, vec![slot("initial", None, Trigger::Initial, 0)]);
    assert!(matches!(
        p.validate(tenant, 0),
        Err(Invalid::ForeignTenant { .. })
    ));
    // A future major version fails closed rather than being guessed at.
    let mut future = plan(tenant, vec![slot("initial", None, Trigger::Initial, 0)]);
    future.schema_version = ROUTING_SCHEMA_VERSION + 1;
    assert!(matches!(
        future.validate(tenant, 0),
        Err(Invalid::IncompatibleVersion { found, .. }) if found == ROUTING_SCHEMA_VERSION + 1
    ));
    // Duplicate slot ids.
    let dup = plan(
        tenant,
        vec![
            slot("initial", None, Trigger::Initial, 0),
            slot("initial", Some("initial"), Trigger::LegFailed, 0),
        ],
    );
    assert!(matches!(
        dup.validate(tenant, 0),
        Err(Invalid::Duplicate { ref value, .. }) if value == "initial"
    ));
    // A predecessor that is not in the plan.
    let dangling = plan(
        tenant,
        vec![
            slot("initial", None, Trigger::Initial, 0),
            slot("review", Some("nowhere"), Trigger::ReviewRequired, 0),
        ],
    );
    assert!(matches!(
        dangling.validate(tenant, 0),
        Err(Invalid::UnresolvedReference { ref reference, .. }) if reference == "nowhere"
    ));
    // A cycle.
    let cyclic = plan(
        tenant,
        vec![
            slot("initial", None, Trigger::Initial, 0),
            slot("a", Some("b"), Trigger::LegFailed, 0),
            slot("b", Some("a"), Trigger::LegFailed, 0),
        ],
    );
    assert!(matches!(
        cyclic.validate(tenant, 0),
        Err(Invalid::UnresolvedReference { ref reference, .. }) if reference.starts_with("cycle")
    ));
    // Exactly one initial slot.
    let two = plan(
        tenant,
        vec![
            slot("a", None, Trigger::Initial, 0),
            slot("b", None, Trigger::Initial, 0),
        ],
    );
    assert!(matches!(
        two.validate(tenant, 0),
        Err(Invalid::OutOfRange { ref field, .. }) if field == "slots[].trigger"
    ));
    // Slots that reserve more than the request's total.
    let greedy = plan(
        tenant,
        vec![
            slot("initial", None, Trigger::Initial, 800),
            slot("stronger", Some("initial"), Trigger::QualityRejected, 800),
        ],
    );
    assert!(matches!(
        greedy.validate(tenant, 0),
        Err(Invalid::OutOfRange { ref field, .. }) if field == "total_budget"
    ));
    // A missing version is a missing input.
    let mut nameless = plan(tenant, vec![slot("initial", None, Trigger::Initial, 0)]);
    nameless.provenance.compiler_version = String::new();
    let nameless = nameless.sealed();
    assert_eq!(
        nameless.validate(tenant, 0),
        Err(Invalid::Missing {
            field: "provenance.compiler_version".into()
        })
    );
}

#[test]
fn money_is_integer_and_probability_is_a_probability() {
    assert!(usd(100).validate("b").is_ok());
    assert!(matches!(
        Money {
            minor_units: 1,
            currency: "usd".into(),
            scale: 2
        }
        .validate("b"),
        Err(Invalid::OutOfRange { .. })
    ));
    assert!(matches!(
        Money {
            minor_units: 1,
            currency: "USD".into(),
            scale: 9
        }
        .validate("b"),
        Err(Invalid::OutOfRange { .. })
    ));
    // Adding across currencies or past the ceiling is refused, never wrapped.
    let eur = Money {
        minor_units: 5,
        currency: "EUR".into(),
        scale: 2,
    };
    assert!(usd(5).checked_add(&eur, "b").is_err());
    let big = Money {
        minor_units: u64::MAX,
        currency: "USD".into(),
        scale: 2,
    };
    assert!(big.checked_add(&usd(1), "b").is_err());
    assert_eq!(usd(5).checked_add(&usd(6), "b").unwrap().minor_units, 11);
    // Probabilities are finite and in range.
    assert_eq!(Probability::new(0.5, "p").unwrap().get(), 0.5);
    assert!(Probability::new(1.0, "p").is_ok());
    for bad in [-0.1, 1.1, f64::NAN, f64::INFINITY] {
        assert!(Probability::new(bad, "p").is_err(), "{bad}");
    }
}

#[test]
fn a_legacy_plan_decodes_with_its_provenance_and_authorizes_nothing() {
    let tenant = TenantId::new();
    let legacy = LegacyExecutionPlan {
        version: 1,
        template: "CASCADE".into(),
        endpoint: "openai".into(),
        model: "gpt-5-mini".into(),
        timeout_ms: Some(60_000),
    };
    let p = decode_legacy(
        &legacy,
        "plan-legacy",
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
    assert_eq!(p.validate(tenant, 0), Ok(()));
    assert_eq!(p.schema_version, ROUTING_SCHEMA_VERSION);
    // One slot, and it is the initial one: a template label bought no
    // continuation.
    assert_eq!(p.slots.len(), 1);
    assert_eq!(p.slots[0].trigger, Trigger::Initial);
    assert_eq!(p.max_total_attempts, 1);
    assert_eq!(p.max_revisions, 0);
    let d = p.provenance.legacy_decode.as_ref().unwrap();
    assert_eq!(
        (d.source_shape.as_str(), d.source_version),
        ("ExecutionPlan", 1)
    );
    assert!(d.unknown_fields.contains(&"quality_lcb".to_owned()));
    assert!(d.note.contains("authorizes no slot"), "{}", d.note);
    // The label itself is not carried into anything executable.
    assert!(!serde_json::to_string(&p.slots).unwrap().contains("CASCADE"));
}

/// The direct path is not a routed path, and its plan says so: one solver slot
/// activated once, provenance that names nothing it did not consult, and no
/// budget it cannot compute.
#[test]
fn the_direct_baseline_is_a_valid_plan_that_claims_no_routing() {
    let tenant = TenantId::new();
    let run = RunId::new();
    let p = ConditionalExecutionPlan::direct(
        tenant,
        SessionId::new(),
        TaskId::new(),
        run,
        3,
        1_700_000_000_000,
        &DirectPath {
            endpoint: "openai",
            model: "gpt-5-mini",
            timeout_ms: 120_000,
            max_output_tokens: 4096,
            max_retries: 2,
            max_turns: 12,
        },
    );
    assert_eq!(p.validate(tenant, 0), Ok(()));
    assert_eq!(p.schema_version, ROUTING_SCHEMA_VERSION);
    assert_eq!(p.plan_id, format!("direct:{run}"));
    assert_eq!(p.routing_epoch, 0, "any compiled plan is newer");
    assert_eq!(p.slots.len(), 1);
    assert_eq!(p.slots[0].trigger, Trigger::Initial);
    assert_eq!(p.slots[0].max_activations, 1);
    assert_eq!(p.max_total_attempts, 12);
    assert_eq!(p.max_revisions, 0, "the direct path revises nothing");
    // Nothing routed it, and the record says so instead of borrowing a version.
    assert_eq!(p.provenance.compiler_version, DIRECT_COMPILER_VERSION);
    for v in [
        &p.provenance.policy_version,
        &p.provenance.registry_generation,
        &p.provenance.profiler_version,
        &p.provenance.statistics_version,
        &p.provenance.gate_version,
        &p.provenance.risk_version,
    ] {
        assert_eq!(v, DIRECT_NOT_CONSULTED);
    }
    assert!(p.provenance.legacy_decode.is_none());
    // Unbudgeted, not free.
    assert_eq!(p.total_budget.minor_units, 0);
    assert_eq!(p.slots[0].budget.reserved.minor_units, 0);
    assert!(DIRECT_NOTE.contains("unbudgeted, not free"));
    // Sealed by its own content, like every other plan.
    let mut tampered = p.clone();
    tampered.slots[0].model = "gpt-5".into();
    assert_ne!(tampered.clone().sealed().content_digest, p.content_digest);
    // A plan compiled in a later epoch is what supersedes it; the direct plan
    // never installs over one.
    assert!(matches!(
        p.validate(tenant, 1),
        Err(Invalid::StaleGeneration { .. })
    ));
}
