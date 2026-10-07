//! `PauseTask` and `ResumeTask` (REQ-PX-101, docs/62, docs/30, docs/14): a
//! person parks a running task at its next turn boundary and later resumes
//! it, over the runtime's existing durable park (REQ-EV-0049, `Runtime::park`
//! — the same suspension a restart leaves, taken by choice).
//!
//! * A pause never abandons work: a tool call or a model stream in flight
//!   finishes the turn it belongs to, and the run stops before the next turn.
//!   The answer says so (`PAUSING`, `waits_for_boundary`, and what the pause
//!   waits for) and `wait_ms` lets a client wait for the boundary.
//! * The park is durable the moment it lands: the run is `Suspended`, the
//!   task `Waiting(Paused)` with a `TaskPaused` record (who, why, the
//!   checkpoint held), in one transaction. A Core that dies afterwards comes
//!   back with the same paused task — startup reconciliation touches only
//!   tasks that were running.
//! * A pending approval stays pending: the approval is its own record and is
//!   answered the same way; a run waiting at one parks once it is answered.
//! * A resume is the existing re-entry (`StartTask` on a waiting task) under
//!   the lease generation the command presents: the same run, the plan and
//!   route it already has, a fresh capacity ticket. A resume with a stale
//!   generation is refused and changes nothing.
//! * Both commands are idempotent per command id and fenced by the session
//!   lease.

use std::sync::Arc;

use modbit_domain::event::AggregateType;
use modbit_domain::state::StateMachine;
use modbit_domain::task::{TaskEvent, TaskState, WaitReason};
use modbit_event_store::{AppendRequest, CommandOutcome, CommandRecord};
use modbit_protocol::v1::{self as wire, CommandAck, CommandEnvelope};
use prost::Message;

use crate::runtime::{PauseRequest, typed};
use crate::server::{Core, accept, handle_command, id16, reject, require_lease, wire_id};

/// How long a `PauseTask` may ask the Core to wait for the boundary.
const MAX_WAIT_MS: u32 = 30_000;

fn error_code(e: &modbit_event_store::Error) -> &'static str {
    crate::server::error_code(e)
}

/// `PauseTask`.
pub(crate) async fn pause_task(
    core: &Arc<Core>,
    env: &CommandEnvelope,
    record: CommandRecord,
) -> CommandAck {
    let cid = env.command_id.clone();
    let Ok(p) = wire::PauseTask::decode(env.payload.as_slice()) else {
        return reject(cid, "BAD_PAYLOAD", "PauseTask");
    };
    let Some(task_id) = p
        .task_id
        .as_ref()
        .and_then(id16)
        .map(modbit_domain::TaskId::from_bytes)
    else {
        return reject(cid, "BAD_PAYLOAD", "task_id required");
    };
    let task = match core.store.lock().await.task(&task_id) {
        Ok(Some(t)) => t,
        Ok(None) => return reject(cid, "UNKNOWN_TASK", task_id.to_string()),
        Err(e) => return reject(cid, error_code(&e), e.to_string()),
    };
    if let Err(ack) = require_lease(core, &cid, env, &task.session_id).await {
        return ack;
    }
    // A retry of the same command acts once: it answers with where the pause
    // stands now.
    let replayed = match core.store.lock().await.prior_command(&record) {
        Ok(Some(_)) => true,
        Ok(None) => false,
        Err(e) => return reject(cid, error_code(&e), e.to_string()),
    };
    if replayed {
        return answer(core, &task, 0, true, false, cid).await;
    }
    let live = core.runtime.is_running(&task_id).await;
    match task.state {
        TaskState::Waiting(WaitReason::Paused) if !live => {
            return answer(core, &task, 0, false, true, cid).await;
        }
        s if s.is_terminal() => {
            return reject(
                cid,
                "TASK_TERMINAL",
                format!("task {task_id} is {s:?}; only a running task pauses"),
            );
        }
        _ if !live => {
            return reject(
                cid,
                "NOT_PAUSABLE",
                format!(
                    "task {task_id} is {:?} and has no run executing; only a running task pauses",
                    task.state
                ),
            );
        }
        _ => {}
    }
    // The request is on the log before the park is raised.
    let actor = modbit_domain::event::Actor::User(core.user_id);
    let req = AppendRequest {
        tenant_id: core.tenant_id,
        session_id: task.session_id,
        task_id: Some(task_id),
        run_id: None,
        turn_id: None,
        step_id: None,
        aggregate_type: AggregateType::Task,
        aggregate_id: *task_id.as_bytes(),
        expected_sequence: None,
        events: vec![typed(
            "TaskPauseRequested",
            &TaskEvent::TaskPauseRequested {
                requested_by: format!("{actor:?}"),
            },
            actor.clone(),
        )],
    };
    let offset = {
        let mut store = core.store.lock().await;
        match store.execute_command(record, req) {
            Ok(CommandOutcome::Applied(evs)) => {
                let o = evs.last().map_or(0, |e| e.offset);
                core.last_offset.send_replace(o);
                o
            }
            Ok(CommandOutcome::Replayed(_)) => {
                drop(store);
                return answer(core, &task, 0, true, false, cid).await;
            }
            Err(e) => return reject(cid, error_code(&e), e.to_string()),
        }
    };
    let asked = core
        .runtime
        .pause(
            &task_id,
            PauseRequest {
                reason: p.reason.clone(),
                actor,
                command_id: env
                    .command_id
                    .as_ref()
                    .and_then(id16)
                    .map(|b| modbit_domain::EventId::from_bytes(b).to_string())
                    .unwrap_or_default(),
            },
        )
        .await;
    if !asked {
        // The loop ended between the check and the request.
        let task = core
            .store
            .lock()
            .await
            .task(&task_id)
            .ok()
            .flatten()
            .unwrap_or(task);
        return answer(core, &task, offset, false, false, cid).await;
    }
    // Wait for the boundary when the client asked to.
    let deadline = std::time::Instant::now()
        + std::time::Duration::from_millis(u64::from(p.wait_ms.min(MAX_WAIT_MS)));
    while p.wait_ms > 0
        && core.runtime.is_running(&task_id).await
        && std::time::Instant::now() < deadline
    {
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }
    let task = core
        .store
        .lock()
        .await
        .task(&task_id)
        .ok()
        .flatten()
        .unwrap_or(task);
    answer(core, &task, offset, false, false, cid).await
}

