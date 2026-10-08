//! In-run control and run modes (REQ-PX-050, REQ-PX-057; docs/65 AFW-E01 to
//! AFW-E07, AFW-F06 to AFW-F12).
//!
//! **The queue.** A task's input queue is the fold of its own log
//! (`modbit_core_runtime::input_queue`): `QueueInput` appends `TaskInputQueued`;
//! the commands here append `TaskInputEdited`, `TaskInputRemoved`,
//! `TaskInputReordered` and `TaskInterruptRequested`; the run loop consumes it
//! with `TaskSteered` naming what it dispatched. Every command checks the
//! queue and appends under one hold of the store, so an edit can never land on
//! an input the loop dispatched a moment before, and a Core killed at any
//! point rebuilds the same queue.
//!
//! **The interrupt.** `SendQueuedInputNow` and `InterruptTask` record the
//! request on the log and then tell the live loop (`Runtime::interrupt`). The
//! loop cuts the model stream (it ends aborted with source `USER_INTERRUPT`),
//! cancels the tool call in flight through the broker, skips the calls that
//! had not started, and records what happened (`TaskInterruptApplied`) at its
//! next boundary — where a sent-now input starts the next turn, or a stop
//! parks the run. Both are idempotent per interrupt id.
//!
//! **Run modes and rules.** `RunPolicies` folds `RunModeSet`, `AllowRuleAdded`
//! and `AllowRuleRevoked` from the log with a cursor; the Capability Kernel
//! reads the result when it reaches the approval step
//! (`modbit_policy::runmode`). `RUN_EVERYTHING` lives only in the Core process
//! that recorded it: the fold ignores one recorded before this process booted.

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use modbit_core_runtime::input_queue::{self, InputQueue, ItemState, QueueItem};
use modbit_domain::event::{Actor, AggregateType};
use modbit_domain::runmode::{AllowRule, AskClass, RuleScope, RunMode};
use modbit_domain::state::StateMachine;
use modbit_domain::task::{InputMode, Task, TaskEvent, TaskOrigin};
use modbit_domain::{TaskId, Timestamp};
use modbit_event_store::{AppendRequest, CommandOutcome, CommandRecord, EventStore};
use modbit_protocol::v1 as wire;
use prost::Message;

use crate::runtime::{PauseRequest, typed};
use crate::server::{Core, accept, id16, reject, require_lease, wire_id};

type Refusal = (String, String);

fn refuse(code: &str, why: impl Into<String>) -> Refusal {
    (code.to_owned(), why.into())
}

/// The warning a move to a mode that approves more must acknowledge.
pub(crate) const RISK_WARNING: &str = "A mode that approves effects without asking lets anything the agent is made to attempt run inside its envelope, including an attempt planted by a prompt injection in a file, a page or a tool result, and data it can reach can leave through an effect that is approved without you seeing it. Effects that write outside the workspace, reach the network, use a secret, touch protected or configuration paths, delete, push or escalate still ask in every mode.";

/// Most live allowlist rules this Core holds.
const MAX_RULES: usize = 1000;
/// Most characters of a free-text field the log keeps.
const MAX_NOTE: usize = 512;

// ---- the policy fold: run modes and allowlist rules ----

#[derive(Clone, Debug)]
struct ModeFact {
    mode: RunMode,
    acknowledged: bool,
    offset: u64,
}

#[derive(Clone, Debug)]
pub(crate) struct RuleRow {
    pub rule: AllowRule,
    pub offset: u64,
    /// Who revoked it, once it is revoked.
    pub revoked_by: Option<String>,
}

#[derive(Default)]
struct Fold {
    /// Every event up to this offset has been read.
    cursor: u64,
    /// The store head when this process booted: a `RUN_EVERYTHING` recorded
    /// at or before it belongs to an earlier process and is ignored.
    boot: u64,
    modes: HashMap<TaskId, ModeFact>,
    rules: BTreeMap<String, RuleRow>,
}

/// The run modes and allowlist rules the Core holds, folded from the log.
#[derive(Default)]
pub struct RunPolicies {
    inner: std::sync::Mutex<Fold>,
}

const POLICY_TYPES: &[&str] = &["RunModeSet", "AllowRuleAdded", "AllowRuleRevoked"];

impl RunPolicies {
    /// Record the store head at boot: from here a `RUN_EVERYTHING` is live.
    pub(crate) fn mark_boot(&self, head: u64) {
        let mut f = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        f.boot = head;
    }

    /// Fold what is new on the log.
    fn refresh<'a>(
        &'a self,
        store: &EventStore,
    ) -> Result<std::sync::MutexGuard<'a, Fold>, String> {
        let mut f = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        let head = store.last_offset().map_err(|e| e.to_string())?;
        if head <= f.cursor {
            return Ok(f);
        }
        loop {
            let events = store
                .read_all_of_types(POLICY_TYPES, f.cursor, 500)
                .map_err(|e| format!("reading the policy records: {e}"))?;
            let full = events.len() >= 500;
            for e in &events {
                f.cursor = f.cursor.max(e.offset);
                let Ok(payload) = store.payload(&e.envelope) else {
                    continue;
                };
                let Ok(ev) = serde_json::from_value::<TaskEvent>(payload) else {
                    continue;
                };
                match ev {
                    TaskEvent::RunModeSet {
                        mode, acknowledged, ..
                    } => {
                        let Some(task) = e.envelope.task_id else {
                            continue;
                        };
                        // Run everything is per process: an earlier
                        // process's is gone, and the mode before it stands.
                        if !mode.durable() && e.offset <= f.boot {
                            continue;
                        }
                        f.modes.insert(
                            task,
                            ModeFact {
                                mode,
                                acknowledged,
                                offset: e.offset,
                            },
                        );
                    }
                    TaskEvent::AllowRuleAdded { rule } => {
                        f.rules.insert(
                            rule.rule_id.clone(),
                            RuleRow {
                                rule,
                                offset: e.offset,
                                revoked_by: None,
                            },
                        );
                    }
                    TaskEvent::AllowRuleRevoked {
                        rule_id,
                        revoked_by,
                        ..
                    } => {
                        if let Some(r) = f.rules.get_mut(&rule_id) {
                            r.revoked_by.get_or_insert(revoked_by);
                        }
                    }
                    _ => {}
                }
            }
            if !full {
                f.cursor = f.cursor.max(head);
                return Ok(f);
            }
        }
    }

    /// The run mode in force for `task`, whether its setting carried the
    /// acknowledgement, and the log offset of the record that set it.
    pub(crate) fn mode_of(
        &self,
        store: &EventStore,
        task: TaskId,
    ) -> Result<(RunMode, bool, u64), String> {
        let f = self.refresh(store)?;
        Ok(f.modes.get(&task).map_or((RunMode::Ask, false, 0), |m| {
            (m.mode, m.acknowledged, m.offset)
        }))
    }

    /// Every rule that has not been revoked.
    pub(crate) fn live_rules(&self, store: &EventStore) -> Result<Vec<AllowRule>, String> {
        let f = self.refresh(store)?;
        Ok(f.rules
            .values()
            .filter(|r| r.revoked_by.is_none())
            .map(|r| r.rule.clone())
            .collect())
    }

    /// Every rule, with its state.
    pub(crate) fn all_rules(&self, store: &EventStore) -> Result<Vec<RuleRow>, String> {
        let f = self.refresh(store)?;
        Ok(f.rules.values().cloned().collect())
    }
}

