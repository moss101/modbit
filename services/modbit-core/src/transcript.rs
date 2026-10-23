//! Conversation read model (PX-042; docs/65 AFW-C01, C04, C05, AFW-B02, B04,
//! B10): transcript rows and agent headers, served by the Core.
//!
//! Both are projections of the canonical event log, computed when they are
//! asked for and stored nowhere, so they cannot drift from it and rebuild
//! identically after any restart:
//!
//! * [`page`] folds one task's log into **atoms** — user messages, assistant
//!   messages (from the stream records of PX-041), tool cards, approval and
//!   question cards, turn footers — and then, by density, groups them. The
//!   three densities differ only in grouping; every atom is in every view
//!   (inside a work group's children when folded). A client renders a
//!   conversation from these rows alone.
//! * [`headers`] gives each task of a session a small header whose status
//!   class is decided here, by the precedence of AFW-B02, so no client
//!   recomputes it. It loads no event of any transcript: the figures come
//!   from the task projection and grouped aggregates over the session's
//!   events (`EventStore::task_digests`); the only events it reads are those
//!   `attention` derives the needs-attention class from.

use std::collections::HashMap;

use modbit_domain::approval::ApprovalState;
use modbit_domain::state::StateMachine;
use modbit_domain::task::{Task, TaskOrigin, TaskState, WaitReason};
use modbit_domain::{SessionId, StreamId};
use modbit_event_store::EventStore;
use modbit_event_store::StoredEvent;
use modbit_protocol::v1 as wire;
use wire::transcript_row::Facts;

use crate::server::Core;

/// Bytes of a message a row carries inline; the whole text is `text_ref`.
const TEXT_MAX: usize = 8 * 1024;
/// Rows per page by default and at most.
const DEFAULT_PAGE: usize = 100;
const MAX_PAGE: usize = 500;
/// Silence that earns a time-boundary row.
const GAP_MS: i64 = 30 * 60 * 1000;

/// Why a read was refused: a stable code and a message.
pub(crate) type Refusal = (&'static str, String);

fn ts(ms: i64) -> Option<prost_types::Timestamp> {
    Some(prost_types::Timestamp {
        seconds: ms.div_euclid(1000),
        nanos: (ms.rem_euclid(1000) * 1_000_000) as i32,
    })
}

/// How a tool's work reads in a conversation.
fn tool_class(name: &str) -> &'static str {
    match name.split('.').next().unwrap_or_default() {
        "fs" => match name {
            "fs.write" | "fs.rm" => "EDIT",
            "fs.glob" => "SEARCH",
            _ => "READ",
        },
        "change" | "notebook" => "EDIT",
        "search" | "lsp" | "memory" if name != "memory.propose" => "SEARCH",
        "git" | "artifact" | "context" | "repo" => "READ",
        "shell" | "proc" | "test" => "SHELL",
        "browser" => "BROWSER",
        _ => "OTHER",
    }
}

/// One line that says what a call was about: its path, command or query.
fn short_text(args: &serde_json::Value) -> String {
    let pick = |k: &str| args[k].as_str().map(str::to_owned);
    let line = pick("path")
        .or_else(|| {
            args["paths"]
                .as_array()
                .and_then(|a| a.first())
                .and_then(|v| v.as_str().map(str::to_owned))
        })
        .or_else(|| pick("command"))
        .or_else(|| {
            args["argv"].as_array().map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str())
                    .collect::<Vec<_>>()
                    .join(" ")
            })
        })
        .or_else(|| pick("query"))
        .or_else(|| pick("pattern"))
        .or_else(|| pick("url"))
        .unwrap_or_default();
    bounded(&line, 160).0
}

fn bounded(s: &str, max: usize) -> (String, bool) {
    if s.len() <= max {
        return (s.to_owned(), false);
    }
    let mut end = max;
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    (s[..end].to_owned(), true)
}

/// A bounded read of an object as text (lossy: a row is a preview).
fn object_text(store: &EventStore, hash: &str, max: usize) -> Option<(String, bool)> {
    let (data, total, _) = store.objects().read_range(hash, 0, max as u64).ok()?;
    let text = String::from_utf8_lossy(&data).into_owned();
    let (text, cut) = bounded(&text, max);
    Some((text, cut || total as usize > data.len()))
}

#[derive(Default)]
struct TurnInfo {
    started_ms: i64,
    input_tokens: u64,
    output_tokens: u64,
    reasoning: bool,
    footer: Option<usize>,
}

/// The atoms of a task's conversation up to `until`, plus what the folding
/// of densities needs to know.
struct Folded {
    atoms: Vec<wire::TranscriptRow>,
    turns: HashMap<String, TurnInfo>,
    attention: Option<String>,
}

fn row(
    id: String,
    kind: wire::TranscriptRowKind,
    e: &StoredEvent,
    turn: Option<String>,
) -> wire::TranscriptRow {
    wire::TranscriptRow {
        ordinal: 0,
        row_id: id,
        kind: kind as i32,
        offset: e.offset,
        last_offset: e.offset,
        at: ts(e.envelope.occurred_at.millis()),
        turn_id: turn.unwrap_or_default(),
        hints: Some(wire::RowHints {
            renderable: true,
            ..Default::default()
        }),
        text: String::new(),
        text_truncated: false,
        text_ref: String::new(),
        children: vec![],
        facts: None,
    }
}

fn hints(r: &mut wire::TranscriptRow) -> &mut wire::RowHints {
    r.hints.get_or_insert_with(Default::default)
}

/// The density-independent rows of a task's conversation at the log's tip:
/// what the full-text index of `conversation_search` indexes, so a search
/// hit names a row `GetTranscript` serves under the same id.
pub(crate) fn atoms(store: &EventStore, task: &Task) -> Result<Vec<wire::TranscriptRow>, Refusal> {
    let tip = store
        .last_offset()
        .map_err(|e| ("STORE_ERROR", e.to_string()))?;
    Ok(fold(store, task, tip)?.atoms)
}

