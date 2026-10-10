//! The paired projection trial (PX-114, QUAL-PX-114): the same tasks, model
//! and environment under three tool surfaces, driven through the real Core
//! by `modbit-cli`, reported with intervals and a verdict recorded whatever
//! its direction.
//!
//! Arms (the labels are the report's; the Core mode each selects is stated):
//! - `direct`: typed tools only, no program runtime offered (`typed`);
//! - `procedural`: today's default projection, typed tools plus `proc.exec`
//!   and `proc.wait`, the model chooses (`direct`);
//! - `exec_only`: the small procedural surface (`exec_only`).
//!
//! A report from a scripted provider is a harness check, never a result:
//! its verdict is `NOT_EVALUATED`. A live run needs a provider the owner
//! configures (`OPENAI_API_KEY` or a gateway endpoint in the environment);
//! nothing here supplies one.

use std::path::{Path, PathBuf};
use std::process::Command;

use serde::{Deserialize, Serialize};

use crate::suite::{TrialTask, apply_reference, materialize};
use crate::{Trial, bootstrap_ci95, mean};

/// The fewest paired tasks a verdict may rest on.
pub const MIN_PAIRS_FOR_VERDICT: usize = 30;

/// The accuracy margin within which an arm still counts as non-inferior.
pub const NON_INFERIORITY_MARGIN: f64 = 0.05;

/// One tool surface under test.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Arm {
    /// Typed tools only.
    Direct,
    /// The default projection: typed tools and the program runtime.
    Procedural,
    /// The small procedural surface.
    ExecOnly,
}

impl Arm {
    /// Every arm, baseline first.
    pub const ALL: [Arm; 3] = [Arm::Direct, Arm::Procedural, Arm::ExecOnly];

    /// The report label.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Direct => "direct",
            Self::Procedural => "procedural",
            Self::ExecOnly => "exec_only",
        }
    }

    /// The Core projection mode this arm selects (`MODBIT_TOOL_PROJECTION`).
    #[must_use]
    pub const fn core_mode(self) -> &'static str {
        match self {
            Self::Direct => "typed",
            Self::Procedural => "direct",
            Self::ExecOnly => "exec_only",
        }
    }
}

/// One run, whatever happened to it. A run that failed or errored is a run:
/// it is counted, not dropped.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RunRecord {
    /// The arm.
    pub arm: Arm,
    /// The measured trial (`variant` carries the arm label).
    pub trial: Trial,
    /// The task's final state as the Core reported it.
    pub state: String,
    /// Why the run produced no measurement, when it did not.
    pub error: Option<String>,
    /// Schema bytes of each request, from the Core's projection events.
    pub schema_bytes_per_request: Vec<u64>,
}

/// What a run needs from the outside.
#[derive(Clone, Debug)]
pub struct RunConfig {
    /// The `modbit-cli` binary.
    pub cli: PathBuf,
    /// The `modbit-core` binary the CLI spawns.
    pub core: PathBuf,
    /// The model id.
    pub model: String,
    /// Extra environment for the Core (provider base url, catalog, keys).
    pub env: Vec<(String, String)>,
    /// Turn cap per run.
    pub max_turns: u32,
    /// Whether the provider is a live one (a scripted stand-in is not).
    pub live: bool,
}

fn cli(cfg: &RunConfig, arm: Arm, data_dir: &Path, args: &[&str]) -> Result<String, String> {
    let out = Command::new(&cfg.cli)
        .arg("--data-dir")
        .arg(data_dir)
        .args(args)
        .env("MODBIT_CORE_BIN", &cfg.core)
        .env("MODBIT_TOOL_PROJECTION", arm.core_mode())
        .envs(cfg.env.iter().map(|(k, v)| (k, v)))
        .output()
        .map_err(|e| format!("spawning modbit-cli: {e}"))?;
    let text = String::from_utf8_lossy(&out.stdout).into_owned();
    // `task status` exits non-zero for a task that is not done; its output
    // is still the answer.
    if !out.status.success() && !args.starts_with(&["task", "status"]) {
        return Err(format!(
            "modbit-cli {args:?}: {}{}",
            text,
            String::from_utf8_lossy(&out.stderr)
        ));
    }
    Ok(text)
}

