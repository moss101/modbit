//! One-agent runtime (M2.7; docs/14 "Main runtime loop" steps 5–9 and the
//! harness contracts; docs/28 competence contracts): the event-driven loop
//! that compiles the prompt, streams the model through the Provider Gateway,
//! validates requested actions, runs them through the tool host (kernel,
//! approvals, receipts) and persists every step before the next begins.
//!
//! Durability: the transcript and harness state are rebuilt from the event
//! log (`RunStep` outputs are content-addressed transcript entries), so a
//! restarted Core resumes a task at a turn boundary without repeating any
//! tool action. Steering inputs are applied between steps; cancellation
//! interrupts at the same boundaries and cancels the in-flight model stream.

use std::collections::HashMap;
use std::sync::Arc;

use sha2::Digest;

use modbit_core_runtime::harness::{
    self, ASK_TOOL, Budgets, COMPLETE_TOOL, HarnessRefusal, HarnessState, PLAN_TOOL, Plan,
    REPAIR_TOOL, RepairEscalation, TOOL_SEARCH, VERIFY_TOOL, WRITE_TOOLS, scope_resolution,
    write_targets,
};
use modbit_domain::event::{Actor, AggregateType};
use modbit_domain::run::{OwnerLocation, RunEvent, RunState};
use modbit_domain::step::VerificationStage;
use modbit_domain::step::{StepEvent, StepType};
use modbit_domain::task::{InputMode, Task, TaskEvent, TaskState, WaitReason};
use modbit_domain::toolcall::ToolCallState;
use modbit_domain::turn::TurnEvent;
use modbit_domain::{RunId, RunStepId, SessionId, TaskId, TenantId, Timestamp, ToolCallId, TurnId};
use modbit_event_store::{AppendRequest, EventStore, NewEvent};
use modbit_providers::{
    ContentPart, Message, ModelEvent, ModelPolicy, Requirements, Role, ToolProjection,
};
use modbit_tools::ToolStatus;
use modbit_verification::{Class, InvariantContext, Stage, VerificationEngine, VerificationPolicy};
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;

use crate::server::Core;

/// Inline observation ceiling (docs/14 contract 2, docs/33 "inline size ceiling").
const OBSERVATION_CEILING_BYTES: usize = 16 * 1024;
/// Model stream timeout per invocation.
const MODEL_TIMEOUT_MS: u64 = 120_000;

/// How a task is run.
#[derive(Clone, Debug)]
pub struct StartConfig {
    /// Gateway endpoint name.
    pub endpoint: String,
    /// Model id.
    pub model: String,
    /// Budgets.
    pub budgets: Budgets,
}

/// Per-task control handle.
struct Running {
    cancel: CancellationToken,
}

/// Runtime state on the Core.
#[derive(Default)]
pub struct Runtime {
    tasks: Mutex<HashMap<TaskId, Running>>,
}

/// One transcript entry persisted as a step output (content-addressed).
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "entry", rename_all = "snake_case")]
enum TranscriptEntry {
    /// The model's message for one invocation.
    Assistant {
        text: String,
        tool_calls: Vec<(String, String, String)>, // (call_id, name, arguments_json)
    },
    /// What the model saw for one requested call.
    ToolResult {
        call_id: String,
        name: String,
        text: String,
        failure_signature: Option<String>,
        clears: Vec<String>,
        wrote: Option<String>,
        progress: bool,
    },
    /// A steering / follow-up input injected between steps.
    User { text: String },
}

/// Outcome of the loop.
enum LoopEnd {
    ReadyForReview,
    /// A typed question is pending (REQ-EV-0222): the run suspends, never hangs.
    NeedsInput(String),
    /// The repair loop escalated (docs/28 §5): the task needs attention with its history.
    NeedsAttention(String),
    Cancelled,
    BudgetExhausted(harness::Exhausted),
    ProviderFailed(String, String),
    NoProgress(u32),
}

impl Runtime {
    /// Start (Queued) or resume (Waiting) a task under the session lease
    /// generation `lease_generation`. Returns the run id and whether it resumed.
    pub async fn start(
        &self,
        core: &Arc<Core>,
        task: Task,
        cfg: StartConfig,
        lease_generation: u64,
        actor: Actor,
    ) -> std::result::Result<(RunId, bool), (String, String)> {
        let mut tasks = self.tasks.lock().await;
        if tasks.contains_key(&task.task_id) {
            return Err(("TASK_ALREADY_RUNNING".into(), task.task_id.to_string()));
        }
        let (run_id, resumed) = {
            let mut store = core.store.lock().await;
            match task.state {
                TaskState::Queued => {
                    let run_id = RunId::new();
                    let attempt = store
                        .runs_for_task(&task.task_id)
                        .map(|r| r.len() as u32 + 1)
                        .unwrap_or(1);
                    append(
                        &mut store,
                        core,
                        Lineage::task(core.tenant_id, task.session_id, task.task_id),
                        AggregateType::Task,
                        *task.task_id.as_bytes(),
                        vec![typed("TaskStarted", &TaskEvent::TaskStarted, actor.clone())],
                    )
                    .map_err(|e| ("STORE".into(), e))?;
                    append(
                        &mut store,
                        core,
                        Lineage::run(core.tenant_id, task.session_id, task.task_id, run_id),
                        AggregateType::Run,
                        *run_id.as_bytes(),
                        vec![
                            typed(
                                "RunCreated",
                                &RunEvent::RunCreated {
                                    task_id: task.task_id,
                                    attempt,
                                    owner_location: OwnerLocation::Local,
                                    kernel_lease_generation: lease_generation,
                                },
                                actor.clone(),
                            ),
                            typed("RunStarted", &RunEvent::RunStarted, actor.clone()),
                        ],
                    )
                    .map_err(|e| ("STORE".into(), e))?;
                    (run_id, false)
                }
                TaskState::Waiting(_) => {
                    if let Some(q) = pending_question(&store, &task) {
                        return Err(("QUESTION_PENDING".into(), q));
                    }
                    let run = store
                        .runs_for_task(&task.task_id)
                        .ok()
                        .and_then(|r| r.into_iter().find(|r| r.state == RunState::Suspended));
                    let run = match run {
                        Some(r) => r,
                        None => {
                            // Returned from review (or never suspended): a fresh attempt.
                            let run_id = RunId::new();
                            let attempt = store
                                .runs_for_task(&task.task_id)
                                .map(|r| r.len() as u32 + 1)
                                .unwrap_or(1);
                            append(
                                &mut store,
                                core,
                                Lineage::task(core.tenant_id, task.session_id, task.task_id),
                                AggregateType::Task,
                                *task.task_id.as_bytes(),
                                vec![typed("TaskResumed", &TaskEvent::TaskResumed, actor.clone())],
                            )
                            .map_err(|e| ("STORE".into(), e))?;
                            append(
                                &mut store,
                                core,
                                Lineage::run(core.tenant_id, task.session_id, task.task_id, run_id),
                                AggregateType::Run,
                                *run_id.as_bytes(),
                                vec![
                                    typed(
                                        "RunCreated",
                                        &RunEvent::RunCreated {
                                            task_id: task.task_id,
                                            attempt,
                                            owner_location: OwnerLocation::Local,
                                            kernel_lease_generation: lease_generation,
                                        },
                                        actor.clone(),
                                    ),
                                    typed("RunStarted", &RunEvent::RunStarted, actor.clone()),
                                ],
                            )
                            .map_err(|e| ("STORE".into(), e))?;
                            drop(store);
                            let cancel = CancellationToken::new();
                            tasks.insert(
                                task.task_id,
                                Running {
                                    cancel: cancel.clone(),
                                },
                            );
                            let core2 = Arc::clone(core);
                            tokio::spawn(async move {
                                let task_id = task.task_id;
                                run_loop(core2.clone(), task, run_id, cfg, cancel).await;
                                core2.runtime.tasks.lock().await.remove(&task_id);
                            });
                            return Ok((run_id, true));
                        }
                    };
                    if lease_generation < run.kernel_lease_generation {
                        return Err((
                            "STALE_LEASE".into(),
                            format!(
                                "run is owned by lease generation {}",
                                run.kernel_lease_generation
                            ),
                        ));
                    }
                    append(
                        &mut store,
                        core,
                        Lineage::task(core.tenant_id, task.session_id, task.task_id),
                        AggregateType::Task,
                        *task.task_id.as_bytes(),
                        vec![typed("TaskResumed", &TaskEvent::TaskResumed, actor.clone())],
                    )
                    .map_err(|e| ("STORE".into(), e))?;
                    append(
                        &mut store,
                        core,
                        Lineage::run(core.tenant_id, task.session_id, task.task_id, run.run_id),
                        AggregateType::Run,
                        *run.run_id.as_bytes(),
                        vec![typed(
                            "RunResumed",
                            &RunEvent::RunResumed {
                                kernel_lease_generation: lease_generation,
                            },
                            actor.clone(),
                        )],
                    )
                    .map_err(|e| ("STORE".into(), e))?;
                    (run.run_id, true)
                }
                other => {
                    return Err((
                        "TASK_NOT_STARTABLE".into(),
                        format!("task is {other:?}; only Queued or Waiting tasks start"),
                    ));
                }
            }
        };
        let cancel = CancellationToken::new();
        tasks.insert(
            task.task_id,
            Running {
                cancel: cancel.clone(),
            },
        );
        let core2 = Arc::clone(core);
        tokio::spawn(async move {
            let task_id = task.task_id;
            run_loop(core2.clone(), task, run_id, cfg, cancel).await;
            core2.runtime.tasks.lock().await.remove(&task_id);
        });
        Ok((run_id, resumed))
    }

    /// Request cancellation; the loop stops at the next safe boundary.
    pub async fn cancel(&self, task_id: &TaskId) -> bool {
        match self.tasks.lock().await.get(task_id) {
            Some(r) => {
                r.cancel.cancel();
                true
            }
            None => false,
        }
    }

    /// Whether the loop for a task is alive.
    pub async fn is_running(&self, task_id: &TaskId) -> bool {
        self.tasks.lock().await.contains_key(task_id)
    }
}

/// Startup reconciliation (docs/14 "Compound execution and interruption"): a
/// task left `Running` by a previous process is suspended at a turn boundary
/// and marked for attention; nothing is re-executed.
pub fn reconcile_after_restart(store: &mut EventStore, core_tenant: TenantId) -> Vec<TaskId> {
    let actor = Actor::Core("recovery".into());
    let mut out = Vec::new();
    let Ok(tasks) = store.running_tasks() else {
        return out;
    };
    for t in tasks {
        if let Ok(runs) = store.runs_for_task(&t.task_id) {
            for r in runs.into_iter().filter(|r| r.state == RunState::Running) {
                let _ = store.append(AppendRequest {
                    tenant_id: core_tenant,
                    session_id: t.session_id,
                    task_id: Some(t.task_id),
                    run_id: Some(r.run_id),
                    turn_id: None,
                    step_id: None,
                    aggregate_type: AggregateType::Run,
                    aggregate_id: *r.run_id.as_bytes(),
                    expected_sequence: None,
                    events: vec![typed(
                        "RunSuspended",
                        &RunEvent::RunSuspended,
                        actor.clone(),
                    )],
                });
            }
        }
        let _ = store.append(AppendRequest {
            tenant_id: core_tenant,
            session_id: t.session_id,
            task_id: Some(t.task_id),
            run_id: None,
            turn_id: None,
            step_id: None,
            aggregate_type: AggregateType::Task,
            aggregate_id: *t.task_id.as_bytes(),
            expected_sequence: None,
            events: vec![
                typed(
                    "TaskWaiting",
                    &TaskEvent::TaskWaiting {
                        reason: WaitReason::External,
                    },
                    actor.clone(),
                ),
                typed(
                    "TaskNeedsAttention",
                    &TaskEvent::TaskNeedsAttention {
                        reason:
                            "runtime restarted while the task was running; resume with StartTask"
                                .into(),
                    },
                    actor.clone(),
                ),
            ],
        });
        out.push(t.task_id);
    }
    out
}

#[derive(Clone, Copy)]
pub(crate) struct Lineage {
    tenant: TenantId,
    session: SessionId,
    task: Option<TaskId>,
    run: Option<RunId>,
    turn: Option<TurnId>,
    step: Option<RunStepId>,
}

impl Lineage {
    pub(crate) fn task(tenant: TenantId, session: SessionId, task: TaskId) -> Self {
        Self {
            tenant,
            session,
            task: Some(task),
            run: None,
            turn: None,
            step: None,
        }
    }
    fn run(tenant: TenantId, session: SessionId, task: TaskId, run: RunId) -> Self {
        Self {
            run: Some(run),
            ..Self::task(tenant, session, task)
        }
    }
}

pub(crate) fn typed<E: serde::Serialize>(event_type: &str, e: &E, actor: Actor) -> NewEvent {
    let mut ev = NewEvent::new(
        event_type,
        serde_json::to_value(e).expect("serializable"),
        actor,
    );
    ev.occurred_at = Some(Timestamp::now());
    ev
}

pub(crate) fn append(
    store: &mut EventStore,
    core: &Core,
    l: Lineage,
    aggregate_type: AggregateType,
    aggregate_id: [u8; 16],
    events: Vec<NewEvent>,
) -> std::result::Result<u64, String> {
    let stored = store
        .append(AppendRequest {
            tenant_id: l.tenant,
            session_id: l.session,
            task_id: l.task,
            run_id: l.run,
            turn_id: l.turn,
            step_id: l.step,
            aggregate_type,
            aggregate_id,
            expected_sequence: None,
            events,
        })
        .map_err(|e| e.to_string())?;
    let offset = stored.last().map(|e| e.offset).unwrap_or(0);
    if offset > 0 {
        core.last_offset.send_replace(offset);
    }
    Ok(offset)
}

