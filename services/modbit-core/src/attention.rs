//! Attention (REQ-EV-0151, REQ-EV-0275; docs/10 "Needs Attention", docs/32):
//! every item a person must act on, derived from the canonical unresolved
//! state of a session and nothing else — an approval still `REQUESTED`, a
//! question still unanswered, a task waiting with the reason its loop
//! recorded, a child a parent must decide about — each with the action that
//! clears it. There is no second scheduler and no reminder store: an item
//! exists exactly while the canonical fact it names is unresolved, and it
//! is gone the moment the resolving event lands (`ApprovalResolved`,
//! `UserQuestionAnswered`, `TaskResumed` / `TaskCompleted` /
//! `TaskCancelled`, `CapacityTicketGranted`, `SubagentResultRecorded`,
//! `SubagentAdmitted`).

use modbit_domain::approval::ApprovalState;
use modbit_domain::task::{Task, TaskState, WaitReason};
use modbit_domain::{SessionId, TaskId};
use modbit_event_store::EventStore;

/// One thing a person must act on.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub(crate) struct AttentionItem {
    /// `APPROVAL` | `QUESTION` | `CAPACITY` | `STALL` | `RESTART` |
    /// `FAILURE` | `BLOCKER` | `CONFLICT` | `PROTECTED_EFFECT`.
    pub kind: &'static str,
    /// The task it belongs to.
    pub task_id: TaskId,
    /// The canonical thing it names: an approval id, a question id, an
    /// agent id, a ticket dimension.
    pub reference: String,
    /// Why, in the words the log holds.
    pub reason: String,
    /// The command or tool that clears it.
    pub action: String,
    /// The offset of the event that raised it.
    pub since_offset: u64,
}

/// Every open item of a session, oldest first.
pub(crate) fn attention(store: &EventStore, session: SessionId) -> Vec<AttentionItem> {
    let mut items = Vec::new();
    // Approvals still requested (docs/23): resolved ones are gone with
    // `ApprovalResolved`.
    for a in store.approvals_for_session(&session).unwrap_or_default() {
        if a.state != ApprovalState::Requested {
            continue;
        }
        items.push(AttentionItem {
            kind: "APPROVAL",
            task_id: a.task_id,
            reference: a.approval_id.to_string(),
            reason: format!(
                "{} asks for a {:?} effect (intent {})",
                a.tool_name,
                a.effect_class,
                &a.intent_hash[..a.intent_hash.len().min(12)]
            ),
            action: format!("ResolveApproval {} (approve or deny)", a.approval_id),
            since_offset: 0,
        });
    }
    let tasks: Vec<Task> = store
        .open_tasks()
        .unwrap_or_default()
        .into_iter()
        .filter(|t| t.session_id == session)
        .collect();
    for t in &tasks {
        items.extend(task_items(store, t));
    }
    items.sort_by_key(|i| i.since_offset);
    items
}

