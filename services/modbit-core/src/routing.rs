//! The routing state of a task, as every client sees it (REQ-EPR-001,
//! docs/27 and docs/38).
//!
//! The view is a read of the durable projection the runtime wrote inside the
//! append transaction: the plan the run was routed by, its slots with their
//! activation counts, and every attempt against them. On the direct path the
//! plan is the degenerate one the product already executes, so a reader sees
//! the same shape before and after a router exists.
//!
//! Two things are deliberate. A slot names its endpoint and nothing more: no
//! base URL and no credential reaches a client, because neither is needed to
//! understand a route and both are secrets. And an attempt whose provider
//! reported no usage says `usage_known: false` rather than reporting zero
//! tokens, because unknown cost stays unknown (EPR-000).

use modbit_domain::TaskId;
use modbit_protocol::v1 as wire;

use crate::server::Core;

/// What this view does not claim.
const NOT_CLAIMED: &[&str] = &[
    "A path label is derived from what actually ran; it is never a template that authorized anything.",
    "Token counts are the provider's own report; an attempt with usage_known=false has no cost estimate, not a zero cost.",
    "Reserved money of zero means unbudgeted: the direct path has no cost model.",
];

/// The routing view of a task: its latest run's plan, slots and attempts.
pub(crate) async fn view(core: &Core, task_id: TaskId) -> wire::RoutingPlanView {
    let store = core.store.lock().await;
    let Some(run) = store
        .runs_for_task(&task_id)
        .ok()
        .and_then(|runs| runs.into_iter().next_back())
    else {
        return wire::RoutingPlanView::default();
    };
    let Some(plan) = store
        .routing_plans(&run.run_id)
        .ok()
        .and_then(|p| p.into_iter().next_back())
    else {
        return wire::RoutingPlanView::default();
    };
    let attempts = store.routing_attempts(&plan.plan_id).unwrap_or_default();
    let admission = admission_view(&store, &plan.plan_id);
    wire::RoutingPlanView {
        plan_id: plan.plan_id.clone(),
        schema_version: plan.schema_version,
        routing_epoch: plan.routing_epoch,
        lease_generation: plan.lease_generation,
        content_digest: plan.content_digest,
        plan_ref: plan.plan_ref,
        total_budget_minor: plan.total_budget.0,
        currency: plan.total_budget.1,
        scale: u32::from(plan.total_budget.2),
        legacy_source: plan.legacy_source.unwrap_or_default(),
        // The label is derived from the slots that actually activated: one
        // solver and nothing else is the direct path.
        path_label: path_label(&plan.slots, &attempts),
        slots: plan
            .slots
            .into_iter()
            .map(|s| wire::RoutingSlotView {
                slot_id: s.slot_id,
                predecessor: s.predecessor.unwrap_or_default(),
                trigger: s.trigger,
                max_activations: s.max_activations,
                activations: s.activations,
                endpoint: s.endpoint,
                model: s.model,
                role: s.role,
                timeout_ms: s.timeout_ms,
                max_output_tokens: s.max_output_tokens,
                max_retries: s.max_retries,
                reserved_minor: s.reserved_minor,
            })
            .collect(),
        attempts: attempts
            .into_iter()
            .map(|a| wire::RoutingAttemptView {
                slot_id: a.slot_id,
                attempt: a.attempt,
                outcome: a.outcome,
                usage_known: a.usage_known,
                input_tokens: a.input_tokens.unwrap_or_default(),
                output_tokens: a.output_tokens.unwrap_or_default(),
                provider_request_id: a.provider_request_id.unwrap_or_default(),
            })
            .collect(),
        not_claimed: NOT_CLAIMED.iter().map(|s| (*s).to_owned()).collect(),
        admission,
    }
}

