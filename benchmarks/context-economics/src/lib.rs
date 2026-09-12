//! Context economics benchmark (REQ-EV-0250 / 0252 / 0274, docs/63): paired
//! trials of one task under two configurations of the product, and the
//! statistics that say whether the difference is worth anything.
//!
//! It also holds the small statistics the other benchmarks share — a median, a
//! mean and a deterministic bootstrap interval — so one definition of each
//! exists.
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
    /// Tool calls the run made, as the log counts them.
    pub tool_calls: u32,
    /// Tool calls normalized across tool families (REQ-EV-0251): protocol
    /// calls left out, a batch counted as its operations.
    #[serde(default)]
    pub normalized_tool_calls: u32,
    /// Model invocations.
    pub model_calls: u32,
    /// Wall time of the agent's work, excluding index building.
    pub agent_ms: u64,
    /// Wall time from a cold start to the first use, index building included.
    pub cold_ms: u64,
    /// Whether the run reached a verified outcome.
    pub verified: bool,
    /// Bytes of tool schemas sent to the model, summed over the run's
    /// requests (M5.6 tool-schema economics; REQ-EV-0116).
    #[serde(default)]
    pub tool_schema_bytes: u64,
}

/// The measured quantities, so a report can speak about any of them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Metric {
    /// Prompt input tokens.
    InputTokens,
    /// Tool calls.
    ToolCalls,
    /// Normalized tool calls.
    NormalizedToolCalls,
    /// Agent time.
    AgentMs,
    /// Cold time to first use.
    ColdMs,
    /// Model invocations.
    ModelCalls,
    /// Tool-schema bytes sent over the run.
    ToolSchemaBytes,
}

impl Metric {
    /// The metric's value in a trial.
    #[must_use]
    pub fn of(self, t: &Trial) -> f64 {
        match self {
            Self::InputTokens => t.input_tokens as f64,
            Self::ToolCalls => f64::from(t.tool_calls),
            Self::NormalizedToolCalls => f64::from(t.normalized_tool_calls),
            Self::AgentMs => t.agent_ms as f64,
            Self::ColdMs => t.cold_ms as f64,
            Self::ModelCalls => f64::from(t.model_calls),
            Self::ToolSchemaBytes => t.tool_schema_bytes as f64,
        }
    }

    /// Its name in a report.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::InputTokens => "input_tokens",
            Self::ToolCalls => "tool_calls",
            Self::NormalizedToolCalls => "normalized_tool_calls",
            Self::AgentMs => "agent_ms",
            Self::ColdMs => "cold_ms",
            Self::ModelCalls => "model_calls",
            Self::ToolSchemaBytes => "tool_schema_bytes",
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
    /// Whether the variants were asked the same thing (REQ-EV-0253), when
    /// the harness recorded what each saw.
    #[serde(default)]
    pub prompt_parity: Option<PromptParity>,
}

/// The middle value of a sample (the mean of the two middle values when the
/// sample is even). Shared with the other benchmarks so one definition of a
/// median exists (docs/81).
#[must_use]
pub fn median(mut v: Vec<f64>) -> f64 {
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

/// The arithmetic mean of a sample.
#[must_use]
pub fn mean(v: &[f64]) -> f64 {
    if v.is_empty() {
        return 0.0;
    }
    v.iter().sum::<f64>() / v.len() as f64
}

/// A deterministic bootstrap 95% interval for the mean of paired deltas:
/// resampling with a fixed generator, so the same trials always produce the
/// same interval and a reader can rerun it. Fewer than two pairs gives no
/// interval at all rather than a fabricated one.
#[must_use]
pub fn bootstrap_ci95(deltas: &[f64]) -> (f64, f64) {
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
        Metric::NormalizedToolCalls,
        Metric::AgentMs,
        Metric::ColdMs,
        Metric::ModelCalls,
        Metric::ToolSchemaBytes,
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
        prompt_parity: None,
    }
}

/// One metric of a report, by name.
#[must_use]
pub fn metric_of(report: &PairedReport, metric: Metric) -> Option<&PairedMetric> {
    report.metrics.iter().find(|m| m.metric == metric)
}

