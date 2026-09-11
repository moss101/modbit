//! REQ-EPR-001 / EPR-E2E-001 / EPR-FI-001: the durable routing state — a plan, its slots and every
//! attempt — written in the append transaction, recovered identically after a
//! kill, and rebuilt from the log alone.
use modbit_domain::event::{Actor, AggregateType};
use modbit_domain::routing::{
    Budget, ConditionalExecutionPlan, Money, Provenance, ROUTING_SCHEMA_VERSION, Slot, Trigger,
};
use modbit_domain::run::{OwnerLocation, RunEvent};
use modbit_domain::{RunId, SessionId, TaskId, TenantId, Timestamp};
use modbit_event_store::{AppendRequest, EventStore, NewEvent};

fn usd(minor: u64) -> Money {
    Money {
        minor_units: minor,
        currency: "USD".into(),
        scale: 2,
    }
}

fn plan(
    tenant: TenantId,
    session: SessionId,
    task: TaskId,
    run: RunId,
) -> ConditionalExecutionPlan {
    let slot = |id: &str, pred: Option<&str>, trigger: Trigger| Slot {
        slot_id: id.into(),
        predecessor: pred.map(str::to_owned),
        trigger,
        max_activations: 2,
        endpoint: "openai".into(),
        model: "gpt-5-mini".into(),
        role: "solver".into(),
        budget: Budget {
            timeout_ms: 120_000,
            max_output_tokens: 4096,
            max_retries: 1,
            reserved: usd(200),
        },
    };
    ConditionalExecutionPlan {
        schema_version: ROUTING_SCHEMA_VERSION,
        plan_id: "plan-store-1".into(),
        tenant_id: tenant,
        session_id: session,
        task_id: task,
        run_id: run,
        routing_epoch: 1,
        lease_generation: 1,
        created_at_ms: 1_700_000_000_000,
        provenance: Provenance {
            policy_version: "policy-1".into(),
            registry_generation: "registry-1".into(),
            profiler_version: "profiler-1".into(),
            statistics_version: "stats-1".into(),
            compiler_version: "compiler-2".into(),
            gate_version: "gate-1".into(),
            risk_version: "risk-1".into(),
            legacy_decode: None,
        },
        input_digest: "b".repeat(64),
        slots: vec![
            slot("initial", None, Trigger::Initial),
            slot("stronger", Some("initial"), Trigger::QualityRejected),
        ],
        max_total_attempts: 4,
        max_revisions: 1,
        verification_reserve: usd(100),
        total_budget: usd(1000),
        content_digest: String::new(),
    }
    .sealed()
}

fn typed<E: serde::Serialize>(t: &str, e: &E) -> NewEvent {
    let mut ev = NewEvent::new(
        t,
        serde_json::to_value(e).expect("serializable"),
        Actor::Core("test".into()),
    );
    ev.occurred_at = Some(Timestamp::now());
    ev
}

/// What the projection holds, read back through the store's own readers: the
/// number of plans, of slots, of attempts, and of attempts whose cost the
/// provider never reported.
fn rows(store: &EventStore, run: &RunId) -> (usize, usize, usize, usize) {
    let plans = store.routing_plans(run).unwrap();
    let slots = plans.iter().map(|p| p.slots.len()).sum();
    let attempts: Vec<_> = plans
        .iter()
        .flat_map(|p| store.routing_attempts(&p.plan_id).unwrap())
        .collect();
    let unknown = attempts.iter().filter(|a| !a.usage_known).count();
    (plans.len(), slots, attempts.len(), unknown)
}

/// Delete the routing projection out of band, the way a corrupted or
/// half-migrated database would look, so the rebuild has something to prove.
fn wipe_projection(dir: &std::path::Path) {
    let conn = rusqlite::Connection::open(dir.join("core.db")).unwrap();
    for t in ["routing_attempts", "routing_slots", "routing_plans"] {
        conn.execute(&format!("DELETE FROM {t}"), []).unwrap();
    }
}