fn word_after<'a>(text: &'a str, key: &str) -> Option<&'a str> {
    text.split_whitespace()
        .skip_while(|w| *w != key)
        .nth(1)
        .or_else(|| {
            text.split_whitespace()
                .find_map(|w| w.strip_prefix(&format!("{key}=")))
        })
}

fn field<'a>(text: &'a str, key: &str) -> Option<&'a str> {
    text.split_whitespace()
        .find_map(|w| w.strip_prefix(&format!("{key}=")))
}

fn num(text: &str, key: &str) -> u64 {
    field(text, key).and_then(|v| v.parse().ok()).unwrap_or(0)
}

/// The Core's account of a run that ended without completing: every gate
/// evaluation and every check that did not pass (with its error class, so a
/// generated `ADVERSARIAL_*` check is named), and the last events.
fn why_not_done(cfg: &RunConfig, arm: Arm, data_dir: &Path, session: &str) -> String {
    let Ok(events) = cli(
        cfg,
        arm,
        data_dir,
        &[
            "events",
            "tail",
            "--session",
            session,
            "--count",
            "100000",
            "--json",
        ],
    ) else {
        return "(the event log could not be read)".into();
    };
    let mut lines = Vec::new();
    let mut tail: Vec<String> = Vec::new();
    for line in events.lines() {
        let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        let kind = v["event_type"].as_str().unwrap_or("?");
        tail.push(kind.to_owned());
        match kind {
            "AcceptanceGateEvaluated" => lines.push(format!("{kind}: {}", v["payload"])),
            "VerificationRunRecorded" => {
                let bad: Vec<String> = v["payload"]["checks"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter(|c| {
                        c["status"]
                            .as_str()
                            .is_some_and(|s| s != "PASS" && s != "SKIP")
                    })
                    .map(|c| c.to_string())
                    .collect();
                lines.push(format!("{kind}: non-passing checks {bad:?}"));
            }
            _ => {}
        }
    }
    let n = tail.len().saturating_sub(25);
    lines.push(format!("last events: {:?}", &tail[n..]));
    lines.join("\n")
}

/// Run one task under one arm, through a real Core.
#[must_use]
pub fn run_trial(cfg: &RunConfig, arm: Arm, task: &TrialTask, repeat: u32) -> RunRecord {
    let mut rec = RunRecord {
        arm,
        trial: Trial {
            variant: arm.label().to_owned(),
            task: task.id.clone(),
            repeat,
            input_tokens: 0,
            output_tokens: 0,
            cached_input_tokens: 0,
            tool_calls: 0,
            normalized_tool_calls: 0,
            model_calls: 0,
            agent_ms: 0,
            cold_ms: 0,
            verified: false,
            tool_schema_bytes: 0,
        },
        state: "NotRun".into(),
        error: None,
        schema_bytes_per_request: Vec::new(),
    };
    let started = std::time::Instant::now();
    let result = (|| -> Result<(), String> {
        let repo = tempfile::tempdir().map_err(|e| e.to_string())?;
        let data = tempfile::tempdir().map_err(|e| e.to_string())?;
        materialize(task, repo.path()).map_err(|e| e.to_string())?;
        let session = cli(cfg, arm, data.path(), &["session", "create"])?;
        let session = word_after(&session, "session")
            .ok_or("no session id")?
            .to_owned();
        let root = repo.path().canonicalize().map_err(|e| e.to_string())?;
        let root = root
            .to_string_lossy()
            .trim_start_matches(r"\\?\")
            .to_owned();
        let created = cli(
            cfg,
            arm,
            data.path(),
            &[
                "task",
                "create",
                "--session",
                &session,
                "--workspace",
                &root,
                &task.goal,
            ],
        )?;
        let tid = word_after(&created, "task").ok_or("no task id")?.to_owned();
        let turns = cfg.max_turns.to_string();
        if let Err(e) = cli(
            cfg,
            arm,
            data.path(),
            &[
                "task",
                "run",
                "--session",
                &session,
                "--task",
                &tid,
                "--model",
                &cfg.model,
                "--max-turns",
                &turns,
                "--wait",
            ],
        ) {
            // A run that did not end cleanly is reported with the Core's own
            // account of why: the gate verdicts and every non-passing check.
            return Err(format!(
                "{e}\n{}",
                why_not_done(cfg, arm, data.path(), &session)
            ));
        }
        rec.trial.agent_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
        let status = cli(cfg, arm, data.path(), &["task", "status", "--task", &tid])?;
        rec.state = field(&status, "state").unwrap_or("unknown").to_owned();
        let econ = cli(
            cfg,
            arm,
            data.path(),
            &["task", "economics", "--task", &tid],
        )?;
        rec.trial.verified = field(&econ, "verified") == Some("true");
        rec.trial.model_calls = u32::try_from(num(&econ, "calls")).unwrap_or(u32::MAX);
        rec.trial.input_tokens = num(&econ, "input_tokens");
        rec.trial.output_tokens = num(&econ, "output_tokens");
        rec.trial.cached_input_tokens = num(&econ, "cached_input");
        let events = cli(
            cfg,
            arm,
            data.path(),
            &[
                "events",
                "tail",
                "--session",
                &session,
                "--count",
                "100000",
                "--json",
            ],
        )?;
        let mut calls = std::collections::BTreeSet::new();
        for line in events.lines() {
            let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else {
                continue;
            };
            match v["event_type"].as_str() {
                Some("ToolProjectionSelected") => {
                    rec.schema_bytes_per_request
                        .push(v["payload"]["projected_bytes"].as_u64().unwrap_or(0));
                }
                Some("ToolCallSucceeded" | "ToolCallFailed") => {
                    calls.insert(v["payload"]["tool_call_id"].to_string());
                }
                _ => {}
            }
        }
        rec.trial.tool_schema_bytes = rec.schema_bytes_per_request.iter().sum();
        // The log counts a program's calls too; a protocol call is not work.
        rec.trial.tool_calls = u32::try_from(calls.len()).unwrap_or(u32::MAX);
        rec.trial.normalized_tool_calls = rec.trial.tool_calls;
        drop(repo);
        Ok(())
    })();
    if let Err(e) = result {
        rec.error = Some(e);
    }
    rec
}

/// Run every arm of every task `repeats` times. Arms are interleaved per
/// (task, repeat) so drift in the provider's behaviour hits them alike.
#[must_use]
pub fn run_matrix(cfg: &RunConfig, tasks: &[TrialTask], repeats: u32) -> Vec<RunRecord> {
    let mut out = Vec::new();
    for task in tasks {
        for repeat in 0..repeats {
            for arm in Arm::ALL {
                out.push(run_trial(cfg, arm, task, repeat));
            }
        }
    }
    out
}

/// A float that JSON wrote as `null` because it was NaN (no sample) reads back
/// as NaN, so a retained report can be read again and re-scored.
pub(crate) fn nan_if_null<'de, D: serde::Deserializer<'de>>(d: D) -> Result<f64, D::Error> {
    Ok(Option::<f64>::deserialize(d)?.unwrap_or(f64::NAN))
}

fn pair_nan_if_null<'de, D: serde::Deserializer<'de>>(d: D) -> Result<(f64, f64), D::Error> {
    let (a, b) = <(Option<f64>, Option<f64>)>::deserialize(d)?;
    Ok((a.unwrap_or(f64::NAN), b.unwrap_or(f64::NAN)))
}

/// A proportion with its 95% Wilson interval.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Proportion {
    /// Successes.
    pub k: usize,
    /// Trials.
    pub n: usize,
    /// k / n.
    #[serde(deserialize_with = "nan_if_null")]
    pub p: f64,
    /// Lower bound.
    #[serde(deserialize_with = "nan_if_null")]
    pub lo: f64,
    /// Upper bound.
    #[serde(deserialize_with = "nan_if_null")]
    pub hi: f64,
}

/// The Wilson score interval of `k` successes in `n`.
#[must_use]
pub fn wilson(k: usize, n: usize) -> Proportion {
    if n == 0 {
        return Proportion {
            k,
            n,
            p: f64::NAN,
            lo: f64::NAN,
            hi: f64::NAN,
        };
    }
    let (k_f, n_f) = (k as f64, n as f64);
    let p = k_f / n_f;
    let z = 1.96_f64;
    let denom = 1.0 + z * z / n_f;
    let centre = (p + z * z / (2.0 * n_f)) / denom;
    let half = z * ((p * (1.0 - p) + z * z / (4.0 * n_f)) / n_f).sqrt() / denom;
    // The interval's analytic endpoints at the boundaries are exactly 0 and 1
    // (centre + half = 1 when k = n, centre - half = 0 when k = 0). The float
    // form of that identity can land a ulp inside 1 or 0, so the boundary is
    // returned exactly instead of being compared with a tolerance later.
    Proportion {
        k,
        n,
        p,
        lo: if k == 0 {
            0.0
        } else {
            (centre - half).max(0.0)
        },
        hi: if k == n {
            1.0
        } else {
            (centre + half).min(1.0)
        },
    }
}

/// A mean with a bootstrap interval.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct MeanCi {
    /// Mean.
    #[serde(deserialize_with = "nan_if_null")]
    pub mean: f64,
    /// 95% interval (NaN under two samples).
    #[serde(deserialize_with = "pair_nan_if_null")]
    pub ci95: (f64, f64),
}

