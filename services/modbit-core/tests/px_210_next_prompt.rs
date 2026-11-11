//! QUAL-PX-210 (DR-PX-2026-11-11-021 MWS-16) on the real Core: the actual
//! `modbit-core` binary over its real socket, its real SQLite event store and
//! the repository's standard model stand-in (a scripted OpenAI-compatible
//! server that records every request it receives). The model is a stand-in;
//! everything the Core does with it is real.
//!
//! * A suggestion arrives after a finished turn, from one request with no
//!   tools; the task's log is byte-identical before and after (a digest of the
//!   task's rows read straight from the store's database); the suggested text
//!   is nowhere in the log; the call is in the task's economics, survives a
//!   restart there and counts against the task's cost cap.
//! * The Core refuses with a typed code, and the model receives no request,
//!   while a turn runs, while an input is queued, with the budget at its cap,
//!   when the call's worst case does not fit under the cap, when a capped
//!   task's call cannot be priced, before any turn has finished, without the
//!   session's lease, after the task has ended, and when the Core runs with
//!   suggestions turned off.

mod px_common;

use std::time::Duration;

use modbit_protocol::client::{Client, ClientError};
use modbit_protocol::v1::{
    AgentHeader, AgentHeaders, CancelTask, GetAgentHeaders, GetTaskEconomics, GetTranscript, Id,
    InputQueued, NextPromptSuggestion, QueueInput, QueuedInputChanged, RemoveQueuedInput,
    SetTaskBudgets, StartTask, SuggestNextPrompt, TaskBudgetsView, TaskCancelRequested,
    TaskEconomicsView, TaskRunStarted, TranscriptDensity, TranscriptPage, TranscriptRow,
    TranscriptRowKind,
};
use prost::Message;
use px_common::*;
use serde_json::{Value, json};
use sha2::Digest;

/// How the suggestion request opens: what tells it from a turn's request.
const MARKER: &str = "[NEXT PROMPT SUGGESTION]";
/// What the stand-in suggests, wrapped the way a model wraps one.
const RAW_SUGGESTION: &str =
    "\n\"Add a test for the empty file case\"\nBecause the parser was never run on one.";
const SUGGESTION: &str = "Add a test for the empty file case";
const ANSWER: &str = "I looked at a.txt.\nIt holds one line.";

fn is_suggestion(body: &Value) -> bool {
    body["messages"].as_array().is_some_and(|m| {
        m.iter()
            .any(|x| x["content"].as_str().is_some_and(|c| c.contains(MARKER)))
    })
}

/// The stand-in: a suggestion request is answered with a suggestion, a turn's
/// request with a text-only answer (which ends the run). `hold`, when set,
/// keeps a turn's answer back until that file exists.
async fn model(hold: Option<String>) -> (String, Seen) {
    scripted_model_fn(std::sync::Arc::new(move |body, _| {
        if is_suggestion(body) {
            return json!({ "text": RAW_SUGGESTION });
        }
        match &hold {
            Some(path) => json!({ "text": ANSWER, "wait_file": path }),
            None => json!({ "text": ANSWER }),
        }
    }))
    .await
}

fn suggestion_requests(seen: &Seen) -> usize {
    seen.lock()
        .unwrap()
        .iter()
        .filter(|b| is_suggestion(b))
        .count()
}

fn requests(seen: &Seen) -> usize {
    seen.lock().unwrap().len()
}

async fn cmd<R: Message + Default>(
    c: &mut Client,
    command_type: &str,
    msg: impl Message,
    g: Option<u64>,
) -> Result<R, (String, String)> {
    cmd_id(c, rand_id(), command_type, msg, g).await
}

async fn cmd_id<R: Message + Default>(
    c: &mut Client,
    id: Id,
    command_type: &str,
    msg: impl Message,
    g: Option<u64>,
) -> Result<R, (String, String)> {
    match c
        .command(envelope_fenced(id, command_type, msg.encode_to_vec(), g))
        .await
    {
        Ok(ack) => Ok(Client::result(&ack).unwrap()),
        Err(ClientError::Rejected { code, message }) => Err((code, message)),
        Err(e) => panic!("{command_type}: {e}"),
    }
}

async fn suggest(
    c: &mut Client,
    task: &Id,
    g: Option<u64>,
) -> Result<NextPromptSuggestion, (String, String)> {
    cmd(
        c,
        "SuggestNextPrompt",
        SuggestNextPrompt {
            task_id: Some(task.clone()),
        },
        g,
    )
    .await
}

