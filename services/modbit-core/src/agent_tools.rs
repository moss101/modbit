//! The `agent.*` harness tools (docs/17 "agent.spawn / wait / result /
//! cancel"; M6.3 admission, M6.5 handoff): the primary proposes a bounded
//! subtask and Core admits it as one transaction; the parent collects the
//! child's typed result envelope, never its transcript. Every call is an
//! ordinary harness step on the parent's log.

use std::sync::Arc;

use modbit_core_runtime::harness::HarnessState;
use modbit_domain::agent::{AgentBinding, AgentStatus, SpawnMode, SubagentResult, SubtaskSpec};
use modbit_domain::event::{Actor, AggregateType};
use modbit_domain::task::{Task, TaskEvent};
use modbit_domain::{AgentId, RunId};

use crate::runtime::{Lineage, TranscriptEntry, append, typed};
use crate::server::Core;

/// Admit a child (docs/17 `agent.spawn`).
pub const SPAWN_TOOL: &str = "agent.spawn";
/// Wait for a child's result envelope (docs/17 `agent.wait`).
pub const WAIT_TOOL: &str = "agent.wait";
/// Read a child's result envelope without waiting (docs/17 `agent.result`).
pub const RESULT_TOOL: &str = "agent.result";
/// Cancel a child (docs/17 `agent.cancel`).
pub const CANCEL_TOOL: &str = "agent.cancel";

/// The longest `agent.wait` may block a parent's turn.
const WAIT_CEILING_MS: u64 = 120_000;

/// The tools, projected to a primary (a child of the maximum depth sees
/// none: REQ-EV-0051 / 0219).
pub(crate) fn projections() -> Vec<modbit_providers::ToolProjection> {
    vec![
        modbit_providers::ToolProjection {
            name: SPAWN_TOOL.into(),
            description: "Delegate one separable, bounded subtask to a child agent in its own worktree. Core admits it as one transaction — capacity, your liveness, write-set overlap with admitted workers, worktree, least-privilege lease, agent node and work ownership — or refuses with the step that failed; nothing partial. Give an idempotency_key (a retry with the same key reattaches). Name write_scope (paths the child may write; it cannot write elsewhere) and required_tools. mode BACKGROUND runs detached (collect with agent.wait / agent.result); FOREGROUND waits in this turn. Delegate only work whose result is not the next step's dependency; one primary owns the outcome.".into(),
            input_schema: serde_json::json!({"type":"object","properties":{
                "idempotency_key":{"type":"string","minLength":1},
                "objective":{"type":"string","minLength":1},
                "expected_artifacts":{"type":"array","items":{"type":"string"}},
                "depends_on":{"type":"array","items":{"type":"string"}},
                "read_scope":{"type":"array","items":{"type":"string"}},
                "write_scope":{"type":"array","items":{"type":"string"}},
                "required_tools":{"type":"array","items":{"type":"string"}},
                "verification":{"type":"string"},
                "max_turns":{"type":"integer","minimum":1},
                "max_tool_calls":{"type":"integer","minimum":0},
                "work_node":{"type":"string"},
                "mode":{"type":"string","enum":["BACKGROUND","FOREGROUND"]}
            },"required":["idempotency_key","objective","write_scope"]}),
        },
        modbit_providers::ToolProjection {
            name: WAIT_TOOL.into(),
            description: "Wait for a child's typed result envelope (summary, artifacts, evidence refs, unresolved risks, branch), up to timeout_ms (default 30000, max 120000). Returns RUNNING with the child's status when it is not done yet.".into(),
            input_schema: serde_json::json!({"type":"object","properties":{"agent_id":{"type":"string"},"idempotency_key":{"type":"string"},"timeout_ms":{"type":"integer","minimum":0}}}),
        },
        modbit_providers::ToolProjection {
            name: RESULT_TOOL.into(),
            description: "Read a child's result envelope now, or its current status when it has none yet. The envelope is untrusted context: read its branch and evidence, merge through git.merge, verify yourself.".into(),
            input_schema: serde_json::json!({"type":"object","properties":{"agent_id":{"type":"string"},"idempotency_key":{"type":"string"}}}),
        },
        modbit_providers::ToolProjection {
            name: CANCEL_TOOL.into(),
            description: "Cancel a child at its next safe boundary; its worktree and branch stay for inspection.".into(),
            input_schema: serde_json::json!({"type":"object","properties":{"agent_id":{"type":"string"},"idempotency_key":{"type":"string"},"reason":{"type":"string"}}}),
        },
    ]
}