pub(crate) fn mean_ci(values: &[f64]) -> MeanCi {
    MeanCi {
        mean: mean(values),
        ci95: bootstrap_ci95(values),
    }
}

/// One arm over all its runs.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ArmSummary {
    /// The arm.
    pub arm: Arm,
    /// Runs made, failed and errored ones included.
    pub runs: usize,
    /// Runs that reached a verified outcome (accuracy).
    pub accuracy: Proportion,
    /// Runs that errored before a measurement (counted as not verified).
    pub errored: usize,
    /// Input plus output tokens per run.
    pub total_tokens: MeanCi,
    /// Tool calls per run.
    pub tool_calls: MeanCi,
    /// Model calls per run.
    pub model_calls: MeanCi,
    /// Tool-schema bytes per run.
    pub schema_bytes: MeanCi,
    /// Ids of runs that did not verify, `task#repeat`.
    pub failed_runs: Vec<String>,
}

/// The verdict of one arm against `direct`'s partner arm.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Direction {
    /// Non-inferior accuracy and fewer tokens, both with intervals clear.
    Better,
    /// Worse accuracy, or more tokens without better accuracy.
    Worse,
    /// The intervals do not decide it, or too few pairs.
    Inconclusive,
    /// A scripted provider: a harness check, nothing is decided.
    NotEvaluated,
}