fn fold(store: &EventStore, task: &Task, until: u64) -> Result<Folded, Refusal> {
    let log = store
        .read_task_log(&task.session_id, &task.task_id, until)
        .map_err(|e| ("STORE_ERROR", e.to_string()))?;
    let mut atoms: Vec<wire::TranscriptRow> = Vec::new();
    let mut turns: HashMap<String, TurnInfo> = HashMap::new();
    let mut streams: HashMap<[u8; 16], usize> = HashMap::new();
    let mut tools: HashMap<String, usize> = HashMap::new();
    let mut approvals: HashMap<[u8; 16], usize> = HashMap::new();
    let mut questions: HashMap<String, usize> = HashMap::new();
    // FileChanged by the tool call that made it: paths and line counts.
    let mut edits: HashMap<String, (Vec<String>, u32, u32)> = HashMap::new();
    let mut attention: Option<String> = None;
    // The turn that is open at this point of the log: events that carry no
    // turn of their own (a recorded plan) belong to it.
    let mut open_turn: Option<String> = None;
    // PX-050: an input the person deleted while it was queued was never said,
    // and one they edited was said as edited: the conversation shows what the
    // model was given, not what was typed first.
    let mut deleted: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut edited: HashMap<String, (String, Option<String>)> = HashMap::new();
    for e in &log {
        match e.envelope.event_type.as_str() {
            "TaskInputRemoved" => {
                if let Some(id) = store
                    .payload(&e.envelope)
                    .ok()
                    .and_then(|p| p["input_id"].as_str().map(str::to_owned))
                {
                    deleted.insert(id);
                }
            }
            "TaskInputEdited" => {
                if let Ok(p) = store.payload(&e.envelope)
                    && let Some(id) = p["input_id"].as_str()
                {
                    let slot = edited.entry(id.to_owned()).or_default();
                    if let Some(t) = p["text"].as_str().filter(|t| !t.is_empty()) {
                        slot.0 = t.to_owned();
                    }
                    if let Some(m) = p["mode"].as_str() {
                        slot.1 = Some(m.to_uppercase());
                    }
                }
            }
            _ => {}
        }
    }
    for e in &log {
        let env = &e.envelope;
        let p = store.payload(env).unwrap_or_default();
        let turn = env.turn_id.map(|t| t.to_string());
        match env.event_type.as_str() {
            "PlanRecorded" | "PlanRevised" => {
                let outcome = p["plan_ref"]
                    .as_str()
                    .and_then(|h| object_text(store, h, 64 * 1024))
                    .and_then(|(t, _)| serde_json::from_str::<serde_json::Value>(&t).ok())
                    .and_then(|v| v["outcome"].as_str().map(str::to_owned))
                    .unwrap_or_default();
                let mut r = row(
                    format!("plan:{}", e.offset),
                    wire::TranscriptRowKind::ToolCard,
                    e,
                    turn.clone().or_else(|| open_turn.clone()),
                );
                hints(&mut r).status = "SUCCEEDED".into();
                hints(&mut r).short_text = bounded(&outcome, 160).0;
                r.text = format!("plan.update {}", bounded(&outcome, 160).0)
                    .trim()
                    .to_owned();
                r.facts = Some(Facts::Tool(wire::ToolFacts {
                    tool_call_id: String::new(),
                    tool_name: "plan.update".into(),
                    tool_class: "OTHER".into(),
                    state: "SUCCEEDED".into(),
                    ..Default::default()
                }));
                atoms.push(r);
            }
            "TaskCreated" => {
                let (text, cut) = bounded(p["goal_text"].as_str().unwrap_or_default(), TEXT_MAX);
                let mut r = row(
                    "user:goal".into(),
                    wire::TranscriptRowKind::UserMessage,
                    e,
                    None,
                );
                r.text = text;
                r.text_truncated = cut;
                r.facts = Some(Facts::User(wire::UserFacts {
                    source: "goal".into(),
                    ..Default::default()
                }));
                atoms.push(r);
            }
            "TaskInputQueued" => {
                let input = p["input_id"].as_str().unwrap_or_default();
                if deleted.contains(input) {
                    continue;
                }
                let edit = edited.get(input);
                let (text, cut) = bounded(
                    edit.map(|e| e.0.as_str())
                        .filter(|t| !t.is_empty())
                        .unwrap_or_else(|| p["text"].as_str().unwrap_or_default()),
                    TEXT_MAX,
                );
                let mut r = row(
                    format!("user:in:{input}"),
                    wire::TranscriptRowKind::UserMessage,
                    e,
                    None,
                );
                r.text = text;
                r.text_truncated = cut;
                r.facts = Some(Facts::User(wire::UserFacts {
                    input_id: input.into(),
                    source: "queued_input".into(),
                    mode: edit
                        .and_then(|e| e.1.clone())
                        .unwrap_or_else(|| p["mode"].as_str().unwrap_or_default().to_uppercase()),
                    provenance: p["provenance"].as_str().unwrap_or_default().into(),
                    untrusted: p["untrusted"].as_bool().unwrap_or(false),
                }));
                atoms.push(r);
            }
            "UserQuestionAsked" => {
                let qid = p["question_id"].as_str().unwrap_or_default().to_owned();
                let (text, cut) = bounded(p["question"].as_str().unwrap_or_default(), TEXT_MAX);
                let mut r = row(
                    format!("question:{qid}"),
                    wire::TranscriptRowKind::ApprovalCard,
                    e,
                    None,
                );
                r.text = text;
                r.text_truncated = cut;
                hints(&mut r).status = "REQUESTED".into();
                r.facts = Some(Facts::Approval(wire::ApprovalFacts {
                    kind: "QUESTION".into(),
                    approval_id: qid.clone(),
                    state: "REQUESTED".into(),
                    ..Default::default()
                }));
                questions.insert(qid, atoms.len());
                atoms.push(r);
            }
            "UserQuestionAnswered" => {
                let qid = p["question_id"].as_str().unwrap_or_default().to_owned();
                if let Some(&i) = questions.get(&qid) {
                    atoms[i].last_offset = e.offset;
                    hints(&mut atoms[i]).status = "ANSWERED".into();
                    if let Some(Facts::Approval(a)) = &mut atoms[i].facts {
                        a.state = "ANSWERED".into();
                    }
                }
                let answer = p["text"]
                    .as_str()
                    .or_else(|| p["option_id"].as_str())
                    .unwrap_or_default();
                let (text, cut) = bounded(answer, TEXT_MAX);
                let mut r = row(
                    format!("user:ans:{qid}"),
                    wire::TranscriptRowKind::UserMessage,
                    e,
                    None,
                );
                r.text = text;
                r.text_truncated = cut;
                r.facts = Some(Facts::User(wire::UserFacts {
                    source: "answer".into(),
                    ..Default::default()
                }));
                atoms.push(r);
            }
            "TaskNeedsAttention" => {
                attention = Some(p["reason"].as_str().unwrap_or_default().to_owned());
            }
            "TaskStarted" | "TaskResumed" => attention = None,
            // ---- streams (PX-041)
            "AssistantTextDelta" => {
                let agg = env.aggregate_id;
                let sid = StreamId::from_bytes(agg).to_string();
                if p["kind"] == "REASONING" {
                    if let Some(t) = &turn {
                        turns.entry(t.clone()).or_default().reasoning = true;
                    }
                } else {
                    let mut r = row(
                        format!("msg:{sid}"),
                        wire::TranscriptRowKind::AssistantMessage,
                        e,
                        turn.clone(),
                    );
                    hints(&mut r).status = "STREAMING".into();
                    r.facts = Some(Facts::Stream(wire::StreamFacts {
                        stream_id: sid,
                        phase: "OPEN".into(),
                        first_offset: e.offset,
                        last_offset: e.offset,
                        delta_count: 1,
                        ..Default::default()
                    }));
                    streams.insert(agg, atoms.len());
                    atoms.push(r);
                }
            }
            "AssistantMessageCompleted" => {
                if let Some(&i) = streams.get(&env.aggregate_id) {
                    let hash = p["text_ref"]["object_hash"].as_str().unwrap_or_default();
                    let (text, cut) = object_text(store, hash, TEXT_MAX).unwrap_or_default();
                    let r = &mut atoms[i];
                    r.text = text;
                    r.text_truncated = cut;
                    r.text_ref = hash.to_owned();
                    r.last_offset = e.offset;
                    hints(r).status = "COMPLETE".into();
                    if let Some(Facts::Stream(s)) = &mut r.facts {
                        s.phase = "COMPLETED".into();
                        s.last_offset = e.offset;
                        s.delta_count = p["delta_count"].as_u64().unwrap_or(0) as u32;
                        s.content_hash = p["content_hash"].as_str().unwrap_or_default().into();
                    }
                }
            }
            "AssistantMessageAborted" => {
                if let Some(&i) = streams.get(&env.aggregate_id) {
                    let r = &mut atoms[i];
                    r.last_offset = e.offset;
                    // Partial text is partial: it is shown as such and a client
                    // must not draw it as a finished message.
                    hints(r).status = "ABORTED".into();
                    hints(r).renderable = false;
                    if let Some(Facts::Stream(s)) = &mut r.facts {
                        s.phase = "ABORTED".into();
                        s.last_offset = e.offset;
                        s.delta_count = p["delta_count"].as_u64().unwrap_or(0) as u32;
                        s.abort_source = p["source"].as_str().unwrap_or_default().into();
                        s.abort_code = p["code"].as_str().unwrap_or_default().into();
                    }
                }
            }
            // ---- tool calls
            "ToolCallProposed" => {
                let tid = modbit_domain::ToolCallId::from_bytes(env.aggregate_id).to_string();
                let name = p["tool_name"].as_str().unwrap_or_default().to_owned();
                let class = tool_class(&name);
                let args_ref = p["arguments_ref"].as_str().unwrap_or_default().to_owned();
                let short = if args_ref.is_empty() {
                    String::new()
                } else {
                    object_text(store, &args_ref, 8 * 1024)
                        .and_then(|(t, _)| serde_json::from_str::<serde_json::Value>(&t).ok())
                        .map(|a| short_text(&a))
                        .unwrap_or_default()
                };
                let turn = p["turn_id"].as_str().map(str::to_owned).or(turn);
                let mut r = row(
                    format!("tool:{tid}"),
                    wire::TranscriptRowKind::ToolCard,
                    e,
                    turn,
                );
                hints(&mut r).groupable = matches!(class, "READ" | "SEARCH" | "SHELL");
                hints(&mut r).short_text = short.clone();
                hints(&mut r).status = "PENDING".into();
                r.text = format!("{name} {short}").trim().to_owned();
                r.facts = Some(Facts::Tool(wire::ToolFacts {
                    tool_call_id: tid.clone(),
                    tool_name: name,
                    tool_class: class.into(),
                    effect_class: p["effect_class"].as_str().unwrap_or_default().into(),
                    state: "PROPOSED".into(),
                    call_id: p["call_id"].as_str().unwrap_or_default().into(),
                    arguments_ref: args_ref,
                    ..Default::default()
                }));
                tools.insert(tid, atoms.len());
                atoms.push(r);
            }
            "ToolCallValidated"
            | "ToolCallPolicyDecision"
            | "ToolCallApprovalRequested"
            | "ToolCallDispatched"
            | "ToolCallSucceeded"
            | "ToolCallFailed"
            | "ToolCallCancelled"
            | "ToolCallUnknownOutcome" => {
                let tid = modbit_domain::ToolCallId::from_bytes(env.aggregate_id).to_string();
                if let Some(&i) = tools.get(&tid) {
                    let (state, status) = match env.event_type.as_str() {
                        "ToolCallValidated" => ("VALIDATED", "PENDING"),
                        "ToolCallPolicyDecision" => (
                            if p["allowed"].as_bool().unwrap_or(false) {
                                "POLICY_CHECKED"
                            } else {
                                "VALIDATED"
                            },
                            "PENDING",
                        ),
                        "ToolCallApprovalRequested" => ("APPROVAL_PENDING", "AWAITING_APPROVAL"),
                        "ToolCallDispatched" => ("DISPATCHED", "RUNNING"),
                        "ToolCallSucceeded" => ("SUCCEEDED", "SUCCEEDED"),
                        "ToolCallFailed" => ("FAILED", "FAILED"),
                        "ToolCallCancelled" => ("CANCELLED", "CANCELLED"),
                        _ => ("UNKNOWN_OUTCOME", "UNKNOWN_OUTCOME"),
                    };
                    let r = &mut atoms[i];
                    r.last_offset = e.offset;
                    hints(r).status = status.into();
                    if env.event_type == "ToolCallDispatched" {
                        // Held in the duration until it ends.
                        hints(r).duration_ms = 0;
                    }
                    if matches!(
                        status,
                        "SUCCEEDED" | "FAILED" | "CANCELLED" | "UNKNOWN_OUTCOME"
                    ) {
                        let started =
                            r.at.as_ref()
                                .map_or(0, |t| t.seconds * 1000 + i64::from(t.nanos) / 1_000_000);
                        hints(r).duration_ms = (env.occurred_at.millis() - started).max(0) as u64;
                    }
                    if let Some(Facts::Tool(t)) = &mut r.facts {
                        t.state = state.into();
                        if let Some(rr) = p["result_ref"].as_str() {
                            t.result_ref = rr.into();
                        }
                        if let Some(c) = p["failure_code"].as_str() {
                            t.failure_code = c.into();
                        }
                    }
                }
            }
            "FileChanged" => {
                let tid = p["tool_call_id"].as_str().unwrap_or_default().to_owned();
                let entry = edits.entry(tid).or_default();
                if let Some(path) = p["path"].as_str() {
                    entry.0.push(path.to_owned());
                }
                entry.1 += p["lines_added"].as_u64().unwrap_or(0) as u32;
                entry.2 += p["lines_removed"].as_u64().unwrap_or(0) as u32;
            }
            // ---- approvals
            "ApprovalRequested" => {
                let aid = modbit_domain::ApprovalId::from_bytes(env.aggregate_id).to_string();
                let tid = p["tool_call_id"].as_str().unwrap_or_default().to_owned();
                let turn = tools
                    .get(&tid)
                    .map(|&i| atoms[i].turn_id.clone())
                    .filter(|t| !t.is_empty())
                    .or(turn);
                let name = p["tool_name"].as_str().unwrap_or_default().to_owned();
                let mut r = row(
                    format!("approval:{aid}"),
                    wire::TranscriptRowKind::ApprovalCard,
                    e,
                    turn,
                );
                hints(&mut r).status = "REQUESTED".into();
                r.text = format!(
                    "{name} asks for a {} effect",
                    p["effect_class"].as_str().unwrap_or_default()
                );
                r.facts = Some(Facts::Approval(wire::ApprovalFacts {
                    kind: "APPROVAL".into(),
                    approval_id: aid,
                    tool_call_id: tid,
                    tool_name: name,
                    effect_class: p["effect_class"].as_str().unwrap_or_default().into(),
                    state: "REQUESTED".into(),
                    resolver: String::new(),
                }));
                approvals.insert(env.aggregate_id, atoms.len());
                atoms.push(r);
            }
            "ApprovalResolved" | "ApprovalExpired" => {
                if let Some(&i) = approvals.get(&env.aggregate_id) {
                    let state = if env.event_type == "ApprovalExpired" {
                        ApprovalState::Expired
                    } else if p["approved"].as_bool().unwrap_or(false) {
                        ApprovalState::Approved
                    } else {
                        ApprovalState::Denied
                    };
                    let label = format!("{state:?}").to_uppercase();
                    let r = &mut atoms[i];
                    r.last_offset = e.offset;
                    hints(r).status = label.clone();
                    if let Some(Facts::Approval(a)) = &mut r.facts {
                        a.state = label;
                        a.resolver = p["resolver"].as_str().unwrap_or_default().into();
                    }
                }
            }
            // ---- a background terminal ended (REQ-PX-043): the end the Core
            // recorded, who ended it and why. A kill is `STOPPED`; it is a
            // row of its own, never a message in the conversation.
            "BackgroundProcessEnded" => {
                let handle = p["handle_id"].as_str().unwrap_or_default().to_owned();
                let how = p["how"].as_str().unwrap_or("EXITED");
                let status = match how {
                    "KILLED" => "STOPPED",
                    "LOST" => "LOST",
                    _ => "EXITED",
                };
                let by = p["ended_by"].as_str().unwrap_or_default();
                let reason = p["reason"].as_str().unwrap_or_default();
                let mut r = row(
                    format!("terminal:{handle}"),
                    wire::TranscriptRowKind::ToolCard,
                    e,
                    turn.clone().or_else(|| open_turn.clone()),
                );
                hints(&mut r).status = status.into();
                hints(&mut r).short_text = format!(
                    "terminal {} {}{}",
                    &handle[..handle.len().min(8)],
                    status.to_lowercase(),
                    if by.is_empty() {
                        String::new()
                    } else {
                        format!(" by {by}")
                    }
                );
                r.text = if reason.is_empty() {
                    hints(&mut r).short_text.clone()
                } else {
                    format!("{}: {}", hints(&mut r).short_text, bounded(reason, 256).0)
                };
                r.facts = Some(Facts::Tool(wire::ToolFacts {
                    tool_call_id: String::new(),
                    tool_name: "shell.background".into(),
                    tool_class: "SHELL".into(),
                    effect_class: String::new(),
                    state: status.into(),
                    result_ref: p["output_ref"].as_str().unwrap_or_default().into(),
                    ..Default::default()
                }));
                atoms.push(r);
            }
            // ---- turns
            "TurnPrepared" => {
                let t = turn.clone().unwrap_or_default();
                open_turn = Some(t.clone());
                turns.entry(t).or_default().started_ms = env.occurred_at.millis();
            }
            "ModelUsageRecorded" => {
                if let Some(t) = &turn {
                    let info = turns.entry(t.clone()).or_default();
                    info.input_tokens += p["input_tokens"].as_u64().unwrap_or(0);
                    info.output_tokens += p["output_tokens"].as_u64().unwrap_or(0);
                }
            }
            "TurnCompleted" | "TurnInterrupted" | "TurnFailed" => {
                let t = turn.clone().unwrap_or_default();
                let info = turns.entry(t.clone()).or_default();
                let outcome = match env.event_type.as_str() {
                    "TurnCompleted" => "COMPLETED",
                    "TurnInterrupted" => "INTERRUPTED",
                    _ => "FAILED",
                };
                let duration = (env.occurred_at.millis() - info.started_ms).max(0) as u64;
                let mut r = row(
                    format!("turn:{t}"),
                    wire::TranscriptRowKind::TurnFooter,
                    e,
                    Some(t),
                );
                hints(&mut r).status = outcome.into();
                hints(&mut r).duration_ms = duration;
                r.facts = Some(Facts::Footer(wire::FooterFacts {
                    turn_id: r.turn_id.clone(),
                    run_id: env.run_id.map(|x| x.to_string()).unwrap_or_default(),
                    outcome: outcome.into(),
                    failure_code: p["failure_code"].as_str().unwrap_or_default().into(),
                    duration_ms: duration,
                    input_tokens: info.input_tokens,
                    output_tokens: info.output_tokens,
                }));
                info.footer = Some(atoms.len());
                if open_turn.as_ref() == turn.as_ref() {
                    open_turn = None;
                }
                atoms.push(r);
            }
            _ => {}
        }
    }
    // Edit facts land on the tool cards that made them.
    for (tid, (paths, added, removed)) in edits {
        if let Some(&i) = tools.get(&tid) {
            let r = &mut atoms[i];
            hints(r).lines_added = added;
            hints(r).lines_removed = removed;
            if let Some(Facts::Tool(t)) = &mut r.facts {
                t.paths = paths;
            }
        }
    }
    // Streams still open or aborted show the text received so far (bounded).
    for (agg, &i) in &streams {
        let phase = match &atoms[i].facts {
            Some(Facts::Stream(s)) => s.phase.clone(),
            _ => continue,
        };
        if phase == "COMPLETED" {
            continue;
        }
        let deltas = store
            .read_aggregate(agg, 0, usize::MAX)
            .map_err(|e| ("STORE_ERROR", e.to_string()))?;
        let mut text = String::new();
        let mut count = 0u32;
        let mut last = atoms[i].last_offset;
        for d in deltas.iter().filter(|d| d.offset <= until) {
            if d.envelope.event_type != "AssistantTextDelta" {
                continue;
            }
            count += 1;
            last = last.max(d.offset);
            if text.len() < TEXT_MAX
                && let Some(t) = store
                    .payload(&d.envelope)
                    .ok()
                    .and_then(|p| p["text"].as_str().map(str::to_owned))
            {
                text.push_str(&t);
            }
        }
        let (text, cut) = bounded(&text, TEXT_MAX);
        let r = &mut atoms[i];
        r.text = text;
        r.text_truncated = cut;
        r.last_offset = r.last_offset.max(last);
        if let Some(Facts::Stream(s)) = &mut r.facts {
            s.delta_count = count;
            s.last_offset = s.last_offset.max(last);
        }
    }
    // A reasoning stream marks the turn's rows.
    for r in &mut atoms {
        if turns.get(&r.turn_id).is_some_and(|t| t.reasoning) {
            hints(r).has_reasoning = true;
        }
    }
    Ok(Folded {
        atoms,
        turns,
        attention,
    })
}

