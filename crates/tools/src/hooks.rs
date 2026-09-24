//! The Hook Bus (REQ-EV-0042, REQ-EV-0139, REQ-EV-0240): typed lifecycle
//! hooks, governed, bounded and audited.
//!
//! A hook is a handler outside the Core — a process the Core starts with a
//! typed JSON request on its standard input and whose answer it reads from
//! its standard output — registered at one typed point: before or after a
//! run, a model request, a tool call, a workspace change, a verification or
//! a compaction. It comes from the layered configuration (Device > Admin >
//! Project > User, with its authority as its scope) or from an extension a
//! session loaded; either way the registration is typed and validated before
//! anything runs.
//!
//! Three boundaries hold whatever a handler does.
//!
//! A hook can only tighten. An intercepting hook before a step may let it
//! continue, deny it, or — before a tool call or a change — rewrite its
//! arguments; there is no `allow`. The rewritten call goes back through the
//! schema, the custody check and the Capability Kernel, which decides last:
//! a deny anywhere is final, and no hook answer clears it (REQ-EV-0239's
//! monotonic verdict). An observing hook's opinion is recorded and never
//! applied.
//!
//! A hook is bounded. It runs under its own timeout with a scrubbed
//! environment (none of the Core's credentials), its output is capped, and a
//! handler that times out, fails or answers nonsense is handled by its
//! declared fail policy: `closed` (the default) stops the step, `open` lets
//! it proceed. Either way the failure is on the record.
//!
//! A registration is live or it is not. An extension's handlers leave with
//! the extension, and an answer that arrives from a handler whose
//! registration was removed while it ran is discarded (`UNLOADED`), so no
//! stale handler can still mutate a call.

use std::path::Path;
use std::process::Stdio;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use crate::registry::BoxFuture;

/// The request/answer contract version every handler is sent.
pub const HOOKS_VERSION: &str = "hooks-1";

/// A handler's timeout when its registration names none.
pub const DEFAULT_TIMEOUT_MS: u64 = 5_000;

/// The longest a handler may be given.
pub const MAX_TIMEOUT_MS: u64 = 30_000;

/// Bytes of a handler's answer read; more is a malformed answer.
pub const MAX_ANSWER_BYTES: usize = 64 * 1024;

/// The file an extension declares itself in.
pub const EXTENSION_MANIFEST: &str = "modbit-extension.json";

/// Where in the lifecycle a hook runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HookPoint {
    /// Before a run takes its first model round.
    BeforeRun,
    /// After a run ended, whatever its end.
    AfterRun,
    /// Before a model request.
    BeforeModel,
    /// After a model answered.
    AfterModel,
    /// Before a tool call reaches the Capability Kernel.
    BeforeTool,
    /// After a tool call's effect.
    AfterTool,
    /// Before a workspace change (a call whose effect is a workspace write).
    BeforeChange,
    /// After a workspace change.
    AfterChange,
    /// Before a verification stage runs its checks.
    BeforeVerification,
    /// After a verification stage.
    AfterVerification,
    /// Before a compaction starts.
    BeforeCompaction,
    /// After a compaction committed.
    AfterCompaction,
}

impl HookPoint {
    /// Every point, in lifecycle order.
    pub const ALL: [Self; 12] = [
        Self::BeforeRun,
        Self::AfterRun,
        Self::BeforeModel,
        Self::AfterModel,
        Self::BeforeTool,
        Self::AfterTool,
        Self::BeforeChange,
        Self::AfterChange,
        Self::BeforeVerification,
        Self::AfterVerification,
        Self::BeforeCompaction,
        Self::AfterCompaction,
    ];

    /// The wire and record label.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::BeforeRun => "before_run",
            Self::AfterRun => "after_run",
            Self::BeforeModel => "before_model",
            Self::AfterModel => "after_model",
            Self::BeforeTool => "before_tool",
            Self::AfterTool => "after_tool",
            Self::BeforeChange => "before_change",
            Self::AfterChange => "after_change",
            Self::BeforeVerification => "before_verification",
            Self::AfterVerification => "after_verification",
            Self::BeforeCompaction => "before_compaction",
            Self::AfterCompaction => "after_compaction",
        }
    }

    /// Whether the step has not happened yet, so an intercepting hook can
    /// stop it.
    #[must_use]
    pub fn is_before(self) -> bool {
        matches!(
            self,
            Self::BeforeRun
                | Self::BeforeModel
                | Self::BeforeTool
                | Self::BeforeChange
                | Self::BeforeVerification
                | Self::BeforeCompaction
        )
    }

    /// Whether the step is a tool call, so a registration may scope itself
    /// to named tools.
    #[must_use]
    pub fn is_tool_point(self) -> bool {
        matches!(
            self,
            Self::BeforeTool | Self::AfterTool | Self::BeforeChange | Self::AfterChange
        )
    }

    /// Whether an intercepting hook may rewrite the step's arguments.
    #[must_use]
    pub fn may_mutate(self) -> bool {
        matches!(self, Self::BeforeTool | Self::BeforeChange)
    }
}

