//! Independent gate calibration (EPR-019, REQ-EPR-019; docs/61 "Gate
//! calibration", docs/27 §9.3–9.4).
//!
//! The Acceptance Gate and RealizedRisk are measured on their own, on
//! held-out corpora with oracle labels, never through plan success or routing
//! savings:
//!
//! - **acceptance** — a real candidate patch on a real fixture repository.
//!   The real verification engine runs BASELINE and COMPLETION, the real diff
//!   invariants and RealizedRisk derivation produce the rest of the evidence,
//!   and the real gate decides. The oracle is independent of all of it: hidden
//!   tests the gate never saw, run on the candidate tree. A false accept is an
//!   ACCEPT the oracle rejects; a false reject is anything but ACCEPT on a
//!   candidate the oracle accepts (INCONCLUSIVE included).
//! - **risk** — candidate facts with oracle labels for review, human and
//!   critical-surface obligations, through the real RealizedRisk rules.
//!
//! The corpora are held out: no lineage of theirs may appear in a tuning set
//! (the assurance rule corpus, the competence suite) — a contaminated holdout
//! is refused, not measured. A case whose declared label disagrees with its
//! oracle is refused as mislabeled. Rates carry Wilson 95% intervals, and the
//! release check compares the upper bounds with an approved, immutable
//! threshold profile: no profile, too few samples or an unsafe gate fails
//! it, whatever the router saves.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::Command;

use modbit_policy::{
    AssurancePolicy, CandidateFacts, ChangeKind, ChangedPath, derive_realized_risk,
};
use modbit_verification::engine::BoxFuture;
use modbit_verification::{
    ArtifactSink, CheckEvidence, CommandRunner, GateInput, InvariantEvidence, RawRun,
    RequiredAssurance, Stage, VerificationEngine, VerificationEvidence, VerificationPolicy,
};
use serde::{Deserialize, Serialize};
use sha2::Digest;

/// The suite's version, in every bundle.
pub const SUITE_VERSION: &str = "gate-calibration/1";

/// The one statistical method a profile may name.
pub const METHOD: &str = "wilson-95-upper";

/// The five independent rates (docs/61).
pub const METRICS: [&str; 5] = [
    "acceptance_false_accept_rate",
    "acceptance_false_reject_rate",
    "realized_risk_false_negative_rate",
    "realized_risk_false_positive_rate",
    "critical_surface_miss_rate",
];

fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(sha2::Sha256::digest(bytes))
}

// ---- Corpora ------------------------------------------------------------

/// A file operation of a candidate patch.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum FileOp {
    /// Write a file.
    Write {
        /// Path.
        path: String,
        /// Content.
        content: String,
    },
    /// Delete a file.
    Delete {
        /// Path.
        path: String,
    },
}

/// A hidden oracle: files added to a copy of the candidate tree and the
/// command whose success says the candidate is correct.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Oracle {
    /// `(path in the tree, file under corpora/oracles)`.
    pub files: Vec<(String, String)>,
    /// The command.
    pub argv: Vec<String>,
}

/// One acceptance case.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AcceptanceCase {
    /// Id.
    pub id: String,
    /// Task/defect lineage: what a tuning set must not share.
    pub lineage: String,
    /// Fixture repository under `tests/fixtures/repos`.
    pub fixture: String,
    /// The leg that produced the candidate (`initial` | `escalation`): gate
    /// errors are attributed to it, never to another leg.
    pub leg: String,
    /// Tests the task's acceptance names.
    pub acceptance: Vec<String>,
    /// The plan's write set.
    pub write_set: Vec<String>,
    /// The candidate patch.
    pub candidate: Vec<FileOp>,
    /// The hidden oracle.
    pub oracle: Oracle,
    /// The label the corpus declares (`correct` | `incorrect`); the oracle
    /// must agree.
    pub declared: String,
}

/// The acceptance corpus.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AcceptanceCorpus {
    /// Version.
    pub version: String,
    /// Cases.
    pub cases: Vec<AcceptanceCase>,
}

