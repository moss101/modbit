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
use modbit_domain::event::{Actor, AggregateType};
use modbit_domain::toolcall::ToolCallEvent;
use modbit_domain::{SessionId, TaskId, TenantId, ToolCallId};
use modbit_event_store::{AppendRequest, EventStore, NewEvent, ObjectStore};
use modbit_protocol::local::{ReadyLine, decode_hex};
use modbit_tools::pipeline::ExecTarget;
use modbit_tools::{
    InvokeContext, ObjectSink, PolicyDecision, ProfilePolicy, ToolRegistry, ToolRuntime, ToolStatus,
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

    async fn workspace(&self, root: &str) -> Result<(Arc<Mutex<WorkspaceService>>, PathBuf)> {
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

    /// Run one call through the pipeline, recording ToolCall events on the log.
    #[allow(clippy::too_many_arguments)]
    pub async fn invoke(
        &self,
        store: &Mutex<EventStore>,
        tenant_id: TenantId,
        session_id: SessionId,
        task_id: TaskId,
        workspace_root: Option<String>,
        execution_profile: &str,
        tool_call_id: ToolCallId,
        tool_name: &str,
        arguments_json: &str,
        output_budget_bytes: u64,
        actor: Actor,
    ) -> Result<(modbit_tools::ToolCallResult, String)> {
        let (workspace, root) = match &workspace_root {
            Some(r) => {
                let (ws, canonical) = self.workspace(r).await?;
                (Some(ws), Some(canonical))
            }
            None => (None, None),
        };
        let sink: Arc<dyn ObjectSink> = Arc::new(StoreSink(store.lock().await.objects().clone()));
        let ctx = InvokeContext {
            task_id,
            execution_profile: execution_profile.to_owned(),
            capability_lease_id: None,
            workspace,
            workspace_root: root,
            exec: self.execd.as_ref().map(|e| e.target.clone()),
            sink,
            output_budget_bytes,
        };
        let outcome = self
            .runtime
            .invoke(&ctx, tool_call_id, tool_name, arguments_json)
            .await;
        // Evidence: the stage trail becomes ToolCall events on the canonical log.
        let effect_class = outcome
            .effect_class
            .unwrap_or(modbit_domain::toolcall::EffectClass::ReadOnly);
        let mut events = vec![typed(
            "ToolCallProposed",
            &ToolCallEvent::ToolCallProposed {
                task_id,
                step_id: None,
                tool_name: tool_name.to_owned(),
                tool_version: outcome.tool_version.clone().unwrap_or_default(),
                effect_class,
                capability_lease_id: None,
                arguments_hash: outcome.result.arguments_hash.clone(),
            },
            actor.clone(),
        )];
        let result_json = serde_json::to_vec(&outcome.result)?;
        let result_ref = store.lock().await.objects().put(&result_json)?;
        match outcome.result.status {
            ToolStatus::InvalidArguments | ToolStatus::UnknownTool => {
                events.push(typed(
                    "ToolCallFailed",
                    &ToolCallEvent::ToolCallFailed {
                        failure_code: outcome.result.error_code.clone().unwrap_or_default(),
                        result_ref: Some(result_ref.clone()),
                    },
                    actor.clone(),
                ));
            }
            ToolStatus::PolicyDenied => {
                let (decision, approval_required) = match &outcome.policy {
                    Some(PolicyDecision::Deny {
                        code,
                        reason,
                        approval_required,
                    }) => (format!("{code}: {reason}"), *approval_required),
                    _ => (
                        outcome.result.error_message.clone().unwrap_or_default(),
                        false,
                    ),
                };
                events.push(typed(
                    "ToolCallValidated",
                    &ToolCallEvent::ToolCallValidated,
                    actor.clone(),
                ));
                events.push(typed(
                    "ToolCallPolicyDecision",
                    &ToolCallEvent::ToolCallPolicyDecision {
                        allowed: false,
                        decision,
                        approval_required,
                    },
                    actor.clone(),
                ));
                if approval_required {
                    // ApprovalPending stays open for M2.5; record the failure of this attempt as evidence via result_ref only.
                } else {
                    // Denied outright: the aggregate is already Failed by the decision event.
                }
            }
            _ => {
                let decision = match &outcome.policy {
                    Some(PolicyDecision::Allow { rule }) => rule.clone(),
                    other => format!("{other:?}"),
                };
                events.push(typed(
                    "ToolCallValidated",
                    &ToolCallEvent::ToolCallValidated,
                    actor.clone(),
                ));
                events.push(typed(
                    "ToolCallPolicyDecision",
                    &ToolCallEvent::ToolCallPolicyDecision {
                        allowed: true,
                        decision,
                        approval_required: false,
                    },
                    actor.clone(),
                ));
                events.push(typed(
                    "ToolCallDispatched",
                    &ToolCallEvent::ToolCallDispatched,
                    actor.clone(),
                ));
                match outcome.result.status {
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
                            reason: outcome.result.error_message.clone().unwrap_or_default(),
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
                            failure_code: outcome.result.error_code.clone().unwrap_or_default(),
                            result_ref: Some(result_ref.clone()),
                        },
                        actor.clone(),
                    )),
                }
            }
        }
        let req = AppendRequest {
            tenant_id,
            session_id,
            task_id: Some(task_id),
            run_id: None,
            turn_id: None,
            step_id: None,
            aggregate_type: AggregateType::ToolCall,
            aggregate_id: *tool_call_id.as_bytes(),
            expected_sequence: Some(0),
            events,
        };
        store.lock().await.append(req)?;
        Ok((outcome.result, result_ref))
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
