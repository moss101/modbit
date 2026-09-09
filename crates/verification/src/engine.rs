//! Verification stages (docs/64 §1, §3): BASELINE, TARGETED, COMPLETION and
//! RERUN over a command runner port; the flake rerun protocol; regression
//! attribution between BASELINE and COMPLETION.

use std::collections::BTreeMap;
use std::future::Future;
use std::path::Path;
use std::pin::Pin;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::adapters::{RawRun, detect, parse};
use crate::plan::{CheckCommand, VerificationPlan};
use crate::report::{
    CheckResult, CheckStatus, Confidence, ParserInfo, ReportStatus, RunnerFamily, RunnerInfo,
    Stage, TestReport, failure_signature,
};

/// A boxed future.
pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// Where commands actually run (the Core binds this to the process broker).
pub trait CommandRunner: Send + Sync {
    /// Run `argv` in `cwd` with `env`, bounded by `timeout_ms`; return the raw outcome.
    fn run<'a>(
        &'a self,
        argv: &'a [String],
        cwd: &'a Path,
        env: &'a [(String, String)],
        timeout_ms: u64,
    ) -> BoxFuture<'a, RawRun>;
}

/// Where raw output goes (content-addressed).
pub trait ArtifactSink: Send + Sync {
    /// Store bytes, return the hash.
    fn put(&self, bytes: &[u8]) -> String;
}

/// docs/64 `VerificationRun`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct VerificationRun {
    /// Id.
    pub verification_run_id: String,
    /// Agent run.
    pub run_id: String,
    /// Plan ref.
    pub plan_ref: String,
    /// Stage.
    pub stage: Stage,
    /// Candidate revision.
    pub candidate_revision: String,
    /// Environment digest.
    pub environment_digest: String,
    /// Status.
    pub status: ReportStatus,
    /// Reports, one per command.
    pub reports: Vec<TestReport>,
    /// Report refs (object hashes) in the same order.
    pub report_refs: Vec<String>,
    /// Wall clock.
    pub duration_ms: u64,
}

impl VerificationRun {
    /// All checks of this stage's reports (isolated RERUN reports attached
    /// by the flake protocol are evidence, not a second set of results).
    #[must_use]
    pub fn checks(&self) -> Vec<&CheckResult> {
        self.reports
            .iter()
            .filter(|r| r.stage == self.stage)
            .flat_map(|r| r.checks.iter())
            .collect()
    }

    /// Failure signatures of this run (FLAKY/UNKNOWN never form one).
    #[must_use]
    pub fn failure_signatures(&self) -> Vec<String> {
        self.checks()
            .into_iter()
            .filter_map(failure_signature)
            .collect()
    }
}

/// Policy knobs (docs/64 §8 Alpha defaults).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct VerificationPolicy {
    /// Isolated reruns of failed checks.
    pub flake_rerun: u32,
    /// Consecutive isolated passes to accept a flaky mandatory check.
    pub mandatory_flaky_passes: u32,
    /// TARGETED wall-clock budget.
    pub targeted_run_budget_ms: u64,
    /// BASELINE/COMPLETION budget.
    pub full_run_budget_ms: u64,
}

impl Default for VerificationPolicy {
    fn default() -> Self {
        Self {
            flake_rerun: 1,
            mandatory_flaky_passes: 3,
            targeted_run_budget_ms: 5 * 60 * 1000,
            full_run_budget_ms: 10 * 60 * 1000,
        }
    }
}

/// A quarantine produced by the rerun protocol.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Quarantine {
    /// Check.
    pub check_id: String,
    /// The run where it failed.
    pub first_run_id: String,
    /// The rerun where it passed.
    pub rerun_id: String,
    /// Revision the quarantine is scoped to.
    pub candidate_revision: String,
}

/// Attribution of one check between BASELINE and COMPLETION (docs/64 §1).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Attribution {
    /// PASS at baseline, failing now: blocks acceptance.
    Regression,
    /// Failing at baseline and still failing.
    KnownFailing,
    /// Failing at baseline, passing now.
    CollateralFix,
    /// Failing now but declared as an expected change before the run.
    DeclaredChange,
    /// Flaky at completion (after the rerun protocol).
    Flaky,
    /// New check, failing.
    NewFailing,
    /// Passing (or new and passing).
    Pass,
}