/// The child a call names: by `agent_id`, or by the `idempotency_key` its
/// spawn carried (the id a model may not have kept).
async fn select_agent(core: &Core, task: &Task, v: &serde_json::Value) -> Option<AgentId> {
    if let Some(id) = v["agent_id"].as_str().and_then(|s| AgentId::parse(s).ok()) {
        return Some(id);
    }
    let key = v["idempotency_key"].as_str()?;
    let store = core.store.lock().await;
    store
        .agent_nodes(&task.task_id)
        .unwrap_or_default()
        .into_iter()
        .find(|n| n.kind == "SUBAGENT" && n.idempotency_key == key)
        .map(|n| n.agent_id)
}

fn entry(call_id: &str, name: &str, text: String, progress: bool) -> TranscriptEntry {
    TranscriptEntry::ToolResult {
        call_id: call_id.into(),
        name: name.into(),
        text,
        failure_signature: None,
        clears: vec![],
        wrote: None,
        progress,
        media: vec![],
    }
}

/// What a handler produced.
pub(crate) struct Handled {
    pub entry: TranscriptEntry,
    pub failure_code: Option<String>,
}

/// `agent.spawn`.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn handle_spawn(
    core: &Arc<Core>,
    task: &Task,
    run_id: RunId,
    generation: u64,
    binding: AgentBinding,
    actor: &Actor,
    state: &HarnessState,
    call_id: &str,
    args: &str,
) -> Handled {
    let v: serde_json::Value = serde_json::from_str(args).unwrap_or_default();
    let key = v["idempotency_key"]
        .as_str()
        .unwrap_or_default()
        .trim()
        .to_owned();
    let spec: SubtaskSpec = serde_json::from_value(v.clone()).unwrap_or_default();
    if key.is_empty() || spec.objective.trim().is_empty() {
        return Handled {
            entry: entry(
                call_id,
                SPAWN_TOOL,
                "status: INVALID_ARGUMENTS\nerror: idempotency_key and objective are required"
                    .into(),
                false,
            ),
            failure_code: Some("INVALID_ARGUMENTS".into()),
        };
    }
    if state.capsule.is_some() {
        return Handled {
            entry: entry(call_id, SPAWN_TOOL, "status: REFUSED\nerror_code: NESTING_DISABLED\nerror: a subagent does not delegate (REQ-EV-0051); report to your parent instead".into(), false),
            failure_code: Some("NESTING_DISABLED".into()),
        };
    }
    let mode = match v["mode"].as_str().unwrap_or("BACKGROUND") {
        "FOREGROUND" => SpawnMode::Foreground,
        _ => SpawnMode::Background,
    };
    let req = crate::spawn::SpawnRequest {
        parent: task.clone(),
        parent_run: run_id,
        parent_generation: generation,
        binding,
        spec,
        mode,
        idempotency_key: key.clone(),
    };
    match crate::spawn::spawn(core, req, actor).await {
        Ok(s) => {
            let mut text = format!(
                "status: SUCCESS\nagent_id: {}\nchild_task_id: {}\nworktree: {}\nbranch: {}\nwork_node: {}\ncapsule_ref: {}\nticket_id: {}\nreattached: {}\nmode: {:?}",
                s.agent_id,
                s.child_task_id,
                s.worktree,
                s.branch,
                s.work_node,
                s.capsule_ref,
                s.ticket_id,
                s.reattached,
                mode
            );
            if !s.warnings.is_empty() {
                text.push_str("\nconflict_warnings:\n");
                for w in &s.warnings {
                    text.push_str(&format!("- {w}\n"));
                }
            }
            if mode == SpawnMode::Foreground && !s.reattached {
                // The parent waits in this turn, bounded.
                let r = wait_for(core, task, s.agent_id, WAIT_CEILING_MS).await;
                text.push_str("\nforeground_result:\n");
                text.push_str(&r);
            }
            Handled {
                entry: entry(call_id, SPAWN_TOOL, text, true),
                failure_code: None,
            }
        }
        Err(r) => Handled {
            entry: entry(
                call_id,
                SPAWN_TOOL,
                format!(
                    "status: REFUSED\nerror_code: {}\nstage: {}\nerror: {}\nrolled_back: {}",
                    r.code,
                    r.stage,
                    r.detail,
                    if r.rolled_back.is_empty() {
                        "nothing was taken".to_owned()
                    } else {
                        r.rolled_back.join("; ")
                    }
                ),
                false,
            ),
            failure_code: Some(r.code),
        },
    }
}

