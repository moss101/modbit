//! M1.2 real-effect tests: migrations against the committed M1.1-era fixture
//! database, checksum drift, projections in the append transaction and after
//! rebuild, and command idempotency.

use std::path::Path;

use modbit_domain::event::{Actor, AggregateType};
use modbit_domain::run::{OwnerLocation, RunEvent, RunState};
use modbit_domain::session::SessionEvent;
use modbit_domain::step::{StepEvent, StepState, StepType};
use modbit_domain::task::{TaskEvent, TaskOrigin, TaskState, WaitReason};
use modbit_domain::turn::{TurnEvent, TurnState};
use modbit_domain::{
    EventId, RunId, RunStepId, SessionId, SpaceId, TaskId, TenantId, TurnId, UserId, WorkspaceId,
};
use modbit_event_store::{
    AppendRequest, CommandOutcome, CommandRecord, Error, EventStore, NewEvent,
};
use serde_json::json;

fn fixture() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/db/core-v1-m1.1.db")
        .canonicalize()
        .unwrap()
}

fn actor() -> Actor {
    Actor::Core("test".into())
}

fn typed<E: serde::Serialize>(event_type: &str, e: &E) -> NewEvent {
    NewEvent::new(event_type, serde_json::to_value(e).unwrap(), actor())
}

fn req(
    tenant: TenantId,
    session: SessionId,
    agg: AggregateType,
    id: [u8; 16],
    events: Vec<NewEvent>,
) -> AppendRequest {
    AppendRequest {
        tenant_id: tenant,
        session_id: session,
        task_id: None,
        run_id: None,
        turn_id: None,
        step_id: None,
        aggregate_type: agg,
        aggregate_id: id,
        expected_sequence: None,
        events,
    }
}

#[test]
fn migrates_the_committed_m1_1_fixture_and_derives_projections() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::copy(fixture(), dir.path().join("core.db")).unwrap();
    let (store, report) = EventStore::open_with_report(dir.path()).unwrap();
    assert_eq!(report.from_version, 1);
    assert_eq!(report.to_version, 11);
    assert_eq!(report.applied, vec![2, 3, 4, 5, 6, 7, 8, 9, 10, 11]);
    // Events untouched (docs/31: migration preserves existing event ids).
    let task = TaskId::from_bytes([0xC3; 16]);
    assert_eq!(store.verify_aggregate(task.as_bytes()).unwrap(), 3);
    let session = SessionId::from_bytes([0xB2; 16]);
    assert_eq!(store.read_session(&session, 0, 100).unwrap().len(), 4);
    // Projections derived from the log during the upgrade.
    let t = store.task(&task).unwrap().expect("task projection");
    assert_eq!(t.state, TaskState::Running);
    assert_eq!(t.goal_text, "seed fixture");
    assert_eq!(t.generation, 3);
    let s = store
        .session(&session)
        .unwrap()
        .expect("session projection");
    assert_eq!(s.generation, 1);
    assert_eq!(store.projection_offset().unwrap(), 4);
    drop(store);
    // Reopening is a no-op migration.
    let (_, report) = EventStore::open_with_report(dir.path()).unwrap();
    assert!(report.applied.is_empty());
    assert_eq!(report.from_version, 11);
    let conn = rusqlite::Connection::open(dir.path().join("core.db")).unwrap();
    let n: i64 = conn
        .query_row("SELECT count(*) FROM schema_migrations", [], |r| r.get(0))
        .unwrap();
    assert_eq!(n, 11);
}

#[test]
fn applied_migration_checksum_drift_and_newer_schema_are_refused() {
    let dir = tempfile::tempdir().unwrap();
    EventStore::open(dir.path()).unwrap();
    let conn = rusqlite::Connection::open(dir.path().join("core.db")).unwrap();
    conn.execute(
        "UPDATE schema_migrations SET checksum = 'tampered' WHERE version = 2",
        [],
    )
    .unwrap();
    drop(conn);
    let err = EventStore::open(dir.path()).unwrap_err();
    assert!(matches!(err, Error::Integrity { sequence: 2, .. }), "{err}");

    let dir = tempfile::tempdir().unwrap();
    EventStore::open(dir.path()).unwrap();
    let conn = rusqlite::Connection::open(dir.path().join("core.db")).unwrap();
    conn.execute("INSERT INTO schema_migrations (version, name, checksum, applied_at) VALUES (99, 'future', 'x', 0)", []).unwrap();
    drop(conn);
    let err = EventStore::open(dir.path()).unwrap_err();
    assert!(
        matches!(
            err,
            Error::SchemaTooNew {
                found: 99,
                supported: 11
            }
        ),
        "{err}"
    );
    assert_eq!(modbit_event_store::migrations::rollback_plans().len(), 11);
}