/// The task's queue, folded from its log. The caller holds the store.
pub(crate) fn queue_of(store: &EventStore, task: TaskId) -> Result<InputQueue, String> {
    let events = store
        .read_aggregate_of_types(task.as_bytes(), input_queue::EVENT_TYPES, 0)
        .map_err(|e| format!("reading the task's queue: {e}"))?;
    let mut q = InputQueue::new();
    for e in &events {
        let payload = store
            .payload(&e.envelope)
            .map_err(|e| format!("reading a queue record: {e}"))?;
        if let Ok(ev) = serde_json::from_value::<TaskEvent>(payload) {
            q.apply(e.offset, &ev);
        }
    }
    Ok(q)
}

// ---- the policy the kernel reads ----

/// What the Capability Kernel is given about the task for every call it
/// decides: the mode in force and the rules. The call's own facts (its argv,
/// its always-ask classes, whether it runs contained) are added per call.
#[derive(Clone, Debug, Default)]
pub(crate) struct RunContext {
    pub mode: RunMode,
    pub rules: Vec<AllowRule>,
    pub task_id: String,
    pub repo_root: String,
}

/// The run context of `task_id`. The caller holds the store. With the mode
/// `ASK` no rule is consulted, so none is loaded.
pub(crate) fn context_for(
    policies: &RunPolicies,
    store: &EventStore,
    task_id: TaskId,
    workspace_root: Option<&str>,
) -> Result<RunContext, String> {
    let (mode, _, _) = policies.mode_of(store, task_id)?;
    let rules = if mode == RunMode::Ask {
        Vec::new()
    } else {
        policies.live_rules(store)?
    };
    Ok(RunContext {
        mode,
        rules,
        task_id: task_id.to_string(),
        repo_root: workspace_root.unwrap_or_default().to_owned(),
    })
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

fn nack(cid: Option<wire::Id>, (code, message): Refusal) -> wire::CommandAck {
    reject(cid, &code, message)
}

fn task_id_of(id: Option<&wire::Id>) -> Result<TaskId, Refusal> {
    id.and_then(id16)
        .map(TaskId::from_bytes)
        .ok_or_else(|| refuse("BAD_PAYLOAD", "task_id required"))
}

/// The task a command names, refused when unknown or over.
async fn open_task(core: &Core, task_id: TaskId) -> Result<Task, Refusal> {
    let store = core.store.lock().await;
    match store.task(&task_id) {
        Ok(Some(t)) if t.state.is_terminal() => Err(refuse(
            "TASK_TERMINAL",
            format!("the task is {:?}; it takes no further control", t.state),
        )),
        Ok(Some(t)) => Ok(t),
        Ok(None) => Err(refuse("UNKNOWN_TASK", task_id.to_string())),
        Err(e) => Err(refuse(crate::server::error_code(&e), e.to_string())),
    }
}

fn mode_word(m: InputMode) -> &'static str {
    match m {
        InputMode::Steer => "STEER",
        InputMode::Collect => "COLLECT",
        InputMode::FollowUp => "FOLLOW_UP",
    }
}

fn parse_mode(s: &str) -> Result<InputMode, Refusal> {
    match s {
        "STEER" => Ok(InputMode::Steer),
        "COLLECT" => Ok(InputMode::Collect),
        "FOLLOW_UP" => Ok(InputMode::FollowUp),
        other => Err(refuse(
            "BAD_PAYLOAD",
            format!("unknown input mode `{other}`; use STEER, COLLECT or FOLLOW_UP"),
        )),
    }
}

fn item_view(i: &QueueItem, position: u32) -> wire::QueuedInputView {
    wire::QueuedInputView {
        input_id: i.input_id.clone(),
        position,
        mode: mode_word(i.mode).into(),
        text: i.text.clone(),
        state: i.state.name().into(),
        model: i.model.clone(),
        edited: i.edited,
        sent_now: i.sent_now,
        provenance: i.provenance.clone(),
        untrusted: i.untrusted,
        queued_offset: i.queued_offset,
        changed_offset: i.changed_offset,
    }
}

fn position_of(q: &InputQueue, id: &str) -> u32 {
    q.pending()
        .iter()
        .position(|i| i.input_id == id)
        .map_or(0, |p| u32::try_from(p + 1).unwrap_or(u32::MAX))
}

