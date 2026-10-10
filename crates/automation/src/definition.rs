//! The automation definition: a versioned, typed, bounded document that
//! *describes* work and runs nothing (docs/68 section 4; DR-PX-2026-10-03-010).
//!
//! A definition is data. Its `prompt` becomes the goal text of an ordinary
//! task when a trigger fires; no field of it is a program, a shell command or
//! a template that is expanded. Parsing rejects unknown fields, unknown
//! trigger kinds and unknown inputs; validation bounds every size, compiles
//! every user expression with a linear-time engine and refuses what it cannot
//! prove safe. The canonical hash is what an owner's enable approval binds.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::cron::Cron;

/// The only schema this build reads.
pub const SCHEMA: &str = "modbit.automation/1";
/// The largest definition document accepted, in bytes.
pub const MAX_DEFINITION_BYTES: usize = 64 * 1024;
/// The shortest interval between two scheduled runs (policy data, V01).
pub const SCHEDULE_FLOOR_MINUTES: i64 = 5;

/// Capabilities a definition may ask for beyond the read-only floor. Native
/// computer control and secret use are never available to an automation
/// (DR-PX-2026-10-03-008, AUT-D02, AUT-D06).
pub const WRITE_CAPABILITIES: &[&str] = &[
    "fs.write",
    "shell.exec",
    "git.worktree",
    "git.apply",
    "git.merge",
    "memory.propose",
];
/// Capabilities that reach beyond the machine.
pub const EXTERNAL_CAPABILITIES: &[&str] = &["network.egress", "external.call"];

/// Event sources the first release adopts (AUT-B01).
pub const EVENT_SOURCES: &[&str] = &["forge"];
/// Repository events from the forge adapter and the actions each carries.
pub const FORGE_EVENTS: &[(&str, &[&str])] = &[
    ("pull_request", &["opened", "updated"]),
    ("push", &[]),
    ("check_completed", &[]),
    ("label_added", &[]),
    ("issue_comment", &[]),
    ("review_comment", &[]),
    ("review_submitted", &[]),
];

/// One problem found in a definition: where, a stable code, and why.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Issue {
    /// JSON-pointer-like location (`/triggers/0/cron`).
    pub path: String,
    /// Stable code (`UNKNOWN_FIELD`, `UNKNOWN_TRIGGER`, `BAD_CRON`, ...).
    pub code: String,
    /// What is wrong, in words.
    pub message: String,
}

fn issue(path: impl Into<String>, code: &str, message: impl Into<String>) -> Issue {
    Issue {
        path: path.into(),
        code: code.into(),
        message: message.into(),
    }
}

/// The type of an input or output value.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ValueType {
    /// Bounded text.
    String,
    /// A signed integer.
    Integer,
    /// A boolean.
    Boolean,
    /// One of a closed list of strings.
    Enum,
}

/// A typed input a manual or test run may supply.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InputSpec {
    /// Name (`[a-z][a-z0-9_]*`).
    pub name: String,
    /// Type.
    #[serde(rename = "type")]
    pub ty: ValueType,
    /// Whether a run must supply it when it has no default.
    #[serde(default)]
    pub required: bool,
    /// Default, of the declared type.
    #[serde(default)]
    pub default: Option<serde_json::Value>,
    /// For `string`: the longest value (default 200, at most 2000).
    #[serde(default)]
    pub max_len: Option<u32>,
    /// For `enum`: the allowed values.
    #[serde(default)]
    pub values: Vec<String>,
    /// Shown to the person.
    #[serde(default)]
    pub description: String,
}

/// A typed output a run reports.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OutputSpec {
    /// Name (`[a-z][a-z0-9_]*`).
    pub name: String,
    /// Type (`enum` is not an output type).
    #[serde(rename = "type")]
    pub ty: ValueType,
    /// Shown to the person.
    #[serde(default)]
    pub description: String,
}

/// Filters over a trigger's event (AUT-B02). Every present filter must match.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Filters {
    /// Branch globs (`main`, `release/*`).
    #[serde(default)]
    pub branches: Vec<String>,
    /// Labels, any one of which must be present.
    #[serde(default)]
    pub labels: Vec<String>,
    /// Authors, any one of which must match (case-insensitive).
    #[serde(default)]
    pub authors: Vec<String>,
    /// Path globs, any one of which must match a changed path.
    #[serde(default)]
    pub paths: Vec<String>,
    /// A regular expression the title must match (linear-time engine).
    #[serde(default)]
    pub title_regex: Option<String>,
    /// A regular expression the body or comment text must match.
    #[serde(default)]
    pub text_regex: Option<String>,
    /// Field tests over a webhook body, by JSON pointer.
    #[serde(default)]
    pub fields: Vec<FieldFilter>,
}

/// A test of one field of a JSON payload.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FieldFilter {
    /// JSON pointer (`/ref`).
    pub pointer: String,
    /// The field must equal this string.
    #[serde(default)]
    pub equals: Option<String>,
    /// The field must match this regular expression (linear-time engine).
    #[serde(default)]
    pub regex: Option<String>,
}

/// A trigger (AUT-B01). An unknown kind is refused, not ignored.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Trigger {
    /// A time schedule in UTC: a five-field cron expression, or a fixed
    /// interval of at least five minutes.
    Schedule {
        /// Identity within the definition.
        id: String,
        /// Five-field cron expression.
        #[serde(default)]
        cron: Option<String>,
        /// Fixed interval in minutes.
        #[serde(default)]
        every_minutes: Option<u32>,
    },
    /// A person starts it (also the test run).
    Manual {
        /// Identity within the definition.
        id: String,
    },
    /// A repository event delivered by the forge adapter.
    Event {
        /// Identity within the definition.
        id: String,
        /// The event source (`forge`).
        source: String,
        /// The event (`pull_request`, `push`, ...).
        event: String,
        /// Actions of the event, any one of which must match.
        #[serde(default)]
        actions: Vec<String>,
        /// Filters.
        #[serde(default)]
        filters: Filters,
    },
    /// A signed generic webhook, delivered through the Cloud API (PX-085).
    Webhook {
        /// Identity within the definition.
        id: String,
        /// The endpoint name the sender uses.
        name: String,
        /// Filters over the body.
        #[serde(default)]
        filters: Filters,
    },
}

