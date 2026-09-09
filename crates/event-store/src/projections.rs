//! Projections of the core aggregates (docs/31 `sessions`, `tasks`, `runs`,
//! `turns`, `run_steps`). Rows are derived from events through the domain
//! reducers, updated inside the same transaction as the append, and can be
//! rebuilt from the event log at any time (consumers are idempotent, docs/13).

use modbit_domain::approval::{Approval, ApprovalEvent};
use modbit_domain::event::{AggregateType, PayloadRef};
use modbit_domain::lease::{CapabilityLease, CapabilityLeaseEvent};
use modbit_domain::run::{Run, RunEvent};
use modbit_domain::session::{Session, SessionEvent};
use modbit_domain::step::{RunStep, StepEvent};
use modbit_domain::task::{Task, TaskEvent, TaskState};
use modbit_domain::toolcall::EffectReceipt;
use modbit_domain::toolcall::{ToolCall, ToolCallEvent};
use modbit_domain::turn::{Turn, TurnEvent};
use modbit_domain::{
    ApprovalId, CapabilityLeaseId, RunId, RunStepId, SessionId, TaskId, Timestamp, ToolCallId,
    TurnId,
};
use rusqlite::{OptionalExtension, Transaction, params};

use crate::store::StoredEvent;
use crate::{Error, Result};

/// Name of the projection cursor row.
pub const PROJECTION_NAME: &str = "core_aggregates";

fn invalid(e: modbit_domain::InvalidTransition, offset: u64) -> Error {
    Error::Projection {
        offset,
        detail: e.to_string(),
    }
}

fn payload_json(
    tx: &Transaction<'_>,
    ev: &StoredEvent,
    objects: &crate::ObjectStore,
) -> Result<serde_json::Value> {
    let _ = tx;
    match &ev.envelope.payload {
        PayloadRef::Inline { payload } => Ok(payload.clone()),
        PayloadRef::Object { object_hash, .. } => {
            Ok(serde_json::from_slice(&objects.get(object_hash)?)?)
        }
    }
}