/// Every command of this module.
pub(crate) async fn handle(core: &Arc<Core>, env: &wire::CommandEnvelope) -> wire::CommandAck {
    let cid = env.command_id.clone();
    let Some(command_id) = env.command_id.as_ref().and_then(id16) else {
        return nack(cid, refuse("BAD_COMMAND_ID", "command_id must be 16 bytes"));
    };
    macro_rules! decode {
        ($ty:ty) => {
            match <$ty>::decode(env.payload.as_slice()) {
                Ok(p) => p,
                Err(_) => {
                    return nack(cid, refuse("BAD_PAYLOAD", stringify!($ty)));
                }
            }
        };
    }
    let r = match env.command_type.as_str() {
        "ListQueuedInputs" => list_queue(core, decode!(wire::ListQueuedInputs)).await,
        "EditQueuedInput" => {
            edit_input(core, env, command_id, decode!(wire::EditQueuedInput)).await
        }
        "RemoveQueuedInput" => {
            remove_input(core, env, command_id, decode!(wire::RemoveQueuedInput)).await
        }
        "ReorderQueuedInput" => {
            reorder_input(core, env, command_id, decode!(wire::ReorderQueuedInput)).await
        }
        "SendQueuedInputNow" => {
            send_now(core, env, command_id, decode!(wire::SendQueuedInputNow)).await
        }
        "SetSendBehavior" => {
            set_send_behavior(core, env, command_id, decode!(wire::SetSendBehavior)).await
        }
        "GetSendBehavior" => get_send_behavior(core, decode!(wire::GetSendBehavior)).await,
        "InterruptTask" => stop_task(core, env, command_id, decode!(wire::InterruptTask)).await,
        "SetRunMode" => set_run_mode(core, env, command_id, decode!(wire::SetRunMode)).await,
        "GetRunMode" => get_run_mode(core, decode!(wire::GetRunMode)).await,
        "AddAllowRule" => add_rule(core, env, command_id, decode!(wire::AddAllowRule)).await,
        "RevokeAllowRule" => {
            revoke_rule(core, env, command_id, decode!(wire::RevokeAllowRule)).await
        }
        "ListAllowRules" => list_rules(core, decode!(wire::ListAllowRules)).await,
        other => Err(refuse("UNKNOWN_COMMAND", other)),
    };
    match r {
        Ok((replayed, bytes)) => accept(cid, replayed, bytes),
        Err(r) => nack(cid, r),
    }
}

type Done = Result<(bool, Vec<u8>), Refusal>;

/// Append one event to the task's log under the command record.
fn append_one(
    core: &Core,
    store: &mut EventStore,
    env: &wire::CommandEnvelope,
    command_id: [u8; 16],
    task: &Task,
    event_type: &str,
    event: &TaskEvent,
) -> Result<(u64, u64, bool), Refusal> {
    let req = AppendRequest {
        tenant_id: core.tenant_id,
        session_id: task.session_id,
        task_id: Some(task.task_id),
        run_id: None,
        turn_id: None,
        step_id: None,
        aggregate_type: AggregateType::Task,
        aggregate_id: *task.task_id.as_bytes(),
        expected_sequence: None,
        events: vec![typed(event_type, event, Actor::User(core.user_id))],
    };
    match store.execute_command(record_of(core, env, command_id), req) {
        Ok(outcome) => {
            let (events, replayed) = match outcome {
                CommandOutcome::Applied(e) => (e, false),
                CommandOutcome::Replayed(e) => (e, true),
            };
            let last = events.last();
            let (offset, sequence) = last.map_or((0, 0), |e| (e.offset, e.envelope.sequence));
            if !replayed && offset > 0 {
                core.last_offset.send_replace(offset);
            }
            Ok((offset, sequence, replayed))
        }
        Err(e) => Err(refuse(crate::server::error_code(&e), e.to_string())),
    }
}

async fn list_queue(core: &Arc<Core>, p: wire::ListQueuedInputs) -> Done {
    let task_id = task_id_of(p.task_id.as_ref())?;
    let (queue, offset) = {
        let store = core.store.lock().await;
        match store.task(&task_id) {
            Ok(Some(_)) => {}
            Ok(None) => return Err(refuse("UNKNOWN_TASK", task_id.to_string())),
            Err(e) => return Err(refuse(crate::server::error_code(&e), e.to_string())),
        }
        (
            queue_of(&store, task_id).map_err(|e| refuse("STORE_ERROR", e))?,
            store.last_offset().unwrap_or(0),
        )
    };
    let pending = queue.pending();
    let mut items: Vec<wire::QueuedInputView> = pending
        .iter()
        .enumerate()
        .map(|(n, i)| item_view(i, u32::try_from(n + 1).unwrap_or(u32::MAX)))
        .collect();
    if p.include_settled {
        items.extend(
            queue
                .all()
                .into_iter()
                .filter(|i| i.state != ItemState::Queued)
                .map(|i| item_view(i, 0)),
        );
    }
    Ok((
        false,
        wire::QueuedInputList {
            task_id: Some(wire_id(task_id.as_bytes())),
            items,
            queued: u32::try_from(pending.len()).unwrap_or(u32::MAX),
            run_alive: core.runtime.is_running(&task_id).await,
            offset,
        }
        .encode_to_vec(),
    ))
}

/// Check the input is a queued one the person may change.
fn queued_item<'q>(q: &'q InputQueue, id: &str) -> Result<&'q QueueItem, Refusal> {
    let Some(i) = q.get(id) else {
        return Err(refuse(
            "UNKNOWN_INPUT",
            format!("no input `{id}` on this task"),
        ));
    };
    if i.state != ItemState::Queued {
        return Err(refuse(
            "INPUT_NOT_QUEUED",
            format!(
                "input `{id}` is {}; only a queued input can be changed",
                i.state.name()
            ),
        ));
    }
    Ok(i)
}

fn changed(task_id: TaskId, q: &InputQueue, id: &str, sequence: u64, offset: u64) -> Vec<u8> {
    wire::QueuedInputChanged {
        task_id: Some(wire_id(task_id.as_bytes())),
        input_id: id.to_owned(),
        state: q.get(id).map_or("", |i| i.state.name()).to_owned(),
        position: position_of(q, id),
        sequence,
        offset,
    }
    .encode_to_vec()
}

