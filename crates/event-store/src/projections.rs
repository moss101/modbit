//! Projections of the core aggregates (docs/31 `sessions`, `tasks`, `runs`,
//! `turns`, `run_steps`). Rows are derived from events through the domain
//! reducers, updated inside the same transaction as the append, and can be
//! rebuilt from the event log at any time (consumers are idempotent, docs/13).

use modbit_domain::event::{AggregateType, PayloadRef};
use modbit_domain::run::{Run, RunEvent};
use modbit_domain::session::{Session, SessionEvent};
use modbit_domain::step::{RunStep, StepEvent};
use modbit_domain::task::{Task, TaskEvent, TaskState};
use modbit_domain::turn::{Turn, TurnEvent};
use modbit_domain::{RunId, RunStepId, SessionId, TaskId, Timestamp, TurnId};
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
                "INSERT OR REPLACE INTO sessions (session_id, tenant_id, user_id, space_id, state, generation, created_at, updated_at, current_task_id, last_event_sequence, lease_generation, lease_owner)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
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
                "INSERT OR REPLACE INTO tasks (task_id, session_id, goal_text, workspace_id, base_revision, execution_profile, policy_profile_id, origin, state, wait_reason, generation, created_at, started_at, completed_at, failure_code)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
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
        // Aggregates whose projections belong to later milestones (protocol state, leases, checkpoints).
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
            "SELECT tenant_id, user_id, space_id, state, generation, created_at, updated_at, current_task_id, lease_generation, lease_owner FROM sessions WHERE session_id = ?1",
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
    }))
}

/// Load a task projection row.
pub fn load_task(tx: &rusqlite::Connection, id: &TaskId) -> Result<Option<Task>> {
    let row = tx
        .query_row(
            "SELECT session_id, goal_text, workspace_id, base_revision, execution_profile, policy_profile_id, origin, state, wait_reason, generation, created_at, started_at, completed_at, failure_code FROM tasks WHERE task_id = ?1",
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

/// Truncate the projection tables and replay every event from offset 0.
pub fn rebuild(tx: &Transaction<'_>, objects: &crate::ObjectStore) -> Result<u64> {
    for t in ["run_steps", "turns", "runs", "tasks", "sessions"] {
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