/// A slot as the test compares it: what it is, what activated it, how often,
/// where it dispatches and what it reserved.
type SlotShape<'a> = (
    &'a str,
    Option<&'a str>,
    &'a str,
    u32,
    u32,
    &'a str,
    &'a str,
    u64,
    u64,
);

/// The session, task and run a routing plan hangs off.
fn seed_run(
    store: &mut EventStore,
    tenant: TenantId,
    session: SessionId,
    task: TaskId,
    run: RunId,
) {
    store
        .append(AppendRequest {
            tenant_id: tenant,
            session_id: session,
            task_id: None,
            run_id: None,
            turn_id: None,
            step_id: None,
            aggregate_type: AggregateType::Session,
            aggregate_id: *session.as_bytes(),
            expected_sequence: None,
            events: vec![typed(
                "SessionCreated",
                &modbit_domain::session::SessionEvent::SessionCreated {
                    tenant_id: tenant,
                    user_id: modbit_domain::UserId::new(),
                    space_id: modbit_domain::SpaceId::new(),
                },
            )],
        })
        .unwrap();
    store
        .append(AppendRequest {
            tenant_id: tenant,
            session_id: session,
            task_id: Some(task),
            run_id: None,
            turn_id: None,
            step_id: None,
            aggregate_type: AggregateType::Task,
            aggregate_id: *task.as_bytes(),
            expected_sequence: None,
            events: vec![typed(
                "TaskCreated",
                &modbit_domain::task::TaskEvent::TaskCreated {
                    session_id: session,
                    goal_text: "route it".into(),
                    workspace_id: modbit_domain::WorkspaceId::new(),
                    workspace_root: None,
                    base_revision: None,
                    execution_profile: "local_trusted".into(),
                    policy_profile_id: None,
                    origin: modbit_domain::task::TaskOrigin::Desktop,
                },
            )],
        })
        .unwrap();
    store
        .append(AppendRequest {
            tenant_id: tenant,
            session_id: session,
            task_id: Some(task),
            run_id: Some(run),
            turn_id: None,
            step_id: None,
            aggregate_type: AggregateType::Run,
            aggregate_id: *run.as_bytes(),
            expected_sequence: None,
            events: vec![typed(
                "RunCreated",
                &RunEvent::RunCreated {
                    task_id: task,
                    attempt: 1,
                    owner_location: OwnerLocation::Local,
                    kernel_lease_generation: 1,
                },
            )],
        })
        .unwrap();
}

/// The plan and two attempts against its initial slot: the first cancelled
/// with the provider reporting nothing, the second a success that reported.
fn append_plan_and_attempts(
    store: &mut EventStore,
    tenant: TenantId,
    session: SessionId,
    task: TaskId,
    run: RunId,
    p: &ConditionalExecutionPlan,
) {
    store
        .append(AppendRequest {
            tenant_id: tenant,
            session_id: session,
            task_id: Some(task),
            run_id: Some(run),
            turn_id: None,
            step_id: None,
            aggregate_type: AggregateType::Run,
            aggregate_id: *run.as_bytes(),
            expected_sequence: None,
            events: vec![
                typed(
                    "RoutingPlanCompiled",
                    &RunEvent::RoutingPlanCompiled {
                        plan: Box::new(p.clone()),
                        plan_ref: "c".repeat(64),
                    },
                ),
                typed(
                    "RoutingPlanAdmitted",
                    &RunEvent::RoutingPlanAdmitted {
                        plan_id: p.plan_id.clone(),
                        validation_digest: modbit_domain::routing::plan_digest(p),
                        reserved_minor: 500,
                        currency: "USD".into(),
                        scale: 2,
                        lease_generation: 1,
                        feasibility: "QUALITY_FLOOR_UNKNOWN".into(),
                        quality_lcb_bp: 0,
                        stats_version: "none".into(),
                        thresholds_version: "none".into(),
                        target_met: false,
                    },
                ),
                typed(
                    "SlotActivated",
                    &RunEvent::SlotActivated {
                        plan_id: p.plan_id.clone(),
                        slot_id: "initial".into(),
                        activation: 1,
                        reserved_minor: 200,
                    },
                ),
                typed(
                    "RoutingAttemptRecorded",
                    &RunEvent::RoutingAttemptRecorded {
                        plan_id: p.plan_id.clone(),
                        slot_id: "initial".into(),
                        attempt: 1,
                        outcome: "CANCELLED".into(),
                        usage_known: false,
                        input_tokens: None,
                        output_tokens: None,
                        provider_request_id: None,
                    },
                ),
                typed(
                    "RoutingAttemptRecorded",
                    &RunEvent::RoutingAttemptRecorded {
                        plan_id: p.plan_id.clone(),
                        slot_id: "initial".into(),
                        attempt: 2,
                        outcome: "SUCCEEDED".into(),
                        usage_known: true,
                        input_tokens: Some(1200),
                        output_tokens: Some(90),
                        provider_request_id: Some("req_abc".into()),
                    },
                ),
            ],
        })
        .unwrap();
}

