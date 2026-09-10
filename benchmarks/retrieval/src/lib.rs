//! The structural-advantage experiment (REQ-EV-0254, docs/63): does the
//! structural profile — AST and symbol anchors, the dependency graph, Git
//! co-change, test links — retrieve better than the hybrid profile that has
//! only text and vectors?
//!
//! It is an experiment, so it is set up to be able to say no. The comparison
//! is per case: every case is run under both profiles on the same corpus at
//! the same revision, and the report shows where the structural profile won,
//! where it lost, where it made no difference, and what the interval around
//! the mean difference is. A mean alone would hide the losses.

use modbit_bench_context_economics::{bootstrap_ci95, mean, median};
use modbit_retrieval::bench::{CaseResult, Profile, Report};
use serde::{Deserialize, Serialize};

/// What the experiment measures on each case.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Measure {
    /// Relevant paths found in the top K.
    RecallAtK,
    /// Relevant paths among the paths returned.
    PrecisionAtK,
    /// Retrieval steps the profile spent.
    Steps,
    /// Tokens the top K would cost a prompt.
    ContextTokens,
}

impl Measure {
    /// The value on one case result.
    #[must_use]
    pub fn of(self, c: &CaseResult) -> f64 {
        match self {
            Self::RecallAtK => f64::from(c.recall_at_k),
            Self::PrecisionAtK => f64::from(c.precision_at_k),
            Self::Steps => c.steps as f64,
            Self::ContextTokens => c.context_tokens_at_k as f64,
        }
    }

    /// Whether a larger value is better for this measure.
    #[must_use]
    pub fn larger_is_better(self) -> bool {
        matches!(self, Self::RecallAtK | Self::PrecisionAtK)
    }
}

/// One case's difference between two profiles.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CaseDelta {
    /// Case id.
    pub case_id: String,
    /// The baseline profile's value.
    pub baseline: f64,
    /// The treatment profile's value.
    pub treatment: f64,
    /// treatment - baseline.
    pub delta: f64,
}

/// The experiment's answer for one measure.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Comparison {
    /// What was measured.
    pub measure: Measure,
    /// The profile the treatment is compared against.
    pub baseline: Profile,
    /// The profile under test.
    pub treatment: Profile,
    /// Every case, in order.
    pub cases: Vec<CaseDelta>,
    /// Cases where the treatment was better.
    pub wins: usize,
    /// Cases where it was worse.
    pub losses: usize,
    /// Cases where it made no difference.
    pub ties: usize,
    /// Mean of the per-case deltas.
    pub mean_delta: f64,
    /// Median of the per-case deltas.
    pub median_delta: f64,
    /// Deterministic bootstrap 95% interval of the mean delta.
    pub ci95: (f64, f64),
    /// Whether the interval excludes zero.
    pub significant: bool,
    /// What the numbers do and do not establish.
    pub method: String,
}

/// Compare two profiles case by case on one measure.
///
/// Cases the two profiles do not share are left out and the comparison says so
/// through its case count; a comparison of different work would not be one.
#[must_use]
pub fn compare(
    report: &Report,
    baseline: Profile,
    treatment: Profile,
    measure: Measure,
    method: &str,
) -> Comparison {
    let value = |p: Profile, id: &str| -> Option<f64> {
        report
            .cases
            .iter()
            .find(|c| c.profile == p && c.case_id == id)
            .map(|c| measure.of(c))
    };
    let mut ids: Vec<String> = report
        .cases
        .iter()
        .filter(|c| c.profile == treatment)
        .map(|c| c.case_id.clone())
        .collect();
    ids.sort();
    ids.dedup();
    let cases: Vec<CaseDelta> = ids
        .into_iter()
        .filter_map(|id| {
            let (b, t) = (value(baseline, &id)?, value(treatment, &id)?);
            Some(CaseDelta {
                case_id: id,
                baseline: b,
                treatment: t,
                delta: t - b,
            })
        })
        .collect();
    let deltas: Vec<f64> = cases.iter().map(|c| c.delta).collect();
    let better = |d: f64| {
        if measure.larger_is_better() {
            d > 0.0
        } else {
            d < 0.0
        }
    };
    let ci95 = bootstrap_ci95(&deltas);
    Comparison {
        measure,
        baseline,
        treatment,
        wins: cases.iter().filter(|c| better(c.delta)).count(),
        losses: cases
            .iter()
            .filter(|c| c.delta != 0.0 && !better(c.delta))
            .count(),
        ties: cases.iter().filter(|c| c.delta == 0.0).count(),
        mean_delta: mean(&deltas),
        median_delta: median(deltas),
        ci95,
        significant: ci95.0.is_finite() && (ci95.0 > 0.0 || ci95.1 < 0.0),
        cases,
        method: method.to_owned(),
    }
}
