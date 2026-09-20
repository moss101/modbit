//! The immutable baseline bundle (docs/63 "Reporting"): suite version and
//! task-list digest, Modbit revision, harness version, model configuration,
//! trials, per-task outcomes, metrics with intervals, RepairAttempt counts,
//! flake quarantines, regression attributions, diff-invariant findings and
//! cost. Validation refuses what QUAL-PX-020 names: gold-patch access, an
//! unpinned container image, a missing or inconsistent trial count.

use std::collections::BTreeMap;

use modbit_bench_outcome_statistics::wilson;
use serde::{Deserialize, Serialize};

use crate::events::Counts;

/// The bundle schema id.
pub const SCHEMA: &str = "modbit-competence-baseline/1";

/// The frozen protocol a run declares.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Protocol {
    /// Independent trials per task.
    pub trials_per_task: u32,
    /// Turn budget per trial.
    pub max_turns: u32,
    /// `direct`: the single-model initial-leg configuration.
    pub configuration: String,
    /// Endpoint name the run pinned (`openai`, `anthropic`).
    pub endpoint: String,
    /// Model id the run pinned.
    pub model: String,
    /// Host of the endpoint's base URL (never a credential).
    pub base_url_host: String,
    /// Catalog prices in USD per million tokens (input, output) when the run
    /// configured them; `None` when the Core's default catalog priced the model.
    pub catalog_prices_usd_per_mtok: Option<(f64, f64)>,
    /// Whether any trial could see a gold patch or hidden acceptance; must be false.
    pub gold_patch_access: bool,
    /// Container images used, each pinned by digest; empty for fixture suites on the runner's toolchains.
    pub images: Vec<String>,
    /// The one answer given to typed questions.
    pub question_answer: String,
    /// What happened to protected-effect approvals.
    pub approvals: String,
}

/// The environment the run happened in.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Environment {
    /// Modbit revision (git commit).
    pub modbit_revision: String,
    /// SHA-256 over the `modbit-core` and `modbit-cli` binaries that ran.
    pub build_digest: String,
    /// Harness version.
    pub harness_version: String,
    /// Toolchain versions seen (`rustc`, `node`, `python3`, `git`).
    pub toolchains: BTreeMap<String, String>,
    /// Operating system.
    pub os: String,
}

/// Per-task economics from `GetTaskEconomics`.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Economics {
    /// Model calls.
    pub model_calls: u32,
    /// Input tokens.
    pub input_tokens: u64,
    /// Cached input tokens.
    pub cached_input_tokens: u64,
    /// Output tokens.
    pub output_tokens: u64,
    /// Cost at catalog list prices; `None` when the model is not priced.
    pub cost_usd: Option<f64>,
    /// Wall-clock milliseconds of the run.
    pub wall_ms: u64,
    /// Milliseconds inside model calls.
    pub model_ms: u64,
    /// Milliseconds inside tool calls.
    pub tool_ms: u64,
    /// Tool calls.
    pub tool_calls: u32,
}

/// The retained event log of a trial.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct EventLog {
    /// File in the bundle directory.
    pub file: String,
    /// SHA-256 of the file.
    pub sha256: String,
    /// First and last offsets.
    pub offsets: Option<(u64, u64)>,
}

/// One trial's outcome.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TrialOutcome {
    /// Task id.
    pub task: String,
    /// Trial ordinal (1-based).
    pub trial: u32,
    /// Task state at the end (`ReadyForReview`, `Completed`, `Failed`, `Cancelled`, `TIMEOUT`, …).
    pub state: String,
    /// Exit code of the last `task run --wait` / `question answer --wait`.
    pub exit_code: i32,
    /// The Core's own completion path: ReadyForReview or Completed with no attributed regression.
    pub core_verified: bool,
    /// Every acceptance command exited 0 after the hidden files were placed.
    pub acceptance_passed: bool,
    /// Every protected test file is byte-identical to before the run (DI-3).
    pub test_integrity_ok: bool,
    /// `core_verified && acceptance_passed && test_integrity_ok`.
    pub verified_success: bool,
    /// Verified success with zero RepairAttempts (docs/63, literally).
    pub first_pass: bool,
    /// Verified success whose first candidate held, or one recorded repair
    /// after a failing planned verification sufficed: at most one
    /// RepairAttempt and no RepairEscalated.
    #[serde(default)]
    pub first_candidate: bool,
    /// Counts from the event log.
    pub counts: Counts,
    /// Economics.
    pub economics: Economics,
    /// Retained log.
    pub event_log: EventLog,
    /// SHA-256 of the final `git diff` of the workspace.
    pub diff_sha256: String,
    /// Interactions the harness had to make (questions answered, approvals denied, resumes).
    pub interactions: u32,
    /// Harness-side notes (timeouts, refusals).
    #[serde(default)]
    pub notes: Vec<String>,
}