/// Where the pause stands for `task`, as the answer to a pause command.
async fn answer(
    core: &Arc<Core>,
    task: &modbit_domain::task::Task,
    offset: u64,
    replayed: bool,
    already: bool,
    cid: Option<wire::Id>,
) -> CommandAck {
    let live = core.runtime.is_running(&task.task_id).await;
    let task = core
        .store
        .lock()
        .await
        .task(&task.task_id)
        .ok()
        .flatten()
        .unwrap_or_else(|| task.clone());
    let paused = matches!(task.state, TaskState::Waiting(WaitReason::Paused)) && !live;
    let (state, waits, note) = if paused {
        (
            if already { "ALREADY_PAUSED" } else { "PAUSED" },
            false,
            "the run is parked at a turn boundary with its worktree and checkpoint held; ResumeTask continues it".to_owned(),
        )
    } else if live {
        let why = match task.state {
            TaskState::Waiting(WaitReason::Approval) => {
                "the run is waiting at a pending approval; it stays pending and answerable, and the pause lands once it is answered".to_owned()
            }
            TaskState::Waiting(r) => format!(
                "the run is waiting ({r:?}); the pause lands at the next turn boundary"
            ),
            _ => "a turn is in flight (a model stream or a tool call); it finishes, and the run parks before the next turn".to_owned(),
        };
        ("PAUSING", true, why)
    } else {
        (
            "ENDED_BEFORE_PAUSE",
            false,
            format!(
                "the run ended before it reached a turn boundary the pause could land on; the task is {:?}",
                task.state
            ),
        )
    };
    let store = core.store.lock().await;
    let checkpoint_id = crate::checkpoint::current(&store, &task)
        .map(|c| c.checkpoint_id.to_string())
        .unwrap_or_default();
    drop(store);
    accept(
        cid,
        replayed,
        wire::TaskPauseResult {
            task_id: Some(wire_id(task.task_id.as_bytes())),
            state: state.into(),
            waits_for_boundary: waits,
            note,
            checkpoint_id,
            offset,
        }
        .encode_to_vec(),
    )
}