async fn edit_input(
    core: &Arc<Core>,
    env: &wire::CommandEnvelope,
    command_id: [u8; 16],
    p: wire::EditQueuedInput,
) -> Done {
    let task_id = task_id_of(p.task_id.as_ref())?;
    let mode = if p.mode.is_empty() {
        None
    } else {
        Some(parse_mode(&p.mode)?)
    };
    if p.text.is_empty() && mode.is_none() && p.model.is_empty() {
        return Err(refuse(
            "EMPTY_EDIT",
            "the command changes nothing: give a text, a mode or a model",
        ));
    }
    if !p.text.is_empty() && p.text.trim().is_empty() {
        return Err(refuse("BAD_PAYLOAD", "text must not be blank"));
    }
    let task = open_task(core, task_id).await?;
    require_lease(core, &env.command_id, env, &task.session_id)
        .await
        .map_err(|ack| (ack.error_code, ack.error_message))?;
    let mut store = core.store.lock().await;
    let before = queue_of(&store, task_id).map_err(|e| refuse("STORE_ERROR", e))?;
    if store
        .prior_command(&record_of(core, env, command_id))
        .map_err(|e| refuse("STORE_ERROR", e.to_string()))?
        .is_none()
    {
        let item = queued_item(&before, &p.input_id)?;
        if item.untrusted {
            return Err(refuse(
                "UNTRUSTED_INPUT",
                "the input is external content queued on someone else's behalf; delete or reorder it, but it is not rewritten as yours",
            ));
        }
    }
    let (offset, sequence, replayed) = append_one(
        core,
        &mut store,
        env,
        command_id,
        &task,
        "TaskInputEdited",
        &TaskEvent::TaskInputEdited {
            input_id: p.input_id.clone(),
            text: p.text,
            mode,
            model: p.model.chars().take(MAX_NOTE).collect(),
        },
    )?;
    let after = queue_of(&store, task_id).map_err(|e| refuse("STORE_ERROR", e))?;
    Ok((
        replayed,
        changed(task_id, &after, &p.input_id, sequence, offset),
    ))
}

async fn remove_input(
    core: &Arc<Core>,
    env: &wire::CommandEnvelope,
    command_id: [u8; 16],
    p: wire::RemoveQueuedInput,
) -> Done {
    let task_id = task_id_of(p.task_id.as_ref())?;
    let task = open_task(core, task_id).await?;
    require_lease(core, &env.command_id, env, &task.session_id)
        .await
        .map_err(|ack| (ack.error_code, ack.error_message))?;
    let mut store = core.store.lock().await;
    let before = queue_of(&store, task_id).map_err(|e| refuse("STORE_ERROR", e))?;
    if store
        .prior_command(&record_of(core, env, command_id))
        .map_err(|e| refuse("STORE_ERROR", e.to_string()))?
        .is_none()
    {
        queued_item(&before, &p.input_id)?;
    }
    let (offset, sequence, replayed) = append_one(
        core,
        &mut store,
        env,
        command_id,
        &task,
        "TaskInputRemoved",
        &TaskEvent::TaskInputRemoved {
            input_id: p.input_id.clone(),
            reason: p.reason.chars().take(MAX_NOTE).collect(),
        },
    )?;
    let after = queue_of(&store, task_id).map_err(|e| refuse("STORE_ERROR", e))?;
    Ok((
        replayed,
        changed(task_id, &after, &p.input_id, sequence, offset),
    ))
}

async fn reorder_input(
    core: &Arc<Core>,
    env: &wire::CommandEnvelope,
    command_id: [u8; 16],
    p: wire::ReorderQueuedInput,
) -> Done {
    let task_id = task_id_of(p.task_id.as_ref())?;
    let task = open_task(core, task_id).await?;
    require_lease(core, &env.command_id, env, &task.session_id)
        .await
        .map_err(|ack| (ack.error_code, ack.error_message))?;
    let mut store = core.store.lock().await;
    let before = queue_of(&store, task_id).map_err(|e| refuse("STORE_ERROR", e))?;
    if store
        .prior_command(&record_of(core, env, command_id))
        .map_err(|e| refuse("STORE_ERROR", e.to_string()))?
        .is_none()
    {
        queued_item(&before, &p.input_id)?;
        if !p.before_input_id.is_empty() {
            if p.before_input_id == p.input_id {
                return Err(refuse(
                    "BAD_PAYLOAD",
                    "an input cannot be moved before itself",
                ));
            }
            queued_item(&before, &p.before_input_id).map_err(|(code, why)| {
                (
                    if code == "UNKNOWN_INPUT" {
                        code
                    } else {
                        "BEFORE_NOT_QUEUED".to_owned()
                    },
                    why,
                )
            })?;
        }
    }
    let (offset, sequence, replayed) = append_one(
        core,
        &mut store,
        env,
        command_id,
        &task,
        "TaskInputReordered",
        &TaskEvent::TaskInputReordered {
            input_id: p.input_id.clone(),
            before_input_id: p.before_input_id,
        },
    )?;
    let after = queue_of(&store, task_id).map_err(|e| refuse("STORE_ERROR", e))?;
    Ok((
        replayed,
        changed(task_id, &after, &p.input_id, sequence, offset),
    ))
}

fn interrupt_id_of(given: &str, command_id: [u8; 16]) -> String {
    if given.is_empty() {
        modbit_domain::EventId::from_bytes(command_id).to_string()
    } else {
        given.chars().take(128).collect()
    }
}