fn group_label(kind: &str, facts: &wire::GroupFacts) -> String {
    match kind {
        "explore" => {
            let n = facts.reads + facts.searches;
            format!("Explored {n} {}", if n == 1 { "item" } else { "items" })
        }
        "shell" => format!(
            "Ran {} {}",
            facts.commands,
            if facts.commands == 1 {
                "command"
            } else {
                "commands"
            }
        ),
        _ => format!("Worked for {}", duration_words(facts.duration_ms)),
    }
}

fn duration_words(ms: u64) -> String {
    let s = ms / 1000;
    if ms < 1000 {
        "under a second".to_owned()
    } else if s < 60 {
        format!("{s} s")
    } else {
        format!("{} min {} s", s / 60, s % 60)
    }
}

/// Fold `members` (detailed atoms, in order) into one work group.
fn group(
    members: Vec<wire::TranscriptRow>,
    kind: &str,
    turn: &str,
    duration_ms: u64,
    open: bool,
) -> wire::TranscriptRow {
    let mut g = wire::GroupFacts {
        turn_id: turn.to_owned(),
        steps: members.len() as u32,
        duration_ms,
        open,
        ..Default::default()
    };
    for m in &members {
        if let Some(Facts::Tool(t)) = &m.facts {
            match t.tool_class.as_str() {
                "READ" => g.reads += 1,
                "SEARCH" => g.searches += 1,
                "SHELL" => g.commands += 1,
                "EDIT" => g.edits += 1,
                _ => g.others += 1,
            }
        } else {
            g.others += 1;
        }
        let h = m.hints.clone().unwrap_or_default();
        g.lines_added += h.lines_added;
        g.lines_removed += h.lines_removed;
    }
    g.label = group_label(kind, &g);
    let first = &members[0];
    let mut children = members.clone();
    for (i, c) in children.iter_mut().enumerate() {
        c.ordinal = i as u32 + 1;
    }
    wire::TranscriptRow {
        ordinal: 0,
        row_id: format!("group:{kind}:{}", first.row_id),
        kind: wire::TranscriptRowKind::WorkGroup as i32,
        offset: first.offset,
        last_offset: members
            .iter()
            .map(|m| m.last_offset)
            .max()
            .unwrap_or(first.offset),
        at: first.at,
        turn_id: turn.to_owned(),
        hints: Some(wire::RowHints {
            renderable: true,
            groupable: false,
            has_reasoning: members
                .iter()
                .any(|m| m.hints.as_ref().is_some_and(|h| h.has_reasoning)),
            duration_ms,
            short_text: g.label.clone(),
            lines_added: g.lines_added,
            lines_removed: g.lines_removed,
            status: if open {
                "RUNNING".into()
            } else {
                "COMPLETE".into()
            },
        }),
        text: g.label.clone(),
        text_truncated: false,
        text_ref: String::new(),
        children,
        facts: Some(Facts::Group(g)),
    }
}