/// Apply one stored event to the projection tables.
pub fn apply(tx: &Transaction<'_>, ev: &StoredEvent, objects: &crate::ObjectStore) -> Result<()> {
    let at = ev.envelope.occurred_at;
    let id = ev.envelope.aggregate_id;
    let payload = payload_json(tx, ev, objects)?;
    let offset = ev.offset;
    match ev.envelope.aggregate_type {
        AggregateType::Session => {
            let event: SessionEvent = serde_json::from_value(payload)?;
            let sid = SessionId::from_bytes(id);
            let mut s = match load_session(tx, &sid)? {
                Some(s) => s,
                None => Session::create(sid, &event, at).map_err(|e| invalid(e, offset))?,
            };
            if ev.envelope.sequence > 1 {
                s.apply(&event, at).map_err(|e| invalid(e, offset))?;
            }
            tx.execute(
                "INSERT OR REPLACE INTO sessions (session_id, tenant_id, user_id, space_id, state, generation, created_at, updated_at, current_task_id, last_event_sequence, lease_generation, lease_owner, emergency_stopped_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
                params![
                    s.session_id.as_bytes().as_slice(),
                    s.tenant_id.as_bytes().as_slice(),
                    s.user_id.as_bytes().as_slice(),
                    s.space_id.as_bytes().as_slice(),
                    serde_json::to_string(&s.state)?.trim_matches('"'),
                    s.generation as i64,
                    s.created_at.millis(),
                    s.updated_at.millis(),
                    s.current_task_id.map(|t| t.as_bytes().to_vec()),
                    ev.envelope.sequence as i64,
                    s.lease_generation as i64,
                    &s.lease_owner,
                    s.emergency_stopped_at.map(Timestamp::millis),
                ],
            )?;
        }
        AggregateType::Task => {
            let event: TaskEvent = serde_json::from_value(payload)?;
            let tid = TaskId::from_bytes(id);
            let mut t = match load_task(tx, &tid)? {
                Some(t) => t,
                None => Task::create(tid, &event, at).map_err(|e| invalid(e, offset))?,
            };
            if ev.envelope.sequence > 1 {
                t.apply(&event, at).map_err(|e| invalid(e, offset))?;
            }
            let (state, wait_reason) = match t.state {
                TaskState::Waiting(r) => (
                    "WAITING".to_owned(),
                    Some(serde_json::to_string(&r)?.trim_matches('"').to_owned()),
                ),
                other => (
                    serde_json::to_string(&other)?.trim_matches('"').to_owned(),
                    None,
                ),
            };
            tx.execute(
                "INSERT OR REPLACE INTO tasks (task_id, session_id, goal_text, workspace_id, base_revision, execution_profile, policy_profile_id, origin, state, wait_reason, generation, created_at, started_at, completed_at, failure_code, workspace_root)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)",
                params![
                    t.task_id.as_bytes().as_slice(),
                    t.session_id.as_bytes().as_slice(),
                    &t.goal_text,
                    t.workspace_id.as_bytes().as_slice(),
                    &t.base_revision,
                    &t.execution_profile,
                    &t.policy_profile_id,
                    serde_json::to_string(&t.origin)?.trim_matches('"'),
                    state,
                    wait_reason,
                    t.generation as i64,
                    t.created_at.millis(),
                    t.started_at.map(Timestamp::millis),
                    t.completed_at.map(Timestamp::millis),
                    &t.failure_code,
                    &t.workspace_root,
                ],
            )?;
        }
        AggregateType::Run => {
            let event: RunEvent = serde_json::from_value(payload)?;
            let rid = RunId::from_bytes(id);
            let mut r = match load_run(tx, &rid)? {
                Some(r) => r,
                None => Run::create(rid, &event, at).map_err(|e| invalid(e, offset))?,
            };
            if ev.envelope.sequence > 1 {
                r.apply(&event, at).map_err(|e| invalid(e, offset))?;
            }
            tx.execute(
                "INSERT OR REPLACE INTO runs (run_id, task_id, attempt, owner_location, kernel_lease_generation, state, generation, started_at, ended_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![
                    r.run_id.as_bytes().as_slice(),
                    r.task_id.as_bytes().as_slice(),
                    r.attempt,
                    serde_json::to_string(&r.owner_location)?.trim_matches('"'),
                    r.kernel_lease_generation as i64,
                    serde_json::to_string(&r.state)?.trim_matches('"'),
                    r.generation as i64,
                    r.started_at.map(Timestamp::millis),
                    r.ended_at.map(Timestamp::millis),
                ],
            )?;
        }
        AggregateType::Turn => {
            let event: TurnEvent = serde_json::from_value(payload)?;
            let tid = TurnId::from_bytes(id);
            let mut t = match load_turn(tx, &tid)? {
                Some(t) => t,
                None => Turn::create(tid, &event, at).map_err(|e| invalid(e, offset))?,
            };
            if ev.envelope.sequence > 1 {
                t.apply(&event, at).map_err(|e| invalid(e, offset))?;
            }
            tx.execute(
                "INSERT OR REPLACE INTO turns (turn_id, run_id, ordinal, state, model_route_json, tool_projection_hash, context_pack_id, generation, started_at, ended_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                params![
                    t.turn_id.as_bytes().as_slice(),
                    t.run_id.as_bytes().as_slice(),
                    t.ordinal,
                    serde_json::to_string(&t.state)?.trim_matches('"'),
                    t.model_route.as_ref().map(ToString::to_string),
                    &t.tool_projection_hash,
                    &t.context_pack_id,
                    t.generation as i64,
                    t.started_at.millis(),
                    t.ended_at.map(Timestamp::millis),
                ],
            )?;
        }
        AggregateType::RunStep => {
            let event: StepEvent = serde_json::from_value(payload)?;
            let sid = RunStepId::from_bytes(id);
            let mut s = match load_step(tx, &sid)? {
                Some(s) => s,
                None => RunStep::create(sid, &event, at).map_err(|e| invalid(e, offset))?,
            };
            if ev.envelope.sequence > 1 {
                s.apply(&event, at).map_err(|e| invalid(e, offset))?;
            }
            tx.execute(
                "INSERT OR REPLACE INTO run_steps (step_id, turn_id, step_type_json, state, ordinal, generation, started_at, ended_at, input_ref, output_ref, failure_code)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
                params![
                    s.step_id.as_bytes().as_slice(),
                    s.turn_id.as_bytes().as_slice(),
                    serde_json::to_string(&s.step_type)?,
                    serde_json::to_string(&s.state)?.trim_matches('"'),
                    s.ordinal,
                    s.generation as i64,
                    s.started_at.map(Timestamp::millis),
                    s.ended_at.map(Timestamp::millis),
                    &s.input_ref,
                    &s.output_ref,
                    &s.failure_code,
                ],
            )?;
        }
        AggregateType::ToolCall => {
            let event: ToolCallEvent = serde_json::from_value(payload)?;
            let cid = ToolCallId::from_bytes(id);
            let mut c = match load_tool_call(tx, &cid)? {
                Some(c) => c,
                None => ToolCall::create(cid, &event, at).map_err(|e| invalid(e, offset))?,
            };
            if ev.envelope.sequence > 1 {
                c.apply(&event, at).map_err(|e| invalid(e, offset))?;
            }
            if let ToolCallEvent::EffectReceiptAppended { receipt } = &event {
                insert_receipt(tx, receipt)?;
            }
            tx.execute(
                "INSERT OR REPLACE INTO tool_calls (tool_call_id, task_id, step_id, tool_name, tool_version, effect_class, capability_lease_id, status, arguments_hash, generation, dispatched_at, completed_at, result_ref, unknown_outcome_reason, policy_decision, approval_id)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)",
                params![
                    c.tool_call_id.as_bytes().as_slice(),
                    c.task_id.as_bytes().as_slice(),
                    c.step_id.map(|s| s.as_bytes().to_vec()),
                    &c.tool_name,
                    &c.tool_version,
                    serde_json::to_string(&c.effect_class)?.trim_matches('"'),
                    c.capability_lease_id.map(|l| l.as_bytes().to_vec()),
                    serde_json::to_string(&c.state)?.trim_matches('"'),
                    &c.arguments_hash,
                    c.generation as i64,
                    c.dispatched_at.map(Timestamp::millis),
                    c.completed_at.map(Timestamp::millis),
                    &c.result_ref,
                    &c.unknown_outcome_reason,
                    &c.policy_decision,
                    c.approval_id.map(|a| a.as_bytes().to_vec()),
                ],
            )?;
        }
        AggregateType::Approval => {
            let event: ApprovalEvent = serde_json::from_value(payload)?;
            let aid = ApprovalId::from_bytes(id);
            let mut a = match load_approval(tx, &aid)? {
                Some(a) => a,
                None => Approval::create(aid, &event, at).map_err(|e| invalid(e, offset))?,
            };
            if ev.envelope.sequence > 1 {
                a.apply(&event, at).map_err(|e| invalid(e, offset))?;
            }
            tx.execute(
                "INSERT OR REPLACE INTO approvals (approval_id, task_id, tool_call_id, tool_name, effect_class, intent_hash, scope_json, status, generation, requested_at, resolved_at, resolver_user_id, expires_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
                params![
                    a.approval_id.as_bytes().as_slice(),
                    a.task_id.as_bytes().as_slice(),
                    a.tool_call_id.as_bytes().as_slice(),
                    &a.tool_name,
                    serde_json::to_string(&a.effect_class)?.trim_matches('"'),
                    &a.intent_hash,
                    &a.scope_json,
                    serde_json::to_string(&a.state)?.trim_matches('"'),
                    a.generation as i64,
                    a.requested_at.millis(),
                    a.resolved_at.map(Timestamp::millis),
                    &a.resolver,
                    a.expires_at.map(Timestamp::millis),
                ],
            )?;
        }
        AggregateType::CapabilityLease => {
            let event: CapabilityLeaseEvent = serde_json::from_value(payload)?;
            let lid = CapabilityLeaseId::from_bytes(id);
            let mut l = match load_lease(tx, &lid)? {
                Some(l) => l,
                None => CapabilityLease::create(lid, &event, at).map_err(|e| invalid(e, offset))?,
            };
            if ev.envelope.sequence > 1 {
                l.apply(&event, at).map_err(|e| invalid(e, offset))?;
            }
            tx.execute(
                "INSERT OR REPLACE INTO capability_leases (lease_id, tenant_id, task_id, agent_id, resource_json, operations_json, effect_ceiling, execution_profile, generation, status, expires_at, revoked_at, revoke_reason)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
                params![
                    l.lease_id.as_bytes().as_slice(),
                    l.tenant_id.as_bytes().as_slice(),
                    l.task_id.as_bytes().as_slice(),
                    &l.agent_id,
                    serde_json::to_string(&l.resources)?,
                    serde_json::to_string(&l.operations)?,
                    serde_json::to_string(&l.effect_ceiling)?.trim_matches('"'),
                    &l.execution_profile,
                    l.generation as i64,
                    serde_json::to_string(&l.state)?.trim_matches('"'),
                    l.expires_at.map(Timestamp::millis),
                    l.revoked_at.map(Timestamp::millis),
                    &l.revoke_reason,
                ],
            )?;
        }
        // Aggregates whose projections belong to later milestones (protocol state, checkpoints).
        _ => {}
    }
    tx.execute(
        "INSERT OR REPLACE INTO projection_state (name, last_offset) VALUES (?1, ?2)",
        params![PROJECTION_NAME, offset as i64],
    )?;
    Ok(())
}

