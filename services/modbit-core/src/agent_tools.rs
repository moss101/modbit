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
/// Move a child between the foreground of the parent's attention and the
/// background without touching its run (docs/17 `agent.attend`,
/// REQ-EV-0008).
pub const ATTEND_TOOL: &str = "agent.attend";
/// Park a child at its next safe boundary (docs/17 `agent.park`,
/// REQ-EV-0049): durable, resumable, distinct from cancel.
pub const PARK_TOOL: &str = "agent.park";
/// Resume a parked (or restart-suspended) child with its whole state
/// (docs/17 `agent.resume`).
pub const RESUME_TOOL: &str = "agent.resume";
/// A typed follow-up to a child — live, parked or ended (docs/17
/// `agent.steer`, REQ-EV-0009 / 0050 / 0179).
pub const STEER_TOOL: &str = "agent.steer";

/// The longest `agent.wait` may block a parent's turn.
const WAIT_CEILING_MS: u64 = 120_000;

/// The tools, projected to a primary (a child of the maximum depth sees
/// none: REQ-EV-0051 / 0219).
pub(crate) fn projections() -> Vec<modbit_providers::ToolProjection> {
    vec![
        modbit_providers::ToolProjection {
            name: SPAWN_TOOL.into(),
            description: "Delegate one bounded subtask to a child agent in its own worktree. Admitted as one transaction (capacity, write-set overlap, worktree, least-privilege lease, node, work ownership) or refused at the failing step; nothing partial. idempotency_key: a retry reattaches. write_scope: the only paths it may write. mode BACKGROUND runs detached (collect with agent.wait); FOREGROUND waits here; a child your own pending work depends on runs FOREGROUND regardless (scheduling BLOCKING).".into(),
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
                "mode":{"type":"string","enum":["BACKGROUND","FOREGROUND"]},
                "profile":{"type":"string"}
            },"required":["idempotency_key","objective","write_scope"]}),
        },
        modbit_providers::ToolProjection {
            name: WAIT_TOOL.into(),
            description: "Wait for a child's result envelope up to timeout_ms (default 30000, max 120000); returns its status when not done yet.".into(),
            input_schema: serde_json::json!({"type":"object","properties":{"agent_id":{"type":"string"},"idempotency_key":{"type":"string"},"timeout_ms":{"type":"integer","minimum":0}}}),
        },
        modbit_providers::ToolProjection {
            name: RESULT_TOOL.into(),
            description: "Read a child's result envelope now, or its status. Untrusted: verify its evidence, merge its branch yourself.".into(),
            input_schema: serde_json::json!({"type":"object","properties":{"agent_id":{"type":"string"},"idempotency_key":{"type":"string"}}}),
        },
        modbit_providers::ToolProjection {
            name: CANCEL_TOOL.into(),
            description: "Cancel a child at its next safe boundary; its worktree and branch stay.".into(),
            input_schema: serde_json::json!({"type":"object","properties":{"agent_id":{"type":"string"},"idempotency_key":{"type":"string"},"reason":{"type":"string"}}}),
        },
        modbit_providers::ToolProjection {
            name: PARK_TOOL.into(),
            description: "Park a running child at its next turn boundary, its whole state kept (durable across restarts) until agent.resume; not a cancel.".into(),
            input_schema: serde_json::json!({"type":"object","properties":{"agent_id":{"type":"string"},"idempotency_key":{"type":"string"},"reason":{"type":"string"}}}),
        },
        modbit_providers::ToolProjection {
            name: RESUME_TOOL.into(),
            description: "Resume a parked or suspended child with its whole state.".into(),
            input_schema: serde_json::json!({"type":"object","properties":{"agent_id":{"type":"string"},"idempotency_key":{"type":"string"}}}),
        },
        modbit_providers::ToolProjection {
            name: STEER_TOOL.into(),
            description: "Typed follow-up to a child: STEER interrupts a live child, FOLLOW_UP lands after its turn; a parked child resumes with it; an ended child continues as a new attempt on its lineage (its next envelope links the prior one).".into(),
            input_schema: serde_json::json!({"type":"object","properties":{"agent_id":{"type":"string"},"idempotency_key":{"type":"string"},"message":{"type":"string","minLength":1},"mode":{"type":"string","enum":["STEER","FOLLOW_UP"]}},"required":["message"]}),
        },
        modbit_providers::ToolProjection {
            name: ATTEND_TOOL.into(),
            description: "Move a running child to the FOREGROUND of your attention or back to the BACKGROUND without restarting it.".into(),
            input_schema: serde_json::json!({"type":"object","properties":{"agent_id":{"type":"string"},"idempotency_key":{"type":"string"},"mode":{"type":"string","enum":["FOREGROUND","BACKGROUND"]}},"required":["mode"]}),
        },
    ]
}