fn explore_or_shell(r: &wire::TranscriptRow) -> Option<&'static str> {
    if r.kind != wire::TranscriptRowKind::ToolCard as i32
        || !r.hints.as_ref().is_some_and(|h| h.groupable)
    {
        return None;
    }
    match &r.facts {
        Some(Facts::Tool(t)) => match t.tool_class.as_str() {
            "READ" | "SEARCH" => Some("explore"),
            "SHELL" => Some("shell"),
            _ => None,
        },
        _ => None,
    }
}

fn apply_density(f: &Folded, density: wire::TranscriptDensity) -> Vec<wire::TranscriptRow> {
    use wire::TranscriptDensity as D;
    match density {
        D::Detailed => f.atoms.clone(),
        D::Balanced => {
            let mut out: Vec<wire::TranscriptRow> = Vec::new();
            let mut run: Vec<wire::TranscriptRow> = Vec::new();
            let mut run_kind = "";
            let flush = |out: &mut Vec<wire::TranscriptRow>,
                         run: &mut Vec<wire::TranscriptRow>,
                         kind: &str| {
                if run.len() >= 2 {
                    let turn = run[0].turn_id.clone();
                    let dur: u64 = run
                        .iter()
                        .filter_map(|r| r.hints.as_ref())
                        .map(|h| h.duration_ms)
                        .sum();
                    let open = run.iter().any(|r| {
                        r.hints.as_ref().is_some_and(|h| {
                            matches!(
                                h.status.as_str(),
                                "PENDING" | "RUNNING" | "AWAITING_APPROVAL"
                            )
                        })
                    });
                    let members = std::mem::take(run);
                    out.push(group(members, kind, &turn, dur, open));
                } else {
                    out.append(run);
                }
            };
            for a in &f.atoms {
                match explore_or_shell(a) {
                    Some(k)
                        if (run.is_empty() || (k == run_kind && a.turn_id == run[0].turn_id)) =>
                    {
                        run_kind = k;
                        run.push(a.clone());
                    }
                    Some(k) => {
                        flush(&mut out, &mut run, run_kind);
                        run_kind = k;
                        run.push(a.clone());
                    }
                    None => {
                        flush(&mut out, &mut run, run_kind);
                        out.push(a.clone());
                    }
                }
            }
            flush(&mut out, &mut run, run_kind);
            out
        }
        _ => {
            // Compact: a finished turn's steps fold under one header, at the
            // place of its first step; a turn still running shows its steps.
            let finished: std::collections::HashSet<&String> = f
                .turns
                .iter()
                .filter(|(_, t)| t.footer.is_some())
                .map(|(k, _)| k)
                .collect();
            let mut members: HashMap<String, Vec<wire::TranscriptRow>> = HashMap::new();
            let mut member_ids: std::collections::HashSet<String> =
                std::collections::HashSet::new();
            for a in &f.atoms {
                let step = a.kind == wire::TranscriptRowKind::ToolCard as i32
                    || (a.kind == wire::TranscriptRowKind::ApprovalCard as i32
                        && !a.turn_id.is_empty()
                        && a.hints.as_ref().is_some_and(|h| h.status != "REQUESTED"));
                if step && finished.contains(&a.turn_id) {
                    member_ids.insert(a.row_id.clone());
                    members
                        .entry(a.turn_id.clone())
                        .or_default()
                        .push(a.clone());
                }
            }
            let mut out = Vec::new();
            let mut placed: std::collections::HashSet<String> = std::collections::HashSet::new();
            for a in &f.atoms {
                if !member_ids.contains(&a.row_id) {
                    out.push(a.clone());
                } else if placed.insert(a.turn_id.clone()) {
                    let m = members.remove(&a.turn_id).unwrap_or_default();
                    let dur = f
                        .turns
                        .get(&a.turn_id)
                        .and_then(|t| t.footer)
                        .map_or(0, |i| {
                            f.atoms[i].hints.as_ref().map_or(0, |h| h.duration_ms)
                        });
                    out.push(group(m, "turn", &a.turn_id, dur, false));
                }
            }
            out
        }
    }
}