/// Whether a hook may act on the step or only observe it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HookMode {
    /// Recorded; its opinion is never applied.
    #[default]
    Observe,
    /// May stop the step (and, before a tool call or a change, rewrite it).
    Intercept,
}

/// What a handler that times out, fails or answers nonsense does to the step.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FailPolicy {
    /// The step does not proceed (before) or the run stops for attention
    /// (after).
    #[default]
    Closed,
    /// The step proceeds; the failure is recorded.
    Open,
}

impl FailPolicy {
    /// Record label.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Closed => "closed",
            Self::Open => "open",
        }
    }
}

impl HookMode {
    /// Record label.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Observe => "observe",
            Self::Intercept => "intercept",
        }
    }
}

/// One hook, as a configuration layer or an extension declares it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HookSpec {
    /// Name, unique within its source.
    pub name: String,
    /// Where it runs.
    pub point: HookPoint,
    /// Observe (default) or intercept.
    #[serde(default)]
    pub mode: HookMode,
    /// The handler: program and arguments, run without a shell.
    pub command: Vec<String>,
    /// Milliseconds the handler is given, at most `MAX_TIMEOUT_MS`.
    #[serde(default = "default_timeout")]
    pub timeout_ms: u64,
    /// What a failing handler does to the step (default `closed`).
    #[serde(default)]
    pub fail_policy: FailPolicy,
    /// At tool and change points, the tools it applies to: exact names or a
    /// `prefix.*`; empty is every tool.
    #[serde(default)]
    pub tools: Vec<String>,
}

fn default_timeout() -> u64 {
    DEFAULT_TIMEOUT_MS
}

impl HookSpec {
    /// Parse and validate one declaration.
    ///
    /// # Errors
    /// Why it is not a hook this build can run.
    pub fn parse(json: &str) -> Result<Self, String> {
        let spec: Self = serde_json::from_str(json).map_err(|e| format!("not a hook: {e}"))?;
        spec.validate()?;
        Ok(spec)
    }

    /// Check a declaration.
    ///
    /// # Errors
    /// The first thing wrong with it.
    pub fn validate(&self) -> Result<(), String> {
        let name_ok = !self.name.is_empty()
            && self.name.len() <= 64
            && self
                .name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'));
        if !name_ok {
            return Err(format!(
                "`{}` is not a hook name (1–64 of A–Z, a–z, 0–9, `-`, `_`, `.`)",
                self.name
            ));
        }
        if self.command.first().is_none_or(|p| p.trim().is_empty()) {
            return Err(format!("hook `{}` names no program", self.name));
        }
        if self.timeout_ms == 0 || self.timeout_ms > MAX_TIMEOUT_MS {
            return Err(format!(
                "hook `{}`: timeout_ms must be 1..={MAX_TIMEOUT_MS}, not {}",
                self.name, self.timeout_ms
            ));
        }
        if self.mode == HookMode::Intercept && !self.point.is_before() {
            return Err(format!(
                "hook `{}`: `{}` is after the step; only a hook before a step can intercept it",
                self.name,
                self.point.label()
            ));
        }
        if !self.tools.is_empty() && !self.point.is_tool_point() {
            return Err(format!(
                "hook `{}`: `tools` scopes a tool or change point, not `{}`",
                self.name,
                self.point.label()
            ));
        }
        Ok(())
    }

    /// Whether the registration applies to `tool` (every step at a point
    /// that is not a tool point).
    #[must_use]
    pub fn applies_to(&self, tool: Option<&str>) -> bool {
        if self.tools.is_empty() {
            return true;
        }
        let Some(tool) = tool else { return false };
        self.tools.iter().any(|t| match t.strip_suffix(".*") {
            Some(prefix) => tool
                .strip_prefix(prefix)
                .is_some_and(|rest| rest.starts_with('.')),
            None => t == tool,
        })
    }
}

