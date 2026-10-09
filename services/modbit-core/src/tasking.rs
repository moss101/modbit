//! Task mode and execution preference (PX-051, PX-053; docs/65 AFW-D03,
//! AFW-D05, AFW-D13, AFW-D14).
//!
//! The mode and the preference are facts of a task's own log, written by the
//! user's typed commands (`SetTaskMode`, `SetExecutionPreference`, and the
//! `mode` and `preference` a `CreateTask` or `StartTask` carries) and by
//! nothing else: not by a model, not by a tool result, not by a client's
//! claim about its own posture. This module folds them out of the log and
//! hands them to the three places that act on them:
//!
//! * the Capability Kernel, which enforces the mode's posture — a narrowing
//!   of the profile's envelope, never a widening (`modbit_policy::kernel`);
//! * the run loop, which adopts the latest mode and preference at each round
//!   boundary and records that it did (`TaskPostureApplied`,
//!   `ExecutionPreferenceApplied`);
//! * the router's existing compile, which reads the objective to choose among
//!   the quality-floor rows the signed registry already defines. Nothing here
//!   changes how a plan is compiled, admitted or gated; with no registry the
//!   outcome is DIRECT and says why.

use std::collections::HashMap;
use std::sync::Arc;

use modbit_core_runtime::harness::HarnessState;
use modbit_domain::event::{Actor, AggregateType};
use modbit_domain::mode::{
    ExecutionPreference, Objective, TaskMode as Mode, valid_effort, valid_tier,
};
use modbit_domain::state::StateMachine;
use modbit_domain::task::{Task, TaskEvent};
use modbit_domain::{RunId, TaskId};
use modbit_event_store::{AppendRequest, CommandOutcome, CommandRecord, EventStore, NewEvent};
use modbit_protocol::v1 as wire;
use prost::Message;

use crate::runtime::{Lineage, StartConfig, append, typed};
use crate::server::Core;

/// What the last `ExecutionPreferenceApplied` said.
#[derive(Clone, Debug, Default)]
pub(crate) struct Applied {
    /// The preference event it applied.
    pub preference_offset: u64,
    /// The run it was applied in.
    pub run: Option<RunId>,
    pub outcome: String,
    pub reason_code: String,
    pub detail: String,
    pub floor_mode: String,
    pub effort_applied: Option<String>,
    pub service_tier_applied: Option<String>,
}

/// A task's mode and preference as its log says.
#[derive(Clone, Debug, Default)]
pub(crate) struct Facts {
    /// Last aggregate sequence folded.
    seq: u64,
    /// The mode the user last set.
    pub mode: Mode,
    /// The event that set it (0 = never set).
    pub mode_offset: u64,
    /// The preference as recorded.
    pub preference: ExecutionPreference,
    /// The event that recorded it (0 = none).
    pub preference_offset: u64,
    /// What routing last did with it.
    pub applied: Option<Applied>,
}

/// The per-Core fold of those facts, and the postures the kernel enforces.
#[derive(Default)]
pub struct Tasking {
    facts: std::sync::Mutex<HashMap<TaskId, Facts>>,
    /// The mode the live loop adopted at its latest round boundary. A call
    /// is decided under this, never under a mode set a moment ago: a mode
    /// change takes effect at the next safe boundary, and an effect already
    /// in flight is not retroactively denied.
    in_force: std::sync::Mutex<HashMap<TaskId, Mode>>,
}

impl Tasking {
    /// The task's facts, folded from its log (only what is new since last time).
    pub(crate) fn facts(&self, store: &EventStore, task: TaskId) -> Result<Facts, String> {
        let mut f = self
            .facts
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(&task)
            .cloned()
            .unwrap_or_default();
        let events = store
            .read_aggregate(task.as_bytes(), f.seq, usize::MAX)
            .map_err(|e| format!("reading the task's log: {e}"))?;
        if events.is_empty() {
            return Ok(f);
        }
        for e in &events {
            f.seq = f.seq.max(e.envelope.sequence);
            if !matches!(
                e.envelope.event_type.as_str(),
                "TaskModeSet" | "ExecutionPreferenceSet" | "ExecutionPreferenceApplied"
            ) {
                continue;
            }
            let payload = store
                .payload(&e.envelope)
                .map_err(|e| format!("reading a task event: {e}"))?;
            match serde_json::from_value::<TaskEvent>(payload) {
                Ok(TaskEvent::TaskModeSet { mode, .. }) => {
                    f.mode = mode;
                    f.mode_offset = e.offset;
                }
                Ok(TaskEvent::ExecutionPreferenceSet { preference, .. }) => {
                    f.preference = preference;
                    f.preference_offset = e.offset;
                }
                Ok(TaskEvent::ExecutionPreferenceApplied {
                    preference_offset,
                    outcome,
                    reason_code,
                    detail,
                    floor_mode,
                    effort_applied,
                    service_tier_applied,
                    ..
                }) => {
                    f.applied = Some(Applied {
                        preference_offset,
                        run: e.envelope.run_id,
                        outcome,
                        reason_code,
                        detail,
                        floor_mode,
                        effort_applied,
                        service_tier_applied,
                    });
                }
                _ => {}
            }
        }
        self.facts
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(task, f.clone());
        Ok(f)
    }

    /// The mode the Capability Kernel enforces on `task` right now: what the
    /// live loop adopted at its last round boundary, or, with no loop, what
    /// the user last set. An unreadable log is an error: the caller refuses.
    pub(crate) fn mode_in_force(&self, store: &EventStore, task: TaskId) -> Result<Mode, String> {
        if let Some(m) = self.in_force_now(task) {
            return Ok(m);
        }
        Ok(self.facts(store, task)?.mode)
    }