/// The code a suggestion is refused with.
async fn refused(c: &mut Client, task: &Id, g: Option<u64>) -> String {
    match suggest(c, task, g).await {
        Err((code, _)) => code,
        Ok(s) => panic!("a suggestion was made: {s:?}"),
    }
}

/// Start (or resume) the task; one text-only turn ends the run.
async fn run_turn(c: &mut Client, task: &Id, g: Option<u64>) {
    start(c, task, g).await;
    let st = wait_task(c, task, 90).await;
    assert!(!st.loop_alive, "the run did not end: {st:?}");
}

async fn start(c: &mut Client, task: &Id, g: Option<u64>) {
    let _: TaskRunStarted = cmd(
        c,
        "StartTask",
        StartTask {
            task_id: Some(task.clone()),
            endpoint: String::new(),
            model: "gpt-5-mini".into(),
            max_turns: 0,
            max_tool_calls: 0,
            max_no_progress_turns: 1,
            skills: vec![],
            ..Default::default()
        },
        g,
    )
    .await
    .unwrap();
}

async fn economics(c: &mut Client, task: &Id) -> TaskEconomicsView {
    cmd(
        c,
        "GetTaskEconomics",
        GetTaskEconomics {
            task_id: Some(task.clone()),
        },
        None,
    )
    .await
    .unwrap()
}

async fn header(c: &mut Client, session: &Id, task: &Id) -> AgentHeader {
    let h: AgentHeaders = cmd(
        c,
        "GetAgentHeaders",
        GetAgentHeaders {
            session_id: Some(session.clone()),
            include_archived: true,
        },
        None,
    )
    .await
    .unwrap();
    h.headers
        .into_iter()
        .find(|x| x.task_id.as_ref() == Some(task))
        .expect("the task's header")
}

async fn transcript(c: &mut Client, task: &Id) -> TranscriptPage {
    cmd(
        c,
        "GetTranscript",
        GetTranscript {
            task_id: Some(task.clone()),
            density: TranscriptDensity::Detailed as i32,
            after_row: 0,
            limit: 500,
            as_of_offset: 0,
        },
        None,
    )
    .await
    .unwrap()
}

async fn set_cost_cap(c: &mut Client, task: &Id, g: Option<u64>, cap: u64) {
    let v: TaskBudgetsView = cmd(
        c,
        "SetTaskBudgets",
        SetTaskBudgets {
            task_id: Some(task.clone()),
            max_cost_minor: cap,
            max_wall_ms: 0,
            max_children: 0,
            forbid_spawn: false,
        },
        g,
    )
    .await
    .unwrap();
    assert_eq!(v.max_cost_minor, cap, "{v:?}");
}