/// The items one task raises, from its own log.
fn task_items(store: &EventStore, t: &Task) -> Vec<AttentionItem> {
    let mut out = Vec::new();
    let events = store
        .read_aggregate(t.task_id.as_bytes(), 0, usize::MAX)
        .unwrap_or_default();
    // The latest attention reason since the task last ran: a `TaskResumed`
    // or `TaskStarted` clears what came before it.
    let mut attention: Option<(u64, String, Option<serde_json::Value>)> = None;
    let mut open_question: Option<(u64, String, String)> = None;
    let mut conflicts: Vec<(u64, String, String)> = Vec::new();
    let mut protected: Vec<(u64, String, String, String)> = Vec::new();
    let mut ended_children: Vec<String> = Vec::new();
    let mut admitted_keys: Vec<(u64, String)> = Vec::new();
    // A start refused for capacity (M6.2) leaves the task queued with a
    // `CapacityDenied`; a later start or grant clears it.
    let mut capacity_denied: Option<(u64, String)> = None;
    for e in &events {
        let p = store.payload(&e.envelope).unwrap_or_default();
        match e.envelope.event_type.as_str() {
            "TaskStarted" | "TaskResumed" | "TaskCompleted" | "TaskCancelled" => {
                attention = None;
                capacity_denied = None;
            }
            "CapacityTicketGranted" => capacity_denied = None,
            "CapacityDenied" => {
                capacity_denied = Some((
                    e.offset,
                    format!(
                        "{}: {} needs {} of {} available (live: {})",
                        p["code"].as_str().unwrap_or_default(),
                        p["dimension"].as_str().unwrap_or_default(),
                        p["needed"].as_u64().unwrap_or(0),
                        p["available"].as_u64().unwrap_or(0),
                        p["live"]
                            .as_array()
                            .map(|l| l
                                .iter()
                                .filter_map(|x| x.as_str())
                                .collect::<Vec<_>>()
                                .join(", "))
                            .unwrap_or_default()
                    ),
                ));
            }
            "TaskNeedsAttention" => {
                attention = Some((
                    e.offset,
                    p["reason"].as_str().unwrap_or_default().to_owned(),
                    p.get("diagnostic").cloned().filter(|d| !d.is_null()),
                ));
            }
            "UserQuestionAsked" => {
                open_question = Some((
                    e.offset,
                    p["question_id"].as_str().unwrap_or_default().to_owned(),
                    p["question"].as_str().unwrap_or_default().to_owned(),
                ));
            }
            "UserQuestionAnswered" => {
                if open_question.as_ref().map(|q| q.1.as_str()) == p["question_id"].as_str() {
                    open_question = None;
                }
            }
            "SubagentAdmissionRefused" if p["code"] == "WRITE_CONFLICT" => {
                conflicts.push((
                    e.offset,
                    p["idempotency_key"].as_str().unwrap_or_default().to_owned(),
                    p["detail"].as_str().unwrap_or_default().to_owned(),
                ));
            }
            "SubagentAdmitted" => {
                admitted_keys.push((
                    e.offset,
                    p["idempotency_key"].as_str().unwrap_or_default().to_owned(),
                ));
            }
            "SubagentProtectedEffect" => {
                protected.push((
                    e.offset,
                    p["agent_id"].as_str().unwrap_or_default().to_owned(),
                    p["idempotency_key"].as_str().unwrap_or_default().to_owned(),
                    format!(
                        "{} ({}) above the child's ceiling {}",
                        p["tool"].as_str().unwrap_or_default(),
                        p["effect_class"].as_str().unwrap_or_default(),
                        p["ceiling"].as_str().unwrap_or_default()
                    ),
                ));
            }
            "SubagentResultRecorded" => {
                ended_children.push(p["agent_id"].as_str().unwrap_or_default().to_owned());
            }
            _ => {}
        }
    }
    if let Some((offset, detail)) = capacity_denied
        && matches!(t.state, TaskState::Queued | TaskState::Waiting(_))
    {
        out.push(AttentionItem {
            kind: "CAPACITY",
            task_id: t.task_id,
            reference: "CAPACITY_EXHAUSTED".into(),
            reason: detail,
            action: "StartTask again once a live run ends, or raise MODBIT_CAPACITY".to_owned(),
            since_offset: offset,
        });
    }
    let has_question = open_question.is_some();
    if let Some((offset, question_id, question)) = open_question {
        out.push(AttentionItem {
            kind: "QUESTION",
            task_id: t.task_id,
            reference: question_id.clone(),
            reason: question,
            action: format!("RespondToQuestion {question_id}, then StartTask to resume"),
            since_offset: offset,
        });
    }
    if let TaskState::Waiting(reason) = t.state
        && let Some((offset, text, diagnostic)) = attention
    {
        let class = diagnostic
            .as_ref()
            .and_then(|d| d["class"].as_str())
            .unwrap_or_default()
            .to_owned();
        let code = diagnostic
            .as_ref()
            .and_then(|d| d["code"].as_str())
            .unwrap_or_default()
            .to_owned();
        let (kind, action) = match reason {
            WaitReason::Capacity => (
                "CAPACITY",
                "wait: the run continues when a ticket is granted; free capacity or raise MODBIT_CAPACITY".to_owned(),
            ),
            WaitReason::Approval => (
                "APPROVAL",
                "resolve the pending approval; the run re-enters the same call".to_owned(),
            ),
            WaitReason::UserInput if has_question => ("QUESTION", String::new()),
            _ => {
                if code == "NO_PROGRESS" || text.contains("without progress") {
                    (
                        "STALL",
                        "steer the task (QueueInput STEER) and StartTask to resume, or CancelTask".to_owned(),
                    )
                } else if class == "RESTART" || code.starts_with("RESTART") || text.contains("runtime restarted") {
                    ("RESTART", "StartTask to resume at the recorded boundary".to_owned())
                } else if matches!(class.as_str(), "HARNESS" | "BUDGET" | "POLICY") {
                    (
                        "BLOCKER",
                        diagnostic
                            .as_ref()
                            .and_then(|d| d["user_action"].as_str())
                            .filter(|s| !s.is_empty())
                            .map(|s| format!("{s}; StartTask to resume"))
                            .unwrap_or_else(|| "StartTask to resume, or CancelTask".to_owned()),
                    )
                } else {
                    (
                        "FAILURE",
                        diagnostic
                            .as_ref()
                            .and_then(|d| d["user_action"].as_str())
                            .filter(|s| !s.is_empty())
                            .map(|s| format!("{s}; StartTask to retry"))
                            .unwrap_or_else(|| "StartTask to retry, or CancelTask".to_owned()),
                    )
                }
            }
        };
        // A question item already stands for a UserInput wait with an open
        // question, and an answered question is resolved whatever the
        // attention line said; an approval item stands for a live approval.
        let answered = code == "QUESTION_PENDING" && !has_question;
        if !action.is_empty() && kind != "APPROVAL" && !answered {
            out.push(AttentionItem {
                kind,
                task_id: t.task_id,
                reference: code,
                reason: text,
                action,
                since_offset: offset,
            });
        }
    }
    // A parent still running decides about its children: a conflict not
    // superseded by a later admission of the same key; a protected effect a
    // child reached and has not ended since.
    if t.state == TaskState::Running {
        for (offset, key, detail) in conflicts {
            if admitted_keys.iter().any(|(o, k)| *o > offset && *k == key) {
                continue;
            }
            out.push(AttentionItem {
                kind: "CONFLICT",
                task_id: t.task_id,
                reference: key.clone(),
                reason: format!("child {key} refused: {detail}"),
                action: "revise the child's write scope and spawn it again, or wait for the holder to finish".to_owned(),
                since_offset: offset,
            });
        }
        for (offset, agent_id, key, what) in protected {
            if ended_children.contains(&agent_id) {
                continue;
            }
            out.push(AttentionItem {
                kind: "PROTECTED_EFFECT",
                task_id: t.task_id,
                reference: agent_id,
                reason: format!("child {key} reached a protected effect: {what}"),
                action: "do it from this task after review, or agent.cancel the child".to_owned(),
                since_offset: offset,
            });
        }
    }
    out
}