fn q<T>(s: &str) -> Result<T>
where
    T: serde::de::DeserializeOwned,
{
    Ok(serde_json::from_value(serde_json::Value::String(
        s.to_owned(),
    ))?)
}

fn blob16(v: Vec<u8>) -> rusqlite::Result<[u8; 16]> {
    v.try_into().map_err(|_| rusqlite::Error::InvalidQuery)
}

/// Load a session projection row.
pub fn load_session(tx: &rusqlite::Connection, id: &SessionId) -> Result<Option<Session>> {
    let row = tx
        .query_row(
            "SELECT tenant_id, user_id, space_id, state, generation, created_at, updated_at, current_task_id, lease_generation, lease_owner, emergency_stopped_at FROM sessions WHERE session_id = ?1",
            params![id.as_bytes().as_slice()],
            |r| {
                Ok((
                    r.get::<_, Vec<u8>>(0)?,
                    r.get::<_, Vec<u8>>(1)?,
                    r.get::<_, Vec<u8>>(2)?,
                    r.get::<_, String>(3)?,
                    r.get::<_, i64>(4)?,
                    r.get::<_, i64>(5)?,
                    r.get::<_, i64>(6)?,
                    r.get::<_, Option<Vec<u8>>>(7)?,
                    r.get::<_, i64>(8)?,
                    r.get::<_, Option<String>>(9)?,
                    r.get::<_, Option<i64>>(10)?,
                ))
            },
        )
        .optional()?;
    let Some((
        tenant,
        user,
        space,
        state,
        generation,
        created,
        updated,
        current,
        lease_generation,
        lease_owner,
        stopped,
    )) = row
    else {
        return Ok(None);
    };
    Ok(Some(Session {
        session_id: *id,
        tenant_id: modbit_domain::TenantId::from_bytes(blob16(tenant)?),
        user_id: modbit_domain::UserId::from_bytes(blob16(user)?),
        space_id: modbit_domain::SpaceId::from_bytes(blob16(space)?),
        state: q(&state)?,
        generation: generation as u64,
        created_at: Timestamp(created),
        updated_at: Timestamp(updated),
        current_task_id: current.map(blob16).transpose()?.map(TaskId::from_bytes),
        lease_generation: lease_generation as u64,
        lease_owner,
        emergency_stopped_at: stopped.map(Timestamp),
    }))
}

