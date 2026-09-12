//! The skill qualification harness (docs/26 "Skill Qualification Harness",
//! docs/57 WSK-E2E-008/009; REQ-EV-0205, 0206, 0207; M5.7): paired trials of
//! one task set under `no_skill`, `current` and `candidate` arms with the
//! shared paired statistics, a per-model transfer report, and the ablation
//! that asks whether the evolution lab earns its place over simpler manual
//! refinement. Every report states its method and what it does not
//! establish; nothing here promotes — the promotion transaction lives in
//! `modbit_skills::evolution` and reads the qualification's hard gates.

use std::collections::BTreeMap;

use modbit_bench_context_economics::{Metric, PairedReport, Trial, metric_of, paired_report};
use modbit_skills::evolution::{QualificationTrial, SkillQualification};
use serde::{Deserialize, Serialize};

/// The paired benchmark of WSK-E2E-008 (REQ-EV-0206): `current` against
/// `no_skill`, `candidate` against `current`, each with confidence
/// intervals, plus the hard-gate verdict that decides regardless of the
/// aggregate.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ArmsReport {
    /// `current` vs `no_skill` (treatment = current).
    pub current_vs_no_skill: PairedReport,
    /// `candidate` vs `current` (treatment = candidate).
    pub candidate_vs_current: PairedReport,
    /// Verified-completion rate per arm, basis points.
    pub verified_bp: BTreeMap<String, i64>,
    /// The qualification's decision and reasons (hard gates before
    /// economics; an aggregate saving never overrides a failed gate).
    pub decision: String,
    /// Reasons, in gate order.
    pub reasons: Vec<String>,
    /// What this report does not establish.
    pub method: String,
}

fn to_trial(t: &QualificationTrial, variant: &str) -> Trial {
    Trial {
        variant: variant.into(),
        task: format!("{}/{}", t.model, t.task),
        repeat: t.repeat,
        input_tokens: t.input_tokens,
        output_tokens: t.output_tokens,
        cached_input_tokens: 0,
        tool_calls: t.tool_calls,
        normalized_tool_calls: t.tool_calls,
        model_calls: 0,
        agent_ms: t.wall_ms,
        cold_ms: t.wall_ms,
        verified: t.verified,
        tool_schema_bytes: 0,
    }
}

fn pair(
    trials: &[QualificationTrial],
    baseline: &str,
    treatment: &str,
    varied: &str,
) -> PairedReport {
    let mapped: Vec<Trial> = trials
        .iter()
        .filter_map(|t| {
            if t.arm == baseline {
                Some(to_trial(t, "baseline"))
            } else if t.arm == treatment {
                Some(to_trial(t, "treatment"))
            } else {
                None
            }
        })
        .collect();
    paired_report(
        &mapped,
        varied,
        &[
            "task",
            "model",
            "environment",
            "repository revision",
            "seed",
        ],
        "Paired trials of the same tasks on the same models in the same environment; the difference is the skill arm. Input tokens, tool calls and wall time are the harness's own accounting of what each trial reported; a deterministic fixture makes identical pairs and an interval of no width. Whether the arms' agents were live models is a property of the trials, not of this report.",
    )
}

/// Build the arms report from the trials and the qualification decided
/// over them.
#[must_use]
pub fn arms_report(
    trials: &[QualificationTrial],
    qualification: &SkillQualification,
) -> ArmsReport {
    let mut verified_bp = BTreeMap::new();
    for arm in ["no_skill", "current", "candidate"] {
        let ts: Vec<&QualificationTrial> = trials.iter().filter(|t| t.arm == arm).collect();
        if !ts.is_empty() {
            let ok = ts.iter().filter(|t| t.verified).count();
            verified_bp.insert(
                arm.to_owned(),
                i64::try_from(ok * 10_000 / ts.len()).unwrap_or(0),
            );
        }
    }
    ArmsReport {
        current_vs_no_skill: pair(trials, "no_skill", "current", "the current skill instead of no skill"),
        candidate_vs_current: pair(trials, "current", "candidate", "the candidate skill instead of the current one"),
        verified_bp,
        decision: qualification.decision.clone(),
        reasons: qualification.reasons.clone(),
        method: "The decision is the qualification's: hard gates (package and authority, safety, correctness by class and by model) before any economics; no aggregate saving promotes a candidate that fails a gate. Confidence intervals are bootstrap over the pairs; with few pairs they are wide, with a deterministic fixture they have no width.".into(),
    }
}

/// The saving a metric shows in `candidate_vs_current`, when significant.
#[must_use]
pub fn significant_saving(report: &ArmsReport, metric: Metric) -> Option<f64> {
    metric_of(&report.candidate_vs_current, metric)
        .filter(|m| m.significant && m.mean_delta < 0.0)
        .map(|m| m.mean_delta)
}

/// One model family's line of the transfer report (WSK-E2E-009,
/// REQ-EV-0205).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransferLine {
    /// Model family.
    pub model: String,
    /// Verified-completion delta candidate − current, basis points.
    pub delta_bp: i64,
    /// Whether this family regresses.
    pub regresses: bool,
}