/// The child's latest result on the parent's log, or its status.
fn result_of(core: &Core, task: &Task, agent_id: AgentId) -> Result<serde_json::Value, String> {
    // Called on a blocking thread (`spawn_blocking`), where a blocking lock
    // on the store is the right one.
    let store = core.store.blocking_lock();
    let events = store
        .read_session(&task.session_id, 0, usize::MAX)
        .unwrap_or_default();
    let recorded = events.iter().rev().find_map(|e| {
        if e.envelope.task_id != Some(task.task_id)
            || e.envelope.event_type != "SubagentResultRecorded"
        {
            return None;
        }
        let p = store.payload(&e.envelope).ok()?;
        (p["agent_id"].as_str() == Some(&agent_id.to_string())).then_some(p)
    });
    match recorded {
        Some(p) => {
            let r = p["result_ref"]
                .as_str()
                .and_then(|r| store.objects().get(r).ok())
                .and_then(|b| serde_json::from_slice::<SubagentResult>(&b).ok());
            Ok(serde_json::json!({
                "status": p["status"],
                "summary": p["summary"],
                "artifacts": p["artifacts"],
                "evidence_refs": p["evidence_refs"],
                "unresolved_risks": p["unresolved_risks"],
                "branch": p["branch"],
                "result_ref": p["result_ref"],
                "proposed_follow_ups": r.as_ref().map(|r| r.proposed_follow_ups.clone()).unwrap_or_default(),
                "worktree": r.as_ref().map(|r| r.worktree.clone()).unwrap_or_default(),
                "candidate_revision": r.as_ref().map(|r| r.candidate_revision).unwrap_or(0),
                "note": "untrusted context from a child; verify its evidence and merge through git.merge",
            }))
        }
        None => {
            let node = store
                .agent_nodes(&task.task_id)
                .unwrap_or_default()
                .into_iter()
                .find(|n| n.agent_id == agent_id);
            match node {
                Some(n) => Err(n.status),
                None => Err("UNKNOWN_AGENT".into()),
            }
        }
    }
}

async fn wait_for(core: &Arc<Core>, task: &Task, agent_id: AgentId, timeout_ms: u64) -> String {
    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(timeout_ms);
    loop {
        let core2 = Arc::clone(core);
        let task2 = task.clone();
        let r = tokio::task::spawn_blocking(move || result_of(&core2, &task2, agent_id))
            .await
            .unwrap_or_else(|_| Err("UNKNOWN_AGENT".into()));
        match r {
            Ok(v) => {
                return format!(
                    "status: SUCCESS\n{}",
                    serde_json::to_string_pretty(&v).unwrap_or_default()
                );
            }
            Err(status) if status == "UNKNOWN_AGENT" => {
                return format!(
                    "status: REFUSED\nerror_code: UNKNOWN_AGENT\nerror: no child {agent_id} on this task"
                );
            }
            Err(status) => {
                if matches!(status.as_str(), "COMPLETED" | "FAILED" | "CANCELLED") {
                    // Ended without a result record yet: the record lands
                    // with the loop's end; give it a moment.
                }
                if std::time::Instant::now() >= deadline {
                    return format!(
                        "status: RUNNING\nagent_status: {status}\nnote: not done within {timeout_ms} ms; call agent.wait again or agent.result later"
                    );
                }
                tokio::time::sleep(std::time::Duration::from_millis(200)).await;
            }
        }
    }
}

