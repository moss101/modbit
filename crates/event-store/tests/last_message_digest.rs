//! PX-218 on a real SQLite file: the task digest names how the assistant's
//! latest FINISHED text message ended, read from the completion record by the
//! database. A stream that is open, one that was aborted and a reasoning
//! summary contribute nothing; an archived task's line is the one it had when
//! it was put away; a record written before the field existed is named with
//! no line (its text object holds it); the answer is the same after the store
//! is closed and opened again.

use modbit_domain::conversation::{self, ConversationEvent};
use modbit_domain::event::{Actor, AggregateType};
use modbit_domain::stream::{AbortSource, Retention, StreamEvent, StreamKind, TextRef, last_line};
use modbit_domain::{RunId, RunStepId, SessionId, StreamId, TaskId, TenantId, TurnId};
use modbit_event_store::digest::LastMessage;
use modbit_event_store::{AppendRequest, EventStore, NewEvent};

const TENANT: TenantId = TenantId::from_bytes([7; 16]);

fn new_event(event_type: &str, payload: serde_json::Value) -> NewEvent {
    NewEvent::new(event_type, payload, Actor::Core("test".into()))
}

fn append(
    store: &mut EventStore,
    session: SessionId,
    task: TaskId,
    aggregate_type: AggregateType,
    aggregate_id: [u8; 16],
    events: Vec<NewEvent>,
) -> u64 {
    store
        .append(AppendRequest {
            tenant_id: TENANT,
            session_id: session,
            task_id: Some(task),
            run_id: None,
            turn_id: None,
            step_id: None,
            aggregate_type,
            aggregate_id,
            expected_sequence: None,
            events,
        })
        .unwrap()
        .last()
        .unwrap()
        .offset
}

fn delta(id: StreamId, kind: StreamKind, text: &str) -> NewEvent {
    let e = StreamEvent::AssistantTextDelta {
        stream_id: id,
        kind,
        sequence: 1,
        run_id: RunId::new(),
        turn_id: TurnId::new(),
        step_id: RunStepId::new(),
        text: text.into(),
        provenance: "MODEL_OUTPUT".into(),
        retention: Retention::CollapsibleOnComplete,
    };
    new_event(e.event_type(), serde_json::to_value(&e).unwrap())
}

/// One whole stream of `kind` holding `text`, closed as completed. Returns
/// the offset of its completion record.
fn finished(
    store: &mut EventStore,
    session: SessionId,
    task: TaskId,
    kind: StreamKind,
    text: &str,
) -> u64 {
    let id = StreamId::new();
    let hash = store.objects().put(text.as_bytes()).unwrap();
    let done = StreamEvent::AssistantMessageCompleted {
        stream_id: id,
        kind,
        delta_count: 1,
        text_ref: TextRef {
            object_hash: hash,
            byte_length: text.len() as u64,
        },
        content_hash: "00".into(),
        last_line: Some(last_line(text)),
    };
    append(
        store,
        session,
        task,
        AggregateType::AssistantStream,
        *id.as_bytes(),
        vec![
            delta(id, kind, text),
            new_event(done.event_type(), serde_json::to_value(&done).unwrap()),
        ],
    )
}

fn message_of(store: &EventStore, session: SessionId, task: TaskId) -> Option<LastMessage> {
    store
        .task_digests(&session)
        .unwrap()
        .into_iter()
        .find(|d| d.task_id == task)
        .and_then(|d| d.last_message)
}

fn line_of(store: &EventStore, session: SessionId, task: TaskId) -> Option<String> {
    message_of(store, session, task).and_then(|m| m.last_line)
}

fn curate(store: &mut EventStore, session: SessionId, task: TaskId, e: &ConversationEvent) -> u64 {
    append(
        store,
        session,
        task,
        AggregateType::Conversation,
        conversation::aggregate_id(task),
        vec![new_event(e.event_type(), serde_json::to_value(e).unwrap())],
    )
}

