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
use crate::registry::{BoxFuture, ToolOutcome, ToolRegistry};

/// Reading back a stored result by range, so a bounded observation can be
/// paged instead of silently truncated (docs/14 harness contract 2).
pub trait ArtifactSource: Send + Sync {
    /// Bytes of `hash` from `offset`, at most `max_bytes`, with the total
    /// size; `Err` carries a typed code and message.
    fn range(
        &self,
        hash: &str,
        offset: u64,
        max_bytes: usize,
    ) -> Result<(Vec<u8>, u64), (String, String)>;
}

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

/// Governed engineering memory the host serves to `memory.query` /
/// `memory.propose` (M9.1, REQ-EV-0162, docs/19). The Core implements it
/// over its durable memory store, resolving the task's scope chain and
/// enforcing the promotion rules; the tools only pass the call's arguments
/// through. A proposal is never promoted here — promotion is a separate
/// governed step (a user or policy action), so `memory.propose` can never
/// make durable memory on its own.
pub trait MemoryPort: Send + Sync {
    /// Curated memory visible to the task, for the query in `args`
    /// (`record_type?`, `topic?`, `limit?`). Never returns a proposal.
    fn query<'a>(
        &'a self,
        args: &'a serde_json::Value,
    ) -> BoxFuture<'a, Result<serde_json::Value, (String, String)>>;
    /// Record a candidate memory item from `args` (`record_type`, `topic`,
    /// `content`, `scope?`, `source?`, `confidence?`, `ttl_ms?`,
    /// `sensitivity?`). It is stored `proposed`; the result carries its id
    /// and says plainly that promotion is separate.
    fn propose<'a>(
        &'a self,
        args: &'a serde_json::Value,
    ) -> BoxFuture<'a, Result<serde_json::Value, (String, String)>>;
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
    /// Diagnostics window: `all` (default) or `changed` — only the lines
    /// changed since the task's baseline for the path (REQ-EV-0070).
    pub window: String,
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
    /// Terminal replay generation the host attaches under (docs/13 "Fencing
    /// and epochs", M4.5): its boot generation, so a restarted Core's reads
    /// supersede the dead one's attachments. 0 = unfenced.
    pub replay_generation: u64,
}

/// What is about to be dispatched (docs/19 layer 2, M4.1): the pipeline
/// hands this to the host's journal right before the effector runs, so the
/// dispatch is on the log before the effect can happen. A Core that dies
/// between the two finds the call `Dispatched` and reconciles it instead of
/// guessing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DispatchRecord {
    /// The call.
    pub tool_call_id: ToolCallId,
    /// Tool.
    pub tool_name: String,
    /// Tool version.
    pub tool_version: String,
    /// Effect class as registered.
    pub effect_class: modbit_domain::toolcall::EffectClass,
    /// sha256 of the normalized arguments.
    pub arguments_hash: String,
    /// The policy decision that allowed the dispatch.
    pub decision: PolicyDecision,
}

/// A boxed future for the journal port (the host's store is async).
pub type JournalFuture<'a> =
    std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), String>> + Send + 'a>>;

