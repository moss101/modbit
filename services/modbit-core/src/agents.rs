//! The AgentGraph in the Core (M6.1; docs/13 "Agent node", docs/14 "One
//! runtime, three explicit graphs"): every task has one primary agent that
//! owns its outcome (REQ-EV-0255), created at its first run and moved
//! through the documented statuses as its runs start and end; the Fast
//! Context specialist and, from M6.3, admitted subagents are nodes under
//! it. Identity outlives the model binding (REQ-EV-0256). Nothing here
//! decides what may run: it records who is running what.

use modbit_domain::agent::{AgentBinding, AgentKind, AgentNode, AgentStatus};
use modbit_domain::event::{Actor, AggregateType};
use modbit_domain::task::{Task, TaskEvent};
use modbit_domain::{AgentId, RunId};
use modbit_event_store::EventStore;

use crate::runtime::{Lineage, append, typed};
use crate::server::Core;

/// The task's primary agent, when its first run has created it.
pub(crate) fn primary(
    store: &EventStore,
    task: &Task,
) -> Option<modbit_event_store::projections::AgentNodeRow> {
    store
        .agent_nodes(&task.task_id)
        .unwrap_or_default()
        .into_iter()
        .find(|n| n.kind == "PRIMARY")
}

fn status_of(label: &str) -> AgentStatus {
    serde_json::from_value(serde_json::Value::String(label.to_owned()))
        .unwrap_or(AgentStatus::Proposed)
}

/// The primary agent is running on `run_id` with `binding`: created now
/// when the task has none (its idempotency key is the task id, so a second
/// first run finds it rather than making another, REQ-EV-0007), moved to
/// `Running` otherwise through the legal transitions (a finished primary
/// resumed for another run passes through `Admitted`, REQ-EV-0050).
pub(crate) fn primary_running(
    store: &mut EventStore,
    core: &Core,
    task: &Task,
    lt: Lineage,
    actor: &Actor,
    run_id: RunId,
    binding: AgentBinding,
) -> Option<AgentId> {
    match primary(store, task) {
        None => {
            let agent_id = AgentId::new();
            let node = AgentNode {
                agent_id,
                task_id: task.task_id,
                parent_agent_id: None,
                root_agent_id: agent_id,
                depth: 0,
                kind: AgentKind::Primary,
                status: AgentStatus::Running,
                run_id: Some(run_id),
                capsule_ref: None,
                binding,
                idempotency_key: task.task_id.to_string(),
                owns: vec![],
            };
            append(
                store,
                core,
                lt,
                AggregateType::Task,
                *task.task_id.as_bytes(),
                vec![typed(
                    "AgentNodeCreated",
                    &TaskEvent::AgentNodeCreated { node },
                    actor.clone(),
                )],
            )
            .ok()
            .map(|_| agent_id)
        }
        Some(row) => {
            let mut events = Vec::new();
            let mut from = status_of(&row.status);
            let current = AgentBinding {
                endpoint: row.endpoint.clone(),
                model: row.model.clone(),
            };
            if from.is_terminal() {
                events.push(typed(
                    "AgentNodeTransitioned",
                    &TaskEvent::AgentNodeTransitioned {
                        agent_id: row.agent_id,
                        from,
                        to: AgentStatus::Admitted,
                        run_id: Some(run_id),
                        reason: "resumed for another run (REQ-EV-0050)".into(),
                    },
                    actor.clone(),
                ));
                from = AgentStatus::Admitted;
            }
            if from != AgentStatus::Running && from.can_transition(AgentStatus::Running) {
                events.push(typed(
                    "AgentNodeTransitioned",
                    &TaskEvent::AgentNodeTransitioned {
                        agent_id: row.agent_id,
                        from,
                        to: AgentStatus::Running,
                        run_id: Some(run_id),
                        reason: "run started".into(),
                    },
                    actor.clone(),
                ));
            }
            if current != binding {
                events.push(typed(
                    "AgentBindingChanged",
                    &TaskEvent::AgentBindingChanged {
                        agent_id: row.agent_id,
                        from: current,
                        to: binding,
                        reason: "the run's route (REQ-EV-0256: the agent stays)".into(),
                    },
                    actor.clone(),
                ));
            }
            if !events.is_empty() {
                let _ = append(
                    store,
                    core,
                    lt,
                    AggregateType::Task,
                    *task.task_id.as_bytes(),
                    events,
                );
            }
            Some(row.agent_id)
        }
    }
}