/// What the oracle says a candidate's risk obliges.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RiskOracle {
    /// Independent review is warranted.
    pub review_required: bool,
    /// A human decision is warranted.
    pub human_required: bool,
    /// A critical surface is touched.
    pub critical_surface: bool,
}

/// One risk case.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RiskCase {
    /// Id.
    pub id: String,
    /// Lineage.
    pub lineage: String,
    /// The candidate's facts.
    pub facts: CandidateFacts,
    /// The oracle labels.
    pub oracle: RiskOracle,
}

/// The risk corpus.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RiskCorpus {
    /// Version.
    pub version: String,
    /// Cases.
    pub cases: Vec<RiskCase>,
}

/// Why a corpus is not measured.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "refusal", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Refusal {
    /// A holdout lineage appears in a tuning set.
    Contaminated {
        /// Case.
        case: String,
        /// Lineage.
        lineage: String,
    },
    /// A lineage appears in both holdout corpora.
    SharedLineage {
        /// Lineage.
        lineage: String,
    },
    /// A case's declared label disagrees with its oracle.
    Mislabeled {
        /// Case.
        case: String,
        /// Declared.
        declared: String,
        /// What the oracle said.
        observed: String,
    },
    /// A case could not be run.
    CaseFailed {
        /// Case.
        case: String,
        /// Why.
        detail: String,
    },
    /// A corpus file could not be read.
    Unreadable {
        /// Detail.
        detail: String,
    },
}

/// The tuning lineages: the assurance rule corpus's cases and the
/// competence suite's tasks. A holdout shares none of them.
#[must_use]
pub fn tuning_lineages(repo_root: &Path) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    if let Ok(t) = std::fs::read_to_string(
        repo_root.join("crates/policy/tests/fixtures/assurance/corpus.json"),
    ) && let Ok(v) = serde_json::from_str::<serde_json::Value>(&t)
    {
        for c in v.as_array().into_iter().flatten() {
            if let Some(n) = c["name"].as_str() {
                out.insert(n.to_owned());
            }
        }
    }
    if let Ok(t) = std::fs::read_to_string(
        repo_root.join("benchmarks/agent-engineering/suites/internal/tasks.json"),
    ) && let Ok(v) = serde_json::from_str::<serde_json::Value>(&t)
    {
        for c in v["tasks"].as_array().into_iter().flatten() {
            if let Some(n) = c["id"].as_str() {
                out.insert(n.to_owned());
            }
        }
    }
    out
}

/// Refuse a holdout that is not held out.
///
/// # Errors
/// A contaminated case, or a lineage shared between the two corpora.
pub fn check_separation(
    acceptance: &AcceptanceCorpus,
    risk: &RiskCorpus,
    tuning: &BTreeSet<String>,
) -> Result<(), Refusal> {
    let mut acc = BTreeSet::new();
    for c in &acceptance.cases {
        if tuning.contains(&c.lineage) || tuning.contains(&c.id) {
            return Err(Refusal::Contaminated {
                case: c.id.clone(),
                lineage: c.lineage.clone(),
            });
        }
        acc.insert(c.lineage.clone());
    }
    for c in &risk.cases {
        if tuning.contains(&c.lineage) || tuning.contains(&c.id) {
            return Err(Refusal::Contaminated {
                case: c.id.clone(),
                lineage: c.lineage.clone(),
            });
        }
        if acc.contains(&c.lineage) {
            return Err(Refusal::SharedLineage {
                lineage: c.lineage.clone(),
            });
        }
    }
    Ok(())
}

// ---- Running the real paths ---------------------------------------------

/// Commands run as local processes (the fixtures' own test commands).
pub struct ProcessRunner;