/// What a page is asked for.
pub(crate) struct PageRequest {
    pub density: wire::TranscriptDensity,
    pub after_row: u32,
    pub limit: u32,
    pub as_of_offset: u64,
}

fn tail_of(task: &Task, f: &Folded, offset: u64, at_ms: i64) -> wire::TranscriptRow {
    let (phase, label, detail) = match task.state {
        TaskState::Created | TaskState::Queued => ("IDLE", "Not started".to_owned(), String::new()),
        TaskState::Running => {
            // The newest atom says what the agent is doing.
            let label = f
                .atoms
                .iter()
                .rev()
                .find(|a| {
                    matches!(
                        wire::TranscriptRowKind::try_from(a.kind),
                        Ok(wire::TranscriptRowKind::ToolCard
                            | wire::TranscriptRowKind::AssistantMessage
                            | wire::TranscriptRowKind::ApprovalCard)
                    )
                })
                .map(
                    |a| match (&a.facts, a.hints.as_ref().map(|h| h.status.as_str())) {
                        (Some(Facts::Stream(s)), _) if s.phase == "OPEN" => "Responding".to_owned(),
                        (Some(Facts::Approval(_)), Some("REQUESTED")) => {
                            "Waiting for approval".to_owned()
                        }
                        (
                            Some(Facts::Tool(t)),
                            Some("PENDING" | "RUNNING" | "AWAITING_APPROVAL"),
                        ) => {
                            let short = a
                                .hints
                                .as_ref()
                                .map(|h| h.short_text.clone())
                                .unwrap_or_default();
                            let with = |w: &str| {
                                if short.is_empty() {
                                    w.to_owned()
                                } else {
                                    format!("{w}: {short}")
                                }
                            };
                            match t.tool_class.as_str() {
                                "READ" => with("Reading"),
                                "SEARCH" => with("Searching"),
                                "SHELL" => with("Running a command"),
                                "EDIT" => with("Editing"),
                                "BROWSER" => "Using the browser".to_owned(),
                                _ => format!("Running {}", t.tool_name),
                            }
                        }
                        _ => "Planning".to_owned(),
                    },
                )
                .unwrap_or_else(|| "Planning".to_owned());
            ("RUNNING", label, String::new())
        }
        TaskState::Waiting(reason) => match (reason, &f.attention) {
            (WaitReason::Approval, _) => {
                ("WAITING", "Waiting for approval".to_owned(), String::new())
            }
            (WaitReason::UserInput, _) => (
                "WAITING",
                "Waiting for your answer".to_owned(),
                String::new(),
            ),
            (_, Some(why)) => ("NEEDS_ATTENTION", "Needs attention".to_owned(), why.clone()),
            (_, None) => ("WAITING", "Waiting".to_owned(), String::new()),
        },
        TaskState::ReadyForReview => (
            "READY_FOR_REVIEW",
            "Ready for review".to_owned(),
            String::new(),
        ),
        TaskState::Completed => ("COMPLETED", "Completed".to_owned(), String::new()),
        TaskState::Failed => (
            "FAILED",
            "Failed".to_owned(),
            task.failure_code.clone().unwrap_or_default(),
        ),
        TaskState::Cancelled => ("CANCELLED", "Cancelled".to_owned(), String::new()),
    };
    wire::TranscriptRow {
        ordinal: 0,
        row_id: "tail".into(),
        kind: wire::TranscriptRowKind::TailStatus as i32,
        offset,
        last_offset: offset,
        at: ts(at_ms),
        turn_id: String::new(),
        hints: Some(wire::RowHints {
            renderable: true,
            status: phase.into(),
            short_text: label.clone(),
            ..Default::default()
        }),
        text: label.clone(),
        text_truncated: false,
        text_ref: String::new(),
        children: vec![],
        facts: Some(Facts::Tail(wire::TailFacts {
            phase: phase.into(),
            label,
            detail,
        })),
    }
}