/// What admission decided about a plan, with the activations it has so far.
fn admission_view(
    store: &modbit_event_store::EventStore,
    plan_id: &str,
) -> Option<wire::RoutingAdmissionView> {
    let row = store.routing_admission(plan_id).ok().flatten()?;
    Some(wire::RoutingAdmissionView {
        admitted: true,
        plan_id: plan_id.to_owned(),
        validation_digest: row.validation_digest,
        reserved_minor: row.reserved_minor,
        currency: row.currency,
        scale: u32::from(row.scale),
        routing_epoch: 0,
        refusal_code: String::new(),
        refusal_detail: String::new(),
        feasibility: row.feasibility,
        quality_lcb_bp: row.quality_lcb_bp,
        stats_version: row.stats_version,
        thresholds_version: row.thresholds_version,
        target_met: row.target_met,
        activations: store
            .routing_activations(plan_id)
            .unwrap_or_default()
            .into_iter()
            .map(|a| wire::RoutingActivationView {
                slot_id: a.slot_id,
                activation: a.activation,
                reserved_minor: a.reserved_minor,
            })
            .collect(),
    })
}

/// The path label a plan earned, from the slots that actually ran.
fn path_label(
    slots: &[modbit_event_store::projections::RoutingSlotRow],
    attempts: &[modbit_event_store::projections::RoutingAttemptRow],
) -> String {
    let ran: std::collections::BTreeSet<&str> =
        attempts.iter().map(|a| a.slot_id.as_str()).collect();
    let roles: Vec<&str> = slots
        .iter()
        .filter(|s| ran.contains(s.slot_id.as_str()))
        .map(|s| s.role.as_str())
        .collect();
    match roles.as_slice() {
        [] => String::new(),
        ["solver"] => "DIRECT".to_owned(),
        _ => roles.join("+").to_uppercase(),
    }
}

