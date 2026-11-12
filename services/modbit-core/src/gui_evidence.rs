//! GUI task completion evidence and the run's judge, on the Core's side
//! (REQ-PX-178, DR-PX-2026-10-26-018). The predicate types and the judge are
//! `modbit_verification::gui`; this module reads the evidence the way the
//! Core reads everything else (its own state read of the task's control
//! session, the workspace through the path policy, the task's own log),
//! gathers what the log shows happened to the run, and records both.
//!
//! Nothing here reads what the model said, and nothing here resolves an
//! approval or grants anything: the evaluation blocks a completion or does
//! not, and the verdict is a record.

use modbit_domain::event::{Actor, AggregateType};
use modbit_domain::task::{Task, TaskEvent};
use modbit_verification::gui::{
    self, CompletionEvidence, ENVIRONMENT_CLOSE_REASONS, ENVIRONMENT_CODES, ElementFact,
    POLICY_CODES, PredicateResult, PredicateStatus, RunFacts,
};
use serde_json::Value;

use crate::runtime::{Lineage, append, typed};
use crate::server::Core;

/// The completion evidence the task's setter declared, from the task's
/// capsule (`None` for a task nobody declared any for).
pub(crate) fn declared(capsule: Option<&Value>) -> Option<CompletionEvidence> {
    let v = capsule?.get("completion_evidence")?;
    if v.is_null() {
        return None;
    }
    serde_json::from_value(v.clone()).ok()
}

/// Read the evidence and evaluate the predicate.
pub(crate) async fn evaluate(
    core: &Core,
    task: &Task,
    run_id: &str,
    evidence: &CompletionEvidence,
) -> PredicateResult {
    match evidence {
        CompletionEvidence::Accessibility { .. } => {
            let stopped = core
                .store
                .lock()
                .await
                .session(&task.session_id)
                .ok()
                .flatten()
                .and_then(|s| s.emergency_stopped_at)
                .is_some();
            match core
                .tools
                .verification_state_read(
                    &core.store,
                    task.task_id,
                    task.session_id,
                    run_id,
                    task.workspace_root.as_deref(),
                    stopped,
                )
                .await
            {
                Ok(out) => {
                    let bundle = out["application"]["bundle_id"].as_str().unwrap_or_default();
                    let snapshot = out["snapshot"].as_str().unwrap_or_default();
                    let elements: Vec<ElementFact> =
                        modbit_computer::tree::parse(out["tree"].as_str().unwrap_or_default())
                            .into_iter()
                            .map(|l| ElementFact {
                                role: l.role,
                                name: l.name,
                                value: l.value,
                            })
                            .collect();
                    gui::eval_accessibility(evidence, bundle, snapshot, &elements)
                }
                Err(why) => PredicateResult::missing(
                    format!(
                        "the Core could not read the application ({why}); keep the control session open on it when completing"
                    ),
                    "nothing: the state read was refused",
                ),
            }
        }
        CompletionEvidence::File { path, .. } => {
            let text = match task.workspace_root.as_deref() {
                Some(root) => match core.tools.workspace(root).await {
                    Ok((ws, _)) => ws
                        .lock()
                        .await
                        .read(path)
                        .ok()
                        .and_then(|r| String::from_utf8(r.bytes).ok()),
                    Err(_) => None,
                },
                None => None,
            };
            gui::eval_file(evidence, text.as_deref())
        }
        CompletionEvidence::Recorded { .. } => {
            let events = {
                let store = core.store.lock().await;
                crate::browser::task_events(&store, task.task_id)
            };
            gui::eval_recorded(evidence, &events)
        }
    }
}

/// Record an evaluation on the task's log.
pub(crate) async fn record_evaluation(
    core: &Core,
    task: &Task,
    lt: Lineage,
    actor: &Actor,
    evidence: &CompletionEvidence,
    result: &PredicateResult,
    at: &str,
) {
    let mut store = core.store.lock().await;
    let _ = append(
        &mut store,
        core,
        lt,
        AggregateType::Task,
        *task.task_id.as_bytes(),
        vec![typed(
            "GuiEvidenceEvaluated",
            &TaskEvent::GuiEvidenceEvaluated {
                evidence: serde_json::to_value(evidence).unwrap_or_default(),
                status: result.status.word().into(),
                detail: result.detail.clone(),
                read: result.read.clone(),
                at: at.into(),
            },
            actor.clone(),
        )],
    );
}

