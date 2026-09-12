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
        .and_then(|p| p.into_iter().max_by_key(|p| p.routing_epoch))
    else {
        return wire::RoutingPlanView::default();
    };
    let attempts = store
        .routing_attempts(&run.run_id, &plan.plan_id)
        .unwrap_or_default();
    // The label covers the run: a transaction admitted for a rejected leg
    // (REQ-EPR-006) ran its continuation after a leg of the transaction
    // before it, and the path is what ran on both.
    let mut plans = store.routing_plans(&run.run_id).unwrap_or_default();
    plans.sort_by_key(|p| p.routing_epoch);
    let mut ran: Vec<(String, String, String)> = Vec::new();
    for p in &plans {
        let attempts = store
            .routing_attempts(&run.run_id, &p.plan_id)
            .unwrap_or_default();
        for s in p
            .slots
            .iter()
            .filter(|s| attempts.iter().any(|a| a.slot_id == s.slot_id))
        {
            let leg = (
                s.role.clone(),
                s.trigger.clone(),
                format!("{}/{}", s.endpoint, s.model),
            );
            // One leg that continued across a reconciled transaction on
            // the same binding is one leg.
            if ran.last() != Some(&leg) {
                ran.push(leg);
            }
        }
    }
    let admission = admission_view(&store, run.run_id, &plan.plan_id);
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
        // The label is derived from the slots that actually ran: one
        // solver and nothing else is the direct path; a stronger solver
        // after a rejected one is the cascade.
        path_label: path_label(&ran),
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
    run_id: modbit_domain::RunId,
    plan_id: &str,
) -> Option<wire::RoutingAdmissionView> {
    let row = store.routing_admission(&run_id, plan_id).ok().flatten()?;
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
            .routing_activations(&run_id, plan_id)
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

/// The path label a run earned, from the `(role, trigger, binding)` of the
/// slots that actually ran, in order: one solver is `DIRECT`; a solver continued
/// by a stronger solver on quality rejection is `CASCADE` (docs/27 §9,
/// REQ-EPR-006: derived from what ran, never a template); anything else is
/// the roles joined.
fn path_label(ran: &[(String, String, String)]) -> String {
    let roles: Vec<&str> = ran.iter().map(|(r, _, _)| r.as_str()).collect();
    match roles.as_slice() {
        [] => String::new(),
        ["solver"] => "DIRECT".to_owned(),
        ["solver", "solver"] if ran[1].1 == "QUALITY_REJECTED" => "CASCADE".to_owned(),
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
            .routing_activations(&run.run_id, &plan_id)
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
        expected_cost_minor: worst_case,
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

/// A plan compiled for a run, admitted and measured, ready to be recorded.
pub(crate) struct CompiledForRun {
    /// What the compiler produced.
    pub compiled: modbit_providers::compiler::Compiled,
    /// Its admission.
    pub admission: modbit_core_runtime::admission::Admission,
    /// What feasibility said, with the versions it was measured under.
    pub feasibility: FeasibilityRecord,
    /// The registry generation it was compiled from.
    pub registry_generation: String,
    /// The stay/switch economics of the compile (REQ-EPR-009).
    pub switch: SwitchRecord,
}

/// Compile the plan for a run (REQ-EPR-004) from the active signed registry,
/// the session's latest statistics snapshot and the mode floor, and admit
/// what the compiler selected.
///
/// Everything the compiler sees is pinned: the registry by its generation and
/// document digest, the statistics by their version, the thresholds by the
/// registry generation that carries the floor. Identical inputs produce an
/// identical plan, and the answer carries every candidate with the reason it
/// was or was not chosen.
///
/// # Errors
/// No registry is active, the registry has no auto floor, the compiler
/// refuses, or admission refuses; each with its own code.
/// Slots of the plans a session compiled, keyed by run and plan id:
/// `(slot_id, endpoint, model)`.
type PlanSlots = std::collections::HashMap<
    (Option<modbit_domain::RunId>, String),
    Vec<(String, String, String)>,
>;

/// The route in force when a compile re-evaluates at a boundary
/// (REQ-EPR-009): the binding and the warm prefix the session holds with
/// it, when any.
#[derive(Clone, Debug)]
pub(crate) struct RouteContext {
    /// Endpoint in force.
    pub endpoint: String,
    /// Model in force.
    pub model: String,
    /// The session's cached prefix with that binding.
    pub cache: Option<modbit_providers::economics::CacheState>,
    /// The plan in force.
    pub plan_id: String,
}

/// What the compile said about staying versus switching, for the record.
#[derive(Clone, Debug)]
pub(crate) struct SwitchRecord {
    /// The comparison against the binding the compiler chose (or, when it
    /// kept the incumbent, against the cheapest feasible alternative).
    pub comparison: Option<modbit_providers::economics::StaySwitch>,
    /// The switch cost the thresholds carried.
    pub switch_cost_minor: u64,
}

/// The demand a re-evaluation prices: the same one leg, at the same prompt
/// size and output ceiling, that the compiler prices a plan's initial slot
/// with — one accounting basis for the saving and for the switch cost
/// (docs/27 §7.6).
fn remaining_demand(expected_input_tokens: u64) -> modbit_providers::economics::RemainingDemand {
    modbit_providers::economics::RemainingDemand {
        input_tokens_per_call: expected_input_tokens,
        output_tokens_per_call: 4_096,
        calls: 1,
    }
}

/// Latency priced at one minor unit per 10 ms of added p50; hysteresis a
/// 5 % margin of the stay cost (docs/27 §7.6: "latency/hysteresis units and
/// conversion policy must be explicit").
const LATENCY_MINOR_PER_MS: u64 = 0;
const HYSTERESIS_BP: u64 = 500;

/// The switch economics of `ctx` against every other solver binding in the
/// registry: the lowest total is the hurdle a cheaper plan must clear.
pub(crate) fn switch_economics(
    registry: &modbit_providers::registry::ModelRegistry,
    ctx: &RouteContext,
    now_ms: i64,
    expected_input_tokens: u64,
) -> Vec<(String, modbit_providers::economics::StaySwitch)> {
    let Some(current) = registry
        .document
        .entries
        .iter()
        .find(|e| e.endpoint == ctx.endpoint && e.model == ctx.model)
    else {
        return vec![];
    };
    registry
        .document
        .entries
        .iter()
        .filter(|e| !(e.endpoint == ctx.endpoint && e.model == ctx.model) && !e.revoked)
        .map(|alt| {
            (
                format!("{}/{}", alt.endpoint, alt.model),
                modbit_providers::economics::compare(
                    current,
                    alt,
                    ctx.cache.as_ref(),
                    now_ms,
                    remaining_demand(expected_input_tokens),
                    LATENCY_MINOR_PER_MS,
                    HYSTERESIS_BP,
                ),
            )
        })
        .collect()
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn compile_for_run(
    core: &Core,
    store: &modbit_event_store::EventStore,
    task: &modbit_domain::task::Task,
    run_id: modbit_domain::RunId,
    lease_generation: u64,
    pin: Option<(String, String)>,
    request_cap_minor: u64,
    context: Option<&RouteContext>,
) -> Result<CompiledForRun, (String, String)> {
    use modbit_bench_outcome_statistics::MIN_CONFIDENT_SAMPLES;
    use modbit_providers::compiler::{CompileInput, Evidence};
    use modbit_providers::feasibility::{LegEvidence, Thresholds};
    let Some(registry) = core.gateway.registry() else {
        return Err((
            "NO_ACTIVE_REGISTRY".into(),
            "no signed registry is active; the direct path is the only plan the product compiles without one".into(),
        ));
    };
    let Some(floor) = registry
        .document
        .quality_floors
        .iter()
        .find(|f| f.mode == "auto")
        .cloned()
    else {
        return Err((
            "NO_MODE_FLOOR".into(),
            "the active registry defines no auto floor".into(),
        ));
    };
    let next_epoch = store
        .routing_plans(&run_id)
        .unwrap_or_default()
        .iter()
        .map(|p| p.routing_epoch)
        .max()
        .map_or(0, |e| e + 1);
    // The statistics the registry pins, when this session has materialized
    // them; otherwise no evidence, which the compiler treats as cold start.
    let snapshot = crate::statistics::latest_snapshot(store, task.session_id).map(|(s, _)| s);
    let evidence = match snapshot.as_ref() {
        Some(s) => Evidence {
            stats_version: s.stats_version.clone(),
            legs: s
                .aggregates
                .iter()
                .map(|a| LegEvidence {
                    key_id: a.key_id.clone(),
                    lcb: a.interval.0,
                    mean: a.mean,
                    samples: a.samples,
                })
                .collect(),
        },
        None => Evidence {
            stats_version: "none".into(),
            legs: vec![],
        },
    };
    // REQ-EPR-009: at a re-evaluation the switch cost is what leaving the
    // binding in force costs once the warm prefix, the re-prefill, the cache
    // write, the latency and the hysteresis margin are counted; the lowest
    // hurdle over the alternatives is what a cheaper plan must clear.
    let expected_input_tokens = 40_000;
    let now_ms = modbit_domain::Timestamp::now().0;
    let economics: Vec<(String, modbit_providers::economics::StaySwitch)> = context
        .map(|ctx| switch_economics(&registry, ctx, now_ms, expected_input_tokens))
        .unwrap_or_default();
    let switch_cost_minor = economics
        .iter()
        .map(|(_, e)| e.switch_cost.total_minor)
        .min()
        .unwrap_or(0);
    let thresholds = Thresholds {
        mode: floor.mode.clone(),
        tau: floor.min_quality,
        delta: 0.05,
        min_samples: MIN_CONFIDENT_SAMPLES,
        switch_cost_minor,
        thresholds_version: registry.generation().to_owned(),
    };
    let cap = if request_cap_minor == 0 {
        floor.max_cost_minor
    } else {
        request_cap_minor
    };
    let money = |m: u64| modbit_domain::routing::Money {
        minor_units: m,
        currency: floor.currency.clone(),
        scale: floor.scale,
    };
    // Assurance: a continuation can only be triggered by a gate with real
    // checks behind it, which the workspace's verification plan decides.
    let assurance_available = task.workspace_root.as_deref().is_some_and(|root| {
        let root = std::path::Path::new(root);
        let plan = modbit_verification::plan::derive(
            root,
            &[],
            &modbit_verification::plan::configured_commands(root),
        );
        !plan.commands.is_empty()
    });
    let input = CompileInput {
        registry: &registry,
        evidence: &evidence,
        thresholds: &thresholds,
        scope: modbit_domain::routing::PlanScope {
            tenant_id: core.tenant_id,
            session_id: task.session_id,
            task_id: task.task_id,
            run_id,
            created_at_ms: task.created_at.0,
        },
        lease_generation,
        routing_epoch: next_epoch,
        needs: modbit_providers::registry::Needs {
            tools: true,
            ..Default::default()
        },
        execution_profile: task.execution_profile.clone(),
        allowed_residencies: vec![],
        request_cap: money(cap),
        verification_reserve: money(cap / 10),
        assurance_available,
        manual_pin: pin,
        harness: crate::baseline::build_digest(),
        policy_version: core.gateway.policy().version(),
        // The profiler is in shadow (REQ-EPR-003): it informs nothing yet, and
        // the plan says so rather than borrowing its version.
        profiler_version: "none".into(),
        gate_version: "gate-1".into(),
        // REQ-EPR-008: the assurance policy the candidate will be judged under.
        risk_version: format!(
            "{}/{}",
            modbit_policy::assurance::REALIZED_RISK_RULES_VERSION,
            core.assurance_policy.version()
        ),
        expected_input_tokens,
        current_binding: context.map(|c| (c.endpoint.clone(), c.model.clone())),
    };
    let compiled = modbit_providers::compiler::compile(&input)
        .map_err(|r| (r.code().to_owned(), format!("{r:?}")))?;
    let chosen = compiled
        .plan
        .initial_slot()
        .map(|s| format!("{}/{}", s.endpoint, s.model))
        .unwrap_or_default();
    let switch = SwitchRecord {
        comparison: economics
            .iter()
            .find(|(label, _)| *label == chosen)
            .or_else(|| economics.iter().min_by_key(|(_, e)| e.switch_minor))
            .map(|(_, e)| e.clone()),
        switch_cost_minor,
    };
    // Admit what was compiled through the same door every plan goes through.
    let admission =
        modbit_core_runtime::admission::admit_plan(&compiled.plan, core.tenant_id, next_epoch)
            .map_err(|r| (r.code().to_owned(), format!("{r:?}")))?;
    let feasibility = FeasibilityRecord {
        code: compiled.selection.code.clone(),
        lcb_bp: bp(compiled.selection.selected_lcb),
        stats_version: evidence.stats_version.clone(),
        thresholds_version: thresholds.thresholds_version.clone(),
        target_met: compiled.selection.target_met,
        detail: String::new(),
    };
    Ok(CompiledForRun {
        compiled,
        admission,
        feasibility,
        registry_generation: registry.generation().to_owned(),
        switch,
    })
}

/// The route the session holds right now (docs/27 §16.1 `RoutingSessionState`,
/// read off the log): the binding of the latest activated slot of the
/// session's latest run, and the warm prefix the provider last reported
/// with it.
pub(crate) fn session_route_context(
    store: &modbit_event_store::EventStore,
    session_id: modbit_domain::SessionId,
    auto_only: bool,
) -> Option<RouteContext> {
    let events = store.read_session(&session_id, 0, usize::MAX).ok()?;
    let mut binding: Option<(String, String, String)> = None;
    let mut plans: PlanSlots = PlanSlots::new();
    let mut cache: Option<modbit_providers::economics::CacheState> = None;
    let mut last_cache_key = String::new();
    // Runs the user pinned by hand: their route is the user's choice for
    // that task, not an incumbent a later auto route inherits.
    let mut pinned_runs: Vec<Option<modbit_domain::RunId>> = Vec::new();
    for e in &events {
        let p = store.payload(&e.envelope).unwrap_or_default();
        match e.envelope.event_type.as_str() {
            "RouteReevaluated"
                if p["decision"] == "INITIAL"
                    && p["reason"]
                        .as_str()
                        .is_some_and(|r| r.starts_with("manual pin")) =>
            {
                pinned_runs.push(e.envelope.run_id);
            }
            "RoutingPlanCompiled" => {
                let plan_id = p["plan"]["plan_id"].as_str().unwrap_or_default().to_owned();
                let slots: Vec<(String, String, String)> = p["plan"]["slots"]
                    .as_array()
                    .map(|a| {
                        a.iter()
                            .map(|s| {
                                (
                                    s["slot_id"].as_str().unwrap_or_default().to_owned(),
                                    s["endpoint"].as_str().unwrap_or_default().to_owned(),
                                    s["model"].as_str().unwrap_or_default().to_owned(),
                                )
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                plans.insert((e.envelope.run_id, plan_id), slots);
            }
            "SlotActivated" => {
                let plan_id = p["plan_id"].as_str().unwrap_or_default().to_owned();
                let slot_id = p["slot_id"].as_str().unwrap_or_default();
                if auto_only && pinned_runs.contains(&e.envelope.run_id) {
                    continue;
                }
                if let Some(slots) = plans.get(&(e.envelope.run_id, plan_id.clone()))
                    && let Some((_, ep, model)) = slots.iter().find(|(s, _, _)| s == slot_id)
                {
                    binding = Some((ep.clone(), model.clone(), plan_id));
                }
            }
            "ModelInvocationStarted" => {
                // The request's cache key: the identity of the prefix the
                // provider's report is about.
                if let Some(k) = p["model_route"]["cache_key"].as_str() {
                    last_cache_key = k.to_owned();
                }
            }
            "ModelUsageRecorded" => {
                // The gateway's route record names the endpoint and the
                // model that was asked for (and, when the provider said so,
                // the one that answered).
                let route = &p["route"];
                let model = route["requested_model"]
                    .as_str()
                    .or_else(|| route["model"].as_str());
                if let (Some(ep), Some(model)) = (route["endpoint"].as_str(), model) {
                    cache = Some(modbit_providers::economics::CacheState {
                        endpoint: ep.to_owned(),
                        model: model.to_owned(),
                        cached_prefix_tokens: p["cached_input_tokens"].as_u64().unwrap_or(0),
                        last_used_at_ms: e.envelope.occurred_at.0,
                        prefix_key: last_cache_key.clone(),
                    });
                }
            }
            _ => {}
        }
    }
    let (endpoint, model, plan_id) = binding?;
    Some(RouteContext {
        cache: cache.filter(|c| c.endpoint == endpoint && c.model == model),
        endpoint,
        model,
        plan_id,
    })
}

/// Why a boundary decided what it did: feasibility first, economics second.
pub(crate) fn decision_reason(c: &CompiledForRun, decision: &str) -> String {
    let economics = c
        .switch
        .comparison
        .as_ref()
        .map(|x| x.reason.clone())
        .unwrap_or_else(|| "no alternative binding".to_owned());
    if c.compiled.selection.code != "FEASIBLE" {
        return format!(
            "no confidence-feasible alternative ({}); the route in force stands; economics: {economics}",
            c.compiled.selection.code
        );
    }
    match decision {
        "STAY" => format!("the plan in force is kept; economics: {economics}"),
        "SWITCH" => format!(
            "a confidence-feasible alternative clears the switch cost; economics: {economics}"
        ),
        _ => economics,
    }
}

/// The `RouteReevaluated` record of one boundary decision.
#[allow(clippy::too_many_arguments)]
pub(crate) fn reevaluated_event(
    boundary: &str,
    route_epoch: u64,
    context: Option<&RouteContext>,
    chosen: &str,
    decision: &str,
    reason: String,
    switch: Option<&SwitchRecord>,
    plan_id: &str,
    actor: modbit_domain::event::Actor,
) -> modbit_event_store::NewEvent {
    let comparison = switch.and_then(|s| s.comparison.as_ref());
    crate::runtime::typed(
        "RouteReevaluated",
        &modbit_domain::run::RunEvent::RouteReevaluated {
            boundary: boundary.into(),
            route_epoch,
            current: context
                .map(|c| format!("{}/{}", c.endpoint, c.model))
                .unwrap_or_default(),
            chosen: chosen.into(),
            decision: decision.into(),
            reason,
            stay_minor: comparison.map(|c| c.stay_minor).unwrap_or(0),
            switch_minor: comparison.map(|c| c.switch_minor).unwrap_or(0),
            switch_cost: comparison
                .map(|c| serde_json::to_value(&c.switch_cost).unwrap_or_default())
                .unwrap_or_else(|| serde_json::json!({"total_minor": switch.map(|s| s.switch_cost_minor).unwrap_or(0)})),
            cache_state: context
                .and_then(|c| c.cache.as_ref())
                .map(|c| serde_json::to_value(c).unwrap_or_default()),
            plan_id: plan_id.into(),
            economics_version: modbit_providers::economics::ECONOMICS_VERSION.into(),
        },
        actor,
    )
}

/// The events that record a compiled plan and its admission.
pub(crate) fn compiled_events(
    c: &CompiledForRun,
    actor: modbit_domain::event::Actor,
) -> Vec<modbit_event_store::NewEvent> {
    let plan_id = c.compiled.plan.plan_id.clone();
    let plan_ref = modbit_domain::routing::plan_digest(&c.compiled.plan);
    vec![
        crate::runtime::typed(
            "RoutingPlanCompiled",
            &modbit_domain::run::RunEvent::RoutingPlanCompiled {
                plan: Box::new(c.compiled.plan.clone()),
                plan_ref,
            },
            actor.clone(),
        ),
        crate::runtime::typed(
            "RoutingPlanAdmitted",
            &modbit_domain::run::RunEvent::RoutingPlanAdmitted {
                plan_id,
                validation_digest: c.admission.validation_digest.clone(),
                reserved_minor: c.admission.reserved.minor_units,
                currency: c.admission.reserved.currency.clone(),
                scale: c.admission.reserved.scale,
                lease_generation: c.admission.lease_generation,
                feasibility: c.feasibility.code.clone(),
                quality_lcb_bp: c.feasibility.lcb_bp,
                stats_version: c.feasibility.stats_version.clone(),
                thresholds_version: c.feasibility.thresholds_version.clone(),
                target_met: c.feasibility.target_met,
            },
            actor,
        ),
    ]
}

/// Compile the plan for a task's current run through the surface protocol
/// (REQ-EPR-004) and install it.
pub(crate) async fn compile(
    core: &Core,
    task_id: TaskId,
    pin: Option<(String, String)>,
    request_cap_minor: u64,
) -> wire::RoutingCompileView {
    let refuse = |code: &str, detail: String| wire::RoutingCompileView {
        compiled: false,
        refusal_code: code.to_owned(),
        refusal_detail: detail,
        ..Default::default()
    };
    let task = match core.store.lock().await.task(&task_id) {
        Ok(Some(t)) => t,
        Ok(None) => return refuse("UNKNOWN_TASK", task_id.to_string()),
        Err(e) => return refuse("STORE", e.to_string()),
    };
    let mut store = core.store.lock().await;
    let Some(run) = store
        .runs_for_task(&task_id)
        .ok()
        .and_then(|runs| runs.into_iter().next_back())
    else {
        return refuse("NO_RUN", "the task has no run to compile a plan for".into());
    };
    let c = match compile_for_run(
        core,
        &store,
        &task,
        run.run_id,
        run.kernel_lease_generation,
        pin,
        request_cap_minor,
        None,
    ) {
        Ok(c) => c,
        Err((code, detail)) => return refuse(&code, detail),
    };
    let plan_id = c.compiled.plan.plan_id.clone();
    if let Err(e) = crate::runtime::append(
        &mut store,
        core,
        crate::runtime::Lineage::run(core.tenant_id, task.session_id, task_id, run.run_id),
        modbit_domain::event::AggregateType::Run,
        *run.run_id.as_bytes(),
        compiled_events(&c, modbit_domain::event::Actor::Core("compiler".into())),
    ) {
        return refuse("STORE", e.to_string());
    }
    wire::RoutingCompileView {
        compiled: true,
        plan_id: plan_id.clone(),
        content_digest: c.compiled.plan.content_digest.clone(),
        input_digest: c.compiled.input_digest.clone(),
        selection_code: c.compiled.selection.code.clone(),
        target_met: c.compiled.selection.target_met,
        registry_generation: c.registry_generation.clone(),
        stats_version: c.feasibility.stats_version.clone(),
        thresholds_version: c.feasibility.thresholds_version.clone(),
        compiler_version: modbit_providers::compiler::COMPILER_VERSION.into(),
        candidates: c
            .compiled
            .candidates
            .iter()
            .map(|k| wire::RoutingCandidateView {
                plan_id: k.plan_id.clone(),
                bindings: k.bindings.clone(),
                worst_case_cost_minor: k.worst_case_cost_minor,
                expected_cost_minor: k.expected_cost_minor,
                quality_lcb_bp: bp(k.quality.lcb),
                confident: k.quality.confident,
                missing_evidence: k.quality.missing.clone(),
                hard_eligible: k.hard_eligible,
                ineligible_reason: k.ineligible_reason.clone(),
            })
            .collect(),
        exclusions: c
            .compiled
            .selection
            .exclusions
            .iter()
            .map(|e| format!("{}: {}", e.plan_id, e.reason))
            .collect(),
        admission: admission_view(&store, run.run_id, &plan_id),
        refusal_code: String::new(),
        refusal_detail: String::new(),
    }
}

/// docs/27 §16.1 `RoutingSessionState`, read off the log for the wire.
pub(crate) fn session_state(
    store: &modbit_event_store::EventStore,
    session_id: modbit_domain::SessionId,
) -> Option<modbit_protocol::v1::RoutingSessionStateView> {
    use modbit_protocol::v1 as wire;
    let session = store.session(&session_id).ok()??;
    let events = store.read_session(&session_id, 0, usize::MAX).ok()?;
    let ctx = session_route_context(store, session_id, false);
    let mut per_run: Vec<(modbit_domain::RunId, Vec<String>)> = Vec::new();
    let mut plans: PlanSlots = PlanSlots::new();
    let mut last_profile_ref = String::new();
    let mut last_route_at = 0i64;
    let mut route_epoch = 0u64;
    let mut decisions = Vec::new();
    for e in &events {
        let p = store.payload(&e.envelope).unwrap_or_default();
        match e.envelope.event_type.as_str() {
            "RoutingPlanCompiled" => {
                let plan_id = p["plan"]["plan_id"].as_str().unwrap_or_default().to_owned();
                let slots: Vec<(String, String, String)> = p["plan"]["slots"]
                    .as_array()
                    .map(|a| {
                        a.iter()
                            .map(|s| {
                                (
                                    s["slot_id"].as_str().unwrap_or_default().to_owned(),
                                    s["endpoint"].as_str().unwrap_or_default().to_owned(),
                                    s["model"].as_str().unwrap_or_default().to_owned(),
                                )
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                plans.insert((e.envelope.run_id, plan_id), slots);
                route_epoch = route_epoch.max(p["plan"]["routing_epoch"].as_u64().unwrap_or(0));
                last_route_at = e.envelope.occurred_at.0;
            }
            "SlotActivated" => {
                let plan_id = p["plan_id"].as_str().unwrap_or_default().to_owned();
                let slot_id = p["slot_id"].as_str().unwrap_or_default();
                if let (Some(run), Some(slots)) =
                    (e.envelope.run_id, plans.get(&(e.envelope.run_id, plan_id)))
                    && let Some((_, ep, model)) = slots.iter().find(|(s, _, _)| s == slot_id)
                {
                    let label = format!("{ep}/{model}");
                    match per_run.iter_mut().find(|(r, _)| *r == run) {
                        Some((_, labels)) => {
                            if labels.last() != Some(&label) {
                                labels.push(label);
                            }
                        }
                        None => per_run.push((run, vec![label])),
                    }
                }
            }
            "RequestProfiled" => {
                last_profile_ref = p["features_digest"].as_str().unwrap_or_default().to_owned();
            }
            "RouteReevaluated" => decisions.push(wire::RouteDecisionView {
                boundary: p["boundary"].as_str().unwrap_or_default().into(),
                route_epoch: p["route_epoch"].as_u64().unwrap_or(0),
                current: p["current"].as_str().unwrap_or_default().into(),
                chosen: p["chosen"].as_str().unwrap_or_default().into(),
                decision: p["decision"].as_str().unwrap_or_default().into(),
                reason: p["reason"].as_str().unwrap_or_default().into(),
                stay_minor: p["stay_minor"].as_u64().unwrap_or(0),
                switch_minor: p["switch_minor"].as_u64().unwrap_or(0),
                switch_cost_minor: p["switch_cost"]["total_minor"].as_u64().unwrap_or(0),
                switch_cost_json: p["switch_cost"].to_string(),
                cache_state_json: if p["cache_state"].is_null() {
                    String::new()
                } else {
                    p["cache_state"].to_string()
                },
                plan_id: p["plan_id"].as_str().unwrap_or_default().into(),
                offset: e.offset,
                run_id: e.envelope.run_id.map(|r| wire::Id {
                    value: r.as_bytes().to_vec(),
                }),
            }),
            _ => {}
        }
    }
    Some(wire::RoutingSessionStateView {
        session_id: Some(wire::Id {
            value: session_id.as_bytes().to_vec(),
        }),
        active_endpoint: ctx.as_ref().map(|c| c.endpoint.clone()).unwrap_or_default(),
        active_model: ctx.as_ref().map(|c| c.model.clone()).unwrap_or_default(),
        active_plan_id: ctx.as_ref().map(|c| c.plan_id.clone()).unwrap_or_default(),
        executed_path_labels: per_run
            .iter()
            .map(|(_, labels)| labels.join(" -> "))
            .collect(),
        cache_state: ctx
            .as_ref()
            .and_then(|c| c.cache.as_ref())
            .map(|c| wire::CacheStateView {
                endpoint: c.endpoint.clone(),
                model: c.model.clone(),
                cached_prefix_tokens: c.cached_prefix_tokens,
                last_used_at_ms: c.last_used_at_ms,
                prefix_key: c.prefix_key.clone(),
            }),
        last_profile_ref,
        last_route_at_ms: last_route_at,
        route_epoch,
        branch_generation: session.branch_generation,
        decisions,
    })
}
