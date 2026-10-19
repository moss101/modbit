//! `SetTaskBudgets` (REQ-PX-116, docs/14 contract 5): the cost, wall-clock
//! and delegation limits of a task, recorded on its log. They are read back
//! by every run of the task (`runtime::rebuild`), so they survive a restart
//! and a resume after `BUDGET_EXHAUSTED` can raise them. Turns and tool
//! calls stay on `StartTask`.
//!
//! A subagent's budget is the slice its parent reserved for it at admission
//! (`spawn.rs`) and nothing a client can change: a limit set on a child
//! would be a way around its parent's cap.
//!
//! A *raise* is a decision of the budget policy (REQ-PX-060, QUAL-PX-060;
//! `modbit_policy::budget`): above the organization's ceiling it is refused
//! with a typed code; within it, it is applied for the person holding the
//! session and waits for an exact-intent approval when a run asks (or when
//! the organization asks for every raise). The approval is an ordinary
//! `Approval` aggregate of the task, listed and decided like any protected
//! effect (`ListApprovals`, `ResolveApproval`); it binds the exact caps in
//! force and asked for, and is spent by the log moving on. Every refusal and
//! every request is an event on the task's log, and the policy (read when the Core starts)
//! fails closed. The same ceiling clamps what the runtime enforces, so a
//! task that never set a cap still runs under it.
//!
//! These decisions are not `EffectReceipt`s: a receipt is bound to a tool
//! call and its kernel decision epoch (docs/23), and a budget is not a tool
//! call (the worktree and checkpoint collectors make the same choice).

use std::sync::OnceLock;

use modbit_core_runtime::harness::DEFAULT_MAX_CHILDREN;
use modbit_domain::approval::{ApprovalEvent, ApprovalState};
use modbit_domain::event::{Actor, AggregateType};
use modbit_domain::state::StateMachine;
use modbit_domain::task::{TaskEvent, TaskOrigin};
use modbit_domain::toolcall::EffectClass;
use modbit_domain::{ApprovalId, TaskId, Timestamp, ToolCallId};
use modbit_event_store::{AppendRequest, CommandRecord};
use modbit_policy::budget::{self, BudgetDecision, BudgetPolicy, Caps, Requester};
use modbit_protocol::v1 as wire;
use prost::Message;

use crate::runtime::typed;
use crate::server::{Core, accept, error_code, id16, reject, require_lease, split};

static POLICY: OnceLock<BudgetPolicy> = OnceLock::new();

/// The organization's budget policy (`MODBIT_BUDGET_POLICY`), read once.
pub(crate) fn policy() -> &'static BudgetPolicy {
    POLICY.get_or_init(BudgetPolicy::from_env)
}

/// Hold a run's caps under the organization's ceilings. A task that never set
/// a cap still runs under them; a subagent's slice is already inside its
/// parent's, which was clamped.
pub(crate) fn clamp_runtime(b: &mut modbit_core_runtime::Budgets) {
    let c = policy().clamp(Caps {
        cost_minor: b.max_cost_minor.unwrap_or(0),
        wall_ms: b.max_wall_ms.unwrap_or(0),
        children: b.max_children,
    });
    b.max_cost_minor = (c.cost_minor > 0).then_some(c.cost_minor);
    b.max_wall_ms = (c.wall_ms > 0).then_some(c.wall_ms);
    b.max_children = c.children;
}