    /// The mode the live loop adopted, when one is running.
    pub(crate) fn in_force_now(&self, task: TaskId) -> Option<Mode> {
        self.in_force
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(&task)
            .copied()
    }

    fn adopt(&self, task: TaskId, mode: Mode) {
        self.in_force
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(task, mode);
    }

    fn release(&self, task: TaskId) {
        self.in_force
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&task);
    }
}

/// Held by a run's loop: when the loop ends, the task's posture goes back to
/// what the user last set.
pub(crate) struct LoopGuard {
    core: Arc<Core>,
    task: TaskId,
}

impl Drop for LoopGuard {
    fn drop(&mut self) {
        self.core.tools.tasking.release(self.task);
    }
}

/// The guard for a loop on `task`.
pub(crate) fn guard(core: &Arc<Core>, task: TaskId) -> LoopGuard {
    LoopGuard {
        core: Arc::clone(core),
        task,
    }
}

// ---- parsing and validation (typed refusals) ----

type Refusal = (String, String);

fn refuse(code: &str, why: impl Into<String>) -> Refusal {
    (code.to_owned(), why.into())
}

/// The mode a wire value names. `required` refuses UNSPECIFIED.
pub(crate) fn mode_of(value: i32, required: bool) -> Result<Option<Mode>, Refusal> {
    match wire::TaskMode::try_from(value) {
        Ok(wire::TaskMode::Unspecified) if required => Err(refuse(
            "MODE_REQUIRED",
            "a mode is required: AGENT, PLAN, DEBUG, MULTITASK or ASK",
        )),
        Ok(wire::TaskMode::Unspecified) => Ok(None),
        Ok(wire::TaskMode::Agent) => Ok(Some(Mode::Agent)),
        Ok(wire::TaskMode::Plan) => Ok(Some(Mode::Plan)),
        Ok(wire::TaskMode::Debug) => Ok(Some(Mode::Debug)),
        Ok(wire::TaskMode::Multitask) => Ok(Some(Mode::Multitask)),
        Ok(wire::TaskMode::Ask) => Ok(Some(Mode::Ask)),
        Err(_) => Err(refuse(
            "UNKNOWN_MODE",
            format!("{value} is not a task mode (AGENT, PLAN, DEBUG, MULTITASK, ASK)"),
        )),
    }
}

fn wire_mode(m: Mode) -> wire::TaskMode {
    match m {
        Mode::Agent => wire::TaskMode::Agent,
        Mode::Plan => wire::TaskMode::Plan,
        Mode::Debug => wire::TaskMode::Debug,
        Mode::Multitask => wire::TaskMode::Multitask,
        Mode::Ask => wire::TaskMode::Ask,
    }
}

fn wire_objective(o: Option<Objective>) -> wire::ObjectiveProfile {
    match o {
        None => wire::ObjectiveProfile::Unspecified,
        Some(Objective::Cost) => wire::ObjectiveProfile::Cost,
        Some(Objective::Balance) => wire::ObjectiveProfile::Balance,
        Some(Objective::Intelligence) => wire::ObjectiveProfile::Intelligence,
    }
}

/// What a command's `ExecutionPreference` asks to change.
#[derive(Clone, Debug, Default)]
pub(crate) struct Patch {
    objective: Option<Objective>,
    /// `Some(None)` = back to the catalog's own.
    effort: Option<Option<String>>,
    service_tier: Option<Option<String>>,
    /// `Some(None)` = drop the pin.
    pin: Option<Option<(String, String)>>,
}

impl Patch {
    /// Whether the command asks for nothing.
    pub(crate) fn is_empty(&self) -> bool {
        self.objective.is_none()
            && self.effort.is_none()
            && self.service_tier.is_none()
            && self.pin.is_none()
    }

    fn merge(&self, base: &ExecutionPreference) -> ExecutionPreference {
        let mut out = base.clone();
        if let Some(o) = self.objective {
            out.objective = Some(o);
        }
        if let Some(e) = &self.effort {
            out.effort.clone_from(e);
        }
        if let Some(t) = &self.service_tier {
            out.service_tier.clone_from(t);
        }
        if let Some(p) = &self.pin {
            out.pin.clone_from(p);
        }
        out
    }

    /// The explicitly set fields, as the event records the command.
    fn as_preference(&self) -> ExecutionPreference {
        ExecutionPreference {
            objective: self.objective,
            effort: self.effort.clone().flatten(),
            service_tier: self.service_tier.clone().flatten(),
            pin: self.pin.clone().flatten(),
        }
    }
}

