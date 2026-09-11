//! Protocol state in the Core (docs/19 layer 2, M4.1, REQ-EV-0055): the
//! reconstruction of a task's outstanding calls, approvals, question and
//! leases from the store; the restart reconciliation that turns calls a dead
//! Core left in flight into unknown outcomes; and the resume-time
//! reconciliation that hands the model what is known about such a call
//! instead of replaying it.

use modbit_domain::event::{Actor, AggregateType};
use modbit_domain::task::{Task, TaskEvent};
use modbit_domain::toolcall::{EffectClass, ToolCallEvent, ToolCallState};
use modbit_domain::{RunId, TaskId, Timestamp, ToolCallId};
use modbit_event_store::{AppendRequest, EventStore};
use modbit_protocol_state::{
    CallPhase, Input, PendingCall, PendingQuestion, ProtocolState, Reconciliation, ResumeBoundary,
    restart_reason,
};

use crate::runtime::{Lineage, TranscriptEntry, append, typed};
use crate::server::Core;

/// The open typed question with the model call that asked it, if any.
pub(crate) fn pending_question(store: &EventStore, task: &TaskId) -> Option<PendingQuestion> {
    let events = store
        .read_aggregate(task.as_bytes(), 0, usize::MAX)
        .unwrap_or_default();
    let mut open: Option<PendingQuestion> = None;
    for e in &events {
        let payload = store.payload(&e.envelope).unwrap_or_default();
        match e.envelope.event_type.as_str() {
            "UserQuestionAsked" => {
                open = payload["question_id"].as_str().map(|q| PendingQuestion {
                    question_id: q.to_owned(),
                    call_id: payload["call_id"].as_str().unwrap_or_default().to_owned(),
                });
            }
            "UserQuestionAnswered"
                if open.as_ref().map(|q| q.question_id.as_str())
                    == payload["question_id"].as_str() =>
            {
                open = None;
            }
            _ => {}
        }
    }
    open
}

/// Calls whose unknown outcome the task's log already reconciled.
pub(crate) fn reconciled_calls(store: &EventStore, task: &TaskId) -> Vec<ToolCallId> {
    let events = store
        .read_aggregate(task.as_bytes(), 0, usize::MAX)
        .unwrap_or_default();
    events
        .iter()
        .filter(|e| e.envelope.event_type == "ToolCallReconciled")
        .filter_map(|e| {
            let payload = store.payload(&e.envelope).ok()?;
            ToolCallId::parse(payload["tool_call_id"].as_str()?).ok()
        })
        .collect()
}

/// The task's protocol state: the row the store materialized in the
/// transaction of the last event that touched it (docs/31 `protocol_state`),
/// or, for a task no such event has reached yet, the reconstruction from
/// the projections directly.
pub(crate) fn reconstruct(store: &EventStore, task: &TaskId) -> ProtocolState {
    if let Ok(Some(t)) = store.task(task)
        && let Ok(Some(state)) = store.protocol_state(&t.session_id, task)
    {
        return state;
    }
    reconstruct_from_rows(store, task)
}

/// Reconstruct the task's protocol state from the projections and the
/// task's log (the same rule the store materializes with).
pub(crate) fn reconstruct_from_rows(store: &EventStore, task: &TaskId) -> ProtocolState {
    let calls = store.open_tool_calls(task).unwrap_or_default();
    let approvals = store.approvals_for_task(task).unwrap_or_default();
    let leases = store.leases_for_task(task).unwrap_or_default();
    let reconciled = reconciled_calls(store, task);
    let terminals = terminal_cursors(store, task);
    ProtocolState::reconstruct(
        *task,
        Input {
            tool_calls: &calls,
            approvals: &approvals,
            leases: &leases,
            question: pending_question(store, task),
            reconciled: &reconciled,
            terminals: &terminals,
            now: Timestamp::now(),
        },
    )
}

/// The terminal cursors the task's log built, oldest event first.
fn terminal_cursors(
    store: &EventStore,
    task: &TaskId,
) -> Vec<modbit_protocol_state::TerminalCursor> {
    use modbit_protocol_state::{TerminalUpdate, apply_terminal};
    let events = store
        .read_aggregate(task.as_bytes(), 0, usize::MAX)
        .unwrap_or_default();
    let mut out = Vec::new();
    for e in &events {
        let Ok(p) = store.payload(&e.envelope) else {
            continue;
        };
        let strs = |v: &serde_json::Value| -> Vec<String> {
            v.as_array()
                .map(|a| {
                    a.iter()
                        .filter_map(|x| x.as_str().map(str::to_owned))
                        .collect()
                })
                .unwrap_or_default()
        };
        let update = match e.envelope.event_type.as_str() {
            "TerminalCreated" => TerminalUpdate::Created {
                handle_id: p["handle_id"].as_str().unwrap_or_default().to_owned(),
                request_id: p["request_id"].as_str().unwrap_or_default().to_owned(),
                argv: strs(&p["argv"]),
                replay_generation: p["replay_generation"].as_u64().unwrap_or(0),
                tool_call_id: p["tool_call_id"].as_str().unwrap_or_default().to_owned(),
            },
            "TerminalOutputAdvanced" => TerminalUpdate::Advanced {
                handle_id: p["handle_id"].as_str().unwrap_or_default().to_owned(),
                cursor: p["cursor"].as_u64().unwrap_or(0),
                running: p["running"].as_bool().unwrap_or(false),
            },
            "ProcessExited" => TerminalUpdate::Exited {
                handle_id: p["handle_id"].as_str().unwrap_or_default().to_owned(),
                output_ref: p["output_ref"].as_str().unwrap_or_default().to_owned(),
                exit_code: p["exit_code"].as_i64().and_then(|c| i32::try_from(c).ok()),
            },
            _ => continue,
        };
        apply_terminal(&mut out, update);
    }
    out
}