/// The cross-model transfer report: results per family, never pooled; a
/// `model_neutral` label needs at least two families and none regressing;
/// poor transfer blocks the label, not a model-specific skill.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransferReport {
    /// Per family.
    pub lines: Vec<TransferLine>,
    /// Whether the skill may carry the model-neutral label.
    pub model_neutral: bool,
    /// Whether a hidden regression (any family) blocks promotion.
    pub hidden_regression: bool,
    /// Why.
    pub finding: String,
}

/// Build the transfer report from a qualification's per-model deltas.
#[must_use]
pub fn transfer_report(qualification: &SkillQualification) -> TransferReport {
    let lines: Vec<TransferLine> = qualification
        .per_model_delta_bp
        .iter()
        .map(|(m, d)| TransferLine {
            model: m.clone(),
            delta_bp: *d,
            regresses: *d < 0,
        })
        .collect();
    let hidden_regression = lines.iter().any(|l| l.regresses);
    let model_neutral = lines.len() >= 2 && !hidden_regression;
    let finding = if lines.len() < 2 {
        "one model family evaluated: the skill may be promoted as model-specific; the model-neutral label needs a second, materially different family".to_owned()
    } else if hidden_regression {
        format!(
            "a family regresses ({}): promotion is blocked on transfer and the model-neutral label is withheld",
            lines
                .iter()
                .filter(|l| l.regresses)
                .map(|l| format!("{} {} bp", l.model, l.delta_bp))
                .collect::<Vec<_>>()
                .join(", ")
        )
    } else {
        format!(
            "no family regresses across {} families: the model-neutral label may be carried",
            lines.len()
        )
    };
    TransferReport {
        lines,
        model_neutral,
        hidden_regression,
        finding,
    }
}

/// The ablation of REQ-EV-0207: does the evolution lab's candidate beat a
/// simpler manual refinement of the same skill on the same tasks? The
/// lab's mechanism is promoted (as a mechanism, by ADR) only on a
/// statistically and practically meaningful lift.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Ablation {
    /// Verified-completion lift evolved − manual, basis points.
    pub lift_bp: i64,
    /// Paired report (treatment = evolved, baseline = manual).
    pub paired: PairedReport,
    /// Whether the lift is statistically meaningful (the verified-rate
    /// difference's interval excludes zero).
    pub statistically_meaningful: bool,
    /// Whether the lift is practically meaningful (at least `practical_bp`).
    pub practically_meaningful: bool,
    /// The threshold used.
    pub practical_bp: i64,
    /// The finding, in words.
    pub finding: String,
}

/// Run the ablation over paired trials whose arms are `manual` and
/// `evolved`.
#[must_use]
pub fn ablation(trials: &[QualificationTrial], practical_bp: i64) -> Ablation {
    let rate = |arm: &str| -> (i64, Vec<f64>) {
        let ts: Vec<&QualificationTrial> = trials.iter().filter(|t| t.arm == arm).collect();
        let vals: Vec<f64> = ts
            .iter()
            .map(|t| if t.verified { 1.0 } else { 0.0 })
            .collect();
        let bp = if ts.is_empty() {
            0
        } else {
            i64::try_from(ts.iter().filter(|t| t.verified).count() * 10_000 / ts.len()).unwrap_or(0)
        };
        (bp, vals)
    };
    let (manual_bp, _) = rate("manual");
    let (evolved_bp, _) = rate("evolved");
    let lift_bp = evolved_bp - manual_bp;
    // Paired verified deltas per (task, repeat): the interval of the mean.
    let mut deltas = Vec::new();
    for e in trials.iter().filter(|t| t.arm == "evolved") {
        if let Some(m) = trials.iter().find(|t| {
            t.arm == "manual" && t.task == e.task && t.repeat == e.repeat && t.model == e.model
        }) {
            deltas.push(f64::from(u8::from(e.verified)) - f64::from(u8::from(m.verified)));
        }
    }
    let (lo, hi) = modbit_bench_context_economics::bootstrap_ci95(&deltas);
    let statistically_meaningful = !deltas.is_empty() && lo > 0.0 && hi > 0.0;
    let practically_meaningful = lift_bp >= practical_bp;
    let paired = pair(
        trials,
        "manual",
        "evolved",
        "the lab's evolved candidate instead of a manual refinement of the same skill",
    );
    let finding = if deltas.is_empty() {
        "no paired trials: nothing is established".to_owned()
    } else if statistically_meaningful && practically_meaningful {
        format!(
            "the evolved candidate verifies {lift_bp} bp more often than the manual refinement (95% interval of the paired delta [{lo:.2}, {hi:.2}] excludes zero): the mechanism earns consideration by ADR"
        )
    } else {
        format!(
            "the evolved candidate's lift over manual refinement is {lift_bp} bp with a paired interval [{lo:.2}, {hi:.2}]: not meaningful enough to justify the lab's complexity; the simpler refinement stands"
        )
    };
    Ablation {
        lift_bp,
        paired,
        statistically_meaningful,
        practically_meaningful,
        practical_bp,
        finding,
    }
}
