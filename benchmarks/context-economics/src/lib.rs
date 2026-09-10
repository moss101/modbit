//! Context economics benchmark (REQ-EV-0250 / 0252 / 0274, docs/63): paired
//! trials of one task under two configurations of the product, and the
//! statistics that say whether the difference is worth anything.
//!
//! Pairing is the point. Each trial of the treatment has a partner in the
//! baseline that ran the same task, on the same model, in the same
//! environment, so the difference between them is the product's machinery and
//! not the weather. The report says what varied, what it measured, and — in
//! its own `method` line — what it does not establish.

use serde::{Deserialize, Serialize};

/// One run of one task under one configuration.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Trial {
    /// `baseline` | `treatment`.
    pub variant: String,
    /// Task identity, the same string in both variants of a pair.
    pub task: String,
    /// Trial index within the task, so pairs line up.
    pub repeat: u32,
    /// Prompt input tokens the provider reported, summed over the run.
    pub input_tokens: u64,
    /// Output tokens.
    pub output_tokens: u64,
    /// Of the input tokens, the part the provider reported as cached.
    pub cached_input_tokens: u64,
    /// Tool calls the run made.
    pub tool_calls: u32,
    /// Model invocations.
    pub model_calls: u32,
    /// Wall time of the agent's work, excluding index building.
    pub agent_ms: u64,
    /// Wall time from a cold start to the first use, index building included.
    pub cold_ms: u64,
    /// Whether the run reached a verified outcome.
    pub verified: bool,
}

/// The measured quantities, so a report can speak about any of them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Metric {
    /// Prompt input tokens.
    InputTokens,
    /// Tool calls.
    ToolCalls,
    /// Agent time.
    AgentMs,
    /// Cold time to first use.
    ColdMs,
}

impl Metric {
    /// The metric's value in a trial.
    #[must_use]
    pub fn of(self, t: &Trial) -> f64 {
        match self {
            Self::InputTokens => t.input_tokens as f64,
            Self::ToolCalls => f64::from(t.tool_calls),
            Self::AgentMs => t.agent_ms as f64,
            Self::ColdMs => t.cold_ms as f64,
        }
    }

    /// Its name in a report.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::InputTokens => "input_tokens",
            Self::ToolCalls => "tool_calls",
            Self::AgentMs => "agent_ms",
            Self::ColdMs => "cold_ms",
        }
    }
}

/// One metric's paired result.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PairedMetric {
    /// Metric.
    pub metric: Metric,
    /// Pairs that contributed.
    pub pairs: usize,
    /// Median of the baseline values.
    pub baseline_median: f64,
    /// Median of the treatment values.
    pub treatment_median: f64,
    /// Mean of (treatment - baseline); negative is a saving.
    pub mean_delta: f64,
    /// Median of the paired deltas.
    pub median_delta: f64,
    /// 95% confidence interval of the mean delta, by bootstrap over the pairs.
    pub ci95: (f64, f64),
    /// Share of the baseline the mean delta represents, when the baseline is
    /// not zero.
    pub relative: Option<f64>,
    /// Whether the interval excludes zero.
    pub significant: bool,
}

/// What a paired run of the benchmark found.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PairedReport {
    /// Tasks that contributed pairs.
    pub tasks: Vec<String>,
    /// Complete pairs.
    pub pairs: usize,
    /// Per metric.
    pub metrics: Vec<PairedMetric>,
    /// Verified outcomes in each variant: (baseline, treatment).
    pub verified: (usize, usize),
    /// What varied between the variants, in words.
    pub varied: String,
    /// What was held constant.
    pub held_constant: Vec<String>,
    /// How the numbers were produced and what they do not establish.
    pub method: String,
    /// Trials that had no partner, by variant and task.
    pub unpaired: Vec<String>,
}

fn median(mut v: Vec<f64>) -> f64 {
    if v.is_empty() {
        return 0.0;
    }
    v.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let mid = v.len() / 2;
    if v.len().is_multiple_of(2) {
        (v[mid - 1] + v[mid]) / 2.0
    } else {
        v[mid]
    }
}