/// `ResumeTask`.
pub(crate) async fn resume_task(
    core: &Arc<Core>,
    env: &CommandEnvelope,
    record: CommandRecord,
) -> CommandAck {
    let cid = env.command_id.clone();
    let Ok(p) = wire::ResumeTask::decode(env.payload.as_slice()) else {
        return reject(cid, "BAD_PAYLOAD", "ResumeTask");
    };
    let Some(task_id) = p
        .task_id
        .as_ref()
        .and_then(id16)
        .map(modbit_domain::TaskId::from_bytes)
    else {
        return reject(cid, "BAD_PAYLOAD", "task_id required");
    };
    let task = match core.store.lock().await.task(&task_id) {
        Ok(Some(t)) => t,
        Ok(None) => return reject(cid, "UNKNOWN_TASK", task_id.to_string()),
        Err(e) => return reject(cid, error_code(&e), e.to_string()),
    };
    // Fenced first: a client holding a superseded lease changes nothing.
    if let Err(ack) = require_lease(core, &cid, env, &task.session_id).await {
        return ack;
    }
    // A retry of the same command acts once.
    let prior = core.store.lock().await.prior_command(&record);
    match prior {
        Ok(Some(_)) => {
            let run_id = core
                .store
                .lock()
                .await
                .runs_for_task(&task_id)
                .ok()
                .and_then(|r| r.into_iter().next())
                .map(|r| wire_id(r.run_id.as_bytes()));
            return accept(
                cid,
                true,
                wire::TaskResumeResult {
                    task_id: Some(wire_id(task_id.as_bytes())),
                    run_id,
                    resumed: true,
                    ..Default::default()
                }
                .encode_to_vec(),
            );
        }
        Ok(None) => {}
        Err(e) => return reject(cid, error_code(&e), e.to_string()),
    }
    if !matches!(task.state, TaskState::Waiting(WaitReason::Paused))
        || core.runtime.is_running(&task_id).await
    {
        return reject(
            cid,
            "NOT_PAUSED",
            format!(
                "task {task_id} is {:?}; only a paused task resumes with ResumeTask",
                task.state
            ),
        );
    }
    // The run's own budgets, as the pause recorded them, unless the resumer
    // names others.
    let held = {
        let store = core.store.lock().await;
        store
            .read_aggregate_of_types(task_id.as_bytes(), &["TaskPaused"], 0)
            .unwrap_or_default()
            .last()
            .and_then(|e| store.payload(&e.envelope).ok())
    };
    let held_budget = |k: &str| {
        held.as_ref()
            .and_then(|p| p[k].as_u64())
            .unwrap_or(0)
            .min(u64::from(u32::MAX)) as u32
    };
    let pick = |asked: u32, k: &str| if asked != 0 { asked } else { held_budget(k) };
    // The re-entry is the ordinary one: same checks (provider, policy,
    // lease, capacity), same recovery of the run's own route.
    let mut inner = env.clone();
    inner.command_type = "StartTask".into();
    inner.payload = wire::StartTask {
        task_id: p.task_id.clone(),
        endpoint: p.endpoint.clone(),
        model: p.model.clone(),
        max_turns: pick(p.max_turns, "max_turns"),
        max_tool_calls: pick(p.max_tool_calls, "max_tool_calls"),
        max_no_progress_turns: pick(p.max_no_progress_turns, "max_no_progress_turns"),
        skills: vec![],
    }
    .encode_to_vec();
    let ack = Box::pin(handle_command(core, inner)).await;
    if ack.status != wire::CommandStatus::Accepted as i32 {
        return ack;
    }
    let started = wire::TaskRunStarted::decode(ack.result.as_slice()).unwrap_or_default();
    // The resume is on the log once it has happened; a retry replays.
    let actor = modbit_domain::event::Actor::User(core.user_id);
    let req = AppendRequest {
        tenant_id: core.tenant_id,
        session_id: task.session_id,
        task_id: Some(task_id),
        run_id: None,
        turn_id: None,
        step_id: None,
        aggregate_type: AggregateType::Task,
        aggregate_id: *task_id.as_bytes(),
        expected_sequence: None,
        events: vec![typed(
            "TaskResumeRequested",
            &TaskEvent::TaskResumeRequested {
                requested_by: format!("{actor:?}"),
            },
            actor,
        )],
    };
    let offset = {
        let mut store = core.store.lock().await;
        match store.execute_command(record, req) {
            Ok(CommandOutcome::Applied(evs)) => {
                let o = evs.last().map_or(0, |e| e.offset);
                core.last_offset.send_replace(o);
                o
            }
            Ok(CommandOutcome::Replayed(evs)) => evs.last().map_or(0, |e| e.offset),
            Err(_) => 0,
        }
    };
    accept(
        cid,
        false,
        wire::TaskResumeResult {
            task_id: Some(wire_id(task_id.as_bytes())),
            run_id: started.run_id,
            resumed: started.resumed,
            endpoint: started.endpoint,
            model: started.model,
            offset,
        }
        .encode_to_vec(),
    )
}
