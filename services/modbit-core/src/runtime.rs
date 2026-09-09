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

use modbit_core_runtime::harness::{
    self, Budgets, COMPLETE_TOOL, HarnessRefusal, HarnessState, PLAN_TOOL, Plan, VERIFY_TOOL,
    WRITE_TOOLS,
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
struct Lineage {
    tenant: TenantId,
    session: SessionId,
    task: Option<TaskId>,
    run: Option<RunId>,
    turn: Option<TurnId>,
    step: Option<RunStepId>,
}

impl Lineage {
    fn task(tenant: TenantId, session: SessionId, task: TaskId) -> Self {
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

fn typed<E: serde::Serialize>(event_type: &str, e: &E, actor: Actor) -> NewEvent {
    let mut ev = NewEvent::new(
        event_type,
        serde_json::to_value(e).expect("serializable"),
        actor,
    );
    ev.occurred_at = Some(Timestamp::now());
    ev
}

fn append(
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
async fn rebuild(
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
    let mut transcript = Vec::new();
    let mut last_offset = 0;
    let mut pending_steps: HashMap<[u8; 16], StepType> = HashMap::new();
    let mut queued: Vec<(String, InputMode)> = Vec::new();
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
fn projection(core: &Core, task: &Task) -> Vec<ToolProjection> {
    let mut tools: Vec<ToolProjection> = core
        .tools
        .runtime
        .registry()
        .specs()
        .into_iter()
        .filter(|s| s.execution_profiles.contains(&task.execution_profile))
        .map(|s| ToolProjection {
            name: s.name,
            description: format!("{} [effect: {:?}]", s.description, s.effect_class),
            input_schema: s.input_schema,
        })
        .collect();
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
    let tools = projection(&core, &task);
    let end = 'outer: loop {
        if cancel.is_cancelled() {
            break LoopEnd::Cancelled;
        }
        // Steering at a safe boundary (docs/14 contract 9): inputs queued
        // before this boundary (including before the loop started).
        let mut inputs = std::mem::take(&mut carried);
        inputs.extend(pending_inputs(&core, &task, seen_offset).await);
        for (text, mode) in inputs {
            let label = match mode {
                InputMode::Steer => "STEER",
                InputMode::Collect => "COLLECT",
                InputMode::FollowUp => "FOLLOW_UP",
            };
            let entry = TranscriptEntry::User {
                text: format!("[{label}] {text}"),
            };
            transcript.push(Message::text(Role::User, format!("[{label}] {text}")));
            state.steers += 1;
            let _ = entry;
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
        // ContextCompile step.
        let harness_json = serde_json::to_value(&state).unwrap_or_default();
        let compiled = modbit_prompt_compiler::compile(modbit_prompt_compiler::PromptInput {
            goal: task.goal_text.clone(),
            workspace_root: task.workspace_root.clone(),
            execution_profile: task.execution_profile.clone(),
            workspace_rules: vec![],
            compaction_summary: None,
            harness_state: harness_json.clone(),
            transcript: transcript.clone(),
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
                .put(serde_json::json!({"harness_state": harness_json, "segment_hashes": compiled.segment_hashes, "tool_projection_hash": compiled.tool_projection_hash}).to_string().as_bytes())
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
        let stream = match core.gateway.stream(request, &needs, cancel.clone()) {
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
        while let Some(ev) = events.recv().await {
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
        let mut step_ordinal = 2u32;
        for (call_id, name, arguments_json) in calls {
            if cancel.is_cancelled() {
                break;
            }
            step_ordinal += 1;
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
                VERIFY_TOOL => {
                    let (entry, ok, rev) =
                        handle_verify(&core, &task, lturn, &actor, &mut state, &call_id).await;
                    progress = true;
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
                _ => {
                    match state.check_tool(&name).and_then(|()| {
                        state
                            .check_tool_budget()
                            .map_err(|x| HarnessRefusal::OpenFailures {
                                failures: vec![format!("{}:{}/{}", x.budget, x.used, x.limit)],
                            })
                    }) {
                        Err(refusal) => {
                            let code = match &refusal {
                                HarnessRefusal::PlanRequired => "PLAN_REQUIRED",
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
                            // BASELINE before the first write (docs/64 §1).
                            if WRITE_TOOLS.contains(&name.as_str()) && !state.baseline_recorded {
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
                            // Per-transaction diff invariants (docs/64 §4).
                            if name == "change.apply"
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
            if completed {
                break;
            }
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
                        reason,
                        version,
                    },
                    actor.clone(),
                )
            };
            let mut store = core.store.lock().await;
            let _ = append(
                &mut store,
                core,
                lt,
                AggregateType::Task,
                *task.task_id.as_bytes(),
                vec![ev],
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
        let is_check = matches!(name, "test.run" | "shell.exec");
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
        let wrote = if name == "change.apply" && r.status == ToolStatus::Success {
            serde_json::from_str::<serde_json::Value>(args)
                .ok()
                .and_then(|v| v["path"].as_str().map(str::to_owned))
        } else {
            None
        };
        if let Some(rev) = r.workspace_revision_after {
            state.candidate_revision = Some(rev);
        }
        let progress = r.status == ToolStatus::Success
            && (wrote.is_some() || is_check || name.starts_with("git.worktree"));
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
    let plan = modbit_verification::derive(root.unwrap_or(std::path::Path::new(".")), &named, &[]);
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
                if v.class == Class::Flag && !state.open_flags.contains(&f) {
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

/// DI evaluation for one proposed `change.apply`; `Some(reason)` denies.
async fn transaction_invariants(
    core: &Core,
    task: &Task,
    lturn: Lineage,
    actor: &Actor,
    state: &mut HarnessState,
    args: &str,
) -> Option<String> {
    let v: serde_json::Value = serde_json::from_str(args).ok()?;
    let path = v["path"].as_str()?.to_owned();
    let root = task.workspace_root.as_deref()?;
    let old = std::fs::read(std::path::Path::new(root).join(&path))
        .ok()
        .map(|b| String::from_utf8_lossy(&b).into_owned());
    let new = match v["op"].as_str() {
        Some("delete") => None,
        Some("create") | Some("replace") => {
            Some(v["content"].as_str().unwrap_or_default().to_owned())
        }
        _ => old.clone(),
    };
    let file = modbit_verification::ChangedFile {
        path: path.clone(),
        old,
        new,
    };
    let violations = modbit_verification::evaluate_file(&invariant_context(state), &file, None);
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