impl Trigger {
    /// The trigger's id.
    #[must_use]
    pub fn id(&self) -> &str {
        match self {
            Self::Schedule { id, .. }
            | Self::Manual { id }
            | Self::Event { id, .. }
            | Self::Webhook { id, .. } => id,
        }
    }

    /// The trigger's kind label.
    #[must_use]
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Schedule { .. } => "schedule",
            Self::Manual { .. } => "manual",
            Self::Event { .. } => "event",
            Self::Webhook { .. } => "webhook",
        }
    }
}

/// The highest effect class a run may reach (the principal's ceiling is
/// intersected with it).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Effects {
    /// Reads only: the default.
    #[default]
    ReadOnly,
    /// Writes inside the run's own worktree.
    ReversibleWrite,
    /// Writes to protected paths and protected effects, each asking.
    ProtectedWrite,
    /// Effects beyond the machine, each asking.
    ExternalSideEffect,
}

impl Effects {
    /// The kernel's name for the class.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::ReadOnly => "read_only",
            Self::ReversibleWrite => "reversible_write",
            Self::ProtectedWrite => "protected_write",
            Self::ExternalSideEffect => "external_side_effect",
        }
    }
}

/// The capability profile of a run (AUT-D02): read-only unless it says more.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Profile {
    /// The highest effect class.
    #[serde(default)]
    pub effects: Effects,
    /// Capabilities beyond the read-only floor, by exact id.
    #[serde(default)]
    pub capabilities: Vec<String>,
    /// Workspace-relative globs a write may touch (`reports/**`).
    #[serde(default)]
    pub paths: Vec<String>,
    /// Hosts the run may reach (`api.github.com`).
    #[serde(default)]
    pub hosts: Vec<String>,
}

impl Profile {
    /// Whether the profile is the read-only floor.
    #[must_use]
    pub fn is_read_only(&self) -> bool {
        self.effects == Effects::ReadOnly
            && self.capabilities.is_empty()
            && self.paths.is_empty()
            && self.hosts.is_empty()
    }
}

/// Whose authority a run takes (AUT-A03).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Principal {
    /// The person who created the definition.
    #[default]
    Creator,
    /// A named service account whose ceiling is the policy's.
    ServiceAccount {
        /// Stable identity (`[a-z][a-z0-9-]*`).
        id: String,
    },
}

/// What to do when a trigger fires while a run is active (AUT-B06).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConcurrencyPolicy {
    /// Record the firing as skipped.
    #[default]
    Skip,
    /// Hold it, up to a bound, and run it when the active run ends.
    Queue,
    /// Cancel the active run and run the new one.
    Replace,
}

/// The concurrency policy.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Concurrency {
    /// The policy.
    #[serde(default)]
    pub policy: ConcurrencyPolicy,
    /// For `queue`: how many firings may wait (1-10).
    #[serde(default = "default_queue_max")]
    pub queue_max: u32,
}

fn default_queue_max() -> u32 {
    3
}

impl Default for Concurrency {
    fn default() -> Self {
        Self {
            policy: ConcurrencyPolicy::Skip,
            queue_max: default_queue_max(),
        }
    }
}

/// What to do about schedule slots missed while the Core was not running
/// (AUT-B03). Missed slots are never replayed in bulk.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MissedPolicy {
    /// Record the missed window as skipped.
    #[default]
    Skip,
    /// Run once for the whole missed window, if it is inside the bound.
    RunOnce,
}

/// The missed-run policy.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Missed {
    /// The policy.
    #[serde(default)]
    pub policy: MissedPolicy,
    /// For `run_once`: how far back a missed slot still counts (minutes).
    #[serde(default = "default_catch_up")]
    pub catch_up_window_minutes: u32,
}

fn default_catch_up() -> u32 {
    1440
}

impl Default for Missed {
    fn default() -> Self {
        Self {
            policy: MissedPolicy::Skip,
            catch_up_window_minutes: default_catch_up(),
        }
    }
}

/// Per-run limits (AUT-C01, PX-116). Every one has a bound and none can be
/// zero: an unattended run always has a ceiling.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Limits {
    /// Wall-clock limit of a run, minutes (1-1440).
    #[serde(default = "default_deadline")]
    pub deadline_minutes: u32,
    /// Cost limit of a run, in minor currency units (1-100000). Optional:
    /// the Core enforces it only for a model it can price, and a cap on a
    /// model it cannot price stops the run (PX-116 fails closed), so a
    /// definition that names one names a priced model.
    #[serde(default)]
    pub max_cost_minor: Option<u64>,
    /// Turn limit (1-200).
    #[serde(default = "default_turns")]
    pub max_turns: u32,
    /// Tool-call limit (1-1000).
    #[serde(default = "default_tool_calls")]
    pub max_tool_calls: u32,
    /// How long a parked approval waits before the run is cancelled and
    /// receipted, minutes (1-10080; the default is 24 h, AUT-D01).
    #[serde(default = "default_approval_wait")]
    pub approval_wait_minutes: u32,
}

fn default_deadline() -> u32 {
    30
}
fn default_turns() -> u32 {
    40
}
fn default_tool_calls() -> u32 {
    200
}
fn default_approval_wait() -> u32 {
    1440
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            deadline_minutes: default_deadline(),
            max_cost_minor: None,
            max_turns: default_turns(),
            max_tool_calls: default_tool_calls(),
            approval_wait_minutes: default_approval_wait(),
        }
    }
}

