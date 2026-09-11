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
    }
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
