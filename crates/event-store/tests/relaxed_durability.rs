//! DR-PX-2026-10-21-015: streamed deltas commit with `synchronous=NORMAL`
//! (`Durability::Relaxed`) and nothing else does. On a real SQLite file:
//! relaxed is refused for anything but `AssistantTextDelta` events of an
//! `AssistantStream` aggregate and writes nothing; the connection is back at
//! FULL after every append, whatever its outcome; and when the unsynced tail
//! of the WAL is lost (a power loss, emulated by cutting the WAL back to
//! every length it had during the run, and to a torn byte in between) the
//! store opens, every aggregate's hash chain verifies, the lost part is only
//! a suffix of deltas, and a FULL commit made after a relaxed one has
//! hardened it.

use std::path::Path;

use modbit_domain::event::{Actor, AggregateType};
use modbit_domain::stream::{Retention, StreamEvent, StreamKind, TextRef};
use modbit_domain::{RunId, RunStepId, SessionId, StreamId, TaskId, TenantId, TurnId};
use modbit_event_store::{AppendRequest, Durability, Error, EventStore, NewEvent};

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

fn request(
    session: SessionId,
    aggregate_type: AggregateType,
    aggregate: StreamId,
    events: &[StreamEvent],
) -> AppendRequest {
    AppendRequest {
        tenant_id: TenantId::from_bytes([7; 16]),
        session_id: session,
        task_id: Some(TaskId::new()),
        run_id: None,
        turn_id: None,
        step_id: None,
        aggregate_type,
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
    }
}

fn wal_len(dir: &Path) -> u64 {
    std::fs::metadata(dir.join("core.db-wal")).map_or(0, |m| m.len())
}

const FULL: i64 = 2;

#[test]
fn relaxed_is_for_streamed_deltas_only_and_the_connection_is_full_again_after_every_append() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = EventStore::open(dir.path()).unwrap();
    let session = SessionId::new();
    let id = StreamId::new();
    assert_eq!(store.synchronous_level().unwrap(), FULL);

    store
        .append_with(
            request(
                session,
                AggregateType::AssistantStream,
                id,
                &[delta(id, 1, "he")],
            ),
            Durability::Relaxed,
        )
        .unwrap();
    assert_eq!(
        store.synchronous_level().unwrap(),
        FULL,
        "after a relaxed append"
    );

    let head = store.last_offset().unwrap();
    let refusals = [
        // a completion record is authoritative
        request(
            session,
            AggregateType::AssistantStream,
            id,
            &[completed(id, 1)],
        ),
        // a delta that is not on an assistant stream aggregate
        request(session, AggregateType::Memory, id, &[delta(id, 2, "x")]),
        // a delta riding with a completion
        request(
            session,
            AggregateType::AssistantStream,
            id,
            &[delta(id, 2, "x"), completed(id, 2)],
        ),
        // nothing at all
        request(session, AggregateType::AssistantStream, id, &[]),
    ];
    for req in refusals {
        match store.append_with(req, Durability::Relaxed) {
            Err(Error::RelaxedRefused { .. }) => {}
            other => panic!("expected RelaxedRefused, got {other:?}"),
        }
        assert_eq!(store.synchronous_level().unwrap(), FULL, "after a refusal");
    }
    assert_eq!(
        store.last_offset().unwrap(),
        head,
        "a refusal writes nothing"
    );

    // A relaxed append the stream rules themselves refuse (out of order) also
    // leaves the connection at FULL and writes nothing.
    let bad = store.append_with(
        request(
            session,
            AggregateType::AssistantStream,
            id,
            &[delta(id, 9, "x")],
        ),
        Durability::Relaxed,
    );
    assert!(bad.is_err());
    assert_eq!(
        store.synchronous_level().unwrap(),
        FULL,
        "after a failed append"
    );
    assert_eq!(store.last_offset().unwrap(), head);

    // The same completion at full durability is accepted.
    store
        .append_with(
            request(
                session,
                AggregateType::AssistantStream,
                id,
                &[completed(id, 1)],
            ),
            Durability::Full,
        )
        .unwrap();
}

