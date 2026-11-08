//! The four-axis agent benchmark (PX-184, QUAL-PX-184): tool use, memory,
//! skill use and sub-agent coordination, at least ten tasks on each axis,
//! each tagged by axis and verified by its own check.
//!
//! The report scores each axis (verified success with a 95% Wilson interval)
//! and states the output-token cost per thousand tasks, computed only from the
//! usage the Core recorded for each trial: the sum of `GetTaskEconomics` over
//! the trial's task and every child it admitted, cross-checked against the
//! `ModelUsageRecorded` events of the trial's log. A trial without recorded
//! usage, or whose two records disagree, refuses the report.
//!
//! A re-run is compared with a retained baseline axis by axis; an axis fails
//! its gate when its verified-success rate falls more than the suite's
//! `max_drop` below the baseline's. A gate that cannot fail (the baseline's
//! rate is at or below `max_drop`, or `max_drop` is not below one) refuses the
//! comparison.
//!
//! The same report and gate serve a suite that declares its own axes (PX-179,
//! the GUI regression suite: `gui_element` and `gui_input`). Such a suite may
//! also set the per-task gate: the report then keeps each task's verified
//! trials, and the comparison names every task that verified in the baseline
//! and no longer does (a regression, which fails the gate whatever the axis
//! rates) and every task that verifies now and did not before.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::bundle::{Bundle, Rate, TrialOutcome};
use crate::events::Event;
use crate::suite::Suite;

/// The four axes, in report order.
pub const AXES: [&str; 4] = ["tool_use", "memory", "skill_use", "subagent"];

/// The fewest tasks an axis may have (QUAL-PX-184).
pub const MIN_TASKS_PER_AXIS: u32 = 10;

/// Schema of `axes.json`.
pub const REPORT_SCHEMA: &str = "modbit-four-axis-report/1";

/// Schema of `comparison.json`.
pub const COMPARISON_SCHEMA: &str = "modbit-four-axis-comparison/1";

/// The four-axis part of a suite's protocol.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AxesProtocol {
    /// The axes the suite's tasks are tagged by, in report order (PX-179).
    /// Absent: the four axes of [`AXES`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub names: Option<Vec<String>>,
    /// The fewest tasks each axis must have; never below [`MIN_TASKS_PER_AXIS`].
    pub min_tasks_per_axis: u32,
    /// The per-axis regression gate.
    pub gate: Gate,
}

impl AxesProtocol {
    /// The axes in report order: the suite's own, or [`AXES`].
    #[must_use]
    pub fn axis_names(&self) -> Vec<String> {
        self.names.clone().unwrap_or_else(default_axes)
    }
}

/// The four axes of PX-184 as owned names.
#[must_use]
pub fn default_axes() -> Vec<String> {
    AXES.iter().map(|a| (*a).to_owned()).collect()
}

/// The per-axis regression gate.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Gate {
    /// An axis fails when its verified-success rate is more than this below
    /// the baseline's (absolute, 0..1).
    pub max_drop: f64,
    /// PX-179: a task that verified in every baseline trial and does not in
    /// every current one fails the gate by itself, named, whatever its
    /// axis's rate. Absent: the per-axis gate only.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub per_task: bool,
}

impl Gate {
    /// The gate is well formed: a finite drop in `[0, 1)`.
    ///
    /// # Errors
    /// Otherwise.
    pub fn check(&self) -> Result<(), String> {
        if !self.max_drop.is_finite() || self.max_drop < 0.0 || self.max_drop >= 1.0 {
            return Err(format!(
                "gate max_drop {} cannot fail: it must be finite and in [0, 1)",
                self.max_drop
            ));
        }
        Ok(())
    }

    /// The lowest rate that passes against `baseline`.
    #[must_use]
    pub fn floor(&self, baseline: f64) -> f64 {
        baseline - self.max_drop
    }

    /// Whether some current rate could fail against `baseline`: a rate of
    /// zero is below the floor.
    #[must_use]
    pub fn can_fail(&self, baseline: f64) -> bool {
        self.check().is_ok() && self.floor(baseline) > 0.0
    }

    /// Whether `current` passes against `baseline`.
    #[must_use]
    pub fn passes(&self, baseline: f64, current: f64) -> bool {
        current + 1e-9 >= self.floor(baseline)
    }
}