/// An extension's declaration of itself (REQ-EV-0240): what it is and the
/// handlers it registers. Its handlers run outside the Core like any hook.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExtensionManifest {
    /// Name.
    pub name: String,
    /// Version.
    pub version: String,
    /// The hooks it registers.
    #[serde(default)]
    pub hooks: Vec<HookSpec>,
}

impl ExtensionManifest {
    /// Parse and validate a manifest.
    ///
    /// # Errors
    /// Why it cannot be loaded.
    pub fn parse(json: &str) -> Result<Self, String> {
        let m: Self =
            serde_json::from_str(json).map_err(|e| format!("not an extension manifest: {e}"))?;
        if m.name.trim().is_empty() || m.version.trim().is_empty() {
            return Err("an extension names itself and its version".into());
        }
        let mut seen = std::collections::BTreeSet::new();
        for h in &m.hooks {
            h.validate()?;
            if !seen.insert(h.name.clone()) {
                return Err(format!("hook `{}` is declared twice", h.name));
            }
        }
        Ok(m)
    }
}

/// Where a registration came from.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum HookSource {
    /// A configuration layer (`device` | `admin` | `project` | `user`).
    Config {
        /// The layer's authority.
        authority: String,
    },
    /// An extension a session loaded.
    Extension {
        /// The load's id.
        extension_id: String,
        /// The extension's name.
        name: String,
    },
}

impl HookSource {
    /// Record label: `config:admin`, `extension:<name>`.
    #[must_use]
    pub fn label(&self) -> String {
        match self {
            Self::Config { authority } => format!("config:{authority}"),
            Self::Extension { name, .. } => format!("extension:{name}"),
        }
    }
}

/// One live registration.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Registration {
    /// Stable id: `<source label>/<name>`.
    pub id: String,
    /// Source.
    pub source: HookSource,
    /// The declaration.
    pub spec: HookSpec,
}

impl Registration {
    /// A registration from its source and declaration.
    #[must_use]
    pub fn new(source: HookSource, spec: HookSpec) -> Self {
        Self {
            id: format!("{}/{}", source.label(), spec.name),
            source,
            spec,
        }
    }
}

/// One step a hook is asked about.
#[derive(Clone, Debug)]
pub struct HookRequest {
    /// Point.
    pub point: HookPoint,
    /// Task.
    pub task_id: String,
    /// Run, when there is one.
    pub run_id: Option<String>,
    /// The tool, at a tool or change point.
    pub tool: Option<String>,
    /// What the step is (typed per point; see `docs/16` "Hook Bus").
    pub payload: Value,
}

/// How one handler invocation ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum HookOutcome {
    /// It let the step continue.
    Ok,
    /// It denied the step.
    Denied,
    /// It rewrote the step's arguments.
    Mutated,
    /// It did not answer in time; it was killed.
    Timeout,
    /// It could not be started or exited with a failure.
    Failed,
    /// Its answer was not a hook answer (including an `allow`, which no
    /// hook can give).
    Malformed,
    /// It observes, or its point allows no rewrite, so what it asked for was
    /// not applied.
    Ignored,
    /// Its registration was removed while it ran; its answer was discarded.
    Unloaded,
}

impl HookOutcome {
    /// Record label.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Ok => "OK",
            Self::Denied => "DENIED",
            Self::Mutated => "MUTATED",
            Self::Timeout => "TIMEOUT",
            Self::Failed => "FAILED",
            Self::Malformed => "MALFORMED",
            Self::Ignored => "IGNORED",
            Self::Unloaded => "UNLOADED",
        }
    }
}

/// The record of one handler invocation (journaled as `HookInvoked`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HookRecord {
    /// Registration id.
    pub hook: String,
    /// Source label.
    pub source: String,
    /// Point.
    pub point: HookPoint,
    /// Mode.
    pub mode: HookMode,
    /// Fail policy.
    pub fail_policy: FailPolicy,
    /// Outcome.
    pub outcome: HookOutcome,
    /// Whether it changed what happened: a denial or a failure that stopped
    /// the step, or a rewrite that was used.
    pub applied: bool,
    /// Wall time, milliseconds.
    pub duration_ms: u64,
    /// The handler's reason, or what went wrong.
    pub detail: String,
    /// The tool, at a tool or change point.
    pub tool: Option<String>,
    /// The rewritten arguments, canonical JSON, when a rewrite was used.
    pub arguments: Option<Value>,
}

