//! Core-side tool invocation (M2.4): the registry + kernel port + real
//! effectors behind the `InvokeTool` command, with every stage recorded as
//! ToolCall events on the canonical log (docs/16 "Tool completion proof").
//! Also supervises the `modbit-execd` broker the shell tools need (docs/33
//! startup step 7).

use std::collections::HashMap;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Arc;

use anyhow::{Context, Result};
use modbit_domain::approval::{Approval, ApprovalEvent};
use modbit_domain::event::{Actor, AggregateType};
use modbit_domain::lease::CapabilityLease;
use modbit_domain::toolcall::{EffectReceipt, ToolCall, ToolCallEvent, ToolCallState};
use modbit_domain::{ApprovalId, SessionId, TaskId, TenantId, ToolCallId};
use modbit_event_store::{AppendRequest, EventStore, NewEvent, ObjectStore};
use modbit_policy::{CapabilityKernel, KernelDecision, KernelRequest};
use modbit_protocol::local::{ReadyLine, decode_hex};
use modbit_tools::pipeline::ExecTarget;
use modbit_tools::{
    CapabilityPort, InvokeContext, ObjectSink, PolicyDecision, PolicyRequest, ProfilePolicy,
    ToolRegistry, ToolRuntime, ToolSpec, ToolStatus,
};
use modbit_workspace::WorkspaceService;
use tokio::sync::Mutex;

/// The supervised broker process.
pub struct Execd {
    child: Child,
    /// Where the shell tools connect.
    pub target: ExecTarget,
}

impl Drop for Execd {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Locate the broker binary: `MODBIT_EXECD_BIN`, else a sibling of this executable.
fn execd_binary() -> PathBuf {
    if let Ok(p) = std::env::var("MODBIT_EXECD_BIN") {
        return PathBuf::from(p);
    }
    let me = std::env::current_exe().unwrap_or_default();
    me.parent()
        .map(|d| {
            d.join(if cfg!(windows) {
                "modbit-execd.exe"
            } else {
                "modbit-execd"
            })
        })
        .unwrap_or_default()
}

/// Spawn the broker for this data directory and wait for its ready line.
pub fn spawn_execd(data_dir: &Path) -> Result<Execd> {
    let bin = execd_binary();
    // stdin is the lifetime tether: the broker exits when this pipe closes,
    // so a hard-killed Core never leaves an orphaned broker behind.
    let mut child = Command::new(&bin)
        .arg("--data-dir")
        .arg(data_dir.join("execd"))
        .arg("--tether-stdin")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .with_context(|| {
            format!(
                "spawning terminal broker `{}` (set MODBIT_EXECD_BIN)",
                bin.display()
            )
        })?;
    let stdout = child.stdout.take().context("broker stdout")?;
    let mut lines = BufReader::new(stdout).lines();
    let ready = loop {
        match lines.next() {
            Some(Ok(line)) => {
                if let Some(r) = ReadyLine::parse(&line) {
                    break r;
                }
            }
            _ => anyhow::bail!("modbit-execd exited before it was ready"),
        }
    };
    std::thread::spawn(move || for _ in lines {});
    let target = ExecTarget {
        endpoint: ready.endpoint,
        boot_secret: decode_hex(&ready.boot_secret_hex).context("broker secret")?,
    };
    Ok(Execd { child, target })
}

/// The event store's object directory as the OutputRef sink.
pub struct StoreSink(pub ObjectStore);
impl ObjectSink for StoreSink {
    fn put(&self, bytes: &[u8]) -> modbit_tools::Result<String> {
        self.0
            .put(bytes)
            .map_err(|e| modbit_tools::Error::Sink(e.to_string()))
    }
}

/// Tool host state in the Core.
pub struct ToolHost {
    /// Runtime (registry + kernel port).
    pub runtime: ToolRuntime,
    /// Broker, when it started.
    pub execd: Option<Execd>,
    /// One workspace service per approved root, kept so revisions stay monotonic in-process.
    workspaces: Mutex<HashMap<PathBuf, Arc<Mutex<WorkspaceService>>>>,
    state_dir: PathBuf,
}

impl ToolHost {
    /// Build the host: direct tools, default policy, broker.
    pub fn new(data_dir: &Path) -> Result<Self> {
        let mut registry = ToolRegistry::new();
        modbit_tools::direct::register_direct(&mut registry).map_err(|e| anyhow::anyhow!("{e}"))?;
        let runtime = ToolRuntime::new(registry, Arc::new(ProfilePolicy));
        let execd = match spawn_execd(data_dir) {
            Ok(e) => Some(e),
            Err(e) => {
                eprintln!(
                    "modbit-core: terminal broker unavailable: {e:#}; shell tools will report INFRA_FAILURE"
                );
                None
            }
        };
        Ok(Self {
            runtime,
            execd,
            workspaces: Mutex::new(HashMap::new()),
            state_dir: data_dir.join("workspaces"),
        })
    }