/// Validate a wire preference into a patch. Every refusal is typed and
/// changes no state.
pub(crate) fn parse_patch(p: &wire::ExecutionPreference) -> Result<Patch, Refusal> {
    let objective = match wire::ObjectiveProfile::try_from(p.objective) {
        Ok(wire::ObjectiveProfile::Unspecified) => None,
        Ok(wire::ObjectiveProfile::Cost) => Some(Objective::Cost),
        Ok(wire::ObjectiveProfile::Balance) => Some(Objective::Balance),
        Ok(wire::ObjectiveProfile::Intelligence) => Some(Objective::Intelligence),
        Err(_) => {
            return Err(refuse(
                "UNKNOWN_OBJECTIVE",
                format!(
                    "{} is not an objective profile (COST, BALANCE, INTELLIGENCE)",
                    p.objective
                ),
            ));
        }
    };
    let mut patch = Patch {
        objective,
        ..Patch::default()
    };
    let effort = p.effort.trim();
    if !effort.is_empty() {
        patch.effort = Some(if effort == "default" {
            None
        } else if valid_effort(effort) {
            Some(effort.to_owned())
        } else {
            return Err(refuse(
                "INVALID_EFFORT",
                format!("effort `{effort}` is not low, medium, high or default"),
            ));
        });
    }
    let tier = p.service_tier.trim();
    if !tier.is_empty() {
        patch.service_tier = Some(if tier == "default" {
            None
        } else if valid_tier(tier) {
            Some(tier.to_owned())
        } else {
            return Err(refuse(
                "INVALID_TIER",
                format!(
                    "service tier `{tier}` is not a tier name (letters, digits and underscore, at most 32) or default"
                ),
            ));
        });
    }
    let (e, m) = (p.pin_endpoint.trim(), p.pin_model.trim());
    if p.clear_pin {
        if !e.is_empty() || !m.is_empty() {
            return Err(refuse(
                "PIN_CONFLICT",
                "a pin is either set or cleared in one command, not both",
            ));
        }
        patch.pin = Some(None);
    } else if !e.is_empty() || !m.is_empty() {
        if e.is_empty() || m.is_empty() {
            return Err(refuse(
                "PIN_INCOMPLETE",
                "a manual pin names an endpoint and a model, both",
            ));
        }
        patch.pin = Some(Some((e.to_owned(), m.to_owned())));
    }
    Ok(patch)
}

/// Whether a pin is one this Core can dispatch and the policy in force
/// allows. A pin the policy forbids never reaches the compiler.
fn check_pin(core: &Core, task: &Task, pin: &(String, String)) -> Result<(), Refusal> {
    let (endpoint, model) = pin;
    if !core.gateway.endpoints().iter().any(|e| &e.name == endpoint) {
        return Err(refuse(
            "NO_PROVIDER",
            format!("provider endpoint `{endpoint}` is not registered in this Core"),
        ));
    }
    if core.gateway.capability(endpoint, model).is_none() {
        return Err(refuse(
            "UNKNOWN_MODEL",
            format!("`{endpoint}` serves no model `{model}`"),
        ));
    }
    // The organisation's block rule is the gateway's own check at dispatch;
    // a pin it would refuse there is refused here with the same rule named,
    // so the person learns it before any token (PX-056, AFW-H02).
    if let Some(ep) = core
        .gateway
        .endpoints()
        .iter()
        .find(|e| &e.name == endpoint)
        && let Some(rule) = core
            .gateway
            .policy()
            .blocking_rule(endpoint, ep.kind, model)
    {
        return Err(refuse(
            "POLICY_BLOCKED",
            format!("{endpoint}/{model} is blocked by the organisation model policy ({rule})"),
        ));
    }
    match crate::routing::refused_by_model_policy(
        crate::routing::model_policy(core, task).as_deref(),
        [(endpoint.clone(), model.clone())],
    ) {
        Some((code, why)) => Err((code, why)),
        None => Ok(()),
    }
}

// ---- commands ----

fn record_of(core: &Core, env: &wire::CommandEnvelope, command_id: [u8; 16]) -> CommandRecord {
    CommandRecord {
        command_id: modbit_domain::EventId::from_bytes(command_id),
        tenant_id: core.tenant_id,
        command_type: env.command_type.clone(),
        request_hash: modbit_event_store::objects::sha256_hex(
            &[env.command_type.as_bytes(), b"\0", &env.payload].concat(),
        ),
    }
}

fn reject(cid: Option<wire::Id>, (code, message): Refusal) -> wire::CommandAck {
    crate::server::reject(cid, &code, message)
}

fn id16(id: &wire::Id) -> Option<[u8; 16]> {
    <[u8; 16]>::try_from(id.value.as_slice()).ok()
}

/// `SetTaskMode`, `SetExecutionPreference` and `GetTaskPosture`.
pub(crate) async fn handle(core: &Arc<Core>, env: &wire::CommandEnvelope) -> wire::CommandAck {
    let cid = env.command_id.clone();
    let Some(command_id) = env.command_id.as_ref().and_then(id16) else {
        return reject(cid, refuse("BAD_COMMAND_ID", "command_id must be 16 bytes"));
    };
    match env.command_type.as_str() {
        "SetTaskMode" => {
            let Ok(p) = wire::SetTaskMode::decode(env.payload.as_slice()) else {
                return reject(cid, refuse("BAD_PAYLOAD", "SetTaskMode"));
            };
            set_mode(core, env, cid, command_id, p).await
        }
        "SetExecutionPreference" => {
            let Ok(p) = wire::SetExecutionPreference::decode(env.payload.as_slice()) else {
                return reject(cid, refuse("BAD_PAYLOAD", "SetExecutionPreference"));
            };
            set_preference(core, env, cid, command_id, p).await
        }
        _ => {
            let Ok(p) = wire::GetTaskPosture::decode(env.payload.as_slice()) else {
                return reject(cid, refuse("BAD_PAYLOAD", "GetTaskPosture"));
            };
            let Some(task_id) = p.task_id.as_ref().and_then(id16).map(TaskId::from_bytes) else {
                return reject(cid, refuse("BAD_PAYLOAD", "task_id required"));
            };
            match posture_view(core, task_id).await {
                Ok(v) => crate::server::accept(cid, false, v.encode_to_vec()),
                Err(r) => reject(cid, r),
            }
        }
    }
}

