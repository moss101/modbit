//! PX-155 (QUAL-PX-155): context-overflow recovery on the real Core. The
//! provider transport is a local OpenAI-compatible server that answers an
//! oversized request exactly as the API does (HTTP 400,
//! `context_length_exceeded`); the gateway types it `CONTEXT_OVERFLOW`, the
//! Core compacts synchronously through the existing ladder and sends the
//! request once more. A second overflow ends the run with the typed failure
//! and never loops.
//!
//! Real: the `modbit-core` binary, its store, the gateway's HTTP client and
//! the compaction ladder. Stand-in: the scripted model's replies.

mod px_common;

use modbit_protocol::client::Client;
use modbit_protocol::v1::{ObjectRangeChunk, ReadObjectRange};
use prost::Message;
use px_common::*;
use serde_json::{Value, json};

const GOAL: &str = "read the four notes files";

fn overflow() -> Value {
    json!({"http_error": {"status": 400, "body": {"error": {
        "message": "This model's maximum context length is 8192 tokens. However, your messages resulted in 9311 tokens.",
        "type": "invalid_request_error",
        "param": "messages",
        "code": "context_length_exceeded"
    }}}})
}

fn has_epoch(body: &Value) -> bool {
    body["messages"].as_array().is_some_and(|m| {
        m.iter().any(|x| {
            x["content"]
                .as_str()
                .unwrap_or_default()
                .contains("Compaction epoch 1")
        })
    })
}

fn is_overflow_reply(body: &Value, results: usize, always: bool) -> bool {
    if has_epoch(body) {
        always
    } else {
        results >= 5
    }
}

/// The script: a plan, four reads, then an overflow once the transcript holds
/// five tool results; after the compaction the request completes the task,
/// or, with `always`, overflows again.
fn reply(always: bool) -> Reply {
    std::sync::Arc::new(move |body, results| {
        if is_overflow_reply(body, results, always) {
            return overflow();
        }
        if has_epoch(body) {
            return json!({"calls": [{"name": "task.complete", "args": {"summary": "done", "self_review": {"findings": []}}}]});
        }
        if results == 0 {
            return json!({"calls": [{"name": "plan.update", "args": {"outcome": "read the notes", "expected_files": []}}]});
        }
        json!({"calls": [{"name": "fs.read", "args": {"path": format!("notes-{results}.txt")}}]})
    })
}

struct Run {
    events: Vec<Value>,
    bodies: Vec<Value>,
    status: modbit_protocol::v1::TaskStatus,
    overflowed: usize,
    client: Client,
    _core: CoreProcess,
    _dir: tempfile::TempDir,
    _repo: tempfile::TempDir,
}

async fn run(always: bool) -> Run {
    let notes = "a line of notes\n".repeat(60);
    let files: Vec<(String, &str)> = (1..=4)
        .map(|i| (format!("notes-{i}.txt"), notes.as_str()))
        .collect();
    let files: Vec<(&str, &str)> = files.iter().map(|(n, t)| (n.as_str(), *t)).collect();
    let (repo, root) = plain_repo(&files);
    let (base, seen) = scripted_model_fn(reply(always)).await;
    let dir = tempfile::tempdir().unwrap();
    let env = model_env(&base);
    let refs: Vec<(&str, &str)> = env.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
    let core = CoreProcess::spawn_with_env(dir.path(), &refs);
    let mut c = core.client().await;
    let (session, g) = session_with_lease(&mut c, 0x51).await;
    let task = create_task(&mut c, &session, g, &root, 0x52, "local_trusted", GOAL).await;
    start_task(&mut c, &task, g, 0x53, "gpt-5-mini").await;
    let status = wait_task(&mut c, &task, 120).await;
    let events = replay(&core, &session).await;
    let bodies = seen.lock().unwrap().clone();
    let overflowed = bodies
        .iter()
        .filter(|b| {
            let results = b["messages"]
                .as_array()
                .map(|m| m.iter().filter(|x| x["role"] == "tool").count())
                .unwrap_or(0);
            is_overflow_reply(b, results, always)
        })
        .count();
    Run {
        events,
        bodies,
        status,
        overflowed,
        client: c,
        _core: core,
        _dir: dir,
        _repo: repo,
    }
}

fn of_type<'a>(ev: &'a [Value], t: &str) -> Vec<&'a Value> {
    ev.iter().filter(|e| e["event_type"] == t).collect()
}