fn row_ms(r: &wire::TranscriptRow) -> i64 {
    r.at.as_ref()
        .map_or(0, |t| t.seconds * 1000 + i64::from(t.nanos) / 1_000_000)
}

fn day_of(ms: i64) -> String {
    // Days since the epoch to a civil date (proleptic Gregorian, UTC).
    let z = ms.div_euclid(86_400_000) + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{y:04}-{m:02}-{d:02}")
}

/// The page of a task's conversation asked for.
pub(crate) fn page(
    store: &EventStore,
    task: &Task,
    req: &PageRequest,
) -> Result<wire::TranscriptPage, Refusal> {
    let tip = store
        .last_offset()
        .map_err(|e| ("STORE_ERROR", e.to_string()))?;
    if req.as_of_offset > tip {
        return Err((
            "INVALID_CURSOR",
            format!(
                "as_of_offset {} is beyond the log at {tip}",
                req.as_of_offset
            ),
        ));
    }
    let as_of = if req.as_of_offset == 0 {
        tip
    } else {
        req.as_of_offset
    };
    let density = match req.density {
        wire::TranscriptDensity::Unspecified => wire::TranscriptDensity::Compact,
        d => d,
    };
    let before = store.events_read();
    let folded = fold(store, task, as_of)?;
    let read_offset = store
        .task_digests(&task.session_id)
        .map_err(|e| ("STORE_ERROR", e.to_string()))?
        .into_iter()
        .find(|d| d.task_id == task.task_id)
        .map_or(0, |d| d.read_offset);
    let mut top = apply_density(&folded, density);
    // The unread divider: before the first row that landed after the person
    // last looked, when they have looked at all.
    if read_offset > 0
        && let Some(i) = top.iter().position(|r| r.offset > read_offset)
        && i > 0
    {
        let unread = top[i..]
            .iter()
            .filter(|r| {
                matches!(
                    wire::TranscriptRowKind::try_from(r.kind),
                    Ok(wire::TranscriptRowKind::UserMessage
                        | wire::TranscriptRowKind::AssistantMessage
                        | wire::TranscriptRowKind::ApprovalCard)
                )
            })
            .count();
        let at = top[i].clone();
        top.insert(
            i,
            wire::TranscriptRow {
                ordinal: 0,
                row_id: "unread".into(),
                kind: wire::TranscriptRowKind::UnreadDivider as i32,
                offset: at.offset,
                last_offset: at.offset,
                at: at.at,
                turn_id: String::new(),
                hints: Some(wire::RowHints {
                    renderable: true,
                    ..Default::default()
                }),
                text: format!("{unread} new"),
                text_truncated: false,
                text_ref: String::new(),
                children: vec![],
                facts: None,
            },
        );
    }
    // Time boundaries between rows far apart or on different UTC days.
    let mut with_gaps: Vec<wire::TranscriptRow> = Vec::with_capacity(top.len() + 4);
    let mut prev_ms: Option<i64> = None;
    for r in top {
        let ms = row_ms(&r);
        if let Some(p) = prev_ms
            && r.kind != wire::TranscriptRowKind::UnreadDivider as i32
            && (ms - p >= GAP_MS || day_of(ms) != day_of(p))
        {
            with_gaps.push(wire::TranscriptRow {
                ordinal: 0,
                row_id: format!("time:{}", r.row_id),
                kind: wire::TranscriptRowKind::TimeBoundary as i32,
                offset: r.offset,
                last_offset: r.offset,
                at: r.at,
                turn_id: String::new(),
                hints: Some(wire::RowHints {
                    renderable: true,
                    ..Default::default()
                }),
                text: day_of(ms),
                text_truncated: false,
                text_ref: String::new(),
                children: vec![],
                facts: Some(Facts::Boundary(wire::BoundaryFacts {
                    gap_ms: (ms - p).max(0) as u64,
                    day: day_of(ms),
                })),
            });
        }
        if r.kind != wire::TranscriptRowKind::UnreadDivider as i32 {
            prev_ms = Some(ms);
        }
        with_gaps.push(r);
    }
    let last_at = with_gaps.last().map_or(0, row_ms);
    let tail = tail_of(task, &folded, as_of, last_at.max(task.created_at.millis()));
    with_gaps.push(tail);
    for (i, r) in with_gaps.iter_mut().enumerate() {
        r.ordinal = i as u32 + 1;
    }
    let total = with_gaps.len();
    let after = req.after_row as usize;
    if after > total {
        return Err((
            "INVALID_CURSOR",
            format!(
                "after_row {after} is beyond the {total} rows of this view; read again from the start"
            ),
        ));
    }
    let limit = match req.limit as usize {
        0 => DEFAULT_PAGE,
        n => n.min(MAX_PAGE),
    };
    let rows: Vec<wire::TranscriptRow> = with_gaps.into_iter().skip(after).take(limit).collect();
    let end = after + rows.len();
    let has_more = end < total;
    Ok(wire::TranscriptPage {
        task_id: Some(crate::server::wire_id(task.task_id.as_bytes())),
        density: density as i32,
        rows,
        total_rows: total as u32,
        next_after_row: if has_more { end as u32 } else { 0 },
        has_more,
        as_of_offset: as_of,
        last_offset: tip,
        task_state: task_state_label(&task.state),
        read_offset,
        events_read: store.events_read() - before,
    })
}

