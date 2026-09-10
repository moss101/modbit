//! The canonical invocation pipeline (docs/16 "V2 canonical inventory":
//! `ToolCallNormalizer → Capability Kernel → effector → typed result →
//! Evidence`), REQ-EV-0239 monotonic guards, REQ-EV-0269 OutputRef paging.

use std::path::PathBuf;
use std::sync::Arc;

use modbit_domain::{CapabilityLeaseId, TaskId, ToolCallId};
use modbit_workspace::WorkspaceService;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use tokio::sync::Mutex;

use crate::policy::{CapabilityPort, PolicyDecision, PolicyRequest};
use crate::registry::{ToolOutcome, ToolRegistry};

/// Where large results go (content-addressed; the event store's object
/// directory in the Core).
pub trait ObjectSink: Send + Sync {
    /// Store bytes, returning the sha256 hex.
    fn put(&self, bytes: &[u8]) -> crate::Result<String>;
}

/// A search request over the workspace index (docs/18 L0).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SearchRequest {
    /// `exact` | `regex` | `paths`.
    pub kind: String,
    /// Needle, pattern or glob.
    pub query: String,
    /// Case-insensitive text search.
    pub case_insensitive: bool,
    /// Path glob restricting text hits.
    pub path_glob: Option<String>,
    /// Total hit ceiling.
    pub max_hits: usize,
}

/// Port to the host's retrieval index; results are JSON the tool bounds.
pub trait SearchPort: Send + Sync {
    /// Run the search; `Err` carries a typed code and message.
    fn search(&self, req: &SearchRequest) -> Result<serde_json::Value, (String, String)>;
}

/// A language-service request (docs/18 "Semantic language services").
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LanguageRequest {
    /// `diagnostics` | `symbols` | `references` | `definition`.
    pub kind: String,
    /// Root-relative path.
    pub path: String,
    /// Zero-based line (references/definition).
    pub line: u32,
    /// Zero-based UTF-16 column (references/definition).
    pub character: u32,
}

/// Port to the host's headless language servers; results are JSON.
pub trait LanguageServicePort: Send + Sync {
    /// Run the request; `Err` carries a typed code and message.
    fn query(&self, req: &LanguageRequest) -> Result<serde_json::Value, (String, String)>;
}

/// Where a shell tool runs.
#[derive(Clone, Debug)]
pub struct ExecTarget {
    /// Broker endpoint.
    pub endpoint: modbit_protocol::local::Endpoint,
    /// Boot secret.
    pub boot_secret: Vec<u8>,
}

/// Everything a tool may touch during one invocation.
#[derive(Clone)]
pub struct InvokeContext {
    /// Task.
    pub task_id: TaskId,
    /// Execution profile.
    pub execution_profile: String,
    /// Lease presented (verified by the kernel, M2.5).
    pub capability_lease_id: Option<CapabilityLeaseId>,
    /// Workspace service for the task's approved root, when any.
    pub workspace: Option<Arc<Mutex<WorkspaceService>>>,
    /// Canonical root path (for git and process cwd).
    pub workspace_root: Option<PathBuf>,
    /// Broker for processes.
    pub exec: Option<ExecTarget>,
    /// Object sink.
    pub sink: Arc<dyn ObjectSink>,
    /// Inline output budget for this call.
    pub output_budget_bytes: u64,
    /// Per-call kernel adapter overriding the runtime's default policy (the
    /// Core binds the task lease, the call's approval and the session state).
    pub kernel: Option<Arc<dyn CapabilityPort>>,
    /// Workspace search (the retrieval index), when the host provides one (M3.1).
    pub search: Option<Arc<dyn SearchPort>>,
    /// Headless language services, when the host provides them (M3.4).
    pub language: Option<Arc<dyn LanguageServicePort>>,
    /// The call being executed (set by the pipeline; effectors derive their
    /// idempotency keys from it so a new call never replays an old one).
    pub tool_call_id: Option<ToolCallId>,
}