fn failed_codes(ev: &[Value]) -> Vec<String> {
    of_type(ev, "TurnFailed")
        .iter()
        .map(|e| {
            e["payload"]["payload"]["failure_code"]
                .as_str()
                .unwrap_or_default()
                .to_owned()
        })
        .collect()
}

async fn read_range(c: &mut Client, hash: &str, offset: u64, length: u64) -> ObjectRangeChunk {
    let ack = c
        .command(envelope(
            rand_id(),
            "ReadObjectRange",
            ReadObjectRange {
                object_hash: hash.to_owned(),
                offset,
                length,
            }
            .encode_to_vec(),
        ))
        .await
        .unwrap();
    Client::result(&ack).unwrap()
}

/// An overflow compacts synchronously, the request is retried once and the
/// turn completes; the compaction is on the log, the user's goal survives in
/// the retried request, and the summarised history stays readable by range.
#[tokio::test(flavor = "multi_thread")]
async fn qual_px_155_an_overflow_compacts_synchronously_and_the_retry_completes_the_turn() {
    let mut r = run(false).await;
    let dump = || {
        format!(
            "{:#?}",
            r.events
                .iter()
                .map(|e| &e["event_type"])
                .collect::<Vec<_>>()
        )
    };
    assert!(!r.status.loop_alive, "{:?}", r.status);
    assert_eq!(
        r.status.state,
        "ReadyForReview",
        "{:?}\n{}",
        r.status,
        dump()
    );
    assert_eq!(r.overflowed, 1, "one overflow, answered once");
    assert_eq!(failed_codes(&r.events), ["CONTEXT_OVERFLOW"], "{}", dump());
    // The compaction request and its commit are on the log, synchronous.
    let started = of_type(&r.events, "CompactionStarted");
    assert_eq!(started.len(), 1, "{}", dump());
    assert_eq!(started[0]["payload"]["payload"]["mode"], "SYNC_FALLBACK");
    let opened = of_type(&r.events, "ContextEpochOpened");
    assert_eq!(opened.len(), 1, "{}", dump());
    let failed_at = of_type(&r.events, "TurnFailed")[0]["offset"]
        .as_u64()
        .unwrap();
    assert!(
        started[0]["offset"].as_u64().unwrap() > failed_at,
        "compaction follows the overflow"
    );
    // The retried request carries the epoch and still the person's goal.
    let retry = r
        .bodies
        .iter()
        .find(|b| has_epoch(b))
        .expect("a retried request");
    assert!(
        retry.to_string().contains(GOAL),
        "the user's message survived: {retry}"
    );
    // The summarised history is intact and readable by range.
    let manifest_ref = opened[0]["payload"]["payload"]["manifest_ref"]
        .as_str()
        .unwrap()
        .to_owned();
    let whole = read_range(&mut r.client, &manifest_ref, 0, 1024 * 1024).await;
    assert!(whole.total_bytes > 2);
    let half = whole.total_bytes / 2;
    let a = read_range(&mut r.client, &manifest_ref, 0, half).await;
    let b = read_range(&mut r.client, &manifest_ref, half, whole.total_bytes - half).await;
    assert_eq!([a.data, b.data].concat(), whole.data);
    let manifest: Value = serde_json::from_slice(&whole.data).unwrap();
    assert!(
        !manifest["resources"].as_array().unwrap().is_empty(),
        "{manifest}"
    );
}

/// A second overflow after the compaction ends the run with the typed
/// failure: exactly two overflowed requests, no third attempt, no loop.
#[tokio::test(flavor = "multi_thread")]
async fn qual_px_155_a_second_overflow_ends_with_the_typed_failure_and_never_loops() {
    let r = run(true).await;
    let dump = format!(
        "{:#?}",
        r.events
            .iter()
            .map(|e| &e["event_type"])
            .collect::<Vec<_>>()
    );
    assert!(!r.status.loop_alive, "{:?}", r.status);
    assert_ne!(r.status.state, "ReadyForReview", "{:?}", r.status);
    assert_eq!(
        r.overflowed, 2,
        "the first and the one retry, nothing more: {dump}"
    );
    assert_eq!(
        failed_codes(&r.events),
        ["CONTEXT_OVERFLOW", "CONTEXT_OVERFLOW"],
        "{dump}"
    );
    assert_eq!(of_type(&r.events, "CompactionStarted").len(), 1, "{dump}");
    let text = serde_json::to_string(&r.events).unwrap();
    assert!(
        text.contains("again after a compaction; not retried"),
        "{dump}"
    );
}
