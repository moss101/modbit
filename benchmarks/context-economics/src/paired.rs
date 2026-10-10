//! The paired DIRECT / CASCADE / CRITIQUE trial (REQ-PX-136, REQ-PX-133;
//! QUAL-PX-136 and QUAL-PX-133 live halves): the same pinned task set, the
//! same repositories and verification commands, the same model bindings, run
//! three ways through the real Core, and reported with intervals whichever way
//! the numbers point.
//!
//! Every trial is a fresh repository, a fresh profile and a fresh Core,
//! driven through `modbit-cli` (plus the typed commands the CLI has no verb
//! for, through [`crate::core_link`]). A signed registry with two bindings is
//! active on every arm, so the three arms differ only in what the product is
//! told to do:
//!
//! - **DIRECT**: the run is pinned to the opening binding; the compiled plan
//!   is the direct one (its reviewer slot is never activated: the assurance
//!   policy does not require review of a small change).
//! - **CASCADE**: the same first leg. When it ends without an accepted
//!   candidate, the operator admits a conditional plan whose prevalidated
//!   stronger-solver slot is the second binding and resumes the run: the
//!   Core's own quality boundary (REQ-EPR-006) activates that slot. The
//!   router never chooses this plan at cold start (it prefers the cheapest
//!   feasible plan and has no evidence for the continuation), so the arm is
//!   the operator path, not the compiler's choice, and the report says so.
//! - **CRITIQUE**: the same first leg under an organization assurance layer
//!   (`MODBIT_POLICY_FILE`, `review_required_at = LOW`) that requires an
//!   independent review of every candidate: the plan's reviewer slot, bound
//!   to the second binding, is activated by the gate (REQ-EPR-007).
//!
//! All three arms run under an organization layer that requires the task's
//! check to pass at completion (`required_checks`). Without it the Acceptance
//! Gate excuses a check that was already failing before the candidate (the
//! very bug the task asks to fix), accepts an unfixed candidate, and the
//! cascade's quality boundary, which activates only on a REJECT, could never
//! fire; that false accept was observed on the real Core while building this
//! harness and is exactly what the gate-error counts below measure.
//!
//! What is scored. A run is a *verified success* only when the Core's own
//! completion gating says so (`RequestOutcome.verified_success`), the task's
//! original `check.py` (restored to its pristine content, run outside the
//! agent's tree) passes on the final tree, and the agent left `check.py`
//! untouched. The Core's gate verdict at the end of a leg is compared with
//! that independent check to count gate false accepts and false rejects.
//! Cost is priced from the usage the gateway returned for every model
//! invocation of the session (the review task's included), at the catalog
//! list prices, and cross-checked against the Core's accounting record.
//!
//! What the report does not claim: the task set is thirty small, single-file
//! Python repair tasks, so a ceiling effect is likely; the two bindings may be
//! the same model behind two endpoints (the report names them), in which case
//! the stronger slot is a second attempt on a different wire, not a stronger
//! model; the independent check is the task's own acceptance, not a held-out
//! suite.

use std::collections::{BTreeMap, VecDeque};
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use modbit_providers::registry::{
    Economics, Governance, Latency, QualityFloor, REGISTRY_SCHEMA_VERSION, RegistryDocument,
    RegistryEntry, SignedRegistry,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::core_link::{CoreLink, id_of};
use crate::projection_trial::{MeanCi, Proportion, mean_ci, wilson};
use crate::spend::{Price, SpendMeter, SpendRecord, price_from_catalog};
use crate::suite::{CHECK_ARGV, TrialTask, builtin_suite, materialize};

/// The report schema.
pub const SCHEMA: &str = "modbit.paired-trial/1";

/// One arm.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Arm {
    /// One pinned solver, no continuation, no review.
    Direct,
    /// The operator-admitted stronger-solver continuation on rejection.
    Cascade,
    /// Independent review of every candidate.
    Critique,
}

impl Arm {
    /// Every arm, the baseline first.
    pub const ALL: [Arm; 3] = [Arm::Direct, Arm::Cascade, Arm::Critique];

    /// The report label.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Direct => "direct",
            Self::Cascade => "cascade",
            Self::Critique => "critique",
        }
    }

    /// Parse a label.
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|a| a.label() == s)
    }
}

/// A model binding the registry names.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Binding {
    /// Gateway endpoint name (`openai`, `anthropic`).
    pub endpoint: String,
    /// Model id.
    pub model: String,
    /// List price, USD per million tokens.
    pub price: Price,
}

impl Binding {
    /// `endpoint/model`.
    #[must_use]
    pub fn label(&self) -> String {
        format!("{}/{}", self.endpoint, self.model)
    }
}

/// What a paired run needs.
#[derive(Clone, Debug)]
pub struct PairedConfig {
    /// `modbit-cli`.
    pub cli: PathBuf,
    /// `modbit-core`.
    pub core: PathBuf,
    /// Scratch root for trials.
    pub work: PathBuf,
    /// The binding every first leg is pinned to.
    pub opener: Binding,
    /// The binding of the stronger-solver and reviewer slots.
    pub second: Binding,
    /// Extra environment for the CLI and the Core it spawns (the provider
    /// endpoints of a scripted run).
    pub env: Vec<(String, String)>,
    /// Turn budget per leg.
    pub max_turns: u32,
    /// Wall-clock limit of one trial.
    pub trial_timeout: Duration,
    /// Trials in flight at once.
    pub parallel: usize,
    /// Repeats per task and arm.
    pub repeats: u32,
    /// Whether the provider is a live one.
    pub live: bool,
    /// The one answer given to a typed question.
    pub question_answer: String,
}

/// The pinned task set.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskSet {
    /// Stable id.
    pub id: String,
    /// Version, bumped when a task changes.
    pub version: String,
    /// Task ids, in run order.
    pub tasks: Vec<String>,
    /// SHA-256 over every task's id, goal, starting files and reference.
    pub digest: String,
    /// What the tasks are.
    pub description: String,
}

/// The digest of a task list: what a pin must match.
#[must_use]
pub fn tasks_digest(tasks: &[TrialTask]) -> String {
    let mut h = Sha256::new();
    for t in tasks {
        for part in [t.id.as_str(), t.goal.as_str()] {
            h.update(part.as_bytes());
            h.update([0]);
        }
        for (p, c) in t.files.iter().chain(t.reference.iter()) {
            h.update(p.as_bytes());
            h.update([0]);
            h.update(c.as_bytes());
            h.update([0]);
        }
        h.update([1]);
    }
    hex::encode(h.finalize())
}

/// Load the pin file and check it against the suite it names: the same ids in
/// the same order and the same digest, and at least `min_tasks` of them.
///
/// # Errors
/// The file is unreadable, a task is missing, the digest differs or there are
/// too few tasks.
pub fn load_taskset(
    path: &Path,
    suite: &[TrialTask],
    min_tasks: usize,
) -> Result<(TaskSet, Vec<TrialTask>), String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let pin: TaskSet =
        serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut picked = Vec::new();
    for id in &pin.tasks {
        let t = suite
            .iter()
            .find(|t| &t.id == id)
            .ok_or_else(|| format!("the pin names task `{id}`, which the suite does not have"))?;
        picked.push(t.clone());
    }
    if picked.len() < min_tasks {
        return Err(format!(
            "the pinned task set has {} tasks; at least {min_tasks} are required",
            picked.len()
        ));
    }
    let digest = tasks_digest(&picked);
    if digest != pin.digest {
        return Err(format!(
            "the pinned task set digest {} does not match the tasks ({digest}): the suite changed after it was pinned",
            pin.digest
        ));
    }
    Ok((pin, picked))
}

/// The pin for the built-in suite (what `taskset.json` is generated from).
#[must_use]
pub fn builtin_taskset() -> TaskSet {
    let suite = builtin_suite();
    TaskSet {
        id: "px136-paired-30".into(),
        version: "1.0.0".into(),
        tasks: suite.iter().map(|t| t.id.clone()).collect(),
        digest: tasks_digest(&suite),
        description: "The thirty small Python repair tasks of the projection trial suite: each repository's mandatory check (`python3 -B check.py`, declared in .modbit/verification.json) fails until the task is done, and every task ships a reference solution that makes it pass.".into(),
    }
}

// ---- The signed registry ----

/// A signed two-binding registry for the trials, with the trust entry the
/// Core needs (`MODBIT_REGISTRY_KEYS`).
#[derive(Clone, Debug)]
pub struct RegistryBundle {
    /// The signed document (JSON).
    pub signed_json: String,
    /// `key-id:public-key-hex`.
    pub trusted_key: String,
    /// The generation the document carries.
    pub generation: String,
    /// SHA-256 of the signed document's text.
    pub digest: String,
}

/// The money scale of the registry: the product allows at most four decimal
/// places, so one minor unit is a hundredth of a cent.
pub const REGISTRY_SCALE: u8 = 4;

fn minor_per_mtok(usd: f64) -> u64 {
    (usd * 10f64.powi(i32::from(REGISTRY_SCALE))).round() as u64
}