/// Load a task projection row.
pub fn load_task(tx: &rusqlite::Connection, id: &TaskId) -> Result<Option<Task>> {
    let row = tx
        .query_row(
            "SELECT session_id, goal_text, workspace_id, base_revision, execution_profile, policy_profile_id, origin, state, wait_reason, generation, created_at, started_at, completed_at, failure_code, workspace_root FROM tasks WHERE task_id = ?1",
            params![id.as_bytes().as_slice()],
            |r| {
                Ok((
                    r.get::<_, Vec<u8>>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, Vec<u8>>(2)?,
                    r.get::<_, Option<String>>(3)?,
                    r.get::<_, String>(4)?,
                    r.get::<_, Option<String>>(5)?,
                    r.get::<_, String>(6)?,
                    r.get::<_, String>(7)?,
                    r.get::<_, Option<String>>(8)?,
                    r.get::<_, i64>(9)?,
                    r.get::<_, i64>(10)?,
                    r.get::<_, Option<i64>>(11)?,
                    r.get::<_, Option<i64>>(12)?,
                    r.get::<_, Option<String>>(13)?,
                    r.get::<_, Option<String>>(14)?,
                ))
            },
        )
        .optional()?;
    let Some((
        session,
        goal,
        ws,
        base,
        profile,
        policy,
        origin,
        state,
        wait,
        generation,
        created,
        started,
        completed,
        failure,
        workspace_root,
    )) = row
    else {
        return Ok(None);
    };
    let state = match (state.as_str(), wait) {
        ("WAITING", Some(reason)) => TaskState::Waiting(q(&reason)?),
        (s, _) => q(s)?,
    };
    Ok(Some(Task {
        task_id: *id,
        session_id: SessionId::from_bytes(blob16(session)?),
        goal_text: goal,
        workspace_id: modbit_domain::WorkspaceId::from_bytes(blob16(ws)?),
        workspace_root,
        base_revision: base,
        execution_profile: profile,
        policy_profile_id: policy,
        origin: q(&origin)?,
        state,
        generation: generation as u64,
        created_at: Timestamp(created),
        started_at: started.map(Timestamp),
        completed_at: completed.map(Timestamp),
        failure_code: failure,
    }))
}