/// Validate a four-axis suite: every task tagged by a known axis and carrying
/// its own verifier (a unique id and checks no other task shares), and every
/// axis with at least `max(min_tasks_per_axis, 10)` tasks.
///
/// # Errors
/// What is wrong, naming the task or axis.
pub fn check_suite(suite: &Suite) -> Result<(), String> {
    let Some(p) = &suite.protocol.axes else {
        return Err("suite declares no four-axis protocol".into());
    };
    p.gate.check()?;
    let min = p.min_tasks_per_axis.max(MIN_TASKS_PER_AXIS);
    let names = p.axis_names();
    if names.is_empty() {
        return Err("suite declares an empty list of axes".into());
    }
    let mut per_axis: BTreeMap<&str, u32> = BTreeMap::new();
    for a in &names {
        if a.trim().is_empty() || per_axis.insert(a.as_str(), 0).is_some() {
            return Err(format!("axis {a:?} is empty or declared twice"));
        }
    }
    let mut ids = BTreeSet::new();
    let mut prints = BTreeMap::new();
    for t in &suite.tasks {
        let axis = t
            .axis
            .as_deref()
            .ok_or_else(|| format!("{}: task is not tagged by axis", t.id))?;
        let n = per_axis
            .get_mut(axis)
            .ok_or_else(|| format!("{}: axis {axis:?} is not one of {names:?}", t.id))?;
        *n += 1;
        let v = t
            .verifier
            .as_ref()
            .ok_or_else(|| format!("{}: task has no verifier of its own", t.id))?;
        v.check().map_err(|e| format!("{}: {e}", t.id))?;
        if !ids.insert(v.id.as_str()) {
            return Err(format!("{}: verifier id {} is shared", t.id, v.id));
        }
        if let Some(other) = prints.insert(v.fingerprint(), t.id.as_str()) {
            return Err(format!(
                "{}: its verifier checks exactly what {other}'s does; each task needs its own",
                t.id
            ));
        }
    }
    for (axis, n) in per_axis {
        if n < min {
            return Err(format!(
                "axis {axis} has {n} tasks; at least {min} are required"
            ));
        }
    }
    Ok(())
}

/// Usage the Core recorded for one trial: the trial's task and its children.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct TrialUsage {
    /// Task ids whose `GetTaskEconomics` were summed (the trial's first).
    pub tasks: Vec<String>,
    /// Model calls, from economics.
    pub model_calls: u32,
    /// Input tokens, from economics.
    pub input_tokens: u64,
    /// Output tokens, from economics.
    pub output_tokens: u64,
    /// Cost at catalog list prices when every task was priced.
    pub cost_usd: Option<f64>,
    /// `ModelUsageRecorded` events in the trial's log.
    pub recorded_model_calls: u32,
    /// Output tokens of those events.
    pub recorded_output_tokens: u64,
}

/// `ModelUsageRecorded` events of a log: (calls, output tokens).
#[must_use]
pub fn recorded_usage(events: &[Event]) -> (u32, u64) {
    let mut calls = 0u32;
    let mut out = 0u64;
    for e in events
        .iter()
        .filter(|e| e.event_type == "ModelUsageRecorded")
    {
        calls += 1;
        out += e.payload["output_tokens"].as_u64().unwrap_or(0);
    }
    (calls, out)
}

/// One axis in the report.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct AxisScore {
    /// Task ids on the axis.
    pub tasks: Vec<String>,
    /// Verified success over the axis's trials, with its 95% Wilson interval.
    pub verified: Rate,
    /// Output tokens the axis's trials used.
    pub output_tokens: u64,
    /// Output tokens per thousand tasks on this axis.
    pub output_tokens_per_thousand_tasks: f64,
    /// USD per thousand tasks on this axis, when every trial was priced.
    pub cost_usd_per_thousand_tasks: Option<f64>,
    /// Tasks whose trials failed, with the failing checks.
    pub failures: Vec<String>,
    /// PX-179 (per-task gate only): each task's verified trials and trials.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub task_results: BTreeMap<String, (u32, u32)>,
}