#[test]
fn projections_follow_the_reducers_in_the_append_transaction_and_after_rebuild() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = EventStore::open(dir.path()).unwrap();
    let tenant = TenantId::new();
    let session = SessionId::new();
    let task = TaskId::new();
    let run = RunId::new();
    let turn = TurnId::new();
    let step = RunStepId::new();
    store
        .append(req(
            tenant,
            session,
            AggregateType::Session,
            *session.as_bytes(),
            vec![
                typed(
                    "SessionCreated",
                    &SessionEvent::SessionCreated {
                        tenant_id: tenant,
                        user_id: UserId::new(),
                        space_id: SpaceId::new(),
                    },
                ),
                typed(
                    "SessionFocusChanged",
                    &SessionEvent::SessionFocusChanged {
                        task_id: Some(task),
                    },
                ),
            ],
        ))
        .unwrap();
    store
        .append(req(
            tenant,
            session,
            AggregateType::Task,
            *task.as_bytes(),
            vec![
                typed(
                    "TaskCreated",
                    &TaskEvent::TaskCreated {
                        session_id: session,
                        goal_text: "g".into(),
                        workspace_id: WorkspaceId::new(),
                        workspace_root: None,
                        base_revision: Some("abc".into()),
                        execution_profile: "local_trusted".into(),
                        policy_profile_id: None,
                        origin: TaskOrigin::Desktop,
                    },
                ),
                typed("TaskQueued", &TaskEvent::TaskQueued),
                typed("TaskStarted", &TaskEvent::TaskStarted),
                typed(
                    "TaskWaiting",
                    &TaskEvent::TaskWaiting {
                        reason: WaitReason::Approval,
                    },
                ),
            ],
        ))
        .unwrap();
    store
        .append(req(
            tenant,
            session,
            AggregateType::Run,
            *run.as_bytes(),
            vec![
                typed(
                    "RunCreated",
                    &RunEvent::RunCreated {
                        task_id: task,
                        attempt: 1,
                        owner_location: OwnerLocation::Local,
                        kernel_lease_generation: 3,
                    },
                ),
                typed("RunStarted", &RunEvent::RunStarted),
            ],
        ))
        .unwrap();
    store
        .append(req(
            tenant,
            session,
            AggregateType::Turn,
            *turn.as_bytes(),
            vec![
                typed(
                    "TurnPrepared",
                    &TurnEvent::TurnPrepared {
                        run_id: run,
                        ordinal: 1,
                    },
                ),
                typed(
                    "ModelInvocationStarted",
                    &TurnEvent::ModelInvocationStarted {
                        model_route: json!({"m": 1}),
                    },
                ),
            ],
        ))
        .unwrap();
    store
        .append(req(
            tenant,
            session,
            AggregateType::RunStep,
            *step.as_bytes(),
            vec![
                typed(
                    "StepScheduled",
                    &StepEvent::StepScheduled {
                        turn_id: turn,
                        step_type: StepType::ToolCall,
                        ordinal: 1,
                        input_ref: Some("in".into()),
                    },
                ),
                typed("StepStarted", &StepEvent::StepStarted),
                typed(
                    "StepUnknownOutcome",
                    &StepEvent::StepUnknownOutcome {
                        reason: "lost".into(),
                    },
                ),
            ],
        ))
        .unwrap();

    let check = |store: &EventStore| {
        let s = store.session(&session).unwrap().unwrap();
        assert_eq!((s.generation, s.current_task_id), (2, Some(task)));
        let t = store.task(&task).unwrap().unwrap();
        assert_eq!(t.state, TaskState::Waiting(WaitReason::Approval));
        assert_eq!(t.generation, 4);
        assert!(t.started_at.is_some());
        let r = store.run(&run).unwrap().unwrap();
        assert_eq!(
            (r.state, r.kernel_lease_generation, r.attempt),
            (RunState::Running, 3, 1)
        );
        let tu = store.turn(&turn).unwrap().unwrap();
        assert_eq!(tu.state, TurnState::Streaming);
        assert_eq!(tu.model_route, Some(json!({"m": 1})));
        let st = store.step(&step).unwrap().unwrap();
        assert_eq!(st.state, StepState::UnknownOutcome);
        assert_eq!(st.failure_code.as_deref(), Some("UNKNOWN_OUTCOME:lost"));
        assert_eq!(store.projection_offset().unwrap(), 13);
    };
    check(&store);

    // An illegal transition in an append is rejected as a whole: no event, no row change.
    let err = store
        .append(req(
            tenant,
            session,
            AggregateType::Task,
            *task.as_bytes(),
            vec![
                typed("TaskSteered", &TaskEvent::TaskSteered { text: "ok".into() }),
                typed("TaskCompleted", &TaskEvent::TaskCompleted),
            ],
        ))
        .unwrap_err();
    assert!(matches!(err, Error::Projection { .. }), "{err}");
    assert_eq!(
        store.head(task.as_bytes()).unwrap().0,
        4,
        "atomic: the legal first event was rolled back with the illegal second"
    );
    check(&store);

    // Rebuild from the log reproduces identical rows.
    assert_eq!(store.rebuild_projections().unwrap(), 13);
    check(&store);
}