/// Load a run projection row.
pub fn load_run(tx: &rusqlite::Connection, id: &RunId) -> Result<Option<Run>> {
    let row = tx
        .query_row(
            "SELECT task_id, attempt, owner_location, kernel_lease_generation, state, generation, started_at, ended_at FROM runs WHERE run_id = ?1",
            params![id.as_bytes().as_slice()],
            |r| {
                Ok((
                    r.get::<_, Vec<u8>>(0)?,
                    r.get::<_, u32>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, i64>(3)?,
                    r.get::<_, String>(4)?,
                    r.get::<_, i64>(5)?,
                    r.get::<_, Option<i64>>(6)?,
                    r.get::<_, Option<i64>>(7)?,
                ))
            },
        )
        .optional()?;
    let Some((task, attempt, loc, lease, state, generation, started, ended)) = row else {
        return Ok(None);
    };
    Ok(Some(Run {
        run_id: *id,
        task_id: TaskId::from_bytes(blob16(task)?),
        attempt,
        owner_location: q(&loc)?,
        kernel_lease_generation: lease as u64,
        state: q(&state)?,
        generation: generation as u64,
        started_at: started.map(Timestamp),
        ended_at: ended.map(Timestamp),
    }))
}

/// Load a turn projection row.
pub fn load_turn(tx: &rusqlite::Connection, id: &TurnId) -> Result<Option<Turn>> {
    let row = tx
        .query_row(
            "SELECT run_id, ordinal, state, model_route_json, tool_projection_hash, context_pack_id, generation, started_at, ended_at FROM turns WHERE turn_id = ?1",
            params![id.as_bytes().as_slice()],
            |r| {
                Ok((
                    r.get::<_, Vec<u8>>(0)?,
                    r.get::<_, u32>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, Option<String>>(3)?,
                    r.get::<_, Option<String>>(4)?,
                    r.get::<_, Option<String>>(5)?,
                    r.get::<_, i64>(6)?,
                    r.get::<_, i64>(7)?,
                    r.get::<_, Option<i64>>(8)?,
                ))
            },
        )
        .optional()?;
    let Some((run, ordinal, state, route, hash, pack, generation, started, ended)) = row else {
        return Ok(None);
    };
    Ok(Some(Turn {
        turn_id: *id,
        run_id: RunId::from_bytes(blob16(run)?),
        ordinal,
        state: q(&state)?,
        model_route: route.map(|s| serde_json::from_str(&s)).transpose()?,
        tool_projection_hash: hash,
        context_pack_id: pack,
        generation: generation as u64,
        started_at: Timestamp(started),
        ended_at: ended.map(Timestamp),
    }))
}

/// Load a run-step projection row.
pub fn load_step(tx: &rusqlite::Connection, id: &RunStepId) -> Result<Option<RunStep>> {
    let row = tx
        .query_row(
            "SELECT turn_id, step_type_json, state, ordinal, generation, started_at, ended_at, input_ref, output_ref, failure_code FROM run_steps WHERE step_id = ?1",
            params![id.as_bytes().as_slice()],
            |r| {
                Ok((
                    r.get::<_, Vec<u8>>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, u32>(3)?,
                    r.get::<_, i64>(4)?,
                    r.get::<_, Option<i64>>(5)?,
                    r.get::<_, Option<i64>>(6)?,
                    r.get::<_, Option<String>>(7)?,
                    r.get::<_, Option<String>>(8)?,
                    r.get::<_, Option<String>>(9)?,
                ))
            },
        )
        .optional()?;
    let Some((turn, kind, state, ordinal, generation, started, ended, input, output, failure)) =
        row
    else {
        return Ok(None);
    };
    Ok(Some(RunStep {
        step_id: *id,
        turn_id: TurnId::from_bytes(blob16(turn)?),
        step_type: serde_json::from_str(&kind)?,
        ordinal,
        state: q(&state)?,
        generation: generation as u64,
        input_ref: input,
        output_ref: output,
        failure_code: failure,
        started_at: started.map(Timestamp),
        ended_at: ended.map(Timestamp),
    }))
}

