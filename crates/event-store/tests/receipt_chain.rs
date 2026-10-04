//! VER-08 / FIX-08: the protected-effect receipt chain under concurrent
//! writers, on a real SQLite file (docs/23 "Protected-effect receipt chain").
//!
//! The chain is one global, linear sequence: every receipt's
//! `previous_receipt_hash` is the hash of the receipt immediately before it
//! (by `seq`), and no two receipts share a parent.

use std::collections::HashSet;
use std::sync::{Arc, Barrier, Mutex};

use modbit_domain::event::{Actor, AggregateType};
use modbit_domain::toolcall::{EffectClass, EffectReceipt, ToolCallEvent};
use modbit_domain::{EffectId, SessionId, TaskId, TenantId, Timestamp, ToolCallId};
use modbit_event_store::{AppendRequest, Error, EventStore, NewEvent};
use sha2::{Digest, Sha256};

const TENANT: [u8; 16] = [7; 16];

fn typed(t: &str, e: &ToolCallEvent) -> NewEvent {
    NewEvent::new(
        t,
        serde_json::to_value(e).unwrap(),
        Actor::Core("test".into()),
    )
}

fn call_request(
    session: SessionId,
    task: TaskId,
    call: ToolCallId,
    expected: Option<u64>,
    events: Vec<NewEvent>,
) -> AppendRequest {
    AppendRequest {
        tenant_id: TenantId::from_bytes(TENANT),
        session_id: session,
        task_id: Some(task),
        run_id: None,
        turn_id: None,
        step_id: None,
        aggregate_type: AggregateType::ToolCall,
        aggregate_id: *call.as_bytes(),
        expected_sequence: expected,
        events,
    }
}

/// Put a call on the log up to `Dispatched` (receipts attach to a dispatched
/// call), exactly as the dispatch journal does.
fn dispatch(store: &mut EventStore, session: SessionId, task: TaskId, call: ToolCallId) {
    // A loaded machine can make SQLite's writer lock time out; nothing is
    // written when it does, so the dispatch is simply tried again.
    for _ in 0..20 {
        match try_dispatch(store, session, task, call) {
            Err(Error::Sqlite(rusqlite::Error::SqliteFailure(e, _)))
                if e.code == rusqlite::ErrorCode::DatabaseBusy => {}
            other => return other.unwrap(),
        }
    }
    try_dispatch(store, session, task, call).unwrap();
}