/// Rate and daily budget of the definition (AUT-B06).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Budget {
    /// Runs the definition may start per hour (1-60).
    #[serde(default = "default_per_hour")]
    pub max_runs_per_hour: u32,
    /// The most the definition's runs may reserve in a UTC day, in minor
    /// currency units (each run reserves its `max_cost_minor`). Optional;
    /// meaningful with a per-run cost limit.
    #[serde(default)]
    pub daily_budget_minor: Option<u64>,
}

fn default_per_hour() -> u32 {
    12
}

impl Default for Budget {
    fn default() -> Self {
        Self {
            max_runs_per_hour: default_per_hour(),
            daily_budget_minor: None,
        }
    }
}

/// An optional first step (AUT-C02): a cheap, read-only evaluation whose
/// typed answer decides whether the rest of the run happens. The gate runs as
/// its own read-only task; its last message must begin `GATE: RUN` or
/// `GATE: SKIP <reason>`, and anything else skips (fail closed).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Gate {
    /// What to decide. Data: it is the gate task's goal text.
    pub prompt: String,
    /// The gate's own turn limit (1-8).
    #[serde(default = "default_gate_turns")]
    pub max_turns: u32,
}

fn default_gate_turns() -> u32 {
    4
}

/// The definition document.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Definition {
    /// Always [`SCHEMA`].
    pub schema: String,
    /// A short stable name (`[a-z0-9][a-z0-9_-]*`, at most 64).
    pub name: String,
    /// What it is for.
    #[serde(default)]
    pub description: String,
    /// The goal text of every task it creates. Data: never expanded, never
    /// run, and the only instruction text a trigger's task starts with.
    pub prompt: String,
    /// Typed inputs.
    #[serde(default)]
    pub inputs: Vec<InputSpec>,
    /// Typed outputs.
    #[serde(default)]
    pub outputs: Vec<OutputSpec>,
    /// Triggers (1-8).
    pub triggers: Vec<Trigger>,
    /// The capability profile (read-only by default).
    #[serde(default)]
    pub profile: Profile,
    /// Whose authority a run takes.
    #[serde(default)]
    pub principal: Principal,
    /// Concurrency.
    #[serde(default)]
    pub concurrency: Concurrency,
    /// Missed runs.
    #[serde(default)]
    pub missed: Missed,
    /// Per-run limits.
    #[serde(default)]
    pub limits: Limits,
    /// Rate and daily budget.
    #[serde(default)]
    pub budget: Budget,
    /// The optional gate step.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gate: Option<Gate>,
}

impl Definition {
    /// The canonical form the hash covers: the typed document serialised
    /// with a fixed field order. Two definitions with the same meaning have
    /// the same canonical text; any change of meaning changes it.
    #[must_use]
    pub fn canonical(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }

    /// sha256 (hex) of [`Definition::canonical`]. This is what an enable
    /// approval is bound to (AUT-A04, AUT-D03).
    #[must_use]
    pub fn hash(&self) -> String {
        sha256_hex(self.canonical().as_bytes())
    }

    /// The trigger with `id`.
    #[must_use]
    pub fn trigger(&self, id: &str) -> Option<&Trigger> {
        self.triggers.iter().find(|t| t.id() == id)
    }

    /// Whether the profile needs an approval that lists capabilities, paths
    /// and hosts (anything beyond the read-only floor).
    #[must_use]
    pub fn needs_listed_approval(&self) -> bool {
        !self.profile.is_read_only()
    }

    /// What stops this definition being *enabled* even though it may be
    /// saved (AUT-D03: enabling validates the budget). A definition that can
    /// write, run commands or reach the network spends money and acts
    /// unattended, so it must state the most one run may cost; a draft
    /// without that limit is saved and refused at the approval, with the
    /// typed code `BUDGET_REQUIRED`.
    #[must_use]
    pub fn enable_issues(&self) -> Vec<Issue> {
        let mut out = Vec::new();
        if !self.profile.is_read_only() && self.limits.max_cost_minor.is_none() {
            out.push(issue(
                "/limits/max_cost_minor",
                "BUDGET_REQUIRED",
                "a definition that can write, run commands or reach the network states the most one run may cost (limits.max_cost_minor, 1-100000 minor units) before it can be enabled",
            ));
        }
        out
    }
}

/// sha256 as lowercase hex.
#[must_use]
pub fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

fn plain_name(s: &str, lead_digit: bool, extra: &[char], max: usize) -> bool {
    let mut chars = s.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    s.len() <= max
        && (first.is_ascii_lowercase() || (lead_digit && first.is_ascii_digit()))
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || extra.contains(&c))
}

/// Parse a definition document. Unknown fields, unknown trigger kinds and a
/// document over [`MAX_DEFINITION_BYTES`] are refused with a typed issue;
/// nothing in the text is executed.
///
/// # Errors
/// The issues found.
pub fn parse(text: &str) -> Result<Definition, Vec<Issue>> {
    if text.len() > MAX_DEFINITION_BYTES {
        return Err(vec![issue(
            "/",
            "TOO_LARGE",
            format!(
                "a definition is at most {MAX_DEFINITION_BYTES} bytes; this one is {}",
                text.len()
            ),
        )]);
    }
    serde_json::from_str::<Definition>(text).map_err(|e| {
        let m = e.to_string();
        let code = if m.contains("unknown field") {
            "UNKNOWN_FIELD"
        } else if m.contains("unknown variant") {
            "UNKNOWN_TRIGGER"
        } else if m.contains("missing field") {
            "MISSING_FIELD"
        } else {
            "BAD_DOCUMENT"
        };
        vec![issue(
            format!("line {} column {}", e.line(), e.column()),
            code,
            m,
        )]
    })
}

/// Parse and validate: the issues, or the definition.
///
/// # Errors
/// Every issue found, not just the first.
pub fn parse_and_validate(text: &str) -> Result<Definition, Vec<Issue>> {
    let d = parse(text)?;
    let issues = validate(&d);
    if issues.is_empty() {
        Ok(d)
    } else {
        Err(issues)
    }
}

