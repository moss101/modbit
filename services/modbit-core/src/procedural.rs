//! The procedural runtime's host (docs/16 "Procedural Tool Runtime", M5.3 /
//! M5.4): `tools.*` bindings that are ordinary tool calls.
//!
//! A program runs in the isolate of `modbit-procedural-runtime` with a
//! [`ProgramHost`] behind its bindings. The host offers exactly the turn's
//! projection (M5.1) narrowed by the effects the program declared, and serves
//! every call through the same pipeline as a direct model call: a fresh
//! `ToolCallId`, the projection fence, the Capability Kernel, the approval
//! flow, the receipt — Proposed → … → Succeeded | Failed on the log, one
//! aggregate per call, with the exec call's id as the `call_id` prefix so
//! the program's calls read as its children. The harness gates of doc 14
//! that govern writes (plan, scope, retrieval, repair) are applied by the
//! host from the harness state the program started under; a scope question
//! cannot be asked from inside a program and refuses the write instead.
//! Nothing here authorizes anything the direct path would not.

use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

use modbit_core_runtime::harness::{self, HarnessRefusal, HarnessState, WRITE_TOOLS};
use modbit_domain::event::Actor;
use modbit_domain::task::{Task, TaskEvent, WaitReason};
use modbit_domain::toolcall::ToolCallState;
use modbit_domain::{ToolCallId, event::AggregateType};
use modbit_procedural_runtime::{Host, HostError, HostFuture};
use modbit_tools::ToolStatus;
use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;

use crate::runtime::{Lineage, append, typed};
use crate::server::Core;

/// The model-visible `proc.exec` tool.
pub const EXEC_TOOL: &str = "proc.exec";
/// The model-visible `proc.wait` tool.
pub const WAIT_TOOL: &str = "proc.wait";

/// How long `proc.exec` waits for a program before handing back a handle.
pub const EXEC_INLINE_GRACE_MS: u64 = 2_000;
/// The longest a single `proc.wait` blocks.
pub const WAIT_CEILING_MS: u64 = 60_000;

/// The arguments of `proc.exec`.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct ExecArgs {
    /// The program: the body of an async function (`await`/`return` work).
    #[serde(default)]
    pub program: String,
    /// Tool names or toolsets the program will use; the bindings are the
    /// turn's projection narrowed to these (empty = the whole projection).
    #[serde(default)]
    pub declared_effects: Vec<String>,
    /// Budget overrides (each capped by the runtime's own ceilings).
    #[serde(default)]
    pub budget: BudgetArgs,
}

/// Budget overrides a program may ask for.
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct BudgetArgs {
    /// CPU time in milliseconds.
    #[serde(default)]
    pub cpu_time_ms: Option<u64>,
    /// Heap ceiling in bytes.
    #[serde(default)]
    pub memory_bytes: Option<usize>,
    /// Ceiling on `tools.*` calls.
    #[serde(default)]
    pub max_tool_calls: Option<u32>,
    /// Ceiling on the returned value's bytes.
    #[serde(default)]
    pub max_output_bytes: Option<usize>,
}

/// The runtime's ceilings on what a program may ask for.
pub const MAX_CPU_TIME_MS: u64 = 120_000;
/// Heap ceiling.
pub const MAX_MEMORY_BYTES: usize = 256 * 1024 * 1024;
/// Output ceiling.
pub const MAX_OUTPUT_BYTES: usize = 1024 * 1024;

/// The budget a program runs under: the request capped by the ceilings and
/// by what the task's own tool budget has left.
#[must_use]
pub fn budget_for(args: &BudgetArgs, state: &HarnessState) -> modbit_procedural_runtime::Budget {
    let defaults = modbit_procedural_runtime::Budget::default();
    let remaining_calls = state
        .budgets
        .max_tool_calls
        .saturating_sub(state.tool_calls);
    modbit_procedural_runtime::Budget {
        cpu_time_ms: args
            .cpu_time_ms
            .unwrap_or(defaults.cpu_time_ms)
            .min(MAX_CPU_TIME_MS),
        memory_bytes: args
            .memory_bytes
            .unwrap_or(defaults.memory_bytes)
            .min(MAX_MEMORY_BYTES),
        max_tool_calls: args
            .max_tool_calls
            .unwrap_or(defaults.max_tool_calls)
            .min(remaining_calls.max(1)),
        max_output_bytes: args
            .max_output_bytes
            .unwrap_or(defaults.max_output_bytes)
            .min(MAX_OUTPUT_BYTES),
        ..defaults
    }
}