/// The task a command names, refused when it is unknown or over.
/// A command that names its session (`CommandEnvelope.session_id`) acts only on that session's tasks: a task of another session
/// is refused `WRONG_SESSION` before its pin, its lease or its log is looked at. A command that names no session keeps the
/// lease check against the task's own session.
fn wrong_session(
    cid: &Option<wire::Id>,
    env: &wire::CommandEnvelope,
    task: &Task,
) -> Option<wire::CommandAck> {
    let named = env.session_id.as_ref().and_then(id16)?;
    (named != *task.session_id.as_bytes()).then(|| {
        reject(
            cid.clone(),
            refuse("WRONG_SESSION", "the task belongs to another session"),
        )
    })
}

async fn open_task(core: &Core, task_id: TaskId) -> Result<Task, Refusal> {
    let store = core.store.lock().await;
    match store.task(&task_id) {
        Ok(Some(t)) if t.state.is_terminal() => Err(refuse(
            "TASK_TERMINAL",
            format!(
                "the task is {:?}; its mode and preference no longer change",
                t.state
            ),
        )),
        Ok(Some(t)) => Ok(t),
        Ok(None) => Err(refuse("UNKNOWN_TASK", task_id.to_string())),
        Err(e) => Err(refuse(crate::server::error_code(&e), e.to_string())),
    }
}

async fn set_mode(
    core: &Arc<Core>,
    env: &wire::CommandEnvelope,
    cid: Option<wire::Id>,
    command_id: [u8; 16],
    p: wire::SetTaskMode,
) -> wire::CommandAck {
    let Some(task_id) = p.task_id.as_ref().and_then(id16).map(TaskId::from_bytes) else {
        return reject(cid, refuse("BAD_PAYLOAD", "task_id required"));
    };
    let mode = match mode_of(p.mode, true) {
        Ok(Some(m)) => m,
        Ok(None) => unreachable!("a required mode is never absent"),
        Err(r) => return reject(cid, r),
    };
    let task = match open_task(core, task_id).await {
        Ok(t) => t,
        Err(r) => return reject(cid, r),
    };
    if let Some(ack) = wrong_session(&cid, env, &task) {
        return ack;
    }
    if let Err(ack) = crate::server::require_lease(core, &cid, env, &task.session_id).await {
        return ack;
    }
    let running = core.runtime.is_running(&task_id).await;
    let effective = if running {
        "NEXT_ROUND_BOUNDARY"
    } else {
        "IMMEDIATE"
    };
    let mut store = core.store.lock().await;
    let facts = match core.tools.tasking.facts(&store, task_id) {
        Ok(f) => f,
        Err(e) => return reject(cid, refuse("STORE_ERROR", e)),
    };
    let previous = facts.mode;
    let accepted_plan_version = if previous == Mode::Plan && mode != Mode::Plan {
        crate::plans::view(&store, task_id)
            .versions
            .iter()
            .map(|v| v.version)
            .max()
            .unwrap_or(0)
    } else {
        0
    };
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
            "TaskModeSet",
            &TaskEvent::TaskModeSet {
                mode,
                previous: Some(previous),
                source: "user".into(),
                reason: p.reason.chars().take(512).collect(),
                accepted_plan_version,
            },
            Actor::User(core.user_id),
        )],
    };
    match store.execute_command(record_of(core, env, command_id), req) {
        Ok(outcome) => {
            let (events, replayed) = match outcome {
                CommandOutcome::Applied(e) => (e, false),
                CommandOutcome::Replayed(e) => (e, true),
            };
            let offset = events.last().map_or(0, |e| e.offset);
            if !replayed {
                core.last_offset.send_replace(offset);
            }
            crate::server::accept(
                cid,
                replayed,
                wire::TaskModeChanged {
                    task_id: Some(crate::server::wire_id(task_id.as_bytes())),
                    mode: wire_mode(mode) as i32,
                    previous_mode: wire_mode(previous) as i32,
                    effective: effective.into(),
                    offset,
                    accepted_plan_version,
                }
                .encode_to_vec(),
            )
        }
        Err(e) => reject(cid, refuse(crate::server::error_code(&e), e.to_string())),
    }
}