/// Load a tool-call projection row.
pub fn load_tool_call(tx: &rusqlite::Connection, id: &ToolCallId) -> Result<Option<ToolCall>> {
    let row = tx
        .query_row(
            "SELECT task_id, step_id, tool_name, tool_version, effect_class, capability_lease_id, status, arguments_hash, generation, dispatched_at, completed_at, result_ref, unknown_outcome_reason, policy_decision, approval_id FROM tool_calls WHERE tool_call_id = ?1",
            params![id.as_bytes().as_slice()],
            |r| {
                Ok((
                    r.get::<_, Vec<u8>>(0)?,
                    r.get::<_, Option<Vec<u8>>>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, String>(3)?,
                    r.get::<_, String>(4)?,
                    r.get::<_, Option<Vec<u8>>>(5)?,
                    r.get::<_, String>(6)?,
                    r.get::<_, String>(7)?,
                    r.get::<_, i64>(8)?,
                    r.get::<_, Option<i64>>(9)?,
                    r.get::<_, Option<i64>>(10)?,
                    r.get::<_, Option<String>>(11)?,
                    r.get::<_, Option<String>>(12)?,
                    r.get::<_, Option<String>>(13)?,
                    r.get::<_, Option<Vec<u8>>>(14)?,
                ))
            },
        )
        .optional()?;
    let Some((
        task,
        step,
        name,
        version,
        effect,
        lease,
        status,
        args,
        generation,
        dispatched,
        completed,
        result,
        unknown,
        policy,
        approval,
    )) = row
    else {
        return Ok(None);
    };
    Ok(Some(ToolCall {
        tool_call_id: *id,
        task_id: TaskId::from_bytes(blob16(task)?),
        step_id: step.map(blob16).transpose()?.map(RunStepId::from_bytes),
        tool_name: name,
        tool_version: version,
        effect_class: q(&effect)?,
        capability_lease_id: lease
            .map(blob16)
            .transpose()?
            .map(modbit_domain::CapabilityLeaseId::from_bytes),
        arguments_hash: args,
        state: q(&status)?,
        generation: generation as u64,
        dispatched_at: dispatched.map(Timestamp),
        completed_at: completed.map(Timestamp),
        result_ref: result,
        unknown_outcome_reason: unknown,
        policy_decision: policy,
        approval_id: approval
            .map(blob16)
            .transpose()?
            .map(ApprovalId::from_bytes),
    }))
}

fn approval_from_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<Approval> {
    let id: Vec<u8> = r.get(0)?;
    let task: Vec<u8> = r.get(1)?;
    let call: Vec<u8> = r.get(2)?;
    let effect: String = r.get(4)?;
    let status: String = r.get(7)?;
    Ok(Approval {
        approval_id: ApprovalId::from_bytes(blob16(id)?),
        task_id: TaskId::from_bytes(blob16(task)?),
        tool_call_id: ToolCallId::from_bytes(blob16(call)?),
        tool_name: r.get(3)?,
        effect_class: q(&effect).map_err(|_| rusqlite::Error::InvalidQuery)?,
        intent_hash: r.get(5)?,
        scope_json: r.get(6)?,
        state: q(&status).map_err(|_| rusqlite::Error::InvalidQuery)?,
        generation: r.get::<_, i64>(8)? as u64,
        requested_at: Timestamp(r.get(9)?),
        resolved_at: r.get::<_, Option<i64>>(10)?.map(Timestamp),
        resolver: r.get(11)?,
        expires_at: r.get::<_, Option<i64>>(12)?.map(Timestamp),
    })
}

const APPROVAL_COLS: &str = "approval_id, task_id, tool_call_id, tool_name, effect_class, intent_hash, scope_json, status, generation, requested_at, resolved_at, resolver_user_id, expires_at";

/// Load an approval.
pub fn load_approval(tx: &rusqlite::Connection, id: &ApprovalId) -> Result<Option<Approval>> {
    Ok(tx
        .query_row(
            &format!("SELECT {APPROVAL_COLS} FROM approvals WHERE approval_id = ?1"),
            params![id.as_bytes().as_slice()],
            approval_from_row,
        )
        .optional()?)
}

/// The latest approval bound to a tool call, if any.
pub fn load_approval_for_call(
    tx: &rusqlite::Connection,
    id: &ToolCallId,
) -> Result<Option<Approval>> {
    Ok(tx
        .query_row(
            &format!(
                "SELECT {APPROVAL_COLS} FROM approvals WHERE tool_call_id = ?1 ORDER BY requested_at DESC, approval_id DESC LIMIT 1"
            ),
            params![id.as_bytes().as_slice()],
            approval_from_row,
        )
        .optional()?)
}

