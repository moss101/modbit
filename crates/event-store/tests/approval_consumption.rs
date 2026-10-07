//! FIX-15: an approval authorizes one execution. The projection reports an
//! approval as consumed from the moment the call it is bound to is
//! dispatched — the same transaction that journals the dispatch — on a real
//! SQLite file, across a reopen and a full projection rebuild.

use modbit_domain::approval::{ApprovalEvent, ApprovalState};
use modbit_domain::event::{Actor, AggregateType};
use modbit_domain::toolcall::{EffectClass, ToolCallEvent};
use modbit_domain::{ApprovalId, SessionId, TaskId, TenantId, Timestamp, ToolCallId};
use modbit_event_store::{AppendRequest, EventStore, NewEvent};

fn typed<E: serde::Serialize>(t: &str, e: &E) -> NewEvent {
    NewEvent::new(
        t,
        serde_json::to_value(e).unwrap(),
        Actor::Core("test".into()),
    )
}

fn request(
    session: SessionId,
    task: TaskId,
    aggregate_type: AggregateType,
    id: [u8; 16],
    expected: Option<u64>,
    events: Vec<NewEvent>,
) -> AppendRequest {
    AppendRequest {
        tenant_id: TenantId::from_bytes([7; 16]),
        session_id: session,
        task_id: Some(task),
        run_id: None,
        turn_id: None,
        step_id: None,
        aggregate_type,
        aggregate_id: id,
        expected_sequence: expected,
        events,
    }
}

#[test]
fn an_approval_is_consumed_by_the_dispatch_of_its_call_and_stays_approved() {
    let dir = tempfile::tempdir().unwrap();
    let session = SessionId::new();
    let (task, call, approval) = (TaskId::new(), ToolCallId::new(), ApprovalId::new());
    let mut store = EventStore::open(dir.path()).unwrap();
    let call_events = |events| {
        request(
            session,
            task,
            AggregateType::ToolCall,
            *call.as_bytes(),
            None,
            events,
        )
    };
    store
        .append(call_events(vec![
            typed(
                "ToolCallProposed",
                &ToolCallEvent::ToolCallProposed {
                    task_id: task,
                    step_id: None,
                    tool_name: "git.worktree.close".into(),
                    tool_version: "1".into(),
                    effect_class: EffectClass::Destructive,
                    capability_lease_id: None,
                    arguments_hash: "intent".into(),
                    run_id: None,
                    turn_id: None,
                    call_id: None,
                    arguments_ref: None,
                },
            ),
            typed("ToolCallValidated", &ToolCallEvent::ToolCallValidated),
            typed(
                "ToolCallApprovalRequested",
                &ToolCallEvent::ToolCallApprovalRequested {
                    approval_id: approval,
                    decision: "APPROVAL_REQUIRED".into(),
                },
            ),
        ]))
        .unwrap();
    store
        .append(request(
            session,
            task,
            AggregateType::Approval,
            *approval.as_bytes(),
            Some(0),
            vec![
                typed(
                    "ApprovalRequested",
                    &ApprovalEvent::ApprovalRequested {
                        task_id: task,
                        tool_call_id: call,
                        tool_name: "git.worktree.close".into(),
                        effect_class: EffectClass::Destructive,
                        intent_hash: "intent".into(),
                        scope_json: "{}".into(),
                        expires_at: None,
                    },
                ),
                typed(
                    "ApprovalResolved",
                    &ApprovalEvent::ApprovalResolved {
                        approved: true,
                        resolver: "user".into(),
                        reason: "ok".into(),
                    },
                ),
            ],
        ))
        .unwrap();

    // Approved and not yet used: it authorizes the intent.
    let a = store.approval_for_call(&call).unwrap().unwrap();
    assert_eq!(a.state, ApprovalState::Approved);
    assert!(a.consumed_at.is_none() && !a.is_consumed());
    assert!(a.authorizes("intent", Timestamp(1)));

    // The dispatch journal appends the decision and the dispatch together.
    let generation = store.tool_call(&call).unwrap().unwrap().generation;
    store
        .append(request(
            session,
            task,
            AggregateType::ToolCall,
            *call.as_bytes(),
            Some(generation),
            vec![
                typed(
                    "ToolCallPolicyDecision",
                    &ToolCallEvent::ToolCallPolicyDecision {
                        allowed: true,
                        decision: format!("approval:{approval}"),
                        approval_required: false,
                    },
                ),
                typed("ToolCallDispatched", &ToolCallEvent::ToolCallDispatched),
            ],
        ))
        .unwrap();

    // Spent: still `Approved` (its resolution did not change), but it no
    // longer authorizes the same intent.
    for store in [&store, &EventStore::open(dir.path()).unwrap()] {
        let a = store.approval_for_call(&call).unwrap().unwrap();
        assert_eq!(a.state, ApprovalState::Approved);
        assert!(a.is_consumed(), "{a:?}");
        assert!(!a.authorizes("intent", Timestamp(1)));
        let by_id = store.approval(&approval).unwrap().unwrap();
        assert_eq!(by_id.consumed_at, a.consumed_at);
        let listed = store.approvals_for_session(&session).unwrap();
        assert!(listed.iter().all(|x| x.is_consumed()), "{listed:?}");
        let listed = store.approvals_for_task(&task).unwrap();
        assert!(listed.iter().all(|x| x.is_consumed()), "{listed:?}");
    }
    // The consumption is a fact of the log, not of one projection build.
    store.rebuild_projections().unwrap();
    assert!(
        store
            .approval_for_call(&call)
            .unwrap()
            .unwrap()
            .is_consumed()
    );
}