/// Status of a tool call result (docs/30 `ToolCallResult.status` plus the
/// pre-execution outcomes).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ToolStatus {
    /// Effector succeeded.
    Success,
    /// Effector ran; the application reported failure (e.g. non-zero exit).
    ApplicationFailure,
    /// Transport / effector unavailable.
    InfraFailure,
    /// Cancelled.
    Cancelled,
    /// Effect may have happened; reconcile before retry.
    UnknownOutcome,
    /// Policy denied before any effect.
    PolicyDenied,
    /// Policy needs an approval bound to this intent before any effect.
    ApprovalPending,
    /// Arguments failed normalization or schema validation before any effect.
    InvalidArguments,
    /// Unknown tool.
    UnknownTool,
}

/// docs/30 `ToolCallResult`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolCallResult {
    /// Call id.
    pub tool_call_id: ToolCallId,
    /// Tool.
    pub tool_name: String,
    /// Status.
    pub status: ToolStatus,
    /// Structured output (bounded; large outputs are replaced by an OutputRef descriptor).
    pub structured_output: Value,
    /// Spilled stdout object hash.
    pub stdout_ref: Option<String>,
    /// Spilled stderr object hash.
    pub stderr_ref: Option<String>,
    /// Artifacts produced.
    pub produced_artifact_ids: Vec<String>,
    /// Effect receipts (M9).
    pub effect_receipt_ids: Vec<String>,
    /// Workspace revision after.
    pub workspace_revision_after: Option<u64>,
    /// Error code.
    pub error_code: Option<String>,
    /// Error message.
    pub error_message: Option<String>,
    /// sha256 of the normalized arguments.
    pub arguments_hash: String,
}

/// One pipeline stage record (evidence).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StageRecord {
    /// Stage name.
    pub stage: String,
    /// Outcome text.
    pub outcome: String,
}

/// Full outcome: the result plus the stage trail the Core turns into events.
#[derive(Clone, Debug)]
pub struct PipelineOutcome {
    /// Result.
    pub result: ToolCallResult,
    /// Stages in order.
    pub stages: Vec<StageRecord>,
    /// Policy decision (when reached).
    pub policy: Option<PolicyDecision>,
    /// Effect class of the tool (when known).
    pub effect_class: Option<crate::EffectClass>,
    /// Tool version (when known).
    pub tool_version: Option<String>,
}

/// A monotonic verdict: it can only tighten (REQ-EV-0239).
#[derive(Debug)]
struct Verdict {
    denied: Option<(String, String)>,
}

impl Verdict {
    fn deny(&mut self, code: &str, reason: &str) {
        if self.denied.is_none() {
            self.denied = Some((code.into(), reason.into()));
        }
    }
}

/// Canonical JSON: keys sorted, no whitespace.
fn canonical(v: &Value) -> String {
    fn sort(v: &Value) -> Value {
        match v {
            Value::Object(m) => Value::Object(
                m.iter()
                    .map(|(k, v)| (k.clone(), sort(v)))
                    .collect::<serde_json::Map<_, _>>(),
            ),
            Value::Array(a) => Value::Array(a.iter().map(sort).collect()),
            other => other.clone(),
        }
    }
    sort(v).to_string()
}

/// sha256 of the canonical JSON of `arguments_json` (the intent hash an
/// approval binds), or `None` when the text is not a JSON object.
#[must_use]
pub fn arguments_hash(arguments_json: &str) -> Option<String> {
    let v: Value = serde_json::from_str(arguments_json).ok()?;
    if !v.is_object() {
        return None;
    }
    let mut h = Sha256::new();
    h.update(canonical(&v).as_bytes());
    Some(hex::encode(h.finalize()))
}

/// Runtime: registry + kernel port.
pub struct ToolRuntime {
    registry: ToolRegistry,
    policy: Arc<dyn CapabilityPort>,
}

impl ToolRuntime {
    /// Build.
    pub fn new(registry: ToolRegistry, policy: Arc<dyn CapabilityPort>) -> Self {
        Self { registry, policy }
    }

    /// Registry.
    #[must_use]
    pub fn registry(&self) -> &ToolRegistry {
        &self.registry
    }