/// Whether a declared effect names this tool: exact, or a toolset prefix.
fn declared(effects: &[String], name: &str) -> bool {
    effects.is_empty()
        || effects.iter().any(|e| {
            let e = e.trim();
            !e.is_empty() && (e == name || name.starts_with(&format!("{e}.")))
        })
}

/// The harness's own tools (and the procedural surface itself): the model's
/// to call, never a program's.
fn is_harness_tool(name: &str) -> bool {
    matches!(
        name,
        harness::PLAN_TOOL
            | harness::COMPLETE_TOOL
            | harness::VERIFY_TOOL
            | harness::ASK_TOOL
            | harness::REPAIR_TOOL
            | harness::TOOL_SEARCH
            | harness::CONTEXT_TOOL
            | EXEC_TOOL
            | WAIT_TOOL
    )
}

/// The bindings a program gets: the turn's projection (registry tools only —
/// the harness's own tools are the model's, not a program's) narrowed by the
/// declared effects.
#[must_use]
pub fn bindings_for(projection: &[String], declared_effects: &[String]) -> Vec<String> {
    projection
        .iter()
        .filter(|n| !is_harness_tool(n))
        .filter(|n| declared(declared_effects, n))
        .cloned()
        .collect()
}

/// The host behind one program.
pub struct ProgramHost {
    core: Arc<Core>,
    task: Task,
    lineage: Lineage,
    actor: Actor,
    /// The turn's projection, as the pipeline fence sees it.
    projection: Vec<String>,
    /// What the program may call.
    bindings: Vec<String>,
    /// The harness state the program started under (plan, scope, repair).
    state: HarnessState,
    /// The exec call's id: each program call is `<exec>#<n>`.
    exec_call_id: String,
    ordinal: AtomicU32,
    handle: tokio::runtime::Handle,
    cancel: CancellationToken,
}

impl ProgramHost {
    /// Build the host for a program started by the exec call `exec_call_id`.
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub fn new(
        core: Arc<Core>,
        task: Task,
        lineage: Lineage,
        actor: Actor,
        projection: Vec<String>,
        declared_effects: &[String],
        state: HarnessState,
        exec_call_id: String,
        cancel: CancellationToken,
    ) -> Self {
        let bindings = bindings_for(&projection, declared_effects);
        Self {
            core,
            task,
            lineage,
            actor,
            projection,
            bindings,
            state,
            exec_call_id,
            ordinal: AtomicU32::new(0),
            handle: tokio::runtime::Handle::current(),
            cancel,
        }
    }

    /// The harness gates a direct call would meet before the pipeline.
    fn harness_gate(&self, name: &str, arguments_json: &str) -> Result<(), HostError> {
        let refusal = self.state.check_tool(name).and_then(|()| {
            harness::write_targets(name, arguments_json)
                .iter()
                .try_for_each(|p| self.state.check_write(p))
        });
        let refusal = refusal.and_then(|()| {
            if WRITE_TOOLS.contains(&name) {
                self.state.check_repair_gate()
            } else {
                Ok(())
            }
        });
        match refusal {
            Ok(()) => Ok(()),
            Err(r) => {
                let code = match &r {
                    HarnessRefusal::PlanRequired => "HARNESS_PLAN_REQUIRED",
                    HarnessRefusal::PlanRevisionRequired { .. } => "HARNESS_PLAN_REVISION_REQUIRED",
                    HarnessRefusal::RepairAttemptRequired { .. } => {
                        "HARNESS_REPAIR_ATTEMPT_REQUIRED"
                    }
                    HarnessRefusal::ScopeQuestionRequired { .. } => {
                        "HARNESS_SCOPE_QUESTION_REQUIRED"
                    }
                    _ => "HARNESS",
                };
                Err(HostError {
                    code: code.into(),
                    message: format!(
                        "{}; a program cannot widen the plan or ask — return, and let the model revise the plan or ask the user",
                        serde_json::to_string(&r).unwrap_or_default()
                    ),
                })
            }
        }
    }
}