#[test]
fn a_plan_its_slots_and_its_attempts_are_durable_and_rebuildable() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = EventStore::open(dir.path()).unwrap();
    let tenant = TenantId::new();
    let session = SessionId::new();
    let task = TaskId::new();
    let run = RunId::new();
    seed_run(&mut store, tenant, session, task, run);
    let p = plan(tenant, session, task, run);
    assert_eq!(p.validate(tenant, 0), Ok(()));
    append_plan_and_attempts(&mut store, tenant, session, task, run, &p);
    assert_eq!(rows(&store, &run), (1, 2, 2, 1));
    // The plan on the log is the plan that was validated, digest and all.
    let events = store.read_aggregate(run.as_bytes(), 0, 100).unwrap();
    let compiled = events
        .iter()
        .find(|e| e.envelope.event_type == "RoutingPlanCompiled")
        .unwrap();
    let payload = store.payload(&compiled.envelope).unwrap();
    let decoded: ConditionalExecutionPlan =
        serde_json::from_value(payload["plan"].clone()).unwrap();
    assert_eq!(decoded, p);
    assert_eq!(decoded.validate(tenant, 0), Ok(()));
    drop(store);
    // Reopening recovers the same rows: the projection was written inside the
    // append transaction, so there is no state in between to lose.
    let store = EventStore::open(dir.path()).unwrap();
    assert_eq!(rows(&store, &run), (1, 2, 2, 1));
    // And rebuilding from the log alone produces the same rows again: the
    // projection is derived state, so losing it costs nothing but time.
    drop(store);
    wipe_projection(dir.path());
    let mut store = EventStore::open(dir.path()).unwrap();
    assert_eq!(rows(&store, &run), (0, 0, 0, 0));
    assert!(store.rebuild_projections().unwrap() >= 4);
    assert_eq!(rows(&store, &run), (1, 2, 2, 1));
    // Every field came back, not just the counts.
    let rebuilt = store.routing_plans(&run).unwrap();
    let one = &rebuilt[0];
    assert_eq!(one.plan_id, p.plan_id);
    assert_eq!(one.schema_version, ROUTING_SCHEMA_VERSION);
    assert_eq!(one.routing_epoch, 1);
    assert_eq!(one.lease_generation, 1);
    assert_eq!(one.content_digest, p.content_digest);
    assert_eq!(one.plan_ref, "c".repeat(64));
    assert_eq!(one.total_budget, (1000, "USD".to_owned(), 2));
    assert_eq!(one.legacy_source, None);
    let shape: Vec<SlotShape<'_>> = one
        .slots
        .iter()
        .map(|s| {
            (
                s.slot_id.as_str(),
                s.predecessor.as_deref(),
                s.trigger.as_str(),
                s.max_activations,
                s.activations,
                s.endpoint.as_str(),
                s.model.as_str(),
                s.timeout_ms,
                s.reserved_minor,
            )
        })
        .collect();
    assert_eq!(
        shape,
        vec![
            (
                "initial",
                None,
                "INITIAL",
                2,
                1,
                "openai",
                "gpt-5-mini",
                120_000,
                200
            ),
            (
                "stronger",
                Some("initial"),
                "QUALITY_REJECTED",
                2,
                0,
                "openai",
                "gpt-5-mini",
                120_000,
                200
            ),
        ],
        "the slot graph, its budgets and its activation counts survive a rebuild"
    );
    // The unknown attempt stayed unknown through all of it: a cancelled call
    // whose provider reported nothing must never be read back as free.
    let attempts = store.routing_attempts(&one.plan_id).unwrap();
    assert_eq!(attempts.len(), 2);
    assert_eq!(
        (
            attempts[0].attempt,
            attempts[0].outcome.as_str(),
            attempts[0].usage_known,
            attempts[0].input_tokens,
            attempts[0].output_tokens,
            attempts[0].provider_request_id.clone(),
        ),
        (1, "CANCELLED", false, None, None, None)
    );
    assert_eq!(
        (
            attempts[1].attempt,
            attempts[1].outcome.as_str(),
            attempts[1].usage_known,
            attempts[1].input_tokens,
            attempts[1].output_tokens,
            attempts[1].provider_request_id.clone(),
        ),
        (
            2,
            "SUCCEEDED",
            true,
            Some(1200),
            Some(90),
            Some("req_abc".to_owned())
        )
    );
}

