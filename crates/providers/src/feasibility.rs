//! Confidence-adjusted feasibility (REQ-EPR-016, docs/27 and docs/38): whether
//! a whole conditional plan can be expected to meet the mode's quality floor,
//! and which of the plans that can is the cheapest.
//!
//! The rule that matters is that a mean is not evidence. A plan qualifies on
//! the lower confidence bound of what was observed, and only when enough was
//! observed for that bound to mean anything; a cheap plan with a perfect mean
//! over three observations is not feasible, however good it looks. Benchmark
//! means are priors and enter only as low-confidence.
//!
//! When no plan is feasible, the answer is the best hard-eligible plan with
//! `QUALITY_FLOOR_INFEASIBLE` said out loud — never a plan outside the
//! eligible set, never a budget or policy violation, and never a claim that
//! the target was met.

use serde::{Deserialize, Serialize};

/// The version a plan pins when it records what this component said.
pub const FEASIBILITY_VERSION: &str = "feasibility-1";

/// The mode's thresholds, pinned by version like every other input.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Thresholds {
    /// Mode (`auto`, `quality`, `economy`).
    pub mode: String,
    /// The quality a plan's lower bound must clear.
    pub tau: f64,
    /// How far below `tau` a bound may fall and still be reported as near
    /// (for the record; a near plan is still infeasible).
    pub delta: f64,
    /// Observations below which a bound is a prior, not evidence.
    pub min_samples: u32,
    /// Money a cheaper plan must save before the selection switches away from
    /// the plan currently in force, so a marginal saving cannot make routing
    /// flap.
    pub switch_cost_minor: u64,
    /// The version of these thresholds.
    pub thresholds_version: String,
}

/// What was observed for one leg of a plan.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LegEvidence {
    /// The statistics key it came from.
    pub key_id: String,
    /// Lower confidence bound of the success rate.
    pub lcb: f64,
    /// The mean, carried for the record and never used to qualify.
    pub mean: f64,
    /// Observations behind it.
    pub samples: u32,
}

/// What a whole plan can be expected to do.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PlanQuality {
    /// Lower bound of the whole plan's success, composed from its legs.
    pub lcb: f64,
    /// The composed mean, for the record.
    pub mean: f64,
    /// Whether every leg that contributed had enough observations.
    pub confident: bool,
    /// Keys with no observation at all.
    pub missing: Vec<String>,
    /// Keys whose observations are too few to count.
    pub thin: Vec<String>,
}

/// Compose a plan's quality from its initial leg and, when the joint
/// escalation key has real support, its continuation.
///
/// A continuation adds only what its own observations support: with no
/// observed escalations it contributes nothing, so the whole plan is worth
/// exactly its initial leg. It never subtracts.
#[must_use]
pub fn plan_quality(
    initial: Option<&LegEvidence>,
    continuation: Option<&LegEvidence>,
    initial_key: &str,
    continuation_key: Option<&str>,
    t: &Thresholds,
) -> PlanQuality {
    let mut missing = Vec::new();
    let mut thin = Vec::new();
    let Some(first) = initial else {
        missing.push(initial_key.to_owned());
        if let Some(k) = continuation_key
            && continuation.is_none()
        {
            missing.push(k.to_owned());
        }
        return PlanQuality {
            lcb: 0.0,
            mean: 0.0,
            confident: false,
            missing,
            thin,
        };
    };
    let mut confident = true;
    if first.samples < t.min_samples {
        thin.push(first.key_id.clone());
        confident = false;
    }
    let (mut lcb, mut mean) = (first.lcb, first.mean);
    match (continuation, continuation_key) {
        (Some(next), _) => {
            if next.samples < t.min_samples {
                // A thin continuation contributes nothing rather than a
                // guess; the plan stays what its initial leg is.
                thin.push(next.key_id.clone());
            } else {
                lcb = 1.0 - (1.0 - lcb) * (1.0 - next.lcb);
                mean = 1.0 - (1.0 - mean) * (1.0 - next.mean);
            }
        }
        (None, Some(k)) => missing.push(k.to_owned()),
        (None, None) => {}
    }
    PlanQuality {
        lcb: lcb.clamp(0.0, 1.0),
        mean: mean.clamp(0.0, 1.0),
        confident,
        missing,
        thin,
    }
}

/// One plan the selector may choose.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Candidate {
    /// Plan.
    pub plan_id: String,
    /// Worst-case complete cost: every slot's reservation plus the
    /// verification reserve, in minor units. This is what the cap is checked
    /// against, because it is what the plan may actually spend.
    pub worst_case_cost_minor: u64,
    /// Expected complete cost: the initial leg plus each continuation
    /// weighted by the lower bound of its being needed, in minor units. This
    /// is what plans are ranked by, and it is never below the initial leg.
    pub expected_cost_minor: u64,
    /// What it can be expected to do.
    pub quality: PlanQuality,
    /// Whether budget and policy allow it at all. A plan that is not hard
    /// eligible is never selected for any reason.
    pub hard_eligible: bool,
    /// Why not, when it is not.
    pub ineligible_reason: String,
}

/// Why a candidate was not the selection.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Exclusion {
    /// Plan.
    pub plan_id: String,
    /// The reason, in words a reader can check against the inputs.
    pub reason: String,
}