/// What the hooks at one point did to the step.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct HookEffect {
    /// The step is stopped: `(code, reason)`.
    pub denied: Option<(String, String)>,
    /// The arguments the step proceeds with, when a hook rewrote them.
    pub arguments: Option<Value>,
    /// Every invocation, in order.
    pub records: Vec<HookRecord>,
}

/// A handler's answer before it is judged.
struct Answer {
    outcome: HookOutcome,
    decision: Option<Decision>,
    detail: String,
    duration_ms: u64,
}

enum Decision {
    Continue,
    Deny(String),
    Mutate(Value),
}

/// The environment a handler gets: enough to find programs and a home, and
/// nothing of the Core's (no credential, no configuration).
fn scrubbed_env() -> Vec<(String, String)> {
    [
        "PATH",
        "HOME",
        "USERPROFILE",
        "SYSTEMROOT",
        "SystemRoot",
        "TEMP",
        "TMP",
        "TMPDIR",
        "LANG",
    ]
    .iter()
    .filter_map(|k| std::env::var(k).ok().map(|v| ((*k).to_owned(), v)))
    .collect()
}

/// Run one handler: the request on its stdin, its answer from its stdout,
/// under its timeout. Never panics; every way it can go wrong is an outcome.
async fn run_handler(spec: &HookSpec, request: &Value, cwd: Option<&Path>) -> Answer {
    let started = Instant::now();
    let elapsed = |s: Instant| u64::try_from(s.elapsed().as_millis()).unwrap_or(u64::MAX);
    let mut cmd = tokio::process::Command::new(&spec.command[0]);
    cmd.args(&spec.command[1..])
        .env_clear()
        .envs(scrubbed_env())
        .env("MODBIT_HOOK_POINT", spec.point.label())
        .env("MODBIT_HOOK_NAME", &spec.name)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    if let Some(dir) = cwd {
        cmd.current_dir(dir);
    }
    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            return Answer {
                outcome: HookOutcome::Failed,
                decision: None,
                detail: format!("`{}` could not be started: {e}", spec.command[0]),
                duration_ms: elapsed(started),
            };
        }
    };
    let input = request.to_string();
    let mut stdin = child.stdin.take();
    let mut stdout = child.stdout.take();
    let mut stderr = child.stderr.take();
    let run = async {
        if let Some(mut s) = stdin.take() {
            // A handler that does not read its request is its own business.
            let _ = s.write_all(input.as_bytes()).await;
            let _ = s.shutdown().await;
        }
        let mut out = Vec::new();
        if let Some(s) = stdout.as_mut() {
            let _ = s
                .take(MAX_ANSWER_BYTES as u64 + 1)
                .read_to_end(&mut out)
                .await;
        }
        let mut err = Vec::new();
        if let Some(s) = stderr.as_mut() {
            let _ = s.take(2_048).read_to_end(&mut err).await;
        }
        let status = child.wait().await;
        (out, err, status)
    };
    let timeout = Duration::from_millis(spec.timeout_ms);
    let Ok((out, err, status)) = tokio::time::timeout(timeout, run).await else {
        // Dropping the future dropped the child: kill_on_drop ends it.
        return Answer {
            outcome: HookOutcome::Timeout,
            decision: None,
            detail: format!(
                "no answer within {} ms; the handler was killed",
                spec.timeout_ms
            ),
            duration_ms: elapsed(started),
        };
    };
    let duration_ms = elapsed(started);
    let status = match status {
        Ok(s) => s,
        Err(e) => {
            return Answer {
                outcome: HookOutcome::Failed,
                decision: None,
                detail: format!("the handler could not be awaited: {e}"),
                duration_ms,
            };
        }
    };
    if !status.success() {
        let tail = String::from_utf8_lossy(&err).trim().to_owned();
        return Answer {
            outcome: HookOutcome::Failed,
            decision: None,
            detail: format!(
                "the handler exited {}{}",
                status
                    .code()
                    .map_or_else(|| "by a signal".to_owned(), |c| format!("with {c}")),
                if tail.is_empty() {
                    String::new()
                } else {
                    format!(": {tail}")
                }
            ),
            duration_ms,
        };
    }
    if out.len() > MAX_ANSWER_BYTES {
        return Answer {
            outcome: HookOutcome::Malformed,
            decision: None,
            detail: format!("the answer is over {MAX_ANSWER_BYTES} bytes"),
            duration_ms,
        };
    }
    let text = String::from_utf8_lossy(&out);
    if text.trim().is_empty() {
        return Answer {
            outcome: HookOutcome::Ok,
            decision: Some(Decision::Continue),
            detail: String::new(),
            duration_ms,
        };
    }
    let malformed = |detail: String| Answer {
        outcome: HookOutcome::Malformed,
        decision: None,
        detail,
        duration_ms,
    };
    let Ok(answer) = serde_json::from_str::<Value>(text.trim()) else {
        return malformed("the answer is not JSON".into());
    };
    let reason = answer["reason"].as_str().unwrap_or_default().to_owned();
    match answer["decision"].as_str().unwrap_or("continue") {
        "continue" => Answer {
            outcome: HookOutcome::Ok,
            decision: Some(Decision::Continue),
            detail: reason,
            duration_ms,
        },
        "deny" => Answer {
            outcome: HookOutcome::Denied,
            decision: Some(Decision::Deny(if reason.is_empty() {
                "denied by the hook".into()
            } else {
                reason
            })),
            detail: String::new(),
            duration_ms,
        },
        "mutate" => match answer.get("arguments") {
            Some(a) if a.is_object() => Answer {
                outcome: HookOutcome::Mutated,
                decision: Some(Decision::Mutate(a.clone())),
                detail: reason,
                duration_ms,
            },
            _ => malformed("`mutate` names no argument object".into()),
        },
        "allow" => malformed(
            "`allow` is not a hook decision: a hook may continue, deny or rewrite; only the Capability Kernel allows".into(),
        ),
        other => malformed(format!("`{other}` is not a hook decision")),
    }
}