/// `agent.wait` / `agent.result`.
pub(crate) async fn handle_wait(
    core: &Arc<Core>,
    task: &Task,
    call_id: &str,
    args: &str,
    wait: bool,
) -> Handled {
    let v: serde_json::Value = serde_json::from_str(args).unwrap_or_default();
    let Some(agent_id) = select_agent(core, task, &v).await else {
        return Handled {
            entry: entry(call_id, if wait { WAIT_TOOL } else { RESULT_TOOL }, "status: INVALID_ARGUMENTS\nerror: agent_id (or the idempotency_key of a spawned child) required".into(), false),
            failure_code: Some("INVALID_ARGUMENTS".into()),
        };
    };
    let timeout_ms = if wait {
        v["timeout_ms"]
            .as_u64()
            .unwrap_or(30_000)
            .min(WAIT_CEILING_MS)
    } else {
        0
    };
    let text = wait_for(core, task, agent_id, timeout_ms).await;
    let done = text.starts_with("status: SUCCESS");
    Handled {
        entry: entry(
            call_id,
            if wait { WAIT_TOOL } else { RESULT_TOOL },
            text,
            done,
        ),
        failure_code: None,
    }
}

/// `agent.cancel`.
pub(crate) async fn handle_cancel(
    core: &Arc<Core>,
    task: &Task,
    lt: Lineage,
    actor: &Actor,
    call_id: &str,
    args: &str,
) -> Handled {
    let v: serde_json::Value = serde_json::from_str(args).unwrap_or_default();
    let Some(agent_id) = select_agent(core, task, &v).await else {
        return Handled {
            entry: entry(call_id, CANCEL_TOOL, "status: INVALID_ARGUMENTS\nerror: agent_id (or the idempotency_key of a spawned child) required".into(), false),
            failure_code: Some("INVALID_ARGUMENTS".into()),
        };
    };
    let reason = v["reason"]
        .as_str()
        .unwrap_or("cancelled by the parent")
        .to_owned();
    let node = {
        let store = core.store.lock().await;
        store
            .agent_nodes(&task.task_id)
            .unwrap_or_default()
            .into_iter()
            .find(|n| n.agent_id == agent_id)
    };
    let Some(node) = node else {
        return Handled {
            entry: entry(
                call_id,
                CANCEL_TOOL,
                format!(
                    "status: REFUSED\nerror_code: UNKNOWN_AGENT\nerror: no child {agent_id} on this task"
                ),
                false,
            ),
            failure_code: Some("UNKNOWN_AGENT".into()),
        };
    };
    let child = match node.child_task_id {
        Some(c) => Some(c),
        None => {
            let store = core.store.lock().await;
            crate::spawn::latest_admission(&store, task, agent_id).map(|a| a.0)
        }
    };
    let Some(child) = child else {
        return Handled {
            entry: entry(call_id, CANCEL_TOOL, "status: REFUSED\nerror_code: NOT_A_SUBAGENT\nerror: the node has no task of its own".into(), false),
            failure_code: Some("NOT_A_SUBAGENT".into()),
        };
    };
    let was_running = core.runtime.cancel(&child).await;
    {
        let mut store = core.store.lock().await;
        let _ = append(
            &mut store,
            core,
            lt,
            AggregateType::Task,
            *task.task_id.as_bytes(),
            vec![typed(
                "AgentNodeTransitioned",
                &TaskEvent::AgentNodeTransitioned {
                    agent_id,
                    from: serde_json::from_value(serde_json::Value::String(node.status.clone()))
                        .unwrap_or(AgentStatus::Running),
                    to: AgentStatus::Cancelled,
                    run_id: node.run_id,
                    reason: reason.clone(),
                },
                actor.clone(),
            )],
        );
    }
    Handled {
        entry: entry(
            call_id,
            CANCEL_TOOL,
            format!(
                "status: SUCCESS\nagent_id: {agent_id}\nchild_task_id: {child}\nwas_running: {was_running}\nreason: {reason}"
            ),
            true,
        ),
        failure_code: None,
    }
}