/// What the selector decided.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Selection {
    /// The chosen plan, when any plan is hard eligible.
    pub selected: Option<String>,
    /// `FEASIBLE` | `QUALITY_FLOOR_INFEASIBLE` | `NO_HARD_ELIGIBLE_PLAN`.
    pub code: String,
    /// Plans that cleared the floor with confidence, cheapest first.
    pub feasible: Vec<String>,
    /// Why every other plan was not chosen.
    pub exclusions: Vec<Exclusion>,
    /// The chosen plan's lower bound.
    pub selected_lcb: f64,
    /// The floor it was measured against.
    pub tau: f64,
    /// Whether the selection may be described as meeting the target. Only a
    /// feasible selection may; the infeasible fallback never is.
    pub target_met: bool,
    /// The versions this decision was made under.
    pub feasibility_version: String,
    /// Threshold version.
    pub thresholds_version: String,
}

fn shortfall(q: &PlanQuality, t: &Thresholds) -> String {
    if !q.missing.is_empty() {
        return format!("no observation for {}", q.missing.join(", "));
    }
    if !q.confident {
        return format!(
            "low confidence: {} below the {}-sample threshold",
            q.thin.join(", "),
            t.min_samples
        );
    }
    let gap = t.tau - q.lcb;
    if gap <= t.delta {
        format!(
            "near: lower bound {:.3} is within {:.3} of tau {:.3}",
            q.lcb, t.delta, t.tau
        )
    } else {
        format!("lower bound {:.3} is below tau {:.3}", q.lcb, t.tau)
    }
}

fn is_feasible(c: &Candidate, t: &Thresholds) -> bool {
    c.hard_eligible && c.quality.confident && c.quality.missing.is_empty() && c.quality.lcb >= t.tau
}

/// Choose the cheapest plan that is feasible with confidence, or say exactly
/// why none is.
///
/// `current` is the plan currently in force, when there is one: a cheaper
/// feasible alternative replaces it only when it saves at least the mode's
/// switch cost, so routing cannot flap on a marginal difference.
#[must_use]
pub fn select(candidates: &[Candidate], t: &Thresholds, current: Option<&str>) -> Selection {
    let mut exclusions = Vec::new();
    let mut feasible: Vec<&Candidate> = Vec::new();
    for c in candidates {
        if !c.hard_eligible {
            exclusions.push(Exclusion {
                plan_id: c.plan_id.clone(),
                reason: format!("not hard eligible: {}", c.ineligible_reason),
            });
        } else if is_feasible(c, t) {
            feasible.push(c);
        } else {
            exclusions.push(Exclusion {
                plan_id: c.plan_id.clone(),
                reason: shortfall(&c.quality, t),
            });
        }
    }
    feasible.sort_by(|a, b| {
        a.expected_cost_minor
            .cmp(&b.expected_cost_minor)
            .then_with(|| a.worst_case_cost_minor.cmp(&b.worst_case_cost_minor))
            .then_with(|| b.quality.lcb.total_cmp(&a.quality.lcb))
            .then_with(|| a.plan_id.cmp(&b.plan_id))
    });
    if let Some(cheapest) = feasible.first() {
        let mut chosen = *cheapest;
        if let Some(cur) = current
            && let Some(kept) = feasible.iter().find(|c| c.plan_id == cur)
            && kept.plan_id != cheapest.plan_id
            && kept
                .worst_case_cost_minor
                .saturating_sub(cheapest.worst_case_cost_minor)
                < t.switch_cost_minor
        {
            exclusions.push(Exclusion {
                plan_id: cheapest.plan_id.clone(),
                reason: format!(
                    "saves {} minor units, below the switch cost of {}; the plan in force is kept",
                    kept.worst_case_cost_minor - cheapest.worst_case_cost_minor,
                    t.switch_cost_minor
                ),
            });
            chosen = kept;
        }
        for f in &feasible {
            if f.plan_id != chosen.plan_id && !exclusions.iter().any(|e| e.plan_id == f.plan_id) {
                exclusions.push(Exclusion {
                    plan_id: f.plan_id.clone(),
                    reason: format!(
                        "feasible, but an expected {} minor units is not the lowest expected complete cost",
                        f.expected_cost_minor
                    ),
                });
            }
        }
        return Selection {
            selected: Some(chosen.plan_id.clone()),
            code: "FEASIBLE".into(),
            feasible: feasible.iter().map(|c| c.plan_id.clone()).collect(),
            exclusions,
            selected_lcb: chosen.quality.lcb,
            tau: t.tau,
            target_met: true,
            feasibility_version: FEASIBILITY_VERSION.into(),
            thresholds_version: t.thresholds_version.clone(),
        };
    }
    // Nothing clears the floor with confidence. The fallback is the best
    // hard-eligible plan by its lower bound, said out loud as infeasible; it
    // never claims the target and never reaches outside the eligible set.
    let mut eligible: Vec<&Candidate> = candidates.iter().filter(|c| c.hard_eligible).collect();
    eligible.sort_by(|a, b| {
        b.quality
            .lcb
            .total_cmp(&a.quality.lcb)
            .then_with(|| a.expected_cost_minor.cmp(&b.expected_cost_minor))
            .then_with(|| a.plan_id.cmp(&b.plan_id))
    });
    match eligible.first() {
        Some(best) => Selection {
            selected: Some(best.plan_id.clone()),
            code: "QUALITY_FLOOR_INFEASIBLE".into(),
            feasible: vec![],
            exclusions,
            selected_lcb: best.quality.lcb,
            tau: t.tau,
            target_met: false,
            feasibility_version: FEASIBILITY_VERSION.into(),
            thresholds_version: t.thresholds_version.clone(),
        },
        None => Selection {
            selected: None,
            code: "NO_HARD_ELIGIBLE_PLAN".into(),
            feasible: vec![],
            exclusions,
            selected_lcb: 0.0,
            tau: t.tau,
            target_met: false,
            feasibility_version: FEASIBILITY_VERSION.into(),
            thresholds_version: t.thresholds_version.clone(),
        },
    }
}