/// Write-ahead journal for dispatches. `Err` means the dispatch was not
/// journaled: the pipeline does not run the effector and reports an
/// infrastructure failure (`JOURNAL_FAILED`).
pub trait DispatchJournal: Send + Sync {
    /// Record that `record` is being dispatched now.
    fn dispatching<'a>(&'a self, record: &'a DispatchRecord) -> JournalFuture<'a>;
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
    /// Reads stored results back by range (`artifact.range`).
    pub artifacts: Option<Arc<dyn ArtifactSource>>,
    /// The call being executed (set by the pipeline; effectors derive their
    /// idempotency keys from it so a new call never replays an old one).
    pub tool_call_id: Option<ToolCallId>,
    /// Write-ahead dispatch journal, when the host keeps one (M4.1).
    pub journal: Option<Arc<dyn DispatchJournal>>,
    /// The forge the host lets `forge.*` reach (PX-006): one API host, the
    /// token in the host's custody. `None` = no forge configured.
    pub forge: Option<Arc<crate::forge::ForgeConfig>>,
    /// The host's record of forge effects by idempotency key (PX-006).
    pub forge_ledger: Option<Arc<dyn crate::forge::ForgeLedger>>,
    /// The browser sessions the host holds for tasks (M7.1, docs/22):
    /// `browser.*` reach the task's live Chromium through it. `None` = no
    /// browser host in this build.
    pub browser: Option<Arc<dyn modbit_browser::BrowserPort>>,
    /// The task's sandbox under `cloud_isolated` (M8.5, docs/21): the tools
    /// with a sandbox path act inside it instead of the host's workspace
    /// and broker. `None` = the task runs on the host.
    pub sandbox: Option<Arc<dyn modbit_sandbox::port::SandboxPort>>,
    /// The effect class this call was judged under (set by the pipeline
    /// before the effector runs): a tool whose effect is per call checks
    /// at run time that what it is about to do is not above it.
    pub effect_class: Option<crate::EffectClass>,
    /// Credentials in the host's custody (M7.7, docs/22 "Prompt-injection
    /// isolation"): a call whose arguments carry one of these values is
    /// refused before policy and before any effect
    /// (`SECRET_EXFILTRATION_BLOCKED`) — a secret never leaves the Core
    /// through a tool, whatever asked for it. Held in memory only; never
    /// journaled, printed or compared as anything but a substring.
    pub secrets_in_custody: Vec<String>,
    /// The environment revision the run is pinned to (REQ-EV-0062): the
    /// variables and `PATH` entries every process the tools start gets.
    pub environment: Option<Arc<ProcessEnvironment>>,
    /// Governed engineering memory (M9.1, REQ-EV-0162): `memory.query` reads
    /// curated memory through it and `memory.propose` records a candidate.
    /// `None` = no memory is served to this task.
    pub memory: Option<Arc<dyn MemoryPort>>,
    /// The host's external tool hub (M9.4, REQ-EV-0104/0193, docs/16):
    /// `external.list` / `external.call` / `external.cancel` reach the
    /// task's MCP servers through it, with the transports, the pool and
    /// the bounds on the host's side. `None` = no external tools are
    /// served to this task.
    pub external: Option<Arc<dyn modbit_mcp::McpPort>>,
    /// The run's cancellation (docs/23 "Emergency stop", M9.5): a process a
    /// tool is running when the run is cancelled is cancelled at the broker
    /// — its group is killed — and reported cancelled, instead of running to
    /// its exit or timeout while the Core has already moved on.
    pub cancel: Option<tokio_util::sync::CancellationToken>,
}

/// What a pinned environment revision gives a process.
#[derive(Clone, Debug, Default)]
pub struct ProcessEnvironment {
    /// Variables (a call's own override these).
    pub env: std::collections::BTreeMap<String, String>,
    /// Directories in front of `PATH`.
    pub path: Vec<String>,
}