/// The cost per thousand tasks over the whole run.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct CostPerThousand {
    /// Trials (a trial is one run of one task).
    pub trials: u32,
    /// Model calls in all trials.
    pub model_calls: u64,
    /// Output tokens in all trials.
    pub output_tokens: u64,
    /// `output_tokens * 1000 / trials`.
    pub output_tokens_per_thousand_tasks: f64,
    /// USD per thousand tasks at catalog list prices, when every trial was priced.
    pub cost_usd_per_thousand_tasks: Option<f64>,
    /// Where the figures come from.
    pub source: String,
}

/// `axes.json`.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AxisReport {
    /// [`REPORT_SCHEMA`].
    pub schema: String,
    /// Suite id.
    pub suite_id: String,
    /// Suite version.
    pub suite_version: String,
    /// Task-list digest of the suite.
    pub task_list_digest: String,
    /// SHA-256 of the bundle file the report was computed from.
    pub bundle_digest: String,
    /// Endpoint and model of the run.
    pub endpoint: String,
    /// Model.
    pub model: String,
    /// Configuration (`direct`).
    pub configuration: String,
    /// Each axis, in [`AXES`] order.
    pub axes: BTreeMap<String, AxisScore>,
    /// Cost per thousand tasks.
    pub cost: CostPerThousand,
    /// The gate this suite declares.
    pub gate: Gate,
}

/// Why a report or comparison is refused (QUAL-PX-184 negative proof).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum ReportRefused {
    /// The bundle's trial carries no axis.
    Untagged(String),
    /// An axis the suite does not declare.
    UnknownAxis(String),
    /// A task's trial carries no verifier outcome.
    NoVerifier(String),
    /// An axis with too few tasks.
    TooFewTasks {
        /// Axis.
        axis: String,
        /// Tasks found.
        found: u32,
        /// Required.
        required: u32,
    },
    /// A trial without usage the Core recorded.
    NoRecordedUsage(String),
    /// A trial whose economics and event log disagree.
    UsageMismatch(String),
    /// A report whose figures are not the ones its bundle gives.
    NotDerived(String),
    /// Two reports of different suites.
    DifferentSuite(String),
    /// A gate that cannot fail.
    GateCannotFail(String),
}

impl std::fmt::Display for ReportRefused {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Untagged(t) => write!(f, "{t}: trial is not tagged by axis"),
            Self::UnknownAxis(a) => write!(f, "axis {a:?} is not one of the suite's axes"),
            Self::NoVerifier(t) => write!(f, "{t}: trial has no verifier outcome of its own"),
            Self::TooFewTasks {
                axis,
                found,
                required,
            } => write!(f, "axis {axis} has {found} tasks; {required} required"),
            Self::NoRecordedUsage(t) => write!(f, "{t}: no usage recorded by the Core"),
            Self::UsageMismatch(t) => write!(
                f,
                "{t}: task economics and the ModelUsageRecorded events disagree"
            ),
            Self::NotDerived(w) => write!(f, "{w} is not derived from the bundle's recorded usage"),
            Self::DifferentSuite(w) => write!(f, "different suites: {w}"),
            Self::GateCannotFail(w) => write!(f, "gate cannot fail: {w}"),
        }
    }
}

fn label(t: &TrialOutcome) -> String {
    format!("{} trial {}", t.task, t.trial)
}

fn per_thousand(total: f64, trials: u32) -> f64 {
    if trials == 0 {
        0.0
    } else {
        total * 1000.0 / f64::from(trials)
    }
}

/// Compute the report from a bundle, refusing what QUAL-PX-184 names.
///
/// # Errors
/// See [`ReportRefused`].
pub fn report(
    bundle: &Bundle,
    bundle_digest: &str,
    gate: Gate,
    min_tasks_per_axis: u32,
) -> Result<AxisReport, ReportRefused> {
    report_axes(
        bundle,
        bundle_digest,
        gate,
        min_tasks_per_axis,
        &default_axes(),
    )
}