/// A paired comparison.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Comparison {
    /// The arm compared.
    pub arm: Arm,
    /// What it is compared against.
    pub against: Arm,
    /// Complete pairs.
    pub pairs: usize,
    /// Mean of (accuracy of arm - accuracy of against) per pair.
    pub accuracy_delta: MeanCi,
    /// Total tokens, arm minus against.
    pub tokens_delta: MeanCi,
    /// Tool calls, arm minus against.
    pub tool_calls_delta: MeanCi,
    /// Schema bytes, arm minus against.
    pub schema_bytes_delta: MeanCi,
    /// The direction under the stated rule.
    pub direction: Direction,
}

/// The rule, stated before any run so it cannot be fitted to a result.
pub const VERDICT_RULE: &str = "An arm is BETTER than the arm it is compared with when, over at least 30 paired tasks, the 95% bootstrap interval of the accuracy difference has a lower bound of at least -0.05 (non-inferior) and the interval of the total-token difference lies entirely below zero; WORSE when the accuracy interval lies entirely below zero, or the token interval lies entirely above zero while the accuracy interval does not lie above zero; INCONCLUSIVE otherwise or with fewer pairs. A scripted provider decides nothing (NOT_EVALUATED). The recommendation follows exec_only against procedural (today's default): the default changes only on BETTER, and only by the owner's decision.";