impl ProcessEnvironment {
    /// A process's variables: the revision's, a call's own on top, and a
    /// `PATH` with the revision's entries in front of the host's (unless
    /// the call set its own).
    #[must_use]
    pub fn apply(
        &self,
        mut env: std::collections::HashMap<String, String>,
    ) -> std::collections::HashMap<String, String> {
        for (k, v) in &self.env {
            env.entry(k.clone()).or_insert_with(|| v.clone());
        }
        if !self.path.is_empty() && !env.contains_key("PATH") {
            let mut full: Vec<std::path::PathBuf> =
                self.path.iter().map(std::path::PathBuf::from).collect();
            if let Ok(host) = std::env::var("PATH") {
                full.extend(std::env::split_paths(&host));
            }
            if let Ok(joined) = std::env::join_paths(full) {
                env.insert("PATH".into(), joined.to_string_lossy().into_owned());
            }
        }
        env
    }
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
/// The JSON path of the first string in `args` containing one of `secrets`
/// (values shorter than 8 bytes are never matched: a short token would
/// match ordinary text).
#[must_use]
pub fn carries_secret(args: &Value, secrets: &[String]) -> Option<String> {
    fn walk(v: &Value, path: &str, secrets: &[String]) -> Option<String> {
        match v {
            Value::String(s) => secrets
                .iter()
                .any(|k| k.len() >= 8 && s.contains(k.as_str()))
                .then(|| path.to_owned()),
            Value::Array(a) => a
                .iter()
                .enumerate()
                .find_map(|(i, x)| walk(x, &format!("{path}[{i}]"), secrets)),
            Value::Object(o) => o.iter().find_map(|(k, x)| {
                walk(
                    x,
                    &if path.is_empty() {
                        k.clone()
                    } else {
                        format!("{path}.{k}")
                    },
                    secrets,
                )
            }),
            _ => None,
        }
    }
    if secrets.iter().all(|k| k.len() < 8) {
        return None;
    }
    walk(args, "", secrets)
}

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

        // The call's effect class: what the tool says of these arguments, never
        // below the registered class (a tool cannot talk its way down).
        let effect_class = tool.effect_of(&args).max(spec.effect_class);

        // M7.7: arguments carrying a credential of the host are refused
        // before the kernel is asked — no approval, no effect, and the
        // record names the refusal, not the value.
        if let Some(field) = carries_secret(&args, &ctx.secrets_in_custody) {
            verdict.deny(
                "SECRET_EXFILTRATION_BLOCKED",
                &format!(
                    "the arguments of `{}` (at `{field}`) carry a credential in the Core's custody; a secret never leaves the Core through a tool call, whatever content asked for it",
                    spec.name
                ),
            );
        }

        // 3. policy (arguments are never shown to the kernel as text)
        let port: &dyn CapabilityPort = match &ctx.kernel {
            Some(k) => k.as_ref(),
            None => self.policy.as_ref(),
        };
        let decision = port.decide(&PolicyRequest {
            tool_name: spec.name.clone(),
            effect_class,
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
                effect_class: Some(effect_class),
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
                effect_class: Some(effect_class),
                tool_version: Some(spec.version.clone()),
            };
        }

        // 4. journal the dispatch before the effect can happen (M4.1): a
        // dispatch that is not on the log does not run.
        if let Some(journal) = &ctx.journal {
            let record = DispatchRecord {
                tool_call_id,
                tool_name: spec.name.clone(),
                tool_version: spec.version.clone(),
                effect_class,
                arguments_hash: args_hash.clone(),
                decision: decision.clone(),
            };
            if let Err(e) = journal.dispatching(&record).await {
                stages.push(StageRecord {
                    stage: "journal".into(),
                    outcome: format!("not journaled: {e}"),
                });
                return PipelineOutcome {
                    result: base(
                        ToolStatus::InfraFailure,
                        &args_hash,
                        Some("JOURNAL_FAILED".into()),
                        Some(format!("the dispatch could not be journaled: {e}")),
                    ),
                    stages,
                    policy: Some(decision),
                    effect_class: Some(effect_class),
                    tool_version: Some(spec.version.clone()),
                };
            }
            stages.push(StageRecord {
                stage: "journal".into(),
                outcome: "dispatched".into(),
            });
        }
        // 5. execute against the real effector
        let mut call_ctx = ctx.clone();
        call_ctx.tool_call_id = Some(tool_call_id);
        call_ctx.effect_class = Some(effect_class);
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

        // 6. postprocess: bounded inline view, complete bytes by OutputRef (docs/16 "Large results")
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
        } else if outcome.error_code.as_deref() == Some("CANCELLED") {
            // The run was cancelled while the process ran (docs/23): the
            // call was cancelled, it did not fail.
            ToolStatus::Cancelled
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
            effect_class: Some(effect_class),
            tool_version: Some(spec.version),
        }
    }
}