/// Build and sign the registry: both bindings may be solver and reviewer, the
/// prices are the catalog's list prices in hundredths of a cent (scale 4, the most the product allows; each attempt's accounting rounds up to the next unit), and the
/// `auto` floor allows two dollars per request. The key is generated for this
/// run only.
///
/// # Panics
/// The registry types refuse to serialize (they do not).
#[must_use]
pub fn build_registry(opener: &Binding, second: &Binding, seed: [u8; 32]) -> RegistryBundle {
    use ed25519_dalek::{Signer, SigningKey};
    let key = SigningKey::from_bytes(&seed);
    let now = modbit_domain::Timestamp::now().0;
    let entry = |b: &Binding| RegistryEntry {
        endpoint: b.endpoint.clone(),
        provider: b.endpoint.clone(),
        family: b.model.clone(),
        model: b.model.clone(),
        roles: vec!["solver".into(), "reviewer".into()],
        input_modalities: vec!["text".into()],
        context_tokens: 128_000,
        max_output_tokens: 16_384,
        tools: true,
        vision: false,
        reasoning: false,
        structured_output: false,
        economics: Economics {
            input_per_mtok_minor: minor_per_mtok(b.price.input_per_mtok_usd),
            output_per_mtok_minor: minor_per_mtok(b.price.output_per_mtok_usd),
            currency: "USD".into(),
            scale: REGISTRY_SCALE,
            cached_input_per_mtok_minor: None,
            cache_write_per_mtok_minor: None,
            cache_ttl_ms: None,
        },
        // Nominal service figures: selection does not read them.
        latency: Latency {
            p50_ms: 5_000,
            p95_ms: 30_000,
        },
        governance: Governance {
            data_residency: "unspecified".into(),
            retains_prompts: false,
            allowed_profiles: vec![],
        },
        revoked: false,
        fallbacks: vec![],
    };
    let mut entries = vec![entry(opener)];
    if second.label() != opener.label() {
        entries.push(entry(second));
    }
    let generation = format!("paired-{}", hex::encode(&seed[..4]));
    let document = RegistryDocument {
        promotion: None,
        schema_version: REGISTRY_SCHEMA_VERSION,
        registry_generation: generation.clone(),
        stats_version: "paired-stats".into(),
        issued_at_ms: now - 60_000,
        expires_at_ms: now + 7 * 86_400_000,
        quality_floors: vec![QualityFloor {
            mode: "auto".into(),
            min_quality: 0.72,
            max_cost_minor: 2 * 10u64.pow(u32::from(REGISTRY_SCALE)),
            currency: "USD".into(),
            scale: REGISTRY_SCALE,
        }],
        entries,
    };
    let document_json = serde_json::to_string(&document).expect("registry document");
    let signed = SignedRegistry {
        key_id: "bench".into(),
        signature_hex: hex::encode(key.sign(document_json.as_bytes()).to_bytes()),
        document_json,
    };
    let signed_json = serde_json::to_string(&signed).expect("signed registry");
    RegistryBundle {
        digest: hex::encode(Sha256::digest(signed_json.as_bytes())),
        trusted_key: format!("bench:{}", hex::encode(key.verifying_key().to_bytes())),
        generation,
        signed_json,
    }
}

// ---- A trial ----

/// What the gate said at the end of a leg and what the independent check said
/// about the same tree.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GateObservation {
    /// `first` (the end of the first leg of a cascade) or `final`.
    pub leg: String,
    /// `ACCEPT` | `REJECT` | `INCONCLUSIVE` | `NONE` (the gate never ran).
    pub verdict: String,
    /// The independent check on that tree.
    pub oracle_pass: bool,
}

/// One statistics aggregate the run's snapshot held.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct StatSample {
    /// The statistics key.
    pub key: String,
    /// Observations.
    pub samples: u32,
    /// Of them verified successes.
    pub successes: u32,
    /// Whether any observation had a reported cost.
    pub cost_known: bool,
    /// Observations with no reported cost.
    pub unknown_cost_samples: u32,
    /// Mean complete cost per observation, minor units.
    pub mean_cost_minor: u64,
}

/// The Core's complete accounting of the request.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Accounting {
    /// Known spend plus held unknowns, minor units.
    pub total_minor: u64,
    /// Of which held for work of unknown usage.
    pub unknown_minor: u64,
    /// Currency.
    pub currency: String,
    /// Scale.
    pub scale: u32,
    /// The label the path earned.
    pub path_label: String,
    /// The Core's final outcome (`pass` | `fail` | ...).
    pub final_outcome: String,
}

/// One trial, whatever became of it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PairedRun {
    /// Task id.
    pub task: String,
    /// The arm.
    pub arm: Arm,
    /// Repeat index.
    pub repeat: u32,
    /// Session id (hex).
    pub session_id: String,
    /// Task id in the Core (hex).
    pub task_id: String,
    /// Run id (hex), from the compiled plan.
    pub run_id: String,
    /// The Core task's final state.
    pub state: String,
    /// The Core's own completion gating said verified.
    pub core_verified: bool,
    /// The pristine check passes on the final tree.
    pub oracle_pass: bool,
    /// `check.py` is byte-identical to the original.
    pub check_untouched: bool,
    /// All three.
    pub verified_success: bool,
    /// Wall time of the whole trial, milliseconds.
    pub wall_ms: u64,
    /// Cost of every model invocation of the session at list prices, USD
    /// (`None` when some usage could not be priced).
    pub cost_usd: Option<f64>,
    /// Model invocations that returned no usage (their cost is unknown, not
    /// zero).
    pub calls_without_usage: u32,
    /// Usage records whose binding the catalog does not price.
    pub unpriced_usage: u32,
    /// Model invocations in the session.
    pub model_calls: u32,
    /// Input tokens.
    pub input_tokens: u64,
    /// Output tokens.
    pub output_tokens: u64,
    /// Bindings that answered, `endpoint/model`, with the number of calls.
    pub answered_by: BTreeMap<String, u32>,
    /// A stronger-solver slot was activated.
    pub escalated: bool,
    /// The harness tried to give the run a continuation.
    pub escalation_attempted: bool,
    /// A reviewer leg was activated.
    pub reviewed: bool,
    /// Reviewer verdicts, in order.
    pub review_verdicts: Vec<String>,
    /// Revisions the reviewer's findings caused.
    pub revisions: u32,
    /// Gate observations paired with the independent check.
    pub gates: Vec<GateObservation>,
    /// The run ended waiting for a person (a question, an approval, a stuck
    /// task) at least once: the human-intervention proxy.
    pub needed_person: bool,
    /// Typed questions and approvals the harness answered.
    pub interactions: u32,
    /// The accounting record of the request.
    pub accounting: Option<Accounting>,
    /// Statistics aggregates of the run's snapshot.
    pub stats: Vec<StatSample>,
    /// The snapshot version and digest.
    pub stats_version: String,
    /// The snapshot digest.
    pub stats_digest: String,
    /// SHA-256 of the retained event log.
    pub events_sha256: String,
    /// Where the retained log is, relative to the output directory.
    pub events_file: String,
    /// The trial timed out and was cancelled.
    pub timed_out: bool,
    /// Why the trial produced no valid measurement (a harness or Core
    /// failure, not the model's).
    pub error: Option<String>,
}

impl PairedRun {
    fn blank(task: &str, arm: Arm, repeat: u32) -> Self {
        Self {
            task: task.to_owned(),
            arm,
            repeat,
            session_id: String::new(),
            task_id: String::new(),
            run_id: String::new(),
            state: "NotRun".into(),
            core_verified: false,
            oracle_pass: false,
            check_untouched: false,
            verified_success: false,
            wall_ms: 0,
            cost_usd: None,
            calls_without_usage: 0,
            unpriced_usage: 0,
            model_calls: 0,
            input_tokens: 0,
            output_tokens: 0,
            answered_by: BTreeMap::new(),
            escalated: false,
            escalation_attempted: false,
            reviewed: false,
            review_verdicts: Vec::new(),
            revisions: 0,
            gates: Vec::new(),
            needed_person: false,
            interactions: 0,
            accounting: None,
            stats: Vec::new(),
            stats_version: String::new(),
            stats_digest: String::new(),
            events_sha256: String::new(),
            events_file: String::new(),
            timed_out: false,
            error: None,
        }
    }
}

struct Out {
    code: i32,
    stdout: String,
    stderr: String,
    timed_out: bool,
}

fn run_process(cmd: &mut Command, timeout: Duration) -> Out {
    cmd.stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            return Out {
                code: -1,
                stdout: String::new(),
                stderr: format!("spawn failed: {e}"),
                timed_out: false,
            };
        }
    };
    let drain = |pipe: Option<Box<dyn std::io::Read + Send>>| {
        pipe.map(|mut p| {
            std::thread::spawn(move || {
                let mut s = String::new();
                let _ = p.read_to_string(&mut s);
                s
            })
        })
    };
    let out_t = drain(
        child
            .stdout
            .take()
            .map(|p| Box::new(p) as Box<dyn std::io::Read + Send>),
    );
    let err_t = drain(
        child
            .stderr
            .take()
            .map(|p| Box::new(p) as Box<dyn std::io::Read + Send>),
    );
    let started = Instant::now();
    let mut timed_out = false;
    let status = loop {
        match child.try_wait() {
            Ok(Some(st)) => break Some(st),
            Ok(None) => {
                if started.elapsed() > timeout {
                    timed_out = true;
                    let _ = child.kill();
                    break child.wait().ok();
                }
                std::thread::sleep(Duration::from_millis(100));
            }
            Err(_) => break None,
        }
    };
    Out {
        code: status.and_then(|s| s.code()).unwrap_or(-1),
        stdout: out_t.and_then(|t| t.join().ok()).unwrap_or_default(),
        stderr: err_t.and_then(|t| t.join().ok()).unwrap_or_default(),
        timed_out,
    }
}

/// What the harness shares between its workers.
pub struct Shared {
    /// The configuration.
    pub cfg: PairedConfig,
    /// The signed registry.
    pub registry: RegistryBundle,
    /// Where the registry file and the policy layer live.
    pub files: PathBuf,
    /// The spend meter.
    pub meter: Arc<SpendMeter>,
    /// The most one trial has cost so far, USD (the reserve for the next).
    pub max_trial_usd: Mutex<f64>,
    /// Where retained logs go.
    pub out: PathBuf,
}

fn kv(text: &str, key: &str) -> Option<String> {
    text.split_whitespace()
        .find_map(|t| t.strip_prefix(&format!("{key}=")))
        .map(str::to_owned)
}

fn first_word_after(text: &str, prefix: &str) -> Option<String> {
    text.lines()
        .find_map(|l| l.strip_prefix(prefix))
        .and_then(|rest| rest.split_whitespace().next())
        .map(str::to_owned)
}