async fn set_preference(
    core: &Arc<Core>,
    env: &wire::CommandEnvelope,
    cid: Option<wire::Id>,
    command_id: [u8; 16],
    p: wire::SetExecutionPreference,
) -> wire::CommandAck {
    let Some(task_id) = p.task_id.as_ref().and_then(id16).map(TaskId::from_bytes) else {
        return reject(cid, refuse("BAD_PAYLOAD", "task_id required"));
    };
    let patch = match parse_patch(&p.preference.clone().unwrap_or_default()) {
        Ok(patch) => patch,
        Err(r) => return reject(cid, r),
    };
    if patch.is_empty() {
        return reject(
            cid,
            refuse(
                "EMPTY_PREFERENCE",
                "the command sets nothing: name an objective, an effort, a service tier or a pin",
            ),
        );
    }
    let task = match open_task(core, task_id).await {
        Ok(t) => t,
        Err(r) => return reject(cid, r),
    };
    if let Some(ack) = wrong_session(&cid, env, &task) {
        return ack;
    }
    if let Some(Some(pin)) = &patch.pin
        && let Err(r) = check_pin(core, &task, pin)
    {
        return reject(cid, r);
    }
    if let Err(ack) = crate::server::require_lease(core, &cid, env, &task.session_id).await {
        return ack;
    }
    let running = core.runtime.is_running(&task_id).await;
    let mut store = core.store.lock().await;
    let facts = match core.tools.tasking.facts(&store, task_id) {
        Ok(f) => f,
        Err(e) => return reject(cid, refuse("STORE_ERROR", e)),
    };
    let preference = patch.merge(&facts.preference);
    let pin_only =
        patch.objective.is_none() && patch.effort.is_none() && patch.service_tier.is_none();
    let effective = if pin_only || !running {
        "NEXT_RUN"
    } else {
        "NEXT_ROUND_BOUNDARY"
    };
    if preference == facts.preference {
        // Nothing would change: no event, and the answer says so.
        let routing = routing_projection(core, &task, &facts.preference);
        return crate::server::accept(
            cid,
            false,
            wire::ExecutionPreferenceSet {
                task_id: Some(crate::server::wire_id(task_id.as_bytes())),
                preference: Some(preference_view(&facts)),
                offset: facts.preference_offset,
                effective: "UNCHANGED".into(),
                routing: Some(routing),
            }
            .encode_to_vec(),
        );
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
            "ExecutionPreferenceSet",
            &TaskEvent::ExecutionPreferenceSet {
                preference: preference.clone(),
                patch: patch.as_preference(),
                source: "user".into(),
            },
            Actor::User(core.user_id),
        )],
    };
    match store.execute_command(record_of(core, env, command_id), req) {
        Ok(outcome) => {
            let (events, replayed) = match outcome {
                CommandOutcome::Applied(e) => (e, false),
                CommandOutcome::Replayed(e) => (e, true),
            };
            let offset = events.last().map_or(0, |e| e.offset);
            if !replayed {
                core.last_offset.send_replace(offset);
            }
            let facts = match core.tools.tasking.facts(&store, task_id) {
                Ok(f) => f,
                Err(e) => return reject(cid, refuse("STORE_ERROR", e)),
            };
            let routing = routing_projection(core, &task, &facts.preference);
            crate::server::accept(
                cid,
                replayed,
                wire::ExecutionPreferenceSet {
                    task_id: Some(crate::server::wire_id(task_id.as_bytes())),
                    preference: Some(preference_view(&facts)),
                    offset,
                    effective: effective.into(),
                    routing: Some(routing),
                }
                .encode_to_vec(),
            )
        }
        Err(e) => reject(cid, refuse(crate::server::error_code(&e), e.to_string())),
    }
}

/// The events a `CreateTask` or `StartTask` appends for the mode and the
/// preference it carries, validated first. `pin` is refused at creation (the
/// task's model is chosen when it starts).
pub(crate) fn creation_events(
    mode: i32,
    preference: Option<&wire::ExecutionPreference>,
    allow_pin: bool,
    source: &str,
    actor: &Actor,
) -> Result<Vec<NewEvent>, Refusal> {
    let mode = mode_of(mode, false)?;
    let patch = match preference {
        Some(p) => parse_patch(p)?,
        None => Patch::default(),
    };
    if !allow_pin && patch.pin.is_some() {
        return Err(refuse(
            "PIN_NOT_HERE",
            "a manual pin is the model this command's task starts with: name it on StartTask (endpoint and model) or set it with SetExecutionPreference",
        ));
    }
    let mut events = Vec::new();
    if let Some(mode) = mode.filter(|m| *m != Mode::Agent) {
        events.push(typed(
            "TaskModeSet",
            &TaskEvent::TaskModeSet {
                mode,
                previous: None,
                source: "create".into(),
                reason: String::new(),
                accepted_plan_version: 0,
            },
            actor.clone(),
        ));
    }
    if !patch.is_empty() {
        events.push(typed(
            "ExecutionPreferenceSet",
            &TaskEvent::ExecutionPreferenceSet {
                preference: patch.merge(&ExecutionPreference::default()),
                patch: patch.as_preference(),
                source: source.into(),
            },
            actor.clone(),
        ));
    }
    Ok(events)
}

/// `StartTask`'s preference: validated, and recorded on the task before the
/// run routes, so the router's first compile reads it. Returns the manual pin
/// the task's recorded preference holds (the model a start with none named
/// uses).
pub(crate) async fn on_start(
    core: &Core,
    task: &Task,
    preference: Option<&wire::ExecutionPreference>,
    actor: &Actor,
) -> Result<Option<(String, String)>, Refusal> {
    let patch = match preference {
        Some(p) => {
            let patch = parse_patch(p)?;
            if patch.pin.is_some() {
                return Err(refuse(
                    "PIN_NOT_HERE",
                    "a manual pin on StartTask is its `endpoint` and `model`",
                ));
            }
            patch
        }
        None => Patch::default(),
    };
    let mut store = core.store.lock().await;
    let facts = core
        .tools
        .tasking
        .facts(&store, task.task_id)
        .map_err(|e| refuse("STORE_ERROR", e))?;
    if !patch.is_empty() {
        let merged = patch.merge(&facts.preference);
        if merged != facts.preference {
            append(
                &mut store,
                core,
                Lineage::task(core.tenant_id, task.session_id, task.task_id),
                AggregateType::Task,
                *task.task_id.as_bytes(),
                vec![typed(
                    "ExecutionPreferenceSet",
                    &TaskEvent::ExecutionPreferenceSet {
                        preference: merged.clone(),
                        patch: patch.as_preference(),
                        source: "start_task".into(),
                    },
                    actor.clone(),
                )],
            )
            .map_err(|e| refuse("STORE_ERROR", e))?;
            return Ok(merged.pin);
        }
    }
    Ok(facts.preference.pin)
}

// ---- views ----