/// Regression attribution result.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AttributionReport {
    /// Per check.
    pub checks: Vec<(String, Attribution)>,
    /// Whether acceptance is blocked (any REGRESSION, NEW_FAILING mandatory, or INDETERMINATE).
    pub blocks_acceptance: bool,
    /// Acceptance is INCONCLUSIVE (flaky mandatory check or UNKNOWN outcome).
    pub inconclusive: bool,
    /// Reasons.
    pub reasons: Vec<String>,
}

/// The engine.
pub struct VerificationEngine<'a> {
    runner: &'a dyn CommandRunner,
    sink: &'a dyn ArtifactSink,
    policy: VerificationPolicy,
}

fn digest(parts: &[String]) -> String {
    let mut h = Sha256::new();
    for p in parts {
        h.update(p.as_bytes());
        h.update([0]);
    }
    hex::encode(h.finalize())
}

/// Environment digest: toolchain versions and lockfile digests (docs/64 §1).
#[must_use]
pub fn environment_digest(root: &Path, inputs: &[String], toolchain: &[String]) -> String {
    let mut parts: Vec<String> = toolchain.to_vec();
    for i in inputs {
        let p = root.join(i);
        let bytes = std::fs::read(&p).unwrap_or_default();
        parts.push(format!("{i}={}", hex::encode(Sha256::digest(&bytes))));
    }
    digest(&parts)
}

impl<'a> VerificationEngine<'a> {
    /// Build.
    pub fn new(
        runner: &'a dyn CommandRunner,
        sink: &'a dyn ArtifactSink,
        policy: VerificationPolicy,
    ) -> Self {
        Self {
            runner,
            sink,
            policy,
        }
    }

    /// Filter argv to specific checks for a RERUN (runner-specific).
    fn rerun_argv(cmd: &CheckCommand, checks: &[&CheckResult]) -> Vec<String> {
        let names: Vec<String> = checks
            .iter()
            .filter_map(|c| c.location.symbol.clone())
            .collect();
        match cmd.family {
            RunnerFamily::Cargo => {
                let mut v: Vec<String> = cmd
                    .argv
                    .iter()
                    .take_while(|a| *a != "--")
                    .cloned()
                    .collect();
                v.push("--".into());
                v.push("--test-threads=1".into());
                v.push("--exact".into());
                v.extend(names);
                v
            }
            RunnerFamily::Vitest | RunnerFamily::Jest => {
                // `-t` matches the full test name (ancestors + title).
                let mut v = cmd.argv.clone();
                let pattern = names
                    .iter()
                    .map(|n| regex_escape(n))
                    .collect::<Vec<_>>()
                    .join("|");
                v.push("-t".into());
                v.push(pattern);
                v
            }
            RunnerFamily::Pytest => {
                let mut v = cmd.argv.clone();
                v.push("-k".into());
                v.push(
                    names
                        .iter()
                        .map(|n| n.split('[').next().unwrap_or(n).to_owned())
                        .collect::<Vec<_>>()
                        .join(" or "),
                );
                v
            }
            RunnerFamily::ConfiguredCommand => cmd.argv.clone(),
        }
    }

    async fn run_command(
        &self,
        cmd: &CheckCommand,
        argv: Vec<String>,
        root: &Path,
        env: &[(String, String)],
        timeout_ms: u64,
        identity: (&str, &str, Stage, &str, &str),
    ) -> TestReport {
        let (report_id, vrun, stage, candidate, envd) = identity;
        // Structured reporter files are written outside the tree so the
        // candidate diff never carries verification artifacts.
        let reporter_path = cmd.reporter_file.as_ref().map(|f| {
            let ext = std::path::Path::new(f)
                .extension()
                .map(|e| e.to_string_lossy().into_owned())
                .unwrap_or_else(|| "out".into());
            std::env::temp_dir().join(format!("modbit-report-{report_id}.{ext}"))
        });
        let argv: Vec<String> = match (&cmd.reporter_file, &reporter_path) {
            (Some(rel), Some(abs)) => argv
                .into_iter()
                .map(|a| a.replace(rel.as_str(), &abs.to_string_lossy()))
                .collect(),
            _ => argv,
        };
        if let Some(p) = &reporter_path {
            let _ = std::fs::remove_file(p);
        }
        let mut raw = self.runner.run(&argv, root, env, timeout_ms).await;
        if let Some(p) = &reporter_path {
            if let Ok(s) = std::fs::read_to_string(p) {
                raw.reporter_file = Some(s);
            }
            let _ = std::fs::remove_file(p);
        }
        let combined = format!("{}\n--- stderr ---\n{}", raw.stdout, raw.stderr);
        let raw_ref = self.sink.put(combined.as_bytes());
        let family = if cmd.family == RunnerFamily::ConfiguredCommand {
            detect(&argv)
        } else {
            cmd.family
        };
        let mut report = TestReport {
            report_id: report_id.to_owned(),
            verification_run_id: vrun.to_owned(),
            stage,
            candidate_revision: candidate.to_owned(),
            environment_digest: envd.to_owned(),
            runner: RunnerInfo {
                family,
                version: String::new(),
                argv: argv.clone(),
                cwd: root.to_string_lossy().into_owned(),
            },
            parser: ParserInfo {
                adapter: String::new(),
                adapter_version: "1".into(),
                confidence: Confidence::Heuristic,
            },
            status: ReportStatus::Unknown,
            counts: Default::default(),
            checks: vec![],
            raw_output_ref: raw_ref,
            exit_code: raw.exit_code,
        };
        parse(family, &raw, &mut report);
        report
    }