/// The independent check: the task's original `check.py` on a copy of the
/// tree, outside the agent's workspace. Returns `(passed, untouched)`.
fn independent_check(task: &TrialTask, workspace: &Path) -> (bool, bool) {
    let original = task
        .files
        .iter()
        .find(|(p, _)| p == "check.py")
        .map(|(_, c)| c.clone())
        .unwrap_or_default();
    let current = std::fs::read_to_string(workspace.join("check.py")).unwrap_or_default();
    let untouched = current.replace("\r\n", "\n") == original;
    let Ok(copy) = tempfile::tempdir() else {
        return (false, untouched);
    };
    fn walk(src: &Path, dst: &Path) {
        let _ = std::fs::create_dir_all(dst);
        let Ok(rd) = std::fs::read_dir(src) else {
            return;
        };
        for e in rd.flatten() {
            let name = e.file_name();
            if matches!(
                name.to_str(),
                Some(".git" | ".modbit" | "__pycache__" | ".pytest_cache")
            ) {
                continue;
            }
            let p = e.path();
            if p.is_dir() {
                walk(&p, &dst.join(&name));
            } else {
                let _ = std::fs::copy(&p, dst.join(&name));
            }
        }
    }
    walk(workspace, copy.path());
    if std::fs::write(copy.path().join("check.py"), &original).is_err() {
        return (false, untouched);
    }
    let out = run_process(
        Command::new(CHECK_ARGV[0])
            .args(&CHECK_ARGV[1..])
            .env("PYTHONDONTWRITEBYTECODE", "1")
            .current_dir(copy.path()),
        Duration::from_secs(120),
    );
    (out.code == 0 && !out.timed_out, untouched)
}

#[derive(Default)]
struct EventFacts {
    last_gate: Option<String>,
    escalations: u32,
    review_legs: u32,
    review_results: Vec<String>,
    revisions: u32,
    invocations: u32,
    usages: u32,
    input_tokens: u64,
    output_tokens: u64,
    cost_usd: f64,
    unpriced: u32,
    answered_by: BTreeMap<String, u32>,
    run_id: String,
    compiled_plan: Option<Value>,
    max_epoch: u64,
    needed_person: bool,
}

fn catalog_price(
    catalogs: &BTreeMap<String, String>,
    endpoint: &str,
    model: &str,
) -> Option<Price> {
    catalogs
        .get(endpoint)
        .and_then(|spec| price_from_catalog(spec, model).ok())
}

fn read_facts(events: &str, catalogs: &BTreeMap<String, String>) -> EventFacts {
    let mut f = EventFacts::default();
    for line in events.lines() {
        let Ok(v) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        let p = &v["payload"];
        match v["event_type"].as_str().unwrap_or_default() {
            "AcceptanceGateEvaluated" => {
                f.last_gate = p["verdict"].as_str().map(str::to_owned);
            }
            "ContinuationActivated" => f.escalations += 1,
            "ReviewLegActivated" => f.review_legs += 1,
            "ReviewerResultRecorded" => {
                f.review_results
                    .push(p["verdict"].as_str().unwrap_or("?").to_owned());
            }
            "RevisionActivated" => f.revisions += 1,
            "ModelInvocationStarted" => f.invocations += 1,
            "ModelUsageRecorded" => {
                f.usages += 1;
                let (i, o) = (
                    p["input_tokens"].as_u64().unwrap_or(0),
                    p["output_tokens"].as_u64().unwrap_or(0),
                );
                f.input_tokens += i;
                f.output_tokens += o;
                let route = &p["route"];
                let endpoint = route["endpoint"].as_str().unwrap_or_default();
                let model = route["requested_model"]
                    .as_str()
                    .or_else(|| route["model"].as_str())
                    .unwrap_or_default();
                *f.answered_by
                    .entry(format!("{endpoint}/{model}"))
                    .or_insert(0) += 1;
                match catalog_price(catalogs, endpoint, model) {
                    Some(price) => f.cost_usd += price.cost_usd(i, o),
                    None => f.unpriced += 1,
                }
            }
            "RoutingPlanCompiled" => {
                let epoch = p["plan"]["routing_epoch"].as_u64().unwrap_or(0);
                if f.compiled_plan.is_none() || epoch >= f.max_epoch {
                    f.max_epoch = epoch;
                    f.compiled_plan = Some(p["plan"].clone());
                }
                if f.run_id.is_empty() {
                    f.run_id = run_id_text(&p["plan"]["run_id"]);
                }
            }
            "TaskNeedsAttention" => f.needed_person = true,
            _ => {}
        }
    }
    f
}

fn run_id_text(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Array(a) => a
            .iter()
            .filter_map(|b| b.as_u64())
            .map(|b| format!("{b:02x}"))
            .collect(),
        other => other.to_string(),
    }
}

/// The operator's cascade plan: the plan in force with the second binding as a
/// prevalidated stronger-solver slot behind a quality rejection, one epoch on.
///
/// # Errors
/// The compiled plan cannot be read as a plan.
pub fn cascade_plan(compiled: &Value, second: &Binding, epoch: u64) -> Result<String, String> {
    use modbit_domain::routing::{ConditionalExecutionPlan, Slot, Trigger};
    let mut plan: ConditionalExecutionPlan = serde_json::from_value(compiled.clone())
        .map_err(|e| format!("the compiled plan is not a plan: {e}"))?;
    let initial = plan
        .slots
        .iter()
        .find(|s| s.trigger == Trigger::Initial)
        .cloned()
        .ok_or("the compiled plan has no initial slot")?;
    let stronger = Slot {
        slot_id: "stronger".into(),
        predecessor: Some(initial.slot_id.clone()),
        trigger: Trigger::QualityRejected,
        max_activations: 1,
        endpoint: second.endpoint.clone(),
        model: second.model.clone(),
        role: "solver".into(),
        budget: initial.budget.clone(),
    };
    plan.plan_id = format!("cascade-{epoch}");
    plan.routing_epoch = epoch;
    plan.provenance.compiler_version = "operator".into();
    plan.slots = vec![initial, stronger];
    plan.max_total_attempts = 4;
    plan.max_revisions = 1;
    let sealed = plan.sealed();
    serde_json::to_string(&sealed).map_err(|e| e.to_string())
}

/// The check every task's repository declares, as the verification run names
/// it.
pub const REQUIRED_CHECK: &str = "configured_command:python3 -B check.py";

/// The name of the organization assurance layer an arm runs under.
#[must_use]
pub fn policy_file(arm: Arm) -> &'static str {
    match arm {
        Arm::Critique => "policy-critique.json",
        _ => "policy-required-check.json",
    }
}

/// The organization assurance layers (`MODBIT_POLICY_FILE`) of the arms. Every
/// arm requires the task's check to pass at completion: without it the gate
/// excuses a check that was already failing before the candidate (the bug the
/// task asks to fix), accepts an unfixed candidate, and no continuation could
/// ever activate. CRITIQUE additionally requires an independent review of
/// every candidate.
#[must_use]
pub fn policy_layers() -> [(Arm, String); 2] {
    [
        (
            Arm::Direct,
            serde_json::json!({"required_checks": [REQUIRED_CHECK]}).to_string(),
        ),
        (
            Arm::Critique,
            serde_json::json!({"required_checks": [REQUIRED_CHECK], "review_required_at": "LOW"})
                .to_string(),
        ),
    ]
}

struct Trial<'a> {
    shared: &'a Shared,
    task: &'a TrialTask,
    arm: Arm,
    data_dir: PathBuf,
    workspace: PathBuf,
    log: std::fs::File,
    session: String,
    task_hex: String,
    interactions: u32,
    needed_person: bool,
}

impl Trial<'_> {
    fn env(&self) -> Vec<(String, String)> {
        let mut env = self.shared.cfg.env.clone();
        env.push((
            "MODBIT_CORE_BIN".into(),
            self.shared.cfg.core.to_string_lossy().into_owned(),
        ));
        env.push(("MODBIT_CORE_IDLE_EXIT_SECS".into(), "900".into()));
        env.push((
            "MODBIT_REGISTRY_KEYS".into(),
            self.shared.registry.trusted_key.clone(),
        ));
        env.push((
            "MODBIT_POLICY_FILE".into(),
            self.shared
                .files
                .join(policy_file(self.arm))
                .to_string_lossy()
                .into_owned(),
        ));
        env
    }

    fn cli(&mut self, args: &[&str], timeout: Duration) -> Out {
        let mut c = Command::new(&self.shared.cfg.cli);
        c.arg("--data-dir").arg(&self.data_dir).args(args);
        for (k, v) in self.env() {
            c.env(k, v);
        }
        let r = run_process(&mut c, timeout);
        let _ = writeln!(
            self.log,
            "$ modbit-cli {}\n[exit {}{}]\n{}{}",
            args.join(" "),
            r.code,
            if r.timed_out { " TIMEOUT" } else { "" },
            r.stdout,
            if r.stderr.is_empty() {
                String::new()
            } else {
                format!("[stderr]\n{}", r.stderr)
            }
        );
        r
    }

    fn events(&mut self) -> String {
        self.cli(
            &[
                "events",
                "tail",
                "--session",
                &self.session.clone(),
                "--json",
                "--count",
                "200000",
            ],
            Duration::from_secs(120),
        )
        .stdout
    }

    /// Run `args` (a `task run ... --wait`), answering the typed questions and
    /// denying the approvals a waiting task asks for, until the task is idle
    /// without a pending question or approval or the deadline passes.
    fn run_leg(&mut self, run_args: &[String], deadline: Instant) -> (Out, bool) {
        let short = Duration::from_secs(120);
        let refs = |v: &[String]| -> Vec<String> { v.to_vec() };
        let args: Vec<String> = refs(run_args);
        let mut r = self.cli(
            &args.iter().map(String::as_str).collect::<Vec<_>>(),
            deadline.saturating_duration_since(Instant::now()),
        );
        let mut rounds = 0;
        while !r.timed_out && r.code == 2 && rounds < 4 {
            rounds += 1;
            self.needed_person = true;
            let task = self.task_hex.clone();
            let session = self.session.clone();
            let q = self.cli(&["question", "list", "--task", &task], short);
            let pending_q: Vec<String> = q
                .stdout
                .lines()
                .filter(|l| l.starts_with("question ") && l.contains("answered=false"))
                .filter_map(|l| l.split_whitespace().nth(1).map(str::to_owned))
                .collect();
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                r.timed_out = true;
                break;
            }
            if let Some(qid) = pending_q.first() {
                self.interactions += 1;
                let answer = self.shared.cfg.question_answer.clone();
                r = self.cli(
                    &[
                        "question",
                        "answer",
                        "--session",
                        &session,
                        "--task",
                        &task,
                        "--question",
                        qid,
                        "--endpoint",
                        &self.shared.cfg.opener.endpoint.clone(),
                        "--model",
                        &self.shared.cfg.opener.model.clone(),
                        "--wait",
                        &answer,
                    ],
                    remaining,
                );
                continue;
            }
            let a = self.cli(&["approval", "list", "--session", &session], short);
            let pending_a: Vec<String> = a
                .stdout
                .lines()
                .filter(|l| l.starts_with("approval ") && l.contains("status=PENDING"))
                .filter_map(|l| l.split_whitespace().nth(1).map(str::to_owned))
                .collect();
            if pending_a.is_empty() {
                break;
            }
            for aid in &pending_a {
                self.interactions += 1;
                let _ = self.cli(
                    &[
                        "approval",
                        "resolve",
                        "--session",
                        &session,
                        "--approval",
                        aid,
                        "deny",
                        "benchmark protocol: protected effects are denied",
                    ],
                    short,
                );
            }
            r = self.cli(
                &args.iter().map(String::as_str).collect::<Vec<_>>(),
                remaining,
            );
        }
        let timed_out = r.timed_out;
        (r, timed_out)
    }
}