impl CommandRunner for ProcessRunner {
    fn run<'a>(
        &'a self,
        argv: &'a [String],
        cwd: &'a Path,
        env: &'a [(String, String)],
        timeout_ms: u64,
    ) -> BoxFuture<'a, RawRun> {
        Box::pin(async move {
            let argv = argv.to_vec();
            let cwd = cwd.to_path_buf();
            let env = env.to_vec();
            tokio::task::spawn_blocking(move || {
                let started = std::time::Instant::now();
                match Command::new(&argv[0])
                    .args(&argv[1..])
                    .current_dir(&cwd)
                    .envs(env.iter().cloned())
                    .output()
                {
                    Ok(o) => RawRun {
                        exit_code: o.status.code(),
                        stdout: String::from_utf8_lossy(&o.stdout).into_owned(),
                        stderr: String::from_utf8_lossy(&o.stderr).into_owned(),
                        reporter_file: None,
                        timed_out: u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX)
                            > timeout_ms,
                        cancelled: false,
                    },
                    Err(e) => RawRun {
                        exit_code: None,
                        stdout: String::new(),
                        stderr: e.to_string(),
                        reporter_file: None,
                        timed_out: false,
                        cancelled: false,
                    },
                }
            })
            .await
            .unwrap_or(RawRun {
                exit_code: None,
                stdout: String::new(),
                stderr: "the runner task failed".into(),
                reporter_file: None,
                timed_out: false,
                cancelled: false,
            })
        })
    }
}

/// Raw output kept in memory for the bundle's digests.
#[derive(Default)]
pub struct MemSink(std::sync::Mutex<Vec<Vec<u8>>>);

impl ArtifactSink for MemSink {
    fn put(&self, bytes: &[u8]) -> String {
        if let Ok(mut v) = self.0.lock() {
            v.push(bytes.to_vec());
        }
        sha256_hex(bytes)
    }
}

fn copy_tree(src: &Path, dst: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for e in std::fs::read_dir(src)? {
        let e = e?;
        let name = e.file_name();
        if name == "target" || name == "node_modules" || name == ".git" {
            continue;
        }
        let p = e.path();
        if p.is_dir() {
            copy_tree(&p, &dst.join(&name))?;
        } else {
            // Checkouts on Windows may carry CRLF; fixtures are LF text.
            let bytes = std::fs::read(&p)?;
            let text = String::from_utf8_lossy(&bytes).replace("\r\n", "\n");
            std::fs::write(dst.join(&name), text)?;
        }
    }
    Ok(())
}

/// What the gate and the oracle said about one acceptance case.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AcceptanceOutcome {
    /// Case.
    pub id: String,
    /// Lineage.
    pub lineage: String,
    /// Leg.
    pub leg: String,
    /// The gate's verdict.
    pub verdict: String,
    /// The oracle's: the candidate is correct.
    pub oracle_correct: bool,
    /// Missing evidence the gate named.
    pub missing_evidence: Vec<String>,
    /// Its reject reasons.
    pub reject_reasons: Vec<String>,
    /// The realized risk level the gate was obliged by.
    pub risk_level: String,
    /// sha256 of the gate result.
    pub gate_result_digest: String,
}

impl AcceptanceOutcome {
    /// ACCEPT on a candidate the oracle rejects.
    #[must_use]
    pub fn false_accept(&self) -> bool {
        self.verdict == "ACCEPT" && !self.oracle_correct
    }

    /// Anything but ACCEPT on a candidate the oracle accepts.
    #[must_use]
    pub fn false_reject(&self) -> bool {
        self.verdict != "ACCEPT" && self.oracle_correct
    }
}

fn changed_files(
    base: &Path,
    ops: &[FileOp],
) -> (Vec<modbit_verification::ChangedFile>, Vec<ChangedPath>) {
    let mut files = Vec::new();
    let mut facts = Vec::new();
    for op in ops {
        let (path, new) = match op {
            FileOp::Write { path, content } => (path.clone(), Some(content.clone())),
            FileOp::Delete { path } => (path.clone(), None),
        };
        let old = std::fs::read_to_string(base.join(&path)).ok();
        let count = |t: &Option<String>| t.as_deref().map_or(0, |t| t.lines().count());
        let (added, removed) = match (&old, &new) {
            (Some(o), Some(n)) => {
                let o: BTreeSet<&str> = o.lines().collect();
                let n: BTreeSet<&str> = n.lines().collect();
                (n.difference(&o).count(), o.difference(&n).count())
            }
            _ => (count(&new), count(&old)),
        };
        facts.push(ChangedPath {
            path: path.clone(),
            change: match (&old, &new) {
                (None, _) => ChangeKind::Created,
                (_, None) => ChangeKind::Deleted,
                _ => ChangeKind::Modified,
            },
            lines_added: u32::try_from(added).unwrap_or(u32::MAX),
            lines_removed: u32::try_from(removed).unwrap_or(u32::MAX),
        });
        files.push(modbit_verification::ChangedFile { path, old, new });
    }
    (files, facts)
}