fn find_secret_shapes(path: &str, v: &serde_json::Value, out: &mut Vec<Issue>) {
    match v {
        serde_json::Value::String(text) => {
            if let Some(what) = modbit_secrets::redact::shape_of(text) {
                out.push(issue(
                    if path.is_empty() { "/" } else { path },
                    "SECRET_IN_DEFINITION",
                    format!(
                        "this text carries {what}; a definition holds no secret value, so use a credential handle the broker resolves for the principal at run time"
                    ),
                ));
            }
        }
        serde_json::Value::Array(items) => {
            for (i, x) in items.iter().enumerate() {
                find_secret_shapes(&format!("{path}/{i}"), x, out);
            }
        }
        serde_json::Value::Object(map) => {
            for (k, x) in map {
                find_secret_shapes(&format!("{path}/{k}"), x, out);
            }
        }
        _ => {}
    }
}

fn check_regex(path: &str, pattern: &str, out: &mut Vec<Issue>) {
    if pattern.len() > 256 {
        out.push(issue(
            path,
            "REGEX_TOO_LONG",
            "a pattern is at most 256 bytes",
        ));
        return;
    }
    if let Err(e) = compile_regex(pattern) {
        out.push(issue(path, "BAD_REGEX", e));
    }
}

/// Compile a user expression with the linear-time engine and a small size
/// limit, so an unsafe or oversized pattern is refused at save time
/// (AUT-B02).
///
/// # Errors
/// Why the pattern was refused.
pub fn compile_regex(pattern: &str) -> Result<regex::Regex, String> {
    regex::RegexBuilder::new(pattern)
        .size_limit(256 * 1024)
        .dfa_size_limit(512 * 1024)
        .nest_limit(32)
        .build()
        .map_err(|e| e.to_string())
}

fn check_globs(path: &str, globs: &[String], max: usize, out: &mut Vec<Issue>) {
    if globs.len() > max {
        out.push(issue(path, "TOO_MANY", format!("at most {max} entries")));
    }
    for (i, g) in globs.iter().enumerate() {
        if g.is_empty() || g.len() > 256 || g.contains('\0') {
            out.push(issue(
                format!("{path}/{i}"),
                "BAD_GLOB",
                "a glob is 1-256 bytes with no NUL",
            ));
        }
    }
}

fn check_filters(path: &str, f: &Filters, out: &mut Vec<Issue>) {
    check_globs(&format!("{path}/branches"), &f.branches, 16, out);
    check_globs(&format!("{path}/paths"), &f.paths, 16, out);
    for (name, list) in [("labels", &f.labels), ("authors", &f.authors)] {
        if list.len() > 16 {
            out.push(issue(
                format!("{path}/{name}"),
                "TOO_MANY",
                "at most 16 entries",
            ));
        }
        for (i, v) in list.iter().enumerate() {
            if v.is_empty() || v.len() > 100 {
                out.push(issue(
                    format!("{path}/{name}/{i}"),
                    "BAD_VALUE",
                    "1-100 bytes",
                ));
            }
        }
    }
    if let Some(r) = &f.title_regex {
        check_regex(&format!("{path}/title_regex"), r, out);
    }
    if let Some(r) = &f.text_regex {
        check_regex(&format!("{path}/text_regex"), r, out);
    }
    if f.fields.len() > 8 {
        out.push(issue(
            format!("{path}/fields"),
            "TOO_MANY",
            "at most 8 field filters",
        ));
    }
    for (i, ff) in f.fields.iter().enumerate() {
        let p = format!("{path}/fields/{i}");
        if !ff.pointer.starts_with('/') || ff.pointer.len() > 200 {
            out.push(issue(
                format!("{p}/pointer"),
                "BAD_POINTER",
                "a JSON pointer starts with `/` and is at most 200 bytes",
            ));
        }
        match (&ff.equals, &ff.regex) {
            (Some(_), Some(_)) | (None, None) => out.push(issue(
                p.clone(),
                "BAD_FILTER",
                "a field filter has exactly one of `equals` and `regex`",
            )),
            (Some(e), None) if e.len() > 500 => {
                out.push(issue(
                    format!("{p}/equals"),
                    "BAD_VALUE",
                    "at most 500 bytes",
                ));
            }
            (None, Some(r)) => check_regex(&format!("{p}/regex"), r, out),
            _ => {}
        }
    }
}

