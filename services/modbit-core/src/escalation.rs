//! Quality-rejection continuations (REQ-EPR-006; docs/27 §9.2 and §9.6,
//! docs/49 EPR-006): when the initial solver's candidate is rejected and the
//! solver has exhausted its own bounded repair, the run continues on the
//! prevalidated stronger-solver slot of the plan in force — the same run,
//! the same transcript with the original request and the failed leg's
//! evidence, the remaining budget — and on nothing else. No slot is invented
//! here: a plan without one stops safely and says so.

use modbit_core_runtime::admission::{self, RunLedger};
use modbit_core_runtime::harness::HarnessState;
use modbit_domain::RunId;
use modbit_domain::event::{Actor, AggregateType};
use modbit_domain::routing::{ConditionalExecutionPlan, Money, Trigger};
use modbit_domain::run::RunEvent;
use modbit_domain::task::Task;
use modbit_event_store::EventStore;

use crate::runtime::{Lineage, StartConfig, append, typed};
use crate::server::Core;

/// Why the initial leg is over without acceptance.
#[derive(Clone, Debug)]
pub(crate) struct Rejection {
    /// `REPAIR_ESCALATED` | `NO_PROGRESS` | `RESUMED`.
    pub code: String,
    /// In words.
    pub detail: String,
}

/// What the boundary decided.
#[derive(Clone, Debug)]
pub(crate) enum Decision {
    /// The stronger slot is active: the loop continues on it.
    Activated {
        /// The note the continuation reads (also on the log).
        note: String,
    },
    /// Nothing activates; the run ends as it would have. `reason` is on the
    /// log as the `RouteReevaluated` STAY.
    Stayed {
        /// Why.
        reason: String,
    },
}

/// The plan in force on a run: the one admitted under the highest routing
/// epoch, as the object the log holds.
pub(crate) fn plan_in_force(
    store: &EventStore,
    task: &Task,
    run_id: RunId,
) -> Option<ConditionalExecutionPlan> {
    let events = store
        .read_session(&task.session_id, 0, usize::MAX)
        .unwrap_or_default();
    events
        .iter()
        .filter(|e| {
            e.envelope.run_id == Some(run_id) && e.envelope.event_type == "RoutingPlanCompiled"
        })
        .filter_map(|e| {
            let p = store.payload(&e.envelope).ok()?;
            serde_json::from_value::<ConditionalExecutionPlan>(p["plan"].clone()).ok()
        })
        .max_by_key(|p| p.routing_epoch)
}

/// The money the run has spent so far, priced from every recorded attempt
/// at the registry's prices for the binding it ran on; an attempt whose
/// usage the provider never reported is charged its slot's worst case, so
/// unknown cost narrows the allowance rather than widening it.
fn spent_so_far(
    store: &EventStore,
    core: &Core,
    run_id: RunId,
    currency: &str,
    scale: u8,
) -> (Money, u32) {
    let registry = core.gateway.registry();
    let mut minor: u64 = 0;
    let mut attempts: u32 = 0;
    for plan in store.routing_plans(&run_id).unwrap_or_default() {
        for a in store
            .routing_attempts(&run_id, &plan.plan_id)
            .unwrap_or_default()
        {
            attempts += 1;
            let slot = plan.slots.iter().find(|s| s.slot_id == a.slot_id);
            let priced = match (a.usage_known, slot) {
                (true, Some(s)) => registry
                    .as_ref()
                    .and_then(|r| r.entry(&s.endpoint, &s.model))
                    .map(|e| {
                        a.input_tokens
                            .unwrap_or(0)
                            .saturating_mul(e.economics.input_per_mtok_minor)
                            .div_ceil(1_000_000)
                            .saturating_add(
                                a.output_tokens
                                    .unwrap_or(0)
                                    .saturating_mul(e.economics.output_per_mtok_minor)
                                    .div_ceil(1_000_000),
                            )
                    }),
                _ => None,
            };
            minor = minor.saturating_add(priced.unwrap_or(slot.map_or(0, |s| s.reserved_minor)));
        }
    }
    (
        Money {
            minor_units: minor,
            currency: currency.to_owned(),
            scale,
        },
        attempts,
    )
}