fn label<T: Serialize>(v: &T) -> String {
    serde_json::to_value(v)
        .ok()
        .and_then(|x| x.as_str().map(str::to_owned))
        .unwrap_or_else(|| format!("{:?}", serde_json::to_value(v).unwrap_or_default()))
}

/// Run one acceptance case through the real engine, invariants, risk
/// derivation and gate, then its hidden oracle.
///
/// # Errors
/// The case could not be run, or its declared label disagrees with its
/// oracle.
#[allow(clippy::too_many_lines)]
pub async fn run_acceptance_case(
    case: &AcceptanceCase,
    repo_root: &Path,
    corpora: &Path,
    work: &Path,
    policy: &AssurancePolicy,
) -> Result<AcceptanceOutcome, Refusal> {
    let fail = |detail: String| Refusal::CaseFailed {
        case: case.id.clone(),
        detail,
    };
    let dir = work.join(&case.id);
    let _ = std::fs::remove_dir_all(&dir);
    let base = dir.join("base");
    let tree = dir.join("candidate");
    let fixture = repo_root.join("tests/fixtures/repos").join(&case.fixture);
    copy_tree(&fixture, &base).map_err(|e| fail(format!("fixture: {e}")))?;
    copy_tree(&fixture, &tree).map_err(|e| fail(format!("fixture: {e}")))?;
    let target = work.join("cargo-target");
    let env: Vec<(String, String)> = vec![
        (
            "FIXTURE_FLAKY_STATE".into(),
            dir.join("flaky-state").to_string_lossy().into_owned(),
        ),
        (
            "CARGO_TARGET_DIR".into(),
            target.to_string_lossy().into_owned(),
        ),
        ("CARGO_TERM_COLOR".into(), "never".into()),
    ];
    let own: Vec<String> = env.iter().map(|(k, _)| k.clone()).collect();
    let env: Vec<(String, String)> = std::env::vars()
        .filter(|(k, _)| !own.contains(k))
        .chain(env)
        .collect();
    let plan = modbit_verification::plan::derive(
        &tree,
        &case.acceptance,
        &modbit_verification::plan::configured_commands(&tree),
    );
    if plan.commands.is_empty() {
        return Err(fail("the fixture has no verification command".into()));
    }
    let sink = MemSink::default();
    let engine = VerificationEngine::new(&ProcessRunner, &sink, VerificationPolicy::default());
    let (baseline, _) = engine
        .run_stage(
            &plan,
            &case.id,
            &case.id,
            Stage::Baseline,
            &tree,
            "rev-0",
            &env,
            &[],
        )
        .await;
    for op in &case.candidate {
        match op {
            FileOp::Write { path, content } => {
                let p = tree.join(path);
                if let Some(parent) = p.parent() {
                    std::fs::create_dir_all(parent).map_err(|e| fail(e.to_string()))?;
                }
                std::fs::write(&p, content).map_err(|e| fail(e.to_string()))?;
            }
            FileOp::Delete { path } => {
                let _ = std::fs::remove_file(tree.join(path));
            }
        }
    }
    let (completion, _) = engine
        .run_stage(
            &plan,
            &case.id,
            &case.id,
            Stage::Completion,
            &tree,
            "rev-1",
            &env,
            &[],
        )
        .await;
    let attribution = modbit_verification::attribute(&baseline, &completion, &plan);
    let (files, changed) = changed_files(&base, &case.candidate);
    let baseline_failing: Vec<String> = baseline
        .checks()
        .into_iter()
        .filter(|c| {
            matches!(
                c.status,
                modbit_verification::CheckStatus::Fail | modbit_verification::CheckStatus::Error
            )
        })
        .filter_map(|c| c.location.symbol.clone())
        .collect();
    let ctx = modbit_verification::InvariantContext {
        write_set: Some(case.write_set.clone()),
        plan_entries: case.write_set.clone(),
        acceptance_named: case.acceptance.clone(),
        baseline_failing,
        protected_paths: vec![],
        formatting_churn_lines: 200,
        expected_revision: None,
    };
    let violations = modbit_verification::evaluate_diff(&ctx, &files, Some(1));
    let invariants = InvariantEvidence {
        deny: modbit_verification::denies(&violations),
        open_flags: vec![],
        refs: violations.iter().map(|v| v.id.clone()).collect(),
    };
    let risk = derive_realized_risk(
        policy,
        &CandidateFacts {
            candidate_revision: 1,
            changed,
            plan_write_set: Some(case.write_set.clone()),
            requested_effects: vec![],
            advisory: modbit_policy::Advisory::default(),
        },
    );
    let input = GateInput {
        plan_id: case.id.clone(),
        leg_id: case.leg.clone(),
        candidate_revision: 1,
        required: RequiredAssurance {
            level: policy.minimum_assurance.max(risk.minimum_assurance),
            independent_review: risk.independent_review_required,
            human: risk.human_required,
            required_checks: policy.required_checks.clone(),
            realized_risk_ref: risk.facts_digest.clone(),
            risk_version: risk.realized_risk_version.clone(),
            policy_version: risk.policy_version.clone(),
            forbidden_effects_requested: risk.forbidden_effects_requested.clone(),
        },
        verification: Some(VerificationEvidence {
            verification_run_id: completion.verification_run_id.clone(),
            stage: "COMPLETION".into(),
            candidate_revision: 1,
            status: format!("{:?}", completion.status).to_uppercase(),
            checks: completion
                .checks()
                .into_iter()
                .map(|c| CheckEvidence {
                    check_id: c.check_id.clone(),
                    kind: format!("{:?}", c.kind).to_lowercase(),
                    status: format!("{:?}", c.status).to_uppercase(),
                })
                .collect(),
            report_refs: completion.report_refs.clone(),
            attribution: attribution
                .checks
                .iter()
                .map(|(id, a)| (id.clone(), label(a)))
                .collect(),
        }),
        invariants,
        reviews: vec![],
    };
    let result = modbit_verification::evaluate_gate(&input);
    // The oracle: hidden tests on a copy of the candidate tree.
    let oracle = dir.join("oracle");
    copy_tree(&tree, &oracle).map_err(|e| fail(format!("oracle copy: {e}")))?;
    for (path, from) in &case.oracle.files {
        let p = oracle.join(path);
        if let Some(parent) = p.parent() {
            std::fs::create_dir_all(parent).map_err(|e| fail(e.to_string()))?;
        }
        std::fs::copy(corpora.join("oracles").join(from), &p)
            .map_err(|e| fail(format!("oracle file {from}: {e}")))?;
    }
    let status = Command::new(&case.oracle.argv[0])
        .args(&case.oracle.argv[1..])
        .current_dir(&oracle)
        .envs(env.iter().cloned())
        .output()
        .map_err(|e| fail(format!("oracle: {e}")))?;
    let oracle_correct = status.status.success();
    let observed = if oracle_correct {
        "correct"
    } else {
        "incorrect"
    };
    if case.declared != observed {
        return Err(Refusal::Mislabeled {
            case: case.id.clone(),
            declared: case.declared.clone(),
            observed: observed.into(),
        });
    }
    Ok(AcceptanceOutcome {
        id: case.id.clone(),
        lineage: case.lineage.clone(),
        leg: case.leg.clone(),
        verdict: result.verdict.label().to_owned(),
        oracle_correct,
        missing_evidence: result.missing_evidence.clone(),
        reject_reasons: result.reject_reasons.clone(),
        risk_level: label(&risk.level),
        gate_result_digest: sha256_hex(&serde_json::to_vec(&result).unwrap_or_default()),
    })
}