/// Validate a parsed definition. Returns every issue; an empty list means
/// the definition may be saved and, once an owner approves its hash, enabled.
#[must_use]
pub fn validate(d: &Definition) -> Vec<Issue> {
    let mut out = Vec::new();
    // AUT-D06: a definition holds no secret value; credentials are broker
    // handles resolved at run time for the principal. The shapes are the
    // Core's own redaction vocabulary, so what the Core would redact from a
    // log it also refuses to store in a definition. The issue names where
    // and what kind, never the value.
    if let Ok(doc) = serde_json::to_value(d) {
        find_secret_shapes("", &doc, &mut out);
    }
    if d.schema != SCHEMA {
        out.push(issue(
            "/schema",
            "BAD_SCHEMA",
            format!("this build reads `{SCHEMA}`, not `{}`", d.schema),
        ));
    }
    if !plain_name(&d.name, true, &['-', '_'], 64) {
        out.push(issue(
            "/name",
            "BAD_NAME",
            "1-64 characters of a-z, 0-9, `-` and `_`, starting with a letter or digit",
        ));
    }
    if d.description.len() > 1000 {
        out.push(issue("/description", "TOO_LONG", "at most 1000 bytes"));
    }
    if d.prompt.trim().is_empty() || d.prompt.len() > 16 * 1024 {
        out.push(issue(
            "/prompt",
            "BAD_PROMPT",
            "the prompt is 1-16384 bytes of text",
        ));
    }
    // Inputs.
    if d.inputs.len() > 16 {
        out.push(issue("/inputs", "TOO_MANY", "at most 16 inputs"));
    }
    let mut seen = BTreeSet::new();
    for (i, inp) in d.inputs.iter().enumerate() {
        let p = format!("/inputs/{i}");
        if !plain_name(&inp.name, false, &['_'], 40) {
            out.push(issue(
                format!("{p}/name"),
                "BAD_NAME",
                "a-z, 0-9 and `_`, starting with a letter, at most 40",
            ));
        }
        if !seen.insert(inp.name.clone()) {
            out.push(issue(
                format!("{p}/name"),
                "DUPLICATE",
                "duplicate input name",
            ));
        }
        if inp.ty == ValueType::Enum {
            if inp.values.is_empty() || inp.values.len() > 32 {
                out.push(issue(
                    format!("{p}/values"),
                    "BAD_ENUM",
                    "an enum input lists 1-32 values",
                ));
            }
        } else if !inp.values.is_empty() {
            out.push(issue(
                format!("{p}/values"),
                "BAD_ENUM",
                "only an enum input has values",
            ));
        }
        if inp.max_len.is_some_and(|m| m == 0 || m > 2000) {
            out.push(issue(format!("{p}/max_len"), "BAD_BOUND", "1-2000"));
        }
        if let Some(def) = &inp.default
            && let Err(why) = check_value(inp, def)
        {
            out.push(issue(format!("{p}/default"), "BAD_DEFAULT", why));
        }
    }
    if d.outputs.len() > 16 {
        out.push(issue("/outputs", "TOO_MANY", "at most 16 outputs"));
    }
    let mut seen_out = BTreeSet::new();
    for (i, o) in d.outputs.iter().enumerate() {
        let p = format!("/outputs/{i}");
        if !plain_name(&o.name, false, &['_'], 40) || !seen_out.insert(o.name.clone()) {
            out.push(issue(
                format!("{p}/name"),
                "BAD_NAME",
                "a unique name of a-z, 0-9 and `_`",
            ));
        }
        if o.ty == ValueType::Enum {
            out.push(issue(
                format!("{p}/type"),
                "BAD_TYPE",
                "an output is a string, an integer or a boolean",
            ));
        }
    }
    // Triggers.
    if d.triggers.is_empty() || d.triggers.len() > 8 {
        out.push(issue("/triggers", "BAD_TRIGGERS", "1-8 triggers"));
    }
    let mut ids = BTreeSet::new();
    for (i, t) in d.triggers.iter().enumerate() {
        let p = format!("/triggers/{i}");
        if !plain_name(t.id(), false, &['-', '_'], 40) || !ids.insert(t.id().to_owned()) {
            out.push(issue(
                format!("{p}/id"),
                "BAD_ID",
                "a unique id of a-z, 0-9, `-` and `_`, starting with a letter",
            ));
        }
        match t {
            Trigger::Schedule {
                cron, every_minutes, ..
            } => match (cron, every_minutes) {
                (Some(c), None) => match Cron::parse(c) {
                    Err(e) => out.push(issue(format!("{p}/cron"), "BAD_CRON", e.to_string())),
                    Ok(parsed) => match parsed.min_gap_minutes() {
                        Some(gap) if gap < SCHEDULE_FLOOR_MINUTES => out.push(issue(
                            format!("{p}/cron"),
                            "BELOW_FLOOR",
                            format!(
                                "this expression can fire {gap} minute(s) after a run; the floor is one run per {SCHEDULE_FLOOR_MINUTES} minutes"
                            ),
                        )),
                        _ => {
                            if parsed.next_after(0).is_none() {
                                out.push(issue(
                                    format!("{p}/cron"),
                                    "NEVER_FIRES",
                                    "this expression never fires",
                                ));
                            }
                        }
                    },
                },
                (None, Some(n)) => {
                    if i64::from(*n) < SCHEDULE_FLOOR_MINUTES || *n > 60 * 24 * 31 {
                        out.push(issue(
                            format!("{p}/every_minutes"),
                            "BELOW_FLOOR",
                            format!("an interval is {SCHEDULE_FLOOR_MINUTES}-44640 minutes"),
                        ));
                    }
                }
                _ => out.push(issue(
                    p.clone(),
                    "BAD_SCHEDULE",
                    "a schedule has exactly one of `cron` and `every_minutes`",
                )),
            },
            Trigger::Manual { .. } => {}
            Trigger::Event {
                source,
                event,
                actions,
                filters,
                ..
            } => {
                if !EVENT_SOURCES.contains(&source.as_str()) {
                    out.push(issue(
                        format!("{p}/source"),
                        "UNKNOWN_SOURCE",
                        format!("sources: {}", EVENT_SOURCES.join(", ")),
                    ));
                }
                match FORGE_EVENTS.iter().find(|(e, _)| e == event) {
                    None => out.push(issue(
                        format!("{p}/event"),
                        "UNKNOWN_EVENT",
                        format!(
                            "events: {}",
                            FORGE_EVENTS.iter().map(|(e, _)| *e).collect::<Vec<_>>().join(", ")
                        ),
                    )),
                    Some((_, allowed)) => {
                        for a in actions {
                            if !allowed.contains(&a.as_str()) {
                                out.push(issue(
                                    format!("{p}/actions"),
                                    "UNKNOWN_ACTION",
                                    format!("`{a}` is not an action of `{event}`"),
                                ));
                            }
                        }
                    }
                }
                check_filters(&format!("{p}/filters"), filters, &mut out);
            }
            Trigger::Webhook { name, filters, .. } => {
                if !plain_name(name, false, &['-', '_'], 40) {
                    out.push(issue(
                        format!("{p}/name"),
                        "BAD_NAME",
                        "a-z, 0-9, `-` and `_`, starting with a letter",
                    ));
                }
                check_filters(&format!("{p}/filters"), filters, &mut out);
            }
        }
    }
    // Profile.
    let pr = &d.profile;
    let mut caps = BTreeSet::new();
    for (i, c) in pr.capabilities.iter().enumerate() {
        let known =
            WRITE_CAPABILITIES.contains(&c.as_str()) || EXTERNAL_CAPABILITIES.contains(&c.as_str());
        if !known {
            out.push(issue(
                format!("/profile/capabilities/{i}"),
                "UNKNOWN_CAPABILITY",
                format!(
                    "`{c}` is not one an automation may ask for ({}); computer control and secret use are never available",
                    [WRITE_CAPABILITIES, EXTERNAL_CAPABILITIES].concat().join(", ")
                ),
            ));
        }
        if !caps.insert(c.clone()) {
            out.push(issue(
                format!("/profile/capabilities/{i}"),
                "DUPLICATE",
                "listed twice",
            ));
        }
        if WRITE_CAPABILITIES.contains(&c.as_str()) && pr.effects < Effects::ReversibleWrite {
            out.push(issue(
                format!("/profile/capabilities/{i}"),
                "CEILING_TOO_LOW",
                format!("`{c}` needs effects of at least reversible_write"),
            ));
        }
        if EXTERNAL_CAPABILITIES.contains(&c.as_str()) && pr.effects < Effects::ExternalSideEffect {
            out.push(issue(
                format!("/profile/capabilities/{i}"),
                "CEILING_TOO_LOW",
                format!("`{c}` needs effects of external_side_effect"),
            ));
        }
    }
    check_globs("/profile/paths", &pr.paths, 16, &mut out);
    for (i, p) in pr.paths.iter().enumerate() {
        let bad = p.starts_with('/')
            || p.starts_with('\\')
            || p.contains(':')
            || p.split(['/', '\\']).any(|s| s == "..");
        if bad {
            out.push(issue(
                format!("/profile/paths/{i}"),
                "PATH_ESCAPES",
                "a path is relative to the workspace and never leaves it",
            ));
        }
    }
    if !pr.paths.is_empty() && !pr.capabilities.iter().any(|c| c == "fs.write") {
        out.push(issue(
            "/profile/paths",
            "PATHS_WITHOUT_WRITE",
            "paths narrow `fs.write`, which is not listed",
        ));
    }
    if pr.hosts.len() > 16 {
        out.push(issue("/profile/hosts", "TOO_MANY", "at most 16 hosts"));
    }
    for (i, h) in pr.hosts.iter().enumerate() {
        let ok = !h.is_empty()
            && h.len() <= 253
            && h.chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '.' || c == '-');
        if !ok {
            out.push(issue(
                format!("/profile/hosts/{i}"),
                "BAD_HOST",
                "a lowercase host name",
            ));
        }
    }
    if !pr.hosts.is_empty() && !pr.capabilities.iter().any(|c| c == "network.egress") {
        out.push(issue(
            "/profile/hosts",
            "HOSTS_WITHOUT_EGRESS",
            "hosts narrow `network.egress`, which is not listed",
        ));
    }
    if pr.capabilities.iter().any(|c| c == "network.egress") && pr.hosts.is_empty() {
        out.push(issue(
            "/profile/hosts",
            "EGRESS_WITHOUT_HOSTS",
            "network access names the hosts it may reach",
        ));
    }
    // Principal.
    if let Principal::ServiceAccount { id } = &d.principal
        && !plain_name(id, false, &['-'], 40)
    {
        out.push(issue(
            "/principal/id",
            "BAD_NAME",
            "a-z, 0-9 and `-`, starting with a letter",
        ));
    }
    if let Some(g) = &d.gate {
        if g.prompt.trim().is_empty() || g.prompt.len() > 4096 {
            out.push(issue("/gate/prompt", "BAD_PROMPT", "1-4096 bytes of text"));
        }
        if !(1..=8).contains(&g.max_turns) {
            out.push(issue("/gate/max_turns", "BAD_BOUND", "1-8"));
        }
    }
    // Concurrency, missed, limits, budget.
    if !(1..=10).contains(&d.concurrency.queue_max) {
        out.push(issue("/concurrency/queue_max", "BAD_BOUND", "1-10"));
    }
    if !(1..=43_200).contains(&d.missed.catch_up_window_minutes) {
        out.push(issue(
            "/missed/catch_up_window_minutes",
            "BAD_BOUND",
            "1-43200",
        ));
    }
    let l = &d.limits;
    for (path, ok, range) in [
        (
            "/limits/deadline_minutes",
            (1..=1440).contains(&l.deadline_minutes),
            "1-1440",
        ),
        (
            "/limits/max_cost_minor",
            l.max_cost_minor.is_none_or(|c| (1..=100_000).contains(&c)),
            "1-100000",
        ),
        (
            "/limits/max_turns",
            (1..=200).contains(&l.max_turns),
            "1-200",
        ),
        (
            "/limits/max_tool_calls",
            (1..=1000).contains(&l.max_tool_calls),
            "1-1000",
        ),
        (
            "/limits/approval_wait_minutes",
            (1..=10_080).contains(&l.approval_wait_minutes),
            "1-10080",
        ),
        (
            "/budget/max_runs_per_hour",
            (1..=60).contains(&d.budget.max_runs_per_hour),
            "1-60",
        ),
        (
            "/budget/daily_budget_minor",
            d.budget
                .daily_budget_minor
                .is_none_or(|b| (1..=10_000_000).contains(&b)),
            "1-10000000",
        ),
    ] {
        if !ok {
            out.push(issue(path, "BAD_BOUND", range));
        }
    }
    if let (Some(daily), Some(cost)) = (d.budget.daily_budget_minor, d.limits.max_cost_minor)
        && daily < cost
    {
        out.push(issue(
            "/budget/daily_budget_minor",
            "BAD_BOUND",
            "the daily budget cannot be below one run's cost limit",
        ));
    }
    out
}