/// The preference as a wire view.
pub(crate) fn preference_view(f: &Facts) -> wire::ExecutionPreferenceView {
    let applied = f.applied.as_ref();
    wire::ExecutionPreferenceView {
        objective: wire_objective(f.preference.objective) as i32,
        effort: f.preference.effort.clone().unwrap_or_default(),
        service_tier: f.preference.service_tier.clone().unwrap_or_default(),
        pin_endpoint: f
            .preference
            .pin
            .as_ref()
            .map(|p| p.0.clone())
            .unwrap_or_default(),
        pin_model: f
            .preference
            .pin
            .as_ref()
            .map(|p| p.1.clone())
            .unwrap_or_default(),
        offset: f.preference_offset,
        applied_offset: applied.map_or(0, |a| a.preference_offset),
        effort_applied: applied
            .and_then(|a| a.effort_applied.clone())
            .unwrap_or_default(),
        service_tier_applied: applied
            .and_then(|a| a.service_tier_applied.clone())
            .unwrap_or_default(),
    }
}

/// What routing did with the recorded preference, or, before it has read it,
/// what it will do: with no signed registry, DIRECT, and why.
fn routing_projection(
    core: &Core,
    task: &Task,
    preference: &ExecutionPreference,
) -> wire::RoutingOutcomeView {
    match crate::model_registry::for_task(core, task) {
        None => wire::RoutingOutcomeView {
            outcome: "DIRECT".into(),
            reason_code: "NO_ACTIVE_REGISTRY".into(),
            detail: NO_REGISTRY_DETAIL.into(),
            floor_mode: String::new(),
            registry_generation: String::new(),
        },
        Some(registry) => {
            let (floor, choice) = floor_for(&registry, preference.objective);
            wire::RoutingOutcomeView {
                outcome: "NOT_YET_EVALUATED".into(),
                reason_code: choice.code().into(),
                detail: format!(
                    "the router reads the preference at its next compile; it would use the `{}` floor",
                    floor.map(|f| f.mode.as_str()).unwrap_or("none")
                ),
                floor_mode: floor.map(|f| f.mode.clone()).unwrap_or_default(),
                registry_generation: registry.generation().to_owned(),
            }
        }
    }
}

const NO_REGISTRY_DETAIL: &str = "no signed registry is active: the direct path dispatches the one binding the task starts with, so an objective cannot select a different one; the preference is recorded, and its effort and tier apply to that binding";

/// The task's posture view: its mode, the posture in force and its preference.
pub(crate) async fn posture_view(
    core: &Core,
    task_id: TaskId,
) -> Result<wire::TaskPostureView, Refusal> {
    let store = core.store.lock().await;
    let task = match store.task(&task_id) {
        Ok(Some(t)) => t,
        Ok(None) => return Err(refuse("UNKNOWN_TASK", task_id.to_string())),
        Err(e) => return Err(refuse(crate::server::error_code(&e), e.to_string())),
    };
    let facts = core
        .tools
        .tasking
        .facts(&store, task_id)
        .map_err(|e| refuse("STORE_ERROR", e))?;
    let in_force = core
        .tools
        .tasking
        .mode_in_force(&store, task_id)
        .map_err(|e| refuse("STORE_ERROR", e))?;
    Ok(build_posture(core, &task, &facts, in_force))
}

fn build_posture(core: &Core, task: &Task, facts: &Facts, in_force: Mode) -> wire::TaskPostureView {
    let posture = in_force.posture();
    let routing = match &facts.applied {
        Some(a) => wire::RoutingOutcomeView {
            outcome: a.outcome.clone(),
            reason_code: a.reason_code.clone(),
            detail: a.detail.clone(),
            floor_mode: a.floor_mode.clone(),
            // What is active now, so a registry activated or rolled back
            // since the last dispatch is what the picker states.
            registry_generation: crate::model_registry::for_task(core, task)
                .map(|r| r.generation().to_owned())
                .unwrap_or_default(),
        },
        None => routing_projection(core, task, &facts.preference),
    };
    wire::TaskPostureView {
        task_id: Some(crate::server::wire_id(task.task_id.as_bytes())),
        mode: wire_mode(facts.mode) as i32,
        mode_offset: facts.mode_offset,
        mode_in_force: wire_mode(in_force) as i32,
        posture: Some(wire::ModePostureView {
            effect_ceiling: posture
                .effect_ceiling
                .map_or_else(|| "NONE".to_owned(), |c| format!("{c:?}").to_uppercase()),
            allowed_capabilities: posture
                .allowed_capabilities
                .map(|c| c.iter().map(|s| (*s).to_owned()).collect())
                .unwrap_or_default(),
            subagents: posture.subagents,
            reproduction_first: posture.reproduction_first,
            writes: posture.writes(),
        }),
        preference: Some(preference_view(facts)),
        routing: Some(routing),
    }
}

/// A `GetTaskStatus` / `GetEffectivePolicy` posture, when the task's log
/// can be read; absent otherwise.
pub(crate) fn posture_for(
    core: &Core,
    store: &EventStore,
    task: &Task,
) -> Option<wire::TaskPostureView> {
    let facts = core.tools.tasking.facts(store, task.task_id).ok()?;
    let in_force = core.tools.tasking.mode_in_force(store, task.task_id).ok()?;
    Some(build_posture(core, task, &facts, in_force))
}

/// The mode a `TaskView` carries.
pub(crate) fn mode_for(core: &Core, store: &EventStore, task_id: TaskId) -> i32 {
    let mode = core
        .tools
        .tasking
        .facts(store, task_id)
        .map_or(Mode::Agent, |f| f.mode);
    wire_mode(mode) as i32
}