/// Tool calls that are the protocol of a run rather than its work: the plan
/// gate, the completion handshake, repair bookkeeping and questions to the
/// user. They are the same under every variant and say nothing about how
/// much the agent had to do, so a normalized count leaves them out
/// (REQ-EV-0251).
pub const PROTOCOL_TOOLS: &[&str] = &[
    "plan.update",
    "task.complete",
    "repair.attempt",
    "user.ask",
    "tool.search",
];

/// Count a run's tool calls in a way that compares across variants that
/// expose different tool families (REQ-EV-0251): protocol calls are left
/// out, a batch counts as the operations it carried, and every other call —
/// a read, a search, a pack, a shell command, an edit — counts as one unit of
/// work whatever it is named.
///
/// `calls` are `(tool name, operations)` pairs, where `operations` is the
/// number of operations a batch carried and `1` for everything else.
#[must_use]
pub fn normalized_tool_calls(calls: &[(String, u32)]) -> u32 {
    calls
        .iter()
        .filter(|(name, _)| !PROTOCOL_TOOLS.contains(&name.as_str()))
        .map(|(name, ops)| {
            if name == "change.batch" {
                (*ops).max(1)
            } else {
                1
            }
        })
        .sum()
}

/// Whether two variants of a benchmark were asked the same thing
/// (REQ-EV-0253): the prompts an agent saw must be identical except for the
/// capability profile it had, so a treatment is never steered toward the
/// machinery under test by its instructions.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PromptParity {
    /// The system prompt is byte-identical across variants.
    pub identical_system: bool,
    /// The user's request is byte-identical across variants.
    pub identical_request: bool,
    /// Tool names one variant had and the other did not: the capability
    /// profile, which is the only thing allowed to differ.
    pub capability_difference: Vec<String>,
    /// Instructions that would force the treatment's hand, found in either
    /// variant's prompts. Any entry here invalidates the comparison.
    pub forcing_instructions: Vec<String>,
    /// Whether the comparison is unbiased: identical prompts, and no forcing.
    pub unbiased: bool,
}

/// Phrases that would tell an agent which tools to use. A benchmark prompt
/// that contains one is steering, not measuring.
pub const FORCING_PHRASES: &[&str] = &[
    "you must use",
    "always use",
    "use the context.pack",
    "call context.pack",
    "use retrieval",
    "do not read files",
    "never read",
    "prefer the context pack",
];

/// One request as the model saw it: the system prompt, the user request and
/// the tools it was offered.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SeenPrompt {
    /// System prompt text.
    pub system: String,
    /// The first user message.
    pub request: String,
    /// Tool names offered.
    pub tools: Vec<String>,
}

impl SeenPrompt {
    /// The same prompt with the workspace's location replaced by a
    /// placeholder: every trial runs in its own copy of the repository, and
    /// where that copy lives is not an instruction.
    #[must_use]
    pub fn with_workspace_placeholder(&self, root: &str) -> Self {
        let canonical = std::path::Path::new(root)
            .canonicalize()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_else(|_| root.to_owned());
        let scrub = |t: &str| {
            t.replace(&canonical, "<workspace>")
                .replace(root, "<workspace>")
        };
        Self {
            system: scrub(&self.system),
            request: scrub(&self.request),
            tools: self.tools.clone(),
        }
    }
}

/// Compare the first prompt of each variant.
#[must_use]
pub fn prompt_parity(baseline: &SeenPrompt, treatment: &SeenPrompt) -> PromptParity {
    let mut capability_difference: Vec<String> = baseline
        .tools
        .iter()
        .filter(|t| !treatment.tools.contains(t))
        .chain(
            treatment
                .tools
                .iter()
                .filter(|t| !baseline.tools.contains(t)),
        )
        .cloned()
        .collect();
    capability_difference.sort();
    capability_difference.dedup();
    let mut forcing = Vec::new();
    for (label, p) in [("baseline", baseline), ("treatment", treatment)] {
        for text in [&p.system, &p.request] {
            let lower = text.to_ascii_lowercase();
            for phrase in FORCING_PHRASES {
                if lower.contains(phrase) {
                    forcing.push(format!("{label}: \"{phrase}\""));
                }
            }
        }
    }
    forcing.sort();
    forcing.dedup();
    let identical_system = baseline.system == treatment.system;
    let identical_request = baseline.request == treatment.request;
    PromptParity {
        identical_system,
        identical_request,
        capability_difference,
        unbiased: identical_system && identical_request && forcing.is_empty(),
        forcing_instructions: forcing,
    }
}