/// [`report`] over the axes a suite declares (PX-179).
///
/// # Errors
/// See [`ReportRefused`].
#[allow(clippy::too_many_lines)]
pub fn report_axes(
    bundle: &Bundle,
    bundle_digest: &str,
    gate: Gate,
    min_tasks_per_axis: u32,
    names: &[String],
) -> Result<AxisReport, ReportRefused> {
    let required = min_tasks_per_axis.max(MIN_TASKS_PER_AXIS);
    let mut by_axis: BTreeMap<&str, Vec<&TrialOutcome>> =
        names.iter().map(|a| (a.as_str(), Vec::new())).collect();
    for t in &bundle.trials {
        let axis = t
            .axis
            .as_deref()
            .ok_or_else(|| ReportRefused::Untagged(label(t)))?;
        by_axis
            .get_mut(axis)
            .ok_or_else(|| ReportRefused::UnknownAxis(axis.to_owned()))?
            .push(t);
        if t.verifier.is_none() {
            return Err(ReportRefused::NoVerifier(label(t)));
        }
        let u = t
            .usage
            .as_ref()
            .ok_or_else(|| ReportRefused::NoRecordedUsage(label(t)))?;
        if u.model_calls == 0 || u.tasks.is_empty() {
            return Err(ReportRefused::NoRecordedUsage(label(t)));
        }
        if u.model_calls != u.recorded_model_calls || u.output_tokens != u.recorded_output_tokens {
            return Err(ReportRefused::UsageMismatch(label(t)));
        }
    }
    let mut axes = BTreeMap::new();
    let mut cost = CostPerThousand {
        source: "per trial: GetTaskEconomics of the task and every child it admitted, equal to the ModelUsageRecorded events of the trial's log".into(),
        ..CostPerThousand::default()
    };
    let mut usd_total = Some(0.0f64);
    for (axis, trials) in &by_axis {
        let tasks: BTreeSet<&str> = trials.iter().map(|t| t.task.as_str()).collect();
        let found = u32::try_from(tasks.len()).unwrap_or(u32::MAX);
        if found < required {
            return Err(ReportRefused::TooFewTasks {
                axis: (*axis).to_owned(),
                found,
                required,
            });
        }
        let n = u32::try_from(trials.len()).unwrap_or(u32::MAX);
        let ok =
            u32::try_from(trials.iter().filter(|t| t.verified_success).count()).unwrap_or(u32::MAX);
        let mut out = 0u64;
        let mut usd = Some(0.0f64);
        for u in trials.iter().filter_map(|t| t.usage.as_ref()) {
            out += u.output_tokens;
            cost.model_calls += u64::from(u.model_calls);
            usd = match (usd, u.cost_usd) {
                (Some(a), Some(b)) => Some(a + b),
                _ => None,
            };
        }
        let failures = trials
            .iter()
            .filter(|t| !t.verified_success)
            .map(|t| {
                let failing: Vec<&str> = t
                    .verifier
                    .as_ref()
                    .map(|v| {
                        v.checks
                            .iter()
                            .filter(|c| !c.passed)
                            .map(|c| c.check.as_str())
                            .collect()
                    })
                    .unwrap_or_default();
                format!(
                    "{} (state {}{}{})",
                    label(t),
                    t.state,
                    if t.core_verified {
                        ""
                    } else {
                        ", not Core-verified"
                    },
                    if failing.is_empty() {
                        String::new()
                    } else {
                        format!("; failing: {}", failing.join(", "))
                    }
                )
            })
            .collect();
        let mut task_results: BTreeMap<String, (u32, u32)> = BTreeMap::new();
        if gate.per_task {
            for t in trials {
                let e = task_results.entry(t.task.clone()).or_default();
                e.0 += u32::from(t.verified_success);
                e.1 += 1;
            }
        }
        cost.trials += n;
        cost.output_tokens += out;
        usd_total = match (usd_total, usd) {
            (Some(a), Some(b)) => Some(a + b),
            _ => None,
        };
        #[allow(clippy::cast_precision_loss)]
        axes.insert(
            (*axis).to_owned(),
            AxisScore {
                tasks: tasks.iter().map(|s| (*s).to_owned()).collect(),
                verified: Rate::of(ok, n),
                output_tokens: out,
                output_tokens_per_thousand_tasks: per_thousand(out as f64, n),
                cost_usd_per_thousand_tasks: usd.map(|c| per_thousand(c, n)),
                failures,
                task_results,
            },
        );
    }
    #[allow(clippy::cast_precision_loss)]
    {
        cost.output_tokens_per_thousand_tasks =
            per_thousand(cost.output_tokens as f64, cost.trials);
    }
    cost.cost_usd_per_thousand_tasks = usd_total.map(|c| per_thousand(c, cost.trials));
    Ok(AxisReport {
        schema: REPORT_SCHEMA.into(),
        suite_id: bundle.suite_id.clone(),
        suite_version: bundle.suite_version.clone(),
        task_list_digest: bundle.task_list_digest.clone(),
        bundle_digest: bundle_digest.into(),
        endpoint: bundle.protocol.endpoint.clone(),
        model: bundle.protocol.model.clone(),
        configuration: bundle.protocol.configuration.clone(),
        axes,
        cost,
        gate,
    })
}