/// Rebuild the transcript and harness state from the task's events.
pub(crate) async fn rebuild(
    core: &Core,
    task: &Task,
    budgets: Budgets,
) -> (Vec<Message>, HarnessState, u64, Vec<(String, InputMode)>) {
    let store = core.store.lock().await;
    let events = store
        .read_session(&task.session_id, 0, usize::MAX)
        .unwrap_or_default();
    let mut state = HarnessState {
        budgets,
        ..Default::default()
    };
    let mut transcript: Vec<Message> = Vec::new();
    let mut last_offset = 0;
    let mut pending_steps: HashMap<[u8; 16], StepType> = HashMap::new();
    let mut queued: Vec<(String, InputMode)> = Vec::new();
    let mut questions: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    let mut applied = 0usize;
    for ev in events
        .iter()
        .filter(|e| e.envelope.task_id == Some(task.task_id))
    {
        last_offset = ev.offset;
        let payload = store.payload(&ev.envelope).unwrap_or_default();
        match ev.envelope.event_type.as_str() {
            "TurnPrepared" => state.turns += 1,
            "PlanRecorded" | "PlanRevised" => {
                if let Some(r) = payload["plan_ref"].as_str()
                    && let Ok(bytes) = store.objects().get(r)
                    && let Ok(plan) = serde_json::from_slice::<Plan>(&bytes)
                {
                    state.record_plan(plan);
                    state.resolve_flags_by_plan();
                }
            }
            "RepairAttemptRecorded" => {
                let a = harness::RepairAttempt {
                    attempt_ordinal: payload["attempt_ordinal"].as_u64().unwrap_or(0) as u32,
                    failure_signature: payload["failure_signature"]
                        .as_str()
                        .unwrap_or_default()
                        .to_owned(),
                    hypothesis: payload["hypothesis"]
                        .as_str()
                        .unwrap_or_default()
                        .to_owned(),
                    hypothesis_fingerprint: payload["hypothesis_fingerprint"]
                        .as_str()
                        .unwrap_or_default()
                        .to_owned(),
                    evidence_refs: payload["evidence_refs"]
                        .as_array()
                        .map(|a| {
                            a.iter()
                                .filter_map(|x| x.as_str().map(str::to_owned))
                                .collect()
                        })
                        .unwrap_or_default(),
                    intended_fix: payload["intended_fix"]
                        .as_str()
                        .unwrap_or_default()
                        .to_owned(),
                    start_revision: payload["start_revision"].as_u64().unwrap_or(0),
                    outcome: None,
                    change_fingerprint: None,
                    start_fingerprint: None,
                };
                state.pending_attempt = Some(a.attempt_ordinal);
                state.repair_attempts.push(a);
            }
            "RepairAttemptConcluded" => {
                let o = payload["attempt_ordinal"].as_u64().unwrap_or(0) as u32;
                if let Some(a) = state
                    .repair_attempts
                    .iter_mut()
                    .find(|a| a.attempt_ordinal == o)
                {
                    a.outcome = payload["outcome"].as_str().map(str::to_owned);
                    a.change_fingerprint =
                        payload["change_fingerprint"].as_str().map(str::to_owned);
                }
                if state.pending_attempt == Some(o) {
                    state.pending_attempt = None;
                }
            }
            "ToolsActivated" => {
                for t in payload["tools"].as_array().into_iter().flatten() {
                    if let Some(n) = t.as_str()
                        && !state.activated_tools.iter().any(|x| x == n)
                    {
                        state.activated_tools.push(n.to_owned());
                    }
                }
            }
            "SelfReviewRecorded" => {
                state.self_review_clean = payload["unresolved"].as_u64() == Some(0);
            }
            "TaskSteered" => {
                state.steers += 1;
                applied += 1;
            }
            "VerificationBaselineRecorded" => {
                state.baseline_recorded = true;
                state.verification_plan_ref = payload["plan_ref"].as_str().map(str::to_owned);
                state.baseline_checks.clear();
                state.baseline_failing.clear();
                for c in payload["checks"].as_array().into_iter().flatten() {
                    let id = c["check_id"].as_str().unwrap_or_default().to_owned();
                    let st = c["status"].as_str().unwrap_or_default().to_owned();
                    if matches!(st.as_str(), "FAIL" | "ERROR" | "TIMEOUT")
                        && let Some(sym) = id.rsplit("::").next()
                    {
                        state.baseline_failing.push(sym.to_owned());
                    }
                    state.baseline_checks.push((id, st));
                }
            }
            "FlakyCheckQuarantined" => {
                if let Some(id) = payload["check_id"].as_str()
                    && !state.quarantined.iter().any(|q| q == id)
                {
                    state.quarantined.push(id.to_owned());
                }
            }
            "DiffInvariantViolated" => {
                if payload["class"] == "FLAG" {
                    let f = format!(
                        "{} {}",
                        payload["invariant"].as_str().unwrap_or_default(),
                        payload["paths"][0].as_str().unwrap_or_default()
                    );
                    if !state.open_flags.contains(&f) {
                        state.open_flags.push(f);
                    }
                }
            }
            "TaskInputQueued" => {
                let mode = serde_json::from_value::<InputMode>(payload["mode"].clone())
                    .unwrap_or(InputMode::FollowUp);
                if let Some(t) = payload["text"].as_str() {
                    queued.push((t.to_owned(), mode));
                }
            }
            "UserQuestionAsked" => {
                if let (Some(q), Some(c)) =
                    (payload["question_id"].as_str(), payload["call_id"].as_str())
                {
                    questions.insert(q.to_owned(), c.to_owned());
                }
            }
            "ContextEpochOpened" => {
                // docs/19: the model-visible transcript before the epoch is
                // replaced by its projection; the log itself is untouched, so
                // the rebuilt context is exactly what the model last saw.
                let n = usize::try_from(payload["source_entries"].as_u64().unwrap_or(0))
                    .unwrap_or(usize::MAX);
                if n >= transcript.len() {
                    transcript.clear();
                } else {
                    transcript.drain(..n);
                }
            }
            "ReproductionRecorded" => {
                state.reproduction = payload["status"].as_str().map(str::to_owned);
            }
            "ScopeExpansionRecorded" => {
                let paths: Vec<String> = payload["paths"]
                    .as_array()
                    .map(|a| {
                        a.iter()
                            .filter_map(|x| x.as_str().map(str::to_owned))
                            .collect()
                    })
                    .unwrap_or_default();
                match payload["resolution"].as_str() {
                    Some("QUESTION_REQUIRED") => state.scope_question_pending = paths,
                    Some("CONTINUE") => {
                        state.scope_question_pending.clear();
                        for p in paths {
                            if !state.scope_unlocked.contains(&p) {
                                state.scope_unlocked.push(p);
                            }
                        }
                    }
                    _ => state.scope_question_pending.clear(),
                }
            }
            "UserQuestionAnswered" => {
                // A scope question is answered by the user, never by the agent
                // (docs/28 §3): only a recorded answer unlocks the expansion.
                if !state.scope_question_pending.is_empty() {
                    state.scope_answer = Some(format!(
                        "{} {}",
                        payload["option_id"].as_str().unwrap_or_default(),
                        payload["text"].as_str().unwrap_or_default()
                    ));
                }
                // The answer becomes the result of the model's `user.ask` call.
                if let Some(call_id) = payload["question_id"]
                    .as_str()
                    .and_then(|q| questions.get(q))
                {
                    let answer = serde_json::json!({"status": "ANSWERED", "option_id": payload["option_id"], "text": payload["text"]})
                        .to_string();
                    let mut replaced = false;
                    for m in transcript.iter_mut().rev() {
                        for part in &mut m.parts {
                            if let ContentPart::ToolResult {
                                call_id: c,
                                content,
                                is_error,
                            } = part
                                && c.as_str() == call_id.as_str()
                            {
                                *content = answer.clone();
                                *is_error = false;
                                replaced = true;
                            }
                        }
                        if replaced {
                            break;
                        }
                    }
                    if !replaced {
                        transcript.push(Message {
                            role: Role::Tool,
                            parts: vec![ContentPart::ToolResult {
                                call_id: call_id.clone(),
                                content: answer,
                                is_error: false,
                            }],
                        });
                    }
                }
            }
            "StepScheduled" => {
                if let Ok(t) = serde_json::from_value::<StepType>(payload["step_type"].clone()) {
                    if t == StepType::ToolCall {
                        state.tool_calls += 1;
                    }
                    pending_steps.insert(ev.envelope.aggregate_id, t);
                }
            }
            "StepSucceeded" | "StepFailed" => {
                if let Some(r) = payload["output_ref"].as_str()
                    && let Ok(bytes) = store.objects().get(r)
                    && let Ok(entry) = serde_json::from_slice::<TranscriptEntry>(&bytes)
                {
                    apply_entry(&mut transcript, &mut state, entry);
                }
                pending_steps.remove(&ev.envelope.aggregate_id);
            }
            _ => {}
        }
    }
    let pending = queued.into_iter().skip(applied).collect();
    (transcript, state, last_offset, pending)
}

fn apply_entry(transcript: &mut Vec<Message>, state: &mut HarnessState, entry: TranscriptEntry) {
    match entry {
        TranscriptEntry::Assistant { text, tool_calls } => {
            let mut parts = Vec::new();
            if !text.is_empty() {
                parts.push(ContentPart::Text { text });
            }
            for (call_id, name, arguments_json) in tool_calls {
                parts.push(ContentPart::ToolCall {
                    call_id,
                    name,
                    arguments_json,
                });
            }
            transcript.push(Message {
                role: Role::Assistant,
                parts,
            });
        }
        TranscriptEntry::ToolResult {
            call_id,
            text,
            failure_signature,
            clears,
            wrote,
            ..
        } => {
            transcript.push(Message {
                role: Role::Tool,
                parts: vec![ContentPart::ToolResult {
                    call_id,
                    content: text,
                    is_error: failure_signature.is_some(),
                }],
            });
            for c in clears {
                state.open_failures.retain(|f| *f != c);
            }
            if let Some(f) = failure_signature
                && !state.open_failures.contains(&f)
            {
                state.open_failures.push(f);
            }
            if let Some(p) = wrote {
                state.note_write(&p);
            }
        }
        TranscriptEntry::User { text } => transcript.push(Message::text(Role::User, text)),
    }
}

/// Tool projection for the task (docs/16): registry tools runnable under the
/// profile plus the harness tools.
fn projection(
    core: &Core,
    task: &Task,
    lease: Option<&modbit_domain::lease::CapabilityLease>,
    state: &HarnessState,
) -> Vec<ToolProjection> {
    let visible = core
        .tools
        .visible_specs(Some(&task.execution_profile), lease);
    // Deferred tool search (REQ-EV-0134/0177/0229): the stable core is
    // projected with schemas; deferred tools are named by toolset in the
    // `tool.search` description and hydrated only once activated.
    let mut deferred: Vec<(String, String)> = Vec::new();
    let mut tools: Vec<ToolProjection> = Vec::new();
    for s in visible {
        if harness::is_deferred(&s.name) && !state.activated_tools.contains(&s.name) {
            deferred.push((harness::toolset_of(&s.name).to_owned(), s.name.clone()));
            continue;
        }
        tools.push(ToolProjection {
            name: s.name,
            description: format!("{} [effect: {:?}]", s.description, s.effect_class),
            input_schema: s.input_schema,
        });
    }
    deferred.sort();
    let mut by_set: Vec<(String, Vec<String>)> = Vec::new();
    for (set, name) in deferred {
        match by_set.last_mut() {
            Some((s, names)) if *s == set => names.push(name),
            _ => by_set.push((set, vec![name])),
        }
    }
    let catalog = by_set
        .iter()
        .map(|(set, names)| format!("{set}: {}", names.join(", ")))
        .collect::<Vec<_>>()
        .join("; ");
    tools.push(ToolProjection {
        name: TOOL_SEARCH.into(),
        description: format!(
            "Search the deferred tool catalog by words in a tool's name, toolset or purpose and activate the matches: they are projected with their schemas from the next turn on. Discovery never authorizes — a tool the task's profile or lease denies is not discoverable and stays refused at invocation. Deferred toolsets: {}",
            if catalog.is_empty() { "none".to_owned() } else { catalog }
        ),
        input_schema: serde_json::json!({"type":"object","properties":{"query":{"type":"string"},"activate":{"type":"array","items":{"type":"string"}}},"additionalProperties":false}),
    });
    tools.push(ToolProjection {
        name: ASK_TOOL.into(),
        description: "Ask the user one typed question only when the change set, the verification or a protected effect depends on the answer; offer concrete options. Never ask what the repository can answer. The run suspends until the answer arrives.".into(),
        input_schema: serde_json::json!({"type":"object","properties":{"question":{"type":"string"},"options":{"type":"array","items":{"type":"object","properties":{"id":{"type":"string"},"label":{"type":"string"}},"required":["id","label"]}},"allow_free_text":{"type":"boolean"},"reason":{"type":"string","enum":["change_set","verification","protected_effect","other"]}},"required":["question","reason"]}),
    });
    tools.push(ToolProjection {
        name: PLAN_TOOL.into(),
        description: "Record or revise the plan before writing: outcome, expected files, verification, protected effects.".into(),
        input_schema: serde_json::json!({"type":"object","properties":{"outcome":{"type":"string"},"expected_files":{"type":"array","items":{"type":"string"}},"verification":{"type":"array","items":{"type":"string"}},"protected_effects":{"type":"array","items":{"type":"string"}},"reason":{"type":"string"}},"required":["outcome","expected_files"]}),
    });
    tools.push(ToolProjection {
        name: COMPLETE_TOOL.into(),
        description: "Propose completion with a self-review: summary, findings (unresolved ones block completion), verification evidence.".into(),
        input_schema: serde_json::json!({"type":"object","properties":{"summary":{"type":"string"},"self_review":{"type":"object","properties":{"findings":{"type":"array","items":{"type":"object","properties":{"text":{"type":"string"},"resolved":{"type":"boolean"}},"required":["text","resolved"]}},"verification":{"type":"array","items":{"type":"string"}}},"required":["findings"]}},"required":["summary","self_review"]}),
    });
    tools.push(ToolProjection {
        name: REPAIR_TOOL.into(),
        description: "Record a repair attempt before changing code after a failed verification: the failing check (check_id), a one-sentence hypothesis, the evidence you read or ran, and the intended fix. A change after a failed verification without a recorded attempt is refused; an equivalent hypothesis for the same failure, or the policy's attempt bounds, escalate the task to Needs Attention with the attempt history; a WORSENED attempt is reverted (docs/28 §5).".into(),
        input_schema: serde_json::json!({"type":"object","properties":{"check_id":{"type":"string"},"failure_signature":{"type":"string"},"hypothesis":{"type":"string","minLength":1},"evidence_refs":{"type":"array","items":{"type":"string"}},"intended_fix":{"type":"string"}},"required":["hypothesis"]}),
    });
    tools.push(ToolProjection {
        name: VERIFY_TOOL.into(),
        description: "Run the derived verification plan as a TARGETED stage (build, tests) and get normalized failing checks first; the COMPLETION run happens on task.complete.".into(),
        input_schema: serde_json::json!({"type":"object","properties":{"reason":{"type":"string"}},"additionalProperties":false}),
    });
    tools.sort_by(|a, b| a.name.cmp(&b.name));
    tools
}

/// Pending steering inputs after `after_offset` (STEER / FOLLOW_UP / COLLECT).
async fn pending_inputs(core: &Core, task: &Task, after_offset: u64) -> Vec<(String, InputMode)> {
    let store = core.store.lock().await;
    let events = store
        .read_session(&task.session_id, after_offset, usize::MAX)
        .unwrap_or_default();
    let mut out = Vec::new();
    for ev in events.iter().filter(|e| {
        e.envelope.task_id == Some(task.task_id) && e.envelope.event_type == "TaskInputQueued"
    }) {
        let p = store.payload(&ev.envelope).unwrap_or_default();
        let mode =
            serde_json::from_value::<InputMode>(p["mode"].clone()).unwrap_or(InputMode::FollowUp);
        if let Some(t) = p["text"].as_str() {
            out.push((t.to_owned(), mode));
        }
    }
    out
}