impl Host for ProgramHost {
    fn bindings(&self) -> Vec<String> {
        self.bindings.clone()
    }

    fn invoke(&self, name: &str, arguments_json: &str) -> HostFuture<Result<String, HostError>> {
        let n = self.ordinal.fetch_add(1, Ordering::Relaxed) + 1;
        let name = name.to_owned();
        let args = arguments_json.to_owned();
        // The bindings are the only names a program can reach; a name outside
        // them (a crafted call on the raw binding, were it reachable) is
        // refused here as the pipeline would refuse it — and the pipeline
        // still gets the projection.
        if !self.bindings.contains(&name) {
            return Box::pin(async move {
                Err(HostError {
                    code: "TOOL_NOT_PROJECTED".into(),
                    message: format!("`{name}` is not among this program's bindings"),
                })
            });
        }
        if let Err(e) = self.harness_gate(&name, &args) {
            return Box::pin(async move { Err(e) });
        }
        let core = self.core.clone();
        let task = self.task.clone();
        let lineage = self.lineage;
        let actor = self.actor.clone();
        let projection = self.projection.clone();
        let call_id = format!("{}#{n}", self.exec_call_id);
        let cancel = self.cancel.clone();
        let handle = self.handle.clone();
        Box::pin(async move {
            let joined = handle
                .spawn(async move {
                    serve(
                        core, task, lineage, actor, projection, call_id, name, args, cancel,
                    )
                    .await
                })
                .await;
            match joined {
                Ok(r) => r,
                Err(_) => Err(HostError {
                    code: "HOST_TASK_FAILED".into(),
                    message: "the host task ended before the call was served".into(),
                }),
            }
        })
    }
}