fn task_state_label(s: &TaskState) -> String {
    match s {
        TaskState::Waiting(_) => "Waiting".to_owned(),
        other => format!("{other:?}"),
    }
}

// ---------------------------------------------------------------- headers

/// What decides a status class.
pub(crate) struct StatusInputs {
    pub state: TaskState,
    pub attention_items: usize,
    pub unseen: bool,
    pub archived: bool,
}

/// The status class of a task and the words for it, by the precedence of
/// AFW-B02: needs attention, failed, ready for review (unseen before seen),
/// running, waiting, completed, draft, archived. The first that holds wins.
pub(crate) fn status_class(i: &StatusInputs) -> (wire::AgentStatusClass, &'static str) {
    use wire::AgentStatusClass as C;
    if i.attention_items > 0 {
        return (C::NeedsAttention, "Needs attention");
    }
    if i.state == TaskState::Failed {
        return (C::Failed, "Failed");
    }
    if i.state == TaskState::ReadyForReview {
        return if i.unseen {
            (C::ReadyForReviewUnseen, "Ready for review")
        } else {
            (C::ReadyForReviewSeen, "Ready for review (seen)")
        };
    }
    match i.state {
        TaskState::Running => return (C::Running, "Running"),
        TaskState::Waiting(_) => return (C::Waiting, "Waiting"),
        TaskState::Completed => return (C::Completed, "Completed"),
        TaskState::Cancelled => return (C::Completed, "Cancelled"),
        _ => {}
    }
    if matches!(i.state, TaskState::Created | TaskState::Queued) && !i.archived {
        return (C::Draft, "Draft");
    }
    (C::Archived, "Archived")
}

/// The title of a conversation: the first line of its goal, bounded.
pub(crate) fn title_of(t: &Task) -> String {
    bounded(t.goal_text.lines().next().unwrap_or_default().trim(), 120).0
}