/// Copy the database and a WAL cut to `wal_bytes` into a fresh directory, as
/// the disk would hold them if everything the OS had not synced were lost.
fn lost_power_at(dir: &Path, wal_bytes: u64) -> tempfile::TempDir {
    let out = tempfile::tempdir().unwrap();
    std::fs::copy(dir.join("core.db"), out.path().join("core.db")).unwrap();
    let wal = std::fs::read(dir.join("core.db-wal")).unwrap();
    let cut = usize::try_from(wal_bytes).unwrap().min(wal.len());
    std::fs::write(out.path().join("core.db-wal"), &wal[..cut]).unwrap();
    out
}

#[test]
fn a_lost_wal_tail_costs_only_a_suffix_of_deltas_and_a_full_commit_hardens_the_ones_before_it() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = EventStore::open(dir.path()).unwrap();
    let session = SessionId::new();
    let (first, second) = (StreamId::new(), StreamId::new());

    // (length of the WAL after the step, what the step was)
    let mut marks: Vec<(u64, &str)> = vec![(wal_len(dir.path()), "open")];
    let relaxed = |store: &mut EventStore, id: StreamId, n: u64| {
        store
            .append_with(
                request(
                    session,
                    AggregateType::AssistantStream,
                    id,
                    &[delta(id, n, "alpha beta gamma ")],
                ),
                Durability::Relaxed,
            )
            .unwrap();
    };
    for n in 1..=3 {
        relaxed(&mut store, first, n);
        marks.push((wal_len(dir.path()), "first-delta"));
    }
    store
        .append_with(
            request(
                session,
                AggregateType::AssistantStream,
                first,
                &[completed(first, 3)],
            ),
            Durability::Full,
        )
        .unwrap();
    let hardened_at = wal_len(dir.path());
    marks.push((hardened_at, "first-completed"));
    for n in 1..=3 {
        relaxed(&mut store, second, n);
        marks.push((wal_len(dir.path()), "second-delta"));
    }
    assert!(
        marks.windows(2).all(|w| w[0].0 <= w[1].0),
        "the WAL only grows during the run: {marks:?}"
    );

    // Every length the WAL had, and a torn byte past each.
    let mut cuts: Vec<u64> = marks.iter().map(|m| m.0).collect();
    cuts.extend(marks.iter().map(|m| m.0 + 7));
    cuts.sort_unstable();
    cuts.dedup();
    let (mut last_first, mut last_second) = (0usize, 0usize);
    for cut in cuts {
        let copy = lost_power_at(dir.path(), cut);
        let reopened = EventStore::open(copy.path())
            .unwrap_or_else(|e| panic!("the store did not open with the WAL cut to {cut}: {e}"));
        let count = |id: StreamId| {
            let events = reopened
                .read_aggregate(id.as_bytes(), 0, usize::MAX)
                .unwrap();
            // the chain verifies and the sequence is contiguous from 1
            assert_eq!(
                reopened.verify_aggregate(id.as_bytes()).unwrap(),
                events.len() as u64,
                "WAL cut to {cut}"
            );
            events
        };
        let first_events = count(first);
        let second_events = count(second);
        // the lost part is a suffix: more WAL never shows fewer events
        assert!(first_events.len() >= last_first, "WAL cut to {cut}");
        assert!(second_events.len() >= last_second, "WAL cut to {cut}");
        (last_first, last_second) = (first_events.len(), second_events.len());
        // the completion, a FULL commit, carries the three relaxed deltas before it
        if cut >= hardened_at {
            assert_eq!(
                first_events.len(),
                4,
                "WAL cut to {cut}: three deltas and the completion"
            );
            assert_eq!(
                first_events.last().unwrap().envelope.event_type,
                "AssistantMessageCompleted"
            );
        }
        // a stream with deltas and no closing record is what recovery closes
        let open = reopened.open_streams().unwrap();
        assert_eq!(
            open.contains(second.as_bytes()),
            !second_events.is_empty(),
            "WAL cut to {cut}"
        );
        assert_eq!(
            open.contains(first.as_bytes()),
            (1..=3).contains(&first_events.len()),
            "the first stream is open exactly while it has deltas and no completion: WAL cut to {cut}"
        );
        // an event is either whole or absent
        for e in first_events.iter().chain(&second_events) {
            assert!(!e.envelope.integrity_hash.is_empty());
        }
    }
    assert_eq!(
        (last_first, last_second),
        (4, 3),
        "the uncut WAL has everything"
    );
}