    pub(crate) async fn workspace(
        &self,
        root: &str,
    ) -> Result<(Arc<Mutex<WorkspaceService>>, PathBuf)> {
        let canonical = Path::new(root)
            .canonicalize()
            .with_context(|| format!("workspace root `{root}`"))?;
        let mut map = self.workspaces.lock().await;
        if let Some(ws) = map.get(&canonical) {
            return Ok((Arc::clone(ws), canonical));
        }
        let ws = Arc::new(Mutex::new(
            WorkspaceService::open(&canonical, &self.state_dir, &[])
                .map_err(|e| anyhow::anyhow!("{e}"))?,
        ));
        map.insert(canonical.clone(), Arc::clone(&ws));
        Ok((ws, canonical))
    }

    /// Run one call through the pipeline with the Capability Kernel bound to
    /// the task's lease, the call's approval and the session state; record
    /// the ToolCall trail (and, for protected effects, a receipt) on the log.
    ///
    /// `existing` is the projection of a call re-entering the pipeline after
    /// an approval (same `tool_call_id`, same intent hash).
    /// The tool surface compiled from support × policy (REQ-EV-0096/0133/0044):
    /// a tool is advertised only when its host consumer exists (the terminal
    /// broker for shell-backed tools), the profile admits it and, given a
    /// lease, the kernel would not deny it outright. Never a dead tool.
    pub(crate) fn visible_specs(
        &self,
        profile: Option<&str>,
        lease: Option<&CapabilityLease>,
    ) -> Vec<ToolSpec> {
        let kernel = CapabilityKernel::default();
        self.runtime
            .registry()
            .specs()
            .into_iter()
            .filter(|s| {
                if s.required_capabilities.iter().any(|c| c == "shell.exec") && self.execd.is_none()
                {
                    return false;
                }
                let Some(p) = profile else {
                    return true;
                };
                if !s.execution_profiles.iter().any(|x| x == p) {
                    return false;
                }
                match lease {
                    Some(l) => !matches!(
                        kernel.decide(&KernelRequest {
                            tool_name: &s.name,
                            effect_class: s.effect_class,
                            required_capabilities: &s.required_capabilities,
                            execution_profile: p,
                            lease: Some(l),
                            approval: None,
                            intent_hash: "",
                            config: None,
                            emergency_stopped: false,
                            now: modbit_domain::Timestamp::now(),
                        }),
                        KernelDecision::Deny { .. }
                    ),
                    None => true,
                }
            })
            .collect()
    }