fn poll_status(
    rt: &tokio::runtime::Runtime,
    link: &mut CoreLink,
    task: &modbit_protocol::v1::Id,
) -> Result<modbit_protocol::v1::TaskStatus, String> {
    rt.block_on(link.task_status(task))
}

fn stop_profile_core(data_dir: &Path) {
    #[cfg(unix)]
    {
        let needle = format!("--data-dir {}", data_dir.display());
        if let Ok(o) = Command::new("pgrep").args(["-f", "--", &needle]).output() {
            for pid in String::from_utf8_lossy(&o.stdout).split_whitespace() {
                let _ = Command::new("kill").args(["-TERM", pid]).status();
            }
        }
    }
    #[cfg(not(unix))]
    let _ = data_dir;
}

/// Run one task under one arm.
#[must_use]
#[allow(clippy::too_many_lines)]
pub fn run_trial(shared: &Shared, task: &TrialTask, arm: Arm, repeat: u32) -> PairedRun {
    let mut rec = PairedRun::blank(&task.id, arm, repeat);
    let started = Instant::now();
    let deadline = started + shared.cfg.trial_timeout;
    let slug = format!("{}-{}-{repeat}", task.id.replace('/', "_"), arm.label());
    let scratch = shared
        .cfg
        .work
        .join(format!("{}-{slug}", std::process::id()));
    let _ = std::fs::remove_dir_all(&scratch);
    let workspace = scratch.join("workspace");
    let data_dir = scratch.join("profile");
    let logs = shared.out.join("trials").join(&slug);
    let setup = (|| -> Result<std::fs::File, String> {
        std::fs::create_dir_all(&workspace).map_err(|e| e.to_string())?;
        std::fs::create_dir_all(&data_dir).map_err(|e| e.to_string())?;
        std::fs::create_dir_all(&logs).map_err(|e| e.to_string())?;
        materialize(task, &workspace).map_err(|e| e.to_string())?;
        std::fs::File::create(logs.join("cli.log")).map_err(|e| e.to_string())
    })();
    let log = match setup {
        Ok(l) => l,
        Err(e) => {
            rec.error = Some(format!("setup: {e}"));
            return rec;
        }
    };
    let mut t = Trial {
        shared,
        task,
        arm,
        data_dir: data_dir.clone(),
        workspace: workspace.clone(),
        log,
        session: String::new(),
        task_hex: String::new(),
        interactions: 0,
        needed_person: false,
    };
    let result = trial_body(&mut t, &mut rec, deadline, started, &logs);
    if let Err(e) = result {
        rec.error = Some(e);
    }
    rec.wall_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
    rec.interactions = t.interactions;
    rec.needed_person |= t.needed_person;
    stop_profile_core(&data_dir);
    let _ = std::fs::remove_dir_all(&scratch);
    rec
}

fn catalogs_of(cfg: &PairedConfig) -> BTreeMap<String, String> {
    let mut m = BTreeMap::new();
    for b in [&cfg.opener, &cfg.second] {
        // The catalog spec the Core reads, rebuilt from the binding's price so
        // the harness and the Core price a call from the same figures.
        let entry = format!(
            "{}={}/{}",
            b.model, b.price.input_per_mtok_usd, b.price.output_per_mtok_usd
        );
        m.entry(b.endpoint.clone())
            .and_modify(|s: &mut String| {
                s.push(',');
                s.push_str(&entry);
            })
            .or_insert(entry);
    }
    m
}

#[allow(clippy::too_many_lines)]
fn trial_body(
    t: &mut Trial<'_>,
    rec: &mut PairedRun,
    deadline: Instant,
    started: Instant,
    logs: &Path,
) -> Result<(), String> {
    let cfg = &t.shared.cfg;
    let short = Duration::from_secs(120);
    let r = t.cli(&["session", "create"], short);
    t.session = first_word_after(&r.stdout, "session ")
        .ok_or_else(|| format!("session create: {}{}", r.stdout, r.stderr))?;
    rec.session_id = t.session.clone();
    // Every arm runs under the same signed registry.
    let registry_file = t.shared.files.join("registry.signed.json");
    let r = t.cli(
        &["registry", "activate", &registry_file.to_string_lossy()],
        short,
    );
    if !r.stdout.contains("active") && r.code != 0 {
        return Err(format!("registry activate: {}{}", r.stdout, r.stderr));
    }
    let root = t
        .workspace
        .canonicalize()
        .map_err(|e| e.to_string())?
        .to_string_lossy()
        .trim_start_matches(r"\\?\")
        .to_owned();
    let session = t.session.clone();
    let r = t.cli(
        &[
            "task",
            "create",
            "--session",
            &session,
            "--workspace",
            &root,
            &t.task.goal.clone(),
        ],
        short,
    );
    t.task_hex = first_word_after(&r.stdout, "task ")
        .ok_or_else(|| format!("task create: {}{}", r.stdout, r.stderr))?;
    rec.task_id = t.task_hex.clone();
    let task_hex = t.task_hex.clone();
    let turns = cfg.max_turns.to_string();
    let first_leg: Vec<String> = [
        "task",
        "run",
        "--session",
        &session,
        "--task",
        &task_hex,
        "--endpoint",
        &cfg.opener.endpoint,
        "--model",
        &cfg.opener.model,
        "--max-turns",
        &turns,
        "--wait",
    ]
    .iter()
    .map(|s| (*s).to_owned())
    .collect();
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| e.to_string())?;
    let (mut r, mut timed_out) = t.run_leg(&first_leg, deadline);
    let catalogs = catalogs_of(cfg);
    let mut task_state = kv(&r.stdout, "state").unwrap_or_default();
    // The first leg's gate verdict and the independent check on its tree.
    let mut events = t.events();
    let facts = read_facts(&events, &catalogs);
    let (first_oracle, _) = independent_check(t.task, &t.workspace);
    let first_verdict = facts.last_gate.clone().unwrap_or_else(|| "NONE".into());
    // CASCADE: a first leg that ended without an accepted candidate is given
    // the stronger-solver continuation the Core's quality boundary activates.
    if t.arm == Arm::Cascade
        && !timed_out
        && r.code == 2
        && let Some(plan) = facts.compiled_plan.clone()
    {
        rec.escalation_attempted = true;
        rec.gates.push(GateObservation {
            leg: "first".into(),
            verdict: first_verdict.clone(),
            oracle_pass: first_oracle,
        });
        let plan_json = cascade_plan(&plan, &cfg.second, facts.max_epoch + 1)?;
        let mut link = rt.block_on(CoreLink::attach(&t.data_dir))?;
        let session_id = id_of(&t.session)?;
        let task_id = id_of(&t.task_hex)?;
        let lease = rt.block_on(link.lease(&session_id))?;
        let admission = rt.block_on(link.admit_plan(&task_id, lease, &plan_json))?;
        if !admission.admitted {
            return Err(format!(
                "the cascade plan was refused: {} {}",
                admission.refusal_code, admission.refusal_detail
            ));
        }
        let resume: Vec<String> = [
            "task",
            "run",
            "--session",
            &session,
            "--task",
            &task_hex,
            "--max-turns",
            &turns,
            "--wait",
        ]
        .iter()
        .map(|s| (*s).to_owned())
        .collect();
        let (r2, to2) = t.run_leg(&resume, deadline);
        r = r2;
        timed_out = to2;
        task_state = kv(&r.stdout, "state").unwrap_or(task_state);
    }
    // CRITIQUE (and any arm a review reached): settle, so a review in flight
    // or a revision it caused has finished before anything is read.
    let task_id = id_of(&t.task_hex)?;
    let mut link = rt.block_on(CoreLink::attach(&t.data_dir))?;
    let mut quiet = 0;
    while !timed_out && quiet < 2 {
        std::thread::sleep(Duration::from_secs(2));
        let st = poll_status(&rt, &mut link, &task_id)?;
        events = t.events();
        let f = read_facts(&events, &catalogs);
        let review_in_flight = f.review_legs > u32::try_from(f.review_results.len()).unwrap_or(0);
        if st.loop_alive || review_in_flight {
            quiet = 0;
        } else {
            quiet += 1;
        }
        if Instant::now() >= deadline {
            timed_out = true;
        }
    }
    if timed_out {
        let _ = t.cli(
            &["task", "cancel", "--session", &session, "--task", &task_hex],
            short,
        );
        rec.timed_out = true;
    }
    let st = poll_status(&rt, &mut link, &task_id)?;
    rec.state = if timed_out {
        "TIMEOUT".into()
    } else {
        st.state.clone()
    };
    let _ = task_state;
    // Everything the log says.
    events = t.events();
    std::fs::write(logs.join("events.jsonl"), &events).map_err(|e| e.to_string())?;
    rec.events_sha256 = hex::encode(Sha256::digest(events.as_bytes()));
    rec.events_file = format!(
        "trials/{}/events.jsonl",
        logs.file_name()
            .map_or_else(String::new, |n| n.to_string_lossy().into_owned())
    );
    let facts = read_facts(&events, &catalogs);
    rec.run_id = facts.run_id.clone();
    rec.escalated = facts.escalations > 0;
    rec.reviewed = facts.review_legs > 0;
    rec.review_verdicts = facts.review_results.clone();
    rec.revisions = facts.revisions;
    rec.model_calls = facts.invocations;
    rec.input_tokens = facts.input_tokens;
    rec.output_tokens = facts.output_tokens;
    rec.answered_by = facts.answered_by.clone();
    rec.calls_without_usage = facts.invocations.saturating_sub(facts.usages);
    rec.unpriced_usage = facts.unpriced;
    rec.cost_usd = (facts.unpriced == 0).then_some(facts.cost_usd);
    rec.needed_person |= facts.needed_person || st.state == "Waiting";
    // The final tree against the independent check; the gate's last word.
    let (oracle, untouched) = independent_check(t.task, &t.workspace);
    rec.oracle_pass = oracle;
    rec.check_untouched = untouched;
    let final_verdict = facts.last_gate.clone().unwrap_or_else(|| "NONE".into());
    rec.gates.push(GateObservation {
        leg: "final".into(),
        verdict: final_verdict,
        oracle_pass: oracle,
    });
    // The Core's accounting of the request, every leg.
    let (view, record) = rt.block_on(link.request_outcome(&task_id))?;
    rec.core_verified = view.verified_success;
    if view.found {
        rec.accounting = Some(Accounting {
            total_minor: view.total_minor,
            unknown_minor: view.unknown_minor,
            currency: view.currency.clone(),
            scale: view.scale,
            path_label: view.path_label.clone(),
            final_outcome: view.final_outcome.clone(),
        });
    }
    let _ = record;
    rec.verified_success = rec.core_verified && rec.oracle_pass && rec.check_untouched;
    // The outcome statistics this run's legs produced.
    let lease = rt.block_on(link.lease(&id_of(&t.session)?))?;
    let version = format!(
        "paired-{slug}",
        slug = rec.run_id.get(..12).unwrap_or("run")
    );
    let stats = rt.block_on(link.materialize_statistics(&id_of(&t.session)?, lease, &version))?;
    if stats.refusal_code == "STATS_NO_SOURCE" {
        // No leg of this run is an outcome sample (a direct run emits none:
        // solver samples come from a published baseline, not from a run), so
        // there is no snapshot, and the router would have read `none`.
        rec.stats_version = "none".into();
    } else if stats.materialized {
        rec.stats_version = stats.stats_version.clone();
        rec.stats_digest = stats.snapshot_digest.clone();
        rec.stats = stats
            .aggregates
            .iter()
            .map(|a| StatSample {
                key: a.key_id.clone(),
                samples: a.samples,
                successes: a.successes,
                cost_known: a.cost_known,
                unknown_cost_samples: a.unknown_cost_samples,
                mean_cost_minor: a.mean_cost_minor,
            })
            .collect();
    } else {
        rec.error = Some(format!(
            "statistics were not materialized: {} {}",
            stats.refusal_code, stats.refusal_detail
        ));
    }
    let _ = started;
    Ok(())
}

