//! The offline Policy Lab (REQ-EPR-012; docs/27 §13, docs/38
//! "PromoteRoutingPolicy"). Not on the request critical path: it searches
//! the approved configuration space of a candidate routing policy on pinned
//! outcome statistics and says which configuration, if any, may be promoted.
//!
//! The legal shape is fixed. What is searched is the candidate's
//! conditional parameters — today the auto quality floor its bounded
//! conditional plans are selected against — never a graph, a new branch or
//! a topology. Each configuration is evaluated by the caller's evaluator,
//! which must be the router's own compiler over the candidate registry with
//! that configuration, so the lab never becomes a second compiler.
//!
//! Two stages, in order (docs/27 §13.3):
//!
//! - **Stage A** keeps only configurations whose selection is feasible with
//!   confidence and whose selected plan's lower bound reaches the target the
//!   spec pinned before anything was evaluated;
//! - **Stage B** takes the cheapest expected complete cost on that feasible
//!   set, the higher floor breaking ties (the safer of two equal costs).
//!
//! The chosen configuration is then evaluated once on the untouched holdout:
//! a snapshot of another version sharing no source with the one searched.
//! It must stay feasible there and select the same plan; otherwise the
//! verdict is `HOLDOUT_REGRESSION` and nothing may be promoted on it.
//!
//! A report carries its spec, both partitions and every evaluation, and a
//! digest over all of it, so the gate that promotes on it can check it and
//! re-run the same search.

#![forbid(unsafe_code)]

use modbit_bench_outcome_statistics::Snapshot;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// The version a report is written by.
pub const LAB_VERSION: &str = "policy-lab-1";

/// The most configurations one search may evaluate: the space is bounded.
pub const MAX_FLOORS: usize = 32;

/// Verdicts.
pub const FEASIBLE: &str = "FEASIBLE";
/// No configuration passed Stage A.
pub const NO_FEASIBLE_CONFIGURATION: &str = "NO_FEASIBLE_CONFIGURATION";
/// The chosen configuration did not hold on the holdout.
pub const HOLDOUT_REGRESSION: &str = "HOLDOUT_REGRESSION";

/// What is searched, pinned before evaluation.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Spec {
    /// The registry generation this search is for.
    pub candidate_generation: String,
    /// Stage A: the lower bound a selected plan must reach.
    pub target: f64,
    /// The auto quality floors searched.
    pub floors: Vec<f64>,
    /// The statistics searched on: the version the candidate joins.
    pub train_stats_version: String,
    /// The untouched holdout.
    pub holdout_stats_version: String,
    /// The representative request each configuration is compiled for.
    #[serde(default)]
    pub request: RequestShape,
}

/// The request a configuration is compiled for.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct RequestShape {
    /// Execution profile; empty is `local_trusted`.
    pub execution_profile: String,
    /// The request's cap in minor units; 0 is the floor's own ceiling.
    pub cap_minor: u64,
    /// Whether an acceptance gate with real assurance is available.
    pub assurance_available: bool,
}

/// What the router's compiler said about one configuration on one partition.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Evaluation {
    /// The auto floor this configuration sets.
    pub floor: f64,
    /// The selection code (`FEASIBLE`, `QUALITY_FLOOR_INFEASIBLE`, …) or the
    /// compiler's refusal code.
    pub code: String,
    /// The selected plan's bindings, in slot order.
    pub selected: Vec<String>,
    /// The selected plan's lower bound.
    pub selected_lcb: f64,
    /// The selected plan's expected complete cost, in minor units.
    pub expected_cost_minor: u64,
    /// Whether the compiler says the target of this floor was met.
    pub target_met: bool,
}

/// One partition of the evidence, as the report pins it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Partition {
    /// `TRAIN` or `HOLDOUT`.
    pub role: String,
    /// Statistics version.
    pub stats_version: String,
    /// The snapshot's digest.
    pub snapshot_digest: String,
    /// Observations in it.
    pub samples: u32,
    /// The sources it was derived from.
    pub source_digests: Vec<String>,
}