/// A rate with its 95% Wilson interval.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct Rate {
    /// Successes.
    pub successes: u32,
    /// Trials.
    pub n: u32,
    /// `successes / n`.
    pub rate: f64,
    /// 95% Wilson interval.
    pub ci95: (f64, f64),
}

impl Rate {
    /// Build from counts.
    #[must_use]
    pub fn of(successes: u32, n: u32) -> Self {
        Self {
            successes,
            n,
            rate: if n == 0 {
                0.0
            } else {
                f64::from(successes) / f64::from(n)
            },
            ci95: wilson(successes, n),
        }
    }
}

/// Doc 63 metrics over every trial.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Metrics {
    /// Verified success across all trials.
    pub verified_success: Rate,
    /// First-pass success across all trials (zero RepairAttempts).
    pub first_pass_success: Rate,
    /// First-candidate success across all trials (at most one repair attempt,
    /// no escalation).
    #[serde(default)]
    pub first_candidate_success: Rate,
    /// Repair loops: RepairAttempts per trial, as a distribution (attempts → trials).
    pub repair_loops: BTreeMap<u32, u32>,
    /// Equivalent-hypothesis escalations: count and rate per trial.
    pub equivalent_hypothesis_escalations: (u32, f64),
    /// No-progress escalations: count and rate per trial.
    pub no_progress_escalations: (u32, f64),
    /// Wrong-effect attempts blocked: total and mean per trial.
    pub wrong_effect_attempts_blocked: (u32, f64),
    /// Evidence coverage: trials whose plan and self-review were both recorded, over completed trials.
    pub evidence_coverage: Rate,
    /// Total cost (priced trials), cost per verified success, wall-clock per verified success, tool time.
    pub cost_and_time: CostAndTime,
    /// Edits without a retrieval record (must be zero).
    pub retrieval_discipline_edits_without_record: u32,
    /// Scope discipline: files outside the original plan per trial, expansions per trial, questions per trial.
    pub scope_discipline: ScopeDiscipline,
    /// Regression attributions in total.
    pub regression_attribution: u32,
    /// Flaky-check quarantines in total and per trial.
    pub flaky_check_rate: (u32, f64),
    /// DI-3 violations attempted and denied in total.
    pub test_integrity_violations: u32,
    /// Test-selection quality is not measured on these suites; stated, not implied.
    pub test_selection_quality: String,
    /// Per task.
    pub per_task: BTreeMap<String, TaskMetrics>,
}

/// Cost and time.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct CostAndTime {
    /// Sum of priced trial costs.
    pub total_cost_usd: f64,
    /// Trials without a price.
    pub unpriced_trials: u32,
    /// Cost per verified success (`None` when there is none).
    pub cost_per_verified_success_usd: Option<f64>,
    /// Wall-clock per verified success.
    pub wall_ms_per_verified_success: Option<f64>,
    /// Total tool time.
    pub tool_ms_total: u64,
    /// Total wall time.
    pub wall_ms_total: u64,
}

/// Scope discipline.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ScopeDiscipline {
    /// Files changed outside `plan_v1` per trial (mean).
    pub files_outside_original_plan_per_trial: f64,
    /// Plan revisions per trial (mean).
    pub scope_expansions_per_trial: f64,
    /// Questions asked per trial (mean).
    pub scope_questions_per_trial: f64,
}

/// Per-task metrics.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct TaskMetrics {
    /// Verified success.
    pub verified_success: Rate,
    /// First-pass success.
    pub first_pass_success: Rate,
    /// First-candidate success.
    #[serde(default)]
    pub first_candidate_success: Rate,
    /// Repair attempts per trial.
    pub repair_attempts: Vec<u32>,
    /// Trial states.
    pub states: Vec<String>,
}