/// Approvals of every task in a session, pending first, oldest first.
pub fn load_approvals_for_session(
    tx: &rusqlite::Connection,
    session: &SessionId,
) -> Result<Vec<Approval>> {
    let mut stmt = tx.prepare(&format!(
        "SELECT {} FROM approvals a JOIN tasks t ON t.task_id = a.task_id WHERE t.session_id = ?1 ORDER BY (a.status <> 'REQUESTED'), a.requested_at, a.approval_id",
        APPROVAL_COLS
            .split(", ")
            .map(|c| format!("a.{c}"))
            .collect::<Vec<_>>()
            .join(", ")
    ))?;
    let rows = stmt.query_map(params![session.as_bytes().as_slice()], approval_from_row)?;
    Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
}

fn lease_from_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<CapabilityLease> {
    let id: Vec<u8> = r.get(0)?;
    let tenant: Vec<u8> = r.get(1)?;
    let task: Vec<u8> = r.get(2)?;
    let resources: String = r.get(4)?;
    let operations: String = r.get(5)?;
    let ceiling: String = r.get(6)?;
    let status: String = r.get(9)?;
    Ok(CapabilityLease {
        lease_id: CapabilityLeaseId::from_bytes(blob16(id)?),
        tenant_id: modbit_domain::TenantId::from_bytes(blob16(tenant)?),
        task_id: TaskId::from_bytes(blob16(task)?),
        agent_id: r.get(3)?,
        resources: serde_json::from_str(&resources).map_err(|_| rusqlite::Error::InvalidQuery)?,
        operations: serde_json::from_str(&operations).map_err(|_| rusqlite::Error::InvalidQuery)?,
        effect_ceiling: q(&ceiling).map_err(|_| rusqlite::Error::InvalidQuery)?,
        execution_profile: r.get(7)?,
        generation: r.get::<_, i64>(8)? as u64,
        state: q(&status).map_err(|_| rusqlite::Error::InvalidQuery)?,
        expires_at: r.get::<_, Option<i64>>(10)?.map(Timestamp),
        revoked_at: r.get::<_, Option<i64>>(11)?.map(Timestamp),
        revoke_reason: r.get(12)?,
    })
}

const LEASE_COLS: &str = "lease_id, tenant_id, task_id, agent_id, resource_json, operations_json, effect_ceiling, execution_profile, generation, status, expires_at, revoked_at, revoke_reason";

/// Load a lease.
pub fn load_lease(
    tx: &rusqlite::Connection,
    id: &CapabilityLeaseId,
) -> Result<Option<CapabilityLease>> {
    Ok(tx
        .query_row(
            &format!("SELECT {LEASE_COLS} FROM capability_leases WHERE lease_id = ?1"),
            params![id.as_bytes().as_slice()],
            lease_from_row,
        )
        .optional()?)
}

/// Every lease of a task, newest generation first.
pub fn load_leases_for_task(
    tx: &rusqlite::Connection,
    task: &TaskId,
) -> Result<Vec<CapabilityLease>> {
    let mut stmt = tx.prepare(&format!(
        "SELECT {LEASE_COLS} FROM capability_leases WHERE task_id = ?1 ORDER BY generation DESC, lease_id DESC"
    ))?;
    let rows = stmt.query_map(params![task.as_bytes().as_slice()], lease_from_row)?;
    Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
}

/// Every active lease of every task in a session.
pub fn load_active_leases_for_session(
    tx: &rusqlite::Connection,
    session: &SessionId,
) -> Result<Vec<CapabilityLease>> {
    let mut stmt = tx.prepare(&format!(
        "SELECT {} FROM capability_leases l JOIN tasks t ON t.task_id = l.task_id WHERE t.session_id = ?1 AND l.status = 'ACTIVE' ORDER BY l.lease_id",
        LEASE_COLS
            .split(", ")
            .map(|c| format!("l.{c}"))
            .collect::<Vec<_>>()
            .join(", ")
    ))?;
    let rows = stmt.query_map(params![session.as_bytes().as_slice()], lease_from_row)?;
    Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
}