/// What the rules and the oracle said about one risk case.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RiskOutcome {
    /// Case.
    pub id: String,
    /// Lineage.
    pub lineage: String,
    /// Derived level.
    pub level: String,
    /// Derived: review required.
    pub review_required: bool,
    /// Derived: human required.
    pub human_required: bool,
    /// The oracle.
    pub oracle: RiskOracle,
    /// The oracle obliges more than the rules did.
    pub false_negative: bool,
    /// The rules obliged what the oracle says is not warranted.
    pub false_positive: bool,
    /// A critical surface the rules did not send to a human.
    pub critical_miss: bool,
}

/// Run one risk case through the real RealizedRisk rules.
#[must_use]
pub fn run_risk_case(case: &RiskCase, policy: &AssurancePolicy) -> RiskOutcome {
    let r = derive_realized_risk(policy, &case.facts);
    let derived_obliges = r.independent_review_required || r.human_required;
    let oracle_obliges = case.oracle.review_required || case.oracle.human_required;
    RiskOutcome {
        id: case.id.clone(),
        lineage: case.lineage.clone(),
        level: label(&r.level),
        review_required: r.independent_review_required,
        human_required: r.human_required,
        oracle: case.oracle.clone(),
        false_negative: (case.oracle.review_required
            && !(r.independent_review_required || r.human_required))
            || (case.oracle.human_required && !r.human_required),
        false_positive: derived_obliges && !oracle_obliges,
        critical_miss: case.oracle.critical_surface && !r.human_required,
    }
}