/// What a restart did to one task's in-flight calls.
#[derive(Clone, Debug, Default)]
pub(crate) struct Orphans {
    /// Effectful calls now of unknown outcome, as UUID text with their tool.
    pub unknown: Vec<(String, String)>,
    /// Read-only calls cancelled (safe to re-propose), as UUID text with their tool.
    pub cancelled: Vec<(String, String)>,
}

/// docs/19 resume step 6: a call the dead Core had dispatched or was
/// streaming is of unknown outcome now — unless it reads only, in which case
/// nothing happened that matters and it is cancelled so the resumed run
/// proposes it afresh. Nothing is retried here.
pub(crate) fn mark_orphaned_calls(
    store: &mut EventStore,
    tenant: modbit_domain::TenantId,
    task: &Task,
    boot_generation: u64,
    actor: &Actor,
) -> Orphans {
    let mut out = Orphans::default();
    let open = store.open_tool_calls(&task.task_id).unwrap_or_default();
    for c in open.into_iter().filter(|c| {
        matches!(
            c.state,
            ToolCallState::Dispatched | ToolCallState::Streaming
        )
    }) {
        let read_only = c.effect_class == EffectClass::ReadOnly;
        let event = if read_only {
            typed(
                "ToolCallCancelled",
                &ToolCallEvent::ToolCallCancelled,
                actor.clone(),
            )
        } else {
            typed(
                "ToolCallUnknownOutcome",
                &ToolCallEvent::ToolCallUnknownOutcome {
                    reason: restart_reason(boot_generation),
                },
                actor.clone(),
            )
        };
        let ok = store
            .append(AppendRequest {
                tenant_id: tenant,
                session_id: task.session_id,
                task_id: Some(task.task_id),
                run_id: c.run_id,
                turn_id: c.turn_id,
                step_id: None,
                aggregate_type: AggregateType::ToolCall,
                aggregate_id: *c.tool_call_id.as_bytes(),
                expected_sequence: Some(c.generation),
                events: vec![event],
            })
            .is_ok();
        if ok {
            let entry = (c.tool_call_id.to_string(), c.tool_name.clone());
            if read_only {
                out.cancelled.push(entry);
            } else {
                out.unknown.push(entry);
            }
        }
    }
    out
}