fn check_value(spec: &InputSpec, v: &serde_json::Value) -> Result<(), String> {
    match (spec.ty, v) {
        (ValueType::String, serde_json::Value::String(s)) => {
            let max = spec.max_len.unwrap_or(200) as usize;
            if s.len() > max {
                Err(format!("longer than {max} bytes"))
            } else if s.contains('\0') {
                Err("contains NUL".into())
            } else {
                Ok(())
            }
        }
        (ValueType::Integer, serde_json::Value::Number(n)) if n.is_i64() => Ok(()),
        (ValueType::Boolean, serde_json::Value::Bool(_)) => Ok(()),
        (ValueType::Enum, serde_json::Value::String(s)) => {
            if spec.values.contains(s) {
                Ok(())
            } else {
                Err(format!("not one of {}", spec.values.join(", ")))
            }
        }
        _ => Err(format!("expected a {:?}", spec.ty).to_lowercase()),
    }
}

/// Check the values a manual or test run supplies against the declared
/// inputs and fill in defaults. An unknown input is refused (AUT-B01 typed
/// inputs). Returns the resolved object.
///
/// # Errors
/// Issues for an unknown, mistyped or missing input.
pub fn resolve_inputs(
    d: &Definition,
    supplied: &serde_json::Map<String, serde_json::Value>,
) -> Result<serde_json::Map<String, serde_json::Value>, Vec<Issue>> {
    let mut out = serde_json::Map::new();
    let mut issues = Vec::new();
    for k in supplied.keys() {
        if !d.inputs.iter().any(|i| &i.name == k) {
            issues.push(issue(
                format!("/inputs/{k}"),
                "UNKNOWN_INPUT",
                format!("`{k}` is not an input of `{}`", d.name),
            ));
        }
    }
    for spec in &d.inputs {
        match supplied.get(&spec.name).or(spec.default.as_ref()) {
            Some(v) => match check_value(spec, v) {
                Ok(()) => {
                    out.insert(spec.name.clone(), v.clone());
                }
                Err(why) => issues.push(issue(format!("/inputs/{}", spec.name), "BAD_INPUT", why)),
            },
            None if spec.required => issues.push(issue(
                format!("/inputs/{}", spec.name),
                "MISSING_INPUT",
                "required and has no default",
            )),
            None => {}
        }
    }
    if issues.is_empty() {
        Ok(out)
    } else {
        Err(issues)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn minimal() -> String {
        serde_json::json!({
            "schema": SCHEMA,
            "name": "drift",
            "prompt": "Report whether main moved ahead.",
            "triggers": [{"kind": "manual", "id": "now"}]
        })
        .to_string()
    }

    #[test]
    fn a_minimal_definition_is_read_only_with_bounded_defaults() {
        let d = parse_and_validate(&minimal()).unwrap();
        assert!(d.profile.is_read_only());
        assert_eq!(d.principal, Principal::Creator);
        assert_eq!(d.limits.approval_wait_minutes, 1440);
        assert_eq!(d.concurrency.policy, ConcurrencyPolicy::Skip);
        assert_eq!(d.missed.policy, MissedPolicy::Skip);
        assert_eq!(d.hash(), parse_and_validate(&minimal()).unwrap().hash());
    }

    #[test]
    fn unknown_fields_triggers_and_inputs_are_refused_with_typed_codes() {
        let mut v: serde_json::Value = serde_json::from_str(&minimal()).unwrap();
        v["extra"] = serde_json::json!(1);
        assert_eq!(parse(&v.to_string()).unwrap_err()[0].code, "UNKNOWN_FIELD");
        let mut v: serde_json::Value = serde_json::from_str(&minimal()).unwrap();
        v["triggers"][0] = serde_json::json!({"kind": "chat_message", "id": "x"});
        assert_eq!(
            parse(&v.to_string()).unwrap_err()[0].code,
            "UNKNOWN_TRIGGER"
        );
        let mut v: serde_json::Value = serde_json::from_str(&minimal()).unwrap();
        v["triggers"][0]["surprise"] = serde_json::json!(true);
        assert_eq!(parse(&v.to_string()).unwrap_err()[0].code, "UNKNOWN_FIELD");
        let d = parse_and_validate(&minimal()).unwrap();
        let mut supplied = serde_json::Map::new();
        supplied.insert("nope".into(), serde_json::json!("x"));
        assert_eq!(
            resolve_inputs(&d, &supplied).unwrap_err()[0].code,
            "UNKNOWN_INPUT"
        );
    }

    #[test]
    fn sizes_floors_and_unsafe_expressions_are_bounded() {
        let big = format!(
            "{{\"schema\":\"{SCHEMA}\",\"name\":\"x\",\"prompt\":\"{}\",\"triggers\":[]}}",
            "a".repeat(MAX_DEFINITION_BYTES)
        );
        assert_eq!(parse(&big).unwrap_err()[0].code, "TOO_LARGE");
        let mut v: serde_json::Value = serde_json::from_str(&minimal()).unwrap();
        v["triggers"] = serde_json::json!([
            {"kind": "schedule", "id": "fast", "cron": "*/2 * * * *"},
            {"kind": "schedule", "id": "fast2", "every_minutes": 1},
            {"kind": "event", "id": "pr", "source": "forge", "event": "pull_request",
             "filters": {"title_regex": "(a+)+$(", "branches": ["main"]}},
        ]);
        let issues = parse_and_validate(&v.to_string()).unwrap_err();
        let codes: Vec<_> = issues.iter().map(|i| i.code.as_str()).collect();
        assert!(codes.contains(&"BELOW_FLOOR"), "{codes:?}");
        assert!(codes.contains(&"BAD_REGEX"), "{codes:?}");
        // A pathological-looking but valid pattern is accepted: the engine is
        // linear-time, so there is nothing to blow up.
        assert!(compile_regex("(a+)+$").is_ok());
        assert!(compile_regex(&"(".repeat(100)).is_err());
    }

    #[test]
    fn capabilities_are_a_closed_set_and_must_be_listed_exactly() {
        let mut v: serde_json::Value = serde_json::from_str(&minimal()).unwrap();
        v["profile"] = serde_json::json!({
            "effects": "read_only",
            "capabilities": ["fs.write", "computer.control", "secret.use"],
            "paths": ["../escape"],
        });
        let codes: Vec<String> = parse_and_validate(&v.to_string())
            .unwrap_err()
            .into_iter()
            .map(|i| i.code)
            .collect();
        for want in ["UNKNOWN_CAPABILITY", "CEILING_TOO_LOW", "PATH_ESCAPES"] {
            assert!(codes.iter().any(|c| c == want), "{want} in {codes:?}");
        }
        v["profile"] = serde_json::json!({
            "effects": "reversible_write",
            "capabilities": ["fs.write"],
            "paths": ["reports/**"],
        });
        let d = parse_and_validate(&v.to_string()).unwrap();
        assert!(d.needs_listed_approval());
    }

    #[test]
    fn a_secret_value_in_any_text_of_a_definition_is_refused_without_echoing_it() {
        let key = "sk-live-0123456789abcdefghij";
        for (field, doc) in [
            ("/prompt", {
                let mut v: serde_json::Value = serde_json::from_str(&minimal()).unwrap();
                v["prompt"] = serde_json::json!(format!("call the api with {key}"));
                v
            }),
            ("/description", {
                let mut v: serde_json::Value = serde_json::from_str(&minimal()).unwrap();
                v["description"] = serde_json::json!("token ghp_abcdefghijklmnopqrstuvwxyz0123");
                v
            }),
            ("/inputs/0/default", {
                let mut v: serde_json::Value = serde_json::from_str(&minimal()).unwrap();
                v["inputs"] = serde_json::json!([
                    {"name": "k", "type": "string", "default": "Bearer abcdefghijklmnopqrstuvwxyz"}
                ]);
                v
            }),
        ] {
            let issues = parse_and_validate(&doc.to_string()).unwrap_err();
            let hit = issues
                .iter()
                .find(|i| i.code == "SECRET_IN_DEFINITION")
                .unwrap_or_else(|| panic!("{field}: {issues:?}"));
            assert_eq!(hit.path, field);
            assert!(
                !hit.message.contains("0123456789abcdefghij")
                    && !hit.message.contains("abcdefghijklmnopqrstuvwxyz"),
                "the refusal never repeats the value: {}",
                hit.message
            );
        }
        // A broker handle is not a secret.
        let mut v: serde_json::Value = serde_json::from_str(&minimal()).unwrap();
        v["prompt"] = serde_json::json!("use the credential handle `forge-token` for the forge");
        assert!(parse_and_validate(&v.to_string()).is_ok());
    }

    #[test]
    fn a_definition_that_can_act_needs_a_cost_limit_before_it_can_be_enabled() {
        let reader = parse_and_validate(&minimal()).unwrap();
        assert!(reader.enable_issues().is_empty(), "read-only needs none");
        let mut v: serde_json::Value = serde_json::from_str(&minimal()).unwrap();
        v["profile"] = serde_json::json!({
            "effects": "reversible_write", "capabilities": ["fs.write"], "paths": ["reports/**"]
        });
        let writer = parse_and_validate(&v.to_string()).unwrap();
        let issues = writer.enable_issues();
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, "BUDGET_REQUIRED");
        v["limits"] = serde_json::json!({"max_cost_minor": 500});
        assert!(
            parse_and_validate(&v.to_string())
                .unwrap()
                .enable_issues()
                .is_empty()
        );
    }

    #[test]
    fn expressions_that_need_backtracking_are_refused_and_the_rest_cannot_blow_up() {
        // Backreferences and look-around are what make matching exponential;
        // the linear-time engine does not compile them, so save refuses them.
        for bad in [r"(a+)\1", "(?=a)b", "(?<!a)b", r"^(\w+)\s\1$"] {
            assert!(compile_regex(bad).is_err(), "{bad}");
        }
        // The classic catastrophic shape is harmless here: a megabyte of
        // hostile text matches in linear time.
        let re = compile_regex("^(a+)+$").unwrap();
        let hostile = format!("{}!", "a".repeat(1_000_000));
        let started = std::time::Instant::now();
        assert!(!re.is_match(&hostile));
        assert!(
            started.elapsed() < std::time::Duration::from_secs(5),
            "{:?}",
            started.elapsed()
        );
    }

    #[test]
    fn the_hash_changes_with_any_change_of_meaning() {
        let a = parse_and_validate(&minimal()).unwrap();
        let mut b = a.clone();
        b.prompt.push(' ');
        assert_ne!(a.hash(), b.hash());
        let mut c = a.clone();
        c.profile.effects = Effects::ReversibleWrite;
        assert_ne!(a.hash(), c.hash());
    }

    #[test]
    fn typed_inputs_resolve_defaults_and_refuse_wrong_types() {
        let mut v: serde_json::Value = serde_json::from_str(&minimal()).unwrap();
        v["inputs"] = serde_json::json!([
            {"name": "branch", "type": "string", "default": "main"},
            {"name": "depth", "type": "integer", "required": true},
            {"name": "level", "type": "enum", "values": ["a", "b"], "default": "a"},
        ]);
        let d = parse_and_validate(&v.to_string()).unwrap();
        let mut s = serde_json::Map::new();
        s.insert("depth".into(), serde_json::json!(3));
        let r = resolve_inputs(&d, &s).unwrap();
        assert_eq!(r["branch"], "main");
        assert_eq!(r["level"], "a");
        s.insert("depth".into(), serde_json::json!("three"));
        assert_eq!(resolve_inputs(&d, &s).unwrap_err()[0].code, "BAD_INPUT");
        s.clear();
        assert_eq!(resolve_inputs(&d, &s).unwrap_err()[0].code, "MISSING_INPUT");
    }
}