/// Serve one program call through the pipeline, with the approval flow of a
/// direct call: an approval-pending effect suspends the program (the task
/// waits on the approval) and re-enters with the same `ToolCallId` once
/// resolved.
#[allow(clippy::too_many_arguments)]
async fn serve(
    core: Arc<Core>,
    task: Task,
    lineage: Lineage,
    actor: Actor,
    projection: Vec<String>,
    call_id: String,
    name: String,
    args: String,
    cancel: CancellationToken,
) -> Result<String, HostError> {
    let tool_call_id = ToolCallId::new();
    let mut waited = false;
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
            tool_name: &name,
            arguments_json: &args,
            output_budget_bytes: 64 * 1024,
            actor: actor.clone(),
            lease,
            approval,
            emergency_stopped: stopped,
            existing,
            run_id: lineage.run_id(),
            turn_id: lineage.turn_id(),
            call_id: Some(call_id.clone()),
            lease_generation: lineage.lease(),
            projection: Some(projection.clone()),
        };
        let done = match core.tools.invoke(&core.store, req).await {
            Ok(d) => d,
            Err(e) => {
                return Err(HostError {
                    code: crate::tools::error_code(&e).to_owned(),
                    message: e.to_string(),
                });
            }
        };
        if done.result.status == ToolStatus::ApprovalPending {
            if !waited {
                waited = true;
                let mut store = core.store.lock().await;
                let _ = append(
                    &mut store,
                    &core,
                    lineage,
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
            loop {
                if cancel.is_cancelled() {
                    return Err(HostError {
                        code: "CANCELLED".into(),
                        message: "cancelled while awaiting approval".into(),
                    });
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
                    &core,
                    lineage,
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
                return Err(HostError {
                    code: "APPROVAL_DENIED".into(),
                    message: "the user denied this effect".into(),
                });
            }
            continue;
        }
        let r = &done.result;
        return if r.status == ToolStatus::Success {
            let mut value = r.structured_output.clone();
            if let serde_json::Value::Object(map) = &mut value {
                map.insert(
                    "tool_call_id".into(),
                    serde_json::Value::String(tool_call_id.to_string()),
                );
                if let Some(s) = &r.stdout_ref {
                    map.insert("stdout_ref".into(), serde_json::Value::String(s.clone()));
                }
                if let Some(rev) = r.workspace_revision_after {
                    map.insert("workspace_revision_after".into(), serde_json::json!(rev));
                }
            }
            Ok(value.to_string())
        } else {
            Err(HostError {
                code: r
                    .error_code
                    .clone()
                    .unwrap_or_else(|| format!("{:?}", r.status).to_uppercase()),
                message: r
                    .error_message
                    .clone()
                    .unwrap_or_else(|| format!("{:?}", r.status)),
            })
        };
    }
}

/// A program still running.
pub(crate) struct RunningProgram {
    join: tokio::task::JoinHandle<modbit_procedural_runtime::Outcome>,
    cancel: modbit_procedural_runtime::Cancel,
    bindings: Vec<String>,
    started: std::time::Instant,
}

/// The programs of one run: running ones by handle, and the outcomes of
/// finished ones (a `proc.wait` on a finished handle reads the outcome
/// again).
#[derive(Default)]
pub(crate) struct Programs {
    running: std::collections::HashMap<String, RunningProgram>,
    finished: std::collections::HashMap<String, modbit_procedural_runtime::Outcome>,
}

impl Programs {
    /// End every running program (the loop is ending): each stops at its
    /// next interrupt poll or binding call.
    pub(crate) fn cancel_all(&mut self) {
        for (_, p) in self.running.drain() {
            p.cancel.cancel();
            p.join.abort();
        }
    }

    /// Whether any program is still running.
    pub(crate) fn any_running(&self) -> bool {
        !self.running.is_empty()
    }
}

/// The arguments of `proc.wait`.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct WaitArgs {
    /// The handle `proc.exec` returned.
    #[serde(default)]
    pub handle: String,
    /// How long to wait, in milliseconds (capped by `WAIT_CEILING_MS`).
    #[serde(default)]
    pub timeout_ms: Option<u64>,
}

/// What `proc.exec` / `proc.wait` tell the model, and the bookkeeping the
/// loop applies.
pub(crate) struct ProgramResult {
    /// The transcript entry.
    pub entry: crate::runtime::TranscriptEntry,
    /// Step failure code, if the exec/wait step failed.
    pub failure_code: Option<String>,
}

const OUTPUT_CEILING_BYTES: usize = 16 * 1024;

fn refused(call_id: &str, name: &str, code: &str, error: &str) -> ProgramResult {
    ProgramResult {
        entry: crate::runtime::TranscriptEntry::ToolResult {
            call_id: call_id.into(),
            name: name.into(),
            text: format!("status: REFUSED\nerror_code: {code}\nerror: {error}"),
            failure_signature: None,
            clears: vec![],
            wrote: None,
            progress: false,
            media: vec![],
        },
        failure_code: Some(code.into()),
    }
}