    pub async fn invoke(
        &self,
        store: &Mutex<EventStore>,
        call: InvokeRequest<'_>,
    ) -> Result<Invoked> {
        let InvokeRequest {
            tenant_id,
            session_id,
            task_id,
            workspace_root,
            execution_profile,
            tool_call_id,
            tool_name,
            arguments_json,
            output_budget_bytes,
            actor,
            lease,
            approval,
            emergency_stopped,
            existing,
        } = call;
        let (workspace, root) = match &workspace_root {
            Some(r) => {
                let (ws, canonical) = self.workspace(r).await?;
                (Some(ws), Some(canonical))
            }
            None => (None, None),
        };
        let sink: Arc<dyn ObjectSink> = Arc::new(StoreSink(store.lock().await.objects().clone()));
        let lease_id = lease.as_ref().map(|l| l.lease_id);
        let port = KernelPort {
            kernel: CapabilityKernel::default(),
            lease,
            approval,
            emergency_stopped,
        };
        let ctx = InvokeContext {
            task_id,
            execution_profile: execution_profile.to_owned(),
            capability_lease_id: lease_id,
            workspace,
            workspace_root: root.clone(),
            exec: self.execd.as_ref().map(|e| e.target.clone()),
            sink,
            output_budget_bytes,
            kernel: Some(Arc::new(port)),
            tool_call_id: None,
        };
        // REQ-EV-0106: snapshot the write targets so every successful write can
        // land a revision-bound FileChanged event with content and diff refs.
        let change_targets = change_targets(tool_name, arguments_json);
        let (pre_bytes, pre_revision) = match (&ctx.workspace, change_targets.is_empty()) {
            (Some(ws), false) => {
                let ws = ws.lock().await;
                let mut m = HashMap::new();
                for p in &change_targets {
                    m.insert(p.clone(), read_workspace_file(&ws, p));
                }
                (m, ws.revision().number)
            }
            _ => (HashMap::new(), 0),
        };
        let outcome = self
            .runtime
            .invoke(&ctx, tool_call_id, tool_name, arguments_json)
            .await;
        let mut result = outcome.result.clone();
        let mut file_events = Vec::new();
        if result.status == ToolStatus::Success
            && !change_targets.is_empty()
            && let (Some(ws), Some(root)) = (&ctx.workspace, &root)
        {
            let changes = workspace_changes(&result.structured_output);
            let ws = ws.lock().await;
            let objects = store.lock().await.objects().clone();
            file_events = file_changed_events(
                &objects,
                &ws,
                task_id,
                tool_call_id,
                &changes,
                &pre_bytes,
                pre_revision,
                "",
            );
            let _ = root;
        }
        let effect_class = outcome
            .effect_class
            .unwrap_or(modbit_domain::toolcall::EffectClass::ReadOnly);
        let now = modbit_domain::Timestamp::now();

        // Evidence: the stage trail becomes ToolCall events on the canonical log.
        let mut events = Vec::new();
        let (expected_sequence, prior_state) = match &existing {
            Some(c) => (Some(c.generation), Some(c.state)),
            None => (Some(0), None),
        };
        if existing.is_none() {
            events.push(typed(
                "ToolCallProposed",
                &ToolCallEvent::ToolCallProposed {
                    task_id,
                    step_id: None,
                    tool_name: tool_name.to_owned(),
                    tool_version: outcome.tool_version.clone().unwrap_or_default(),
                    effect_class,
                    capability_lease_id: lease_id,
                    arguments_hash: result.arguments_hash.clone(),
                },
                actor.clone(),
            ));
        }
        let mut approval_id: Option<ApprovalId> = None;
        let mut approval_events: Option<(ApprovalId, Vec<NewEvent>)> = None;
        let mut receipt: Option<EffectReceipt> = None;
        match result.status {
            ToolStatus::InvalidArguments | ToolStatus::UnknownTool => {
                events.push(typed(
                    "ToolCallFailed",
                    &ToolCallEvent::ToolCallFailed {
                        failure_code: result.error_code.clone().unwrap_or_default(),
                        result_ref: None,
                    },
                    actor.clone(),
                ));
            }
            ToolStatus::ApprovalPending => {
                let (reason, scope_json) = match &outcome.policy {
                    Some(PolicyDecision::ApprovalRequired { reason, scope_json }) => {
                        (reason.clone(), scope_json.clone())
                    }
                    _ => (
                        result.error_message.clone().unwrap_or_default(),
                        "{}".into(),
                    ),
                };
                if prior_state == Some(ToolCallState::ApprovalPending) {
                    // Still waiting: nothing new on the log; report the open approval.
                    approval_id = existing.as_ref().and_then(|c| c.approval_id);
                    events.clear();
                } else {
                    let id = ApprovalId::new();
                    approval_id = Some(id);
                    events.push(typed(
                        "ToolCallValidated",
                        &ToolCallEvent::ToolCallValidated,
                        actor.clone(),
                    ));
                    events.push(typed(
                        "ToolCallApprovalRequested",
                        &ToolCallEvent::ToolCallApprovalRequested {
                            approval_id: id,
                            decision: format!("APPROVAL_REQUIRED: {reason}"),
                        },
                        actor.clone(),
                    ));
                    approval_events = Some((
                        id,
                        vec![typed(
                            "ApprovalRequested",
                            &ApprovalEvent::ApprovalRequested {
                                task_id,
                                tool_call_id,
                                tool_name: tool_name.to_owned(),
                                effect_class,
                                intent_hash: result.arguments_hash.clone(),
                                scope_json,
                                expires_at: Some(modbit_domain::Timestamp(now.0 + APPROVAL_TTL_MS)),
                            },
                            actor.clone(),
                        )],
                    ));
                }
            }
            ToolStatus::PolicyDenied => {
                let decision = match &outcome.policy {
                    Some(PolicyDecision::Deny { code, reason, .. }) => format!("{code}: {reason}"),
                    _ => result.error_message.clone().unwrap_or_default(),
                };
                if prior_state != Some(ToolCallState::ApprovalPending) {
                    events.push(typed(
                        "ToolCallValidated",
                        &ToolCallEvent::ToolCallValidated,
                        actor.clone(),
                    ));
                }
                events.push(typed(
                    "ToolCallPolicyDecision",
                    &ToolCallEvent::ToolCallPolicyDecision {
                        allowed: false,
                        decision,
                        approval_required: false,
                    },
                    actor.clone(),
                ));
            }
            _ => {
                let (decision, used_approval) = match &outcome.policy {
                    Some(PolicyDecision::Allow { rule, approval_id }) => (
                        rule.clone(),
                        approval_id.as_ref().and_then(|a| ApprovalId::parse(a).ok()),
                    ),
                    other => (format!("{other:?}"), None),
                };
                approval_id = used_approval;
                if prior_state != Some(ToolCallState::ApprovalPending) {
                    events.push(typed(
                        "ToolCallValidated",
                        &ToolCallEvent::ToolCallValidated,
                        actor.clone(),
                    ));
                }
                events.push(typed(
                    "ToolCallPolicyDecision",
                    &ToolCallEvent::ToolCallPolicyDecision {
                        allowed: true,
                        decision: decision.clone(),
                        approval_required: false,
                    },
                    actor.clone(),
                ));
                events.push(typed(
                    "ToolCallDispatched",
                    &ToolCallEvent::ToolCallDispatched,
                    actor.clone(),
                ));
                // Protected/external/destructive effects get a receipt in the chain.
                if effect_class >= modbit_domain::toolcall::EffectClass::ProtectedWrite {
                    let effect_id = modbit_domain::EffectId::new();
                    result.effect_receipt_ids.push(effect_id.to_string());
                    receipt = Some(EffectReceipt {
                        effect_id,
                        previous_receipt_hash: None,
                        task_id,
                        tool_call_id,
                        capability_lease_id: lease_id,
                        intent_hash: result.arguments_hash.clone(),
                        policy_decision: decision,
                        approval_id: used_approval,
                        execution_target: root
                            .as_ref()
                            .map(|r| format!("local:{}", r.display()))
                            .unwrap_or_else(|| "local".into()),
                        evidence_ref: None,
                        status: format!("{:?}", result.status).to_uppercase(),
                        occurred_at: now,
                        receipt_hash: String::new(),
                    });
                }
            }
        }
        let result_json = serde_json::to_vec(&result)?;
        let result_ref = store.lock().await.objects().put(&result_json)?;
        if let Some(mut r) = receipt.take() {
            r.evidence_ref = Some(result_ref.clone());
            r.previous_receipt_hash = store.lock().await.last_receipt_hash()?;
            let sealed = modbit_policy::ledger::seal(r);
            events.push(typed(
                "EffectReceiptAppended",
                &ToolCallEvent::EffectReceiptAppended { receipt: sealed },
                actor.clone(),
            ));
        }
        if matches!(
            result.status,
            ToolStatus::Success
                | ToolStatus::ApplicationFailure
                | ToolStatus::InfraFailure
                | ToolStatus::Cancelled
                | ToolStatus::UnknownOutcome
        ) {
            match result.status {
                ToolStatus::Success => events.push(typed(
                    "ToolCallSucceeded",
                    &ToolCallEvent::ToolCallSucceeded {
                        result_ref: result_ref.clone(),
                    },
                    actor.clone(),
                )),
                ToolStatus::UnknownOutcome => events.push(typed(
                    "ToolCallUnknownOutcome",
                    &ToolCallEvent::ToolCallUnknownOutcome {
                        reason: result.error_message.clone().unwrap_or_default(),
                    },
                    actor.clone(),
                )),
                ToolStatus::Cancelled => events.push(typed(
                    "ToolCallCancelled",
                    &ToolCallEvent::ToolCallCancelled,
                    actor.clone(),
                )),
                _ => events.push(typed(
                    "ToolCallFailed",
                    &ToolCallEvent::ToolCallFailed {
                        failure_code: result.error_code.clone().unwrap_or_default(),
                        result_ref: Some(result_ref.clone()),
                    },
                    actor.clone(),
                )),
            }
        } else if matches!(
            result.status,
            ToolStatus::InvalidArguments | ToolStatus::UnknownTool
        ) {
            // Attach the diagnostics object to the failure recorded above.
            if let Some(NewEvent { payload, .. }) = events.last_mut()
                && let Some(obj) = payload.as_object_mut()
            {
                obj.insert(
                    "result_ref".into(),
                    serde_json::Value::String(result_ref.clone()),
                );
            }
        }
        if !events.is_empty() {
            let mut st = store.lock().await;
            st.append(AppendRequest {
                tenant_id,
                session_id,
                task_id: Some(task_id),
                run_id: None,
                turn_id: None,
                step_id: None,
                aggregate_type: AggregateType::ToolCall,
                aggregate_id: *tool_call_id.as_bytes(),
                expected_sequence,
                events,
            })?;
            if let Some((id, evs)) = approval_events {
                st.append(AppendRequest {
                    tenant_id,
                    session_id,
                    task_id: Some(task_id),
                    run_id: None,
                    turn_id: None,
                    step_id: None,
                    aggregate_type: AggregateType::Approval,
                    aggregate_id: *id.as_bytes(),
                    expected_sequence: Some(0),
                    events: evs,
                })?;
            }
            if !file_events.is_empty()
                && let Some(root) = &root
            {
                append_file_events(
                    &mut st,
                    tenant_id,
                    session_id,
                    task_id,
                    &root.to_string_lossy(),
                    file_events,
                )?;
            }
        }
        Ok(Invoked {
            result,
            result_ref,
            approval_id,
        })
    }
}

/// Approvals expire a day after they are requested (docs/23: approval binds expiry).
const APPROVAL_TTL_MS: i64 = 24 * 60 * 60 * 1000;

/// One invocation as the Core sees it.
pub struct InvokeRequest<'a> {
    /// Tenant.
    pub tenant_id: TenantId,
    /// Session.
    pub session_id: SessionId,
    /// Task.
    pub task_id: TaskId,
    /// Approved workspace root.
    pub workspace_root: Option<String>,
    /// Execution profile.
    pub execution_profile: &'a str,
    /// Client-stable call id.
    pub tool_call_id: ToolCallId,
    /// Tool.
    pub tool_name: &'a str,
    /// Arguments.
    pub arguments_json: &'a str,
    /// Inline budget.
    pub output_budget_bytes: u64,
    /// Actor.
    pub actor: Actor,
    /// The task's current lease.
    pub lease: Option<CapabilityLease>,
    /// Approval bound to this call, if any.
    pub approval: Option<Approval>,
    /// Session emergency stop.
    pub emergency_stopped: bool,
    /// Existing projection when the call re-enters after an approval.
    pub existing: Option<ToolCall>,
}