/// The caps in force on a task (its last `TaskBudgetsSet`, held under the
/// ceilings) and how many cap decisions its log holds.
fn caps_in_force(
    store: &modbit_event_store::EventStore,
    task: &TaskId,
) -> Result<(Caps, u64), String> {
    let events = store
        .read_aggregate_of_types(task.as_bytes(), &["TaskBudgetsSet"], 0)
        .map_err(|e| e.to_string())?;
    let mut caps = Caps::unset(DEFAULT_MAX_CHILDREN);
    for e in &events {
        let p = store.payload(&e.envelope).map_err(|e| e.to_string())?;
        caps = Caps::from_set(
            p["max_cost_minor"].as_u64().unwrap_or(0),
            p["max_wall_ms"].as_u64().unwrap_or(0),
            p["max_children"]
                .as_u64()
                .and_then(|c| u32::try_from(c).ok())
                .unwrap_or(0),
            p["forbid_spawn"].as_bool() == Some(true),
            DEFAULT_MAX_CHILDREN,
        );
    }
    Ok((policy().clamp(caps), events.len() as u64))
}

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
    let requested = Caps::from_set(
        p.max_cost_minor,
        p.max_wall_ms,
        p.max_children,
        p.forbid_spawn,
        DEFAULT_MAX_CHILDREN,
    );
    let who = if matches!(actor, Actor::User(_)) {
        Requester::Person
    } else {
        Requester::Autonomous
    };
    // One lock from the read of the caps in force to the append: two raises
    // cannot both be judged against the same log.
    let mut store = core.store.lock().await;
    let (current, decisions) = match caps_in_force(&store, &task_id) {
        Ok(v) => v,
        // Fail closed: caps that cannot be read are not caps that can be raised.
        Err(e) => return reject(cid, "BUDGET_STATE_UNREADABLE", e),
    };
    let decided = |event: TaskEvent, approval: Option<AppendRequest>| {
        let mut reqs = vec![AppendRequest {
            tenant_id: core.tenant_id,
            session_id: task.session_id,
            task_id: Some(task_id),
            run_id: None,
            turn_id: None,
            step_id: None,
            aggregate_type: AggregateType::Task,
            aggregate_id: *task_id.as_bytes(),
            expected_sequence: None,
            events: vec![typed("TaskBudgetRaiseDecided", &event, actor.clone())],
        }];
        reqs.extend(approval);
        reqs
    };
    let refused_event =
        |code: &str, reason: &str, decision: &str, approval: Option<(String, String)>| {
            TaskEvent::TaskBudgetRaiseDecided {
                decision: decision.into(),
                code: code.into(),
                reason: reason.into(),
                approval_id: approval.as_ref().map(|a| a.0.clone()),
                intent_hash: approval.map(|a| a.1),
                requested_cost_minor: requested.cost_minor,
                requested_wall_ms: requested.wall_ms,
                requested_children: requested.children,
            }
        };
    let rule: String;
    let mut approval_used: Option<ApprovalId> = None;
    match policy().decide(&current, &requested, who) {
        BudgetDecision::Allow { rule: r } => rule = r,
        BudgetDecision::Deny { code, reason } => {
            let reqs = decided(refused_event(code, &reason, "REFUSED", None), None);
            if let Err(e) = store.append_all(reqs, None) {
                return reject(cid, error_code(&e), e.to_string());
            }
            return reject(cid, code, reason);
        }
        BudgetDecision::ApprovalRequired { reason } => {
            let hash = budget::intent_hash(&task_id.to_string(), &current, &requested, decisions);
            let now = Timestamp::now();
            let mut mine: Vec<_> = match store.approvals_for_task(&task_id) {
                Ok(a) => a
                    .into_iter()
                    .filter(|a| a.tool_name == budget::RAISE_EFFECT && a.intent_hash == hash)
                    .collect(),
                Err(e) => return reject(cid, error_code(&e), e.to_string()),
            };
            mine.sort_by_key(|a| a.requested_at.0);
            if let Some(a) = mine.iter().rev().find(|a| a.authorizes(&hash, now)) {
                approval_used = Some(a.approval_id);
                rule = "approved".into();
            } else if let Some(a) = mine.iter().rev().find(|a| {
                a.state == ApprovalState::Requested && a.expires_at.is_none_or(|e| now.0 < e.0)
            }) {
                return reject(
                    cid,
                    budget::APPROVAL_PENDING,
                    format!(
                        "approval {} for this exact raise is waiting for a person; once it is approved, ask again",
                        a.approval_id
                    ),
                );
            } else if who == Requester::Autonomous
                && mine
                    .last()
                    .is_some_and(|a| a.state == ApprovalState::Denied)
            {
                return reject(
                    cid,
                    budget::DENIED,
                    "a person denied this exact raise; the budget stays as it is",
                );
            } else {
                let approval_id = ApprovalId::new();
                let scope = serde_json::json!({
                    "ask_reason": budget::ASK_REASON,
                    "why": reason,
                    "budget_raise": {
                        "from": { "cost_minor": current.cost_minor, "wall_ms": current.wall_ms, "children": current.children },
                        "to": { "cost_minor": requested.cost_minor, "wall_ms": requested.wall_ms, "children": requested.children },
                        "forbid_spawn": p.forbid_spawn,
                        "max_children": p.max_children,
                    },
                });
                let approval_req = AppendRequest {
                    tenant_id: core.tenant_id,
                    session_id: task.session_id,
                    task_id: Some(task_id),
                    run_id: None,
                    turn_id: None,
                    step_id: None,
                    aggregate_type: AggregateType::Approval,
                    aggregate_id: *approval_id.as_bytes(),
                    expected_sequence: None,
                    events: vec![typed(
                        "ApprovalRequested",
                        &ApprovalEvent::ApprovalRequested {
                            task_id,
                            // The effect is not a tool call; the id only keeps the row unique.
                            tool_call_id: ToolCallId::new(),
                            tool_name: budget::RAISE_EFFECT.into(),
                            effect_class: EffectClass::ProtectedWrite,
                            intent_hash: hash.clone(),
                            scope_json: scope.to_string(),
                            expires_at: Some(Timestamp(now.0 + crate::approval_expiry::ttl_ms())),
                        },
                        actor.clone(),
                    )],
                };
                let message = format!(
                    "{reason}; approval {approval_id} is waiting for a person, and once it is approved, ask again"
                );
                let reqs = decided(
                    refused_event(
                        budget::NEEDS_APPROVAL,
                        &message,
                        "APPROVAL_REQUESTED",
                        Some((approval_id.to_string(), hash)),
                    ),
                    Some(approval_req),
                );
                return match store.append_all(reqs, None) {
                    Ok(ev) => {
                        if let Some(last) = ev.last() {
                            core.last_offset.send_replace(last.offset);
                        }
                        reject(cid, budget::NEEDS_APPROVAL, message)
                    }
                    Err(e) => reject(cid, error_code(&e), e.to_string()),
                };
            }
        }
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
                rule,
                approval_id: approval_used.map(|a| a.to_string()),
            },
            actor,
        )],
    };
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