/// The completion gate for a GUI task: `None` when the evidence holds,
/// otherwise the reason completion is blocked. The evaluation is recorded
/// either way.
pub(crate) async fn completion_blocker(
    core: &Core,
    task: &Task,
    lt: Lineage,
    actor: &Actor,
    run_id: &str,
    evidence: &CompletionEvidence,
) -> Option<String> {
    let result = evaluate(core, task, run_id, evidence).await;
    record_evaluation(core, task, lt, actor, evidence, &result, "COMPLETION").await;
    (result.status != PredicateStatus::Pass).then(|| {
        format!(
            "COMPLETION_EVIDENCE_NOT_MET: this task is done when {}; the Core read {} and found: {} ({}). Your summary is not evidence: make it true in the application, then complete",
            evidence.describe(),
            result.read,
            result.detail,
            result.status.word()
        )
    })
}

/// What the log shows happened to the task's run: environment failures and
/// refusals, each as a line naming the event.
pub(crate) async fn run_facts(core: &Core, task: &Task, run_end: &str) -> RunFacts {
    let store = core.store.lock().await;
    let mut facts = RunFacts {
        run_end: run_end.to_owned(),
        ..RunFacts::default()
    };
    for e in store
        .read_aggregate(task.task_id.as_bytes(), 0, usize::MAX)
        .unwrap_or_default()
    {
        let t = e.envelope.event_type.as_str();
        let Ok(p) = store.payload(&e.envelope) else {
            continue;
        };
        let at = e.offset;
        match t {
            "ComputerLatched" => facts.environment.push(format!(
                "ComputerLatched at offset {at}: {}",
                p["reason"].as_str().unwrap_or_default()
            )),
            "ComputerSessionClosed" => {
                let reason = p["reason"].as_str().unwrap_or_default();
                if ENVIRONMENT_CLOSE_REASONS.contains(&reason) {
                    facts.environment.push(format!(
                        "ComputerSessionClosed at offset {at}: {reason} ({})",
                        p["bundle_id"].as_str().unwrap_or_default()
                    ));
                } else if matches!(reason, "USER_ABORTED" | "EMERGENCY_STOP") {
                    facts
                        .policy
                        .push(format!("ComputerSessionClosed at offset {at}: {reason}"));
                }
            }
            "ComputerUserAborted" => facts.policy.push(format!(
                "ComputerUserAborted at offset {at}: {}",
                p["reason"].as_str().unwrap_or_default()
            )),
            _ => {}
        }
    }
    for a in store.approvals_for_task(&task.task_id).unwrap_or_default() {
        use modbit_domain::approval::ApprovalState;
        let who = a.resolver.clone().unwrap_or_default();
        match a.state {
            ApprovalState::Denied => facts.policy.push(format!(
                "approval {} of {} DENIED by {who}",
                a.approval_id, a.tool_name
            )),
            ApprovalState::Expired => facts.policy.push(format!(
                "approval {} of {} EXPIRED without a decision",
                a.approval_id, a.tool_name
            )),
            _ => {}
        }
    }
    for call in store.tool_calls_for_task(&task.task_id).unwrap_or_default() {
        for e in store
            .read_aggregate(call.tool_call_id.as_bytes(), 0, usize::MAX)
            .unwrap_or_default()
        {
            if e.envelope.event_type != "ToolCallFailed" {
                continue;
            }
            let Ok(p) = store.payload(&e.envelope) else {
                continue;
            };
            let code = p["failure_code"].as_str().unwrap_or_default();
            let line = format!(
                "ToolCallFailed at offset {}: {} {code}",
                e.offset, call.tool_name
            );
            if call.tool_name.starts_with("computer.") && ENVIRONMENT_CODES.contains(&code) {
                facts.environment.push(line);
            } else if POLICY_CODES.contains(&code) {
                // A denied approval is already a line of its own.
                if code != "APPROVAL_DENIED" && code != "APPROVAL_EXPIRED" {
                    facts.policy.push(line);
                }
            }
        }
    }
    facts
}

/// Judge a GUI task's run as it ends and record the verdict. The evidence
/// is read again here unless the run ends right after a completion that read
/// it (a run that ends any other way is judged on what holds now).
pub(crate) async fn judge_run_end(
    core: &Core,
    task: &Task,
    lt: Lineage,
    actor: &Actor,
    run_id: &str,
    evidence: &CompletionEvidence,
    run_end: &str,
) {
    let result = evaluate(core, task, run_id, evidence).await;
    record_evaluation(core, task, lt, actor, evidence, &result, "RUN_END").await;
    let facts = run_facts(core, task, run_end).await;
    let j = gui::judge(Some(&result), &facts);
    let mut store = core.store.lock().await;
    let _ = append(
        &mut store,
        core,
        lt,
        AggregateType::Task,
        *task.task_id.as_bytes(),
        vec![typed(
            "GuiOutcomeJudged",
            &TaskEvent::GuiOutcomeJudged {
                run_id: run_id.to_owned(),
                verdict: j.verdict.word().into(),
                evidence_status: result.status.word().into(),
                basis: j.basis,
                run_end: run_end.to_owned(),
            },
            actor.clone(),
        )],
    );
}