impl AxisReport {
    /// Check this report against the bundle it names: every rate and cost
    /// figure must be the one the bundle's recorded usage gives.
    ///
    /// # Errors
    /// [`ReportRefused::NotDerived`], or anything [`report`] refuses.
    pub fn verify_against(
        &self,
        bundle: &Bundle,
        bundle_digest: &str,
    ) -> Result<(), ReportRefused> {
        if self.bundle_digest != bundle_digest {
            return Err(ReportRefused::NotDerived(format!(
                "a report of bundle {} (this bundle is {bundle_digest})",
                self.bundle_digest
            )));
        }
        let names = self.axis_order();
        let fresh = report_axes(bundle, bundle_digest, self.gate, MIN_TASKS_PER_AXIS, &names)?;
        let close = |a: f64, b: f64| (a - b).abs() <= 1e-6 * a.abs().max(1.0);
        let same_usd = |a: Option<f64>, b: Option<f64>| match (a, b) {
            (None, None) => true,
            (Some(a), Some(b)) => close(a, b),
            _ => false,
        };
        if self.cost.output_tokens != fresh.cost.output_tokens
            || self.cost.trials != fresh.cost.trials
            || !close(
                self.cost.output_tokens_per_thousand_tasks,
                fresh.cost.output_tokens_per_thousand_tasks,
            )
            || !same_usd(
                self.cost.cost_usd_per_thousand_tasks,
                fresh.cost.cost_usd_per_thousand_tasks,
            )
        {
            return Err(ReportRefused::NotDerived(
                "the cost per thousand tasks".into(),
            ));
        }
        for (axis, s) in &fresh.axes {
            let mine = self
                .axes
                .get(axis)
                .ok_or_else(|| ReportRefused::NotDerived(format!("axis {axis}")))?;
            if mine.verified.successes != s.verified.successes
                || mine.verified.n != s.verified.n
                || mine.output_tokens != s.output_tokens
                || mine.task_results != s.task_results
            {
                return Err(ReportRefused::NotDerived(format!("axis {axis}")));
            }
        }
        Ok(())
    }

    /// The report's axes in report order: [`AXES`] order when they are the
    /// four, otherwise the order of their names.
    #[must_use]
    pub fn axis_order(&self) -> Vec<String> {
        let keys: Vec<String> = self.axes.keys().cloned().collect();
        let mut four = default_axes();
        four.retain(|a| self.axes.contains_key(a));
        if four.len() == keys.len() { four } else { keys }
    }
}

/// One axis compared with the baseline.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AxisDelta {
    /// Axis.
    pub axis: String,
    /// The baseline's verified success.
    pub baseline: Rate,
    /// This run's verified success.
    pub current: Rate,
    /// `current.rate - baseline.rate`.
    pub delta: f64,
    /// The lowest rate that passes.
    pub floor: f64,
    /// The gate's verdict.
    pub passed: bool,
    /// Output tokens per thousand tasks: baseline, current, difference.
    pub output_tokens_per_thousand_tasks: (f64, f64, f64),
}