async fn send_now(
    core: &Arc<Core>,
    env: &wire::CommandEnvelope,
    command_id: [u8; 16],
    p: wire::SendQueuedInputNow,
) -> Done {
    let task_id = task_id_of(p.task_id.as_ref())?;
    let task = open_task(core, task_id).await?;
    require_lease(core, &env.command_id, env, &task.session_id)
        .await
        .map_err(|ack| (ack.error_code, ack.error_message))?;
    let interrupt_id = interrupt_id_of(&p.interrupt_id, command_id);
    let (offset, sequence, replayed) = {
        let mut store = core.store.lock().await;
        let before = queue_of(&store, task_id).map_err(|e| refuse("STORE_ERROR", e))?;
        // The same interrupt asked for again — under another command id —
        // is the interrupt that was asked for: it acts once.
        if let Some(prior) = before.interrupt(&interrupt_id) {
            let offset = prior.requested_offset;
            return Ok((
                true,
                accepted(task_id, &interrupt_id, "SEND_NOW", false, 0, offset),
            ));
        }
        let replay = store
            .prior_command(&record_of(core, env, command_id))
            .map_err(|e| refuse("STORE_ERROR", e.to_string()))?
            .is_some();
        if !replay {
            queued_item(&before, &p.input_id)?;
        }
        // The task's setting says what Send now does (AFW-E05). `STEER` is the
        // input's own mode: it replaces the response at the next safe
        // boundary and leaves a tool call in flight to finish; it is never
        // labelled as sending without stopping the agent.
        if before.behavior().send_now == "STEER" && !replay {
            let (offset, sequence, replayed) = append_one(
                core,
                &mut store,
                env,
                command_id,
                &task,
                "TaskInputEdited",
                &TaskEvent::TaskInputEdited {
                    input_id: p.input_id.clone(),
                    text: String::new(),
                    mode: Some(InputMode::Steer),
                    model: String::new(),
                },
            )?;
            drop(store);
            return Ok((
                replayed,
                accepted(
                    task_id,
                    &interrupt_id,
                    "STEER",
                    core.runtime.is_running(&task_id).await,
                    sequence,
                    offset,
                ),
            ));
        }
        append_one(
            core,
            &mut store,
            env,
            command_id,
            &task,
            "TaskInterruptRequested",
            &TaskEvent::TaskInterruptRequested {
                interrupt_id: interrupt_id.clone(),
                kind: "SEND_NOW".into(),
                input_id: p.input_id.clone(),
                reason: p.reason.chars().take(MAX_NOTE).collect(),
            },
        )?
    };
    // A replay of the command acts once: the first one told the loop.
    let alive = if replayed {
        core.runtime.is_running(&task_id).await
    } else {
        core.runtime.interrupt(&task_id).await
    };
    Ok((
        replayed,
        accepted(task_id, &interrupt_id, "SEND_NOW", alive, sequence, offset),
    ))
}

fn accepted(
    task_id: TaskId,
    interrupt_id: &str,
    kind: &str,
    alive: bool,
    sequence: u64,
    offset: u64,
) -> Vec<u8> {
    wire::InterruptAccepted {
        task_id: Some(wire_id(task_id.as_bytes())),
        interrupt_id: interrupt_id.to_owned(),
        kind: kind.to_owned(),
        run_alive: alive,
        sequence,
        offset,
    }
    .encode_to_vec()
}

async fn stop_task(
    core: &Arc<Core>,
    env: &wire::CommandEnvelope,
    command_id: [u8; 16],
    p: wire::InterruptTask,
) -> Done {
    let task_id = task_id_of(p.task_id.as_ref())?;
    let task = open_task(core, task_id).await?;
    require_lease(core, &env.command_id, env, &task.session_id)
        .await
        .map_err(|ack| (ack.error_code, ack.error_message))?;
    let interrupt_id = interrupt_id_of(&p.interrupt_id, command_id);
    if !core.runtime.is_running(&task_id).await {
        return Err(refuse(
            "NOT_RUNNING",
            format!(
                "task {task_id} has no run executing ({:?}); there is nothing to stop",
                task.state
            ),
        ));
    }
    let (offset, sequence, replayed) = {
        let mut store = core.store.lock().await;
        let before = queue_of(&store, task_id).map_err(|e| refuse("STORE_ERROR", e))?;
        if let Some(prior) = before.interrupt(&interrupt_id) {
            let offset = prior.requested_offset;
            return Ok((
                true,
                accepted(task_id, &interrupt_id, "STOP", false, 0, offset),
            ));
        }
        append_one(
            core,
            &mut store,
            env,
            command_id,
            &task,
            "TaskInterruptRequested",
            &TaskEvent::TaskInterruptRequested {
                interrupt_id: interrupt_id.clone(),
                kind: "STOP".into(),
                input_id: String::new(),
                reason: p.reason.chars().take(MAX_NOTE).collect(),
            },
        )?
    };
    let alive = if replayed {
        core.runtime.is_running(&task_id).await
    } else {
        // The park is raised first, so the boundary the interrupt brings the
        // loop to is one it stops at.
        let asked = core
            .runtime
            .pause(
                &task_id,
                PauseRequest {
                    reason: if p.reason.is_empty() {
                        "stopped by the user".into()
                    } else {
                        p.reason.chars().take(MAX_NOTE).collect()
                    },
                    actor: Actor::User(core.user_id),
                    command_id: modbit_domain::EventId::from_bytes(command_id).to_string(),
                },
            )
            .await;
        asked && core.runtime.interrupt(&task_id).await
    };
    Ok((
        replayed,
        accepted(task_id, &interrupt_id, "STOP", alive, sequence, offset),
    ))
}

// ---- the send behavior (AFW-E05) ----

fn behavior_view(task_id: TaskId, q: &InputQueue) -> wire::SendBehaviorView {
    let b = q.behavior();
    let consequence = |v: &str| -> &'static str {
        match v {
            "QUEUE" => {
                "Your message waits and runs as its own turn when the current turn ends. The agent is not stopped."
            }
            "COLLECT" => {
                "Your messages are gathered and run as one turn when the current turn ends. The agent is not stopped."
            }
            "STEER" => {
                "Your message replaces the model's response at the next safe point and the agent continues from it. A tool call already running finishes first. This can interrupt the current model call."
            }
            "STOP_AND_SEND" => {
                "The agent is stopped at once: the model stream is cut, a running command is cancelled, and your message starts the next turn."
            }
            "INTERRUPT" => {
                "Stops the agent now: the model stream is cut, a running command is cancelled, and the message starts the next turn."
            }
            _ => "",
        }
    };
    wire::SendBehaviorView {
        task_id: Some(wire_id(task_id.as_bytes())),
        while_running: b.while_running.clone(),
        send_now: b.send_now.clone(),
        while_running_values: input_queue::WHILE_RUNNING
            .iter()
            .map(|s| (*s).to_owned())
            .collect(),
        send_now_values: input_queue::SEND_NOW
            .iter()
            .map(|s| (*s).to_owned())
            .collect(),
        while_running_consequence: consequence(&b.while_running).into(),
        send_now_consequence: consequence(&b.send_now).into(),
        offset: q.behavior_offset(),
    }
}