// ---- The matrix ----

/// One planned trial.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Planned {
    /// Index into the task list.
    pub task: usize,
    /// The arm.
    pub arm: Arm,
    /// The repeat.
    pub repeat: u32,
}

/// The order trials are started in: task by task, every arm of a task before
/// the next task, so a run stopped by the cap leaves balanced pairs.
#[must_use]
pub fn plan(tasks: usize, arms: &[Arm], repeats: u32) -> Vec<Planned> {
    let mut v = Vec::new();
    for task in 0..tasks {
        for repeat in 0..repeats {
            for arm in arms {
                v.push(Planned {
                    task,
                    arm: *arm,
                    repeat,
                });
            }
        }
    }
    v
}

/// Run the plan with `cfg.parallel` workers. A trial is started only when the
/// meter admits a reserve of `parallel` times the dearest trial so far.
#[must_use]
pub fn run_matrix(
    shared: &Arc<Shared>,
    tasks: &[TrialTask],
    planned: Vec<Planned>,
) -> Vec<PairedRun> {
    let queue = Arc::new(Mutex::new(VecDeque::from(planned)));
    let results = Arc::new(Mutex::new(Vec::new()));
    let tasks: Arc<Vec<TrialTask>> = Arc::new(tasks.to_vec());
    let workers = shared.cfg.parallel.max(1);
    let mut handles = Vec::new();
    for _ in 0..workers {
        let (queue, results, shared, tasks) = (
            Arc::clone(&queue),
            Arc::clone(&results),
            Arc::clone(shared),
            Arc::clone(&tasks),
        );
        handles.push(std::thread::spawn(move || {
            loop {
                let reserve = {
                    let seen = *shared
                        .max_trial_usd
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner);
                    seen.max(0.10) * workers as f64
                };
                if shared.meter.admit(reserve).is_err() {
                    return;
                }
                let Some(p) = queue
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .pop_front()
                else {
                    return;
                };
                let rec = run_trial(&shared, &tasks[p.task], p.arm, p.repeat);
                // A trial's cost is charged from what it measured; one that
                // could not be priced is charged the dearest seen so far.
                let seen = *shared
                    .max_trial_usd
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                let cost = rec.cost_usd.unwrap_or(seen.max(0.10));
                shared
                    .meter
                    .charge_usd(cost, rec.input_tokens, rec.output_tokens);
                {
                    let mut m = shared
                        .max_trial_usd
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner);
                    *m = m.max(cost);
                }
                results
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .push(rec);
            }
        }));
    }
    for h in handles {
        let _ = h.join();
    }
    let mut out = std::mem::take(
        &mut *results
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner),
    );
    out.sort_by(|a, b| (&a.task, a.repeat, a.arm).cmp(&(&b.task, b.repeat, b.arm)));
    out
}

// ---- The report ----

/// One arm over all its runs.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ArmSummary {
    /// The arm.
    pub arm: Arm,
    /// Runs made (failed ones included).
    pub runs: usize,
    /// Verified successes, with the Wilson interval.
    pub verified_success: Proportion,
    /// The Core's own gating said verified.
    pub core_verified: Proportion,
    /// Total cost per run, USD, every leg.
    pub cost_usd: MeanCi,
    /// The Core's own accounting total per run, USD, with a reservation held
    /// for every model invocation whose usage was never reported (an upper
    /// bound where the gateway's usage is a lower bound).
    pub accounting_cost_usd: MeanCi,
    /// Runs with a model invocation that returned no usage.
    pub runs_with_unreported_usage: usize,
    /// Total cost divided by verified successes (`None` with no success).
    pub cost_per_verified_success_usd: Option<f64>,
    /// Wall time per run, seconds.
    pub latency_s: MeanCi,
    /// Median wall time, seconds.
    pub median_latency_s: f64,
    /// Share of runs that activated a stronger-solver slot.
    pub escalation_rate: Proportion,
    /// Share of runs the harness tried to continue (the cascade's denominator
    /// of runs that needed a second leg).
    pub escalation_attempt_rate: Proportion,
    /// Share of runs that activated a reviewer.
    pub review_rate: Proportion,
    /// Share of runs that needed a person (the human-intervention proxy).
    pub human_intervention_rate: Proportion,
    /// Model invocations per run.
    pub model_calls: MeanCi,
    /// Input plus output tokens per run.
    pub tokens: MeanCi,
    /// Runs with a harness or Core failure (no valid measurement).
    pub errored: usize,
    /// Runs that timed out.
    pub timed_out: usize,
    /// Runs whose cost could not be fully priced.
    pub cost_unpriced: usize,
    /// Path labels the Core's accounting gave the runs.
    pub path_labels: BTreeMap<String, u32>,
    /// Ids of runs that did not verify, `task#repeat`.
    pub failed_runs: Vec<String>,
}

/// The gate's errors against the independent check.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct GateErrors {
    /// Observations (gate verdict paired with the independent check).
    pub observations: usize,
    /// Candidates the independent check passes.
    pub good_candidates: usize,
    /// Candidates the independent check fails.
    pub bad_candidates: usize,
    /// Gate accepted a candidate the independent check fails.
    pub false_accept: Proportion,
    /// Gate rejected a candidate the independent check passes.
    pub false_reject: Proportion,
    /// Verdicts that were neither (INCONCLUSIVE or NONE).
    pub no_verdict: usize,
}

/// A paired comparison against DIRECT.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Comparison {
    /// The arm compared.
    pub arm: Arm,
    /// Complete pairs (a run of the arm with a DIRECT run of the same task and
    /// repeat, both without a harness error).
    pub pairs: usize,
    /// Verified-success difference (arm minus direct), per pair.
    pub success_delta: MeanCi,
    /// Total cost difference, USD.
    pub cost_delta_usd: MeanCi,
    /// Wall time difference, seconds.
    pub latency_delta_s: MeanCi,
    /// `BETTER` | `WORSE` | `NO_DIFFERENCE_DETECTED` for verified success.
    pub quality: String,
    /// `MORE` | `LESS` | `INDETERMINATE` for total cost.
    pub cost: String,
}

/// The statistics a family of legs produced across the runs.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SampleFamily {
    /// The key prefix (`solver`, `escalation`, `reviewer`).
    pub kind: String,
    /// Runs that emitted at least one.
    pub runs: usize,
    /// Observations.
    pub samples: u32,
    /// Verified successes among them.
    pub successes: u32,
    /// Observations whose cost was not reported.
    pub unknown_cost_samples: u32,
    /// Sum of mean cost times samples, minor units (known cost only).
    pub total_cost_minor: u64,
    /// The keys seen.
    pub keys: Vec<String>,
}

/// The rule the comparisons follow, stated before any run.
pub const VERDICT_RULE: &str = "For each of CASCADE and CRITIQUE against DIRECT, over the tasks both arms ran: QUALITY is BETTER when the 95% bootstrap interval of the paired verified-success difference lies entirely above zero, WORSE when it lies entirely below zero, NO_DIFFERENCE_DETECTED otherwise; COST is MORE when the interval of the paired total-cost difference (every leg priced from gateway usage) lies entirely above zero, LESS when entirely below, INDETERMINATE otherwise. The harness decides nothing about the default route: it stays DIRECT, and changing it is an EPR Decision Record after this evidence.";