/// Admit a conditional plan for the task's current run (REQ-EPR-014).
///
/// The plan is validated whole before anything can dispatch from it, and it is
/// recorded with the digest of exactly what was validated and the money it
/// reserves. A plan compiled in an epoch that is not newer than the admitted
/// one is refused rather than installed over it, and a legacy template label
/// authorizes nothing.
pub(crate) async fn admit(
    core: &Core,
    task_id: TaskId,
    plan_json: &str,
) -> wire::RoutingAdmissionView {
    use modbit_domain::routing::ConditionalExecutionPlan;
    let refuse = |code: &str, detail: String| wire::RoutingAdmissionView {
        admitted: false,
        refusal_code: code.to_owned(),
        refusal_detail: detail,
        ..Default::default()
    };
    let plan: ConditionalExecutionPlan = match serde_json::from_str(plan_json) {
        Ok(p) => p,
        Err(e) => return refuse("BAD_PLAN", e.to_string()),
    };
    let Some(session_id) = core
        .store
        .lock()
        .await
        .task(&task_id)
        .ok()
        .flatten()
        .map(|t| t.session_id)
    else {
        return refuse("UNKNOWN_TASK", task_id.to_string());
    };
    let mut store = core.store.lock().await;
    let Some(run) = store
        .runs_for_task(&task_id)
        .ok()
        .and_then(|runs| runs.into_iter().next_back())
    else {
        return refuse("NO_RUN", "the task has no run to admit a plan for".into());
    };
    if plan.run_id != run.run_id {
        return refuse(
            "WRONG_RUN",
            format!(
                "the plan names run {} and the task's run is {}",
                plan.run_id, run.run_id
            ),
        );
    }
    // A plan installs only over an older epoch, so admitting one twice is a
    // stale generation rather than a second installation.
    let next_epoch = store
        .routing_plans(&run.run_id)
        .unwrap_or_default()
        .iter()
        .map(|p| p.routing_epoch)
        .max()
        .map_or(0, |e| e + 1);
    let admission =
        match modbit_core_runtime::admission::admit_plan(&plan, core.tenant_id, next_epoch) {
            Ok(a) => a,
            Err(r) => return refuse(r.code(), format!("{r:?}")),
        };
    let plan_id = plan.plan_id.clone();
    let plan_ref = modbit_domain::routing::plan_digest(&plan);
    let registry = core.gateway.registry();
    let feasibility = feasibility_of(&store, registry.as_ref(), session_id, &plan);
    let events = vec![
        crate::runtime::typed(
            "RoutingPlanCompiled",
            &modbit_domain::run::RunEvent::RoutingPlanCompiled {
                plan: Box::new(plan),
                plan_ref,
            },
            modbit_domain::event::Actor::Core("admission".into()),
        ),
        crate::runtime::typed(
            "RoutingPlanAdmitted",
            &modbit_domain::run::RunEvent::RoutingPlanAdmitted {
                plan_id: plan_id.clone(),
                validation_digest: admission.validation_digest.clone(),
                reserved_minor: admission.reserved.minor_units,
                currency: admission.reserved.currency.clone(),
                scale: admission.reserved.scale,
                lease_generation: admission.lease_generation,
                feasibility: feasibility.code.clone(),
                quality_lcb_bp: feasibility.lcb_bp,
                stats_version: feasibility.stats_version.clone(),
                thresholds_version: feasibility.thresholds_version.clone(),
                target_met: feasibility.target_met,
            },
            modbit_domain::event::Actor::Core("admission".into()),
        ),
    ];
    if let Err(e) = crate::runtime::append(
        &mut store,
        core,
        crate::runtime::Lineage::run(core.tenant_id, session_id, task_id, run.run_id),
        modbit_domain::event::AggregateType::Run,
        *run.run_id.as_bytes(),
        events,
    ) {
        return refuse("STORE", e.to_string());
    }
    wire::RoutingAdmissionView {
        admitted: true,
        plan_id: plan_id.clone(),
        validation_digest: admission.validation_digest,
        reserved_minor: admission.reserved.minor_units,
        currency: admission.reserved.currency,
        scale: u32::from(admission.reserved.scale),
        routing_epoch: admission.routing_epoch,
        refusal_code: String::new(),
        refusal_detail: feasibility.detail.clone(),
        feasibility: feasibility.code.clone(),
        quality_lcb_bp: feasibility.lcb_bp,
        stats_version: feasibility.stats_version.clone(),
        thresholds_version: feasibility.thresholds_version.clone(),
        target_met: feasibility.target_met,
        activations: store
            .routing_activations(&plan_id)
            .unwrap_or_default()
            .into_iter()
            .map(|a| wire::RoutingActivationView {
                slot_id: a.slot_id,
                activation: a.activation,
                reserved_minor: a.reserved_minor,
            })
            .collect(),
    }
}

/// What confidence-adjusted feasibility said about a plan at admission
/// (REQ-EPR-016). Admission does not refuse on it; it records it, so the
/// plan can never later be described as meeting a target the evidence did
/// not support.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct FeasibilityRecord {
    /// `FEASIBLE` | `QUALITY_FLOOR_INFEASIBLE` | `QUALITY_FLOOR_UNKNOWN`.
    pub code: String,
    /// Quality lower bound, in basis points.
    pub lcb_bp: u32,
    /// The snapshot the bound came from, or `none`.
    pub stats_version: String,
    /// The threshold version, or `none`.
    pub thresholds_version: String,
    /// Whether the plan may be described as meeting the target.
    pub target_met: bool,
    /// Why, in the selector's own words.
    pub detail: String,
}

fn bp(v: f64) -> u32 {
    let scaled = (v * 10_000.0).round();
    if scaled.is_finite() && scaled >= 0.0 {
        u32::try_from(scaled as i64).unwrap_or(10_000).min(10_000)
    } else {
        0
    }
}