/// The whole report.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TrialReport {
    /// Whether the provider was live.
    pub live: bool,
    /// The model.
    pub model: String,
    /// Tasks.
    pub tasks: usize,
    /// Repeats per task and arm.
    pub repeats: usize,
    /// Per arm, every arm present.
    pub arms: Vec<ArmSummary>,
    /// Each non-baseline arm against the one before it in `Arm::ALL`, and
    /// exec_only against procedural.
    pub comparisons: Vec<Comparison>,
    /// The rule the directions follow.
    pub rule: String,
    /// What the owner decides, and on what.
    pub recommendation: String,
    /// What the numbers do not establish.
    pub method: String,
}

fn pair_values(runs: &[RunRecord], a: Arm, b: Arm, f: impl Fn(&RunRecord) -> f64) -> Vec<f64> {
    let mut out = Vec::new();
    for ra in runs.iter().filter(|r| r.arm == a) {
        if let Some(rb) = runs.iter().find(|r| {
            r.arm == b && r.trial.task == ra.trial.task && r.trial.repeat == ra.trial.repeat
        }) {
            out.push(f(ra) - f(rb));
        }
    }
    out
}

fn tokens(r: &RunRecord) -> f64 {
    (r.trial.input_tokens + r.trial.output_tokens) as f64
}

fn compare(runs: &[RunRecord], arm: Arm, against: Arm, live: bool) -> Comparison {
    let acc = pair_values(runs, arm, against, |r| {
        f64::from(u8::from(r.trial.verified))
    });
    let tok = pair_values(runs, arm, against, tokens);
    let calls = pair_values(runs, arm, against, |r| f64::from(r.trial.tool_calls));
    let bytes = pair_values(runs, arm, against, |r| r.trial.tool_schema_bytes as f64);
    let (acc_ci, tok_ci) = (mean_ci(&acc), mean_ci(&tok));
    let direction = if !live {
        Direction::NotEvaluated
    } else if acc.len() < MIN_PAIRS_FOR_VERDICT || !acc_ci.ci95.0.is_finite() {
        Direction::Inconclusive
    } else if acc_ci.ci95.0 >= -NON_INFERIORITY_MARGIN && tok_ci.ci95.1 < 0.0 {
        Direction::Better
    } else if acc_ci.ci95.1 < 0.0 || (tok_ci.ci95.0 > 0.0 && acc_ci.ci95.0 <= 0.0) {
        Direction::Worse
    } else {
        Direction::Inconclusive
    };
    Comparison {
        arm,
        against,
        pairs: acc.len(),
        accuracy_delta: acc_ci,
        tokens_delta: tok_ci,
        tool_calls_delta: mean_ci(&calls),
        schema_bytes_delta: mean_ci(&bytes),
        direction,
    }
}