async fn get_send_behavior(core: &Arc<Core>, p: wire::GetSendBehavior) -> Done {
    let task_id = task_of_id(p.task_id.as_ref())?;
    let store = core.store.lock().await;
    match store.task(&task_id) {
        Ok(Some(_)) => {}
        Ok(None) => return Err(refuse("UNKNOWN_TASK", task_id.to_string())),
        Err(e) => return Err(refuse(crate::server::error_code(&e), e.to_string())),
    }
    let q = queue_of(&store, task_id).map_err(|e| refuse("STORE_ERROR", e))?;
    Ok((false, behavior_view(task_id, &q).encode_to_vec()))
}

fn task_of_id(id: Option<&wire::Id>) -> Result<TaskId, Refusal> {
    task_id_of(id)
}

async fn set_send_behavior(
    core: &Arc<Core>,
    env: &wire::CommandEnvelope,
    command_id: [u8; 16],
    p: wire::SetSendBehavior,
) -> Done {
    let task_id = task_id_of(p.task_id.as_ref())?;
    if !p.while_running.is_empty()
        && !input_queue::WHILE_RUNNING.contains(&p.while_running.as_str())
    {
        return Err(refuse(
            "BAD_PAYLOAD",
            format!(
                "while_running `{}` is not one of {}",
                p.while_running,
                input_queue::WHILE_RUNNING.join(", ")
            ),
        ));
    }
    if !p.send_now.is_empty() && !input_queue::SEND_NOW.contains(&p.send_now.as_str()) {
        return Err(refuse(
            "BAD_PAYLOAD",
            format!(
                "send_now `{}` is not one of {}",
                p.send_now,
                input_queue::SEND_NOW.join(", ")
            ),
        ));
    }
    if p.while_running.is_empty() && p.send_now.is_empty() {
        return Err(refuse("EMPTY_EDIT", "the command sets nothing"));
    }
    let task = open_task(core, task_id).await?;
    require_lease(core, &env.command_id, env, &task.session_id)
        .await
        .map_err(|ack| (ack.error_code, ack.error_message))?;
    let mut store = core.store.lock().await;
    let (_, _, replayed) = append_one(
        core,
        &mut store,
        env,
        command_id,
        &task,
        "SendBehaviorSet",
        &TaskEvent::SendBehaviorSet {
            while_running: p.while_running,
            send_now: p.send_now,
        },
    )?;
    let q = queue_of(&store, task_id).map_err(|e| refuse("STORE_ERROR", e))?;
    Ok((replayed, behavior_view(task_id, &q).encode_to_vec()))
}

/// What `QueueInput` with the mode `DEFAULT` becomes: the mode the task's
/// setting names for an input sent while a turn runs, and whether the setting
/// is stop-and-send (the input then also interrupts, once it is queued).
pub(crate) async fn resolve_default(
    core: &Core,
    task_id: TaskId,
) -> Result<(InputMode, bool), String> {
    let store = core.store.lock().await;
    let q = queue_of(&store, task_id)?;
    Ok(match q.behavior().while_running.as_str() {
        "COLLECT" => (InputMode::Collect, false),
        "STEER" => (InputMode::Steer, false),
        "STOP_AND_SEND" => (InputMode::FollowUp, true),
        _ => (InputMode::FollowUp, false),
    })
}

/// The stop-and-send of an input just queued: the typed interrupt-and-replace
/// the person would have asked for with Send now, recorded and fired.
pub(crate) async fn request_send_now(core: &Core, task: &Task, input_id: &str) {
    let interrupt_id = format!("stop-and-send:{input_id}");
    {
        let mut store = core.store.lock().await;
        if queue_of(&store, task.task_id)
            .ok()
            .is_some_and(|q| q.interrupt(&interrupt_id).is_some())
        {
            return;
        }
        let req = AppendRequest {
            tenant_id: core.tenant_id,
            session_id: task.session_id,
            task_id: Some(task.task_id),
            run_id: None,
            turn_id: None,
            step_id: None,
            aggregate_type: AggregateType::Task,
            aggregate_id: *task.task_id.as_bytes(),
            expected_sequence: None,
            events: vec![typed(
                "TaskInterruptRequested",
                &TaskEvent::TaskInterruptRequested {
                    interrupt_id,
                    kind: "SEND_NOW".into(),
                    input_id: input_id.to_owned(),
                    reason: "stop-and-send".into(),
                },
                Actor::User(core.user_id),
            )],
        };
        match store.append(req) {
            Ok(events) => {
                if let Some(e) = events.last() {
                    core.last_offset.send_replace(e.offset);
                }
            }
            Err(_) => return,
        }
    }
    core.runtime.interrupt(&task.task_id).await;
}

// ---- run modes ----