/// Measure one plan against the session's latest statistics snapshot and the
/// active registry's mode floor. With no floor there is nothing to measure
/// against, and the record says unknown rather than feasible.
pub(crate) fn feasibility_of(
    store: &modbit_event_store::EventStore,
    registry: Option<&modbit_providers::registry::ModelRegistry>,
    session_id: modbit_domain::SessionId,
    plan: &modbit_domain::routing::ConditionalExecutionPlan,
) -> FeasibilityRecord {
    use modbit_providers::feasibility::{Candidate, LegEvidence, Thresholds, plan_quality, select};
    let Some(floor) = registry.and_then(|r| {
        r.document
            .quality_floors
            .iter()
            .find(|f| f.mode == "auto")
            .cloned()
    }) else {
        return FeasibilityRecord {
            code: "QUALITY_FLOOR_UNKNOWN".into(),
            lcb_bp: 0,
            stats_version: "none".into(),
            thresholds_version: "none".into(),
            target_met: false,
            detail: "no active registry defines a mode floor; nothing can be measured against"
                .into(),
        };
    };
    let thresholds = Thresholds {
        mode: floor.mode.clone(),
        tau: floor.min_quality,
        delta: 0.05,
        min_samples: modbit_bench_outcome_statistics::MIN_CONFIDENT_SAMPLES,
        switch_cost_minor: 0,
        thresholds_version: registry
            .map_or_else(|| "none".to_owned(), |r| r.generation().to_owned()),
    };
    let snapshot = crate::statistics::latest_snapshot(store, session_id).map(|(s, _)| s);
    let stats_version = snapshot
        .as_ref()
        .map_or_else(|| "none".to_owned(), |s| s.stats_version.clone());
    let evidence_for = |key: &modbit_bench_outcome_statistics::StatKey| {
        snapshot
            .as_ref()
            .and_then(|s| s.get(key))
            .map(|a| LegEvidence {
                key_id: a.key_id.clone(),
                lcb: a.interval.0,
                mean: a.mean,
                samples: a.samples,
            })
    };
    let initial = plan
        .slots
        .iter()
        .find(|s| s.trigger == modbit_domain::routing::Trigger::Initial);
    let continuation = plan
        .slots
        .iter()
        .find(|s| s.trigger == modbit_domain::routing::Trigger::QualityRejected);
    let initial_key = initial.map(|s| modbit_bench_outcome_statistics::StatKey::Solver {
        model: s.model.clone(),
        skill: "none".into(),
        harness: crate::baseline::build_digest(),
    });
    let continuation_key = initial.zip(continuation).map(|(i, c)| {
        modbit_bench_outcome_statistics::StatKey::Escalation {
            from_model: i.model.clone(),
            to_model: c.model.clone(),
            gate: "acceptance".into(),
            repository: "workspace".into(),
            verification: "configured".into(),
        }
    });
    let initial_key_id = initial_key
        .as_ref()
        .map_or_else(|| "solver|none".to_owned(), |k| k.key_id());
    let continuation_key_id = continuation_key.as_ref().map(|k| k.key_id());
    let quality = plan_quality(
        initial_key.as_ref().and_then(&evidence_for).as_ref(),
        continuation_key.as_ref().and_then(&evidence_for).as_ref(),
        &initial_key_id,
        continuation_key_id.as_deref(),
        &thresholds,
    );
    let worst_case = plan
        .slots
        .iter()
        .map(|s| s.budget.reserved.minor_units)
        .sum::<u64>()
        .saturating_add(plan.verification_reserve.minor_units);
    let candidate = Candidate {
        plan_id: plan.plan_id.clone(),
        worst_case_cost_minor: worst_case,
        quality: quality.clone(),
        // Admission already validated the plan against its own cap; budget
        // and policy hold by construction here.
        hard_eligible: true,
        ineligible_reason: String::new(),
    };
    let selection = select(&[candidate], &thresholds, None);
    FeasibilityRecord {
        code: selection.code.clone(),
        lcb_bp: bp(quality.lcb),
        stats_version,
        thresholds_version: thresholds.thresholds_version.clone(),
        target_met: selection.target_met,
        detail: selection.exclusions.first().map_or_else(
            || {
                format!(
                    "lower bound {:.3} clears tau {:.3}",
                    quality.lcb, thresholds.tau
                )
            },
            |e| e.reason.clone(),
        ),
    }
}