/// Turn a database into one from before the routing tables existed: drop them
/// and forget the migration that created them. The events stay exactly as they
/// were, which is the point — the routing state has to come back from them.
fn make_pre_routing(dir: &std::path::Path) {
    let conn = rusqlite::Connection::open(dir.join("core.db")).unwrap();
    for t in [
        "routing_activations",
        "routing_admissions",
        "routing_attempts",
        "routing_slots",
        "routing_plans",
    ] {
        conn.execute(&format!("DROP TABLE {t}"), []).unwrap();
    }
    // V10 (M4.1) added the protocol binding columns to `tool_calls` and the
    // `protocol_state` table.
    conn.execute("DROP TABLE IF EXISTS protocol_state", [])
        .unwrap();
    conn.execute("DROP INDEX IF EXISTS tool_calls_run", [])
        .unwrap();
    for c in ["run_id", "turn_id", "call_id", "arguments_ref"] {
        conn.execute(&format!("ALTER TABLE tool_calls DROP COLUMN {c}"), [])
            .unwrap();
    }
    conn.execute("DELETE FROM schema_migrations WHERE version >= 7", [])
        .unwrap();
}

/// A run recorded before the routing tables existed, in a database that
/// predates them. Opening it migrates to the current schema and derives the
/// routing state from the log it already held — nothing is invented, and
/// nothing that was recorded is lost.
#[test]
fn a_database_from_before_the_routing_tables_upgrades_and_derives_them() {
    let dir = tempfile::tempdir().unwrap();
    let (tenant, session, task, run) = (
        TenantId::new(),
        SessionId::new(),
        TaskId::new(),
        RunId::new(),
    );
    let mut store = EventStore::open(dir.path()).unwrap();
    seed_run(&mut store, tenant, session, task, run);
    let p = plan(tenant, session, task, run);
    append_plan_and_attempts(&mut store, tenant, session, task, run, &p);
    assert_eq!(rows(&store, &run), (1, 2, 2, 1));
    let events = store.last_offset().unwrap();
    drop(store);

    make_pre_routing(dir.path());
    let (store, report) = EventStore::open_with_report(dir.path()).unwrap();
    assert_eq!((report.from_version, report.to_version), (6, 10));
    assert_eq!(report.applied, vec![7, 8, 9, 10]);
    assert_eq!(
        store.last_offset().unwrap(),
        events,
        "migration preserves the log"
    );
    assert_eq!(
        rows(&store, &run),
        (1, 2, 2, 1),
        "the routing state was derived from events written before the tables existed"
    );
    let one = &store.routing_plans(&run).unwrap()[0];
    assert_eq!(one.content_digest, p.content_digest);
    assert_eq!(
        store
            .routing_attempts(&one.plan_id)
            .unwrap()
            .iter()
            .filter(|a| !a.usage_known)
            .count(),
        1,
        "the attempt with no reported usage is still unknown after the upgrade"
    );
}