/// What the lab decided, and everything it decided it on.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Report {
    /// [`LAB_VERSION`].
    pub lab_version: String,
    /// The spec, as pinned.
    pub spec: Spec,
    /// The searched partition.
    pub train: Partition,
    /// The untouched holdout.
    pub holdout: Partition,
    /// Every configuration, evaluated on the train partition, in spec order.
    pub evaluated: Vec<Evaluation>,
    /// The floors that passed Stage A.
    pub feasible: Vec<f64>,
    /// Stage B's choice.
    pub chosen: Option<Evaluation>,
    /// The chosen configuration, evaluated on the holdout.
    pub holdout_check: Option<Evaluation>,
    /// [`FEASIBLE`], [`NO_FEASIBLE_CONFIGURATION`] or [`HOLDOUT_REGRESSION`].
    pub verdict: String,
    /// Why, in words a reader can check against the evaluations.
    pub reasons: Vec<String>,
    /// SHA-256 over the report with this field empty.
    pub digest: String,
}

/// Why a search did not run.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Refused {
    /// The spec is not a bounded, well-formed search.
    BadSpec(String),
    /// The partitions are not the pinned, independent evidence the spec names.
    Partition(String),
}

impl Refused {
    /// Stable code.
    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            Self::BadSpec(_) => "SEARCH_SPEC_INVALID",
            Self::Partition(_) => "SEARCH_PARTITION_INVALID",
        }
    }

    /// Detail.
    #[must_use]
    pub fn detail(&self) -> &str {
        match self {
            Self::BadSpec(d) | Self::Partition(d) => d,
        }
    }
}

/// Stage A: feasible with confidence and at the pinned target.
#[must_use]
pub fn stage_a(spec: &Spec, e: &Evaluation) -> bool {
    e.code == FEASIBLE && e.target_met && !e.selected.is_empty() && e.selected_lcb >= spec.target
}

fn check_spec(spec: &Spec) -> Result<(), Refused> {
    let bad = |d: String| Err(Refused::BadSpec(d));
    if spec.candidate_generation.trim().is_empty() {
        return bad("a search names the candidate generation it is for".into());
    }
    if !(spec.target.is_finite() && spec.target > 0.0 && spec.target <= 1.0) {
        return bad(format!("target {} is not in (0, 1]", spec.target));
    }
    if spec.floors.is_empty() || spec.floors.len() > MAX_FLOORS {
        return bad(format!(
            "{} floors: a search evaluates between 1 and {MAX_FLOORS} configurations",
            spec.floors.len()
        ));
    }
    for (i, f) in spec.floors.iter().enumerate() {
        if !(f.is_finite() && (0.0..=1.0).contains(f)) {
            return bad(format!("floor {f} is not in [0, 1]"));
        }
        if spec.floors[..i].iter().any(|g| g.total_cmp(f).is_eq()) {
            return bad(format!("floor {f} is listed twice"));
        }
    }
    if spec.train_stats_version.trim().is_empty() || spec.holdout_stats_version.trim().is_empty() {
        return bad("a search pins both the train and the holdout statistics versions".into());
    }
    if spec.train_stats_version == spec.holdout_stats_version {
        return bad("the holdout is a different statistics version from the one searched".into());
    }
    Ok(())
}

fn partition(role: &str, s: &Snapshot) -> Partition {
    Partition {
        role: role.to_owned(),
        stats_version: s.stats_version.clone(),
        snapshot_digest: s.digest.clone(),
        samples: s.samples,
        source_digests: s.source_digests.clone(),
    }
}