    /// Execute a stage. `selection` narrows TARGETED runs to check ids (empty =
    /// all commands); `previous_failures` orders failed-first.
    #[allow(clippy::too_many_arguments)]
    pub async fn run_stage(
        &self,
        plan: &VerificationPlan,
        plan_ref: &str,
        run_id: &str,
        stage: Stage,
        root: &Path,
        candidate_revision: &str,
        env: &[(String, String)],
        toolchain: &[String],
    ) -> (VerificationRun, Vec<Quarantine>) {
        let started = std::time::Instant::now();
        let vrun_id = format!(
            "vr-{}",
            &hex::encode(Sha256::digest(
                format!(
                    "{run_id}|{stage:?}|{candidate_revision}|{}",
                    started.elapsed().as_nanos()
                )
                .as_bytes()
            ))[..16]
        );
        let envd = environment_digest(root, &plan.environment_inputs, toolchain);
        let budget = match stage {
            Stage::Targeted | Stage::Rerun => self.policy.targeted_run_budget_ms,
            _ => self.policy.full_run_budget_ms,
        };
        let mut reports = Vec::new();
        for (i, cmd) in plan.commands.iter().enumerate() {
            let rid = format!("{vrun_id}-{i}");
            let r = self
                .run_command(
                    cmd,
                    cmd.argv.clone(),
                    root,
                    env,
                    budget,
                    (&rid, &vrun_id, stage, candidate_revision, &envd),
                )
                .await;
            reports.push(r);
        }
        // Flake protocol (docs/64 §3): rerun failed checks in isolation.
        let mut quarantines = Vec::new();
        if self.policy.flake_rerun > 0 && stage != Stage::Rerun {
            for (i, cmd) in plan.commands.iter().enumerate() {
                let failed: Vec<CheckResult> = reports[i]
                    .checks
                    .iter()
                    .filter(|c| c.status == CheckStatus::Fail || c.status == CheckStatus::Error)
                    .filter(|c| {
                        c.location.symbol.is_some() && cmd.family != RunnerFamily::ConfiguredCommand
                    })
                    .cloned()
                    .collect();
                if failed.is_empty() {
                    continue;
                }
                let refs: Vec<&CheckResult> = failed.iter().collect();
                let argv = Self::rerun_argv(cmd, &refs);
                let rid = format!("{vrun_id}-rerun-{i}");
                let rerun = self
                    .run_command(
                        cmd,
                        argv,
                        root,
                        env,
                        self.policy.targeted_run_budget_ms,
                        (
                            &rid,
                            &format!("{vrun_id}-rerun"),
                            Stage::Rerun,
                            candidate_revision,
                            &envd,
                        ),
                    )
                    .await;
                for f in &failed {
                    if rerun
                        .checks
                        .iter()
                        .any(|c| c.check_id == f.check_id && c.status == CheckStatus::Pass)
                    {
                        if let Some(c) = reports[i]
                            .checks
                            .iter_mut()
                            .find(|c| c.check_id == f.check_id)
                        {
                            c.status = CheckStatus::Flaky;
                        }
                        quarantines.push(Quarantine {
                            check_id: f.check_id.clone(),
                            first_run_id: vrun_id.clone(),
                            rerun_id: rid.clone(),
                            candidate_revision: candidate_revision.to_owned(),
                        });
                    }
                }
                reports[i].finalize();
                reports.push(rerun);
            }
        }
        let status = overall(&reports);
        let report_refs: Vec<String> = reports
            .iter()
            .map(|r| {
                self.sink
                    .put(serde_json::to_vec(r).unwrap_or_default().as_slice())
            })
            .collect();
        (
            VerificationRun {
                verification_run_id: vrun_id,
                run_id: run_id.to_owned(),
                plan_ref: plan_ref.to_owned(),
                stage,
                candidate_revision: candidate_revision.to_owned(),
                environment_digest: envd,
                status,
                reports,
                report_refs,
                duration_ms: started.elapsed().as_millis() as u64,
            },
            quarantines,
        )
    }
}