/// Compute the metrics.
#[must_use]
pub fn metrics(trials: &[TrialOutcome]) -> Metrics {
    let n = u32::try_from(trials.len()).unwrap_or(u32::MAX);
    let nf = f64::from(n.max(1));
    let verified = trials.iter().filter(|t| t.verified_success).count();
    let first = trials.iter().filter(|t| t.first_pass).count();
    let candidate = trials.iter().filter(|t| t.first_candidate).count();
    let mut m = Metrics {
        verified_success: Rate::of(u32::try_from(verified).unwrap_or(u32::MAX), n),
        first_pass_success: Rate::of(u32::try_from(first).unwrap_or(u32::MAX), n),
        first_candidate_success: Rate::of(u32::try_from(candidate).unwrap_or(u32::MAX), n),
        test_selection_quality: "not measured: the fixture suites are below the size at which TARGETED selection differs from the full suite (PX-035 reports it)".into(),
        ..Metrics::default()
    };
    let mut escal = 0u32;
    let mut nop = 0u32;
    let mut denies = 0u32;
    let mut outside = 0u32;
    let mut expansions = 0u32;
    let mut questions = 0u32;
    let mut flaky = 0u32;
    let mut completed = 0u32;
    let mut covered = 0u32;
    for t in trials {
        *m.repair_loops.entry(t.counts.repair_attempts).or_insert(0) += 1;
        escal += t.counts.repair_escalations;
        nop += t.counts.no_progress_escalations;
        denies += t.counts.policy_denies;
        outside += u32::try_from(t.counts.files_outside_plan.len()).unwrap_or(u32::MAX);
        expansions += t.counts.plan_revisions;
        questions += t.counts.questions;
        flaky += t.counts.flaky_quarantines;
        m.retrieval_discipline_edits_without_record += t.counts.edits_without_retrieval;
        m.regression_attribution += t.counts.regressions_attributed;
        m.test_integrity_violations += t.counts.di3_violations;
        if t.counts.ready_for_review || t.counts.completed {
            completed += 1;
            if t.counts.plan_recorded && t.counts.self_review_unresolved.is_some() {
                covered += 1;
            }
        }
        match t.economics.cost_usd {
            Some(c) => m.cost_and_time.total_cost_usd += c,
            None => m.cost_and_time.unpriced_trials += 1,
        }
        m.cost_and_time.tool_ms_total += t.economics.tool_ms;
        m.cost_and_time.wall_ms_total += t.economics.wall_ms;
        let tm = m.per_task.entry(t.task.clone()).or_default();
        tm.repair_attempts.push(t.counts.repair_attempts);
        tm.states.push(t.state.clone());
    }
    m.equivalent_hypothesis_escalations = (escal, f64::from(escal) / nf);
    m.no_progress_escalations = (nop, f64::from(nop) / nf);
    m.wrong_effect_attempts_blocked = (denies, f64::from(denies) / nf);
    m.evidence_coverage = Rate::of(covered, completed);
    m.flaky_check_rate = (flaky, f64::from(flaky) / nf);
    m.scope_discipline = ScopeDiscipline {
        files_outside_original_plan_per_trial: f64::from(outside) / nf,
        scope_expansions_per_trial: f64::from(expansions) / nf,
        scope_questions_per_trial: f64::from(questions) / nf,
    };
    if verified > 0 {
        let v = u32::try_from(verified).unwrap_or(u32::MAX);
        m.cost_and_time.cost_per_verified_success_usd =
            Some(m.cost_and_time.total_cost_usd / f64::from(v));
        m.cost_and_time.wall_ms_per_verified_success =
            Some(m.cost_and_time.wall_ms_total as f64 / f64::from(v));
    }
    for (task, tm) in &mut m.per_task {
        let ts: Vec<&TrialOutcome> = trials.iter().filter(|t| &t.task == task).collect();
        let tn = u32::try_from(ts.len()).unwrap_or(u32::MAX);
        tm.verified_success = Rate::of(
            u32::try_from(ts.iter().filter(|t| t.verified_success).count()).unwrap_or(u32::MAX),
            tn,
        );
        tm.first_pass_success = Rate::of(
            u32::try_from(ts.iter().filter(|t| t.first_pass).count()).unwrap_or(u32::MAX),
            tn,
        );
        tm.first_candidate_success = Rate::of(
            u32::try_from(ts.iter().filter(|t| t.first_candidate).count()).unwrap_or(u32::MAX),
            tn,
        );
    }
    m
}

/// The bundle.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Bundle {
    /// Schema id.
    pub schema: String,
    /// Suite id.
    pub suite_id: String,
    /// Suite version.
    pub suite_version: String,
    /// Task-list digest (the suite file and every hidden acceptance file).
    pub task_list_digest: String,
    /// Task ids in the suite.
    pub tasks: Vec<String>,
    /// Protocol.
    pub protocol: Protocol,
    /// Environment.
    pub environment: Environment,
    /// Every trial.
    pub trials: Vec<TrialOutcome>,
    /// Metrics.
    pub metrics: Metrics,
    /// When the bundle was generated (UTC, RFC 3339).
    pub generated_at: String,
}

/// Why a bundle is refused (QUAL-PX-020 negative proof).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum BundleRefused {
    /// A trial could see gold patches or hidden acceptance.
    GoldPatchAccess,
    /// An image without a `@sha256:` digest.
    UnpinnedImage(String),
    /// No positive trial count.
    MissingTrialCount,
    /// A task's trials disagree with the declared count.
    TrialCountMismatch {
        /// Task.
        task: String,
        /// Declared.
        expected: u32,
        /// Recorded.
        found: u32,
    },
    /// No model, endpoint or revision pinned.
    Unpinned(String),
    /// No tasks.
    NoTasks,
    /// Wrong schema.
    Schema(String),
}