/// The routing view with what the user asked for and what routing did.
pub(crate) async fn annotate_routing(
    core: &Core,
    task_id: TaskId,
    view: &mut wire::RoutingPlanView,
) {
    let store = core.store.lock().await;
    let Ok(Some(task)) = store.task(&task_id) else {
        return;
    };
    let Ok(facts) = core.tools.tasking.facts(&store, task_id) else {
        return;
    };
    drop(store);
    view.preference = Some(preference_view(&facts));
    view.preference_routing = Some(match &facts.applied {
        Some(a) => wire::RoutingOutcomeView {
            outcome: a.outcome.clone(),
            reason_code: a.reason_code.clone(),
            detail: a.detail.clone(),
            floor_mode: a.floor_mode.clone(),
            registry_generation: crate::model_registry::for_task(core, &task)
                .map(|r| r.generation().to_owned())
                .unwrap_or_default(),
        },
        None => routing_projection(core, &task, &facts.preference),
    });
}

// ---- the router's read ----

/// How an objective met the registry's floor rows.
pub(crate) enum FloorChoice {
    /// No objective was asked for: the `auto` floor, as before.
    Default,
    /// The row the objective names.
    Selected,
    /// The registry has no row for the objective: `auto` stands.
    Undefined,
}

impl FloorChoice {
    fn code(&self) -> &'static str {
        match self {
            FloorChoice::Default => "NO_OBJECTIVE",
            FloorChoice::Selected => "FLOOR_APPLIED",
            FloorChoice::Undefined => "FLOOR_UNDEFINED",
        }
    }
}

/// The registry floor row the objective selects: the row of the mode the
/// registry document names for it (`economy` for COST, `auto` for BALANCE,
/// `quality` for INTELLIGENCE), the `auto` row when none was asked for or the
/// registry defines no row for it. It only chooses among rows the signed
/// registry already carries.
pub(crate) fn floor_for(
    registry: &modbit_providers::registry::ModelRegistry,
    objective: Option<Objective>,
) -> (
    Option<&modbit_providers::registry::QualityFloor>,
    FloorChoice,
) {
    let floors = &registry.document.quality_floors;
    let auto = floors.iter().find(|f| f.mode == "auto");
    match objective {
        None => (auto, FloorChoice::Default),
        Some(o) => match floors.iter().find(|f| f.mode == o.floor_mode()) {
            Some(f) => (Some(f), FloorChoice::Selected),
            None => (auto, FloorChoice::Undefined),
        },
    }
}

/// The objective recorded for `task`, read by the router's compile.
pub(crate) fn objective_of(core: &Core, store: &EventStore, task: TaskId) -> Option<Objective> {
    core.tools
        .tasking
        .facts(store, task)
        .ok()
        .and_then(|f| f.preference.objective)
}

// ---- the run loop's boundary ----

/// What a run's loop has adopted so far.
pub(crate) struct Edge {
    /// Whether the goal itself reports a failure (PX-039's keyword rule).
    reproduction_from_goal: bool,
    mode: Option<Mode>,
    preference_offset: u64,
    first: bool,
    effort: Option<String>,
    service_tier: Option<String>,
    /// The preference offset the dispatch's route record names.
    pub preference_offset_applied: u64,
    /// The mode a resumed run decides its interrupted round under (the one
    /// the round was frozen with, REQ-PX-131); taken at the first boundary.
    frozen_mode: Option<Mode>,
}

impl Edge {
    /// An edge for a loop on `task`.
    pub(crate) fn new(task: &Task) -> Self {
        Self {
            reproduction_from_goal: modbit_core_runtime::harness::goal_reports_failure(
                &task.goal_text,
            ),
            mode: None,
            preference_offset: 0,
            first: true,
            effort: None,
            service_tier: None,
            preference_offset_applied: 0,
            frozen_mode: None,
        }
    }

    /// The run resumes inside a round it froze before the Core stopped: the
    /// mode of that round stands for it, and the user's latest mode applies
    /// at the next boundary (REQ-PX-131).
    pub(crate) fn freeze_mode(&mut self, mode: Mode) {
        self.frozen_mode = Some(mode);
    }

    /// The limits a dispatch of `endpoint/model` carries: the catalog's own,
    /// with the effort and tier the user's preference set where the model can
    /// take them. An effort for a model that exposes no reasoning is not sent
    /// (the route would refuse it); the boundary record said so.
    pub(crate) fn overlay(
        &self,
        core: &Core,
        endpoint: &str,
        model: &str,
        mut limits: crate::model_registry::DispatchLimits,
    ) -> crate::model_registry::DispatchLimits {
        if let Some(effort) = &self.effort
            && core
                .gateway
                .capability(endpoint, model)
                .is_some_and(|c| c.reasoning)
        {
            limits.reasoning_effort = Some(effort.clone());
        }
        if let Some(tier) = &self.service_tier {
            limits.service_tier = Some(tier.clone());
        }
        limits
    }
}