/// The request a handler is sent.
fn request_json(reg: &Registration, req: &HookRequest, payload: &Value) -> Value {
    json!({
        "hooks_version": HOOKS_VERSION,
        "point": req.point.label(),
        "hook": reg.spec.name,
        "source": reg.source.label(),
        "task_id": req.task_id,
        "run_id": req.run_id,
        "tool": req.tool,
        "payload": payload,
    })
}

/// Run every live registration at `req.point` that applies, in order, and
/// fold what they did into one effect. The fold only tightens: the first
/// denial (or fail-closed failure) stops the step and ends the fold; a
/// rewrite is handed to the hooks after it and to the caller, who takes it
/// back through validation and the kernel. `live` is asked before each
/// handler starts and again before its answer is used.
pub async fn fire(
    registrations: &[Registration],
    req: &HookRequest,
    cwd: Option<&Path>,
    live: &(dyn Fn(&Registration) -> bool + Send + Sync),
) -> HookEffect {
    let mut effect = HookEffect::default();
    for reg in registrations {
        if reg.spec.point != req.point || !reg.spec.applies_to(req.tool.as_deref()) || !live(reg) {
            continue;
        }
        // Each hook sees the arguments the hooks before it left.
        let mut payload = req.payload.clone();
        if let (Some(a), Some(obj)) = (&effect.arguments, payload.as_object_mut()) {
            obj.insert("arguments".into(), a.clone());
        }
        let answer = run_handler(&reg.spec, &request_json(reg, req, &payload), cwd).await;
        let mut record = HookRecord {
            hook: reg.id.clone(),
            source: reg.source.label(),
            point: req.point,
            mode: reg.spec.mode,
            fail_policy: reg.spec.fail_policy,
            outcome: answer.outcome,
            applied: false,
            duration_ms: answer.duration_ms,
            detail: answer.detail.clone(),
            tool: req.tool.clone(),
            arguments: None,
        };
        let failed = matches!(
            answer.outcome,
            HookOutcome::Timeout | HookOutcome::Failed | HookOutcome::Malformed
        );
        // Nothing a removed registration said is used.
        let still_live = live(reg);
        if !still_live
            && (failed
                || matches!(
                    answer.decision,
                    Some(Decision::Deny(_) | Decision::Mutate(_))
                ))
        {
            record.outcome = HookOutcome::Unloaded;
            record.detail = format!(
                "the registration was removed while the handler ran; its answer ({}) was discarded",
                answer.outcome.label()
            );
            effect.records.push(record);
            continue;
        }
        if failed {
            if reg.spec.fail_policy == FailPolicy::Closed {
                record.applied = true;
                let code = match answer.outcome {
                    HookOutcome::Timeout => "HOOK_TIMEOUT",
                    HookOutcome::Malformed => "HOOK_MALFORMED",
                    _ => "HOOK_FAILED",
                };
                effect.denied = Some((
                    code.to_owned(),
                    format!(
                        "hook `{}` ({}) failed closed at {}: {}",
                        reg.spec.name,
                        reg.source.label(),
                        req.point.label(),
                        answer.detail
                    ),
                ));
                effect.records.push(record);
                return effect;
            }
            effect.records.push(record);
            continue;
        }
        match answer.decision {
            Some(Decision::Deny(reason)) => {
                if reg.spec.mode == HookMode::Intercept {
                    record.applied = true;
                    record.detail = reason.clone();
                    effect.denied = Some((
                        "HOOK_DENIED".to_owned(),
                        format!(
                            "hook `{}` ({}) denied at {}: {reason}",
                            reg.spec.name,
                            reg.source.label(),
                            req.point.label()
                        ),
                    ));
                    effect.records.push(record);
                    return effect;
                }
                record.outcome = HookOutcome::Ignored;
                record.detail = format!("an observing hook cannot deny ({reason})");
            }
            Some(Decision::Mutate(arguments)) => {
                if reg.spec.mode == HookMode::Intercept && req.point.may_mutate() {
                    record.applied = true;
                    record.arguments = Some(arguments.clone());
                    effect.arguments = Some(arguments);
                } else {
                    record.outcome = HookOutcome::Ignored;
                    record.detail = format!(
                        "{} cannot rewrite at {}",
                        if reg.spec.mode == HookMode::Observe {
                            "an observing hook"
                        } else {
                            "a hook"
                        },
                        req.point.label()
                    );
                }
            }
            Some(Decision::Continue) | None => {}
        }
        effect.records.push(record);
    }
    effect
}