fn subtitle_of(root: &str) -> String {
    std::path::Path::new(root)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// The headers of a session's tasks, newest activity first.
pub(crate) fn headers(
    core: &Core,
    store: &EventStore,
    session: SessionId,
    include_archived: bool,
) -> Result<wire::AgentHeaders, Refusal> {
    let events_before = store.events_read();
    let objects_before = store.objects().reads();
    let tasks = store
        .session_tasks(&session)
        .map_err(|e| ("STORE_ERROR", e.to_string()))?;
    let digests: HashMap<_, _> = store
        .task_digests(&session)
        .map_err(|e| ("STORE_ERROR", e.to_string()))?
        .into_iter()
        .map(|d| (d.task_id, d))
        .collect();
    let items = crate::attention::attention_of(store, session, |t| {
        digests
            .get(&t.task_id)
            .is_some_and(|d| d.attention_events > 0)
    });
    let pending: std::collections::HashSet<_> = store
        .approvals_for_session(&session)
        .unwrap_or_default()
        .into_iter()
        .filter(|a| a.state == ApprovalState::Requested)
        .map(|a| a.task_id)
        .collect();
    let projects = crate::projects::memberships(store);
    // A task that works in a worktree of its own names the checkout the
    // worktree was made from (`checkout_root`): the repository the person
    // knows. Its `workspace_root` stays the root it really works in.
    let worktrees = crate::worktrees::registry_of_session(store, &session);
    let checkout_of = |root: &str| -> Option<String> {
        worktrees
            .iter()
            .find(|r| !r.removed && crate::worktrees::same_path(&r.path, root))
            .map(|r| r.origin_root.clone())
    };
    let mut out: Vec<wire::AgentHeader> = Vec::new();
    for t in tasks {
        let Some(d) = digests.get(&t.task_id) else {
            continue;
        };
        if d.archived && !include_archived {
            continue;
        }
        let attention_items = items.iter().filter(|i| i.task_id == t.task_id).count();
        let (class, label) = status_class(&StatusInputs {
            state: t.state,
            attention_items,
            unseen: d.visible_offset > d.read_offset,
            archived: d.archived,
        });
        let context_percent = match (&d.model, d.context_tokens) {
            (Some((endpoint, model)), Some(tokens)) => core
                .gateway
                .capability(endpoint, model)
                .filter(|c| c.context_tokens > 0)
                .map_or(0, |c| {
                    u32::try_from((tokens * 100 / u64::from(c.context_tokens)).min(100))
                        .unwrap_or(100)
                }),
            _ => 0,
        };
        out.push(wire::AgentHeader {
            task_id: Some(crate::server::wire_id(t.task_id.as_bytes())),
            session_id: Some(crate::server::wire_id(t.session_id.as_bytes())),
            workspace_root: t.workspace_root.clone().unwrap_or_default(),
            title: title_of(&t),
            subtitle: t
                .workspace_root
                .as_deref()
                .map(subtitle_of)
                .unwrap_or_default(),
            created_at: ts(t.created_at.millis()),
            updated_at: ts(d.last_at.millis()),
            status_class: class as i32,
            status_label: label.into(),
            unread: d.visible_offset > d.read_offset,
            pending_approval: pending.contains(&t.task_id),
            // Plan mode is the Core's plan gate (docs/65 AFW-D05): the task
            // records a plan and writes nothing until the person accepts it,
            // and acceptance is leaving PLAN (`SetTaskMode`, which carries
            // the plan version accepted). So a plan is pending exactly while
            // the task is in PLAN mode, has a plan version on its log and has
            // not ended; outside PLAN mode a recorded plan is the model's
            // own plan-before-write step and nothing holds it for a decision.
            pending_plan: !t.state.is_terminal()
                && d.mode.as_deref() == Some("PLAN")
                && d.plan_versions > 0,
            context_percent,
            files_changed: d.files_changed,
            lines_added: d.lines_added,
            lines_removed: d.lines_removed,
            last_checkpoint_at: d.last_checkpoint_at.and_then(|c| ts(c.millis())),
            subagent: t.origin == TaskOrigin::Subagent,
            archived: d.archived,
            execution_location: if t.execution_profile == "cloud_isolated" {
                "cloud"
            } else {
                "local"
            }
            .into(),
            origin: format!("{:?}", t.origin).to_lowercase(),
            task_state: task_state_label(&t.state),
            last_offset: d.last_offset,
            read_offset: d.read_offset,
            attention_items: attention_items as u32,
            project_id: projects
                .get(&t.task_id)
                .map(|p| crate::server::wire_id(p.as_bytes())),
            checkout_root: t
                .workspace_root
                .as_deref()
                .and_then(checkout_of)
                .unwrap_or_default(),
        });
    }
    out.sort_by(|a, b| {
        b.updated_at
            .as_ref()
            .map(|t| (t.seconds, t.nanos))
            .cmp(&a.updated_at.as_ref().map(|t| (t.seconds, t.nanos)))
    });
    Ok(wire::AgentHeaders {
        session_id: Some(crate::server::wire_id(session.as_bytes())),
        headers: out,
        last_offset: store.last_offset().unwrap_or(0),
        events_read: store.events_read() - events_before,
        objects_read: store.objects().reads() - objects_before,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use wire::AgentStatusClass as C;

    fn inputs(
        state: TaskState,
        attention: usize,
        unseen: bool,
        archived: bool,
    ) -> (C, &'static str) {
        status_class(&StatusInputs {
            state,
            attention_items: attention,
            unseen,
            archived,
        })
    }

    #[test]
    fn the_first_condition_of_the_precedence_wins_when_several_coincide() {
        // needs attention beats failed, ready, running, waiting
        assert_eq!(
            inputs(TaskState::Failed, 1, true, true).0,
            C::NeedsAttention
        );
        assert_eq!(
            inputs(TaskState::Running, 2, false, false).0,
            C::NeedsAttention
        );
        // failed beats the rest
        assert_eq!(inputs(TaskState::Failed, 0, true, true).0, C::Failed);
        // ready for review: unseen before seen, ahead of running
        assert_eq!(
            inputs(TaskState::ReadyForReview, 0, true, false).0,
            C::ReadyForReviewUnseen
        );
        assert_eq!(
            inputs(TaskState::ReadyForReview, 0, false, false).0,
            C::ReadyForReviewSeen
        );
        assert_eq!(inputs(TaskState::Running, 0, true, false).0, C::Running);
        assert_eq!(
            inputs(TaskState::Waiting(WaitReason::External), 0, false, false).0,
            C::Waiting
        );
        assert_eq!(inputs(TaskState::Completed, 0, true, false).0, C::Completed);
        assert_eq!(inputs(TaskState::Created, 0, false, false).0, C::Draft);
        assert_eq!(inputs(TaskState::Created, 0, false, true).0, C::Archived);
        // the class order is the precedence order
        assert!((C::NeedsAttention as i32) < (C::Failed as i32));
        assert!((C::Failed as i32) < (C::ReadyForReviewUnseen as i32));
        assert!((C::ReadyForReviewSeen as i32) < (C::Running as i32));
        assert!((C::Completed as i32) < (C::Draft as i32));
    }

    #[test]
    fn days_are_utc_civil_dates() {
        assert_eq!(day_of(0), "1970-01-01");
        assert_eq!(day_of(1_700_000_000_000), "2023-11-14");
        assert_eq!(day_of(951_782_400_000), "2000-02-29");
    }

    #[test]
    fn tools_are_classed_by_what_they_do() {
        for (name, class) in [
            ("fs.read", "READ"),
            ("fs.write", "EDIT"),
            ("change.apply", "EDIT"),
            ("search.regex", "SEARCH"),
            ("lsp.references", "SEARCH"),
            ("shell.exec", "SHELL"),
            ("test.run", "SHELL"),
            ("browser.act", "BROWSER"),
            ("plan.update", "OTHER"),
            ("memory.propose", "OTHER"),
        ] {
            assert_eq!(tool_class(name), class, "{name}");
        }
        assert_eq!(
            short_text(&serde_json::json!({"argv": ["git", "grep", "x"]})),
            "git grep x"
        );
    }
}
