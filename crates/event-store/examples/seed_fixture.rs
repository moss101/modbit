//! Produces a small production-like `core.db` for migration tests
//! (docs/31 "Migration safety": migrations run against a copied fixture DB).
//!
//! Usage: `cargo run -p modbit-event-store --example seed_fixture -- <out-dir>`
//! Writes `<out-dir>/core.db` with one session, one task and one run.

use modbit_domain::event::{Actor, AggregateType};
use modbit_domain::{SessionId, TaskId, TenantId};
use modbit_event_store::{AppendRequest, EventStore, NewEvent};
use serde_json::json;

fn main() {
    let out = std::env::args().nth(1).expect("out dir");
    let dir = std::path::Path::new(&out);
    let mut store = EventStore::open(dir).expect("open");
    let tenant = TenantId::from_bytes([0xA1; 16]);
    let session = SessionId::from_bytes([0xB2; 16]);
    let task = TaskId::from_bytes([0xC3; 16]);
    let base = |agg: AggregateType, id: [u8; 16], events: Vec<NewEvent>| AppendRequest {
        tenant_id: tenant,
        session_id: session,
        task_id: Some(task),
        run_id: None,
        turn_id: None,
        step_id: None,
        aggregate_type: agg,
        aggregate_id: id,
        expected_sequence: None,
        events,
    };
    let actor = Actor::Core("seed".into());
    store
        .append(base(
            AggregateType::Session,
            *session.as_bytes(),
            vec![NewEvent::new(
                "SessionCreated",
                json!({"event_type": "SessionCreated", "tenant_id": tenant.to_string(), "user_id": "018f0000-0000-7000-8000-000000000001", "space_id": "018f0000-0000-7000-8000-000000000002"}),
                actor.clone(),
            )],
        ))
        .expect("session");
    store
        .append(base(
            AggregateType::Task,
            *task.as_bytes(),
            vec![
                NewEvent::new(
                    "TaskCreated",
                    json!({"event_type": "TaskCreated", "session_id": session.to_string(), "goal_text": "seed fixture", "workspace_id": "018f0000-0000-7000-8000-000000000003", "base_revision": null, "execution_profile": "local_trusted", "policy_profile_id": null, "origin": "cli"}),
                    actor.clone(),
                ),
                NewEvent::new("TaskQueued", json!({"event_type": "TaskQueued"}), actor.clone()),
                NewEvent::new("TaskStarted", json!({"event_type": "TaskStarted"}), actor),
            ],
        ))
        .expect("task");
    println!("wrote {}", dir.join("core.db").display());
}