impl std::fmt::Display for BundleRefused {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::GoldPatchAccess => {
                write!(f, "a trial had gold-patch or hidden-acceptance access")
            }
            Self::UnpinnedImage(i) => write!(f, "image {i:?} is not pinned by digest"),
            Self::MissingTrialCount => write!(f, "the protocol states no positive trial count"),
            Self::TrialCountMismatch {
                task,
                expected,
                found,
            } => {
                write!(
                    f,
                    "task {task}: {found} trials recorded, protocol declares {expected}"
                )
            }
            Self::Unpinned(what) => write!(f, "{what} is not pinned"),
            Self::NoTasks => write!(f, "no tasks"),
            Self::Schema(s) => write!(f, "schema {s:?} is not {SCHEMA}"),
        }
    }
}

impl Bundle {
    /// Refuse what the frozen protocol forbids.
    ///
    /// # Errors
    /// See [`BundleRefused`].
    pub fn validate(&self) -> Result<(), BundleRefused> {
        if self.schema != SCHEMA {
            return Err(BundleRefused::Schema(self.schema.clone()));
        }
        if self.tasks.is_empty() {
            return Err(BundleRefused::NoTasks);
        }
        if self.protocol.gold_patch_access {
            return Err(BundleRefused::GoldPatchAccess);
        }
        for img in &self.protocol.images {
            if !img.contains("@sha256:") {
                return Err(BundleRefused::UnpinnedImage(img.clone()));
            }
        }
        if self.protocol.trials_per_task == 0 {
            return Err(BundleRefused::MissingTrialCount);
        }
        for what in [
            ("model", &self.protocol.model),
            ("endpoint", &self.protocol.endpoint),
            ("modbit_revision", &self.environment.modbit_revision),
            ("build_digest", &self.environment.build_digest),
            ("task_list_digest", &self.task_list_digest),
        ] {
            if what.1.trim().is_empty() {
                return Err(BundleRefused::Unpinned(what.0.into()));
            }
        }
        for task in &self.tasks {
            let found = u32::try_from(self.trials.iter().filter(|t| &t.task == task).count())
                .unwrap_or(u32::MAX);
            if found != self.protocol.trials_per_task {
                return Err(BundleRefused::TrialCountMismatch {
                    task: task.clone(),
                    expected: self.protocol.trials_per_task,
                    found,
                });
            }
        }
        Ok(())
    }

    /// The bytes of `baseline.json` as the harness writes them: pretty JSON
    /// and a trailing newline. The bundle is referenced by the SHA-256 of
    /// exactly these bytes ([`digest_of`]), never by a re-serialization of a
    /// parsed copy, which JSON float parsing does not make byte-stable.
    ///
    /// # Panics
    /// Never: the bundle serializes.
    #[must_use]
    pub fn to_file_bytes(&self) -> Vec<u8> {
        let mut v = serde_json::to_vec_pretty(self).expect("bundle serializes");
        v.push(b'\n');
        v
    }
}

/// The digest a bundle is referenced by: SHA-256 of its file bytes.
#[must_use]
pub fn digest_of(file_bytes: &[u8]) -> String {
    crate::sha256_hex(file_bytes)
}

fn is_digest(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit())
}

/// A competence target (PX-021) may only be recorded against an existing,
/// valid baseline: it carries the baseline's digest, and refuses without one.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TargetRecord {
    /// The baseline bundle digest the target is measured against.
    pub baseline_digest: String,
    /// Metric name (docs/63).
    pub metric: String,
    /// Language tier.
    pub tier: String,
    /// Threshold.
    pub threshold: f64,
    /// Statistical method and trial count, as the Decision Record states them.
    pub method: String,
}

impl TargetRecord {
    /// Record a target against a validated baseline.
    ///
    /// # Errors
    /// The baseline is invalid, the digest is not one, or the target is incomplete.
    pub fn against(
        baseline: &Bundle,
        baseline_digest: &str,
        metric: &str,
        tier: &str,
        threshold: f64,
        method: &str,
    ) -> Result<Self, String> {
        baseline
            .validate()
            .map_err(|e| format!("no target before a valid baseline: {e}"))?;
        if !is_digest(baseline_digest) {
            return Err(format!(
                "no target before a valid baseline: {baseline_digest:?} is not a bundle digest"
            ));
        }
        if metric.trim().is_empty() || method.trim().is_empty() || !threshold.is_finite() {
            return Err("a target names its metric, method and a finite threshold".into());
        }
        Ok(Self {
            baseline_digest: baseline_digest.into(),
            metric: metric.into(),
            tier: tier.into(),
            threshold,
            method: method.into(),
        })
    }
}