fn run_mode_view(
    core: &Core,
    store: &EventStore,
    task_id: TaskId,
) -> Result<wire::RunModeView, Refusal> {
    let (mode, acknowledged, offset) = core
        .tools
        .run_policies
        .mode_of(store, task_id)
        .map_err(|e| refuse("STORE_ERROR", e))?;
    let rules = core
        .tools
        .run_policies
        .live_rules(store)
        .map_err(|e| refuse("STORE_ERROR", e))?;
    let task = store.task(&task_id).ok().flatten();
    let policy = modbit_policy::RunPolicy {
        mode,
        task_id: task_id.to_string(),
        repo_root: task
            .as_ref()
            .and_then(|t| t.workspace_root.clone())
            .unwrap_or_default(),
        rules,
        ..Default::default()
    };
    let now = Timestamp::now().0;
    Ok(wire::RunModeView {
        task_id: Some(wire_id(task_id.as_bytes())),
        mode: mode.name().into(),
        acknowledged,
        always_ask: ALWAYS_ASK.iter().map(|c| c.name().to_owned()).collect(),
        modes: RunMode::ALL.iter().map(|m| m.name().to_owned()).collect(),
        warning: RISK_WARNING.into(),
        session_only: !mode.durable(),
        offset,
        rules_in_force: u32::try_from(policy.live_rules(now).len()).unwrap_or(u32::MAX),
    })
}

/// The classes that ask in every mode.
pub(crate) const ALWAYS_ASK: [AskClass; 7] = [
    AskClass::OutsideWorkspaceWrite,
    AskClass::Network,
    AskClass::Secret,
    AskClass::ProtectedPath,
    AskClass::Deletion,
    AskClass::Push,
    AskClass::Escalation,
];

async fn get_run_mode(core: &Arc<Core>, p: wire::GetRunMode) -> Done {
    let task_id = task_id_of(p.task_id.as_ref())?;
    let store = core.store.lock().await;
    match store.task(&task_id) {
        Ok(Some(_)) => {}
        Ok(None) => return Err(refuse("UNKNOWN_TASK", task_id.to_string())),
        Err(e) => return Err(refuse(crate::server::error_code(&e), e.to_string())),
    }
    Ok((false, run_mode_view(core, &store, task_id)?.encode_to_vec()))
}

async fn set_run_mode(
    core: &Arc<Core>,
    env: &wire::CommandEnvelope,
    command_id: [u8; 16],
    p: wire::SetRunMode,
) -> Done {
    let task_id = task_id_of(p.task_id.as_ref())?;
    let Some(mode) = RunMode::parse(&p.mode) else {
        return Err(refuse(
            "UNKNOWN_RUN_MODE",
            format!(
                "`{}` is not a run mode; use ASK, ALLOWLIST, ALLOWLIST_SANDBOX or RUN_EVERYTHING",
                p.mode
            ),
        ));
    };
    let task = open_task(core, task_id).await?;
    if task.execution_profile == modbit_policy::kernel::PROFILE_LOCAL_AUTONOMOUS {
        return Err(refuse(
            "UNATTENDED_PROFILE",
            "a task that runs unattended cannot ask a person, so who approves its effects is not a choice",
        ));
    }
    require_lease(core, &env.command_id, env, &task.session_id)
        .await
        .map_err(|ack| (ack.error_code, ack.error_message))?;
    let mut store = core.store.lock().await;
    let (previous, _, _) = core
        .tools
        .run_policies
        .mode_of(&store, task_id)
        .map_err(|e| refuse("STORE_ERROR", e))?;
    // A move to a mode that approves more — and RUN_EVERYTHING every time it
    // is set — carries the person's acknowledgement (AFW-F07).
    let approves_more = mode.rank() > previous.rank() || mode == RunMode::RunEverything;
    if approves_more
        && !p.acknowledge_risk
        && store
            .prior_command(&record_of(core, env, command_id))
            .map_err(|e| refuse("STORE_ERROR", e.to_string()))?
            .is_none()
    {
        return Err(refuse("ACKNOWLEDGEMENT_REQUIRED", RISK_WARNING));
    }
    append_one(
        core,
        &mut store,
        env,
        command_id,
        &task,
        "RunModeSet",
        &TaskEvent::RunModeSet {
            mode,
            previous: Some(previous),
            acknowledged: approves_more && p.acknowledge_risk,
            warning: if approves_more {
                RISK_WARNING.into()
            } else {
                String::new()
            },
        },
    )?;
    Ok((false, run_mode_view(core, &store, task_id)?.encode_to_vec()))
}

// ---- allowlist rules ----

fn rule_view(row: &RuleRow, now: i64) -> wire::AllowRuleView {
    let state = if row.revoked_by.is_some() {
        "REVOKED"
    } else if row.rule.expires_at_ms.is_some_and(|e| e <= now) {
        "EXPIRED"
    } else {
        "ACTIVE"
    };
    wire::AllowRuleView {
        rule_id: row.rule.rule_id.clone(),
        pattern: row.rule.pattern.clone(),
        scope: row.rule.scope.name().into(),
        scope_key: row.rule.scope_key.clone(),
        created_by: row.rule.created_by.clone(),
        created_at_ms: row.rule.created_at_ms,
        expires_at_ms: row.rule.expires_at_ms.unwrap_or(0),
        covers_always_ask: row.rule.covers_always_ask,
        state: state.into(),
        revoked_by: row.revoked_by.clone().unwrap_or_default(),
        origin_task: row.rule.origin_task.clone(),
        offset: row.offset,
    }
}

/// Whether a rule applies to `task`.
fn applies_to(rule: &AllowRule, task: &Task) -> bool {
    match rule.scope {
        RuleScope::Task => rule.scope_key == task.task_id.to_string(),
        RuleScope::Repo => task.workspace_root.as_deref().is_some_and(|r| {
            std::path::Path::new(r.trim_end_matches(['/', '\\']))
                == std::path::Path::new(rule.scope_key.trim_end_matches(['/', '\\']))
        }),
        RuleScope::User => true,
    }
}

