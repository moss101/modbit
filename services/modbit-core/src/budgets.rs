//! `SetTaskBudgets` (REQ-PX-116, docs/14 contract 5): the cost, wall-clock
//! and delegation limits of a task, recorded on its log. They are read back
//! by every run of the task (`runtime::rebuild`), so they survive a restart
//! and a resume after `BUDGET_EXHAUSTED` can raise them. Turns and tool
//! calls stay on `StartTask`.
//!
//! A subagent's budget is the slice its parent reserved for it at admission
//! (`spawn.rs`) and nothing a client can change: a limit set on a child
//! would be a way around its parent's cap.

use modbit_domain::TaskId;
use modbit_domain::event::{Actor, AggregateType};
use modbit_domain::state::StateMachine;
use modbit_domain::task::{TaskEvent, TaskOrigin};
use modbit_event_store::{AppendRequest, CommandRecord};
use modbit_protocol::v1 as wire;
use prost::Message;

use crate::runtime::typed;
use crate::server::{Core, accept, error_code, id16, reject, require_lease, split};

/// Handle `SetTaskBudgets`.
pub(crate) async fn set(
    core: &Core,
    cid: Option<wire::Id>,
    env: &wire::CommandEnvelope,
    record: CommandRecord,
    actor: Actor,
) -> wire::CommandAck {
    let Ok(p) = wire::SetTaskBudgets::decode(env.payload.as_slice()) else {
        return reject(cid, "BAD_PAYLOAD", "SetTaskBudgets");
    };
    let Some(task_id) = p.task_id.as_ref().and_then(id16).map(TaskId::from_bytes) else {
        return reject(cid, "BAD_PAYLOAD", "task_id required");
    };
    let task = {
        let store = core.store.lock().await;
        match store.task(&task_id) {
            Ok(Some(t)) => t,
            Ok(None) => return reject(cid, "UNKNOWN_TASK", task_id.to_string()),
            Err(e) => return reject(cid, error_code(&e), e.to_string()),
        }
    };
    if task.origin == TaskOrigin::Subagent {
        return reject(
            cid,
            "CHILD_BUDGET_FIXED",
            "a subagent's budget is the slice its parent reserved at admission; set the parent's",
        );
    }
    if task.state.is_terminal() {
        return reject(
            cid,
            "TASK_ENDED",
            format!("task is {:?}; its budgets no longer apply", task.state),
        );
    }
    if let Err(ack) = require_lease(core, &cid, env, &task.session_id).await {
        return ack;
    }
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
            "TaskBudgetsSet",
            &TaskEvent::TaskBudgetsSet {
                max_cost_minor: p.max_cost_minor,
                max_wall_ms: p.max_wall_ms,
                max_children: p.max_children,
                forbid_spawn: p.forbid_spawn,
            },
            actor,
        )],
    };
    let mut store = core.store.lock().await;
    match store.execute_command(record, req) {
        Ok(outcome) => {
            let (events, replayed) = split(outcome);
            let offset = events.last().map_or(0, |e| e.offset);
            if !replayed {
                core.last_offset.send_replace(offset);
            }
            accept(
                cid,
                replayed,
                wire::TaskBudgetsView {
                    max_cost_minor: p.max_cost_minor,
                    max_wall_ms: p.max_wall_ms,
                    max_children: p.max_children,
                    forbid_spawn: p.forbid_spawn,
                    offset,
                }
                .encode_to_vec(),
            )
        }
        Err(e) => reject(cid, error_code(&e), e.to_string()),
    }
}