fn try_dispatch(
    store: &mut EventStore,
    session: SessionId,
    task: TaskId,
    call: ToolCallId,
) -> Result<(), Error> {
    let events = vec![
        typed(
            "ToolCallProposed",
            &ToolCallEvent::ToolCallProposed {
                task_id: task,
                step_id: None,
                tool_name: "git.push".into(),
                tool_version: "1".into(),
                effect_class: EffectClass::ExternalSideEffect,
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
            "ToolCallPolicyDecision",
            &ToolCallEvent::ToolCallPolicyDecision {
                allowed: true,
                decision: "allow".into(),
                approval_required: false,
            },
        ),
        typed("ToolCallDispatched", &ToolCallEvent::ToolCallDispatched),
    ];
    store
        .append(call_request(session, task, call, None, events))
        .map(|_| ())
}

fn receipt(task: TaskId, call: ToolCallId, prev: Option<String>) -> EffectReceipt {
    seal(EffectReceipt {
        effect_id: EffectId::new(),
        previous_receipt_hash: prev,
        task_id: task,
        tool_call_id: call,
        capability_lease_id: None,
        intent_hash: "intent".into(),
        policy_decision: "allow".into(),
        approval_id: None,
        execution_target: "local".into(),
        evidence_ref: None,
        status: "SUCCESS".into(),
        occurred_at: Timestamp(1),
        reversibility: None,
        compensates: None,
        receipt_hash: String::new(),
    })
}

/// Same shape as `modbit_policy::ledger::seal` (sha256 over the receipt's
/// JSON minus its own hash); the store only needs *a* deterministic seal.
fn seal(mut r: EffectReceipt) -> EffectReceipt {
    let mut v = serde_json::to_value(&r).unwrap();
    v.as_object_mut().unwrap().remove("receipt_hash");
    r.receipt_hash = hex::encode(Sha256::digest(v.to_string().as_bytes()));
    r
}

fn receipt_event(r: EffectReceipt) -> NewEvent {
    typed(
        "EffectReceiptAppended",
        &ToolCallEvent::EffectReceiptAppended { receipt: r },
    )
}

fn assert_linear(store: &EventStore, expected: usize) {
    let chain = store.receipts(None).unwrap();
    assert_eq!(chain.len(), expected, "every receipt landed");
    let mut parents = HashSet::new();
    let mut prev: Option<&str> = None;
    for (i, r) in chain.iter().enumerate() {
        assert_eq!(
            r.previous_receipt_hash.as_deref(),
            prev,
            "receipt {i} must link to the receipt before it (a fork: two receipts share a parent)"
        );
        assert!(
            parents.insert(r.previous_receipt_hash.clone()),
            "receipt {i}: two receipts share the parent {:?}",
            r.previous_receipt_hash
        );
        prev = Some(r.receipt_hash.as_str());
    }
}

/// VER-08: the Core's protected-effect path, as it was written — read the
/// chain tail under one store lock, build and seal the receipt, append it
/// under another. Two effects that both read the tail before either appends
/// would fork the chain (on unmodified code this test fails with "receipt 1
/// must link to the receipt before it": both receipts carried `None`). The
/// barrier makes the interleaving certain instead of probable; it is the
/// interleaving two tasks produce on the multi-threaded runtime.
///
/// After FIX-08 the store refuses the loser at commit (`ReceiptChainStale`,
/// nothing written) so the chain stays linear, and the loser's retry over the
/// new tail lands as the second link.
#[test]
fn ver_08_tail_read_outside_the_append_transaction_cannot_fork_the_chain() {
    let dir = tempfile::tempdir().unwrap();
    let session = SessionId::new();
    let store = Arc::new(Mutex::new(EventStore::open(dir.path()).unwrap()));
    let barrier = Arc::new(Barrier::new(2));
    let mut handles = Vec::new();
    for _ in 0..2 {
        let (store, barrier) = (store.clone(), barrier.clone());
        handles.push(std::thread::spawn(move || {
            let (task, call) = (TaskId::new(), ToolCallId::new());
            dispatch(&mut store.lock().unwrap(), session, task, call);
            // Step 1: read the tail (store lock released at the end of the line).
            let prev = store.lock().unwrap().last_receipt_hash().unwrap();
            barrier.wait();
            // Step 2: append the receipt sealed over that tail.
            let append = |prev: Option<String>| {
                let mut st = store.lock().unwrap();
                let expected = st.tool_call(&call).unwrap().map(|c| c.generation);
                st.append(call_request(
                    session,
                    task,
                    call,
                    expected,
                    vec![receipt_event(receipt(task, call, prev))],
                ))
                .map(|_| ())
            };
            match append(prev) {
                Ok(()) => false,
                Err(Error::ReceiptChainStale { tail, .. }) => {
                    // Refused whole: nothing of the loser's receipt is on the log.
                    assert!(
                        store
                            .lock()
                            .unwrap()
                            .receipts(Some(&task))
                            .unwrap()
                            .is_empty()
                    );
                    append(tail).expect("a retry over the new tail lands");
                    true
                }
                Err(e) => panic!("unexpected: {e}"),
            }
        }));
    }
    let retried = handles
        .into_iter()
        .map(|h| h.join().unwrap())
        .filter(|r| *r)
        .count();
    assert_eq!(retried, 1, "exactly one writer lost the race");
    assert_linear(&store.lock().unwrap(), 2);
}

/// The store refuses any receipt whose parent is not the tail, whichever
/// append path carried it, and a refused batch writes nothing — not even its
/// other events.
#[test]
fn a_receipt_not_linked_to_the_tail_is_refused_and_the_whole_batch_rolls_back() {
    let dir = tempfile::tempdir().unwrap();
    let session = SessionId::new();
    let mut store = EventStore::open(dir.path()).unwrap();
    let (task, call) = (TaskId::new(), ToolCallId::new());
    dispatch(&mut store, session, task, call);
    let first = receipt(task, call, None);
    let g = store.tool_call(&call).unwrap().unwrap().generation;
    store
        .append(call_request(
            session,
            task,
            call,
            Some(g),
            vec![receipt_event(first.clone())],
        ))
        .unwrap();
    // Linked to nothing (a fork off the root) and linked to a made-up hash.
    for bad in [None, Some("f".repeat(64))] {
        let g = store.tool_call(&call).unwrap().unwrap().generation;
        let err = store
            .append(call_request(
                session,
                task,
                call,
                Some(g),
                vec![
                    typed(
                        "ToolCallSucceeded",
                        &ToolCallEvent::ToolCallSucceeded {
                            result_ref: "r".into(),
                        },
                    ),
                    receipt_event(receipt(task, call, bad)),
                ],
            ))
            .unwrap_err();
        assert!(matches!(err, Error::ReceiptChainStale { .. }), "{err}");
    }
    let g = store.tool_call(&call).unwrap().unwrap().generation;
    assert_eq!(
        store.tool_call(&call).unwrap().unwrap().state,
        modbit_domain::toolcall::ToolCallState::Dispatched,
        "the refused batch's other events rolled back with it"
    );
    assert_linear(&store, 1);
    // Linked to the tail: accepted, and two receipts in one batch chain in order.
    let second = receipt(task, call, Some(first.receipt_hash.clone()));
    let third = receipt(task, call, Some(second.receipt_hash.clone()));
    store
        .append(call_request(
            session,
            task,
            call,
            Some(g),
            vec![receipt_event(second), receipt_event(third)],
        ))
        .unwrap();
    assert_linear(&store, 3);
}

/// FIX-08: concurrent writers — separate connections to one SQLite file, the
/// way a second process or a second store handle sees it — each landing a
/// two-receipt batch (authorization, then result) through the chained
/// append. The tail is read inside the transaction, so the chain is one
/// linear sequence, nothing is refused, and every receipt is on it.
#[test]
fn concurrent_chained_appends_from_separate_connections_produce_one_linear_chain() {
    const WRITERS: usize = 6;
    const EFFECTS: usize = 4;
    let dir = tempfile::tempdir().unwrap();
    let session = SessionId::new();
    let path = dir.path().to_path_buf();
    // Open once up front so migrations are not part of the race.
    drop(EventStore::open(&path).unwrap());
    let barrier = Arc::new(Barrier::new(WRITERS));
    let handles: Vec<_> = (0..WRITERS)
        .map(|_| {
            let (path, barrier) = (path.clone(), barrier.clone());
            std::thread::spawn(move || {
                let mut store = EventStore::open(&path).unwrap();
                barrier.wait();
                for _ in 0..EFFECTS {
                    let (task, call) = (TaskId::new(), ToolCallId::new());
                    dispatch(&mut store, session, task, call);
                    let g = store.tool_call(&call).unwrap().unwrap().generation;
                    // A stale link on purpose: the store re-links and re-seals.
                    let stale = || receipt(task, call, Some("stale".into()));
                    let batch = || {
                        vec![call_request(
                            session,
                            task,
                            call,
                            Some(g),
                            vec![receipt_event(stale()), receipt_event(stale())],
                        )]
                    };
                    // SQLite's own writer lock can time out when the machine
                    // is loaded; nothing is written then, so it is retried.
                    // The chain, not the lock, is what is under test: the
                    // chain's tail is never the reason an append fails.
                    let mut attempts = 0;
                    loop {
                        match store.append_all_chained(batch(), None, seal) {
                            Ok(_) => break,
                            Err(Error::Sqlite(rusqlite::Error::SqliteFailure(e, _)))
                                if e.code == rusqlite::ErrorCode::DatabaseBusy && attempts < 20 =>
                            {
                                attempts += 1;
                            }
                            Err(e) => panic!("a chained append never loses the race: {e}"),
                        }
                    }
                }
            })
        })
        .collect();
    for h in handles {
        h.join().unwrap();
    }
    let store = EventStore::open(&path).unwrap();
    assert_linear(&store, WRITERS * EFFECTS * 2);
    for r in store.receipts(None).unwrap() {
        assert_eq!(
            r,
            seal(r.clone()),
            "the hash was re-sealed over the new link"
        );
    }
}
