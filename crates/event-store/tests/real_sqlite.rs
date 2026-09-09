//! Real-effect tests for the Event Store: real SQLite files on disk, a real
//! object directory, process reopen, a hard kill (SIGKILL / TerminateProcess)
//! of a writer process mid-append, and row-level tampering.
//!
//! These tests exercise the raw log with the `ToolCall` aggregate, whose
//! projection belongs to a later milestone, so payloads stay free-form.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use modbit_domain::event::{Actor, AggregateType, PayloadRef};
use modbit_domain::{SessionId, TaskId, TenantId};
use modbit_event_store::{AppendRequest, Error, EventStore, NewEvent};
use serde_json::json;

fn req(
    session: SessionId,
    agg: [u8; 16],
    expected: Option<u64>,
    events: Vec<NewEvent>,
) -> AppendRequest {
    AppendRequest {
        tenant_id: TenantId::from_bytes([7; 16]),
        session_id: session,
        task_id: None,
        run_id: None,
        turn_id: None,
        step_id: None,
        aggregate_type: AggregateType::ToolCall,
        aggregate_id: agg,
        expected_sequence: expected,
        events,
    }
}

fn ev(t: &str, payload: serde_json::Value) -> NewEvent {
    NewEvent::new(t, payload, Actor::Core("test".into()))
}

#[test]
fn append_read_chain_and_reopen_on_real_sqlite() {
    let dir = tempfile::tempdir().unwrap();
    let session = SessionId::new();
    let task = *TaskId::new().as_bytes();
    let mut store = EventStore::open(dir.path()).unwrap();
    let stored = store
        .append(req(
            session,
            task,
            Some(0),
            vec![
                ev("TaskCreated", json!({"goal": "x"})),
                ev("TaskQueued", json!({})),
            ],
        ))
        .unwrap();
    assert_eq!(
        stored
            .iter()
            .map(|s| s.envelope.sequence)
            .collect::<Vec<_>>(),
        vec![1, 2]
    );
    assert_eq!(
        stored.iter().map(|s| s.offset).collect::<Vec<_>>(),
        vec![1, 2]
    );
    assert_ne!(
        stored[0].envelope.integrity_hash,
        stored[1].envelope.integrity_hash
    );

    // Optimistic concurrency: a stale expectation is rejected and nothing is written.
    let err = store
        .append(req(
            session,
            task,
            Some(0),
            vec![ev("TaskStarted", json!({}))],
        ))
        .unwrap_err();
    assert!(
        matches!(
            err,
            Error::SequenceConflict {
                expected: 0,
                actual: 2,
                ..
            }
        ),
        "{err}"
    );
    assert_eq!(store.head(&task).unwrap().0, 2);

    // Large payloads go to the object store and verify on read.
    let big = json!({"blob": "z".repeat(100_000)});
    let s3 = store
        .append(req(
            session,
            task,
            Some(2),
            vec![ev("TaskSteered", big.clone())],
        ))
        .unwrap();
    assert!(matches!(s3[0].envelope.payload, PayloadRef::Object { .. }));
    assert_eq!(store.payload(&s3[0].envelope).unwrap(), big);
    assert_eq!(store.verify_aggregate(&task).unwrap(), 3);
    store.integrity_check().unwrap();
    drop(store);

    // Reopen from disk: everything is still there, in order, with the same hashes.
    let store = EventStore::open(dir.path()).unwrap();
    let events = store.read_aggregate(&task, 0, 100).unwrap();
    assert_eq!(events.len(), 3);
    assert_eq!(events[2].envelope.event_type, "TaskSteered");
    assert_eq!(
        events[0].envelope.integrity_hash,
        stored[0].envelope.integrity_hash
    );
    assert_eq!(store.verify_aggregate(&task).unwrap(), 3);
    assert!(dir.path().join("core.db").exists());
    assert!(dir.path().join("objects").is_dir());
}

#[test]
fn session_stream_resumes_from_offset_across_aggregates() {
    let dir = tempfile::tempdir().unwrap();
    let session = SessionId::new();
    let other_session = SessionId::new();
    let a = *TaskId::new().as_bytes();
    let b = *TaskId::new().as_bytes();
    let mut store = EventStore::open(dir.path()).unwrap();
    store
        .append(req(
            session,
            a,
            None,
            vec![ev("A1", json!({})), ev("A2", json!({}))],
        ))
        .unwrap();
    store
        .append(req(other_session, b, None, vec![ev("OTHER", json!({}))]))
        .unwrap();
    store
        .append(req(session, b, None, vec![ev("B1", json!({}))]))
        .unwrap();
    store
        .append(req(session, a, None, vec![ev("A3", json!({}))]))
        .unwrap();

    let first = store.read_session(&session, 0, 2).unwrap();
    assert_eq!(
        first
            .iter()
            .map(|e| e.envelope.event_type.as_str())
            .collect::<Vec<_>>(),
        ["A1", "A2"]
    );
    let cursor = first.last().unwrap().offset;
    let rest = store.read_session(&session, cursor, 100).unwrap();
    assert_eq!(
        rest.iter()
            .map(|e| e.envelope.event_type.as_str())
            .collect::<Vec<_>>(),
        ["B1", "A3"]
    );
    let all: Vec<_> = store.read_session(&session, 0, 100).unwrap();
    assert_eq!(all.len(), 4, "other session's events are excluded");
    assert_eq!(store.last_offset().unwrap(), 5);
}