/// Summarise every run into the report. Every arm appears, and every run is
/// counted, the failed ones named.
#[must_use]
pub fn report(
    runs: &[RunRecord],
    model: &str,
    tasks: usize,
    repeats: usize,
    live: bool,
) -> TrialReport {
    let arms: Vec<ArmSummary> = Arm::ALL
        .into_iter()
        .map(|arm| {
            let mine: Vec<&RunRecord> = runs.iter().filter(|r| r.arm == arm).collect();
            let vals =
                |f: &dyn Fn(&RunRecord) -> f64| -> Vec<f64> { mine.iter().map(|r| f(r)).collect() };
            ArmSummary {
                arm,
                runs: mine.len(),
                accuracy: wilson(mine.iter().filter(|r| r.trial.verified).count(), mine.len()),
                errored: mine.iter().filter(|r| r.error.is_some()).count(),
                total_tokens: mean_ci(&vals(&tokens)),
                tool_calls: mean_ci(&vals(&|r| f64::from(r.trial.tool_calls))),
                model_calls: mean_ci(&vals(&|r| f64::from(r.trial.model_calls))),
                schema_bytes: mean_ci(&vals(&|r| r.trial.tool_schema_bytes as f64)),
                failed_runs: mine
                    .iter()
                    .filter(|r| !r.trial.verified)
                    .map(|r| format!("{}#{}", r.trial.task, r.trial.repeat))
                    .collect(),
            }
        })
        .collect();
    let comparisons = vec![
        compare(runs, Arm::Procedural, Arm::Direct, live),
        compare(runs, Arm::ExecOnly, Arm::Direct, live),
        compare(runs, Arm::ExecOnly, Arm::Procedural, live),
    ];
    let main = comparisons[2].direction;
    let recommendation = match (live, main) {
        (false, _) => "NOT_EVALUATED: this report came from a scripted provider and decides nothing; the default stays direct until a live run decides it.".to_owned(),
        (true, Direction::Better) => "exec_only is BETTER than today's default under the stated rule; the owner may decide to make it the default for this model. Nothing changes without that decision.".to_owned(),
        (true, Direction::Worse) => "exec_only is WORSE than today's default under the stated rule; the default stays as it is.".to_owned(),
        (true, _) => "INCONCLUSIVE: the default stays as it is; more tasks or repeats would be needed.".to_owned(),
    };
    TrialReport {
        live,
        model: model.to_owned(),
        tasks,
        repeats,
        arms,
        comparisons,
        rule: VERDICT_RULE.to_owned(),
        recommendation,
        method: format!(
            "{tasks} tasks x {repeats} repeats x 3 arms, each run a fresh repository and a fresh Core under the arm's projection mode, driven through modbit-cli; tokens, model calls and verification from the Core's economics view, tool calls and schema bytes from the event log. Arms are interleaved per task. Accuracy is the share of runs whose mandatory check passed at completion (Wilson interval); the paired differences use a deterministic bootstrap. {}",
            if live {
                "The provider was live: the numbers are the model's, on this suite, and say nothing about other models or tasks."
            } else {
                "The provider was a scripted stand-in: this checks the harness and is not a live result."
            }
        ),
    }
}

/// Whether a report is complete: every arm present with every run counted
/// (failed and errored ones included) and the rule and recommendation stated.
///
/// # Errors
/// What is missing.
pub fn validate(report: &TrialReport, tasks: usize, repeats: usize) -> Result<(), String> {
    for arm in Arm::ALL {
        let Some(s) = report.arms.iter().find(|s| s.arm == arm) else {
            return Err(format!("arm {} is missing", arm.label()));
        };
        if s.runs != tasks * repeats {
            return Err(format!(
                "arm {} reports {} runs, {} were made",
                arm.label(),
                s.runs,
                tasks * repeats
            ));
        }
        if s.accuracy.n != s.runs || s.failed_runs.len() != s.runs - s.accuracy.k {
            return Err(format!("arm {} drops a failed run", arm.label()));
        }
    }
    if report.rule.is_empty() || report.recommendation.is_empty() {
        return Err("no stated rule or recommendation".into());
    }
    if !report.live
        && report
            .comparisons
            .iter()
            .any(|c| c.direction != Direction::NotEvaluated)
    {
        return Err("a scripted run carries a verdict".into());
    }
    Ok(())
}

/// Self-check of a suite: each task's check fails at the start and passes
/// with the reference applied. Returns the ids that are broken.
#[must_use]
pub fn broken_tasks(tasks: &[TrialTask]) -> Vec<String> {
    let mut broken = Vec::new();
    for t in tasks {
        let ok = (|| -> std::io::Result<bool> {
            let dir = tempfile::tempdir()?;
            materialize(t, dir.path())?;
            let before = crate::suite::check_passes(dir.path())?;
            apply_reference(t, dir.path())?;
            let after = crate::suite::check_passes(dir.path())?;
            Ok(!before && after)
        })();
        if !ok.unwrap_or(false) {
            broken.push(t.id.clone());
        }
    }
    broken
}