/// `comparison.json`: a re-run against the retained baseline.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Comparison {
    /// [`COMPARISON_SCHEMA`].
    pub schema: String,
    /// Baseline bundle digest.
    pub baseline_digest: String,
    /// This run's bundle digest.
    pub current_digest: String,
    /// The gate applied.
    pub gate: Gate,
    /// Each axis.
    pub axes: Vec<AxisDelta>,
    /// Output tokens per thousand tasks: baseline, current, difference.
    pub output_tokens_per_thousand_tasks: (f64, f64, f64),
    /// USD per thousand tasks: baseline, current (when priced).
    pub cost_usd_per_thousand_tasks: (Option<f64>, Option<f64>),
    /// Axes whose gate failed.
    pub failed_axes: Vec<String>,
    /// PX-179 (per-task gate): tasks that verified in every baseline trial
    /// and not in every current one. Any one fails the gate.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub regressed_tasks: Vec<String>,
    /// PX-179 (per-task gate): tasks that verify now in every trial and did
    /// not in the baseline. Reported, never a failure.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub recovered_tasks: Vec<String>,
    /// Every gate passed.
    pub passed: bool,
}

/// Compare a run with the retained baseline, axis by axis.
///
/// # Errors
/// Reports of different suites or gates, or a gate that cannot fail.
pub fn compare(baseline: &AxisReport, current: &AxisReport) -> Result<Comparison, ReportRefused> {
    if baseline.suite_id != current.suite_id
        || baseline.suite_version != current.suite_version
        || baseline.task_list_digest != current.task_list_digest
    {
        return Err(ReportRefused::DifferentSuite(format!(
            "{} v{} ({}) vs {} v{} ({})",
            baseline.suite_id,
            baseline.suite_version,
            baseline.task_list_digest,
            current.suite_id,
            current.suite_version,
            current.task_list_digest
        )));
    }
    if baseline.gate != current.gate {
        return Err(ReportRefused::DifferentSuite("the gates differ".into()));
    }
    let gate = baseline.gate;
    gate.check().map_err(ReportRefused::GateCannotFail)?;
    let names = baseline.axis_order();
    if names != current.axis_order() {
        return Err(ReportRefused::DifferentSuite("the axes differ".into()));
    }
    let mut axes = Vec::new();
    let mut regressed_tasks = Vec::new();
    let mut recovered_tasks = Vec::new();
    for axis in &names {
        let axis = axis.as_str();
        let (Some(b), Some(c)) = (baseline.axes.get(axis), current.axes.get(axis)) else {
            return Err(ReportRefused::DifferentSuite(format!(
                "axis {axis} missing"
            )));
        };
        if !gate.can_fail(b.verified.rate) {
            return Err(ReportRefused::GateCannotFail(format!(
                "axis {axis}: baseline rate {:.3} with max_drop {} leaves no rate below the floor",
                b.verified.rate, gate.max_drop
            )));
        }
        if gate.per_task {
            let all = |r: &(u32, u32)| r.1 > 0 && r.0 == r.1;
            if b.task_results.keys().ne(c.task_results.keys())
                || b.task_results.len() != b.tasks.len()
            {
                return Err(ReportRefused::DifferentSuite(format!(
                    "axis {axis}: the per-task results do not name the same tasks"
                )));
            }
            for (task, br) in &b.task_results {
                let cr = &c.task_results[task];
                if all(br) && !all(cr) {
                    regressed_tasks.push(task.clone());
                } else if all(cr) && !all(br) {
                    recovered_tasks.push(task.clone());
                }
            }
        }
        axes.push(AxisDelta {
            axis: axis.to_owned(),
            baseline: b.verified.clone(),
            current: c.verified.clone(),
            delta: c.verified.rate - b.verified.rate,
            floor: gate.floor(b.verified.rate),
            passed: gate.passes(b.verified.rate, c.verified.rate),
            output_tokens_per_thousand_tasks: (
                b.output_tokens_per_thousand_tasks,
                c.output_tokens_per_thousand_tasks,
                c.output_tokens_per_thousand_tasks - b.output_tokens_per_thousand_tasks,
            ),
        });
    }
    let failed_axes: Vec<String> = axes
        .iter()
        .filter(|a| !a.passed)
        .map(|a| a.axis.clone())
        .collect();
    Ok(Comparison {
        schema: COMPARISON_SCHEMA.into(),
        baseline_digest: baseline.bundle_digest.clone(),
        current_digest: current.bundle_digest.clone(),
        gate,
        passed: failed_axes.is_empty() && regressed_tasks.is_empty(),
        failed_axes,
        regressed_tasks,
        recovered_tasks,
        axes,
        output_tokens_per_thousand_tasks: (
            baseline.cost.output_tokens_per_thousand_tasks,
            current.cost.output_tokens_per_thousand_tasks,
            current.cost.output_tokens_per_thousand_tasks
                - baseline.cost.output_tokens_per_thousand_tasks,
        ),
        cost_usd_per_thousand_tasks: (
            baseline.cost.cost_usd_per_thousand_tasks,
            current.cost.cost_usd_per_thousand_tasks,
        ),
    })
}