/// The quality boundary (docs/27 §9.2 "If solver quality is inadequate,
/// select the prevalidated escalation slot"). Called once the initial leg
/// is over without acceptance; `rejection` says how. The candidate is
/// judged by the Acceptance Gate at its exact revision — a COMPLETION run
/// is made when the leg never proposed one — and only a REJECT continues.
///
/// On activation the run's configuration is switched to the slot's binding
/// and the transcript gains the continuation note; the events are on the
/// run. On anything else the decision is on the log as a STAY and the
/// caller ends the run as it would have.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn at_quality_boundary(
    core: &Core,
    task: &Task,
    run_id: RunId,
    cfg: &mut StartConfig,
    state: &mut HarnessState,
    lt: Lineage,
    actor: &Actor,
    rejection: &Rejection,
) -> Decision {
    let current = format!("{}/{}", cfg.endpoint, cfg.model);
    let context = crate::routing::RouteContext {
        endpoint: cfg.endpoint.clone(),
        model: cfg.model.clone(),
        cache: None,
        plan_id: cfg.plan_id.clone(),
    };
    let stay = |reason: String| -> Decision { Decision::Stayed { reason } };
    // 1. The plan in force and its prevalidated continuation of the leg
    //    that just failed.
    let plan = {
        let store = core.store.lock().await;
        plan_in_force(&store, task, run_id)
    };
    let Some(plan) = plan else {
        return stay("no plan is in force on this run".into());
    };
    let slot = plan.slots.iter().find(|s| {
        s.trigger == Trigger::QualityRejected
            && s.predecessor.as_deref().is_some_and(|p| {
                plan.slots
                    .iter()
                    .any(|q| q.slot_id == p && q.endpoint == cfg.endpoint && q.model == cfg.model)
            })
    });
    let record_stay = |reason: String| {
        let ev = crate::routing::reevaluated_event(
            "QUALITY",
            plan.routing_epoch,
            Some(&context),
            &current,
            "STAY",
            reason.clone(),
            None,
            &plan.plan_id,
            actor.clone(),
        );
        (ev, reason)
    };
    let Some(slot) = slot.cloned() else {
        let (ev, reason) = record_stay(format!(
            "{}: {}; the plan in force ({}) has no prevalidated stronger-solver slot continuing {current}; the run stops safely",
            rejection.code, rejection.detail, plan.plan_id
        ));
        let mut store = core.store.lock().await;
        let _ = append(
            &mut store,
            core,
            lt,
            AggregateType::Run,
            *run_id.as_bytes(),
            vec![ev],
        );
        return stay(reason);
    };
    // 2. The gate, at the exact candidate revision: what the leg proposed,
    //    or a COMPLETION run made now so the evidence is current.
    let rev = state.candidate_revision.unwrap_or(0);
    let gate_current = state
        .acceptance
        .as_ref()
        .and_then(|a| a["candidate_revision"].as_u64())
        == Some(rev)
        && state.acceptance.is_some();
    if !gate_current {
        let _ = crate::runtime::run_verification(
            core,
            task,
            lt,
            actor,
            state,
            modbit_verification::Stage::Completion,
            0,
        )
        .await;
    }
    let verdict = state
        .acceptance
        .as_ref()
        .and_then(|a| a["verdict"].as_str())
        .unwrap_or("")
        .to_owned();
    let gate_ref = state
        .acceptance
        .as_ref()
        .and_then(|a| a["gate_ref"].as_str())
        .unwrap_or("")
        .to_owned();
    let reject_reasons: Vec<String> = state
        .acceptance
        .as_ref()
        .and_then(|a| a["reject_reasons"].as_array())
        .map(|a| {
            a.iter()
                .filter_map(|x| x.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default();
    if verdict != "REJECT" {
        let (ev, reason) = record_stay(format!(
            "{}: {}; the acceptance gate says {} for the candidate at revision {rev} (gate {gate_ref}); a continuation activates only on REJECT",
            rejection.code,
            rejection.detail,
            if verdict.is_empty() {
                "nothing"
            } else {
                verdict.as_str()
            }
        ));
        let mut store = core.store.lock().await;
        let _ = append(
            &mut store,
            core,
            lt,
            AggregateType::Run,
            *run_id.as_bytes(),
            vec![ev],
        );
        return stay(reason);
    }
    // 3. Admission against what the run has already spent: the continuation
    //    gets the remaining budget, never a fresh one.
    let mut store = core.store.lock().await;
    let (spent, attempts) = spent_so_far(
        &store,
        core,
        run_id,
        &plan.total_budget.currency,
        plan.total_budget.scale,
    );
    let activations: Vec<(String, u32)> = store
        .routing_activations(&run_id, &plan.plan_id)
        .unwrap_or_default()
        .into_iter()
        .fold(Vec::new(), |mut acc, a| {
            match acc.iter_mut().find(|(s, _)| *s == a.slot_id) {
                Some((_, n)) => *n = (*n).max(a.activation),
                None => acc.push((a.slot_id, a.activation)),
            }
            acc
        });
    // The transaction's attempt ceiling counts legs — every activation the
    // run has made on any of its plans — while `attempts` above counts the
    // invocations that were priced.
    let legs: u32 = store
        .routing_plans(&run_id)
        .unwrap_or_default()
        .iter()
        .map(|p| {
            store
                .routing_activations(&run_id, &p.plan_id)
                .map(|a| a.len() as u32)
                .unwrap_or(0)
        })
        .sum();
    let ledger = RunLedger {
        activations,
        attempts: legs,
        spent: spent.clone(),
        in_flight: Money::zero(&plan.total_budget.currency, plan.total_budget.scale),
    };
    let activation = match admission::admit_activation(
        &plan,
        &ledger,
        &slot.slot_id,
        Trigger::QualityRejected,
    ) {
        Ok(a) => a,
        Err(r) => {
            let (ev, reason) = record_stay(format!(
                "{}: {}; the stronger-solver slot `{}` ({}/{}) is not admitted: {} ({r:?}); spent {} of {} {} so far over {attempts} invocation(s) in {legs} leg(s); the run stops safely",
                rejection.code,
                rejection.detail,
                slot.slot_id,
                slot.endpoint,
                slot.model,
                r.code(),
                spent.minor_units,
                plan.total_budget.minor_units,
                plan.total_budget.currency
            ));
            let _ = append(
                &mut store,
                core,
                lt,
                AggregateType::Run,
                *run_id.as_bytes(),
                vec![ev],
            );
            return stay(reason);
        }
    };
    let remaining = plan
        .total_budget
        .minor_units
        .saturating_sub(spent.minor_units)
        .saturating_sub(plan.verification_reserve.minor_units);
    let repair_history: Vec<String> = state
        .repair_attempts
        .iter()
        .map(|a| {
            format!(
                "#{} {}: {} -> {}",
                a.attempt_ordinal,
                a.failure_signature,
                a.hypothesis,
                a.outcome.as_deref().unwrap_or("pending")
            )
        })
        .collect();
    let chosen = format!("{}/{}", slot.endpoint, slot.model);
    let note = format!(
        "[CONTINUATION] The runtime activated the plan's prevalidated stronger-solver slot `{}`: you are {chosen}, continuing the same task on the same workspace after the initial leg on {current} was rejected.\nWhy: {} — {}.\nAcceptance gate at revision {rev}: REJECT ({}); gate record {gate_ref}.\nRepair attempts of the failed leg ({}): {}.\nThe original request is the first user message above and stands unchanged; every tool result above is the failed leg's evidence — read it, do not repeat its change. Remaining budget: {remaining} {} minor units of {}; this continuation has no further continuation.",
        slot.slot_id,
        rejection.code,
        rejection.detail,
        reject_reasons.join("; "),
        repair_history.len(),
        if repair_history.is_empty() {
            "none".to_owned()
        } else {
            repair_history.join(" | ")
        },
        plan.total_budget.currency,
        plan.total_budget.minor_units,
    );
    let events = vec![
        typed(
            "SlotActivated",
            &RunEvent::SlotActivated {
                plan_id: plan.plan_id.clone(),
                slot_id: activation.slot_id.clone(),
                activation: activation.activation,
                reserved_minor: activation.reserved.minor_units,
            },
            actor.clone(),
        ),
        crate::routing::reevaluated_event(
            "QUALITY",
            plan.routing_epoch,
            Some(&context),
            &chosen,
            "SWITCH",
            format!(
                "{}: {}; acceptance gate REJECT at revision {rev}; prevalidated stronger-solver slot `{}` activated on the same run with the remaining budget",
                rejection.code, rejection.detail, slot.slot_id
            ),
            None,
            &plan.plan_id,
            actor.clone(),
        ),
        typed(
            "ContinuationActivated",
            &RunEvent::ContinuationActivated {
                plan_id: plan.plan_id.clone(),
                from_plan_id: cfg.plan_id.clone(),
                from_slot_id: cfg.slot_id.clone(),
                slot_id: slot.slot_id.clone(),
                activation: activation.activation,
                trigger: "QUALITY_REJECTED".into(),
                cause: rejection.code.clone(),
                endpoint: slot.endpoint.clone(),
                model: slot.model.clone(),
                gate_ref: gate_ref.clone(),
                candidate_revision: rev,
                reject_reasons: reject_reasons.clone(),
                failed_leg_attempts: attempts,
                failed_leg_repair_attempts: repair_history.len() as u32,
                spent_minor: spent.minor_units,
                reserved_minor: activation.reserved.minor_units,
                remaining_minor: remaining,
                currency: plan.total_budget.currency.clone(),
                scale: plan.total_budget.scale,
                note: note.clone(),
            },
            actor.clone(),
        ),
    ];
    if let Err(e) = append(
        &mut store,
        core,
        lt,
        AggregateType::Run,
        *run_id.as_bytes(),
        events,
    ) {
        return stay(format!("the activation could not be recorded: {e}"));
    }
    drop(store);
    // The run continues on the slot's binding: attempts from here are the
    // continuation's, the harness's repair loop starts a fresh leg (the
    // failed leg's history stays on the log), and turns without progress
    // are counted from now.
    cfg.plan_id = plan.plan_id.clone();
    cfg.slot_id = slot.slot_id.clone();
    cfg.endpoint = slot.endpoint.clone();
    cfg.model = slot.model.clone();
    state.begin_leg();
    Decision::Activated { note }
}