/// The task's log as the store holds it: every column of every row of the
/// task, in log order, straight from the database file. Returns the digest
/// and the number of rows.
fn task_log_digest(data: &std::path::Path, task: &Id) -> (String, usize) {
    let conn = rusqlite::Connection::open_with_flags(
        data.join("core/core.db"),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    let mut stmt = conn
        .prepare(
            "SELECT offset, event_id, tenant_id, session_id, task_id, run_id, turn_id, step_id, aggregate_type, aggregate_id, sequence, event_type, schema_version, occurred_at, actor_type, actor_id, causation_id, correlation_id, payload_inline, payload_object_hash, payload_byte_length, integrity_hash
             FROM events WHERE task_id = ?1 ORDER BY offset",
        )
        .unwrap();
    let columns = stmt.column_count();
    let mut hash = sha2::Sha256::new();
    let mut rows = 0;
    let mut q = stmt
        .query(rusqlite::params![task.value.as_slice()])
        .unwrap();
    while let Some(r) = q.next().unwrap() {
        rows += 1;
        for i in 0..columns {
            let bytes: Vec<u8> = match r.get_ref(i).unwrap() {
                rusqlite::types::ValueRef::Null => vec![0],
                rusqlite::types::ValueRef::Integer(n) => {
                    [vec![1], n.to_le_bytes().to_vec()].concat()
                }
                rusqlite::types::ValueRef::Real(f) => [vec![2], f.to_le_bytes().to_vec()].concat(),
                rusqlite::types::ValueRef::Text(t) => [vec![3], t.to_vec()].concat(),
                rusqlite::types::ValueRef::Blob(b) => [vec![4], b.to_vec()].concat(),
            };
            hash.update((bytes.len() as u64).to_le_bytes());
            hash.update(&bytes);
        }
    }
    (hex::encode(hash.finalize()), rows)
}

// ---- a priced model: a cost in minor units needs a signed registry ----

const PRICING_KEY: [u8; 32] = [210u8; 32];

fn pricing_env() -> (String, String) {
    let key = ed25519_dalek::SigningKey::from_bytes(&PRICING_KEY);
    (
        "MODBIT_REGISTRY_KEYS".into(),
        format!("ops:{}", hex::encode(key.verifying_key().to_bytes())),
    )
}

async fn activate_pricing(c: &mut Client) {
    use ed25519_dalek::Signer;
    use modbit_providers::registry::{
        Economics, Governance, Latency, QualityFloor, REGISTRY_SCHEMA_VERSION, RegistryDocument,
        RegistryEntry, SignedRegistry,
    };
    let key = ed25519_dalek::SigningKey::from_bytes(&PRICING_KEY);
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64;
    let entry = RegistryEntry {
        endpoint: "openai".into(),
        provider: "openai".into(),
        family: "gpt-5".into(),
        model: "gpt-5-mini".into(),
        roles: vec!["solver".into(), "reviewer".into()],
        input_modalities: vec!["text".into()],
        context_tokens: 400_000,
        max_output_tokens: 64_000,
        tools: true,
        vision: false,
        reasoning: true,
        structured_output: true,
        economics: Economics {
            input_per_mtok_minor: 25,
            output_per_mtok_minor: 200,
            currency: "USD".into(),
            scale: 2,
            cached_input_per_mtok_minor: None,
            cache_write_per_mtok_minor: None,
            cache_ttl_ms: None,
        },
        latency: Latency {
            p50_ms: 900,
            p95_ms: 4_200,
        },
        governance: Governance {
            data_residency: "us".into(),
            retains_prompts: false,
            allowed_profiles: vec![],
        },
        revoked: false,
        fallbacks: vec![],
    };
    let doc = RegistryDocument {
        promotion: None,
        schema_version: REGISTRY_SCHEMA_VERSION,
        registry_generation: "registry-px210".into(),
        stats_version: "stats-1".into(),
        issued_at_ms: now - 60_000,
        expires_at_ms: now + 7 * 86_400_000,
        quality_floors: vec![QualityFloor {
            mode: "auto".into(),
            min_quality: 0.0,
            max_cost_minor: 5_000,
            currency: "USD".into(),
            scale: 2,
        }],
        entries: vec![entry],
    };
    let json = serde_json::to_string(&doc).unwrap();
    let signed = serde_json::to_string(&SignedRegistry {
        key_id: "ops".into(),
        signature_hex: hex::encode(key.sign(json.as_bytes()).to_bytes()),
        document_json: json,
    })
    .unwrap();
    let r: modbit_protocol::v1::ModelRegistryView = cmd(
        c,
        "ActivateModelRegistry",
        modbit_protocol::v1::ActivateModelRegistry {
            signed_json: signed,
            expected_generation: String::new(),
        },
        None,
    )
    .await
    .unwrap();
    assert!(r.active, "{r:?}");
}

fn spawn(data: &std::path::Path, base: &str, extra: &[(&str, &str)]) -> CoreProcess {
    let mut env = model_env(base);
    env.push(("MODBIT_DEFAULT_MODEL".into(), "gpt-5-mini".into()));
    for (k, v) in extra {
        env.push(((*k).into(), (*v).into()));
    }
    let refs: Vec<(&str, &str)> = env.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
    CoreProcess::spawn_with_env(data, &refs)
}

/// The events of the session's log that are suggestion usage records.
async fn usage_records(core: &CoreProcess, session: &Id) -> Vec<Value> {
    replay(core, session)
        .await
        .into_iter()
        .filter(|e| e["event_type"] == "SuggestionUsageRecorded")
        .collect()
}

/// A suggestion arrives after a finished turn; the task's log is
/// byte-identical before and after the call; the cost is in the task's
/// economics, survives a restart and counts against the task's cap.
#[tokio::test(flavor = "multi_thread")]
async fn qual_px_210_a_suggestion_writes_nothing_to_the_tasks_log_and_is_charged_to_the_task() {
    let (_repo, root) = plain_repo(&[("a.txt", "one line\n")]);
    let (base, seen) = model(None).await;
    let data = tempfile::tempdir().unwrap();
    let (k, v) = pricing_env();
    let extra = [(k.as_str(), v.as_str())];
    let mut core = spawn(data.path(), &base, &extra);
    let mut c = core.client().await;
    activate_pricing(&mut c).await;
    let (session, g) = session_with_lease(&mut c, 0x21).await;
    let task = create_task(
        &mut c,
        &session,
        g,
        &root,
        0x22,
        "local_trusted",
        "say what a.txt holds",
    )
    .await;
    run_turn(&mut c, &task, g).await;
    assert_eq!(requests(&seen), 1, "one turn, one request");

    // What the task looks like before the call.
    let econ_before = economics(&mut c, &task).await;
    assert_eq!(econ_before.model_calls, 1);
    assert_eq!(econ_before.priced_calls, 1, "{econ_before:?}");
    assert!(econ_before.cost_minor > 0);
    assert_eq!(
        (
            econ_before.suggestion_calls,
            econ_before.suggestion_cost_minor
        ),
        (0, 0)
    );
    let header_before = header(&mut c, &session, &task).await;
    assert_eq!(header_before.last_line, "It holds one line.");
    let status_before = wait_task(&mut c, &task, 5).await;
    let transcript_before = transcript(&mut c, &task).await;
    let (digest_before, rows_before) = task_log_digest(data.path(), &task);
    assert!(rows_before > 5, "the task has a log to compare");
    let session_events_before = replay(&core, &session).await.len();

    // The suggestion.
    let s = suggest(&mut c, &task, g).await.unwrap();
    assert_eq!(s.text, SUGGESTION, "one line, without its wrapping");
    assert_eq!(
        (s.endpoint.as_str(), s.model.as_str()),
        ("openai", "gpt-5-mini")
    );
    assert!(s.cost_known && s.cost_minor > 0, "{s:?}");
    assert!(s.input_tokens > 0 && s.output_tokens > 0);
    assert_eq!(s.task_offset, header_before.last_offset);
    assert!(s.usage_offset > s.task_offset);

    // One request, bounded, with no tools, over a snapshot.
    assert_eq!(requests(&seen), 2);
    assert_eq!(suggestion_requests(&seen), 1);
    let body = seen.lock().unwrap()[1].clone();
    assert!(
        body["tools"].as_array().is_none_or(|t| t.is_empty()),
        "no tools on a suggestion: {body}"
    );
    let msgs = body["messages"].as_array().unwrap();
    assert_eq!(msgs.len(), 2, "a system line and the snapshot: {body}");
    assert_eq!(msgs[0]["role"], "system");
    let snapshot = msgs[1]["content"].as_str().unwrap();
    assert!(snapshot.starts_with(MARKER));
    assert!(snapshot.contains("Task goal: say what a.txt holds"));
    assert!(snapshot.contains("Agent: I looked at a.txt.\nIt holds one line."));
    let cap = body["max_completion_tokens"]
        .as_u64()
        .or_else(|| body["max_tokens"].as_u64())
        .expect("the request carries an output cap");
    assert!(cap <= 128, "a bounded call: {cap}");

    // The task's log is byte-identical: same rows, same bytes, same chain.
    let (digest_after, rows_after) = task_log_digest(data.path(), &task);
    assert_eq!(rows_after, rows_before, "a row was added to the task's log");
    assert_eq!(digest_after, digest_before, "the task's log changed");
    // The conversation, the header and the task's status are what they were.
    let transcript_after = transcript(&mut c, &task).await;
    // (The tail row is the Core's status line, stamped with the log's tip;
    // it says the same thing and is not part of the conversation.)
    let conversation = |p: &TranscriptPage| -> Vec<TranscriptRow> {
        p.rows
            .iter()
            .filter(|r| r.kind != TranscriptRowKind::TailStatus as i32)
            .cloned()
            .collect()
    };
    let tail = |p: &TranscriptPage| -> Vec<String> {
        p.rows
            .iter()
            .filter(|r| r.kind == TranscriptRowKind::TailStatus as i32)
            .map(|r| r.text.clone())
            .collect()
    };
    assert_eq!(
        conversation(&transcript_after),
        conversation(&transcript_before)
    );
    assert_eq!(conversation(&transcript_after).len(), 3);
    assert_eq!(tail(&transcript_after), tail(&transcript_before));
    assert!(!format!("{transcript_after:?}").contains(SUGGESTION));
    assert_eq!(header(&mut c, &session, &task).await, header_before);
    let status_after = wait_task(&mut c, &task, 5).await;
    assert_eq!(
        (
            status_after.state.as_str(),
            status_after.wait_reason.as_str(),
            status_after.attention_reason.as_str(),
            status_after.loop_alive
        ),
        (
            status_before.state.as_str(),
            status_before.wait_reason.as_str(),
            status_before.attention_reason.as_str(),
            false
        )
    );
    // The session's own cursor moved by the one usage record, and only by it.
    assert_eq!(status_after.last_offset, status_before.last_offset + 1);
    assert_eq!(status_after.last_offset, s.usage_offset);

    // The session's log gained exactly the usage record: on the session's
    // aggregate, outside the task's lineage, naming the task, without the text.
    let events = replay(&core, &session).await;
    assert_eq!(events.len(), session_events_before + 1);
    let record = events.last().unwrap();
    assert_eq!(record["event_type"], "SuggestionUsageRecorded");
    assert_eq!(record["aggregate_type"], "session");
    assert_eq!(record["task_id"], Value::Null, "not in the task's lineage");
    assert_eq!(record["offset"], s.usage_offset);
    let payload = &record["payload"]["payload"];
    let named = payload["task_id"].as_str().unwrap().replace('-', "");
    assert_eq!(named, hex_id(&task));
    assert_eq!(payload["input_tokens"], s.input_tokens);
    assert_eq!(payload["output_tokens"], s.output_tokens);
    assert_eq!(payload["reported"], true);
    assert_eq!(payload["route"]["requested_model"], "gpt-5-mini");
    // What the record holds is usage and a route: nothing the model said.
    let mut keys: Vec<&str> = payload
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    keys.sort_unstable();
    assert_eq!(
        keys,
        [
            "cached_input_tokens",
            "event_type",
            "input_tokens",
            "output_tokens",
            "reported",
            "route",
            "task_id"
        ]
    );
    // The suggestion is written nowhere: not in the log, not in an object.
    let whole_log = serde_json::to_string(&events).unwrap();
    assert!(
        !whole_log.contains(SUGGESTION),
        "the suggestion is on the log"
    );
    assert!(!whole_log.contains("never run on one"));

    // The charge is in the task's economics, as a call of the task's.
    let econ = economics(&mut c, &task).await;
    assert_eq!(econ.model_calls, econ_before.model_calls + 1);
    assert_eq!(econ.priced_calls, econ_before.priced_calls + 1);
    assert_eq!((econ.unpriced_calls, econ.unreported_calls), (0, 0));
    assert_eq!(econ.cost_minor, econ_before.cost_minor + s.cost_minor);
    assert_eq!(
        (econ.suggestion_calls, econ.suggestion_cost_minor),
        (1, s.cost_minor)
    );
    assert_eq!(econ.input_tokens, econ_before.input_tokens + s.input_tokens);
    assert_eq!(
        econ.output_tokens,
        econ_before.output_tokens + s.output_tokens
    );
    // It is inside no run: the per-run attribution is what it was.
    assert_eq!(econ.runs, econ_before.runs);

    // The same command id again is answered once: no second call, no second
    // charge, and no text (it was never kept).
    let id = rand_id();
    let again: NextPromptSuggestion = cmd_id(
        &mut c,
        id.clone(),
        "SuggestNextPrompt",
        SuggestNextPrompt {
            task_id: Some(task.clone()),
        },
        g,
    )
    .await
    .unwrap();
    assert_eq!(again.text, SUGGESTION);
    let replayed: Result<NextPromptSuggestion, _> = cmd_id(
        &mut c,
        id,
        "SuggestNextPrompt",
        SuggestNextPrompt {
            task_id: Some(task.clone()),
        },
        g,
    )
    .await;
    assert_eq!(replayed.unwrap_err().0, "ALREADY_ANSWERED");
    assert_eq!(suggestion_requests(&seen), 2);
    assert_eq!(usage_records(&core, &session).await.len(), 2);
    let spent = econ_before.cost_minor + s.cost_minor + again.cost_minor;
    assert_eq!(economics(&mut c, &task).await.cost_minor, spent);

    // A Core restart reads the same charge back from the log.
    drop(c);
    core.kill();
    let core = spawn(data.path(), &base, &extra);
    let mut c = core.client().await;
    let econ = economics(&mut c, &task).await;
    assert_eq!(econ.cost_minor, spent);
    assert_eq!(econ.suggestion_calls, 2);
    assert_eq!(task_log_digest(data.path(), &task).0, digest_before);

    // The budget at its cap: no call.
    let before = requests(&seen);
    set_cost_cap(&mut c, &task, g, spent).await;
    assert_eq!(refused(&mut c, &task, g).await, "BUDGET_EXHAUSTED");
    // One unit under the cap: the call's worst case does not fit.
    set_cost_cap(&mut c, &task, g, spent + 1).await;
    assert_eq!(refused(&mut c, &task, g).await, "BUDGET_WOULD_EXCEED");
    assert_eq!(requests(&seen), before, "a refused call reached the model");
    assert_eq!(usage_records(&core, &session).await.len(), 2);
    // With room, it is made, and what is left under the cap shrinks by it.
    set_cost_cap(&mut c, &task, g, spent + 100).await;
    let third = suggest(&mut c, &task, g).await.unwrap();
    let econ = economics(&mut c, &task).await;
    assert_eq!(econ.cost_minor, spent + third.cost_minor);
    assert_eq!(econ.cost_remaining_minor, 100 - third.cost_minor);
    // Its log is still the log it had: the budget records are the person's
    // own acts on the task, and no suggestion added to them.
    let (_, rows_with_caps) = task_log_digest(data.path(), &task);
    assert_eq!(rows_with_caps, rows_before + 3, "three SetTaskBudgets");

    // The charge is the task's own spend where the cap is enforced: a cap
    // the turns alone would fit under stops the next run because of it.
    let spent_now = spent + third.cost_minor;
    assert!(spent_now - 1 > econ_before.cost_minor);
    set_cost_cap(&mut c, &task, g, spent_now - 1).await;
    let before = requests(&seen);
    run_turn(&mut c, &task, g).await;
    assert_eq!(
        requests(&seen),
        before,
        "a run over its cap called the model"
    );
    let exhausted: Vec<Value> = replay(&core, &session)
        .await
        .into_iter()
        .filter(|e| e["event_type"] == "HarnessBudgetExhausted")
        .collect();
    assert_eq!(exhausted.len(), 1, "{exhausted:?}");
    assert_eq!(
        exhausted[0]["payload"]["payload"]["budget"],
        "max_cost_minor"
    );
    assert_eq!(exhausted[0]["payload"]["payload"]["used"], spent_now);
}

/// The Core refuses with a typed code, and makes no model call, whenever a
/// suggestion is not wanted: before a turn has finished, without the lease,
/// while a turn runs, while an input is queued, when a capped task's call
/// cannot be priced, and once the task has ended.
#[tokio::test(flavor = "multi_thread")]
async fn qual_px_210_the_core_refuses_while_a_turn_runs_an_input_is_queued_or_the_call_is_not_affordable()
 {
    let (_repo, root) = plain_repo(&[("a.txt", "one line\n")]);
    let data = tempfile::tempdir().unwrap();
    let gate = data.path().join("let-the-turn-finish");
    let (base, seen) = model(Some(gate.to_string_lossy().into_owned())).await;
    let core = spawn(data.path(), &base, &[]);
    let mut c = core.client().await;
    let (session, g) = session_with_lease(&mut c, 0x31).await;
    let task = create_task(
        &mut c,
        &session,
        g,
        &root,
        0x32,
        "local_trusted",
        "say what a.txt holds",
    )
    .await;

    // No task, and a task that has not run.
    assert_eq!(refused(&mut c, &id16(0x7E), g).await, "UNKNOWN_TASK");
    assert_eq!(refused(&mut c, &task, g).await, "NO_FINISHED_TURN");

    // A turn is running: the model has the request and is holding its answer.
    start(&mut c, &task, g).await;
    let deadline = std::time::Instant::now() + Duration::from_secs(60);
    while requests(&seen) == 0 {
        assert!(std::time::Instant::now() < deadline, "the turn never began");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert_eq!(refused(&mut c, &task, g).await, "TURN_RUNNING");
    assert_eq!(requests(&seen), 1, "a call was made during a turn");
    std::fs::write(&gate, "go").unwrap();
    let st = wait_task(&mut c, &task, 90).await;
    assert!(!st.loop_alive, "{st:?}");

    // A mutating command needs the session's lease.
    assert_eq!(refused(&mut c, &task, None).await, "LEASE_REQUIRED");
    assert_eq!(refused(&mut c, &task, Some(9_999)).await, "STALE_LEASE");

    // An input is queued: it is the task's next message.
    let _: InputQueued = cmd(
        &mut c,
        "QueueInput",
        QueueInput {
            task_id: Some(task.clone()),
            input_id: "q1".into(),
            mode: "FOLLOW_UP".into(),
            text: "and what about b.txt?".into(),
        },
        g,
    )
    .await
    .unwrap();
    assert_eq!(refused(&mut c, &task, g).await, "INPUT_QUEUED");
    assert_eq!(requests(&seen), 1, "a call was made with an input queued");
    let _: QueuedInputChanged = cmd(
        &mut c,
        "RemoveQueuedInput",
        RemoveQueuedInput {
            task_id: Some(task.clone()),
            input_id: "q1".into(),
            reason: "test".into(),
        },
        g,
    )
    .await
    .unwrap();

    // Nothing queued, the turn ended: the suggestion is made. This Core has
    // no registry, so the call is one whose cost is unknown, never zero.
    let (digest_before, _) = task_log_digest(data.path(), &task);
    let s = suggest(&mut c, &task, g).await.unwrap();
    assert_eq!(s.text, SUGGESTION);
    assert!(!s.cost_known && s.cost_minor == 0);
    assert_eq!((requests(&seen), suggestion_requests(&seen)), (2, 1));
    assert_eq!(task_log_digest(data.path(), &task).0, digest_before);
    let econ = economics(&mut c, &task).await;
    assert_eq!((econ.model_calls, econ.suggestion_calls), (2, 1));
    assert_eq!((econ.priced_calls, econ.unpriced_calls), (0, 2));
    assert_eq!((econ.cost_minor, econ.suggestion_cost_minor), (0, 0));

    // A cost cap on a task whose calls nobody can price: no call.
    set_cost_cap(&mut c, &task, g, 1_000).await;
    assert_eq!(refused(&mut c, &task, g).await, "COST_UNMETERED");
    assert_eq!(requests(&seen), 2);

    // The task has ended: it takes no more messages, so none is suggested.
    let _: TaskCancelRequested = cmd(
        &mut c,
        "CancelTask",
        CancelTask {
            task_id: Some(task.clone()),
        },
        g,
    )
    .await
    .unwrap();
    let st = wait_task(&mut c, &task, 30).await;
    assert_eq!(st.state, "Cancelled", "{st:?}");
    assert_eq!(refused(&mut c, &task, g).await, "TASK_ENDED");
    assert_eq!(requests(&seen), 2);
    assert_eq!(usage_records(&core, &session).await.len(), 1);
}

/// With suggestions turned off for the Core no call is made: the scripted
/// model receives no request and nothing is recorded.
#[tokio::test(flavor = "multi_thread")]
async fn qual_px_210_with_the_setting_off_no_call_is_made() {
    let (_repo, root) = plain_repo(&[("a.txt", "one line\n")]);
    let (base, seen) = model(None).await;
    for (value, off) in [("off", true), ("0", true), ("false", true), ("on", false)] {
        let data = tempfile::tempdir().unwrap();
        let core = spawn(
            data.path(),
            &base,
            &[("MODBIT_NEXT_PROMPT_SUGGESTIONS", value)],
        );
        let mut c = core.client().await;
        let (session, g) = session_with_lease(&mut c, 0x41).await;
        let task = create_task(
            &mut c,
            &session,
            g,
            &root,
            0x42,
            "local_trusted",
            "say what a.txt holds",
        )
        .await;
        run_turn(&mut c, &task, g).await;
        let before = requests(&seen);
        let made = suggestion_requests(&seen);
        if off {
            assert_eq!(
                refused(&mut c, &task, g).await,
                "SUGGESTIONS_OFF",
                "{value}"
            );
            assert_eq!(
                requests(&seen),
                before,
                "{value}: a request reached the model"
            );
            assert!(usage_records(&core, &session).await.is_empty());
            assert_eq!(economics(&mut c, &task).await.suggestion_calls, 0);
        } else {
            assert_eq!(suggest(&mut c, &task, g).await.unwrap().text, SUGGESTION);
            assert_eq!(suggestion_requests(&seen), made + 1);
        }
    }
}