fn insert_receipt(tx: &rusqlite::Connection, r: &EffectReceipt) -> Result<()> {
    let seq: i64 = tx.query_row(
        "SELECT COALESCE(MAX(seq), 0) + 1 FROM effect_receipts",
        [],
        |row| row.get(0),
    )?;
    tx.execute(
        "INSERT OR IGNORE INTO effect_receipts (effect_id, seq, previous_receipt_hash, task_id, turn_id, step_id, tool_call_id, capability_lease_id, intent_hash, policy_decision, approval_id, execution_target, evidence_ref, status, occurred_at, receipt_hash)
         VALUES (?1, ?2, ?3, ?4, NULL, NULL, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
        params![
            r.effect_id.as_bytes().as_slice(),
            seq,
            &r.previous_receipt_hash,
            r.task_id.as_bytes().as_slice(),
            r.tool_call_id.as_bytes().as_slice(),
            r.capability_lease_id.map(|l| l.as_bytes().to_vec()),
            &r.intent_hash,
            &r.policy_decision,
            r.approval_id.map(|a| a.as_bytes().to_vec()),
            &r.execution_target,
            &r.evidence_ref,
            &r.status,
            r.occurred_at.millis(),
            &r.receipt_hash,
        ],
    )?;
    Ok(())
}

fn receipt_from_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<EffectReceipt> {
    let id: Vec<u8> = r.get(0)?;
    let task: Vec<u8> = r.get(2)?;
    let call: Vec<u8> = r.get(3)?;
    let lease: Option<Vec<u8>> = r.get(4)?;
    let approval: Option<Vec<u8>> = r.get(7)?;
    Ok(EffectReceipt {
        effect_id: modbit_domain::EffectId::from_bytes(blob16(id)?),
        previous_receipt_hash: r.get(1)?,
        task_id: TaskId::from_bytes(blob16(task)?),
        tool_call_id: ToolCallId::from_bytes(blob16(call)?),
        capability_lease_id: lease
            .map(blob16)
            .transpose()?
            .map(CapabilityLeaseId::from_bytes),
        intent_hash: r.get(5)?,
        policy_decision: r.get(6)?,
        approval_id: approval
            .map(blob16)
            .transpose()?
            .map(ApprovalId::from_bytes),
        execution_target: r.get(8)?,
        evidence_ref: r.get(9)?,
        status: r.get(10)?,
        occurred_at: Timestamp(r.get(11)?),
        receipt_hash: r.get(12)?,
    })
}

const RECEIPT_COLS: &str = "effect_id, previous_receipt_hash, task_id, tool_call_id, capability_lease_id, intent_hash, policy_decision, approval_id, execution_target, evidence_ref, status, occurred_at, receipt_hash";

/// The hash of the newest receipt in the chain, if any.
pub fn last_receipt_hash(tx: &rusqlite::Connection) -> Result<Option<String>> {
    Ok(tx
        .query_row(
            "SELECT receipt_hash FROM effect_receipts ORDER BY seq DESC LIMIT 1",
            [],
            |r| r.get(0),
        )
        .optional()?)
}

/// The whole receipt chain in order (optionally one task's receipts).
pub fn load_receipts(
    tx: &rusqlite::Connection,
    task: Option<&TaskId>,
) -> Result<Vec<EffectReceipt>> {
    let mut out = Vec::new();
    match task {
        Some(t) => {
            let mut stmt = tx.prepare(&format!(
                "SELECT {RECEIPT_COLS} FROM effect_receipts WHERE task_id = ?1 ORDER BY seq"
            ))?;
            let rows = stmt.query_map(params![t.as_bytes().as_slice()], receipt_from_row)?;
            for r in rows {
                out.push(r?);
            }
        }
        None => {
            let mut stmt = tx.prepare(&format!(
                "SELECT {RECEIPT_COLS} FROM effect_receipts ORDER BY seq"
            ))?;
            let rows = stmt.query_map([], receipt_from_row)?;
            for r in rows {
                out.push(r?);
            }
        }
    }
    Ok(out)
}

/// Truncate the projection tables and replay every event from offset 0.
pub fn rebuild(tx: &Transaction<'_>, objects: &crate::ObjectStore) -> Result<u64> {
    for t in [
        "effect_receipts",
        "approvals",
        "capability_leases",
        "tool_calls",
        "run_steps",
        "turns",
        "runs",
        "tasks",
        "sessions",
    ] {
        tx.execute(&format!("DELETE FROM {t}"), [])?;
    }
    tx.execute(
        "INSERT OR REPLACE INTO projection_state (name, last_offset) VALUES (?1, 0)",
        params![PROJECTION_NAME],
    )?;
    let events = crate::store::read_all_from(tx, 0)?;
    let n = events.len() as u64;
    for ev in &events {
        apply(tx, ev, objects)?;
    }
    Ok(n)
}