// ---- Metrics --------------------------------------------------------------

/// A rate with its Wilson 95% interval.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Rate {
    /// Errors.
    pub count: u32,
    /// Denominator.
    pub n: u32,
    /// Point estimate (0 when `n` is 0).
    pub rate: f64,
    /// Lower bound.
    pub lower: f64,
    /// Upper bound (1 when `n` is 0: nothing measured is nothing known).
    pub upper: f64,
}

/// A rate and its Wilson 95% interval.
#[must_use]
pub fn wilson(count: u32, n: u32) -> Rate {
    if n == 0 {
        return Rate {
            count,
            n,
            rate: 0.0,
            lower: 0.0,
            upper: 1.0,
        };
    }
    let z = 1.959_963_984_540_054_f64;
    let nf = f64::from(n);
    let p = f64::from(count) / nf;
    let z2 = z * z;
    let denom = 1.0 + z2 / nf;
    let centre = (p + z2 / (2.0 * nf)) / denom;
    let half = z * ((p * (1.0 - p) + z2 / (4.0 * nf)) / nf).sqrt() / denom;
    Rate {
        count,
        n,
        rate: p,
        // The interval contains the estimate; at 0 or n the bound is exact,
        // not a rounding error on the wrong side of it.
        lower: (centre - half).clamp(0.0, p),
        upper: (centre + half).clamp(p, 1.0),
    }
}

/// The independent rates, and the acceptance rates per leg.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Metrics {
    /// By name (`METRICS`).
    pub rates: BTreeMap<String, Rate>,
    /// Leg → (false accept, false reject).
    pub per_leg: BTreeMap<String, (Rate, Rate)>,
}

fn u(n: usize) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