/// What an invocation produced.
pub struct Invoked {
    /// Result.
    pub result: modbit_tools::ToolCallResult,
    /// Object hash of the result JSON.
    pub result_ref: String,
    /// Approval opened or consumed.
    pub approval_id: Option<ApprovalId>,
}

/// Per-call adapter from the pipeline port to the Capability Kernel.
struct KernelPort {
    kernel: CapabilityKernel,
    lease: Option<CapabilityLease>,
    approval: Option<Approval>,
    emergency_stopped: bool,
}

impl CapabilityPort for KernelPort {
    fn decide(&self, req: &PolicyRequest) -> PolicyDecision {
        let d = self.kernel.decide(&KernelRequest {
            tool_name: &req.tool_name,
            effect_class: req.effect_class,
            required_capabilities: &req.required_capabilities,
            execution_profile: &req.execution_profile,
            lease: self.lease.as_ref(),
            approval: self.approval.as_ref(),
            intent_hash: &req.intent_hash,
            config: None,
            emergency_stopped: self.emergency_stopped,
            now: modbit_domain::Timestamp::now(),
        });
        match d {
            KernelDecision::Allow { rule, approval_id } => PolicyDecision::Allow {
                rule,
                approval_id: approval_id.map(|a| a.to_string()),
            },
            KernelDecision::Deny { code, reason } => PolicyDecision::Deny {
                code,
                reason,
                approval_required: false,
            },
            KernelDecision::ApprovalRequired { reason, scope_json } => {
                PolicyDecision::ApprovalRequired { reason, scope_json }
            }
        }
    }
}