/// `proc.exec`: start the program under the turn's projection; return its
/// outcome if it ends within the inline grace, else a handle for
/// `proc.wait`.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn handle_exec(
    core: &Arc<Core>,
    task: &Task,
    lineage: Lineage,
    actor: &Actor,
    state: &mut HarnessState,
    programs: &mut Programs,
    projection: &[String],
    call_id: &str,
    arguments_json: &str,
    cancel: &CancellationToken,
) -> ProgramResult {
    let args: ExecArgs = match serde_json::from_str(arguments_json) {
        Ok(a) => a,
        Err(e) => return refused(call_id, EXEC_TOOL, "INVALID_ARGUMENTS", &e.to_string()),
    };
    if args.program.trim().is_empty() {
        return refused(call_id, EXEC_TOOL, "INVALID_PROGRAM", "`program` is empty");
    }
    if programs.running.contains_key(call_id) || programs.finished.contains_key(call_id) {
        return refused(
            call_id,
            EXEC_TOOL,
            "DUPLICATE_HANDLE",
            "this call id already ran",
        );
    }
    let budget = budget_for(&args.budget, state);
    let host = ProgramHost::new(
        core.clone(),
        task.clone(),
        lineage,
        actor.clone(),
        projection.to_vec(),
        &args.declared_effects,
        state.clone(),
        call_id.to_owned(),
        cancel.clone(),
    );
    let bindings = host.bindings();
    let program_ref = {
        let store = core.store.lock().await;
        store
            .objects()
            .put(args.program.as_bytes())
            .unwrap_or_default()
    };
    {
        let mut store = core.store.lock().await;
        let _ = append(
            &mut store,
            core,
            lineage,
            AggregateType::Task,
            *task.task_id.as_bytes(),
            vec![typed(
                "ProgramStarted",
                &TaskEvent::ProgramStarted {
                    handle: call_id.to_owned(),
                    program_ref: program_ref.clone(),
                    declared_effects: args.declared_effects.clone(),
                    bindings: bindings.clone(),
                    budget: serde_json::to_value(&budget).unwrap_or_default(),
                },
                actor.clone(),
            )],
        );
    }
    let isolate_cancel = modbit_procedural_runtime::Cancel::new();
    let join = tokio::spawn(modbit_procedural_runtime::run_async(
        args.program.clone(),
        budget,
        Arc::new(host),
        isolate_cancel.clone(),
    ));
    let mut running = RunningProgram {
        join,
        cancel: isolate_cancel,
        bindings,
        started: std::time::Instant::now(),
    };
    match tokio::time::timeout(
        std::time::Duration::from_millis(EXEC_INLINE_GRACE_MS),
        &mut running.join,
    )
    .await
    {
        Ok(joined) => {
            let outcome = joined.unwrap_or_else(|_| failed_outcome("the program task ended early"));
            finalize(
                core, task, lineage, actor, state, programs, call_id, EXEC_TOOL, outcome,
            )
            .await
        }
        Err(_) => {
            let text = format!(
                "status: RUNNING\nhandle: {call_id}\nbindings: {}\nnote: the program is still running after {EXEC_INLINE_GRACE_MS} ms; call proc.wait with this handle (it may be awaiting an approval)",
                running.bindings.join(", ")
            );
            programs.running.insert(call_id.to_owned(), running);
            ProgramResult {
                entry: crate::runtime::TranscriptEntry::ToolResult {
                    call_id: call_id.into(),
                    name: EXEC_TOOL.into(),
                    text,
                    failure_signature: None,
                    clears: vec![],
                    wrote: None,
                    progress: true,
                    media: vec![],
                },
                failure_code: None,
            }
        }
    }
}

/// `proc.wait`: wait for a handle up to the timeout; the outcome once it
/// ended (again, if asked again), else `RUNNING`.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn handle_wait(
    core: &Arc<Core>,
    task: &Task,
    lineage: Lineage,
    actor: &Actor,
    state: &mut HarnessState,
    programs: &mut Programs,
    call_id: &str,
    arguments_json: &str,
    cancel: &CancellationToken,
) -> ProgramResult {
    let args: WaitArgs = match serde_json::from_str(arguments_json) {
        Ok(a) => a,
        Err(e) => return refused(call_id, WAIT_TOOL, "INVALID_ARGUMENTS", &e.to_string()),
    };
    if let Some(outcome) = programs.finished.get(&args.handle) {
        return ProgramResult {
            entry: outcome_entry(call_id, WAIT_TOOL, &args.handle, outcome, None),
            failure_code: None,
        };
    }
    let Some(mut running) = programs.running.remove(&args.handle) else {
        return refused(
            call_id,
            WAIT_TOOL,
            "UNKNOWN_HANDLE",
            &format!("no program `{}` in this run", args.handle),
        );
    };
    let timeout = args
        .timeout_ms
        .unwrap_or(WAIT_CEILING_MS)
        .min(WAIT_CEILING_MS);
    let waited = tokio::select! {
        r = tokio::time::timeout(std::time::Duration::from_millis(timeout), &mut running.join) => r,
        () = cancel.cancelled() => {
            running.cancel.cancel();
            programs.running.insert(args.handle.clone(), running);
            return refused(call_id, WAIT_TOOL, "CANCELLED", "the run was cancelled while waiting");
        }
    };
    match waited {
        Ok(joined) => {
            let outcome = joined.unwrap_or_else(|_| failed_outcome("the program task ended early"));
            finalize(
                core,
                task,
                lineage,
                actor,
                state,
                programs,
                &args.handle,
                WAIT_TOOL,
                outcome,
            )
            .await
        }
        Err(_) => {
            let text = format!(
                "status: RUNNING\nhandle: {}\nrunning_for_ms: {}\nnote: still running after {timeout} ms; call proc.wait again",
                args.handle,
                running.started.elapsed().as_millis()
            );
            programs.running.insert(args.handle.clone(), running);
            ProgramResult {
                entry: crate::runtime::TranscriptEntry::ToolResult {
                    call_id: call_id.into(),
                    name: WAIT_TOOL.into(),
                    text,
                    failure_signature: None,
                    clears: vec![],
                    wrote: None,
                    progress: false,
                    media: vec![],
                },
                failure_code: None,
            }
        }
    }
}