async fn run_loop(
    core: Arc<Core>,
    task: Task,
    run_id: RunId,
    cfg: StartConfig,
    cancel: CancellationToken,
) {
    let actor = Actor::Agent(format!("solver:{}", task.task_id));
    let lt = Lineage::run(core.tenant_id, task.session_id, task.task_id, run_id);
    let (mut transcript, mut state, mut seen_offset, mut carried) =
        rebuild(&core, &task, cfg.budgets).await;
    // docs/28 §5 (PX-039): a goal that reports a failure must reproduce it
    // before a fix transaction.
    state.goal_reports_failure = harness::goal_reports_failure(&task.goal_text);
    // docs/19 (REQ-EV-0056/0092/0130): the epoch rebuilt from the log, if any.
    let mut epoch: Option<modbit_compaction::CompactionManifest> = epoch_of(&core, &task).await;
    let lease = core
        .store
        .lock()
        .await
        .leases_for_task(&task.task_id)
        .ok()
        .and_then(|l| l.into_iter().next());
    let mut tools: Vec<ToolProjection>;
    let end = 'outer: loop {
        if cancel.is_cancelled() {
            break LoopEnd::Cancelled;
        }
        // The projection follows the harness state: deferred tools activated
        // by `tool.search` appear with their schemas from this turn on.
        tools = projection(&core, &task, lease.as_ref(), &state);
        // Scope policy (docs/28 §3, PX-038): the user's answer decides whether
        // the expansion proceeds, and is recorded with the counters it was
        // measured against — always the original plan's.
        if !state.scope_question_pending.is_empty()
            && let Some(answer) = state.scope_answer.take()
        {
            let paths = std::mem::take(&mut state.scope_question_pending);
            let (out_of_plan_files, plan_revisions) = state.scope_counters();
            let resolution = scope_resolution(&answer, "");
            {
                let mut store = core.store.lock().await;
                let _ = append(
                    &mut store,
                    &core,
                    lt,
                    AggregateType::Task,
                    *task.task_id.as_bytes(),
                    vec![typed(
                        "ScopeExpansionRecorded",
                        &TaskEvent::ScopeExpansionRecorded {
                            paths: paths.clone(),
                            out_of_plan_files,
                            plan_revisions,
                            reason: "answered".to_owned(),
                            resolution: resolution.to_owned(),
                            answer: answer.clone(),
                        },
                        actor.clone(),
                    )],
                );
            }
            if resolution == "CONTINUE" {
                for p in paths {
                    if !state.scope_unlocked.contains(&p) {
                        state.scope_unlocked.push(p);
                    }
                }
            }
        }
        // Deferred tools the task may still call by name (discovery by use):
        // visible under profile × lease × kernel, not yet projected.
        let deferred_visible: Vec<String> = core
            .tools
            .visible_specs(Some(&task.execution_profile), lease.as_ref())
            .into_iter()
            .map(|s| s.name)
            .filter(|n| harness::is_deferred(n) && !state.activated_tools.contains(n))
            .collect();
        // Steering at a safe boundary (docs/14 contract 9): inputs queued
        // before this boundary (including before the loop started).
        let mut inputs = std::mem::take(&mut carried);
        inputs.extend(pending_inputs(&core, &task, seen_offset).await);
        // SteeringPolicy (REQ-EV-0191): STEER = interrupt-and-replace (applied
        // in order, and an in-flight model stream is cut when one arrives);
        // COLLECT = coalesced into one message after the current turn;
        // FOLLOW_UP = ordered separate turns (one per boundary, the rest carried).
        let mut apply: Vec<(String, &str)> = Vec::new();
        let mut collects: Vec<String> = Vec::new();
        let mut follow_ups: Vec<String> = Vec::new();
        for (text, mode) in inputs {
            match mode {
                InputMode::Steer => apply.push((text, "STEER")),
                InputMode::Collect => collects.push(text),
                InputMode::FollowUp => follow_ups.push(text),
            }
        }
        if !collects.is_empty() {
            apply.push((collects.join("\n"), "COLLECT"));
        }
        let mut follow_ups = follow_ups.into_iter();
        if let Some(first) = follow_ups.next() {
            apply.push((first, "FOLLOW_UP"));
        }
        carried.extend(follow_ups.map(|t| (t, InputMode::FollowUp)));
        for (text, label) in apply {
            transcript.push(Message::text(Role::User, format!("[{label}] {text}")));
            state.steers += 1;
            let mut store = core.store.lock().await;
            let off = append(
                &mut store,
                &core,
                lt,
                AggregateType::Task,
                *task.task_id.as_bytes(),
                vec![typed(
                    "TaskSteered",
                    &TaskEvent::TaskSteered { text },
                    actor.clone(),
                )],
            )
            .unwrap_or(0);
            seen_offset = seen_offset.max(off);
        }
        if let Err(x) = state.check_turn_budget() {
            break LoopEnd::BudgetExhausted(x);
        }
        // ---- Turn
        let turn_id = TurnId::new();
        let ordinal = state.turns + 1;
        state.turns = ordinal;
        let lturn = Lineage {
            turn: Some(turn_id),
            ..lt
        };
        {
            let mut store = core.store.lock().await;
            if append(
                &mut store,
                &core,
                lturn,
                AggregateType::Turn,
                *turn_id.as_bytes(),
                vec![typed(
                    "TurnPrepared",
                    &TurnEvent::TurnPrepared { run_id, ordinal },
                    actor.clone(),
                )],
            )
            .is_err()
            {
                break LoopEnd::ProviderFailed("STORE".into(), "append failed".into());
            }
        }
        // Compaction epochs (docs/19): when the model-visible transcript passes
        // the budget, the older entries become one epoch projection. The
        // canonical log is untouched and every dropped result stays reachable
        // by its ref.
        if let Some((manifest, kept)) = maybe_compact(
            &core,
            &task,
            &transcript,
            epoch.as_ref(),
            compaction_budget(),
        )
        .await
        {
            let manifest_ref = {
                let store = core.store.lock().await;
                store
                    .objects()
                    .put(serde_json::to_vec(&manifest).unwrap_or_default().as_slice())
                    .unwrap_or_default()
            };
            {
                let mut store = core.store.lock().await;
                let _ = append(
                    &mut store,
                    &core,
                    lt,
                    AggregateType::Task,
                    *task.task_id.as_bytes(),
                    vec![typed(
                        "ContextEpochOpened",
                        &TaskEvent::ContextEpochOpened {
                            epoch: manifest.epoch,
                            previous_epoch: manifest.previous_epoch,
                            source_head_offset: manifest.source_head_offset,
                            source_entries: u32::try_from(manifest.source_entries)
                                .unwrap_or(u32::MAX),
                            manifest_ref: manifest_ref.clone(),
                            manifest_hash: manifest.manifest_hash.clone(),
                            projection_tokens: manifest.projection_tokens,
                        },
                        actor.clone(),
                    )],
                );
            }
            epoch = Some(manifest);
            transcript = kept;
        }
        // ContextCompile step.
        let harness_json = serde_json::to_value(&state).unwrap_or_default();
        // REQ-EV-0169: the task's latest Context Pack enters the prompt with
        // its provenance; the envelope refuses any fragment that lacks it.
        let context_fragments = {
            let ledger = core.tools.ledger(task.task_id).await;
            let ledger = ledger.lock().await;
            ledger
                .last_pack
                .as_ref()
                .map(|p| {
                    p.entries
                        .iter()
                        .map(|e| modbit_prompt_compiler::ContextFragment {
                            source_ref: e.source_ref.clone(),
                            path: e.provenance.path.clone(),
                            workspace_revision: e.provenance.workspace_revision,
                            content_hash: e.provenance.content_hash.clone().unwrap_or_default(),
                            retrieval_reason: format!(
                                "{}; {}",
                                e.reason,
                                e.provenance.retrieval_reasons.join(", ")
                            ),
                            lines: e.lines,
                            text: e.text.clone(),
                            ephemeral: false,
                        })
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default()
        };
        let compiled = modbit_prompt_compiler::compile(modbit_prompt_compiler::PromptInput {
            goal: task.goal_text.clone(),
            workspace_root: task.workspace_root.clone(),
            execution_profile: task.execution_profile.clone(),
            workspace_rules: vec![],
            compaction_summary: epoch.as_ref().map(|m| m.projection.clone()),
            harness_state: harness_json.clone(),
            transcript: transcript.clone(),
            context: context_fragments,
            tools: tools.clone(),
            model_policy: ModelPolicy {
                endpoint: cfg.endpoint.clone(),
                model: cfg.model.clone(),
                reasoning_effort: None,
                service_tier: None,
            },
            max_output_tokens: 4096,
            timeout_ms: MODEL_TIMEOUT_MS,
        });
        let mut request = compiled.request;
        request.request_id = format!("{}:{}", task.task_id, ordinal);
        let pack_ref = {
            let store = core.store.lock().await;
            store
                .objects()
                .put(serde_json::json!({"harness_state": harness_json, "segment_hashes": compiled.segment_hashes, "tool_projection_hash": compiled.tool_projection_hash, "injected_fragments": compiled.injected_fragments, "rejected_fragments": compiled.rejected_fragments}).to_string().as_bytes())
                .ok()
        };
        let ctx_step = RunStepId::new();
        {
            let mut store = core.store.lock().await;
            let _ = append(
                &mut store,
                &core,
                Lineage {
                    step: Some(ctx_step),
                    ..lturn
                },
                AggregateType::RunStep,
                *ctx_step.as_bytes(),
                vec![
                    typed(
                        "StepScheduled",
                        &StepEvent::StepScheduled {
                            turn_id,
                            step_type: StepType::ContextCompile,
                            ordinal: 1,
                            input_ref: None,
                        },
                        actor.clone(),
                    ),
                    typed("StepStarted", &StepEvent::StepStarted, actor.clone()),
                    typed(
                        "StepSucceeded",
                        &StepEvent::StepSucceeded {
                            output_ref: pack_ref.clone(),
                        },
                        actor.clone(),
                    ),
                ],
            );
            let _ = append(
                &mut store,
                &core,
                lturn,
                AggregateType::Turn,
                *turn_id.as_bytes(),
                vec![
                    typed(
                        "ContextPackCompiled",
                        &TurnEvent::ContextPackCompiled {
                            context_pack_id: compiled.context_pack_id.clone(),
                        },
                        actor.clone(),
                    ),
                    typed(
                        "ToolProjectionSelected",
                        &TurnEvent::ToolProjectionSelected {
                            tool_projection_hash: compiled.tool_projection_hash.clone(),
                        },
                        actor.clone(),
                    ),
                ],
            );
        }
        // ModelInvoke step.
        let invoke_step = RunStepId::new();
        let route_json = serde_json::json!({"endpoint": cfg.endpoint, "model": cfg.model, "cache_key": request.cache_key});
        {
            let mut store = core.store.lock().await;
            let _ = append(
                &mut store,
                &core,
                Lineage {
                    step: Some(invoke_step),
                    ..lturn
                },
                AggregateType::RunStep,
                *invoke_step.as_bytes(),
                vec![
                    typed(
                        "StepScheduled",
                        &StepEvent::StepScheduled {
                            turn_id,
                            step_type: StepType::ModelInvoke,
                            ordinal: 2,
                            input_ref: pack_ref.clone(),
                        },
                        actor.clone(),
                    ),
                    typed("StepStarted", &StepEvent::StepStarted, actor.clone()),
                ],
            );
            let _ = append(
                &mut store,
                &core,
                lturn,
                AggregateType::Turn,
                *turn_id.as_bytes(),
                vec![typed(
                    "ModelInvocationStarted",
                    &TurnEvent::ModelInvocationStarted {
                        model_route: route_json.clone(),
                    },
                    actor.clone(),
                )],
            );
        }
        let needs = Requirements {
            tools: true,
            ..Default::default()
        };
        let stream_cancel = cancel.child_token();
        let stream = match core.gateway.stream(request, &needs, stream_cancel.clone()) {
            Ok(s) => s,
            Err(e) => {
                let mut store = core.store.lock().await;
                let _ = append(
                    &mut store,
                    &core,
                    Lineage {
                        step: Some(invoke_step),
                        ..lturn
                    },
                    AggregateType::RunStep,
                    *invoke_step.as_bytes(),
                    vec![typed(
                        "StepFailed",
                        &StepEvent::StepFailed {
                            failure_code: "ROUTE_REFUSED".into(),
                            output_ref: None,
                        },
                        actor.clone(),
                    )],
                );
                let _ = append(
                    &mut store,
                    &core,
                    lturn,
                    AggregateType::Turn,
                    *turn_id.as_bytes(),
                    vec![typed(
                        "TurnFailed",
                        &TurnEvent::TurnFailed {
                            failure_code: "ROUTE_REFUSED".into(),
                        },
                        actor.clone(),
                    )],
                );
                break LoopEnd::ProviderFailed("ROUTE_REFUSED".into(), e.to_string());
            }
        };
        let mut text = String::new();
        let mut calls: Vec<(String, String, String)> = Vec::new();
        let mut usage = modbit_providers::Usage::default();
        let mut error: Option<(String, String)> = None;
        let mut events = stream.events;
        // A STEER queued while the model streams interrupts the stream
        // (REQ-EV-0191 interrupt-and-replace); the log offset watch wakes us.
        let mut offset_rx = core.last_offset.subscribe();
        let mut interrupted = false;
        loop {
            tokio::select! {
                ev = events.recv() => {
                    let Some(ev) = ev else { break };
                    match ev {
                        ModelEvent::MessageDelta { text: t } => text.push_str(&t),
                        ModelEvent::ToolCallComplete {
                            call_id,
                            name,
                            arguments_json,
                        } => calls.push((call_id, name, arguments_json)),
                        ModelEvent::Usage { usage: u } => usage = u,
                        ModelEvent::Completed { .. } => {}
                        ModelEvent::Error { code, message, .. } => error = Some((code, message)),
                        _ => {}
                    }
                }
                changed = offset_rx.changed() => {
                    if changed.is_err() {
                        continue;
                    }
                    let steer_pending = pending_inputs(&core, &task, seen_offset)
                        .await
                        .iter()
                        .any(|(_, m)| matches!(m, InputMode::Steer));
                    if steer_pending {
                        interrupted = true;
                        stream_cancel.cancel();
                        break;
                    }
                }
            }
        }
        if interrupted {
            let mut store = core.store.lock().await;
            let _ = append(
                &mut store,
                &core,
                Lineage {
                    step: Some(invoke_step),
                    ..lturn
                },
                AggregateType::RunStep,
                *invoke_step.as_bytes(),
                vec![typed(
                    "StepCancelled",
                    &StepEvent::StepCancelled,
                    actor.clone(),
                )],
            );
            let _ = append(
                &mut store,
                &core,
                lturn,
                AggregateType::Turn,
                *turn_id.as_bytes(),
                vec![typed(
                    "TurnInterrupted",
                    &TurnEvent::TurnInterrupted,
                    actor.clone(),
                )],
            );
            drop(store);
            // Nothing from the interrupted response is applied; the boundary
            // applies the steer and the next turn starts from it.
            continue 'outer;
        }
        let route_record = stream
            .route
            .lock()
            .map(|r| serde_json::to_value(&*r).unwrap_or_default())
            .unwrap_or_default();
        if cancel.is_cancelled() {
            let mut store = core.store.lock().await;
            let _ = append(
                &mut store,
                &core,
                Lineage {
                    step: Some(invoke_step),
                    ..lturn
                },
                AggregateType::RunStep,
                *invoke_step.as_bytes(),
                vec![typed(
                    "StepCancelled",
                    &StepEvent::StepCancelled,
                    actor.clone(),
                )],
            );
            let _ = append(
                &mut store,
                &core,
                lturn,
                AggregateType::Turn,
                *turn_id.as_bytes(),
                vec![typed(
                    "TurnInterrupted",
                    &TurnEvent::TurnInterrupted,
                    actor.clone(),
                )],
            );
            break LoopEnd::Cancelled;
        }
        if let Some((code, message)) = error {
            let mut store = core.store.lock().await;
            let _ = append(
                &mut store,
                &core,
                Lineage {
                    step: Some(invoke_step),
                    ..lturn
                },
                AggregateType::RunStep,
                *invoke_step.as_bytes(),
                vec![typed(
                    "StepFailed",
                    &StepEvent::StepFailed {
                        failure_code: code.clone(),
                        output_ref: None,
                    },
                    actor.clone(),
                )],
            );
            let _ = append(
                &mut store,
                &core,
                lturn,
                AggregateType::Turn,
                *turn_id.as_bytes(),
                vec![typed(
                    "TurnFailed",
                    &TurnEvent::TurnFailed {
                        failure_code: code.clone(),
                    },
                    actor.clone(),
                )],
            );
            break LoopEnd::ProviderFailed(code, message);
        }
        // Persist the assistant message before any action (docs/14 contract 1).
        let assistant = TranscriptEntry::Assistant {
            text: text.clone(),
            tool_calls: calls.clone(),
        };
        let assistant_ref = {
            let store = core.store.lock().await;
            store
                .objects()
                .put(
                    serde_json::to_vec(&assistant)
                        .unwrap_or_default()
                        .as_slice(),
                )
                .ok()
        };
        apply_entry(&mut transcript, &mut state, assistant);
        {
            let mut store = core.store.lock().await;
            let _ = append(
                &mut store,
                &core,
                lturn,
                AggregateType::Turn,
                *turn_id.as_bytes(),
                vec![
                    typed(
                        "ModelUsageRecorded",
                        &TurnEvent::ModelUsageRecorded {
                            input_tokens: usage.input_tokens,
                            output_tokens: usage.output_tokens,
                            cached_input_tokens: usage.cached_input_tokens,
                            route: route_record,
                        },
                        actor.clone(),
                    ),
                    typed(
                        "ModelInvocationCompleted",
                        &TurnEvent::ModelInvocationCompleted {
                            requested_actions: !calls.is_empty(),
                        },
                        actor.clone(),
                    ),
                ],
            );
            let _ = append(
                &mut store,
                &core,
                Lineage {
                    step: Some(invoke_step),
                    ..lturn
                },
                AggregateType::RunStep,
                *invoke_step.as_bytes(),
                vec![typed(
                    "StepSucceeded",
                    &StepEvent::StepSucceeded {
                        output_ref: assistant_ref,
                    },
                    actor.clone(),
                )],
            );
        }
        // ---- Actions
        let mut progress = false;
        let mut completed = false;
        let mut pending_question: Option<String> = None;
        let mut escalation: Option<RepairEscalation> = None;
        let mut scope_fail_closed: Option<String> = None;
        let mut step_ordinal = 2u32;
        for (call_id, name, arguments_json) in calls {
            if cancel.is_cancelled() {
                break;
            }
            step_ordinal += 1;
            // A direct call to a deferred-but-visible tool activates it
            // (REQ-EV-0134: discovery by use; authority still sits with the
            // kernel at dispatch).
            if deferred_visible.contains(&name) && !state.activated_tools.contains(&name) {
                state.activated_tools.push(name.clone());
                let mut store = core.store.lock().await;
                let _ = append(
                    &mut store,
                    &core,
                    lt,
                    AggregateType::Task,
                    *task.task_id.as_bytes(),
                    vec![typed(
                        "ToolsActivated",
                        &TaskEvent::ToolsActivated {
                            query: format!("invoked:{name}"),
                            tools: vec![name.clone()],
                        },
                        actor.clone(),
                    )],
                );
            }
            // docs/28 §2 (PX-015): an edit of an existing file needs a retrieval
            // record at the current workspace revision.
            let unretrieved: (Vec<String>, u64) = if WRITE_TOOLS.contains(&name.as_str()) {
                unretrieved_targets(&core, &task, &name, &arguments_json).await
            } else {
                (vec![], 0)
            };
            let (entry, step_type, failure_code) = match name.as_str() {
                PLAN_TOOL => {
                    let (entry, ok) = handle_plan(
                        &core,
                        &task,
                        lt,
                        &actor,
                        &mut state,
                        &call_id,
                        &arguments_json,
                    )
                    .await;
                    progress = true;
                    (
                        entry,
                        StepType::Plan,
                        if ok {
                            None
                        } else {
                            Some("INVALID_PLAN".to_owned())
                        },
                    )
                }
                TOOL_SEARCH => {
                    let entry = handle_tool_search(
                        &core,
                        &task,
                        lt,
                        &actor,
                        &mut state,
                        lease.as_ref(),
                        &call_id,
                        &arguments_json,
                    )
                    .await;
                    progress = true;
                    (entry, StepType::Plan, None)
                }
                REPAIR_TOOL => {
                    let (entry, esc) = handle_repair(
                        &core,
                        &task,
                        lt,
                        &actor,
                        &mut state,
                        &call_id,
                        &arguments_json,
                    )
                    .await;
                    progress = true;
                    let failed = esc.is_some();
                    if esc.is_some() {
                        escalation = esc;
                    }
                    (
                        entry,
                        StepType::Plan,
                        failed.then(|| "REPAIR_ESCALATED".to_owned()),
                    )
                }
                ASK_TOOL => {
                    let (entry, question_id) =
                        handle_ask(&core, &task, lturn, &actor, &call_id, &arguments_json).await;
                    match question_id {
                        Some(q) => {
                            pending_question = Some(q);
                            progress = true;
                            (entry, StepType::UserQuestion, None)
                        }
                        None => (entry, StepType::UserQuestion, Some("BAD_QUESTION".into())),
                    }
                }
                VERIFY_TOOL => {
                    let before = state.open_verify_signatures();
                    let (entry, ok, rev) =
                        handle_verify(&core, &task, lturn, &actor, &mut state, &call_id).await;
                    progress = true;
                    if state.pending_attempt.is_some()
                        && let Some(esc) = conclude_repair(
                            &core,
                            &task,
                            lt,
                            &actor,
                            &mut state,
                            &before,
                            rev.clone(),
                        )
                        .await
                    {
                        escalation = Some(esc);
                    }
                    (
                        entry,
                        StepType::Verification {
                            stage: VerificationStage::Targeted,
                            candidate_revision: rev,
                        },
                        if ok {
                            None
                        } else {
                            Some("VERIFICATION_FAILED".to_owned())
                        },
                    )
                }
                COMPLETE_TOOL => {
                    let (entry, ok) = handle_complete(
                        &core,
                        &task,
                        lturn,
                        &actor,
                        &mut state,
                        &call_id,
                        &arguments_json,
                    )
                    .await;
                    completed = ok;
                    (
                        entry,
                        StepType::SelfReview,
                        if ok {
                            None
                        } else {
                            Some("COMPLETION_REFUSED".to_owned())
                        },
                    )
                }
                // REQ-EV-0044: a tool outside the compiled surface (unsupported by
                // the host or not authorized) is refused before any effector.
                n if !tools.iter().any(|t| t.name == n)
                    && !deferred_visible.iter().any(|d| d == n) =>
                {
                    let entry = TranscriptEntry::ToolResult {
                        call_id: call_id.clone(),
                        name: name.clone(),
                        text: "status: REFUSED\nerror_code: TOOL_NOT_VISIBLE\nerror: the tool is not in this task's compiled surface (host support x policy)".into(),
                        failure_signature: None,
                        clears: vec![],
                        wrote: None,
                        progress: false,
                    };
                    (entry, StepType::ToolCall, Some("TOOL_NOT_VISIBLE".into()))
                }
                _ => {
                    let planned = state.check_tool(&name).and_then(|()| {
                        // docs/28 §3 (PX-016): no silent scope widening — every
                        // written path must be declared by the current plan.
                        write_targets(&name, &arguments_json)
                            .iter()
                            .try_for_each(|p| state.check_write(p))
                    });
                    // BASELINE before the first write (docs/64 §1): it is also
                    // the run that decides whether the reported failure
                    // reproduces (docs/28 §5, PX-039).
                    if planned.is_ok()
                        && WRITE_TOOLS.contains(&name.as_str())
                        && !state.baseline_recorded
                    {
                        let _ = run_verification(
                            &core,
                            &task,
                            lturn,
                            &actor,
                            &mut state,
                            Stage::Baseline,
                            step_ordinal,
                        )
                        .await;
                        step_ordinal += 1;
                    }
                    match planned
                        .and_then(|()| {
                            if unretrieved.0.is_empty() {
                                Ok(())
                            } else {
                                Err(HarnessRefusal::RetrievalRequired {
                                    paths: unretrieved.0.clone(),
                                    workspace_revision: unretrieved.1,
                                })
                            }
                        })
                        .and_then(|()| {
                            // docs/28 §5 (PX-039): reproduction first.
                            if WRITE_TOOLS.contains(&name.as_str()) {
                                state.check_reproduction()
                            } else {
                                Ok(())
                            }
                        })
                        .and_then(|()| {
                            // docs/28 §5 (PX-018): after a failed verification a
                            // change needs a recorded repair attempt.
                            if WRITE_TOOLS.contains(&name.as_str()) {
                                state.check_repair_gate()
                            } else {
                                Ok(())
                            }
                        })
                        .and_then(|()| {
                            state
                                .check_tool_budget()
                                .map_err(|x| HarnessRefusal::OpenFailures {
                                    failures: vec![format!("{}:{}/{}", x.budget, x.used, x.limit)],
                                })
                        }) {
                        Err(refusal) => {
                            // Scope policy (docs/28 §3, PX-038): record the
                            // expansion with its counters; a headless task
                            // fails closed instead of asking.
                            if let HarnessRefusal::ScopeQuestionRequired {
                                path,
                                reason,
                                out_of_plan_files,
                                plan_revisions,
                            } = &refusal
                            {
                                let headless = !matches!(
                                    task.origin,
                                    modbit_domain::task::TaskOrigin::Desktop
                                        | modbit_domain::task::TaskOrigin::IdeAdapter
                                );
                                let fails_closed = headless
                                    && state.scope_policy.headless_resolution == "FAIL_CLOSED";
                                let resolution = if fails_closed {
                                    "FAIL_CLOSED"
                                } else {
                                    "QUESTION_REQUIRED"
                                };
                                let mut store = core.store.lock().await;
                                let _ = append(
                                    &mut store,
                                    &core,
                                    lt,
                                    AggregateType::Task,
                                    *task.task_id.as_bytes(),
                                    vec![typed(
                                        "ScopeExpansionRecorded",
                                        &TaskEvent::ScopeExpansionRecorded {
                                            paths: vec![path.clone()],
                                            out_of_plan_files: *out_of_plan_files,
                                            plan_revisions: *plan_revisions,
                                            reason: reason.clone(),
                                            resolution: resolution.to_owned(),
                                            answer: String::new(),
                                        },
                                        actor.clone(),
                                    )],
                                );
                                drop(store);
                                if fails_closed {
                                    scope_fail_closed = Some(format!(
                                        "scope expansion to {path} needs a decision ({reason}); headless resolution is FAIL_CLOSED"
                                    ));
                                } else if !state.scope_question_pending.contains(path) {
                                    state.scope_question_pending.push(path.clone());
                                }
                            }
                            let code = match &refusal {
                                HarnessRefusal::PlanRequired => "PLAN_REQUIRED",
                                HarnessRefusal::PlanRevisionRequired { .. } => {
                                    "PLAN_REVISION_REQUIRED"
                                }
                                HarnessRefusal::RepairAttemptRequired { .. } => {
                                    "REPAIR_ATTEMPT_REQUIRED"
                                }
                                HarnessRefusal::RetrievalRequired { .. } => "RETRIEVAL_REQUIRED",
                                HarnessRefusal::ScopeQuestionRequired { .. } => {
                                    "SCOPE_QUESTION_REQUIRED"
                                }
                                HarnessRefusal::ReproductionRequired { .. } => {
                                    "REPRODUCTION_REQUIRED"
                                }
                                HarnessRefusal::OpenFailures { .. } => "BUDGET_EXHAUSTED",
                                _ => "HARNESS",
                            };
                            let entry = TranscriptEntry::ToolResult {
                                call_id: call_id.clone(),
                                name: name.clone(),
                                text: format!(
                                    "status: REFUSED\nerror_code: HARNESS_{code}\nerror: {}",
                                    serde_json::to_string(&refusal).unwrap_or_default()
                                ),
                                failure_signature: None,
                                clears: vec![],
                                wrote: None,
                                progress: false,
                            };
                            (entry, StepType::ToolCall, Some(format!("HARNESS_{code}")))
                        }
                        Ok(()) => {
                            // Per-transaction diff invariants (docs/64 §4).
                            if (name == "change.apply" || name == "change.batch")
                                && let Some(refusal) = transaction_invariants(
                                    &core,
                                    &task,
                                    lturn,
                                    &actor,
                                    &mut state,
                                    &arguments_json,
                                )
                                .await
                            {
                                let entry = TranscriptEntry::ToolResult {
                                    call_id: call_id.clone(),
                                    name: name.clone(),
                                    text: format!(
                                        "status: REFUSED\nerror_code: DIFF_INVARIANT_DENY\nerror: {refusal}"
                                    ),
                                    failure_signature: None,
                                    clears: vec![],
                                    wrote: None,
                                    progress: false,
                                };
                                let entry_ref = {
                                    let store = core.store.lock().await;
                                    store
                                        .objects()
                                        .put(
                                            serde_json::to_vec(&entry)
                                                .unwrap_or_default()
                                                .as_slice(),
                                        )
                                        .ok()
                                };
                                let step_id = RunStepId::new();
                                {
                                    let mut store = core.store.lock().await;
                                    let _ = append(
                                        &mut store,
                                        &core,
                                        Lineage {
                                            step: Some(step_id),
                                            ..lturn
                                        },
                                        AggregateType::RunStep,
                                        *step_id.as_bytes(),
                                        vec![
                                            typed(
                                                "StepScheduled",
                                                &StepEvent::StepScheduled {
                                                    turn_id,
                                                    step_type: StepType::ToolCall,
                                                    ordinal: step_ordinal,
                                                    input_ref: None,
                                                },
                                                actor.clone(),
                                            ),
                                            typed(
                                                "StepStarted",
                                                &StepEvent::StepStarted,
                                                actor.clone(),
                                            ),
                                            typed(
                                                "StepFailed",
                                                &StepEvent::StepFailed {
                                                    failure_code: "DIFF_INVARIANT_DENY".into(),
                                                    output_ref: entry_ref,
                                                },
                                                actor.clone(),
                                            ),
                                        ],
                                    );
                                }
                                apply_entry(&mut transcript, &mut state, entry);
                                continue;
                            }
                            state.tool_calls += 1;
                            let entry = execute_tool(
                                &core,
                                &task,
                                lt,
                                &actor,
                                &mut state,
                                &call_id,
                                &name,
                                &arguments_json,
                                &cancel,
                            )
                            .await;
                            let failure = match &entry {
                                TranscriptEntry::ToolResult {
                                    failure_signature,
                                    progress: p,
                                    ..
                                } => {
                                    progress |= *p;
                                    failure_signature.clone()
                                }
                                _ => None,
                            };
                            // docs/28 §5 (PX-039): a failing command or test the
                            // agent ran reproduces the reported failure too.
                            if is_check_tool(&name)
                                && failure.is_some()
                                && let Some(status) = state.record_reproduction(true)
                            {
                                let mut store = core.store.lock().await;
                                let _ = append(
                                    &mut store,
                                    &core,
                                    lt,
                                    AggregateType::Task,
                                    *task.task_id.as_bytes(),
                                    vec![typed(
                                        "ReproductionRecorded",
                                        &TaskEvent::ReproductionRecorded {
                                            status: status.to_owned(),
                                            failing_checks: vec![name.clone()],
                                            note: "a command the agent ran failed".into(),
                                        },
                                        actor.clone(),
                                    )],
                                );
                            }
                            (entry, StepType::ToolCall, failure)
                        }
                    }
                }
            };
            let entry_ref = {
                let store = core.store.lock().await;
                store
                    .objects()
                    .put(serde_json::to_vec(&entry).unwrap_or_default().as_slice())
                    .ok()
            };
            let step_id = RunStepId::new();
            {
                let mut store = core.store.lock().await;
                let mut evs = vec![
                    typed(
                        "StepScheduled",
                        &StepEvent::StepScheduled {
                            turn_id,
                            step_type,
                            ordinal: step_ordinal,
                            input_ref: None,
                        },
                        actor.clone(),
                    ),
                    typed("StepStarted", &StepEvent::StepStarted, actor.clone()),
                ];
                match &failure_code {
                    None => evs.push(typed(
                        "StepSucceeded",
                        &StepEvent::StepSucceeded {
                            output_ref: entry_ref.clone(),
                        },
                        actor.clone(),
                    )),
                    Some(code) => evs.push(typed(
                        "StepFailed",
                        &StepEvent::StepFailed {
                            failure_code: code.clone(),
                            output_ref: entry_ref.clone(),
                        },
                        actor.clone(),
                    )),
                }
                let _ = append(
                    &mut store,
                    &core,
                    Lineage {
                        step: Some(step_id),
                        ..lturn
                    },
                    AggregateType::RunStep,
                    *step_id.as_bytes(),
                    evs,
                );
            }
            apply_entry(&mut transcript, &mut state, entry);
            if completed
                || pending_question.is_some()
                || escalation.is_some()
                || scope_fail_closed.is_some()
            {
                break;
            }
        }
        if let Some(q) = pending_question {
            break LoopEnd::NeedsInput(q);
        }
        if let Some(reason) = scope_fail_closed {
            break LoopEnd::NeedsAttention(reason);
        }
        if let Some(esc) = escalation {
            break LoopEnd::NeedsAttention(format!(
                "repair escalated for {}: {} ({} attempt(s))",
                esc.failure_signature, esc.reason, esc.attempts
            ));
        }
        if cancel.is_cancelled() {
            let mut store = core.store.lock().await;
            let _ = append(
                &mut store,
                &core,
                lturn,
                AggregateType::Turn,
                *turn_id.as_bytes(),
                vec![typed(
                    "TurnInterrupted",
                    &TurnEvent::TurnInterrupted,
                    actor.clone(),
                )],
            );
            break LoopEnd::Cancelled;
        }
        {
            let mut store = core.store.lock().await;
            let mut evs = Vec::new();
            if !transcript.is_empty() && step_ordinal > 2 {
                evs.push(typed(
                    "TurnVerifying",
                    &TurnEvent::TurnVerifying,
                    actor.clone(),
                ));
            }
            evs.push(typed(
                "TurnCompleted",
                &TurnEvent::TurnCompleted,
                actor.clone(),
            ));
            let _ = append(
                &mut store,
                &core,
                lturn,
                AggregateType::Turn,
                *turn_id.as_bytes(),
                evs,
            );
        }
        if completed {
            break LoopEnd::ReadyForReview;
        }
        if progress {
            state.no_progress_turns = 0;
        } else {
            state.no_progress_turns += 1;
            if state.no_progress_turns >= state.budgets.max_consecutive_no_progress_turns {
                break 'outer LoopEnd::NoProgress(state.no_progress_turns);
            }
        }
        seen_offset = core
            .store
            .lock()
            .await
            .last_offset()
            .unwrap_or(seen_offset)
            .max(seen_offset);
    };
    // ---- Loop end: persist the run/task outcome.
    let mut store = core.store.lock().await;
    match end {
        LoopEnd::ReadyForReview => {
            let _ = append(
                &mut store,
                &core,
                lt,
                AggregateType::Run,
                *run_id.as_bytes(),
                vec![typed(
                    "RunCompleted",
                    &RunEvent::RunCompleted,
                    actor.clone(),
                )],
            );
            let _ = append(
                &mut store,
                &core,
                lt,
                AggregateType::Task,
                *task.task_id.as_bytes(),
                vec![typed(
                    "TaskReadyForReview",
                    &TaskEvent::TaskReadyForReview,
                    actor.clone(),
                )],
            );
        }
        LoopEnd::Cancelled => {
            let _ = append(
                &mut store,
                &core,
                lt,
                AggregateType::Run,
                *run_id.as_bytes(),
                vec![typed(
                    "RunCancelled",
                    &RunEvent::RunCancelled,
                    actor.clone(),
                )],
            );
            let _ = append(
                &mut store,
                &core,
                lt,
                AggregateType::Task,
                *task.task_id.as_bytes(),
                vec![typed(
                    "TaskCancelled",
                    &TaskEvent::TaskCancelled,
                    actor.clone(),
                )],
            );
        }
        LoopEnd::BudgetExhausted(x) => {
            let _ = append(
                &mut store,
                &core,
                lt,
                AggregateType::Run,
                *run_id.as_bytes(),
                vec![typed(
                    "RunSuspended",
                    &RunEvent::RunSuspended,
                    actor.clone(),
                )],
            );
            let _ = append(
                &mut store,
                &core,
                lt,
                AggregateType::Task,
                *task.task_id.as_bytes(),
                vec![
                    typed(
                        "HarnessBudgetExhausted",
                        &TaskEvent::HarnessBudgetExhausted {
                            budget: x.budget.clone(),
                            limit: x.limit,
                            used: x.used,
                        },
                        actor.clone(),
                    ),
                    typed(
                        "TaskWaiting",
                        &TaskEvent::TaskWaiting {
                            reason: WaitReason::UserInput,
                        },
                        actor.clone(),
                    ),
                    typed(
                        "TaskNeedsAttention",
                        &TaskEvent::TaskNeedsAttention {
                            reason: format!(
                                "budget `{}` exhausted ({}/{}); partial evidence retained",
                                x.budget, x.used, x.limit
                            ),
                        },
                        actor.clone(),
                    ),
                ],
            );
        }
        LoopEnd::NeedsInput(question_id) => {
            let _ = append(
                &mut store,
                &core,
                lt,
                AggregateType::Run,
                *run_id.as_bytes(),
                vec![typed(
                    "RunSuspended",
                    &RunEvent::RunSuspended,
                    actor.clone(),
                )],
            );
            let _ = append(
                &mut store,
                &core,
                lt,
                AggregateType::Task,
                *task.task_id.as_bytes(),
                vec![
                    typed(
                        "TaskWaiting",
                        &TaskEvent::TaskWaiting {
                            reason: WaitReason::UserInput,
                        },
                        actor.clone(),
                    ),
                    typed(
                        "TaskNeedsAttention",
                        &TaskEvent::TaskNeedsAttention {
                            reason: format!("question pending: {question_id}"),
                        },
                        actor.clone(),
                    ),
                ],
            );
        }
        LoopEnd::NeedsAttention(reason) => {
            let _ = append(
                &mut store,
                &core,
                lt,
                AggregateType::Run,
                *run_id.as_bytes(),
                vec![typed(
                    "RunSuspended",
                    &RunEvent::RunSuspended,
                    actor.clone(),
                )],
            );
            let _ = append(
                &mut store,
                &core,
                lt,
                AggregateType::Task,
                *task.task_id.as_bytes(),
                vec![
                    typed(
                        "TaskWaiting",
                        &TaskEvent::TaskWaiting {
                            reason: WaitReason::UserInput,
                        },
                        actor.clone(),
                    ),
                    typed(
                        "TaskNeedsAttention",
                        &TaskEvent::TaskNeedsAttention { reason },
                        actor.clone(),
                    ),
                ],
            );
        }
        LoopEnd::NoProgress(turns) => {
            let _ = append(
                &mut store,
                &core,
                lt,
                AggregateType::Run,
                *run_id.as_bytes(),
                vec![typed(
                    "RunSuspended",
                    &RunEvent::RunSuspended,
                    actor.clone(),
                )],
            );
            let _ = append(
                &mut store,
                &core,
                lt,
                AggregateType::Task,
                *task.task_id.as_bytes(),
                vec![
                    typed(
                        "NoProgressDetected",
                        &TaskEvent::NoProgressDetected { turns },
                        actor.clone(),
                    ),
                    typed(
                        "TaskWaiting",
                        &TaskEvent::TaskWaiting {
                            reason: WaitReason::UserInput,
                        },
                        actor.clone(),
                    ),
                    typed(
                        "TaskNeedsAttention",
                        &TaskEvent::TaskNeedsAttention {
                            reason: format!("{turns} consecutive turns without progress"),
                        },
                        actor.clone(),
                    ),
                ],
            );
        }
        LoopEnd::ProviderFailed(code, message) => {
            let _ = append(
                &mut store,
                &core,
                lt,
                AggregateType::Run,
                *run_id.as_bytes(),
                vec![typed(
                    "RunSuspended",
                    &RunEvent::RunSuspended,
                    actor.clone(),
                )],
            );
            let _ = append(
                &mut store,
                &core,
                lt,
                AggregateType::Task,
                *task.task_id.as_bytes(),
                vec![
                    typed(
                        "TaskWaiting",
                        &TaskEvent::TaskWaiting {
                            reason: WaitReason::Provider,
                        },
                        actor.clone(),
                    ),
                    typed(
                        "TaskNeedsAttention",
                        &TaskEvent::TaskNeedsAttention {
                            reason: format!("provider failure {code}: {message}"),
                        },
                        actor.clone(),
                    ),
                ],
            );
        }
    }
}

/// The existing files a write targets that have no retrieval record at the
/// current workspace revision, with that revision (docs/28 §2, PX-015).
async fn unretrieved_targets(
    core: &Core,
    task: &Task,
    tool_name: &str,
    arguments_json: &str,
) -> (Vec<String>, u64) {
    let targets = write_targets(tool_name, arguments_json);
    let Some(root) = task.workspace_root.as_deref() else {
        return (vec![], 0);
    };
    if targets.is_empty() {
        return (vec![], 0);
    }
    let rev = match core.tools.workspace(root).await {
        Ok((ws, _)) => ws.lock().await.revision().number,
        Err(_) => return (vec![], 0),
    };
    // A record binds to the bytes on disk now: reading a file and then editing
    // it elsewhere leaves the record stale (docs/28 §2).
    let current: Vec<(String, String)> = targets
        .into_iter()
        .filter_map(|p| {
            let abs = std::path::Path::new(root).join(&p);
            let bytes = std::fs::read(&abs).ok()?;
            Some((p, modbit_workspace::content_hash(&bytes)))
        })
        .collect();
    if current.is_empty() {
        return (vec![], rev);
    }
    let ledger = core.tools.ledger(task.task_id).await;
    let mut missing: Vec<(String, String)> = {
        let ledger = ledger.lock().await;
        current
            .into_iter()
            .filter(|(p, h)| !ledger.has_current_record(p, h))
            .collect()
    };
    // The durable records survive a Core restart even though the in-memory
    // ledger does not.
    if !missing.is_empty() {
        let store = core.store.lock().await;
        if let Ok(events) = store.read_aggregate(task.task_id.as_bytes(), 0, 100_000) {
            let mut recorded: Vec<(String, String)> = Vec::new();
            for e in &events {
                if e.envelope.event_type == "RetrievalRecorded"
                    && let Ok(p) = store.payload(&e.envelope)
                    && let (Some(path), Some(hash)) = (
                        p["path"].as_str().map(str::to_owned),
                        p["content_hash"].as_str().map(str::to_owned),
                    )
                {
                    recorded.push((path, hash));
                }
            }
            missing.retain(|m| !recorded.contains(m));
        }
    }
    (missing.into_iter().map(|(p, _)| p).collect(), rev)
}

/// The model-visible transcript budget before a compaction epoch opens
/// (docs/19). `MODBIT_COMPACTION_TOKEN_BUDGET` overrides it for tests and
/// for a deployment with a smaller context window.
fn compaction_budget() -> u32 {
    std::env::var("MODBIT_COMPACTION_TOKEN_BUDGET")
        .ok()
        .and_then(|v| v.parse().ok())
        .filter(|v| *v > 0)
        .unwrap_or(12_000)
}

/// Entries kept out of a compaction: the freshest turns stay verbatim.
const COMPACTION_KEEP_TAIL: usize = 4;

/// The manifest of the installed epoch, rebuilt from the log: the projection
/// the model sees and the facts the next epoch carries forward.
async fn epoch_of(core: &Core, task: &Task) -> Option<modbit_compaction::CompactionManifest> {
    let store = core.store.lock().await;
    let events = store
        .read_aggregate(task.task_id.as_bytes(), 0, 100_000)
        .ok()?;
    let mut out = None;
    for e in &events {
        if e.envelope.event_type != "ContextEpochOpened" {
            continue;
        }
        let Ok(p) = store.payload(&e.envelope) else {
            continue;
        };
        if let Some(m) = p["manifest_ref"]
            .as_str()
            .and_then(|r| store.objects().get(r).ok())
            .and_then(|b| serde_json::from_slice::<modbit_compaction::CompactionManifest>(&b).ok())
        {
            out = Some(m);
        }
    }
    out
}

/// Compact the transcript when it passes the budget: returns the manifest and
/// the entries that stay verbatim. `None` when nothing needs compacting or a
/// concurrent write moved the log (the stale guard refuses the result).
async fn maybe_compact(
    core: &Core,
    task: &Task,
    transcript: &[Message],
    installed: Option<&modbit_compaction::CompactionManifest>,
    budget: u32,
) -> Option<(modbit_compaction::CompactionManifest, Vec<Message>)> {
    let tokens: u32 = transcript
        .iter()
        .map(|m| modbit_compaction::estimate_tokens(&message_text(m)))
        .sum();
    if tokens <= budget || transcript.len() <= COMPACTION_KEEP_TAIL + 1 {
        return None;
    }
    let cut = transcript.len() - COMPACTION_KEEP_TAIL;
    let source: Vec<modbit_compaction::SourceEntry> = transcript[..cut]
        .iter()
        .map(|m| modbit_compaction::SourceEntry {
            role: format!("{:?}", m.role).to_lowercase(),
            name: String::new(),
            text: message_text(m),
            failure_signature: None,
        })
        .collect();
    let (generation, head) = {
        let store = core.store.lock().await;
        let generation = store
            .task(&task.task_id)
            .ok()
            .flatten()
            .map_or(0, |t| t.generation);
        (generation, store.last_offset().unwrap_or(0))
    };
    let manifest = modbit_compaction::compact(&modbit_compaction::CompactionRequest {
        entries: &source,
        previous: installed,
        task_generation: generation,
        source_head_offset: head,
        compiler_version: modbit_prompt_compiler::COMPILER_VERSION,
        target_tokens: budget / 4,
    });
    // docs/19: the result installs only while its source is still current.
    let (generation_now, head_now) = {
        let store = core.store.lock().await;
        let g = store
            .task(&task.task_id)
            .ok()
            .flatten()
            .map_or(0, |t| t.generation);
        (g, store.last_offset().unwrap_or(0))
    };
    if let Err(rejected) = modbit_compaction::accept(
        &manifest,
        installed.map(|m| m.epoch),
        generation_now,
        head_now,
    ) {
        eprintln!("modbit-core: compaction refused: {rejected:?}");
        return None;
    }
    Some((manifest, transcript[cut..].to_vec()))
}

/// The text of a message, for the token estimate and the compaction source.
fn message_text(m: &Message) -> String {
    m.parts
        .iter()
        .map(|p| match p {
            ContentPart::Text { text } => text.clone(),
            ContentPart::ToolCall {
                name,
                arguments_json,
                ..
            } => format!("{name} {arguments_json}"),
            ContentPart::ToolResult { content, .. } => content.clone(),
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Tools whose result is command or test evidence (docs/14 contract 3).
fn is_check_tool(name: &str) -> bool {
    matches!(name, "test.run" | "shell.exec")
}

/// The workspace's `FileChanged` records after `since` revision, oldest
/// first: (tool call, path, after hash, op, revision).
type ChangeRecord = (
    ToolCallId,
    String,
    Option<String>,
    Option<String>,
    String,
    u64,
);

async fn changes_since(core: &Core, root: &str, since: u64) -> Vec<ChangeRecord> {
    let store = core.store.lock().await;
    let Ok(events) = store.read_aggregate(&crate::tools::workspace_aggregate_id(root), 0, 100_000)
    else {
        return vec![];
    };
    let mut out = Vec::new();
    for e in &events {
        if let Ok(modbit_domain::workspace::WorkspaceEvent::FileChanged {
            tool_call_id,
            path,
            before_hash,
            after_hash,
            op,
            workspace_revision,
            ..
        }) = store
            .payload(&e.envelope)
            .and_then(|p| serde_json::from_value(p).map_err(Into::into))
            && workspace_revision > since
        {
            out.push((
                tool_call_id,
                path,
                before_hash,
                after_hash,
                op,
                workspace_revision,
            ));
        }
    }
    out
}

/// Latest workspace revision recorded by a `FileChanged` (0 when none).
async fn latest_change_revision(core: &Core, root: &str) -> u64 {
    changes_since(core, root, 0)
        .await
        .iter()
        .map(|c| c.5)
        .max()
        .unwrap_or(0)
}

/// `repair.attempt` (docs/28 §5, PX-018): record the attempt before the
/// change, or escalate.
async fn handle_repair(
    core: &Core,
    task: &Task,
    lt: Lineage,
    actor: &Actor,
    state: &mut HarnessState,
    call_id: &str,
    args: &str,
) -> (TranscriptEntry, Option<RepairEscalation>) {
    let v: serde_json::Value = serde_json::from_str(args).unwrap_or_default();
    let given = v["failure_signature"]
        .as_str()
        .or_else(|| v["check_id"].as_str())
        .unwrap_or_default();
    let hypothesis = v["hypothesis"]
        .as_str()
        .unwrap_or_default()
        .trim()
        .to_owned();
    let evidence_refs: Vec<String> = v["evidence_refs"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|x| x.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default();
    let intended_fix = v["intended_fix"].as_str().unwrap_or_default().to_owned();
    let refuse = |code: &str, msg: String| TranscriptEntry::ToolResult {
        call_id: call_id.into(),
        name: REPAIR_TOOL.into(),
        text: format!("status: REFUSED\nerror_code: {code}\nerror: {msg}"),
        failure_signature: None,
        clears: vec![],
        wrote: None,
        progress: false,
    };
    if hypothesis.is_empty() {
        return (
            refuse("INVALID_ARGUMENTS", "hypothesis is required".into()),
            None,
        );
    }
    let Some(sig) = state.resolve_signature(given) else {
        return (
            refuse(
                "NO_SUCH_OPEN_FAILURE",
                format!(
                    "`{given}` is not an open verification failure; open: {}",
                    state.open_verify_signatures().join(", ")
                ),
            ),
            None,
        );
    };
    let root = task.workspace_root.clone().unwrap_or_default();
    let start_revision = latest_change_revision(core, &root).await;
    match state.start_attempt(
        &sig,
        &hypothesis,
        evidence_refs.clone(),
        &intended_fix,
        start_revision,
    ) {
        Ok(a) => {
            let a = a.clone();
            let attempt_ref = {
                let store = core.store.lock().await;
                store
                    .objects()
                    .put(serde_json::to_vec(&a).unwrap_or_default().as_slice())
                    .unwrap_or_default()
            };
            let mut store = core.store.lock().await;
            let _ = append(
                &mut store,
                core,
                lt,
                AggregateType::Task,
                *task.task_id.as_bytes(),
                vec![typed(
                    "RepairAttemptRecorded",
                    &TaskEvent::RepairAttemptRecorded {
                        attempt_ordinal: a.attempt_ordinal,
                        failure_signature: a.failure_signature.clone(),
                        hypothesis: a.hypothesis.clone(),
                        hypothesis_fingerprint: a.hypothesis_fingerprint.clone(),
                        evidence_refs: a.evidence_refs.clone(),
                        intended_fix: a.intended_fix.clone(),
                        attempt_ref: attempt_ref.clone(),
                        start_revision: a.start_revision,
                    },
                    actor.clone(),
                )],
            );
            (
                TranscriptEntry::ToolResult {
                    call_id: call_id.into(),
                    name: REPAIR_TOOL.into(),
                    text: format!(
                        "status: SUCCESS\nrepair attempt {} recorded for {} (ref {attempt_ref}); make the change, then verify.run concludes it\nattempts on this failure: {}/{}; on this task: {}/{}",
                        a.attempt_ordinal,
                        a.failure_signature,
                        state
                            .repair_attempts
                            .iter()
                            .filter(|x| x.failure_signature == a.failure_signature)
                            .count(),
                        state.repair_policy.max_attempts_per_signature,
                        state.repair_attempts.len(),
                        state.repair_policy.max_attempts_per_task
                    ),
                    failure_signature: None,
                    clears: vec![],
                    wrote: None,
                    progress: true,
                },
                None,
            )
        }
        Err(esc) => {
            let history_ref = record_escalation(core, task, lt, actor, state, &esc).await;
            (
                TranscriptEntry::ToolResult {
                    call_id: call_id.into(),
                    name: REPAIR_TOOL.into(),
                    text: format!(
                        "status: REFUSED\nerror_code: REPAIR_ESCALATED\nerror: {} — the task moves to Needs Attention with the attempt history (ref {history_ref})",
                        esc.reason
                    ),
                    failure_signature: None,
                    clears: vec![],
                    wrote: None,
                    progress: false,
                },
                Some(esc),
            )
        }
    }
}

/// Record `RepairEscalated` with the attempt history as an object.
async fn record_escalation(
    core: &Core,
    task: &Task,
    lt: Lineage,
    actor: &Actor,
    state: &HarnessState,
    esc: &RepairEscalation,
) -> String {
    let history_ref = {
        let store = core.store.lock().await;
        store
            .objects()
            .put(
                serde_json::to_vec(&state.repair_attempts)
                    .unwrap_or_default()
                    .as_slice(),
            )
            .unwrap_or_default()
    };
    let mut store = core.store.lock().await;
    let _ = append(
        &mut store,
        core,
        lt,
        AggregateType::Task,
        *task.task_id.as_bytes(),
        vec![typed(
            "RepairEscalated",
            &TaskEvent::RepairEscalated {
                failure_signature: esc.failure_signature.clone(),
                reason: esc.reason.clone(),
                attempts: esc.attempts,
                history_ref: history_ref.clone(),
            },
            actor.clone(),
        )],
    );
    history_ref
}

/// Conclude the pending repair attempt after a verification run: outcome
/// from the signatures before and after, the change fingerprint from the
/// attempt's `FileChanged` records, a WORSENED change reverted through the
/// Change Engine, and the escalations of docs/28 §5.
async fn conclude_repair(
    core: &Arc<Core>,
    task: &Task,
    lt: Lineage,
    actor: &Actor,
    state: &mut HarnessState,
    before: &[String],
    verification_revision: String,
) -> Option<RepairEscalation> {
    let root = task.workspace_root.clone().unwrap_or_default();
    let start = state
        .pending_attempt
        .and_then(|o| {
            state
                .repair_attempts
                .iter()
                .find(|a| a.attempt_ordinal == o)
        })
        .map_or(0, |a| a.start_revision);
    let changes = changes_since(core, &root, start).await;
    let mut material = String::new();
    let mut change_refs: Vec<String> = Vec::new();
    let mut start_material = String::new();
    for (call, path, before_hash, after, _op, _rev) in &changes {
        material.push_str(&format!("{path}={}\n", after.clone().unwrap_or_default()));
        start_material.push_str(&format!(
            "{path}={}\n",
            before_hash.clone().unwrap_or_default()
        ));
        let c = call.to_string();
        if !change_refs.contains(&c) {
            change_refs.push(c);
        }
    }
    let fp = |m: &str| {
        if m.is_empty() {
            String::new()
        } else {
            hex::encode(sha2::Sha256::digest(m.as_bytes()))
        }
    };
    let change_fingerprint = fp(&material);
    let start_fingerprint = fp(&start_material);
    let after = state.open_verify_signatures();
    let (ordinal, outcome, escalation) =
        state.conclude_attempt(before, &after, &change_fingerprint, &start_fingerprint)?;
    let failure_signature = state
        .repair_attempts
        .iter()
        .find(|a| a.attempt_ordinal == ordinal)
        .map(|a| a.failure_signature.clone())
        .unwrap_or_default();
    // WORSENED: revert the attempt's changes, latest first (docs/28 §5).
    let mut reverted = false;
    if outcome == "WORSENED" {
        let mut calls: Vec<ToolCallId> = Vec::new();
        for (call, _, _, _, _, _) in changes.iter().rev() {
            if !calls.contains(call) {
                calls.push(*call);
            }
        }
        let mut all_ok = !calls.is_empty();
        for call in calls {
            match crate::undo::plan(core, &root, call).await {
                Ok(plan) => {
                    match crate::undo::apply(
                        core,
                        core.tenant_id,
                        task.session_id,
                        task.task_id,
                        &root,
                        &plan,
                    )
                    .await
                    {
                        Ok((ok, _, _)) => all_ok &= ok,
                        Err(_) => all_ok = false,
                    }
                }
                Err(_) => all_ok = false,
            }
        }
        reverted = all_ok;
    }
    {
        let mut store = core.store.lock().await;
        let _ = append(
            &mut store,
            core,
            lt,
            AggregateType::Task,
            *task.task_id.as_bytes(),
            vec![typed(
                "RepairAttemptConcluded",
                &TaskEvent::RepairAttemptConcluded {
                    attempt_ordinal: ordinal,
                    failure_signature,
                    outcome: outcome.clone(),
                    verification_revision,
                    change_refs,
                    change_fingerprint,
                    reverted,
                },
                actor.clone(),
            )],
        );
    }
    if let Some(esc) = &escalation {
        record_escalation(core, task, lt, actor, state, esc).await;
    }
    escalation
}

/// `tool.search` (REQ-EV-0134/0177/0229): match the query against the
/// deferred tools the task may see (profile and lease already applied by
/// `visible_specs`), activate the matches and record `ToolsActivated`.
#[allow(clippy::too_many_arguments)]
async fn handle_tool_search(
    core: &Core,
    task: &Task,
    lt: Lineage,
    actor: &Actor,
    state: &mut HarnessState,
    lease: Option<&modbit_domain::lease::CapabilityLease>,
    call_id: &str,
    args: &str,
) -> TranscriptEntry {
    let v: serde_json::Value = serde_json::from_str(args).unwrap_or_default();
    let query = v["query"].as_str().unwrap_or_default().to_ascii_lowercase();
    let explicit: Vec<String> = v["activate"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|x| x.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default();
    let words: Vec<&str> = query.split_whitespace().collect();
    let visible = core
        .tools
        .visible_specs(Some(&task.execution_profile), lease);
    let mut matched: Vec<&modbit_tools::ToolSpec> = visible
        .iter()
        .filter(|s| harness::is_deferred(&s.name))
        .filter(|s| {
            explicit.contains(&s.name)
                || (!words.is_empty() && {
                    let hay = format!(
                        "{} {} {}",
                        s.name,
                        harness::toolset_of(&s.name),
                        s.description
                    )
                    .to_ascii_lowercase();
                    words.iter().any(|w| hay.contains(w))
                })
        })
        .collect();
    matched.sort_by(|a, b| a.name.cmp(&b.name));
    matched.truncate(12);
    let mut activated = Vec::new();
    for s in &matched {
        if !state.activated_tools.contains(&s.name) {
            state.activated_tools.push(s.name.clone());
            activated.push(s.name.clone());
        }
    }
    if !activated.is_empty() {
        let mut store = core.store.lock().await;
        let _ = append(
            &mut store,
            core,
            lt,
            AggregateType::Task,
            *task.task_id.as_bytes(),
            vec![typed(
                "ToolsActivated",
                &TaskEvent::ToolsActivated {
                    query: query.clone(),
                    tools: activated.clone(),
                },
                actor.clone(),
            )],
        );
    }
    let mut text = String::from("status: SUCCESS\n");
    if matched.is_empty() {
        text.push_str("no deferred tool matches the query within this task's profile and lease; discovery cannot widen what the task may use\n");
    } else {
        for s in &matched {
            text.push_str(&format!(
                "- {} [toolset: {}; effect: {:?}; capabilities: {}]: {}\n",
                s.name,
                harness::toolset_of(&s.name),
                s.effect_class,
                s.required_capabilities.join(","),
                s.description
            ));
        }
        text.push_str(&format!(
            "activated for the next turns: {}\nactivation does not authorize: the Capability Kernel decides every invocation\n",
            if activated.is_empty() { "(already active)".to_owned() } else { activated.join(", ") }
        ));
    }
    TranscriptEntry::ToolResult {
        call_id: call_id.into(),
        name: TOOL_SEARCH.into(),
        text,
        failure_signature: None,
        clears: vec![],
        wrote: None,
        progress: true,
    }
}

async fn handle_plan(
    core: &Core,
    task: &Task,
    lt: Lineage,
    actor: &Actor,
    state: &mut HarnessState,
    call_id: &str,
    args: &str,
) -> (TranscriptEntry, bool) {
    let plan: Result<Plan, _> = serde_json::from_str(args);
    match plan {
        Ok(plan) => {
            let plan_ref = {
                let store = core.store.lock().await;
                store
                    .objects()
                    .put(serde_json::to_vec(&plan).unwrap_or_default().as_slice())
                    .unwrap_or_default()
            };
            let (version, added, removed) = state.record_plan(plan.clone());
            state.resolve_flags_by_plan();
            let reason = serde_json::from_str::<serde_json::Value>(args)
                .ok()
                .and_then(|v| v["reason"].as_str().map(str::to_owned))
                .unwrap_or_default();
            // docs/28 §5 (PX-039): the plan may record that the reported failure
            // could not be reproduced; the limitation is then explicit, never silent.
            let waived = state.waive_reproduction(&plan.outcome, &reason);
            let ev = if version == 1 {
                typed(
                    "PlanRecorded",
                    &TaskEvent::PlanRecorded {
                        plan_ref: plan_ref.clone(),
                        expected_files: plan.expected_files.clone(),
                        version,
                    },
                    actor.clone(),
                )
            } else {
                typed(
                    "PlanRevised",
                    &TaskEvent::PlanRevised {
                        plan_ref: plan_ref.clone(),
                        added,
                        removed,
                        reason: reason.clone(),
                        version,
                    },
                    actor.clone(),
                )
            };
            let mut evs = vec![ev];
            if waived {
                evs.push(typed(
                    "ReproductionRecorded",
                    &TaskEvent::ReproductionRecorded {
                        status: "WAIVED".into(),
                        failing_checks: vec![],
                        note: reason.clone(),
                    },
                    actor.clone(),
                ));
            }
            let mut store = core.store.lock().await;
            let _ = append(
                &mut store,
                core,
                lt,
                AggregateType::Task,
                *task.task_id.as_bytes(),
                evs,
            );
            (
                TranscriptEntry::ToolResult {
                    call_id: call_id.into(),
                    name: PLAN_TOOL.into(),
                    text: format!(
                        "status: SUCCESS\nplan version {version} recorded (ref {plan_ref})"
                    ),
                    failure_signature: None,
                    clears: vec![],
                    wrote: None,
                    progress: true,
                },
                true,
            )
        }
        Err(e) => (
            TranscriptEntry::ToolResult {
                call_id: call_id.into(),
                name: PLAN_TOOL.into(),
                text: format!("status: INVALID_ARGUMENTS\nerror: {e}"),
                failure_signature: None,
                clears: vec![],
                wrote: None,
                progress: false,
            },
            false,
        ),
    }
}

async fn handle_complete(
    core: &Core,
    task: &Task,
    lt: Lineage,
    actor: &Actor,
    state: &mut HarnessState,
    call_id: &str,
    args: &str,
) -> (TranscriptEntry, bool) {
    let v: serde_json::Value = serde_json::from_str(args).unwrap_or_default();
    let findings = v["self_review"]["findings"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let unresolved = findings
        .iter()
        .filter(|f| f["resolved"].as_bool() != Some(true))
        .count() as u32;
    let review_ref = {
        let store = core.store.lock().await;
        store
            .objects()
            .put(v.to_string().as_bytes())
            .unwrap_or_default()
    };
    {
        let mut store = core.store.lock().await;
        let _ = append(
            &mut store,
            core,
            lt,
            AggregateType::Task,
            *task.task_id.as_bytes(),
            vec![typed(
                "SelfReviewRecorded",
                &TaskEvent::SelfReviewRecorded {
                    review_ref: review_ref.clone(),
                    unresolved,
                },
                actor.clone(),
            )],
        );
    }
    state.self_review_clean = unresolved == 0;
    // COMPLETION run at the final candidate revision (docs/64 §1, docs/14 contract 11):
    // regression attribution against BASELINE and whole-diff invariants.
    // Signatures from earlier TARGETED/COMPLETION runs are superseded by the
    // COMPLETION run at the final revision; the plan gate, self-review and
    // command failures still refuse outright.
    let mut precheck_state = state.clone();
    precheck_state
        .open_failures
        .retain(|f| !f.starts_with("verify:") && !f.starts_with("completion:"));
    let precheck = precheck_state.check_completion(unresolved);
    let verdict = if precheck.is_ok() {
        let (_entry, ok, _rev) =
            run_verification(core, task, lt, actor, state, Stage::Completion, 0).await;
        if ok {
            Ok(())
        } else {
            Err(HarnessRefusal::CompletionBlocked {
                reasons: state.open_failures.clone(),
            })
        }
    } else {
        precheck
    };
    match verdict {
        Ok(()) => (
            TranscriptEntry::ToolResult {
                call_id: call_id.into(),
                name: COMPLETE_TOOL.into(),
                text: "status: SUCCESS\ncompletion proposed; the task moves to ReadyForReview"
                    .into(),
                failure_signature: None,
                clears: vec![],
                wrote: None,
                progress: true,
            },
            true,
        ),
        Err(r) => (
            TranscriptEntry::ToolResult {
                call_id: call_id.into(),
                name: COMPLETE_TOOL.into(),
                text: format!(
                    "status: REFUSED\nerror_code: COMPLETION_REFUSED\nerror: {}",
                    serde_json::to_string(&r).unwrap_or_default()
                ),
                failure_signature: None,
                clears: vec![],
                wrote: None,
                progress: false,
            },
            false,
        ),
    }
}

#[allow(clippy::too_many_arguments)]
async fn execute_tool(
    core: &Core,
    task: &Task,
    lt: Lineage,
    actor: &Actor,
    state: &mut HarnessState,
    call_id: &str,
    name: &str,
    args: &str,
    cancel: &CancellationToken,
) -> TranscriptEntry {
    let tool_call_id = ToolCallId::new();
    let mut first = true;
    loop {
        let (lease, approval, existing, stopped) = {
            let store = core.store.lock().await;
            let lease = store
                .leases_for_task(&task.task_id)
                .ok()
                .and_then(|l| l.into_iter().next());
            let approval = store.approval_for_call(&tool_call_id).unwrap_or(None);
            let existing = store.tool_call(&tool_call_id).unwrap_or(None);
            let stopped = store
                .session(&task.session_id)
                .ok()
                .flatten()
                .and_then(|s| s.emergency_stopped_at)
                .is_some();
            (lease, approval, existing, stopped)
        };
        let req = crate::tools::InvokeRequest {
            tenant_id: core.tenant_id,
            session_id: task.session_id,
            task_id: task.task_id,
            workspace_root: task.workspace_root.clone(),
            execution_profile: &task.execution_profile,
            tool_call_id,
            tool_name: name,
            arguments_json: args,
            output_budget_bytes: 64 * 1024,
            actor: actor.clone(),
            lease,
            approval,
            emergency_stopped: stopped,
            existing,
        };
        let done = match core.tools.invoke(&core.store, req).await {
            Ok(d) => d,
            Err(e) => {
                return TranscriptEntry::ToolResult {
                    call_id: call_id.into(),
                    name: name.into(),
                    text: format!("status: INFRA_FAILURE\nerror: {e}"),
                    failure_signature: Some(harness::failure_signature(
                        name,
                        "INFRA",
                        &e.to_string(),
                    )),
                    clears: vec![],
                    wrote: None,
                    progress: false,
                };
            }
        };
        if done.result.status == ToolStatus::ApprovalPending {
            if first {
                first = false;
                let mut store = core.store.lock().await;
                let _ = append(
                    &mut store,
                    core,
                    lt,
                    AggregateType::Task,
                    *task.task_id.as_bytes(),
                    vec![typed(
                        "TaskWaiting",
                        &TaskEvent::TaskWaiting {
                            reason: WaitReason::Approval,
                        },
                        actor.clone(),
                    )],
                );
            }
            // Wait for the resolver at a safe boundary; the same tool_call_id re-enters.
            loop {
                if cancel.is_cancelled() {
                    return TranscriptEntry::ToolResult {
                        call_id: call_id.into(),
                        name: name.into(),
                        text: "status: CANCELLED".into(),
                        failure_signature: None,
                        clears: vec![],
                        wrote: None,
                        progress: false,
                    };
                }
                tokio::time::sleep(std::time::Duration::from_millis(150)).await;
                let a = core
                    .store
                    .lock()
                    .await
                    .approval_for_call(&tool_call_id)
                    .unwrap_or(None);
                if a.is_some_and(|a| a.state != modbit_domain::approval::ApprovalState::Requested) {
                    break;
                }
            }
            {
                let mut store = core.store.lock().await;
                let _ = append(
                    &mut store,
                    core,
                    lt,
                    AggregateType::Task,
                    *task.task_id.as_bytes(),
                    vec![typed("TaskResumed", &TaskEvent::TaskResumed, actor.clone())],
                );
            }
            let call = core
                .store
                .lock()
                .await
                .tool_call(&tool_call_id)
                .unwrap_or(None);
            if call.is_some_and(|c| c.state == ToolCallState::Failed) {
                return TranscriptEntry::ToolResult { call_id: call_id.into(), name: name.into(), text: "status: POLICY_DENIED\nerror_code: APPROVAL_DENIED\nerror: the user denied this effect".into(), failure_signature: None, clears: vec![], wrote: None, progress: false };
            }
            continue;
        }
        let r = &done.result;
        let status = format!("{:?}", r.status).to_uppercase();
        let obs = harness::observe(
            &status,
            r.error_code.as_deref(),
            r.error_message.as_deref(),
            &r.structured_output.to_string(),
            r.stdout_ref.as_deref(),
            &done.result_ref,
            OBSERVATION_CEILING_BYTES,
        );
        // Failure evidence (docs/14 contract 3): commands and tests keep a signature until they pass.
        let is_check = is_check_tool(name);
        let check_failed = is_check
            && (r.status != ToolStatus::Success
                || r.structured_output["status"].as_str() == Some("FAILED")
                || r.structured_output["exit_code"]
                    .as_i64()
                    .is_some_and(|c| c != 0));
        let sig_prefix = format!("{name}:");
        let clears: Vec<String> = if is_check && !check_failed {
            state
                .open_failures
                .iter()
                .filter(|f| f.starts_with(&sig_prefix))
                .cloned()
                .collect()
        } else {
            vec![]
        };
        let failure_signature = if check_failed {
            Some(harness::failure_signature(
                name,
                r.error_code.as_deref().unwrap_or("FAILED"),
                &r.structured_output.to_string(),
            ))
        } else {
            None
        };
        let wrote = if (name == "change.apply" || name == "change.batch")
            && r.status == ToolStatus::Success
        {
            serde_json::from_str::<serde_json::Value>(args)
                .ok()
                .and_then(|v| {
                    v["path"]
                        .as_str()
                        .or_else(|| v["ops"][0]["path"].as_str())
                        .map(str::to_owned)
                })
        } else {
            None
        };
        if let Some(rev) = r.workspace_revision_after {
            state.candidate_revision = Some(rev);
        }
        // docs/28 §5: a transaction, a verification, a retrieval record, a plan
        // revision or a question is progress; browsing is not.
        let progress = r.status == ToolStatus::Success
            && (wrote.is_some()
                || is_check
                || name == "fs.read"
                || name.starts_with("lsp.")
                || name.starts_with("git.worktree"));
        return TranscriptEntry::ToolResult {
            call_id: call_id.into(),
            name: name.into(),
            text: obs.text,
            failure_signature,
            clears,
            wrote,
            progress,
        };
    }
}

/// Derive (or reload) the verification plan for the task.
async fn verification_plan(
    core: &Core,
    task: &Task,
    state: &mut HarnessState,
) -> (modbit_verification::VerificationPlan, String) {
    let root = task.workspace_root.as_deref().map(std::path::Path::new);
    if let Some(r) = &state.verification_plan_ref {
        let store = core.store.lock().await;
        if let Ok(bytes) = store.objects().get(r)
            && let Ok(mut plan) =
                serde_json::from_slice::<modbit_verification::VerificationPlan>(&bytes)
        {
            if let Some(p) = &state.plan {
                for v in &p.verification {
                    if !plan.acceptance_named.contains(v) {
                        plan.acceptance_named.push(v.clone());
                    }
                }
            }
            return (plan, r.clone());
        }
    }
    let named: Vec<String> = state
        .plan
        .as_ref()
        .map(|p| p.verification.clone())
        .unwrap_or_default();
    let mut plan =
        modbit_verification::derive(root.unwrap_or(std::path::Path::new(".")), &named, &[]);
    // docs/64 §6 (PX-035): targeting a run by impact evidence is heuristic and
    // the plan says so; only the COMPLETION run supports acceptance.
    plan.limitations
        .push(modbit_retrieval::impact::HEURISTIC_LIMITATION.to_owned());
    let plan_ref = {
        let store = core.store.lock().await;
        store
            .objects()
            .put(serde_json::to_vec(&plan).unwrap_or_default().as_slice())
            .unwrap_or_default()
    };
    state.verification_plan_ref = Some(plan_ref.clone());
    (plan, plan_ref)
}

fn is_failing(s: modbit_verification::CheckStatus) -> bool {
    matches!(
        s,
        modbit_verification::CheckStatus::Fail
            | modbit_verification::CheckStatus::Error
            | modbit_verification::CheckStatus::Timeout
    )
}

/// Run a verification stage through the engine, record it on the Run and as
/// a Verification step; update harness failure state. Returns the
/// observation entry, whether acceptance is not blocked, and the revision.
async fn run_verification(
    core: &Core,
    task: &Task,
    lturn: Lineage,
    actor: &Actor,
    state: &mut HarnessState,
    stage: Stage,
    ordinal: u32,
) -> (TranscriptEntry, bool, String) {
    let candidate = format!("ws-rev-{}", state.candidate_revision.unwrap_or(0));
    let Some(root) = task.workspace_root.clone() else {
        return (
            TranscriptEntry::ToolResult {
                call_id: String::new(),
                name: VERIFY_TOOL.into(),
                text: "status: INFRA_FAILURE\nerror: task has no workspace root".into(),
                failure_signature: None,
                clears: vec![],
                wrote: None,
                progress: false,
            },
            false,
            candidate,
        );
    };
    let (plan, plan_ref) = verification_plan(core, task, state).await;
    let runner = crate::verify::BrokerRunner {
        target: core.tools.execd.as_ref().map(|e| e.target.clone()),
        execution_profile: task.execution_profile.clone(),
    };
    let sink = crate::verify::ObjectSinkAdapter(core.store.lock().await.objects().clone());
    let engine = VerificationEngine::new(&runner, &sink, VerificationPolicy::default());
    let run_id = lturn.run.map(|r| r.to_string()).unwrap_or_default();
    // Fixture-facing state lives outside the candidate tree, per agent run,
    // and starts clean at BASELINE.
    let flaky_state = std::env::temp_dir().join(format!("modbit-flaky-{run_id}"));
    if stage == Stage::Baseline {
        let _ = std::fs::remove_file(&flaky_state);
    }
    let env: Vec<(String, String)> = vec![
        (
            "FIXTURE_FLAKY_STATE".into(),
            flaky_state.to_string_lossy().into_owned(),
        ),
        ("CARGO_TERM_COLOR".into(), "never".into()),
        ("NO_COLOR".into(), "1".into()),
    ];
    let step_id = RunStepId::new();
    let vstage = match stage {
        Stage::Baseline => VerificationStage::Baseline,
        Stage::Targeted => VerificationStage::Targeted,
        Stage::Completion => VerificationStage::Completion,
        Stage::Rerun => VerificationStage::Rerun,
    };
    if let Some(turn_id) = lturn.turn {
        let mut store = core.store.lock().await;
        let _ = append(
            &mut store,
            core,
            Lineage {
                step: Some(step_id),
                ..lturn
            },
            AggregateType::RunStep,
            *step_id.as_bytes(),
            vec![
                typed(
                    "StepScheduled",
                    &StepEvent::StepScheduled {
                        turn_id,
                        step_type: StepType::Verification {
                            stage: vstage,
                            candidate_revision: candidate.clone(),
                        },
                        ordinal: ordinal.max(1),
                        input_ref: Some(plan_ref.clone()),
                    },
                    actor.clone(),
                ),
                typed("StepStarted", &StepEvent::StepStarted, actor.clone()),
            ],
        );
    }
    let (vrun, quarantines) = engine
        .run_stage(
            &plan,
            &plan_ref,
            &run_id,
            stage,
            std::path::Path::new(&root),
            &candidate,
            &env,
            &["modbit-core".into()],
        )
        .await;
    let vrun_ref = {
        let store = core.store.lock().await;
        store
            .objects()
            .put(serde_json::to_vec(&vrun).unwrap_or_default().as_slice())
            .unwrap_or_default()
    };
    let mut events = crate::verify::stage_events(&vrun, &quarantines, actor);
    let indeterminate = matches!(
        vrun.status,
        modbit_verification::ReportStatus::Unknown
            | modbit_verification::ReportStatus::Timeout
            | modbit_verification::ReportStatus::Cancelled
    );
    let ok;
    let mut text = crate::verify::observation(&vrun, &*core.store.lock().await);
    for q in &quarantines {
        if !state.quarantined.contains(&q.check_id) {
            state.quarantined.push(q.check_id.clone());
        }
    }
    match stage {
        Stage::Baseline => {
            state.baseline_recorded = true;
            state.baseline_checks = vrun
                .checks()
                .into_iter()
                .map(|c| (c.check_id.clone(), format!("{:?}", c.status).to_uppercase()))
                .collect();
            state.baseline_failing = vrun
                .checks()
                .into_iter()
                .filter(|c| is_failing(c.status))
                .filter_map(|c| c.location.symbol.clone())
                .collect();
            text.push_str(&format!(
                "baseline recorded: {} KNOWN_FAILING check(s) are never attributed to you: {}\n",
                state.baseline_failing.len(),
                state.baseline_failing.join(", ")
            ));
            ok = true;
        }
        Stage::Targeted | Stage::Rerun => {
            state.open_failures.retain(|f| !f.starts_with("verify:"));
            for sig in vrun.failure_signatures() {
                let sym = sig
                    .split("::")
                    .nth(1)
                    .map(|s| s.split(':').next().unwrap_or_default())
                    .unwrap_or_default();
                if state.baseline_failing.iter().any(|b| b == sym) {
                    continue;
                }
                state.open_failures.push(format!("verify:{sig}"));
            }
            ok = !indeterminate
                && state
                    .open_failures
                    .iter()
                    .all(|f| !f.starts_with("verify:"));
        }
        Stage::Completion => {
            let base: std::collections::BTreeMap<String, modbit_verification::CheckStatus> = state
                .baseline_checks
                .iter()
                .filter_map(|(id, st)| {
                    serde_json::from_value::<modbit_verification::CheckStatus>(
                        serde_json::Value::String(st.clone()),
                    )
                    .ok()
                    .map(|s| (id.clone(), s))
                })
                .collect();
            let attribution = modbit_verification::attribute_against(&base, &vrun, &plan);
            for (check, a) in &attribution.checks {
                if *a != modbit_verification::Attribution::Pass {
                    events.push(typed(
                        "RegressionAttributed",
                        &RunEvent::RegressionAttributed {
                            verification_run_id: vrun.verification_run_id.clone(),
                            check_id: check.clone(),
                            attribution: attribution_label(*a),
                        },
                        actor.clone(),
                    ));
                }
            }
            let files = crate::verify::changed_files(std::path::Path::new(&root));
            let ctx = invariant_context(state);
            let violations = modbit_verification::evaluate_diff(&ctx, &files, None);
            // A FLAG whose path the current plan declares has already been
            // surfaced and justified (docs/64 §4, docs/28 §3): it is recorded
            // on every stage but blocks completion only until the plan says why.
            let planned_paths: Vec<String> = state
                .plan
                .as_ref()
                .map(|p| p.expected_files.clone())
                .unwrap_or_default();
            let mut deny = false;
            for v in &violations {
                events.push(typed(
                    "DiffInvariantViolated",
                    &RunEvent::DiffInvariantViolated {
                        invariant: v.id.clone(),
                        class: format!("{:?}", v.class).to_uppercase(),
                        paths: v.paths.clone(),
                        evidence: v.evidence.clone(),
                        stage: "COMPLETION".into(),
                    },
                    actor.clone(),
                ));
                if v.class == Class::Deny {
                    deny = true;
                }
                let f = format!("{} {}", v.id, v.paths.first().cloned().unwrap_or_default());
                let justified = !v.paths.is_empty()
                    && v.paths.iter().all(|p| {
                        planned_paths
                            .iter()
                            .any(|e| e == p || (e.ends_with('/') && p.starts_with(e.as_str())))
                    });
                if v.class == Class::Flag && !justified && !state.open_flags.contains(&f) {
                    state.open_flags.push(f);
                }
            }
            state
                .open_failures
                .retain(|f| !f.starts_with("verify:") && !f.starts_with("completion:"));
            for r in &attribution.reasons {
                state.open_failures.push(format!("completion:{r}"));
            }
            if deny {
                state
                    .open_failures
                    .push("completion:DENY-class diff invariant violated".into());
            }
            ok = !attribution.blocks_acceptance
                && !deny
                && state.open_flags.is_empty()
                && matches!(
                    vrun.status,
                    modbit_verification::ReportStatus::Passed
                        | modbit_verification::ReportStatus::Failed
                );
            if ok {
                state.completion_verified_revision = state.candidate_revision;
            }
            text.push_str(&format!(
                "attribution: {}\n",
                attribution
                    .checks
                    .iter()
                    .filter(|(_, a)| *a != modbit_verification::Attribution::Pass)
                    .map(|(c, a)| format!("{c}={a:?}"))
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
            for r in &attribution.reasons {
                text.push_str(&format!("blocked: {r}\n"));
            }
            for v in &violations {
                text.push_str(&format!(
                    "invariant {} [{:?}] {}: {}\n",
                    v.id,
                    v.class,
                    v.paths.join(","),
                    v.evidence
                ));
            }
        }
    }
    // docs/28 §5 (PX-039): what this run said about the failure the goal
    // reports — reproduced, or explicitly not.
    // The run's own failing checks (FLAKY and UNKNOWN never form a signature,
    // docs/64 §2), not a derived subset: a configured command's failure counts.
    let failing: Vec<String> = vrun.failure_signatures();
    if let Some(status) = state.record_reproduction(!failing.is_empty()) {
        let ev = typed(
            "ReproductionRecorded",
            &TaskEvent::ReproductionRecorded {
                status: status.to_owned(),
                failing_checks: failing.clone(),
                note: format!("verification run {}", vrun.verification_run_id),
            },
            actor.clone(),
        );
        let mut store = core.store.lock().await;
        let _ = append(
            &mut store,
            core,
            lturn,
            AggregateType::Task,
            *task.task_id.as_bytes(),
            vec![ev],
        );
        text.push_str(&format!(
            "reproduction: {status}{}\n",
            if failing.is_empty() {
                String::new()
            } else {
                format!(" ({})", failing.join(", "))
            }
        ));
    }
    {
        let mut store = core.store.lock().await;
        if let Some(run_id) = lturn.run {
            let _ = append(
                &mut store,
                core,
                lturn,
                AggregateType::Run,
                *run_id.as_bytes(),
                events,
            );
        }
        if lturn.turn.is_some() {
            let outcome = if ok {
                typed(
                    "StepSucceeded",
                    &StepEvent::StepSucceeded {
                        output_ref: Some(vrun_ref.clone()),
                    },
                    actor.clone(),
                )
            } else {
                typed(
                    "StepFailed",
                    &StepEvent::StepFailed {
                        failure_code: format!("{:?}", vrun.status).to_uppercase(),
                        output_ref: Some(vrun_ref.clone()),
                    },
                    actor.clone(),
                )
            };
            let _ = append(
                &mut store,
                core,
                Lineage {
                    step: Some(step_id),
                    ..lturn
                },
                AggregateType::RunStep,
                *step_id.as_bytes(),
                vec![outcome],
            );
        }
    }
    (
        TranscriptEntry::ToolResult {
            call_id: String::new(),
            name: VERIFY_TOOL.into(),
            text,
            failure_signature: None,
            clears: vec![],
            wrote: None,
            progress: true,
        },
        ok,
        candidate,
    )
}

fn attribution_label(a: modbit_verification::Attribution) -> String {
    use modbit_verification::Attribution::*;
    match a {
        Regression => "REGRESSION",
        KnownFailing => "KNOWN_FAILING",
        CollateralFix => "COLLATERAL_FIX",
        DeclaredChange => "DECLARED_CHANGE",
        Flaky => "FLAKY",
        NewFailing => "NEW_FAILING",
        Pass => "PASS",
    }
    .into()
}

fn invariant_context(state: &HarnessState) -> InvariantContext {
    InvariantContext {
        write_set: state.plan.as_ref().map(|p| p.expected_files.clone()),
        plan_entries: state
            .plan
            .as_ref()
            .map(|p| p.expected_files.clone())
            .unwrap_or_default(),
        acceptance_named: state
            .plan
            .as_ref()
            .map(|p| p.verification.clone())
            .unwrap_or_default(),
        baseline_failing: state.baseline_failing.clone(),
        protected_paths: vec![".github/".into(), ".modbit/".into()],
        formatting_churn_lines: 50,
        expected_revision: None,
    }
}

/// The `verify.run` harness tool: a TARGETED stage.
async fn handle_verify(
    core: &Core,
    task: &Task,
    lturn: Lineage,
    actor: &Actor,
    state: &mut HarnessState,
    call_id: &str,
) -> (TranscriptEntry, bool, String) {
    let (mut entry, ok, rev) =
        run_verification(core, task, lturn, actor, state, Stage::Targeted, 0).await;
    if let TranscriptEntry::ToolResult { call_id: c, .. } = &mut entry {
        *c = call_id.to_owned();
    }
    (entry, ok, rev)
}

/// The file as one change op would leave it (text ops are simulated; a
/// failing edit leaves the old content, which the tool then refuses anyway).
fn proposed_file(root: &str, op: &serde_json::Value) -> Option<modbit_verification::ChangedFile> {
    let path = op["path"].as_str()?.to_owned();
    let old = std::fs::read(std::path::Path::new(root).join(&path))
        .ok()
        .map(|b| String::from_utf8_lossy(&b).into_owned());
    let new = match op["op"].as_str() {
        Some("delete") => None,
        Some("create") | Some("replace") => {
            Some(op["content"].as_str().unwrap_or_default().to_owned())
        }
        Some("edit") => {
            let mut content = old.clone()?;
            for e in op["text_edits"].as_array().cloned().unwrap_or_default() {
                let (o, n) = (
                    e["old"].as_str().unwrap_or_default(),
                    e["new"].as_str().unwrap_or_default(),
                );
                match modbit_workspace::locate(&content, o, &path) {
                    Ok((start, end, _)) => content.replace_range(start..end, n),
                    Err(_) => break,
                }
            }
            Some(content)
        }
        Some("patch") => {
            let original = old.clone()?;
            let mut out = String::new();
            let mut cursor = 0usize;
            for e in op["edits"].as_array().cloned().unwrap_or_default() {
                let (start, end) = (
                    e["start"].as_u64().unwrap_or(0) as usize,
                    e["end"].as_u64().unwrap_or(0) as usize,
                );
                if start < cursor || end < start || end > original.len() {
                    return Some(modbit_verification::ChangedFile {
                        path,
                        old: Some(original.clone()),
                        new: Some(original),
                    });
                }
                out.push_str(original.get(cursor..start).unwrap_or_default());
                out.push_str(e["replacement"].as_str().unwrap_or_default());
                cursor = end;
            }
            out.push_str(original.get(cursor..).unwrap_or_default());
            Some(out)
        }
        _ => old.clone(),
    };
    Some(modbit_verification::ChangedFile { path, old, new })
}

/// DI evaluation for one proposed `change.apply` / `change.batch`; `Some(reason)` denies.
async fn transaction_invariants(
    core: &Core,
    task: &Task,
    lturn: Lineage,
    actor: &Actor,
    state: &mut HarnessState,
    args: &str,
) -> Option<String> {
    let v: serde_json::Value = serde_json::from_str(args).ok()?;
    let root = task.workspace_root.as_deref()?;
    let ops: Vec<serde_json::Value> = match v["ops"].as_array() {
        Some(ops) => ops.clone(),
        None => vec![v.clone()],
    };
    let ctx = invariant_context(state);
    let mut violations = Vec::new();
    let mut path = String::new();
    for op in &ops {
        let Some(file) = proposed_file(root, op) else {
            continue;
        };
        path.clone_from(&file.path);
        violations.extend(modbit_verification::evaluate_file(&ctx, &file, None));
    }
    if violations.is_empty() {
        return None;
    }
    let mut events = Vec::new();
    let mut deny_reasons = Vec::new();
    for x in &violations {
        events.push(typed(
            "DiffInvariantViolated",
            &RunEvent::DiffInvariantViolated {
                invariant: x.id.clone(),
                class: format!("{:?}", x.class).to_uppercase(),
                paths: x.paths.clone(),
                evidence: x.evidence.clone(),
                stage: "TRANSACTION".into(),
            },
            actor.clone(),
        ));
        match x.class {
            Class::Deny => {
                deny_reasons.push(format!("{} ({}): {}", x.id, x.paths.join(","), x.evidence));
            }
            Class::Flag => {
                let f = format!("{} {}", x.id, path);
                if !state.open_flags.contains(&f) {
                    state.open_flags.push(f);
                }
            }
        }
    }
    if let Some(run_id) = lturn.run {
        let mut store = core.store.lock().await;
        let _ = append(
            &mut store,
            core,
            lturn,
            AggregateType::Run,
            *run_id.as_bytes(),
            events,
        );
    }
    if deny_reasons.is_empty() {
        None
    } else {
        Some(deny_reasons.join("; "))
    }
}

/// The unanswered question of `task`, if any (asked without a later answer).
pub(crate) fn pending_question(store: &EventStore, task: &Task) -> Option<String> {
    let events = store
        .read_aggregate(task.task_id.as_bytes(), 0, usize::MAX)
        .unwrap_or_default();
    let mut open: Option<String> = None;
    for e in &events {
        let payload = store.payload(&e.envelope).unwrap_or_default();
        match e.envelope.event_type.as_str() {
            "UserQuestionAsked" => open = payload["question_id"].as_str().map(str::to_owned),
            "UserQuestionAnswered" if open.as_deref() == payload["question_id"].as_str() => {
                open = None;
            }
            _ => {}
        }
    }
    open
}

/// `user.ask`: record the typed question (with policy flags) and hand back the
/// placeholder result; the loop suspends the run right after.
async fn handle_ask(
    core: &Core,
    task: &Task,
    lt: Lineage,
    actor: &Actor,
    call_id: &str,
    arguments_json: &str,
) -> (TranscriptEntry, Option<String>) {
    let v: serde_json::Value = serde_json::from_str(arguments_json).unwrap_or_default();
    let question = v["question"].as_str().unwrap_or_default().trim().to_owned();
    let options: Vec<modbit_domain::task::QuestionOption> = v["options"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|o| {
                    Some(modbit_domain::task::QuestionOption {
                        id: o["id"].as_str()?.to_owned(),
                        label: o["label"].as_str().unwrap_or_default().to_owned(),
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    let allow_free_text = v["allow_free_text"].as_bool().unwrap_or(options.is_empty());
    let reason = v["reason"].as_str().unwrap_or("other").to_owned();
    if question.is_empty() || (options.is_empty() && !allow_free_text) {
        return (
            TranscriptEntry::ToolResult {
                call_id: call_id.into(),
                name: ASK_TOOL.into(),
                text: "status: REFUSED\nerror_code: BAD_QUESTION\nerror: a question needs text and either options or free text".into(),
                failure_signature: None,
                clears: vec![],
                wrote: None,
                progress: false,
            },
            None,
        );
    }
    // docs/28: never ask what the repository can answer. A yes/no question
    // naming an existing workspace path is flagged (the answer is verifiable).
    let mut flags = Vec::new();
    if let Some(root) = task.workspace_root.as_deref() {
        let mentions_existing_path = question
            .split(|c: char| c.is_whitespace() || c == '`' || c == '\'' || c == '"' || c == '?')
            .filter(|w| w.contains('.') || w.contains('/'))
            .any(|w| std::path::Path::new(root).join(w).exists());
        let yes_no = options.len() <= 2
            && options.iter().all(|o| {
                let id = o.id.to_ascii_lowercase();
                matches!(id.as_str(), "yes" | "no" | "y" | "n" | "true" | "false")
            });
        if mentions_existing_path && (yes_no || options.is_empty()) {
            flags.push("CONFIRMS_REPOSITORY_FACT".to_owned());
        }
    }
    let clean: String = call_id
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .take(24)
        .collect();
    let question_id = format!("q-{clean}");
    let mut store = core.store.lock().await;
    let _ = append(
        &mut store,
        core,
        lt,
        AggregateType::Task,
        *task.task_id.as_bytes(),
        vec![typed(
            "UserQuestionAsked",
            &TaskEvent::UserQuestionAsked {
                question_id: question_id.clone(),
                call_id: call_id.into(),
                question: question.clone(),
                options: options.clone(),
                allow_free_text,
                reason,
                flags: flags.clone(),
            },
            actor.clone(),
        )],
    );
    (
        TranscriptEntry::ToolResult {
            call_id: call_id.into(),
            name: ASK_TOOL.into(),
            text: format!(
                "status: PENDING\nquestion_id: {question_id}\nthe run is suspended until the user answers{}",
                if flags.is_empty() {
                    String::new()
                } else {
                    format!("\nflags: {}", flags.join(","))
                }
            ),
            failure_signature: None,
            clears: vec![],
            wrote: None,
            progress: true,
        },
        Some(question_id),
    )
}