#[test]
fn tampered_row_fails_verification() {
    let dir = tempfile::tempdir().unwrap();
    let session = SessionId::new();
    let task = *TaskId::new().as_bytes();
    let mut store = EventStore::open(dir.path()).unwrap();
    store
        .append(req(
            session,
            task,
            None,
            vec![
                ev("TaskCreated", json!({"goal": "honest"})),
                ev("TaskQueued", json!({})),
            ],
        ))
        .unwrap();
    drop(store);
    let conn = rusqlite::Connection::open(dir.path().join("core.db")).unwrap();
    conn.execute(
        "UPDATE events SET payload_inline = '{\"goal\":\"forged\"}' WHERE sequence = 1",
        [],
    )
    .unwrap();
    drop(conn);
    let store = EventStore::open(dir.path()).unwrap();
    let err = store.verify_aggregate(&task).unwrap_err();
    assert!(matches!(err, Error::Integrity { sequence: 1, .. }), "{err}");

    // A deleted row leaves a gap the chain check reports too.
    let conn = rusqlite::Connection::open(dir.path().join("core.db")).unwrap();
    conn.execute(
        "UPDATE events SET payload_inline = '{\"goal\":\"honest\"}' WHERE sequence = 1",
        [],
    )
    .unwrap();
    conn.execute("DELETE FROM events WHERE sequence = 1", [])
        .unwrap();
    drop(conn);
    let err = store.verify_aggregate(&task).unwrap_err();
    assert!(matches!(err, Error::Integrity { sequence: 2, .. }), "{err}");
}

#[test]
fn newer_schema_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    EventStore::open(dir.path()).unwrap();
    let conn = rusqlite::Connection::open(dir.path().join("core.db")).unwrap();
    conn.execute(
        "INSERT INTO schema_migrations (version, name, checksum, applied_at) VALUES (99, 'future', 'x', 0)",
        [],
    )
    .unwrap();
    drop(conn);
    let err = EventStore::open(dir.path()).unwrap_err();
    assert!(
        matches!(
            err,
            Error::SchemaTooNew {
                found: 99,
                supported: 3
            }
        ),
        "{err}"
    );
}

/// Writer role for the kill test: appends forever, one transaction per event.
fn writer_role(dir: &Path) -> ! {
    let session = SessionId::from_bytes([1; 16]);
    let task = [2u8; 16];
    let mut store = EventStore::open(dir).unwrap();
    let mut i = 0u64;
    loop {
        i += 1;
        store
            .append(req(
                session,
                task,
                None,
                vec![ev("Tick", json!({"i": i, "pad": "p".repeat(2000)}))],
            ))
            .unwrap();
    }
}

#[test]
fn hard_killed_writer_leaves_a_verifiable_prefix_and_no_torn_event() {
    if let Ok(dir) = std::env::var("MODBIT_EVENT_STORE_WRITER_DIR") {
        writer_role(Path::new(&dir));
    }
    let dir = tempfile::tempdir().unwrap();
    let exe: PathBuf = std::env::current_exe().unwrap();
    let mut child = Command::new(&exe)
        .args([
            "--exact",
            "hard_killed_writer_leaves_a_verifiable_prefix_and_no_torn_event",
            "--nocapture",
        ])
        .env("MODBIT_EVENT_STORE_WRITER_DIR", dir.path())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .unwrap();

    // Let it commit a meaningful number of events, then kill it mid-flight.
    let task = [2u8; 16];
    let deadline = Instant::now() + Duration::from_secs(30);
    let mut before_kill = 0;
    while Instant::now() < deadline {
        if let Ok(store) = EventStore::open(dir.path()) {
            before_kill = store.head(&task).unwrap().0;
            if before_kill >= 200 {
                break;
            }
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(
        before_kill >= 200,
        "writer produced only {before_kill} events in 30s"
    );
    child.kill().unwrap();
    let status = child.wait().unwrap();
    assert!(
        !status.success(),
        "child must have died from the kill, not exited cleanly"
    );

    // Recovery: the database opens, SQLite reports ok, and the hash chain is a
    // contiguous prefix of what the writer committed.
    let store = EventStore::open(dir.path()).unwrap();
    store.integrity_check().unwrap();
    let after = store.head(&task).unwrap().0;
    assert!(
        after >= before_kill,
        "committed events were lost: {after} < {before_kill}"
    );
    let verified = store.verify_aggregate(&task).unwrap();
    assert_eq!(verified, after);
    let last = store.read_aggregate(&task, after - 1, 1).unwrap().remove(0);
    assert_eq!(
        store.payload(&last.envelope).unwrap()["i"],
        json!(after),
        "last row is a whole event, not a torn one"
    );
}