/// docs/19 "Compaction epochs" (M4.2): the session branch generation is the
/// durable key an asynchronous compaction is judged against. It only moves
/// forward, a stale generation is refused at the store, and it survives a
/// rebuild from the log.
#[test]
fn session_branch_generation_moves_forward_only_and_is_projected() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = EventStore::open(dir.path()).unwrap();
    let tenant = TenantId::new();
    let session = SessionId::new();
    store
        .append(req(
            tenant,
            session,
            AggregateType::Session,
            *session.as_bytes(),
            vec![typed(
                "SessionCreated",
                &SessionEvent::SessionCreated {
                    tenant_id: tenant,
                    user_id: UserId::new(),
                    space_id: SpaceId::new(),
                },
            )],
        ))
        .unwrap();
    assert_eq!(
        store.session(&session).unwrap().unwrap().branch_generation,
        0
    );
    store
        .append(req(
            tenant,
            session,
            AggregateType::Session,
            *session.as_bytes(),
            vec![typed(
                "SessionBranched",
                &SessionEvent::SessionBranched {
                    branch_generation: 1,
                    kind: "fork".into(),
                    reason: "forked at offset 3".into(),
                },
            )],
        ))
        .unwrap();
    assert_eq!(
        store.session(&session).unwrap().unwrap().branch_generation,
        1
    );
    // Not forward: refused, and nothing changes.
    let err = store
        .append(req(
            tenant,
            session,
            AggregateType::Session,
            *session.as_bytes(),
            vec![typed(
                "SessionBranched",
                &SessionEvent::SessionBranched {
                    branch_generation: 1,
                    kind: "revert".into(),
                    reason: "stale".into(),
                },
            )],
        ))
        .unwrap_err();
    assert!(matches!(err, Error::Projection { .. }), "{err}");
    assert_eq!(
        store.session(&session).unwrap().unwrap().branch_generation,
        1
    );
    store.rebuild_projections().unwrap();
    assert_eq!(
        store.session(&session).unwrap().unwrap().branch_generation,
        1
    );
}

#[test]
fn command_replay_is_idempotent_and_conflicting_reuse_is_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = EventStore::open(dir.path()).unwrap();
    let tenant = TenantId::new();
    let session = SessionId::new();
    let task = TaskId::new();
    store
        .append(req(
            tenant,
            session,
            AggregateType::Session,
            *session.as_bytes(),
            vec![typed(
                "SessionCreated",
                &SessionEvent::SessionCreated {
                    tenant_id: tenant,
                    user_id: UserId::new(),
                    space_id: SpaceId::new(),
                },
            )],
        ))
        .unwrap();
    let cmd = CommandRecord {
        command_id: EventId::new(),
        tenant_id: tenant,
        command_type: "CreateTask".into(),
        request_hash: "h1".into(),
    };
    let make = || {
        req(
            tenant,
            session,
            AggregateType::Task,
            *task.as_bytes(),
            vec![typed(
                "TaskCreated",
                &TaskEvent::TaskCreated {
                    session_id: session,
                    goal_text: "once".into(),
                    workspace_id: WorkspaceId::new(),
                    workspace_root: None,
                    base_revision: None,
                    execution_profile: "local_trusted".into(),
                    policy_profile_id: None,
                    origin: TaskOrigin::Cli,
                },
            )],
        )
    };
    let first = store.execute_command(cmd.clone(), make()).unwrap();
    let CommandOutcome::Applied(applied) = &first else {
        panic!("{first:?}")
    };
    assert_eq!(applied.len(), 1);

    // Transport retry: same id, same request -> replayed, nothing appended.
    let second = store.execute_command(cmd.clone(), make()).unwrap();
    let CommandOutcome::Replayed(replayed) = &second else {
        panic!("{second:?}")
    };
    assert_eq!(replayed, applied);
    assert_eq!(store.head(task.as_bytes()).unwrap().0, 1);
    assert_eq!(store.last_offset().unwrap(), 2);

    // Same id, different request -> conflict, nothing appended.
    let forged = CommandRecord {
        request_hash: "h2".into(),
        ..cmd.clone()
    };
    let err = store.execute_command(forged, make()).unwrap_err();
    assert!(matches!(err, Error::IdempotencyConflict { .. }), "{err}");
    assert_eq!(store.last_offset().unwrap(), 2);

    // Survives reopen.
    drop(store);
    let mut store = EventStore::open(dir.path()).unwrap();
    assert!(matches!(
        store.execute_command(cmd, make()).unwrap(),
        CommandOutcome::Replayed(_)
    ));
}