/// `agent.attend` (REQ-EV-0008): the child's node moves between
/// `BACKGROUND` and `RUNNING` on the parent's log — a scheduling/attention
/// change that preserves the child's identity and touches nothing of its
/// run.
pub(crate) async fn handle_attend(
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
            entry: entry(call_id, ATTEND_TOOL, "status: INVALID_ARGUMENTS\nerror: agent_id (or the idempotency_key of a spawned child) required".into(), false),
            failure_code: Some("INVALID_ARGUMENTS".into()),
        };
    };
    let to = match v["mode"].as_str() {
        Some("FOREGROUND") => AgentStatus::Running,
        Some("BACKGROUND") => AgentStatus::Background,
        _ => {
            return Handled {
                entry: entry(
                    call_id,
                    ATTEND_TOOL,
                    "status: INVALID_ARGUMENTS\nerror: mode must be FOREGROUND or BACKGROUND"
                        .into(),
                    false,
                ),
                failure_code: Some("INVALID_ARGUMENTS".into()),
            };
        }
    };
    let mut store = core.store.lock().await;
    let Some(node) = store
        .agent_nodes(&task.task_id)
        .unwrap_or_default()
        .into_iter()
        .find(|n| n.agent_id == agent_id)
    else {
        return Handled {
            entry: entry(
                call_id,
                ATTEND_TOOL,
                format!(
                    "status: REFUSED\nerror_code: UNKNOWN_AGENT\nerror: no child {agent_id} on this task"
                ),
                false,
            ),
            failure_code: Some("UNKNOWN_AGENT".into()),
        };
    };
    let from: AgentStatus = serde_json::from_value(serde_json::Value::String(node.status.clone()))
        .unwrap_or(AgentStatus::Running);
    let child_offset = node
        .child_task_id
        .and_then(|t| store.read_aggregate(t.as_bytes(), 0, usize::MAX).ok())
        .and_then(|evs| evs.last().map(|e| e.offset))
        .unwrap_or(0);
    if from == to {
        return Handled {
            entry: entry(
                call_id,
                ATTEND_TOOL,
                format!(
                    "status: SUCCESS\nagent_id: {agent_id}\nstatus: {}\nnote: already there; nothing changed\nchild_last_offset: {child_offset}",
                    node.status
                ),
                false,
            ),
            failure_code: None,
        };
    }
    if !from.can_transition(to) {
        return Handled {
            entry: entry(
                call_id,
                ATTEND_TOOL,
                format!(
                    "status: REFUSED\nerror_code: NOT_RUNNING\nerror: the child is {}; only a RUNNING or BACKGROUND child changes attention mode",
                    node.status
                ),
                false,
            ),
            failure_code: Some("NOT_RUNNING".into()),
        };
    }
    let mode = if to == AgentStatus::Running {
        "FOREGROUND"
    } else {
        "BACKGROUND"
    };
    match append(
        &mut store,
        core,
        lt,
        AggregateType::Task,
        *task.task_id.as_bytes(),
        vec![typed(
            "AgentNodeTransitioned",
            &TaskEvent::AgentNodeTransitioned {
                agent_id,
                from,
                to,
                run_id: node.run_id,
                reason: format!(
                    "attention mode {mode} by the parent (agent.attend); the run continues"
                ),
            },
            actor.clone(),
        )],
    ) {
        Ok(_) => Handled {
            entry: entry(
                call_id,
                ATTEND_TOOL,
                format!(
                    "status: SUCCESS\nagent_id: {agent_id}\nmode: {mode}\nfrom: {:?}\nto: {:?}\nchild_last_offset: {child_offset}\nnote: the child's run, task and offsets are unchanged",
                    from, to
                ),
                true,
            ),
            failure_code: None,
        },
        Err(e) => Handled {
            entry: entry(
                call_id,
                ATTEND_TOOL,
                format!("status: INFRA_FAILURE\nerror: {e}"),
                false,
            ),
            failure_code: Some("STORE".into()),
        },
    }
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
            // REQ-EV-0180: the mode in force is the scheduler's, not the
            // spawn's word — a child the parent's own pending work depends
            // on runs in the foreground.
            let mode = s.mode;
            let mut text = format!(
                "status: SUCCESS\nagent_id: {}\nchild_task_id: {}\nworktree: {}\nbranch: {}\nwork_node: {}\ncapsule_ref: {}\nticket_id: {}\nreattached: {}\nmode: {:?}\nscheduling: {}{}",
                s.agent_id,
                s.child_task_id,
                s.worktree,
                s.branch,
                s.work_node,
                s.capsule_ref,
                s.ticket_id,
                s.reattached,
                mode,
                s.scheduling,
                if s.profile.is_empty() {
                    String::new()
                } else {
                    format!(
                        "\nprofile: {}\nnarrowed_tools: {}",
                        s.profile,
                        if s.narrowed_tools.is_empty() {
                            "none".to_owned()
                        } else {
                            s.narrowed_tools.join("; ")
                        }
                    )
                }
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
    // The envelope that stands is the latest one — unless it says the child
    // stopped short (PARKED / WAITING) and the child has since been brought
    // back: then there is no result yet, only a status (REQ-EV-0049).
    let node_status = store
        .agent_nodes(&task.task_id)
        .unwrap_or_default()
        .into_iter()
        .find(|n| n.agent_id == agent_id)
        .map(|n| n.status);
    // An envelope recorded before the child's latest (re)admission belongs
    // to an earlier attempt (REQ-EV-0050): the current attempt has none
    // until it ends.
    let admitted_at = events
        .iter()
        .filter(|e| e.envelope.task_id == Some(task.task_id))
        .filter_map(|e| {
            let p = store.payload(&e.envelope).ok()?;
            let mine = match e.envelope.event_type.as_str() {
                "AgentNodeCreated" => p["node"]["agent_id"].as_str() == Some(&agent_id.to_string()),
                "AgentNodeTransitioned" => {
                    p["agent_id"].as_str() == Some(&agent_id.to_string())
                        && p["to"].as_str() == Some("ADMITTED")
                }
                _ => false,
            };
            mine.then_some(e.offset)
        })
        .max()
        .unwrap_or(0);
    let recorded = events
        .iter()
        .rev()
        .filter(|e| e.offset > admitted_at)
        .find_map(|e| {
            if e.envelope.task_id != Some(task.task_id)
                || e.envelope.event_type != "SubagentResultRecorded"
            {
                return None;
            }
            let p = store.payload(&e.envelope).ok()?;
            (p["agent_id"].as_str() == Some(&agent_id.to_string())).then_some(p)
        })
        .filter(|p| {
            let interim = matches!(p["status"].as_str(), Some("PARKED") | Some("WAITING"));
            !interim
                || matches!(
                    node_status.as_deref(),
                    Some("PARKED") | Some("WAITING") | None
                )
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
        None => match node_status {
            Some(status) => Err(status),
            None => Err("UNKNOWN_AGENT".into()),
        },
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
                // An interim envelope (the child stopped short) is not a
                // result: say what the child is, with what it left behind.
                let status = v["status"].as_str().unwrap_or_default().to_owned();
                if status == "PARKED" || status == "WAITING" {
                    return format!(
                        "status: {status}\nagent_status: {status}\nnote: {}\n{}",
                        if status == "PARKED" {
                            "the child is parked; agent.resume brings it back with its whole state"
                        } else {
                            "the child stopped short of a result; agent.resume continues it, agent.cancel ends it"
                        },
                        serde_json::to_string_pretty(&v).unwrap_or_default()
                    );
                }
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
                // A parked child is not going to finish on its own
                // (REQ-EV-0049): say so now instead of running the clock.
                if status == "PARKED" {
                    return "status: PARKED\nagent_status: PARKED\nnote: the child is parked; agent.resume brings it back with its whole state".to_owned();
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

/// REQ-EV-0046 (docs/14 "Agent-to-agent communication"): a child inside a
/// capsule asks for a tool whose effect class is above the capsule's
/// ceiling. The call is refused here — before the kernel, before any
/// effector, without an approval of its own: a detached agent consumes
/// the grants it was admitted with and never opens an interactive
/// privilege expansion — and the parent transitions to its attention state
/// (`SubagentProtectedEffect`, then `TaskNeedsAttention`) to decide: do the
/// effect itself after review, or cancel the child. Returns the refusal
/// entry for the child's transcript, or `None` when the call is within the
/// ceiling (or the task is not a child).
pub(crate) async fn protected_effect(
    core: &Arc<Core>,
    task: &Task,
    state: &HarnessState,
    call_id: &str,
    tool: &str,
    actor: &Actor,
) -> Option<TranscriptEntry> {
    use modbit_domain::toolcall::EffectClass;
    let capsule = state.capsule.as_ref()?;
    let effect = core
        .tools
        .runtime
        .registry()
        .get(tool)
        .map(|t| t.spec().effect_class)?;
    let ceiling: EffectClass = capsule["effect_ceiling"]
        .as_str()
        .and_then(|c| serde_json::from_value(serde_json::Value::String(c.to_owned())).ok())
        .unwrap_or(EffectClass::ReversibleWrite);
    if effect <= ceiling {
        return None;
    }
    let effect_label = serde_json::to_value(effect)
        .ok()
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_else(|| format!("{effect:?}"));
    let ceiling_label = serde_json::to_value(ceiling)
        .ok()
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_else(|| format!("{ceiling:?}"));
    let (agent_id, parent_task_id) = (
        capsule["agent_id"]
            .as_str()
            .and_then(|s| AgentId::parse(s).ok()),
        capsule["parent_task_id"]
            .as_str()
            .and_then(|s| modbit_domain::TaskId::parse(s).ok()),
    );
    if let (Some(agent_id), Some(parent_task_id)) = (agent_id, parent_task_id) {
        let mut store = core.store.lock().await;
        if let Ok(Some(parent)) = store.task(&parent_task_id) {
            let key = store
                .agent_nodes(&parent.task_id)
                .unwrap_or_default()
                .into_iter()
                .find(|n| n.agent_id == agent_id)
                .map(|n| n.idempotency_key)
                .unwrap_or_default();
            let reason = format!(
                "subagent {key} ({agent_id}) reached a protected effect: {tool} is {effect_label}, above its capsule ceiling {ceiling_label}; it was refused without an approval of its own. Decide: do it from this task after review, or cancel the child (agent.cancel)"
            );
            let diagnostic = Some(modbit_core_runtime::classify(
                &modbit_core_runtime::FailureSource::Tool {
                    tool,
                    status: "POLICY_DENIED",
                    code: Some("PROTECTED_EFFECT_REFUSED"),
                    message: Some(&reason),
                    result_ref: "",
                    timed_out: false,
                    cancelled: false,
                },
            ));
            let _ = append(
                &mut store,
                core,
                Lineage::task(core.tenant_id, parent.session_id, parent.task_id),
                AggregateType::Task,
                *parent.task_id.as_bytes(),
                vec![
                    typed(
                        "SubagentProtectedEffect",
                        &TaskEvent::SubagentProtectedEffect {
                            agent_id,
                            child_task_id: task.task_id,
                            idempotency_key: key,
                            tool: tool.to_owned(),
                            effect_class: effect_label.clone(),
                            ceiling: ceiling_label.clone(),
                            call_id: call_id.to_owned(),
                        },
                        actor.clone(),
                    ),
                    typed(
                        "TaskNeedsAttention",
                        &TaskEvent::TaskNeedsAttention { reason, diagnostic },
                        actor.clone(),
                    ),
                ],
            );
        }
    }
    Some(entry(
        call_id,
        tool,
        format!(
            "status: REFUSED\nerror_code: PROTECTED_EFFECT_REFUSED\nerror: {tool} is {effect_label}, above this capsule's ceiling {ceiling_label}; a background agent cannot expand its own privilege (REQ-EV-0046). Your parent has been told and decides; report what you needed in task.complete"
        ),
        false,
    ))
}

/// `agent.park` (REQ-EV-0049): the child's loop suspends at its next turn
/// boundary with its run intact; the node reads `PARKED` once it has (the
/// loop's end records it on the parent's log).
pub(crate) async fn handle_park(
    core: &Arc<Core>,
    task: &Task,
    call_id: &str,
    args: &str,
) -> Handled {
    let v: serde_json::Value = serde_json::from_str(args).unwrap_or_default();
    let Some(agent_id) = select_agent(core, task, &v).await else {
        return Handled {
            entry: entry(call_id, PARK_TOOL, "status: INVALID_ARGUMENTS\nerror: agent_id (or the idempotency_key of a spawned child) required".into(), false),
            failure_code: Some("INVALID_ARGUMENTS".into()),
        };
    };
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
                PARK_TOOL,
                format!(
                    "status: REFUSED\nerror_code: UNKNOWN_AGENT\nerror: no child {agent_id} on this task"
                ),
                false,
            ),
            failure_code: Some("UNKNOWN_AGENT".into()),
        };
    };
    let Some(child) = node.child_task_id else {
        return Handled {
            entry: entry(call_id, PARK_TOOL, "status: REFUSED\nerror_code: NOT_A_SUBAGENT\nerror: the node has no task of its own".into(), false),
            failure_code: Some("NOT_A_SUBAGENT".into()),
        };
    };
    if node.status == "PARKED" {
        return Handled {
            entry: entry(
                call_id,
                PARK_TOOL,
                format!(
                    "status: SUCCESS\nagent_id: {agent_id}\nstatus: PARKED\nnote: already parked"
                ),
                false,
            ),
            failure_code: None,
        };
    }
    if !core.runtime.park(&child).await {
        return Handled {
            entry: entry(
                call_id,
                PARK_TOOL,
                format!(
                    "status: REFUSED\nerror_code: NOT_RUNNING\nerror: the child is {} with no live loop; nothing to park",
                    node.status
                ),
                false,
            ),
            failure_code: Some("NOT_RUNNING".into()),
        };
    }
    Handled {
        entry: entry(
            call_id,
            PARK_TOOL,
            format!(
                "status: PARK_REQUESTED\nagent_id: {agent_id}\nchild_task_id: {child}\nnote: the child stops at its next turn boundary and its node reads PARKED; agent.wait returns PARKED then; agent.resume brings it back"
            ),
            true,
        ),
        failure_code: None,
    }
}

/// `agent.resume`: a parked or restart-suspended child continues on its own
/// log (M6.7 `spawn::resume_child`).
pub(crate) async fn handle_resume(
    core: &Arc<Core>,
    task: &Task,
    generation: u64,
    actor: &Actor,
    call_id: &str,
    args: &str,
) -> Handled {
    let v: serde_json::Value = serde_json::from_str(args).unwrap_or_default();
    let Some(agent_id) = select_agent(core, task, &v).await else {
        return Handled {
            entry: entry(call_id, RESUME_TOOL, "status: INVALID_ARGUMENTS\nerror: agent_id (or the idempotency_key of a spawned child) required".into(), false),
            failure_code: Some("INVALID_ARGUMENTS".into()),
        };
    };
    let (node, admission) = {
        let store = core.store.lock().await;
        let node = store
            .agent_nodes(&task.task_id)
            .unwrap_or_default()
            .into_iter()
            .find(|n| n.agent_id == agent_id);
        let admission = crate::spawn::latest_admission(&store, task, agent_id);
        (node, admission)
    };
    let (Some(node), Some(a)) = (node, admission) else {
        return Handled {
            entry: entry(
                call_id,
                RESUME_TOOL,
                format!(
                    "status: REFUSED\nerror_code: UNKNOWN_AGENT\nerror: no admitted child {agent_id} on this task"
                ),
                false,
            ),
            failure_code: Some("UNKNOWN_AGENT".into()),
        };
    };
    match crate::spawn::resume_child(core, task, &node, &a.0, &a.1, generation, actor).await {
        Ok(Some(run_id)) => Handled {
            entry: entry(
                call_id,
                RESUME_TOOL,
                format!(
                    "status: SUCCESS\nagent_id: {agent_id}\nchild_task_id: {}\nrun_id: {run_id}\nnote: resumed with its whole state; collect with agent.wait",
                    a.0
                ),
                true,
            ),
            failure_code: None,
        },
        Ok(None) => Handled {
            entry: entry(
                call_id,
                RESUME_TOOL,
                format!(
                    "status: REFUSED\nerror_code: NOT_SUSPENDED\nerror: the child is {}; only a parked or suspended child resumes",
                    node.status
                ),
                false,
            ),
            failure_code: Some("NOT_SUSPENDED".into()),
        },
        Err((code, detail)) => Handled {
            entry: entry(
                call_id,
                RESUME_TOOL,
                format!("status: REFUSED\nerror_code: {code}\nerror: {detail}"),
                false,
            ),
            failure_code: Some(code),
        },
    }
}

/// `agent.steer` (REQ-EV-0009 / 0050 / 0179): see `spawn::follow_up_child`.
pub(crate) async fn handle_steer(
    core: &Arc<Core>,
    task: &Task,
    generation: u64,
    actor: &Actor,
    call_id: &str,
    args: &str,
) -> Handled {
    let v: serde_json::Value = serde_json::from_str(args).unwrap_or_default();
    let message = v["message"].as_str().unwrap_or_default().trim().to_owned();
    let Some(agent_id) = select_agent(core, task, &v).await else {
        return Handled {
            entry: entry(call_id, STEER_TOOL, "status: INVALID_ARGUMENTS\nerror: agent_id (or the idempotency_key of a spawned child) required".into(), false),
            failure_code: Some("INVALID_ARGUMENTS".into()),
        };
    };
    if message.is_empty() {
        return Handled {
            entry: entry(
                call_id,
                STEER_TOOL,
                "status: INVALID_ARGUMENTS\nerror: message required".into(),
                false,
            ),
            failure_code: Some("INVALID_ARGUMENTS".into()),
        };
    }
    let live_mode = match v["mode"].as_str() {
        Some("FOLLOW_UP") => modbit_domain::task::InputMode::FollowUp,
        _ => modbit_domain::task::InputMode::Steer,
    };
    let (node, admission) = {
        let store = core.store.lock().await;
        let node = store
            .agent_nodes(&task.task_id)
            .unwrap_or_default()
            .into_iter()
            .find(|n| n.agent_id == agent_id);
        let admission = crate::spawn::latest_admission(&store, task, agent_id);
        (node, admission)
    };
    let (Some(node), Some(a)) = (node, admission) else {
        return Handled {
            entry: entry(
                call_id,
                STEER_TOOL,
                format!(
                    "status: REFUSED\nerror_code: UNKNOWN_AGENT\nerror: no admitted child {agent_id} on this task"
                ),
                false,
            ),
            failure_code: Some("UNKNOWN_AGENT".into()),
        };
    };
    match crate::spawn::follow_up_child(
        core, task, &node, &a.0, &a.1, generation, &message, live_mode, actor,
    )
    .await
    {
        Ok(text) => Handled {
            entry: entry(
                call_id,
                STEER_TOOL,
                format!("{text}\nagent_id: {agent_id}\nchild_task_id: {}", a.0),
                true,
            ),
            failure_code: None,
        },
        Err((code, detail)) => Handled {
            entry: entry(
                call_id,
                STEER_TOOL,
                format!("status: REFUSED\nerror_code: {code}\nerror: {detail}"),
                false,
            ),
            failure_code: Some(code),
        },
    }
}
