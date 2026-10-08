//! PX-041 store rules on a real SQLite file: an assistant stream's events are
//! refused by the store itself when a delta is out of sequence, names another
//! stream, is over the size bound, or follows the record that closed the
//! stream; a refused append writes nothing; open streams are found by the
//! recovery query; a reopened store verifies the stream's chain.

use modbit_domain::event::{Actor, AggregateType};
use modbit_domain::stream::{
    AbortSource, MAX_DELTA_BYTES, Retention, StreamEvent, StreamKind, TextRef,
};
use modbit_domain::{RunId, RunStepId, SessionId, StreamId, TaskId, TenantId, TurnId};
use modbit_event_store::{AppendRequest, Error, EventStore, NewEvent};

fn delta(id: StreamId, sequence: u64, text: &str) -> StreamEvent {
    StreamEvent::AssistantTextDelta {
        stream_id: id,
        kind: StreamKind::Text,
        sequence,
        run_id: RunId::new(),
        turn_id: TurnId::new(),
        step_id: RunStepId::new(),
        text: text.into(),
        provenance: "MODEL_OUTPUT".into(),
        retention: Retention::CollapsibleOnComplete,
    }
}

fn completed(id: StreamId, deltas: u64) -> StreamEvent {
    StreamEvent::AssistantMessageCompleted {
        stream_id: id,
        kind: StreamKind::Text,
        delta_count: deltas,
        text_ref: TextRef {
            object_hash: "00".into(),
            byte_length: 1,
        },
        content_hash: "00".into(),
    }
}

fn aborted(id: StreamId, deltas: u64) -> StreamEvent {
    StreamEvent::AssistantMessageAborted {
        stream_id: id,
        kind: StreamKind::Text,
        source: AbortSource::Recovery,
        code: "ABORTED_BY_RECOVERY".into(),
        reason: "restart".into(),
        delta_count: deltas,
        bytes_streamed: 1,
    }
}

fn append(
    store: &mut EventStore,
    session: SessionId,
    aggregate: StreamId,
    events: &[StreamEvent],
) -> Result<usize, Error> {
    store
        .append(AppendRequest {
            tenant_id: TenantId::from_bytes([7; 16]),
            session_id: session,
            task_id: Some(TaskId::new()),
            run_id: None,
            turn_id: None,
            step_id: None,
            aggregate_type: AggregateType::AssistantStream,
            aggregate_id: *aggregate.as_bytes(),
            expected_sequence: None,
            events: events
                .iter()
                .map(|e| {
                    NewEvent::new(
                        e.event_type(),
                        serde_json::to_value(e).unwrap(),
                        Actor::Core("test".into()),
                    )
                })
                .collect(),
        })
        .map(|s| s.len())
}

fn refused(r: Result<usize, Error>) -> String {
    match r {
        Err(Error::Projection { detail, .. }) => detail,
        other => panic!("expected the store to refuse, got {other:?}"),
    }
}

#[test]
fn the_store_refuses_a_delta_that_breaks_the_stream_and_writes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = EventStore::open(dir.path()).unwrap();
    let session = SessionId::new();
    let id = StreamId::new();
    assert_eq!(
        append(&mut store, session, id, &[delta(id, 1, "he")]).unwrap(),
        1
    );
    assert_eq!(
        append(&mut store, session, id, &[delta(id, 2, "llo")]).unwrap(),
        1
    );
    let head = store.last_offset().unwrap();

    // out of sequence, repeated, from the future
    assert!(
        refused(append(&mut store, session, id, &[delta(id, 4, "x")])).contains("out of order")
    );
    assert!(
        refused(append(&mut store, session, id, &[delta(id, 2, "x")])).contains("out of order")
    );
    // over the byte bound
    let big = "x".repeat(MAX_DELTA_BYTES + 1);
    assert!(refused(append(&mut store, session, id, &[delta(id, 3, &big)])).contains("bound"));
    // empty
    assert!(refused(append(&mut store, session, id, &[delta(id, 3, "")])).contains("carries text"));
    // naming a stream other than the aggregate it is appended to
    let other = StreamId::new();
    assert!(
        refused(append(&mut store, session, id, &[delta(other, 3, "x")])).contains("other than")
    );
    // a completion that miscounts
    assert!(refused(append(&mut store, session, id, &[completed(id, 5)])).contains("counts"));
    // nothing was written by any refusal
    assert_eq!(store.last_offset().unwrap(), head);
    assert_eq!(
        store.read_aggregate(id.as_bytes(), 0, 100).unwrap().len(),
        2
    );

    // The completion closes the stream; nothing follows it.
    assert_eq!(
        append(&mut store, session, id, &[completed(id, 2)]).unwrap(),
        1
    );
    assert!(refused(append(&mut store, session, id, &[delta(id, 4, "late")])).contains("closed"));
    assert!(refused(append(&mut store, session, id, &[aborted(id, 2)])).contains("closed"));
    // A stream cannot complete before it has a delta.
    let empty = StreamId::new();
    assert!(
        refused(append(&mut store, session, empty, &[completed(empty, 0)])).contains("first delta")
    );
    store.integrity_check().unwrap();
}

#[test]
fn open_streams_are_found_for_recovery_and_closing_removes_them() {
    let dir = tempfile::tempdir().unwrap();
    let session = SessionId::new();
    let (open, done, cut) = (StreamId::new(), StreamId::new(), StreamId::new());
    {
        let mut store = EventStore::open(dir.path()).unwrap();
        append(
            &mut store,
            session,
            open,
            &[delta(open, 1, "a"), delta(open, 2, "b")],
        )
        .unwrap();
        append(
            &mut store,
            session,
            done,
            &[delta(done, 1, "a"), completed(done, 1)],
        )
        .unwrap();
        append(
            &mut store,
            session,
            cut,
            &[delta(cut, 1, "a"), aborted(cut, 1)],
        )
        .unwrap();
        assert_eq!(store.open_streams().unwrap(), vec![*open.as_bytes()]);
    }
    // A reopened store (the Core after a kill) sees the same, verifies every
    // chain, and the abort record closes the stream for good.
    let mut store = EventStore::open(dir.path()).unwrap();
    store.recover_on_start().unwrap();
    assert_eq!(store.open_streams().unwrap(), vec![*open.as_bytes()]);
    append(&mut store, session, open, &[aborted(open, 2)]).unwrap();
    assert!(store.open_streams().unwrap().is_empty());
    assert_eq!(store.verify_aggregate(open.as_bytes()).unwrap(), 3);
    // Projections rebuild from the log without objecting to any stream.
    store.rebuild_projections().unwrap();
}