/// Compute the rates.
#[must_use]
pub fn metrics(acceptance: &[AcceptanceOutcome], risk: &[RiskOutcome]) -> Metrics {
    let acc_rates = |cases: &[&AcceptanceOutcome]| {
        let incorrect: Vec<_> = cases.iter().filter(|c| !c.oracle_correct).collect();
        let correct: Vec<_> = cases.iter().filter(|c| c.oracle_correct).collect();
        (
            wilson(
                u(incorrect.iter().filter(|c| c.false_accept()).count()),
                u(incorrect.len()),
            ),
            wilson(
                u(correct.iter().filter(|c| c.false_reject()).count()),
                u(correct.len()),
            ),
        )
    };
    let all: Vec<&AcceptanceOutcome> = acceptance.iter().collect();
    let (fa, fr) = acc_rates(&all);
    let obliged: Vec<_> = risk
        .iter()
        .filter(|r| r.oracle.review_required || r.oracle.human_required)
        .collect();
    let benign: Vec<_> = risk
        .iter()
        .filter(|r| !(r.oracle.review_required || r.oracle.human_required))
        .collect();
    let critical: Vec<_> = risk.iter().filter(|r| r.oracle.critical_surface).collect();
    let mut rates = BTreeMap::new();
    rates.insert(METRICS[0].to_owned(), fa);
    rates.insert(METRICS[1].to_owned(), fr);
    rates.insert(
        METRICS[2].to_owned(),
        wilson(
            u(obliged.iter().filter(|r| r.false_negative).count()),
            u(obliged.len()),
        ),
    );
    rates.insert(
        METRICS[3].to_owned(),
        wilson(
            u(benign.iter().filter(|r| r.false_positive).count()),
            u(benign.len()),
        ),
    );
    rates.insert(
        METRICS[4].to_owned(),
        wilson(
            u(critical.iter().filter(|r| r.critical_miss).count()),
            u(critical.len()),
        ),
    );
    let mut per_leg = BTreeMap::new();
    let legs: BTreeSet<&str> = acceptance.iter().map(|c| c.leg.as_str()).collect();
    for leg in legs {
        let cases: Vec<&AcceptanceOutcome> = acceptance.iter().filter(|c| c.leg == leg).collect();
        per_leg.insert(leg.to_owned(), acc_rates(&cases));
    }
    Metrics { rates, per_leg }
}

// ---- Threshold profile and release check ----------------------------------

/// An approved, immutable threshold profile (docs/61): who approved it,
/// how rates are bounded, the largest upper bound each rate may have and the
/// samples each needs. The numbers are the owner's; this suite invents none.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ThresholdProfile {
    /// Id.
    pub profile_id: String,
    /// Who approved it.
    pub approved_by: String,
    /// When.
    pub approved_at: String,
    /// `wilson-95-upper`.
    pub method: String,
    /// Metric → largest allowed upper bound.
    pub limits: BTreeMap<String, f64>,
    /// Metric → smallest denominator measured.
    pub min_samples: BTreeMap<String, u32>,
}

/// The release check.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ReleaseCheck {
    /// `PASS` | `FAIL`.
    pub verdict: String,
    /// Why it failed.
    pub reasons: Vec<String>,
    /// The profile it was checked against, by digest.
    pub profile_digest: Option<String>,
    /// What the router saves (negative) or costs, minor units: recorded,
    /// never a reason to pass.
    pub router_cost_delta_minor: i64,
}

/// Check the rates against the profile. A missing profile, a metric the
/// profile does not bound, too few samples or an upper bound over its limit
/// fails; the router's savings are recorded and change nothing.
#[must_use]
pub fn release_check(
    m: &Metrics,
    profile: Option<(&ThresholdProfile, &str)>,
    router_cost_delta_minor: i64,
) -> ReleaseCheck {
    let mut reasons = Vec::new();
    let digest = profile.map(|(_, d)| d.to_owned());
    match profile {
        None => reasons.push(
            "MISSING_THRESHOLD_PROFILE: no approved threshold profile; missing thresholds fail promotion"
                .into(),
        ),
        Some((p, _)) => {
            if p.method != METHOD {
                reasons.push(format!(
                    "METHOD_UNSUPPORTED: `{}` (only {METHOD})",
                    p.method
                ));
            }
            if p.approved_by.trim().is_empty() {
                reasons.push("UNAPPROVED_PROFILE: the profile names no approver".into());
            }
            for name in METRICS {
                let rate = m.rates.get(name);
                let Some(limit) = p.limits.get(name) else {
                    reasons.push(format!("MISSING_THRESHOLD: {name}"));
                    continue;
                };
                let Some(rate) = rate else {
                    reasons.push(format!("UNMEASURED: {name}"));
                    continue;
                };
                let min = p.min_samples.get(name).copied().unwrap_or(1);
                if rate.n < min {
                    reasons.push(format!(
                        "INSUFFICIENT_SAMPLES: {name} measured on {} (profile needs {min})",
                        rate.n
                    ));
                }
                if rate.upper > *limit {
                    reasons.push(format!(
                        "UNSAFE_GATE: {name} upper bound {:.4} exceeds {limit}",
                        rate.upper
                    ));
                }
            }
        }
    }
    ReleaseCheck {
        verdict: if reasons.is_empty() { "PASS" } else { "FAIL" }.into(),
        reasons,
        profile_digest: digest,
        router_cost_delta_minor,
    }
}