    /// Run the whole pipeline for one call.
    pub async fn invoke(
        &self,
        ctx: &InvokeContext,
        tool_call_id: ToolCallId,
        tool_name: &str,
        arguments_json: &str,
    ) -> PipelineOutcome {
        let mut stages = Vec::new();
        let mut verdict = Verdict { denied: None };
        let base = |status: ToolStatus,
                    args_hash: &str,
                    code: Option<String>,
                    msg: Option<String>| ToolCallResult {
            tool_call_id,
            tool_name: tool_name.to_owned(),
            status,
            structured_output: Value::Null,
            stdout_ref: None,
            stderr_ref: None,
            produced_artifact_ids: vec![],
            effect_receipt_ids: vec![],
            workspace_revision_after: None,
            error_code: code,
            error_message: msg,
            arguments_hash: args_hash.to_owned(),
        };

        // 1. normalize
        let args: Value = match serde_json::from_str::<Value>(arguments_json) {
            Ok(v) if v.is_object() => v,
            Ok(_) => {
                stages.push(StageRecord {
                    stage: "normalize".into(),
                    outcome: "arguments must be a JSON object".into(),
                });
                return PipelineOutcome {
                    result: base(
                        ToolStatus::InvalidArguments,
                        "",
                        Some("ARGUMENTS_NOT_OBJECT".into()),
                        Some("arguments must be a JSON object".into()),
                    ),
                    stages,
                    policy: None,
                    effect_class: None,
                    tool_version: None,
                };
            }
            Err(e) => {
                stages.push(StageRecord {
                    stage: "normalize".into(),
                    outcome: format!("invalid JSON: {e}"),
                });
                return PipelineOutcome {
                    result: base(
                        ToolStatus::InvalidArguments,
                        "",
                        Some("ARGUMENTS_NOT_JSON".into()),
                        Some(e.to_string()),
                    ),
                    stages,
                    policy: None,
                    effect_class: None,
                    tool_version: None,
                };
            }
        };
        let canon = canonical(&args);
        let args_hash = hex::encode(Sha256::digest(canon.as_bytes()));
        stages.push(StageRecord {
            stage: "normalize".into(),
            outcome: format!("sha256:{args_hash}"),
        });

        let Some(tool) = self.registry.get(tool_name) else {
            stages.push(StageRecord {
                stage: "resolve".into(),
                outcome: "unknown tool".into(),
            });
            return PipelineOutcome {
                result: base(
                    ToolStatus::UnknownTool,
                    &args_hash,
                    Some("UNKNOWN_TOOL".into()),
                    Some(format!("`{tool_name}` is not registered")),
                ),
                stages,
                policy: None,
                effect_class: None,
                tool_version: None,
            };
        };
        let spec = tool.spec().clone();

        // 2. validate
        let validator = jsonschema::validator_for(&spec.input_schema)
            .expect("schema validated at registration");
        let errors: Vec<String> = validator
            .iter_errors(&args)
            .map(|e| format!("{} at {}", e, e.instance_path()))
            .collect();
        if !errors.is_empty() {
            stages.push(StageRecord {
                stage: "validate".into(),
                outcome: errors.join("; "),
            });
            return PipelineOutcome {
                result: base(
                    ToolStatus::InvalidArguments,
                    &args_hash,
                    Some("SCHEMA_VIOLATION".into()),
                    Some(errors.join("; ")),
                ),
                stages,
                policy: None,
                effect_class: Some(spec.effect_class),
                tool_version: Some(spec.version.clone()),
            };
        }
        stages.push(StageRecord {
            stage: "validate".into(),
            outcome: "ok".into(),
        });

        // 3. policy (arguments are never shown to the kernel as text)
        let port: &dyn CapabilityPort = match &ctx.kernel {
            Some(k) => k.as_ref(),
            None => self.policy.as_ref(),
        };
        let decision = port.decide(&PolicyRequest {
            tool_name: spec.name.clone(),
            effect_class: spec.effect_class,
            required_capabilities: spec.required_capabilities.clone(),
            execution_profile: ctx.execution_profile.clone(),
            has_workspace: ctx.workspace.is_some(),
            has_lease: ctx.capability_lease_id.is_some(),
            intent_hash: args_hash.clone(),
        });
        if !spec
            .execution_profiles
            .iter()
            .any(|p| p == &ctx.execution_profile)
        {
            verdict.deny(
                "PROFILE_NOT_ALLOWED",
                &format!(
                    "tool `{}` does not run under `{}`",
                    spec.name, ctx.execution_profile
                ),
            );
        }
        if let PolicyDecision::Deny { code, reason, .. } = &decision {
            verdict.deny(code, reason);
        }
        stages.push(StageRecord {
            stage: "policy".into(),
            outcome: format!("{decision:?}"),
        });
        // Approval needed: no effect; the Core opens the approval and the same
        // tool_call_id re-enters the pipeline once it is resolved.
        if let PolicyDecision::ApprovalRequired { reason, .. } = &decision
            && verdict.denied.is_none()
        {
            return PipelineOutcome {
                result: base(
                    ToolStatus::ApprovalPending,
                    &args_hash,
                    Some("APPROVAL_REQUIRED".into()),
                    Some(reason.clone()),
                ),
                stages,
                policy: Some(decision),
                effect_class: Some(spec.effect_class),
                tool_version: Some(spec.version.clone()),
            };
        }
        // Monotonic guard: nothing after this point may clear a denial.
        if let Some((code, reason)) = &verdict.denied {
            return PipelineOutcome {
                result: base(
                    ToolStatus::PolicyDenied,
                    &args_hash,
                    Some(code.clone()),
                    Some(reason.clone()),
                ),
                stages,
                policy: Some(decision),
                effect_class: Some(spec.effect_class),
                tool_version: Some(spec.version.clone()),
            };
        }

        // 4. execute against the real effector
        let mut call_ctx = ctx.clone();
        call_ctx.tool_call_id = Some(tool_call_id);
        let outcome: ToolOutcome = tool.invoke(&call_ctx, args).await;
        stages.push(StageRecord {
            stage: "execute".into(),
            outcome: if outcome.ok {
                "ok".into()
            } else {
                outcome
                    .error_code
                    .clone()
                    .unwrap_or_else(|| "failed".into())
            },
        });

        // 5. postprocess: bounded inline view, complete bytes by OutputRef (docs/16 "Large results")
        let budget = if ctx.output_budget_bytes > 0 {
            ctx.output_budget_bytes as usize
        } else {
            spec.output_budget_bytes as usize
        };
        let mut structured = outcome.structured_output.clone();
        let text = structured.to_string();
        if text.len() > budget {
            match ctx.sink.put(text.as_bytes()) {
                Ok(hash) => {
                    let preview: String = text.chars().take(budget.min(2048)).collect();
                    structured = json!({ "output_ref": hash, "byte_length": text.len(), "mime": "application/json", "preview": preview, "truncated": true });
                }
                Err(e) => stages.push(StageRecord {
                    stage: "postprocess".into(),
                    outcome: format!("spill failed: {e}"),
                }),
            }
        }
        let spill = |bytes: &Option<Vec<u8>>, stages: &mut Vec<StageRecord>| -> Option<String> {
            let b = bytes.as_ref()?;
            match ctx.sink.put(b) {
                Ok(h) => Some(h),
                Err(e) => {
                    stages.push(StageRecord {
                        stage: "postprocess".into(),
                        outcome: format!("spill failed: {e}"),
                    });
                    None
                }
            }
        };
        let stdout_ref = spill(&outcome.stdout, &mut stages);
        let stderr_ref = spill(&outcome.stderr, &mut stages);
        stages.push(StageRecord {
            stage: "postprocess".into(),
            outcome: format!(
                "inline {} bytes; stdout_ref={stdout_ref:?}",
                structured.to_string().len()
            ),
        });

        let status = if let Some(reason) = &outcome.unknown_outcome {
            let _ = reason;
            ToolStatus::UnknownOutcome
        } else if outcome.ok {
            ToolStatus::Success
        } else if outcome.infra_failure {
            ToolStatus::InfraFailure
        } else {
            ToolStatus::ApplicationFailure
        };
        let result = ToolCallResult {
            tool_call_id,
            tool_name: spec.name.clone(),
            status,
            structured_output: structured,
            stdout_ref,
            stderr_ref,
            produced_artifact_ids: vec![],
            effect_receipt_ids: vec![],
            workspace_revision_after: outcome.workspace_revision_after,
            error_code: outcome.error_code.clone().or_else(|| {
                outcome
                    .unknown_outcome
                    .as_ref()
                    .map(|_| "UNKNOWN_OUTCOME".into())
            }),
            error_message: outcome
                .error_message
                .clone()
                .or(outcome.unknown_outcome.clone()),
            arguments_hash: args_hash,
        };
        PipelineOutcome {
            result,
            stages,
            policy: Some(decision),
            effect_class: Some(spec.effect_class),
            tool_version: Some(spec.version),
        }
    }
}