/// The whole report.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PairedReport {
    /// Schema.
    pub schema: String,
    /// Whether the provider was live.
    pub live: bool,
    /// The pinned task set.
    pub task_set: TaskSet,
    /// The opening binding.
    pub opener: Binding,
    /// The second binding (stronger-solver and reviewer slots).
    pub second: Binding,
    /// Registry generation, signer and digest.
    pub registry_generation: String,
    /// SHA-256 of the signed registry document.
    pub registry_digest: String,
    /// Repeats per task and arm.
    pub repeats: u32,
    /// Per arm.
    pub arms: Vec<ArmSummary>,
    /// CASCADE and CRITIQUE against DIRECT.
    pub comparisons: Vec<Comparison>,
    /// The gate's errors per arm and overall.
    pub gate: BTreeMap<String, GateErrors>,
    /// The outcome statistics each family of legs produced (REQ-PX-133).
    pub samples: Vec<SampleFamily>,
    /// Every run id, with the statistics snapshot it fed.
    pub run_ids: Vec<(String, String, String)>,
    /// The rule.
    pub rule: String,
    /// The verdict, in words, whichever way it points.
    pub verdict: String,
    /// What the numbers do not establish.
    pub method: String,
    /// sha256 over the canonical content above.
    pub digest: String,
}

fn proportion(runs: &[&PairedRun], f: impl Fn(&PairedRun) -> bool) -> Proportion {
    wilson(runs.iter().filter(|r| f(r)).count(), runs.len())
}

fn median(mut v: Vec<f64>) -> f64 {
    if v.is_empty() {
        return f64::NAN;
    }
    v.sort_by(f64::total_cmp);
    let m = v.len() / 2;
    if v.len() % 2 == 1 {
        v[m]
    } else {
        f64::midpoint(v[m - 1], v[m])
    }
}

fn gate_errors(runs: &[&PairedRun]) -> GateErrors {
    let obs: Vec<&GateObservation> = runs.iter().flat_map(|r| r.gates.iter()).collect();
    let good = obs.iter().filter(|o| o.oracle_pass).count();
    let bad = obs.len() - good;
    let fa = obs
        .iter()
        .filter(|o| !o.oracle_pass && o.verdict == "ACCEPT")
        .count();
    let fr = obs
        .iter()
        .filter(|o| o.oracle_pass && o.verdict == "REJECT")
        .count();
    GateErrors {
        observations: obs.len(),
        good_candidates: good,
        bad_candidates: bad,
        false_accept: wilson(fa, bad),
        false_reject: wilson(fr, good),
        no_verdict: obs
            .iter()
            .filter(|o| o.verdict != "ACCEPT" && o.verdict != "REJECT")
            .count(),
    }
}

fn paired(runs: &[PairedRun], arm: Arm, f: impl Fn(&PairedRun) -> Option<f64>) -> Vec<f64> {
    let mut out = Vec::new();
    for a in runs.iter().filter(|r| r.arm == arm && r.error.is_none()) {
        if let Some(d) = runs.iter().find(|r| {
            r.arm == Arm::Direct && r.task == a.task && r.repeat == a.repeat && r.error.is_none()
        }) && let (Some(x), Some(y)) = (f(a), f(d))
        {
            out.push(x - y);
        }
    }
    out
}

/// Summarise the runs. Every arm that ran appears; every run is counted, the
/// failed and errored ones named.
#[must_use]
pub fn report(
    runs: &[PairedRun],
    task_set: &TaskSet,
    cfg: &PairedConfig,
    registry: &RegistryBundle,
) -> PairedReport {
    let arms_run: Vec<Arm> = Arm::ALL
        .into_iter()
        .filter(|a| runs.iter().any(|r| r.arm == *a))
        .collect();
    let arms: Vec<ArmSummary> = arms_run
        .iter()
        .map(|arm| {
            let mine: Vec<&PairedRun> = runs.iter().filter(|r| r.arm == *arm).collect();
            let costs: Vec<f64> = mine.iter().filter_map(|r| r.cost_usd).collect();
            let successes = mine.iter().filter(|r| r.verified_success).count();
            let lat: Vec<f64> = mine.iter().map(|r| r.wall_ms as f64 / 1000.0).collect();
            let mut labels: BTreeMap<String, u32> = BTreeMap::new();
            for r in &mine {
                if let Some(a) = &r.accounting {
                    *labels.entry(a.path_label.clone()).or_insert(0) += 1;
                }
            }
            ArmSummary {
                arm: *arm,
                runs: mine.len(),
                verified_success: proportion(&mine, |r| r.verified_success),
                core_verified: proportion(&mine, |r| r.core_verified),
                cost_usd: mean_ci(&costs),
                accounting_cost_usd: mean_ci(
                    &mine
                        .iter()
                        .filter_map(|r| r.accounting.as_ref())
                        .map(|a| {
                            a.total_minor as f64 / 10f64.powi(i32::try_from(a.scale).unwrap_or(6))
                        })
                        .collect::<Vec<_>>(),
                ),
                runs_with_unreported_usage: mine
                    .iter()
                    .filter(|r| r.calls_without_usage > 0)
                    .count(),
                cost_per_verified_success_usd: (successes > 0)
                    .then(|| costs.iter().sum::<f64>() / successes as f64),
                latency_s: mean_ci(&lat),
                median_latency_s: median(lat),
                escalation_rate: proportion(&mine, |r| r.escalated),
                escalation_attempt_rate: proportion(&mine, |r| r.escalation_attempted),
                review_rate: proportion(&mine, |r| r.reviewed),
                human_intervention_rate: proportion(&mine, |r| r.needed_person),
                model_calls: mean_ci(
                    &mine
                        .iter()
                        .map(|r| f64::from(r.model_calls))
                        .collect::<Vec<_>>(),
                ),
                tokens: mean_ci(
                    &mine
                        .iter()
                        .map(|r| (r.input_tokens + r.output_tokens) as f64)
                        .collect::<Vec<_>>(),
                ),
                errored: mine.iter().filter(|r| r.error.is_some()).count(),
                timed_out: mine.iter().filter(|r| r.timed_out).count(),
                cost_unpriced: mine.iter().filter(|r| r.cost_usd.is_none()).count(),
                path_labels: labels,
                failed_runs: mine
                    .iter()
                    .filter(|r| !r.verified_success)
                    .map(|r| format!("{}#{}", r.task, r.repeat))
                    .collect(),
            }
        })
        .collect();
    let comparisons: Vec<Comparison> = [Arm::Cascade, Arm::Critique]
        .into_iter()
        .filter(|a| arms_run.contains(a) && arms_run.contains(&Arm::Direct))
        .map(|arm| {
            let s = paired(runs, arm, |r| Some(f64::from(u8::from(r.verified_success))));
            let c = paired(runs, arm, |r| r.cost_usd);
            let l = paired(runs, arm, |r| Some(r.wall_ms as f64 / 1000.0));
            let (sc, cc) = (mean_ci(&s), mean_ci(&c));
            let quality = if !sc.ci95.0.is_finite() {
                "NO_DIFFERENCE_DETECTED"
            } else if sc.ci95.0 > 0.0 {
                "BETTER"
            } else if sc.ci95.1 < 0.0 {
                "WORSE"
            } else {
                "NO_DIFFERENCE_DETECTED"
            };
            let cost = if !cc.ci95.0.is_finite() {
                "INDETERMINATE"
            } else if cc.ci95.0 > 0.0 {
                "MORE"
            } else if cc.ci95.1 < 0.0 {
                "LESS"
            } else {
                "INDETERMINATE"
            };
            Comparison {
                arm,
                pairs: s.len(),
                success_delta: sc,
                cost_delta_usd: cc,
                latency_delta_s: mean_ci(&l),
                quality: quality.into(),
                cost: cost.into(),
            }
        })
        .collect();
    let mut gate: BTreeMap<String, GateErrors> = BTreeMap::new();
    for arm in &arms_run {
        let mine: Vec<&PairedRun> = runs.iter().filter(|r| r.arm == *arm).collect();
        gate.insert(arm.label().into(), gate_errors(&mine));
    }
    gate.insert("all".into(), gate_errors(&runs.iter().collect::<Vec<_>>()));
    let mut families: Vec<SampleFamily> = Vec::new();
    for kind in ["solver", "escalation", "reviewer"] {
        let mut fam = SampleFamily {
            kind: kind.into(),
            runs: 0,
            samples: 0,
            successes: 0,
            unknown_cost_samples: 0,
            total_cost_minor: 0,
            keys: Vec::new(),
        };
        for r in runs {
            let mut any = false;
            for s in r
                .stats
                .iter()
                .filter(|s| s.key.starts_with(&format!("{kind}|")))
            {
                any = true;
                fam.samples += s.samples;
                fam.successes += s.successes;
                fam.unknown_cost_samples += s.unknown_cost_samples;
                if s.cost_known {
                    fam.total_cost_minor += s.mean_cost_minor * u64::from(s.samples);
                }
                if !fam.keys.contains(&s.key) {
                    fam.keys.push(s.key.clone());
                }
            }
            fam.runs += usize::from(any);
        }
        fam.keys.sort();
        families.push(fam);
    }
    let verdict = verdict_text(&arms, &comparisons, cfg.live);
    let method = format!(
        "{} tasks x {} repeats x {} arms, each trial a fresh repository, a fresh profile and a fresh Core driven through modbit-cli under the same signed two-binding registry ({} opens, {} is the stronger-solver and reviewer binding). {} Verified success needs the Core's own completion gating, the task's pristine check.py passing on the final tree and check.py untouched. Cost is priced from the usage the gateway returned for every model invocation of the session (the review task's included) at the catalog list prices, no cache discount. Gate errors compare the gate's verdict at the end of a leg with the independent check of the same tree. Wilson intervals for proportions, deterministic bootstrap for means and paired differences. {}",
        task_set.tasks.len(),
        cfg.repeats,
        arms_run.len(),
        cfg.opener.label(),
        cfg.second.label(),
        if cfg.opener.model == cfg.second.model {
            "BOTH BINDINGS ARE THE SAME MODEL behind two endpoints, so CASCADE measures a second attempt on a second wire and CRITIQUE a review by the same model, not escalation to a stronger one."
        } else {
            "The second binding is a different model from the opener."
        },
        if cfg.live {
            "The provider was live; the numbers are this model's on this small task set and say nothing about other models or tasks. A ceiling effect is likely on tasks this small."
        } else {
            "The provider was a scripted stand-in: this checks the harness and is not a result."
        }
    );
    let mut report = PairedReport {
        schema: SCHEMA.into(),
        live: cfg.live,
        task_set: task_set.clone(),
        opener: cfg.opener.clone(),
        second: cfg.second.clone(),
        registry_generation: registry.generation.clone(),
        registry_digest: registry.digest.clone(),
        repeats: cfg.repeats,
        arms,
        comparisons,
        gate,
        samples: families,
        run_ids: runs
            .iter()
            .map(|r| {
                (
                    format!("{}#{}/{}", r.task, r.repeat, r.arm.label()),
                    r.run_id.clone(),
                    r.stats_version.clone(),
                )
            })
            .collect(),
        rule: VERDICT_RULE.into(),
        verdict,
        method,
        digest: String::new(),
    };
    report.digest = digest_of(&report);
    report
}