// ---- The bundle ---------------------------------------------------------------

/// Digests of what was measured.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CorporaDigests {
    /// The acceptance corpus.
    pub acceptance: String,
    /// The risk corpus.
    pub risk: String,
    /// Every oracle file, in name order.
    pub oracles: String,
    /// The tuning lineages the holdout was checked against.
    pub tuning: String,
}

/// One calibration, whole.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Bundle {
    /// Suite version.
    pub suite_version: String,
    /// Gate rules version.
    pub gate_version: String,
    /// Realized-risk rules version.
    pub risk_version: String,
    /// Assurance policy version.
    pub policy_version: String,
    /// Corpora and oracle digests.
    pub corpora: CorporaDigests,
    /// Every acceptance case.
    pub acceptance: Vec<AcceptanceOutcome>,
    /// Every risk case.
    pub risk: Vec<RiskOutcome>,
    /// The rates.
    pub metrics: Metrics,
    /// The release check.
    pub release: ReleaseCheck,
}

fn read_json<T: serde::de::DeserializeOwned>(p: &Path) -> Result<(T, String), Refusal> {
    let bytes = std::fs::read(p).map_err(|e| Refusal::Unreadable {
        detail: format!("{}: {e}", p.display()),
    })?;
    let v = serde_json::from_slice(&bytes).map_err(|e| Refusal::Unreadable {
        detail: format!("{}: {e}", p.display()),
    })?;
    Ok((v, sha256_hex(&bytes)))
}

fn oracles_digest(corpora: &Path) -> String {
    let mut names: Vec<PathBuf> = std::fs::read_dir(corpora.join("oracles"))
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .collect();
    names.sort();
    let mut h = sha2::Sha256::new();
    for p in names {
        h.update(
            p.file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .as_bytes(),
        );
        h.update(std::fs::read(&p).unwrap_or_default());
    }
    hex::encode(h.finalize())
}

/// Calibrate: check the holdout is held out, run every case on the real
/// paths, compute the rates and check them against `profile`.
///
/// # Errors
/// A contaminated or mislabeled holdout, or a case that could not be run.
pub async fn calibrate(
    repo_root: &Path,
    corpora: &Path,
    work: &Path,
    profile: Option<(&ThresholdProfile, &str)>,
    router_cost_delta_minor: i64,
) -> Result<Bundle, Refusal> {
    let (acceptance, acc_digest): (AcceptanceCorpus, String) =
        read_json(&corpora.join("acceptance-holdout.json"))?;
    let (risk, risk_digest): (RiskCorpus, String) = read_json(&corpora.join("risk-holdout.json"))?;
    let tuning = tuning_lineages(repo_root);
    check_separation(&acceptance, &risk, &tuning)?;
    let policy = AssurancePolicy::default();
    let mut acc = Vec::new();
    for case in &acceptance.cases {
        acc.push(run_acceptance_case(case, repo_root, corpora, work, &policy).await?);
    }
    let risk_out: Vec<RiskOutcome> = risk
        .cases
        .iter()
        .map(|c| run_risk_case(c, &policy))
        .collect();
    let m = metrics(&acc, &risk_out);
    let release = release_check(&m, profile, router_cost_delta_minor);
    Ok(Bundle {
        suite_version: SUITE_VERSION.into(),
        gate_version: modbit_verification::gate::GATE_VERSION.into(),
        risk_version: modbit_policy::assurance::REALIZED_RISK_RULES_VERSION.into(),
        policy_version: policy.version(),
        corpora: CorporaDigests {
            acceptance: acc_digest,
            risk: risk_digest,
            oracles: oracles_digest(corpora),
            tuning: sha256_hex(
                tuning
                    .iter()
                    .cloned()
                    .collect::<Vec<_>>()
                    .join("\n")
                    .as_bytes(),
            ),
        },
        acceptance: acc,
        risk: risk_out,
        metrics: m,
        release,
    })
}