fn overall(reports: &[TestReport]) -> ReportStatus {
    let main: Vec<&TestReport> = reports.iter().filter(|r| r.stage != Stage::Rerun).collect();
    if main.iter().any(|r| r.status == ReportStatus::Timeout) {
        ReportStatus::Timeout
    } else if main.iter().any(|r| r.status == ReportStatus::Cancelled) {
        ReportStatus::Cancelled
    } else if main.iter().any(|r| r.status == ReportStatus::Failed) {
        ReportStatus::Failed
    } else if main.iter().any(|r| r.status == ReportStatus::Error) {
        ReportStatus::Error
    } else if main.iter().any(|r| r.status == ReportStatus::Unknown) {
        ReportStatus::Unknown
    } else {
        ReportStatus::Passed
    }
}

fn regex_escape(s: &str) -> String {
    let mut out = String::new();
    for c in s.chars() {
        if "\\^$.|?*+()[]{}".contains(c) {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

/// Regression attribution (docs/64 §1) between a BASELINE and a COMPLETION run.
#[must_use]
pub fn attribute(
    baseline: &VerificationRun,
    completion: &VerificationRun,
    plan: &VerificationPlan,
) -> AttributionReport {
    let base: BTreeMap<String, CheckStatus> = baseline
        .checks()
        .into_iter()
        .map(|c| (c.check_id.clone(), c.status))
        .collect();
    attribute_against(&base, completion, plan)
}

/// Attribution against recorded baseline statuses (as rebuilt from the log).
#[must_use]
pub fn attribute_against(
    base: &BTreeMap<String, CheckStatus>,
    completion: &VerificationRun,
    plan: &VerificationPlan,
) -> AttributionReport {
    let mut checks = Vec::new();
    let mut reasons = Vec::new();
    let mut blocks = false;
    let mut inconclusive = false;
    let failing = |s: CheckStatus| {
        matches!(
            s,
            CheckStatus::Fail | CheckStatus::Error | CheckStatus::Timeout
        )
    };
    for c in completion.checks() {
        let declared = plan
            .declared_changes
            .iter()
            .any(|(id, _)| id == &c.check_id);
        let a = match (base.get(c.check_id.as_str()).copied(), c.status) {
            (_, CheckStatus::Flaky) => {
                inconclusive = true;
                reasons.push(format!(
                    "{} is FLAKY at completion; acceptance INCONCLUSIVE",
                    c.check_id
                ));
                Attribution::Flaky
            }
            (_, CheckStatus::Unknown) => {
                inconclusive = true;
                blocks = true;
                reasons.push(format!(
                    "{} outcome UNKNOWN; INDETERMINATE, never a pass",
                    c.check_id
                ));
                Attribution::NewFailing
            }
            (Some(CheckStatus::Pass), s) if failing(s) => {
                if declared {
                    Attribution::DeclaredChange
                } else {
                    blocks = true;
                    reasons.push(format!(
                        "{} passed at baseline and fails at the candidate revision (REGRESSION)",
                        c.check_id
                    ));
                    Attribution::Regression
                }
            }
            (Some(b), s) if failing(b) && failing(s) => Attribution::KnownFailing,
            (Some(b), CheckStatus::Pass) if failing(b) => Attribution::CollateralFix,
            (None, s) if failing(s) => {
                if declared {
                    Attribution::DeclaredChange
                } else {
                    blocks = true;
                    reasons.push(format!("{} is new and failing", c.check_id));
                    Attribution::NewFailing
                }
            }
            _ => Attribution::Pass,
        };
        checks.push((c.check_id.clone(), a));
    }
    if completion.status == ReportStatus::Unknown || completion.status == ReportStatus::Timeout {
        blocks = true;
        inconclusive = true;
        reasons.push(format!(
            "completion run status {:?} is INDETERMINATE",
            completion.status
        ));
    }
    AttributionReport {
        checks,
        blocks_acceptance: blocks,
        inconclusive,
        reasons,
    }
}