fn check_partitions(spec: &Spec, train: &Snapshot, holdout: &Snapshot) -> Result<(), Refused> {
    let bad = |d: String| Err(Refused::Partition(d));
    train
        .joined(&spec.train_stats_version)
        .map_err(|e| Refused::Partition(format!("train: {e:?}")))?;
    holdout
        .joined(&spec.holdout_stats_version)
        .map_err(|e| Refused::Partition(format!("holdout: {e:?}")))?;
    holdout
        .for_tenant(&train.tenant_id)
        .map_err(|e| Refused::Partition(format!("holdout: {e:?}")))?;
    if train.digest == holdout.digest {
        return bad("the holdout is the snapshot searched".into());
    }
    let shared: Vec<&String> = holdout
        .source_digests
        .iter()
        .filter(|d| train.source_digests.contains(d))
        .collect();
    if !shared.is_empty() {
        return bad(format!(
            "the holdout is not untouched: it shares sources with the searched partition ({})",
            shared
                .iter()
                .map(|s| s.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    Ok(())
}

/// Run the search. `evaluate(floor, snapshot)` must compile the candidate
/// with that floor against that snapshot's evidence through the router's
/// own compiler.
///
/// # Errors
/// The spec is unbounded or malformed, or the partitions are not the
/// pinned, independent snapshots it names.
pub fn search(
    spec: &Spec,
    train: &Snapshot,
    holdout: &Snapshot,
    mut evaluate: impl FnMut(f64, &Snapshot) -> Evaluation,
) -> Result<Report, Refused> {
    check_spec(spec)?;
    check_partitions(spec, train, holdout)?;
    let evaluated: Vec<Evaluation> = spec.floors.iter().map(|f| evaluate(*f, train)).collect();
    let feasible: Vec<&Evaluation> = evaluated.iter().filter(|e| stage_a(spec, e)).collect();
    let mut reasons = Vec::new();
    for e in &evaluated {
        if !stage_a(spec, e) {
            reasons.push(format!(
                "floor {}: {} (selected lower bound {:.3}, target {:.3})",
                e.floor, e.code, e.selected_lcb, spec.target
            ));
        }
    }
    let chosen = feasible
        .iter()
        .min_by(|a, b| {
            a.expected_cost_minor
                .cmp(&b.expected_cost_minor)
                .then(b.floor.total_cmp(&a.floor))
        })
        .map(|e| (*e).clone());
    let (verdict, holdout_check) = match &chosen {
        None => {
            reasons.push("no configuration is feasible with confidence at the target".into());
            (NO_FEASIBLE_CONFIGURATION, None)
        }
        Some(c) => {
            let h = evaluate(c.floor, holdout);
            if !stage_a(spec, &h) {
                reasons.push(format!(
                    "holdout: floor {} is {} there (selected lower bound {:.3}, target {:.3})",
                    c.floor, h.code, h.selected_lcb, spec.target
                ));
                (HOLDOUT_REGRESSION, Some(h))
            } else if h.selected != c.selected {
                reasons.push(format!(
                    "holdout: floor {} selects {:?} there, not {:?}",
                    c.floor, h.selected, c.selected
                ));
                (HOLDOUT_REGRESSION, Some(h))
            } else {
                (FEASIBLE, Some(h))
            }
        }
    };
    let mut report = Report {
        lab_version: LAB_VERSION.to_owned(),
        spec: spec.clone(),
        train: partition("TRAIN", train),
        holdout: partition("HOLDOUT", holdout),
        feasible: feasible.iter().map(|e| e.floor).collect(),
        evaluated,
        chosen,
        holdout_check,
        verdict: verdict.to_owned(),
        reasons,
        digest: String::new(),
    };
    report.digest = digest_of(&report);
    Ok(report)
}

fn digest_of(report: &Report) -> String {
    let mut r = report.clone();
    r.digest.clear();
    let mut h = Sha256::new();
    h.update(serde_json::to_vec(&r).unwrap_or_default());
    hex::encode(h.finalize())
}

/// Whether a report's digest covers exactly what it says.
#[must_use]
pub fn verify(report: &Report) -> bool {
    report.lab_version == LAB_VERSION && digest_of(report) == report.digest
}