async fn add_rule(
    core: &Arc<Core>,
    env: &wire::CommandEnvelope,
    command_id: [u8; 16],
    p: wire::AddAllowRule,
) -> Done {
    let task_id = task_id_of(p.task_id.as_ref())?;
    let Some(scope) = RuleScope::parse(&p.scope) else {
        return Err(refuse(
            "BAD_PAYLOAD",
            format!("unknown scope `{}`; use TASK, REPO or USER", p.scope),
        ));
    };
    let task = open_task(core, task_id).await?;
    // A rule is a person's decision about effects the agent will attempt: it
    // is not made for a task that cannot ask, nor from a background agent.
    if task.execution_profile == modbit_policy::kernel::PROFILE_LOCAL_AUTONOMOUS
        || task.origin == TaskOrigin::Subagent
    {
        return Err(refuse(
            "RULE_NOT_ALLOWED",
            "allowlist rules are made by a person for a task they attend; an unattended task and a subagent have none to make",
        ));
    }
    let now = Timestamp::now().0;
    let expires = (p.expires_at_ms != 0).then_some(p.expires_at_ms);
    if expires.is_some_and(|e| e <= now) {
        return Err(refuse(
            "EXPIRY_IN_PAST",
            "the rule would already be expired",
        ));
    }
    let scope_key = match scope {
        RuleScope::Task => task_id.to_string(),
        RuleScope::Repo => match task.workspace_root.clone() {
            Some(r) => r,
            None => {
                return Err(refuse(
                    "NO_WORKSPACE",
                    "a repository rule needs the task's workspace root",
                ));
            }
        },
        RuleScope::User => String::new(),
    };
    let rule_id = if p.rule_id.is_empty() {
        modbit_domain::EventId::from_bytes(command_id).to_string()
    } else {
        p.rule_id.chars().take(128).collect()
    };
    let rule = AllowRule {
        rule_id: rule_id.clone(),
        pattern: p.pattern,
        scope,
        scope_key,
        created_by: format!("user:{}", core.user_id),
        created_at_ms: now,
        expires_at_ms: expires,
        covers_always_ask: p.covers_always_ask,
        origin_task: task_id.to_string(),
    };
    modbit_policy::runmode::validate_rule(&rule).map_err(|r| refuse(r.code, r.reason))?;
    require_lease(core, &env.command_id, env, &task.session_id)
        .await
        .map_err(|ack| (ack.error_code, ack.error_message))?;
    let mut store = core.store.lock().await;
    let replay = store
        .prior_command(&record_of(core, env, command_id))
        .map_err(|e| refuse("STORE_ERROR", e.to_string()))?
        .is_some();
    if !replay {
        let existing = core
            .tools
            .run_policies
            .all_rules(&store)
            .map_err(|e| refuse("STORE_ERROR", e))?;
        if existing.iter().any(|r| r.rule.rule_id == rule_id) {
            return Err(refuse(
                "RULE_EXISTS",
                format!("a rule `{rule_id}` already exists"),
            ));
        }
        if existing.iter().filter(|r| r.revoked_by.is_none()).count() >= MAX_RULES {
            return Err(refuse(
                "TOO_MANY_RULES",
                format!("this Core holds {MAX_RULES} live rules; revoke some first"),
            ));
        }
    }
    let (offset, _, replayed) = append_one(
        core,
        &mut store,
        env,
        command_id,
        &task,
        "AllowRuleAdded",
        &TaskEvent::AllowRuleAdded { rule: rule.clone() },
    )?;
    let row = RuleRow {
        rule,
        offset,
        revoked_by: None,
    };
    Ok((replayed, rule_view(&row, now).encode_to_vec()))
}

async fn revoke_rule(
    core: &Arc<Core>,
    env: &wire::CommandEnvelope,
    command_id: [u8; 16],
    p: wire::RevokeAllowRule,
) -> Done {
    let task_id = task_id_of(p.task_id.as_ref())?;
    let task = open_task(core, task_id).await?;
    require_lease(core, &env.command_id, env, &task.session_id)
        .await
        .map_err(|ack| (ack.error_code, ack.error_message))?;
    let mut store = core.store.lock().await;
    let replay = store
        .prior_command(&record_of(core, env, command_id))
        .map_err(|e| refuse("STORE_ERROR", e.to_string()))?
        .is_some();
    if !replay {
        let rows = core
            .tools
            .run_policies
            .all_rules(&store)
            .map_err(|e| refuse("STORE_ERROR", e))?;
        let Some(row) = rows.iter().find(|r| r.rule.rule_id == p.rule_id) else {
            return Err(refuse("UNKNOWN_RULE", p.rule_id));
        };
        if row.revoked_by.is_some() {
            return Err(refuse("RULE_NOT_ACTIVE", "the rule is already revoked"));
        }
        if !applies_to(&row.rule, &task) {
            return Err(refuse(
                "RULE_OUT_OF_SCOPE",
                "the rule does not apply to the task named: revoke it from a task it applies to",
            ));
        }
    }
    let (offset, _, replayed) = append_one(
        core,
        &mut store,
        env,
        command_id,
        &task,
        "AllowRuleRevoked",
        &TaskEvent::AllowRuleRevoked {
            rule_id: p.rule_id.clone(),
            revoked_by: format!("user:{}", core.user_id),
            reason: p.reason.chars().take(MAX_NOTE).collect(),
        },
    )?;
    Ok((
        replayed,
        wire::AllowRuleRevoked {
            rule_id: p.rule_id,
            offset,
        }
        .encode_to_vec(),
    ))
}

async fn list_rules(core: &Arc<Core>, p: wire::ListAllowRules) -> Done {
    let task = match p.task_id.as_ref().and_then(id16).map(TaskId::from_bytes) {
        Some(id) => match core.store.lock().await.task(&id) {
            Ok(Some(t)) => Some(t),
            Ok(None) => return Err(refuse("UNKNOWN_TASK", id.to_string())),
            Err(e) => return Err(refuse(crate::server::error_code(&e), e.to_string())),
        },
        None => None,
    };
    let rows = {
        let store = core.store.lock().await;
        core.tools
            .run_policies
            .all_rules(&store)
            .map_err(|e| refuse("STORE_ERROR", e))?
    };
    let now = Timestamp::now().0;
    let rules = rows
        .iter()
        .filter(|r| task.as_ref().is_none_or(|t| applies_to(&r.rule, t)))
        .map(|r| rule_view(r, now))
        .filter(|v| p.include_inactive || v.state == "ACTIVE")
        .collect();
    Ok((false, wire::AllowRuleList { rules }.encode_to_vec()))
}