fn typed<E: serde::Serialize>(event_type: &str, e: &E, actor: Actor) -> NewEvent {
    let mut ev = NewEvent::new(
        event_type,
        serde_json::to_value(e).expect("serializable"),
        actor,
    );
    ev.occurred_at = Some(modbit_domain::Timestamp::now());
    ev
}

/// Root-relative paths a write tool targets (for the pre-write snapshot).
fn change_targets(tool_name: &str, arguments_json: &str) -> Vec<String> {
    let Ok(v) = serde_json::from_str::<serde_json::Value>(arguments_json) else {
        return vec![];
    };
    match tool_name {
        "change.apply" => v
            .get("path")
            .and_then(serde_json::Value::as_str)
            .map(|p| vec![p.to_owned()])
            .unwrap_or_default(),
        "change.batch" => v
            .get("ops")
            .and_then(serde_json::Value::as_array)
            .map(|ops| {
                ops.iter()
                    .filter_map(|o| o.get("path").and_then(serde_json::Value::as_str))
                    .map(str::to_owned)
                    .collect()
            })
            .unwrap_or_default(),
        _ => vec![],
    }
}

/// Current bytes of a workspace file (`None` when absent or unreadable).
pub(crate) fn read_workspace_file(ws: &WorkspaceService, path: &str) -> Option<Vec<u8>> {
    let r = ws.resolve(path).ok()?;
    std::fs::read(&r.absolute).ok()
}