fn mean(v: &[f64]) -> f64 {
    if v.is_empty() {
        return 0.0;
    }
    v.iter().sum::<f64>() / v.len() as f64
}

/// A deterministic bootstrap: resampling with a fixed generator, so the same
/// trials always produce the same interval and a reader can rerun it.
fn bootstrap_ci95(deltas: &[f64]) -> (f64, f64) {
    if deltas.len() < 2 {
        return (f64::NAN, f64::NAN);
    }
    let mut state: u64 = 0x2545_F491_4F6C_DD1D;
    let mut next = || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        state
    };
    let mut means: Vec<f64> = (0..2000)
        .map(|_| {
            let sample: Vec<f64> = (0..deltas.len())
                .map(|_| deltas[(next() as usize) % deltas.len()])
                .collect();
            mean(&sample)
        })
        .collect();
    means.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let lo = means[(means.len() as f64 * 0.025) as usize];
    let hi = means[((means.len() as f64 * 0.975) as usize).min(means.len() - 1)];
    (lo, hi)
}

/// Pair the trials by (task, repeat) and report each metric.
///
/// A trial with no partner is named in `unpaired` and counted nowhere else: an
/// unpaired run cannot say anything about a difference.
#[must_use]
pub fn paired_report(
    trials: &[Trial],
    varied: &str,
    held_constant: &[&str],
    method: &str,
) -> PairedReport {
    let mut pairs: Vec<(&Trial, &Trial)> = Vec::new();
    let mut unpaired: Vec<String> = Vec::new();
    for t in trials.iter().filter(|t| t.variant == "treatment") {
        match trials
            .iter()
            .find(|b| b.variant == "baseline" && b.task == t.task && b.repeat == t.repeat)
        {
            Some(b) => pairs.push((b, t)),
            None => unpaired.push(format!("treatment {} #{}", t.task, t.repeat)),
        }
    }
    for b in trials.iter().filter(|t| t.variant == "baseline") {
        if !trials
            .iter()
            .any(|t| t.variant == "treatment" && t.task == b.task && t.repeat == b.repeat)
        {
            unpaired.push(format!("baseline {} #{}", b.task, b.repeat));
        }
    }
    let metrics = [
        Metric::InputTokens,
        Metric::ToolCalls,
        Metric::AgentMs,
        Metric::ColdMs,
    ]
    .into_iter()
    .map(|metric| {
        let deltas: Vec<f64> = pairs
            .iter()
            .map(|(b, t)| metric.of(t) - metric.of(b))
            .collect();
        let base: Vec<f64> = pairs.iter().map(|(b, _)| metric.of(b)).collect();
        let treat: Vec<f64> = pairs.iter().map(|(_, t)| metric.of(t)).collect();
        let ci95 = bootstrap_ci95(&deltas);
        let baseline_median = median(base.clone());
        PairedMetric {
            metric,
            pairs: pairs.len(),
            baseline_median,
            treatment_median: median(treat),
            mean_delta: mean(&deltas),
            median_delta: median(deltas.clone()),
            ci95,
            relative: (baseline_median.abs() > f64::EPSILON)
                .then(|| mean(&deltas) / baseline_median),
            significant: ci95.0.is_finite() && (ci95.0 > 0.0 || ci95.1 < 0.0),
        }
    })
    .collect();
    let mut tasks: Vec<String> = pairs.iter().map(|(b, _)| b.task.clone()).collect();
    tasks.sort();
    tasks.dedup();
    unpaired.sort();
    PairedReport {
        tasks,
        pairs: pairs.len(),
        metrics,
        verified: (
            pairs.iter().filter(|(b, _)| b.verified).count(),
            pairs.iter().filter(|(_, t)| t.verified).count(),
        ),
        varied: varied.to_owned(),
        held_constant: held_constant.iter().map(|s| (*s).to_owned()).collect(),
        method: method.to_owned(),
        unpaired,
    }
}

/// One metric of a report, by name.
#[must_use]
pub fn metric_of(report: &PairedReport, metric: Metric) -> Option<&PairedMetric> {
    report.metrics.iter().find(|m| m.metric == metric)
}