fn digest_of(r: &PairedReport) -> String {
    let mut copy = r.clone();
    copy.digest = String::new();
    hex::encode(Sha256::digest(
        serde_json::to_string(&copy).unwrap_or_default().as_bytes(),
    ))
}

fn verdict_text(arms: &[ArmSummary], comparisons: &[Comparison], live: bool) -> String {
    if !live {
        return "NOT_EVALUATED: this report came from a scripted provider and decides nothing."
            .into();
    }
    let mut parts = Vec::new();
    for a in arms {
        parts.push(format!(
            "{}: {}/{} verified ({:.2}-{:.2}), ${:.4} per run, {:.0} s per run",
            a.arm.label(),
            a.verified_success.k,
            a.verified_success.n,
            a.verified_success.lo,
            a.verified_success.hi,
            a.cost_usd.mean,
            a.latency_s.mean
        ));
    }
    for c in comparisons {
        parts.push(format!(
            "{} against direct over {} pairs: quality {}, cost {}",
            c.arm.label(),
            c.pairs,
            c.quality,
            c.cost
        ));
    }
    parts.push("the default route stays DIRECT; nothing here attests a gate".into());
    parts.join("; ")
}

/// What a complete run needs.
#[derive(Clone, Copy, Debug)]
pub struct Requirements {
    /// Tasks run.
    pub tasks: usize,
    /// Repeats per task and arm.
    pub repeats: u32,
    /// The fewest escalation samples.
    pub min_escalation_samples: u32,
    /// The fewest reviewer samples.
    pub min_reviewer_samples: u32,
    /// The fewest tasks.
    pub min_tasks: usize,
}

/// Whether a run's usage-priced cost reconciles with the Core's accounting of
/// the same request: the accounting prices every attempt of every leg (the
/// review task's too), rounds each attempt up to the next hundredth of a cent
/// and holds a reservation for any attempt whose usage was never reported. A
/// run whose cost leaves out a failed or uncertain leg differs by more than
/// that rounding and is named.
#[must_use]
pub fn reconcile(run: &PairedRun) -> Option<String> {
    let (cost, a) = (run.cost_usd?, run.accounting.as_ref()?);
    let unit = 10f64.powi(-i32::try_from(a.scale).unwrap_or(4));
    let accounted = a.total_minor as f64 * unit;
    let held = a.unknown_minor as f64 * unit;
    let slack = (2.0 * f64::from(run.model_calls) + 1.0) * unit;
    ((cost - (accounted - held)).abs() > slack).then(|| {
        format!(
            "{}#{}/{}: the cost ${cost:.6} priced from usage does not reconcile with the Core's accounting ${:.6} (of which ${held:.6} held for unknown usage): a leg's cost is missing or double counted",
            run.task,
            run.repeat,
            run.arm.label(),
            accounted
        )
    })
}

/// Check a report is whole: every requested arm appears with every planned
/// run, the failed runs counted, the registry and the task set recorded,
/// and the digest reproduces.
///
/// # Errors
/// What is missing or inconsistent, as a list.
pub fn missing(
    report: &PairedReport,
    runs: &[PairedRun],
    arms: &[Arm],
    req: &Requirements,
) -> Vec<String> {
    let Requirements {
        tasks,
        repeats,
        min_escalation_samples,
        min_reviewer_samples,
        min_tasks,
    } = *req;
    let mut m = Vec::new();
    if tasks < min_tasks {
        m.push(format!(
            "only {tasks} tasks were run; at least {min_tasks} are required"
        ));
    }
    for arm in arms {
        let want = tasks * repeats as usize;
        let have = runs.iter().filter(|r| r.arm == *arm).count();
        if have != want {
            m.push(format!("arm {} has {have} of {want} runs", arm.label()));
        }
        if !report.arms.iter().any(|a| a.arm == *arm) {
            m.push(format!("arm {} is missing from the report", arm.label()));
        }
    }
    for r in runs.iter().filter(|r| r.error.is_some()) {
        m.push(format!(
            "{}#{}/{} produced no valid measurement: {}",
            r.task,
            r.repeat,
            r.arm.label(),
            r.error.as_deref().unwrap_or_default()
        ));
    }
    for r in runs
        .iter()
        .filter(|r| r.cost_usd.is_none() && r.error.is_none())
    {
        m.push(format!(
            "{}#{}/{} has a model invocation that could not be priced",
            r.task,
            r.repeat,
            r.arm.label()
        ));
    }
    for r in runs.iter().filter(|r| r.error.is_none()) {
        if let Some(why) = reconcile(r) {
            m.push(why);
        }
    }
    for r in runs
        .iter()
        .filter(|r| r.accounting.is_none() && r.error.is_none())
    {
        m.push(format!(
            "{}#{}/{} has no accounting record of the request",
            r.task,
            r.repeat,
            r.arm.label()
        ));
    }
    if report.registry_digest.is_empty() || report.registry_generation.is_empty() {
        m.push("the registry digest and generation are not recorded".into());
    }
    if runs
        .iter()
        .any(|r| r.run_id.is_empty() && r.error.is_none())
    {
        m.push("a run id is not recorded".into());
    }
    if runs
        .iter()
        .any(|r| r.stats_version.is_empty() && r.error.is_none())
    {
        m.push("a statistics snapshot version is not recorded".into());
    }
    let fam = |k: &str| report.samples.iter().find(|s| s.kind == k);
    if let Some(f) = fam("escalation") {
        if f.samples < min_escalation_samples {
            m.push(format!(
                "{} escalation samples, {min_escalation_samples} required",
                f.samples
            ));
        }
        if f.samples > 0 && f.unknown_cost_samples > 0 {
            m.push("an escalation sample has no priced cost".into());
        }
    }
    if let Some(f) = fam("reviewer") {
        if f.samples < min_reviewer_samples {
            m.push(format!(
                "{} reviewer samples, {min_reviewer_samples} required",
                f.samples
            ));
        }
        if f.samples > 0 && f.unknown_cost_samples > 0 {
            m.push("a reviewer sample has no priced cost".into());
        }
    }
    if digest_of(report) != report.digest {
        m.push("the report digest does not reproduce".into());
    }
    m
}

/// Recompute the report from the retained runs and check its digest: the
/// analysis reproduces from the retained data.
///
/// # Errors
/// The digest differs.
pub fn rescore(
    runs: &[PairedRun],
    task_set: &TaskSet,
    cfg: &PairedConfig,
    registry: &RegistryBundle,
    expected_digest: &str,
) -> Result<PairedReport, String> {
    let r = report(runs, task_set, cfg, registry);
    if r.digest == expected_digest {
        Ok(r)
    } else {
        Err(format!(
            "the rescored digest {} is not the recorded {expected_digest}",
            r.digest
        ))
    }
}

/// The summary a person reads.
#[must_use]
pub fn summary_md(r: &PairedReport, spend: &SpendRecord, missing: &[String]) -> String {
    let mut s = String::new();
    s.push_str(&format!(
        "# PX-136 paired benchmark: DIRECT / CASCADE / CRITIQUE\n\n{}\n\n- task set: `{}` v{} ({} tasks, digest `{}`)\n- opener: `{}`; second binding: `{}`\n- registry: `{}` (digest `{}`)\n- live: {}\n- spend: ${:.4} of a ${:.2} cap over {} calls{}\n\n",
        r.verdict,
        r.task_set.id,
        r.task_set.version,
        r.task_set.tasks.len(),
        &r.task_set.digest[..12],
        r.opener.label(),
        r.second.label(),
        r.registry_generation,
        &r.registry_digest[..12.min(r.registry_digest.len())],
        r.live,
        spend.spent_usd,
        spend.cap_usd,
        spend.calls,
        if spend.stopped_by_cap { " (STOPPED BY THE CAP)" } else { "" }
    ));
    s.push_str("| arm | runs | verified success (95% Wilson) | cost/run USD (95% CI) | accounting total/run USD | cost/verified success | latency s (95% CI) | escalation | review | needed a person |\n|---|---|---|---|---|---|---|---|---|---|\n");
    for a in &r.arms {
        s.push_str(&format!(
            "| {} | {} | {}/{} = {:.3} ({:.3}-{:.3}) | {:.4} ({:.4}-{:.4}) | {:.4} | {} | {:.0} ({:.0}-{:.0}) | {}/{} | {}/{} | {}/{} |\n",
            a.arm.label(),
            a.runs,
            a.verified_success.k,
            a.verified_success.n,
            a.verified_success.p,
            a.verified_success.lo,
            a.verified_success.hi,
            a.cost_usd.mean,
            a.cost_usd.ci95.0,
            a.cost_usd.ci95.1,
            a.accounting_cost_usd.mean,
            a.cost_per_verified_success_usd.map_or("n/a".into(), |c| format!("{c:.4}")),
            a.latency_s.mean,
            a.latency_s.ci95.0,
            a.latency_s.ci95.1,
            a.escalation_rate.k,
            a.escalation_rate.n,
            a.review_rate.k,
            a.review_rate.n,
            a.human_intervention_rate.k,
            a.human_intervention_rate.n,
        ));
    }
    s.push_str("\n## Against DIRECT (paired)\n\n");
    for c in &r.comparisons {
        s.push_str(&format!(
            "- {}: {} pairs; success delta {:+.3} ({:+.3} to {:+.3}) => quality {}; cost delta {:+.4} USD ({:+.4} to {:+.4}) => cost {}; latency delta {:+.0} s\n",
            c.arm.label(),
            c.pairs,
            c.success_delta.mean,
            c.success_delta.ci95.0,
            c.success_delta.ci95.1,
            c.quality,
            c.cost_delta_usd.mean,
            c.cost_delta_usd.ci95.0,
            c.cost_delta_usd.ci95.1,
            c.cost,
            c.latency_delta_s.mean
        ));
    }
    s.push_str("\n## Gate errors against the independent check\n\n");
    for (k, g) in &r.gate {
        s.push_str(&format!(
            "- {k}: {} observations; false accepts {}/{} ({:.3}-{:.3}); false rejects {}/{} ({:.3}-{:.3}); no verdict {}\n",
            g.observations,
            g.false_accept.k,
            g.false_accept.n,
            g.false_accept.lo,
            g.false_accept.hi,
            g.false_reject.k,
            g.false_reject.n,
            g.false_reject.lo,
            g.false_reject.hi,
            g.no_verdict
        ));
    }
    s.push_str("\n## Outcome statistics samples by leg family (REQ-PX-133)\n\n");
    for f in &r.samples {
        s.push_str(&format!(
            "- {}: {} observations over {} runs, {} verified, {} with unreported cost, total priced cost {} minor units; keys: {}\n",
            f.kind,
            f.samples,
            f.runs,
            f.successes,
            f.unknown_cost_samples,
            f.total_cost_minor,
            f.keys.join(", ")
        ));
    }
    s.push_str(&format!(
        "\n## Method\n\n{}\n\nRule: {}\n\nReport digest: `{}`\n",
        r.method, r.rule, r.digest
    ));
    if !missing.is_empty() {
        s.push_str("\n## INCOMPLETE: missing measurements\n\n");
        for m in missing {
            s.push_str(&format!("- {m}\n"));
        }
    }
    s
}