/// The typed change records a write tool reported (`change` or `changes`).
pub(crate) fn workspace_changes(
    structured_output: &serde_json::Value,
) -> Vec<modbit_workspace::WorkspaceChange> {
    let mut out = Vec::new();
    if let Some(c) = structured_output.get("change")
        && let Ok(c) = serde_json::from_value(c.clone())
    {
        out.push(c);
    }
    if let Some(cs) = structured_output
        .get("changes")
        .and_then(serde_json::Value::as_array)
    {
        out.extend(
            cs.iter()
                .filter_map(|c| serde_json::from_value(c.clone()).ok()),
        );
    }
    out
}

/// Workspace aggregate id: the worktree id digest (stable per root).
pub(crate) fn workspace_aggregate_id(root: &str) -> [u8; 16] {
    use sha2::{Digest, Sha256};
    // Canonical and free of the Windows verbatim prefix, so the tool host
    // (canonical root) and the surface commands (the task's stored root)
    // address one aggregate.
    let canon = std::fs::canonicalize(root).unwrap_or_else(|_| PathBuf::from(root));
    let canon = canon.to_string_lossy().into_owned();
    let canon = canon
        .trim_start_matches(r"\\?\")
        .trim_start_matches("//?/")
        .to_owned();
    let id = modbit_workspace::WorkspaceRevision::worktree_id_for(Path::new(&canon));
    let h = Sha256::digest(id.as_bytes());
    let mut out = [0u8; 16];
    out.copy_from_slice(&h[..16]);
    out
}

/// Build `FileChanged` events for the change records of one tool call:
/// before/after content and a unified diff stored by reference, never bytes.
#[allow(clippy::too_many_arguments)]
pub(crate) fn file_changed_events(
    objects: &ObjectStore,
    ws: &WorkspaceService,
    task_id: TaskId,
    tool_call_id: ToolCallId,
    changes: &[modbit_workspace::WorkspaceChange],
    pre_bytes: &HashMap<String, Option<Vec<u8>>>,
    pre_revision: u64,
    op_prefix: &str,
) -> Vec<NewEvent> {
    use modbit_domain::workspace::WorkspaceEvent;
    let mut previous = pre_revision;
    let mut events = Vec::new();
    for c in changes {
        let before = pre_bytes.get(&c.path).cloned().flatten();
        let before = before.filter(|_| c.before_hash.is_some());
        let after = if c.after_hash.is_some() {
            read_workspace_file(ws, &c.path)
        } else {
            None
        };
        let put = |b: &Option<Vec<u8>>| b.as_ref().and_then(|b| objects.put(b).ok());
        let before_ref = put(&before);
        let after_ref = put(&after);
        let diff_ref = match (
            before.as_deref().map(std::str::from_utf8),
            after.as_deref().map(std::str::from_utf8),
        ) {
            (Some(Ok(o)), Some(Ok(n))) => Some(unified_diff(&c.path, o, n)),
            (Some(Ok(o)), None) => Some(unified_diff(&c.path, o, "")),
            (None, Some(Ok(n))) => Some(unified_diff(&c.path, "", n)),
            _ => None,
        }
        .and_then(|d| objects.put(d.as_bytes()).ok());
        events.push(typed(
            "FileChanged",
            &WorkspaceEvent::FileChanged {
                task_id,
                tool_call_id,
                path: c.path.clone(),
                op: format!("{op_prefix}{}", c.op),
                before_hash: c.before_hash.clone(),
                after_hash: c.after_hash.clone(),
                before_ref,
                after_ref,
                diff_ref,
                workspace_revision: c.workspace_revision.number,
                previous_revision: previous,
            },
            Actor::Core("tool-host".into()),
        ));
        previous = c.workspace_revision.number;
    }
    events
}

/// Unified diff (3 lines of context, `a/` `b/` headers) between two texts.
pub(crate) fn unified_diff(path: &str, old: &str, new: &str) -> String {
    similar::TextDiff::from_lines(old, new)
        .unified_diff()
        .context_radius(3)
        .header(&format!("a/{path}"), &format!("b/{path}"))
        .to_string()
}

/// Append `FileChanged` events on the workspace aggregate of `root`.
pub(crate) fn append_file_events(
    st: &mut EventStore,
    tenant_id: TenantId,
    session_id: SessionId,
    task_id: TaskId,
    root: &str,
    events: Vec<NewEvent>,
) -> Result<()> {
    st.append(AppendRequest {
        tenant_id,
        session_id,
        task_id: Some(task_id),
        run_id: None,
        turn_id: None,
        step_id: None,
        aggregate_type: AggregateType::Workspace,
        aggregate_id: workspace_aggregate_id(root),
        expected_sequence: None,
        events,
    })?;
    Ok(())
}