#[test]
fn concurrent_openers_of_a_fresh_database_all_succeed_and_migrate_once() {
    // Regression for the race fixed in M1.2: two processes opening a fresh
    // core.db at the same time must serialize the migration, not collide on
    // the ledger's unique version key.
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().to_path_buf();
    let handles: Vec<_> = (0..6)
        .map(|_| {
            let p = path.clone();
            std::thread::spawn(move || {
                EventStore::open_with_report(&p)
                    .map(|(_, r)| r)
                    .map_err(|e| e.to_string())
            })
        })
        .collect();
    let reports: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();
    for r in &reports {
        assert!(r.is_ok(), "{r:?}");
    }
    let applied_by = reports
        .iter()
        .filter(|r| !r.as_ref().unwrap().applied.is_empty())
        .count();
    assert_eq!(applied_by, 1, "exactly one opener applied the migrations");
    let conn = rusqlite::Connection::open(dir.path().join("core.db")).unwrap();
    let n: i64 = conn
        .query_row("SELECT count(*) FROM schema_migrations", [], |r| r.get(0))
        .unwrap();
    assert_eq!(n, 11);
}

#[test]
fn startup_recovery_verifies_chains_rebuilds_lagging_projections_and_bumps_boot_generation() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = EventStore::open(dir.path()).unwrap();
    let tenant = TenantId::new();
    let session = SessionId::new();
    store
        .append(req(
            tenant,
            session,
            AggregateType::Session,
            *session.as_bytes(),
            vec![typed(
                "SessionCreated",
                &SessionEvent::SessionCreated {
                    tenant_id: tenant,
                    user_id: UserId::new(),
                    space_id: SpaceId::new(),
                },
            )],
        ))
        .unwrap();
    let task = TaskId::new();
    store
        .append(req(
            tenant,
            session,
            AggregateType::Task,
            *task.as_bytes(),
            vec![
                typed(
                    "TaskCreated",
                    &TaskEvent::TaskCreated {
                        session_id: session,
                        goal_text: "r".into(),
                        workspace_id: WorkspaceId::new(),
                        workspace_root: None,
                        base_revision: None,
                        execution_profile: "local_trusted".into(),
                        policy_profile_id: None,
                        origin: TaskOrigin::Cli,
                    },
                ),
                typed("TaskQueued", &TaskEvent::TaskQueued),
            ],
        ))
        .unwrap();
    let r1 = store.recover_on_start().unwrap();
    assert_eq!(
        (
            r1.boot_generation,
            r1.events_verified,
            r1.aggregates_verified,
            r1.sessions,
            r1.tasks
        ),
        (1, 3, 2, 1, 1)
    );
    assert!(!r1.projections_rebuilt && r1.notes.is_empty(), "{r1:?}");
    drop(store);

    // Simulate a lagging projection cursor (e.g. an interrupted rebuild).
    let conn = rusqlite::Connection::open(dir.path().join("core.db")).unwrap();
    conn.execute("UPDATE projection_state SET last_offset = 1", [])
        .unwrap();
    conn.execute("DELETE FROM tasks", []).unwrap();
    drop(conn);
    let mut store = EventStore::open(dir.path()).unwrap();
    assert!(store.task(&task).unwrap().is_none());
    let r2 = store.recover_on_start().unwrap();
    assert_eq!(r2.boot_generation, 2);
    assert!(r2.projections_rebuilt);
    assert_eq!(r2.notes.len(), 1, "{:?}", r2.notes);
    assert!(
        store.task(&task).unwrap().is_some(),
        "projection rebuilt from the log"
    );
    assert_eq!(store.projection_offset().unwrap(), 3);

    // A tampered log fails recovery instead of serving corrupt state.
    drop(store);
    let conn = rusqlite::Connection::open(dir.path().join("core.db")).unwrap();
    conn.execute(
        "UPDATE events SET event_type = 'Forged' WHERE sequence = 2",
        [],
    )
    .unwrap();
    drop(conn);
    let mut store = EventStore::open(dir.path()).unwrap();
    assert!(matches!(
        store.recover_on_start().unwrap_err(),
        Error::Integrity { .. }
    ));
}