// ---- Running it and retaining the bundle ----

/// What a run of the harness is asked to do.
#[derive(Clone, Debug)]
pub struct Options {
    /// The row (`px136` or `px133`).
    pub row: String,
    /// The harness configuration.
    pub cfg: PairedConfig,
    /// The pin file.
    pub taskset: PathBuf,
    /// The arms to run.
    pub arms: Vec<Arm>,
    /// The spend cap, USD.
    pub max_cost_usd: f64,
    /// Where the bundle goes.
    pub out_dir: PathBuf,
    /// The fewest tasks a complete run needs.
    pub min_tasks: usize,
    /// Run only the first `n` tasks of the pin (a pilot); the run is then
    /// incomplete by `min_tasks` unless the requirement was lowered with it.
    pub limit_tasks: Option<usize>,
    /// The fewest escalation samples a complete run needs.
    pub min_escalation_samples: u32,
    /// The fewest reviewer samples a complete run needs.
    pub min_reviewer_samples: u32,
    /// Environment the report records for the endpoints (base URLs and
    /// catalogs: configuration, never a credential).
    pub environment: BTreeMap<String, String>,
}

/// The retained result.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Bundle {
    /// The report.
    pub report: PairedReport,
    /// Every run.
    pub runs: Vec<PairedRun>,
    /// The status the workflow reads.
    pub status: crate::spend::LiveStatus,
    /// Endpoint configuration (no credential).
    pub environment: BTreeMap<String, String>,
    /// The arms requested.
    pub arms: Vec<Arm>,
    /// Tasks run.
    pub tasks: usize,
    /// The opener and second bindings and the registry, so the analysis can
    /// be re-run from this file alone.
    pub registry_trusted_key_id: String,
}

/// Run the harness and write the bundle. The returned status says whether the
/// run is complete; the bundle is written either way.
///
/// # Errors
/// The pin, the suite or the output directory cannot be used.
pub fn execute(o: &Options) -> Result<crate::spend::LiveStatus, String> {
    let suite = builtin_suite();
    let (pin, mut tasks) = load_taskset(&o.taskset, &suite, 1)?;
    if let Some(n) = o.limit_tasks {
        tasks.truncate(n);
    }
    std::fs::create_dir_all(&o.out_dir).map_err(|e| e.to_string())?;
    let files = o.out_dir.join("inputs");
    std::fs::create_dir_all(&files).map_err(|e| e.to_string())?;
    let seed: [u8; 32] = rand::random();
    let registry = build_registry(&o.cfg.opener, &o.cfg.second, seed);
    std::fs::write(files.join("registry.signed.json"), &registry.signed_json)
        .map_err(|e| e.to_string())?;
    for (arm, layer) in policy_layers() {
        std::fs::write(files.join(policy_file(arm)), layer).map_err(|e| e.to_string())?;
    }
    std::fs::create_dir_all(&o.cfg.work).map_err(|e| e.to_string())?;
    let meter = Arc::new(SpendMeter::new(o.max_cost_usd)?);
    let shared = Arc::new(Shared {
        cfg: o.cfg.clone(),
        registry: registry.clone(),
        files: files.clone(),
        meter: Arc::clone(&meter),
        max_trial_usd: Mutex::new(0.0),
        out: o.out_dir.clone(),
    });
    let planned = plan(tasks.len(), &o.arms, o.cfg.repeats);
    let runs = run_matrix(&shared, &tasks, planned);
    let mut pin_run = pin.clone();
    pin_run.tasks = tasks.iter().map(|t| t.id.clone()).collect();
    let rep = report(&runs, &pin_run, &o.cfg, &registry);
    let miss = missing(
        &rep,
        &runs,
        &o.arms,
        &Requirements {
            tasks: tasks.len(),
            repeats: o.cfg.repeats,
            min_escalation_samples: o.min_escalation_samples,
            min_reviewer_samples: o.min_reviewer_samples,
            min_tasks: o.min_tasks,
        },
    );
    let status = crate::spend::LiveStatus::new(&o.row, miss.clone(), meter.record());
    let bundle = Bundle {
        report: rep.clone(),
        runs: runs.clone(),
        status: status.clone(),
        environment: o.environment.clone(),
        arms: o.arms.clone(),
        tasks: tasks.len(),
        registry_trusted_key_id: "bench".into(),
    };
    std::fs::write(
        o.out_dir.join("result.json"),
        serde_json::to_string_pretty(&bundle).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    std::fs::write(
        o.out_dir.join("status.json"),
        serde_json::to_string_pretty(&status).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    std::fs::write(
        o.out_dir.join("summary.md"),
        summary_md(&rep, &status.spend, &status.missing),
    )
    .map_err(|e| e.to_string())?;
    Ok(status)
}

/// Recompute the report from a retained bundle and check the digest.
///
/// # Errors
/// The bundle is unreadable or the digest does not reproduce.
pub fn rescore_bundle(dir: &Path) -> Result<String, String> {
    let text = std::fs::read_to_string(dir.join("result.json")).map_err(|e| e.to_string())?;
    let bundle: Bundle = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    for r in &bundle.runs {
        if r.events_file.is_empty() {
            continue;
        }
        let events = std::fs::read(dir.join(&r.events_file))
            .map_err(|e| format!("{}: {e}", r.events_file))?;
        if hex::encode(Sha256::digest(&events)) != r.events_sha256 {
            return Err(format!(
                "{} was altered after it was retained",
                r.events_file
            ));
        }
    }
    let cfg = PairedConfig {
        cli: PathBuf::new(),
        core: PathBuf::new(),
        work: PathBuf::new(),
        opener: bundle.report.opener.clone(),
        second: bundle.report.second.clone(),
        env: vec![],
        max_turns: 0,
        trial_timeout: Duration::ZERO,
        parallel: 1,
        repeats: bundle.report.repeats,
        live: bundle.report.live,
        question_answer: String::new(),
    };
    let registry = RegistryBundle {
        signed_json: String::new(),
        trusted_key: String::new(),
        generation: bundle.report.registry_generation.clone(),
        digest: bundle.report.registry_digest.clone(),
    };
    let again = rescore(
        &bundle.runs,
        &bundle.report.task_set,
        &cfg,
        &registry,
        &bundle.report.digest,
    )?;
    Ok(again.digest)
}

/// The live environment: both provider families the registry's two bindings
/// need, refused unless every part of it is real.
///
/// # Errors
/// The first reason the run is refused, in the `LIVE: NOT RUN` wording.
pub fn live_bindings(
    var: &dyn Fn(&str) -> Option<String>,
    model: Option<&str>,
    second_endpoint: &str,
    second_model: Option<&str>,
) -> Result<(Binding, Binding, BTreeMap<String, String>), String> {
    let get = |n: &str| {
        var(n)
            .map(|v| v.trim().to_owned())
            .filter(|v| !v.is_empty())
    };
    if get("MODBIT_LIVE").as_deref() != Some("1") {
        return Err("LIVE: NOT RUN (MODBIT_LIVE=1 is not set)".into());
    }
    let model = model
        .map(str::to_owned)
        .or_else(|| get("MODBIT_LIVE_MODEL"))
        .ok_or("LIVE: NOT RUN (--model or MODBIT_LIVE_MODEL is not set)")?;
    let second_model = second_model.map_or_else(|| model.clone(), str::to_owned);
    let allow_loopback = get("MODBIT_LIVE_ALLOW_LOOPBACK").as_deref() == Some("1");
    let mut environment = BTreeMap::new();
    let mut binding = |endpoint: &str, model: &str| -> Result<Binding, String> {
        let upper = endpoint.to_ascii_uppercase();
        let key_var = format!("{upper}_API_KEY");
        let key = get(&key_var).ok_or_else(|| {
            format!("LIVE: NOT RUN ({key_var} is not set; nothing is recorded without a provider)")
        })?;
        if crate::live::is_placeholder(&key) {
            return Err(format!(
                "LIVE: NOT RUN ({key_var} is a placeholder, not a credential)"
            ));
        }
        if let Some(base) = get(&format!("MODBIT_{upper}_BASE_URL")) {
            if crate::live::is_loopback(&base) && !allow_loopback {
                return Err(format!(
                    "LIVE: NOT RUN (MODBIT_{upper}_BASE_URL is a loopback address, which is a stand-in)"
                ));
            }
            environment.insert(format!("MODBIT_{upper}_BASE_URL"), base);
        }
        let catalog = get(&format!("MODBIT_{upper}_MODELS")).ok_or_else(|| {
            format!(
                "LIVE: NOT RUN (MODBIT_{upper}_MODELS is not set: an unknown price is not free)"
            )
        })?;
        let price =
            price_from_catalog(&catalog, model).map_err(|e| format!("LIVE: NOT RUN ({e})"))?;
        environment.insert(format!("MODBIT_{upper}_MODELS"), catalog);
        Ok(Binding {
            endpoint: endpoint.to_owned(),
            model: model.to_owned(),
            price,
        })
    };
    let opener = binding("openai", &model)?;
    let second = binding(second_endpoint, &second_model)?;
    Ok((opener, second, environment))
}