/// The report as Markdown, appended to the run's summary.
#[must_use]
pub fn render(r: &AxisReport) -> String {
    let names = r.axis_order();
    let title = if names == default_axes() {
        "Four axes (PX-184)".to_owned()
    } else {
        format!("Axes of suite {} (PX-179)", r.suite_id)
    };
    let mut s = format!(
        "\n## {title}\n\n- bundle digest: `{}`\n- gate: an axis fails when its verified-success rate is more than {} below the baseline's\n- output tokens per thousand tasks: {:.0} ({} output tokens over {} trials, {} model calls){}\n- source: {}\n{}\n| axis | tasks | verified | 95% interval | output tokens / 1000 tasks |\n|---|---|---|---|---|\n",
        r.bundle_digest,
        r.gate.max_drop,
        r.cost.output_tokens_per_thousand_tasks,
        r.cost.output_tokens,
        r.cost.trials,
        r.cost.model_calls,
        r.cost
            .cost_usd_per_thousand_tasks
            .map(|c| format!("; {c:.4} USD per thousand tasks at catalog list prices"))
            .unwrap_or_default(),
        r.cost.source,
        if r.gate.per_task {
            "- per-task gate: a task that verified in the baseline and does not now fails the gate, named\n"
        } else {
            ""
        }
    );
    for axis in &names {
        if let Some(a) = r.axes.get(axis) {
            s.push_str(&format!(
                "| {axis} | {} | {}/{} = {:.3} | {:.3}–{:.3} | {:.0} |\n",
                a.tasks.len(),
                a.verified.successes,
                a.verified.n,
                a.verified.rate,
                a.verified.ci95.0,
                a.verified.ci95.1,
                a.output_tokens_per_thousand_tasks
            ));
        }
    }
    for axis in &names {
        if let Some(a) = r.axes.get(axis) {
            for f in &a.failures {
                s.push_str(&format!("\n- {axis}: {f}"));
            }
        }
    }
    s.push('\n');
    s
}

/// The comparison as Markdown.
#[must_use]
pub fn render_comparison(c: &Comparison) -> String {
    let mut s = format!(
        "# Axis comparison (PX-184, PX-179)\n\n- baseline: `{}`\n- current: `{}`\n- gate: max_drop {}\n- output tokens per thousand tasks: {:.0} -> {:.0} ({:+.0})\n\n| axis | baseline | current | delta | floor | gate |\n|---|---|---|---|---|---|\n",
        c.baseline_digest,
        c.current_digest,
        c.gate.max_drop,
        c.output_tokens_per_thousand_tasks.0,
        c.output_tokens_per_thousand_tasks.1,
        c.output_tokens_per_thousand_tasks.2
    );
    for a in &c.axes {
        s.push_str(&format!(
            "| {} | {}/{} | {}/{} | {:+.3} | {:.3} | {} |\n",
            a.axis,
            a.baseline.successes,
            a.baseline.n,
            a.current.successes,
            a.current.n,
            a.delta,
            a.floor,
            if a.passed { "pass" } else { "FAIL" }
        ));
    }
    if !c.recovered_tasks.is_empty() {
        s.push_str(&format!(
            "\nrecovered tasks: {}\n",
            c.recovered_tasks.join(", ")
        ));
    }
    if !c.regressed_tasks.is_empty() {
        s.push_str(&format!(
            "\nregressed tasks: {}\n",
            c.regressed_tasks.join(", ")
        ));
    }
    let mut failed: Vec<String> = c.failed_axes.clone();
    failed.extend(c.regressed_tasks.iter().map(|t| format!("task {t}")));
    s.push_str(&format!(
        "\n{}\n",
        if c.passed {
            "every axis passed its gate".to_owned()
        } else {
            format!("gate failed for: {}", failed.join(", "))
        }
    ));
    s
}
