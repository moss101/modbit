//! PX-113: the `memory_items` table is a projection of the `Memory`
//! aggregate's events. It is kept in the transaction that appends them,
//! rebuilds from the log to the same rows, drops a row the log does not
//! hold, and a legacy row (one written before memory mutations were events)
//! is put on the log exactly once.

use modbit_domain::event::{Actor, AggregateType};
use modbit_domain::{SessionId, TenantId};
use modbit_event_store::{AppendRequest, EventStore, MemoryRow, NewEvent, memory_aggregate_id};
use serde_json::json;

fn row(id: &str, status: &str, content: &str) -> MemoryRow {
    MemoryRow {
        id: id.into(),
        scope_key: "user:u1".into(),
        record_type: "fact".into(),
        topic: "t".into(),
        status: status.into(),
        sensitivity: "normal".into(),
        created_at_ms: 1,
        expires_at_ms: None,
        updated_at_ms: 1,
        doc: format!("{{\"status\":\"{status}\",\"content\":\"{content}\"}}"),
    }
}

fn append(
    store: &mut EventStore,
    session: SessionId,
    id: &str,
    kind: &str,
    payload: serde_json::Value,
) {
    store
        .append(AppendRequest {
            tenant_id: TenantId::new(),
            session_id: session,
            task_id: None,
            run_id: None,
            turn_id: None,
            step_id: None,
            aggregate_type: AggregateType::Memory,
            aggregate_id: memory_aggregate_id(id),
            expected_sequence: None,
            events: vec![NewEvent::new(kind, payload, Actor::Core("test".into()))],
        })
        .unwrap();
}

fn snapshot(store: &EventStore) -> Vec<(String, String, String)> {
    let mut v: Vec<_> = store
        .memory_all()
        .unwrap()
        .into_iter()
        .map(|r| (r.id, r.status, r.doc))
        .collect();
    v.sort();
    v
}

#[test]
fn memory_events_project_into_the_table_and_a_rebuild_replays_them_to_the_same_rows() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = EventStore::open(dir.path()).unwrap();
    let (s1, s2) = (SessionId::new(), SessionId::new());
    // Proposed on one session's log, promoted and forgotten on another's:
    // memory is cross-session, and the projection reads them all.
    append(
        &mut store,
        s1,
        "a",
        "MemoryProposed",
        json!({"memory_id": "a", "row": row("a", "proposed", "x")}),
    );
    append(
        &mut store,
        s1,
        "b",
        "MemoryProposed",
        json!({"memory_id": "b", "row": row("b", "proposed", "y")}),
    );
    append(
        &mut store,
        s2,
        "a",
        "MemoryPromoted",
        json!({"memory_id": "a", "row": row("a", "curated", "x")}),
    );
    append(
        &mut store,
        s2,
        "b",
        "MemoryForgotten",
        json!({"memory_id": "b", "removed": true, "mode": "delete"}),
    );
    let before = snapshot(&store);
    assert_eq!(before.len(), 1, "{before:?}");
    assert_eq!(
        (before[0].0.as_str(), before[0].1.as_str()),
        ("a", "curated")
    );
    // The ledger is readable across sessions, oldest first.
    let events = store.read_memory_events(0, 100).unwrap();
    let kinds: Vec<&str> = events
        .iter()
        .map(|e| e.envelope.event_type.as_str())
        .collect();
    assert_eq!(
        kinds,
        [
            "MemoryProposed",
            "MemoryProposed",
            "MemoryPromoted",
            "MemoryForgotten"
        ]
    );
    // A rebuild replays to the same table.
    store.rebuild_projections().unwrap();
    assert_eq!(snapshot(&store), before);
    // A row written straight into the table is not on the log: a rebuild
    // drops it, and a recovery that finds the cursor behind does too.
    store.memory_upsert(&row("rogue", "curated", "z")).unwrap();
    assert_eq!(snapshot(&store).len(), 2);
    store.rebuild_projections().unwrap();
    assert_eq!(snapshot(&store), before, "a direct write does not survive");
}

#[test]
fn a_legacy_row_is_imported_once_and_is_then_a_pure_projection() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = EventStore::open(dir.path()).unwrap();
    // A row from before mutations were events.
    store
        .memory_upsert(&row("legacy", "curated", "old"))
        .unwrap();
    let ledger = SessionId::from_bytes([0x4D; 16]);
    assert_eq!(
        store
            .backfill_legacy_memory(TenantId::new(), ledger)
            .unwrap(),
        1
    );
    assert_eq!(
        store
            .backfill_legacy_memory(TenantId::new(), ledger)
            .unwrap(),
        0,
        "once per database"
    );
    store.rebuild_projections().unwrap();
    assert_eq!(
        snapshot(&store).len(),
        1,
        "the imported row survives a rebuild"
    );
    // From now on a row the log does not hold is not memory.
    store
        .memory_upsert(&row("late-direct-write", "curated", "z"))
        .unwrap();
    assert_eq!(
        store
            .backfill_legacy_memory(TenantId::new(), ledger)
            .unwrap(),
        0
    );
    store.rebuild_projections().unwrap();
    let rows = snapshot(&store);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].0, "legacy");
}