/// Move the primary agent to `to` when the transition is legal; a run that
/// ends leaves the primary `Waiting`, `Completed` (its proposal is with the
/// user), `Failed` or `Cancelled`.
pub(crate) fn primary_transition(
    store: &mut EventStore,
    core: &Core,
    task: &Task,
    lt: Lineage,
    actor: &Actor,
    to: AgentStatus,
    reason: &str,
) {
    let Some(row) = primary(store, task) else {
        return;
    };
    let from = status_of(&row.status);
    if from == to || !from.can_transition(to) {
        return;
    }
    let _ = append(
        store,
        core,
        lt,
        AggregateType::Task,
        *task.task_id.as_bytes(),
        vec![typed(
            "AgentNodeTransitioned",
            &TaskEvent::AgentNodeTransitioned {
                agent_id: row.agent_id,
                from,
                to,
                run_id: row.run_id,
                reason: reason.to_owned(),
            },
            actor.clone(),
        )],
    );
}

/// The primary agent runs on another binding from here (REQ-EV-0256): a
/// continuation on a stronger solver, a provider fallback. Its identity,
/// lineage and run stay.
pub(crate) fn primary_rebound(
    store: &mut EventStore,
    core: &Core,
    task: &Task,
    lt: Lineage,
    actor: &Actor,
    to: AgentBinding,
    reason: &str,
) {
    let Some(row) = primary(store, task) else {
        return;
    };
    let from = AgentBinding {
        endpoint: row.endpoint.clone(),
        model: row.model.clone(),
    };
    if from == to {
        return;
    }
    let _ = append(
        store,
        core,
        lt,
        AggregateType::Task,
        *task.task_id.as_bytes(),
        vec![typed(
            "AgentBindingChanged",
            &TaskEvent::AgentBindingChanged {
                agent_id: row.agent_id,
                from,
                to,
                reason: reason.to_owned(),
            },
            actor.clone(),
        )],
    );
}

/// A child node under the primary (a specialist sub-run now; admitted
/// subagents from M6.3): created `Running` on the primary's run, with the
/// idempotency key the caller names. Returns the node's id, or the id of the
/// node that already carries that key (REQ-EV-0007).
#[allow(clippy::too_many_arguments)]
pub(crate) fn child_started(
    store: &mut EventStore,
    core: &Core,
    task: &Task,
    lt: Lineage,
    actor: &Actor,
    kind: AgentKind,
    run_id: Option<RunId>,
    binding: AgentBinding,
    idempotency_key: &str,
) -> Option<AgentId> {
    let nodes = store.agent_nodes(&task.task_id).unwrap_or_default();
    if let Some(existing) = nodes.iter().find(|n| n.idempotency_key == idempotency_key) {
        return Some(existing.agent_id);
    }
    let parent = nodes.iter().find(|n| n.kind == "PRIMARY")?;
    let agent_id = AgentId::new();
    let node = AgentNode {
        agent_id,
        task_id: task.task_id,
        parent_agent_id: Some(parent.agent_id),
        root_agent_id: parent.root_agent_id,
        depth: parent.depth + 1,
        kind,
        status: AgentStatus::Running,
        run_id,
        capsule_ref: None,
        binding,
        idempotency_key: idempotency_key.to_owned(),
        owns: vec![],
    };
    append(
        store,
        core,
        lt,
        AggregateType::Task,
        *task.task_id.as_bytes(),
        vec![typed(
            "AgentNodeCreated",
            &TaskEvent::AgentNodeCreated { node },
            actor.clone(),
        )],
    )
    .ok()
    .map(|_| agent_id)
}

/// A child node ended.
#[allow(clippy::too_many_arguments)]
pub(crate) fn child_ended(
    store: &mut EventStore,
    core: &Core,
    task: &Task,
    lt: Lineage,
    actor: &Actor,
    agent_id: AgentId,
    to: AgentStatus,
    reason: &str,
) {
    let Some(row) = store
        .agent_nodes(&task.task_id)
        .unwrap_or_default()
        .into_iter()
        .find(|n| n.agent_id == agent_id)
    else {
        return;
    };
    let from = status_of(&row.status);
    if !from.can_transition(to) {
        return;
    }
    let _ = append(
        store,
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
                run_id: row.run_id,
                reason: reason.to_owned(),
            },
            actor.clone(),
        )],
    );
}