#[test]
fn the_digest_names_the_last_line_of_the_latest_finished_text_message_only() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = EventStore::open(dir.path()).unwrap();
    let session = SessionId::new();
    let (task, quiet) = (TaskId::new(), TaskId::new());

    // A task with events and no answer has no message; a task with a stream
    // still open has none either.
    let open = StreamId::new();
    append(
        &mut store,
        session,
        quiet,
        AggregateType::AssistantStream,
        *open.as_bytes(),
        vec![delta(
            open,
            StreamKind::Text,
            "still being written\nline two",
        )],
    );
    assert_eq!(message_of(&store, session, quiet), None);

    finished(
        &mut store,
        session,
        task,
        StreamKind::Text,
        "First I read the file.\n\nThe parser is in src/parse.rs.\n",
    );
    assert_eq!(
        line_of(&store, session, task).as_deref(),
        Some("The parser is in src/parse.rs.")
    );
    // The other task is untouched by it.
    assert_eq!(message_of(&store, session, quiet), None);

    // A later message replaces it.
    finished(
        &mut store,
        session,
        task,
        StreamKind::Text,
        "Done.\nAll 12 tests pass.",
    );
    assert_eq!(
        line_of(&store, session, task).as_deref(),
        Some("All 12 tests pass.")
    );

    // A later reasoning summary is not a message of the assistant's.
    finished(
        &mut store,
        session,
        task,
        StreamKind::Reasoning,
        "weighing the options\nprivate conclusion",
    );
    // A later stream that was cut off, and one still open, are not finished.
    let cut = StreamId::new();
    let aborted = StreamEvent::AssistantMessageAborted {
        stream_id: cut,
        kind: StreamKind::Text,
        source: AbortSource::UserInterrupt,
        code: "INTERRUPTED".into(),
        reason: "stopped".into(),
        delta_count: 1,
        bytes_streamed: 9,
    };
    append(
        &mut store,
        session,
        task,
        AggregateType::AssistantStream,
        *cut.as_bytes(),
        vec![
            delta(cut, StreamKind::Text, "half a tho"),
            new_event(
                aborted.event_type(),
                serde_json::to_value(&aborted).unwrap(),
            ),
        ],
    );
    let streaming = StreamId::new();
    append(
        &mut store,
        session,
        task,
        AggregateType::AssistantStream,
        *streaming.as_bytes(),
        vec![delta(streaming, StreamKind::Text, "not finished yet\n")],
    );
    assert_eq!(
        line_of(&store, session, task).as_deref(),
        Some("All 12 tests pass.")
    );

    // The same after the store is closed and opened again: it is read from
    // the log, not kept anywhere.
    drop(store);
    let store = EventStore::open(dir.path()).unwrap();
    assert_eq!(
        line_of(&store, session, task).as_deref(),
        Some("All 12 tests pass.")
    );
    assert_eq!(message_of(&store, session, quiet), None);
}

#[test]
fn an_archived_tasks_line_is_the_one_it_had_when_it_was_put_away() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = EventStore::open(dir.path()).unwrap();
    let session = SessionId::new();
    let (task, never) = (TaskId::new(), TaskId::new());

    finished(&mut store, session, task, StreamKind::Text, "before");
    curate(
        &mut store,
        session,
        task,
        &ConversationEvent::ConversationArchived,
    );
    assert_eq!(line_of(&store, session, task).as_deref(), Some("before"));
    // The task answers again while it is archived: its header does not change.
    finished(&mut store, session, task, StreamKind::Text, "after");
    assert_eq!(line_of(&store, session, task).as_deref(), Some("before"));
    // Brought back, it shows what it says now.
    curate(
        &mut store,
        session,
        task,
        &ConversationEvent::ConversationUnarchived,
    );
    assert_eq!(line_of(&store, session, task).as_deref(), Some("after"));
    // Archived again: frozen at the second archive, not at the first.
    curate(
        &mut store,
        session,
        task,
        &ConversationEvent::ConversationArchived,
    );
    finished(&mut store, session, task, StreamKind::Text, "later still");
    assert_eq!(line_of(&store, session, task).as_deref(), Some("after"));

    // A task archived before it ever answered has no line, whatever it says
    // afterwards.
    curate(
        &mut store,
        session,
        never,
        &ConversationEvent::ConversationArchived,
    );
    finished(&mut store, session, never, StreamKind::Text, "too late");
    assert_eq!(message_of(&store, session, never), None);
}

#[test]
fn a_record_written_before_the_field_existed_is_named_without_a_line() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = EventStore::open(dir.path()).unwrap();
    let session = SessionId::new();
    let task = TaskId::new();
    let id = StreamId::new();
    let text = "an old answer\nits last line";
    let hash = store.objects().put(text.as_bytes()).unwrap();
    // Exactly what a Core before PX-218 wrote: no `last_line` key.
    let old = serde_json::json!({
        "event_type": "AssistantMessageCompleted", "stream_id": id, "kind": "TEXT",
        "delta_count": 1, "text_ref": {"object_hash": hash, "byte_length": text.len()},
        "content_hash": "00"
    });
    append(
        &mut store,
        session,
        task,
        AggregateType::AssistantStream,
        *id.as_bytes(),
        vec![
            delta(id, StreamKind::Text, text),
            new_event("AssistantMessageCompleted", old),
        ],
    );
    let message = message_of(&store, session, task).expect("the task answered");
    assert_eq!(message.last_line, None);
    // The text object is where its line is.
    assert_eq!(
        last_line(&String::from_utf8(store.objects().get(&message.text_hash).unwrap()).unwrap()),
        "its last line"
    );
}