/// EPR-FI-001: a process killed while the migration is in flight leaves a
/// database the next open recovers. The child applies the real migration SQL
/// inside a transaction it never commits, then dies hard; SQLite rolls the
/// transaction back, and the store migrates the database properly afterwards.
#[test]
fn a_crash_during_the_routing_migration_leaves_a_recoverable_database() {
    if let Ok(dir) = std::env::var("MODBIT_ROUTING_MIGRATION_CRASH_DIR") {
        migrator_role(std::path::Path::new(&dir));
    }
    let dir = tempfile::tempdir().unwrap();
    let (tenant, session, task, run) = (
        TenantId::new(),
        SessionId::new(),
        TaskId::new(),
        RunId::new(),
    );
    let mut store = EventStore::open(dir.path()).unwrap();
    seed_run(&mut store, tenant, session, task, run);
    let p = plan(tenant, session, task, run);
    append_plan_and_attempts(&mut store, tenant, session, task, run, &p);
    drop(store);
    make_pre_routing(dir.path());

    let exe = std::env::current_exe().unwrap();
    let mut child = std::process::Command::new(&exe)
        .args([
            "--exact",
            "a_crash_during_the_routing_migration_leaves_a_recoverable_database",
            "--nocapture",
        ])
        .env("MODBIT_ROUTING_MIGRATION_CRASH_DIR", dir.path())
        .stdout(std::process::Stdio::null())
        .spawn()
        .unwrap();
    // Wait until the child has the migration open, then kill it mid-flight.
    let flag = dir.path().join("migrating");
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    while !flag.exists() && std::time::Instant::now() < deadline {
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    assert!(flag.exists(), "the child never started the migration");
    child.kill().unwrap();
    let status = child.wait().unwrap();
    assert!(
        !status.success(),
        "the child must have died from the kill, not exited cleanly"
    );

    let (store, report) = EventStore::open_with_report(dir.path()).unwrap();
    assert_eq!(
        (report.from_version, report.to_version),
        (6, 10),
        "the killed migration committed nothing"
    );
    assert_eq!(
        rows(&store, &run),
        (1, 2, 2, 1),
        "and the second attempt at it recovers the whole routing state"
    );
    store.integrity_check().unwrap();
}

/// The child of the crash test: applies the routing migration for real and
/// then waits to be killed without ever committing it.
fn migrator_role(dir: &std::path::Path) -> ! {
    let conn = rusqlite::Connection::open(dir.join("core.db")).unwrap();
    conn.busy_timeout(std::time::Duration::from_secs(10))
        .unwrap();
    conn.pragma_update(None, "journal_mode", "WAL").unwrap();
    let up = modbit_event_store::schema::MIGRATIONS
        .iter()
        .find(|m| m.version == 7)
        .expect("the routing migration")
        .up;
    conn.execute_batch(&format!("BEGIN IMMEDIATE;\n{up}"))
        .unwrap();
    conn.execute(
        "INSERT INTO schema_migrations (version, name, checksum, applied_at) VALUES (7, 'routing_plans_slots_attempts', 'x', 0)",
        [],
    )
    .unwrap();
    std::fs::write(dir.join("migrating"), b"in flight").unwrap();
    loop {
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
}