/// The Hook Bus as the tool pipeline sees it: the host resolves the task's
/// registrations, runs them, journals every invocation, and answers with
/// what they did to the call.
pub trait HookPort: Send + Sync {
    /// Before the call reaches the kernel (`BeforeTool`, then `BeforeChange`
    /// for a workspace write).
    fn before<'a>(
        &'a self,
        point: HookPoint,
        tool: &'a str,
        effect_class: crate::EffectClass,
        arguments: &'a Value,
    ) -> BoxFuture<'a, HookEffect>;
    /// After the call's effect (`AfterTool`, then `AfterChange`); the effect
    /// has happened, so nothing here can stop it — a fail-closed failure
    /// stops the run at its next boundary instead.
    fn after<'a>(
        &'a self,
        point: HookPoint,
        tool: &'a str,
        effect_class: crate::EffectClass,
        result: &'a Value,
    ) -> BoxFuture<'a, HookEffect>;
}

/// Whether a call of this class is a workspace change.
#[must_use]
pub fn is_change(effect_class: crate::EffectClass) -> bool {
    matches!(
        effect_class,
        crate::EffectClass::ReversibleWrite | crate::EffectClass::ProtectedWrite
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(json: &str) -> HookSpec {
        HookSpec::parse(json).expect("valid")
    }

    #[test]
    fn a_declaration_is_typed_and_validated_before_anything_runs() {
        let s = spec(
            r#"{"name":"lint","point":"before_tool","mode":"intercept","command":["sh","-c","true"],"tools":["fs.*"]}"#,
        );
        assert_eq!(s.fail_policy, FailPolicy::Closed, "fail closed by default");
        assert_eq!(s.timeout_ms, DEFAULT_TIMEOUT_MS);
        assert!(s.applies_to(Some("fs.read")));
        assert!(!s.applies_to(Some("fsx.read")));
        assert!(!s.applies_to(Some("change.apply")));
        for bad in [
            r#"{"name":"x","point":"after_tool","mode":"intercept","command":["true"]}"#,
            r#"{"name":"x","point":"before_run","command":["true"],"tools":["fs.read"]}"#,
            r#"{"name":"x","point":"before_tool","command":[]}"#,
            r#"{"name":"x","point":"before_tool","command":["true"],"timeout_ms":60000}"#,
            r#"{"name":"x y","point":"before_tool","command":["true"]}"#,
            r#"{"name":"x","point":"before_everything","command":["true"]}"#,
            r#"{"name":"x","point":"before_tool","command":["true"],"allow":true}"#,
        ] {
            assert!(HookSpec::parse(bad).is_err(), "{bad}");
        }
        assert!(
            ExtensionManifest::parse(
                r#"{"name":"e","version":"1","hooks":[{"name":"a","point":"after_run","command":["true"]},{"name":"a","point":"after_run","command":["true"]}]}"#
            )
            .is_err(),
            "a hook declared twice"
        );
    }
}