/// The run loop's round boundary: adopt the latest mode, make its posture the
/// one the kernel enforces, and read the latest preference. Everything it
/// adopts is on the log; a task whose mode and preference were never set
/// leaves no trace and runs exactly as before.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn at_boundary(
    core: &Arc<Core>,
    task: &Task,
    run_id: RunId,
    cfg: &mut StartConfig,
    edge: &mut Edge,
    state: &mut HarnessState,
    lt: Lineage,
    actor: &Actor,
) {
    let boundary = if edge.first { "RUN_START" } else { "ROUND" };
    let facts = {
        let store = core.store.lock().await;
        match core.tools.tasking.facts(&store, task.task_id) {
            Ok(f) => f,
            Err(e) => {
                // Unreadable: the posture in force stands (a task never runs
                // under less than it had); nothing new is adopted.
                eprintln!("modbit-core: reading task {} posture: {e}", task.task_id);
                return;
            }
        }
    };
    if edge.first {
        edge.preference_offset = facts
            .applied
            .as_ref()
            .filter(|a| a.run == Some(run_id))
            .map_or(0, |a| a.preference_offset);
        edge.preference_offset_applied = edge.preference_offset;
        if let Some(a) = facts.applied.as_ref().filter(|a| a.run == Some(run_id)) {
            edge.effort.clone_from(&a.effort_applied);
            edge.service_tier.clone_from(&a.service_tier_applied);
        }
    }
    // ---- the mode ----
    let (adopted, adopted_offset) = match edge.frozen_mode.take() {
        Some(frozen) if frozen != facts.mode => (frozen, 0),
        _ => (facts.mode, facts.mode_offset),
    };
    if edge.mode != Some(adopted) {
        let silent_default = edge.mode.is_none() && adopted_offset == 0;
        core.tools.tasking.adopt(task.task_id, adopted);
        if !silent_default {
            let mut store = core.store.lock().await;
            let _ = append(
                &mut store,
                core,
                lt,
                AggregateType::Task,
                *task.task_id.as_bytes(),
                vec![typed(
                    "TaskPostureApplied",
                    &TaskEvent::TaskPostureApplied {
                        mode: adopted,
                        previous: edge.mode,
                        boundary: boundary.into(),
                        mode_offset: adopted_offset,
                    },
                    actor.clone(),
                )],
            );
        }
        edge.mode = Some(adopted);
    }
    // PX-039: a fix needs its failure reproduced first. DEBUG makes that
    // mandatory whatever the goal says; the goal's own rule still holds.
    state.goal_reports_failure =
        edge.reproduction_from_goal || adopted.posture().reproduction_first;
    // ---- the preference ----
    if facts.preference_offset > edge.preference_offset {
        apply_preference(core, task, run_id, cfg, edge, &facts, boundary, lt, actor).await;
    }
    edge.first = false;
}

#[allow(clippy::too_many_arguments)]
async fn apply_preference(
    core: &Arc<Core>,
    task: &Task,
    run_id: RunId,
    cfg: &mut StartConfig,
    edge: &mut Edge,
    facts: &Facts,
    boundary: &str,
    lt: Lineage,
    actor: &Actor,
) {
    let pref = &facts.preference;
    let registry = crate::model_registry::for_task(core, task);
    let (outcome, reason_code, mut detail, floor_mode) = match &registry {
        None => (
            "DIRECT",
            "NO_ACTIVE_REGISTRY".to_owned(),
            NO_REGISTRY_DETAIL.to_owned(),
            String::new(),
        ),
        Some(_) if cfg.pinned => (
            "PINNED",
            "MANUAL_PIN".to_owned(),
            format!(
                "the route is the user's pin {}/{}; a pin is never switched away from",
                cfg.endpoint, cfg.model
            ),
            String::new(),
        ),
        Some(registry) => {
            // The objective changes which floor row the router compiles
            // under. At a round boundary the router re-evaluates the route
            // now, through the same door a boundary re-evaluation always
            // uses; at the run's start its first compile has already read
            // the preference.
            if boundary == "ROUND" && pref.objective.is_some() {
                crate::runtime::reroute_at_boundary(core, task, run_id, cfg, "PREFERENCE", actor)
                    .await;
            }
            let (floor, choice) = floor_for(registry, pref.objective);
            (
                "ROUTED",
                choice.code().to_owned(),
                format!(
                    "the router compiles under the `{}` floor",
                    floor.map_or("none", |f| f.mode.as_str())
                ),
                floor.map(|f| f.mode.clone()).unwrap_or_default(),
            )
        }
    };
    // Effort and tier ride on whatever binding dispatches.
    let reasoning = core
        .gateway
        .capability(&cfg.endpoint, &cfg.model)
        .is_some_and(|c| c.reasoning);
    let effort_applied = match &pref.effort {
        Some(e) if reasoning => Some(e.clone()),
        Some(e) => {
            detail.push_str(&format!(
                "; effort `{e}` is not sent: {}/{} exposes no reasoning (EFFORT_NOT_SUPPORTED)",
                cfg.endpoint, cfg.model
            ));
            None
        }
        None => None,
    };
    if pref.pin.is_some() && !cfg.pinned {
        detail.push_str("; the manual pin applies from the next run (PIN_NEXT_RUN)");
    }
    edge.effort.clone_from(&effort_applied);
    edge.service_tier.clone_from(&pref.service_tier);
    edge.preference_offset = facts.preference_offset;
    edge.preference_offset_applied = facts.preference_offset;
    let mut store = core.store.lock().await;
    let _ = append(
        &mut store,
        core,
        lt,
        AggregateType::Task,
        *task.task_id.as_bytes(),
        vec![typed(
            "ExecutionPreferenceApplied",
            &TaskEvent::ExecutionPreferenceApplied {
                preference: pref.clone(),
                preference_offset: facts.preference_offset,
                boundary: boundary.into(),
                outcome: outcome.into(),
                reason_code,
                detail,
                floor_mode,
                effort_applied,
                service_tier_applied: pref.service_tier.clone(),
            },
            actor.clone(),
        )],
    );
}