fn failed_outcome(reason: &str) -> modbit_procedural_runtime::Outcome {
    modbit_procedural_runtime::Outcome {
        status: modbit_procedural_runtime::Status::Failed,
        value_json: None,
        error: Some(reason.into()),
        log: vec![],
        log_truncated: false,
        calls: vec![],
        interrupt_polls: 0,
        elapsed_ms: 0,
        memory_peak_bytes: 0,
    }
}

fn status_label(s: &modbit_procedural_runtime::Status) -> (String, Option<String>) {
    use modbit_procedural_runtime::Status;
    match s {
        Status::Completed => ("COMPLETED".into(), None),
        Status::Failed => ("FAILED".into(), None),
        Status::BudgetExhausted { budget } => (
            "BUDGET_EXHAUSTED".into(),
            Some(
                serde_json::to_value(budget)
                    .ok()
                    .and_then(|v| v.as_str().map(str::to_owned))
                    .unwrap_or_else(|| format!("{budget:?}")),
            ),
        ),
        Status::Cancelled => ("CANCELLED".into(), None),
    }
}

/// The program ended: its calls are the task's calls (budget, writes,
/// candidate revision, failure evidence), the outcome is an object on the
/// log, and the model sees a bounded rendering.
#[allow(clippy::too_many_arguments)]
async fn finalize(
    core: &Arc<Core>,
    task: &Task,
    lineage: Lineage,
    actor: &Actor,
    state: &mut HarnessState,
    programs: &mut Programs,
    handle: &str,
    tool: &str,
    outcome: modbit_procedural_runtime::Outcome,
) -> ProgramResult {
    use modbit_procedural_runtime::Status;
    let calls = u32::try_from(outcome.calls.len()).unwrap_or(u32::MAX);
    state.tool_calls = state.tool_calls.saturating_add(calls);
    let mut failure_signature = None;
    let mut clears = Vec::new();
    for c in &outcome.calls {
        let Ok(result) = &c.outcome else { continue };
        let v: serde_json::Value = serde_json::from_str(result).unwrap_or_default();
        if WRITE_TOOLS.contains(&c.name.as_str()) {
            for p in harness::write_targets(&c.name, &c.arguments_json) {
                state.note_write(&p);
            }
            if let Some(rev) = v["workspace_revision_after"].as_u64() {
                state.candidate_revision = Some(rev);
            }
        }
        if matches!(c.name.as_str(), "test.run" | "shell.exec") {
            let failed = v["status"].as_str() == Some("FAILED")
                || v["exit_code"].as_i64().is_some_and(|x| x != 0);
            let prefix = format!("{}:", c.name);
            if failed {
                failure_signature = Some(harness::failure_signature(
                    &c.name,
                    v["error_code"].as_str().unwrap_or("FAILED"),
                    result,
                ));
            } else {
                clears.extend(
                    state
                        .open_failures
                        .iter()
                        .filter(|f| f.starts_with(&prefix))
                        .cloned(),
                );
            }
        }
    }
    let outcome_ref = {
        let store = core.store.lock().await;
        store
            .objects()
            .put(serde_json::to_vec(&outcome).unwrap_or_default().as_slice())
            .ok()
    };
    let (status, budget_exhausted) = status_label(&outcome.status);
    {
        let mut store = core.store.lock().await;
        let _ = append(
            &mut store,
            core,
            lineage,
            AggregateType::Task,
            *task.task_id.as_bytes(),
            vec![typed(
                "ProgramEnded",
                &TaskEvent::ProgramEnded {
                    handle: handle.to_owned(),
                    status: status.clone(),
                    budget_exhausted: budget_exhausted.clone(),
                    outcome_ref: outcome_ref.clone(),
                    tool_calls: calls,
                    elapsed_ms: outcome.elapsed_ms,
                    interrupt_polls: outcome.interrupt_polls,
                },
                actor.clone(),
            )],
        );
    }
    let failure_code = match &outcome.status {
        Status::Completed => None,
        Status::Failed => Some("PROGRAM_FAILED".to_owned()),
        Status::BudgetExhausted { .. } => Some("PROGRAM_BUDGET_EXHAUSTED".to_owned()),
        Status::Cancelled => Some("PROGRAM_CANCELLED".to_owned()),
    };
    let mut entry = outcome_entry(handle, tool, handle, &outcome, outcome_ref.as_deref());
    if let crate::runtime::TranscriptEntry::ToolResult {
        failure_signature: fs,
        clears: cl,
        progress,
        ..
    } = &mut entry
    {
        *fs = failure_signature;
        *cl = clears;
        *progress = matches!(outcome.status, Status::Completed) && calls > 0;
    }
    programs.finished.insert(handle.to_owned(), outcome);
    ProgramResult {
        entry,
        failure_code,
    }
}