/// The attention line a restart leaves on a task, by the boundary it stands at.
pub(crate) fn attention_after_restart(boundary: &ResumeBoundary, orphans: &Orphans) -> String {
    match boundary {
        ResumeBoundary::AwaitingApproval {
            approval_id,
            tool_call_id,
            ..
        } => format!(
            "runtime restarted while the task awaited approval {approval_id} for tool call {tool_call_id}; the same approval is still open: resolve it and resume with StartTask"
        ),
        ResumeBoundary::Reconciling { tool_call_ids } => format!(
            "runtime restarted with {} tool call(s) in flight ({}); their outcome is unknown and nothing was replayed: resume with StartTask to reconcile them before anything else runs",
            tool_call_ids.len(),
            orphans
                .unknown
                .iter()
                .map(|(id, tool)| format!("{tool} {id}"))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        ResumeBoundary::AwaitingAnswer { question_id } => format!(
            "runtime restarted while the task awaited the answer to question {question_id}; answer it and resume with StartTask"
        ),
        ResumeBoundary::Executing { tool_call_ids } => format!(
            "runtime restarted with {} proposed tool call(s) not yet run; resume with StartTask to re-enter them by id",
            tool_call_ids.len()
        ),
        ResumeBoundary::TurnStart => {
            "runtime restarted while the task was running; resume with StartTask".to_owned()
        }
    }
}

/// The model's `call_id` in `run` names an outstanding call: the id to
/// re-enter, when its intent is the recorded one.
pub(crate) fn resumed_call(
    state: &Option<ProtocolState>,
    run: RunId,
    call_id: &str,
    tool_name: &str,
    arguments_json: &str,
) -> Option<ToolCallId> {
    let state = state.as_ref()?;
    let hash = modbit_tools::arguments_hash(arguments_json)?;
    state
        .find_call(run, call_id, tool_name, &hash)
        .filter(|c| {
            !matches!(
                c.phase,
                CallPhase::UnknownOutcome { .. } | CallPhase::InFlight
            )
        })
        .map(|c| c.tool_call_id)
}

/// The outstanding call of unknown outcome the model's `call_id` names, if any.
pub(crate) fn unknown_call(
    state: &Option<ProtocolState>,
    run: RunId,
    call_id: &str,
    tool_name: &str,
    arguments_json: &str,
) -> Option<PendingCall> {
    let state = state.as_ref()?;
    let hash = modbit_tools::arguments_hash(arguments_json)?;
    state
        .find_call(run, call_id, tool_name, &hash)
        .filter(|c| {
            matches!(
                c.phase,
                CallPhase::UnknownOutcome { .. } | CallPhase::InFlight
            )
        })
        .cloned()
}

/// docs/19 resume step 6, docs/13: reconcile a call of unknown outcome with
/// the effect ledger and the target, record what was found, and hand the
/// model the observation as the call's result. The Core never replays the
/// effect; a retry is the model's decision and a new call.
pub(crate) async fn reconcile_unknown(
    core: &Core,
    task: &Task,
    lt: Lineage,
    actor: &Actor,
    call_id: &str,
    pending: &PendingCall,
    arguments_json: &str,
) -> TranscriptEntry {
    let rule = Reconciliation::for_effect(pending.effect_class);
    let reason = match &pending.phase {
        CallPhase::UnknownOutcome { reason } => reason.clone(),
        _ => "the call was in flight when the Core stopped".to_owned(),
    };
    let (resolution, observed, advice) = match rule {
        Reconciliation::HoldForReceipt => {
            let receipt = core
                .store
                .lock()
                .await
                .receipts(Some(&task.task_id))
                .unwrap_or_default()
                .into_iter()
                .find(|r| r.tool_call_id == pending.tool_call_id);
            match receipt {
                Some(r) => (
                    "RECEIPT_FOUND",
                    format!("effect receipt {} status {}", r.receipt_hash, r.status),
                    format!(
                        "the effect ledger holds a receipt for this call with status {}: the effect happened; do not repeat it",
                        r.status
                    ),
                ),
                None => (
                    "HELD_FOR_RECEIPT",
                    "no effect receipt for this call".to_owned(),
                    "no receipt was recorded, so whether the effect happened is unknown; it is not retried automatically. Inspect the target before deciding; a retry is a new call and needs its own approval".to_owned(),
                ),
            }
        }
        Reconciliation::InspectTarget => {
            let targets = crate::tools::change_targets(&pending.tool_name, arguments_json);
            let mut seen = Vec::new();
            if !targets.is_empty()
                && let Some(root) = task.workspace_root.as_deref()
                && let Ok((ws, _)) = core.tools.workspace(root).await
            {
                let ws = ws.lock().await;
                for p in &targets {
                    match crate::tools::read_workspace_file(&ws, p) {
                        Some(bytes) => seen.push(format!(
                            "{p}: present, {} bytes, sha256 {}",
                            bytes.len(),
                            modbit_workspace::content_hash(&bytes)
                        )),
                        None => seen.push(format!("{p}: absent")),
                    }
                }
            }
            let observed = if seen.is_empty() {
                format!("no workspace target to inspect for {}", pending.tool_name)
            } else {
                seen.join("; ")
            };
            (
                "TARGET_INSPECTED",
                observed.clone(),
                format!(
                    "the workspace was inspected instead of replaying the call: {observed}. Read the target and decide; nothing was retried"
                ),
            )
        }
        Reconciliation::ReplaySafe => (
            "REPLAY_SAFE",
            "read-only call; no effect to reconcile".to_owned(),
            "the call reads only and can simply be issued again".to_owned(),
        ),
    };
    {
        let mut store = core.store.lock().await;
        let _ = append(
            &mut store,
            core,
            lt,
            AggregateType::Task,
            *task.task_id.as_bytes(),
            vec![typed(
                "ToolCallReconciled",
                &TaskEvent::ToolCallReconciled {
                    tool_call_id: pending.tool_call_id.to_string(),
                    tool_name: pending.tool_name.clone(),
                    effect_class: format!("{:?}", pending.effect_class),
                    resolution: resolution.to_owned(),
                    observed: observed.clone(),
                },
                actor.clone(),
            )],
        );
    }
    TranscriptEntry::ToolResult {
        call_id: call_id.to_owned(),
        name: pending.tool_name.clone(),
        text: format!(
            "status: UNKNOWN_OUTCOME\nerror_code: UNKNOWN_OUTCOME\nerror: {reason}\nreconciliation: {resolution}\nobserved: {observed}\nadvice: {advice}"
        ),
        failure_signature: None,
        clears: vec![],
        wrote: None,
        progress: false,
        media: vec![],
    }
}