/// The bounded rendering the model sees.
fn outcome_entry(
    call_id: &str,
    tool: &str,
    handle: &str,
    outcome: &modbit_procedural_runtime::Outcome,
    outcome_ref: Option<&str>,
) -> crate::runtime::TranscriptEntry {
    let (status, budget_exhausted) = status_label(&outcome.status);
    let mut text = format!("status: {status}\nhandle: {handle}\n");
    if let Some(b) = &budget_exhausted {
        text.push_str(&format!("budget_exhausted: {b}\n"));
    }
    if let Some(e) = &outcome.error {
        text.push_str(&format!("error: {e}\n"));
    }
    text.push_str(&format!(
        "tool_calls: {}\nelapsed_ms: {}\n",
        outcome.calls.len(),
        outcome.elapsed_ms
    ));
    if let Some(r) = outcome_ref {
        text.push_str(&format!("outcome_ref: {r}\n"));
    }
    if !outcome.calls.is_empty() {
        text.push_str("calls:\n");
        for c in &outcome.calls {
            match &c.outcome {
                Ok(_) => text.push_str(&format!("- {}: SUCCESS\n", c.name)),
                Err(code) => text.push_str(&format!("- {}: {code}\n", c.name)),
            }
        }
    }
    if !outcome.log.is_empty() {
        text.push_str("log:\n");
        for l in &outcome.log {
            text.push_str(&format!("  {l}\n"));
        }
        if outcome.log_truncated {
            text.push_str("  (log truncated at the budget)\n");
        }
    }
    if let Some(v) = &outcome.value_json {
        let (shown, cut) = if v.len() > OUTPUT_CEILING_BYTES {
            (&v[..OUTPUT_CEILING_BYTES], true)
        } else {
            (v.as_str(), false)
        };
        text.push_str(&format!("value:\n{shown}\n"));
        if cut {
            text.push_str(&format!(
                "(value truncated to {OUTPUT_CEILING_BYTES} of {} bytes; the full outcome is the object above)\n",
                v.len()
            ));
        }
    }
    crate::runtime::TranscriptEntry::ToolResult {
        call_id: call_id.into(),
        name: tool.into(),
        text,
        failure_signature: None,
        clears: vec![],
        wrote: None,
        progress: false,
        media: vec![],
    }
}
