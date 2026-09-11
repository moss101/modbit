//! The conditional plan compiler (REQ-EPR-004, docs/27 and docs/38 §5): from
//! the signed registry, the pinned statistics and the mode's floor, enumerate
//! the bounded conditional plans this request could run, prevalidate every
//! one of them before anything dispatches, and choose by confidence-adjusted
//! feasibility and then by lowest expected complete cost.
//!
//! Nothing here is a template. A plan is enumerated from parameters — which
//! live solver bindings may open it, which stronger binding may continue it
//! on a quality rejection — and the label a plan later earns (`DIRECT`,
//! `CASCADE`) is derived from what actually ran, never dispatched by name.
//! The runtime activates the slots a compiled plan declares and cannot add
//! one; a gate asking for a topology the plan does not contain is refused by
//! admission (REQ-EPR-014).
//!
//! Every candidate the compiler considered is in the output with the reason
//! it was or was not chosen, so a reader can check the decision against its
//! inputs. Identical inputs produce an identical plan and an identical
//! digest.

use modbit_domain::routing::{
    Budget, ConditionalExecutionPlan, Money, PlanScope, Provenance, ROUTING_SCHEMA_VERSION, Slot,
    Trigger,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::feasibility::{
    Candidate, Exclusion, LegEvidence, PlanQuality, Selection, Thresholds, plan_quality, select,
};
use crate::registry::{ModelRegistry, Needs, RegistryEntry};

/// The compiler version every plan it produces pins.
pub const COMPILER_VERSION: &str = "compiler-2";

/// Evidence the compiler may consult, already joined to pinned versions by the
/// caller (docs/38: the compiler joins the registry and the statistics
/// independently; it never reads either from anywhere but its inputs).
#[derive(Clone, Debug, PartialEq)]
pub struct Evidence {
    /// The statistics version the evidence came from, or `none`.
    pub stats_version: String,
    /// Per-key observations, as `(key_id, lcb, mean, samples)`.
    pub legs: Vec<LegEvidence>,
}

impl Evidence {
    fn for_key(&self, key_id: &str) -> Option<&LegEvidence> {
        self.legs.iter().find(|l| l.key_id == key_id)
    }
}

/// What the compiler is asked to compile.
#[derive(Clone, Debug, PartialEq)]
pub struct CompileInput<'a> {
    /// The active registry.
    pub registry: &'a ModelRegistry,
    /// The evidence, pinned.
    pub evidence: &'a Evidence,
    /// The mode's thresholds, pinned.
    pub thresholds: &'a Thresholds,
    /// Where the plan will live.
    pub scope: PlanScope,
    /// The lease generation compiling it.
    pub lease_generation: u64,
    /// The routing epoch it is compiled in.
    pub routing_epoch: u64,
    /// What the request needs of a binding.
    pub needs: Needs,
    /// The execution profile the task runs under.
    pub execution_profile: String,
    /// Data residencies policy allows; empty means any.
    pub allowed_residencies: Vec<String>,
    /// The request's spending cap.
    pub request_cap: Money,
    /// Money held back for verification, inside the cap.
    pub verification_reserve: Money,
    /// Whether an acceptance gate with real assurance is available for this
    /// request (a configured verification command, a check to run). Without
    /// one, no continuation can be triggered honestly, so no conditional plan
    /// is eligible.
    pub assurance_available: bool,
    /// A manual pin: only plans whose initial slot is this binding are
    /// considered. Policy still applies to the pin.
    pub manual_pin: Option<(String, String)>,
    /// The harness identity, for the statistics key.
    pub harness: String,
    /// Versions the plan records as provenance.
    pub policy_version: String,
    /// Profiler version, or `none` when the profile was not consulted.
    pub profiler_version: String,
    /// Gate version.
    pub gate_version: String,
    /// Risk version.
    pub risk_version: String,
    /// Tokens the initial leg is expected to read, for the worst-case price.
    pub expected_input_tokens: u64,
    /// The binding in force when this compile re-evaluates a route at a
    /// boundary (REQ-EPR-009): the candidate opening with it is kept
    /// unless a cheaper feasible one saves at least the thresholds' switch
    /// cost. `None` at a fresh start (nothing to switch from).
    pub current_binding: Option<(String, String)>,
}

/// One candidate the compiler considered, with everything a reader needs to
/// check what was decided about it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CandidateRecord {
    /// Plan id.
    pub plan_id: String,
    /// Slots as `endpoint/model` in order.
    pub bindings: Vec<String>,
    /// Worst-case complete cost, in minor units: what the cap is checked
    /// against.
    pub worst_case_cost_minor: u64,
    /// Expected complete cost, in minor units: the initial leg, plus each
    /// continuation weighted by the lower bound of its being needed, plus
    /// the verification reserve. What plans are ranked by.
    pub expected_cost_minor: u64,
    /// What it can be expected to do.
    pub quality: PlanQuality,
    /// Whether budget, policy and prevalidation allow it at all.
    pub hard_eligible: bool,
    /// Why not, when not.
    pub ineligible_reason: String,
}

/// What the compiler produced.
#[derive(Clone, Debug, PartialEq)]
pub struct Compiled {
    /// The plan to admit.
    pub plan: ConditionalExecutionPlan,
    /// What the selector said about it.
    pub selection: Selection,
    /// Every candidate considered.
    pub candidates: Vec<CandidateRecord>,
    /// Digest of the exact inputs, so identical inputs are provably identical.
    pub input_digest: String,
}

/// Why nothing could be compiled.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "refusal", rename_all = "snake_case")]
pub enum CompileRefused {
    /// No binding is eligible at all, so there is no plan to compile.
    NoEligibleBinding {
        /// Every binding considered and why it was excluded.
        exclusions: Vec<Exclusion>,
    },
    /// The manual pin names a binding that is not eligible.
    PinNotEligible {
        /// The pin.
        pin: String,
        /// Why.
        reason: String,
    },
    /// The statistics evidence is not the version the registry pins.
    StaleStatistics {
        /// What the registry pins.
        pinned: String,
        /// What was supplied.
        found: String,
    },
    /// A price does not fit in the plan's integers.
    Overflow {
        /// Where.
        field: String,
    },
    /// The cap does not cover even the verification reserve.
    InsufficientCap {
        /// The cap.
        cap: u64,
        /// The reserve.
        reserve: u64,
    },
}

impl CompileRefused {
    /// The stable code a client sees.
    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            Self::NoEligibleBinding { .. } => "NO_ELIGIBLE_BINDING",
            Self::PinNotEligible { .. } => "PIN_NOT_ELIGIBLE",
            Self::StaleStatistics { .. } => "STALE_STATISTICS",
            Self::Overflow { .. } => "OVERFLOW",
            Self::InsufficientCap { .. } => "INSUFFICIENT_CAP",
        }
    }
}

fn digest(parts: &[&str]) -> String {
    let mut h = Sha256::new();
    for p in parts {
        h.update(p.as_bytes());
        h.update([0]);
    }
    hex::encode(h.finalize())
}

/// The worst-case price of one leg: every expected input token and every
/// output token the slot may produce, at the binding's current prices.
///
/// # Errors
/// The product does not fit in `u64`.
fn worst_case_minor(
    e: &RegistryEntry,
    input_tokens: u64,
    output_tokens: u64,
) -> Result<u64, CompileRefused> {
    let overflow = |field: &str| CompileRefused::Overflow {
        field: field.to_owned(),
    };
    let input = input_tokens
        .checked_mul(e.economics.input_per_mtok_minor)
        .ok_or_else(|| overflow("input"))?
        .div_ceil(1_000_000);
    let output = output_tokens
        .checked_mul(e.economics.output_per_mtok_minor)
        .ok_or_else(|| overflow("output"))?
        .div_ceil(1_000_000);
    input.checked_add(output).ok_or_else(|| overflow("leg"))
}

/// Why a binding cannot open or continue a plan for this request, when it
/// cannot.
fn binding_exclusion(e: &RegistryEntry, input: &CompileInput<'_>, role: &str) -> Option<String> {
    if e.revoked {
        return Some("revoked in the active registry".into());
    }
    if !e.roles.iter().any(|r| r == role) {
        return Some(format!("not bound to the {role} role"));
    }
    if !input.allowed_residencies.is_empty()
        && !input
            .allowed_residencies
            .contains(&e.governance.data_residency)
    {
        return Some(format!(
            "data residency {} is not allowed for this request",
            e.governance.data_residency
        ));
    }
    if !e.governance.allowed_profiles.is_empty()
        && !e
            .governance
            .allowed_profiles
            .contains(&input.execution_profile)
    {
        return Some(format!(
            "governance does not allow the {} profile",
            input.execution_profile
        ));
    }
    if e.economics.currency != input.request_cap.currency
        || e.economics.scale != input.request_cap.scale
    {
        return Some(format!(
            "priced in {} at scale {}, not the request's {} at scale {}",
            e.economics.currency,
            e.economics.scale,
            input.request_cap.currency,
            input.request_cap.scale
        ));
    }
    let needs = &input.needs;
    if needs.tools && !e.tools {
        return Some("cannot call tools".into());
    }
    if needs.vision && !e.vision {
        return Some("does not accept images".into());
    }
    if needs.structured_output && !e.structured_output {
        return Some("cannot be held to a schema".into());
    }
    if e.context_tokens < needs.min_context_tokens {
        return Some(format!(
            "context of {} tokens is below the {} the request needs",
            e.context_tokens, needs.min_context_tokens
        ));
    }
    None
}

/// Compile the plan for one request.
///
/// # Errors
/// The evidence is stale, no binding is eligible, the pin is not eligible, a
/// price overflows, or the cap does not cover the verification reserve.
pub fn compile(input: &CompileInput<'_>) -> Result<Compiled, CompileRefused> {
    let registry = input.registry;
    let pinned = registry.stats_version();
    if input.evidence.stats_version != "none" && input.evidence.stats_version != pinned {
        return Err(CompileRefused::StaleStatistics {
            pinned: pinned.to_owned(),
            found: input.evidence.stats_version.clone(),
        });
    }
    if input.verification_reserve.minor_units > input.request_cap.minor_units {
        return Err(CompileRefused::InsufficientCap {
            cap: input.request_cap.minor_units,
            reserve: input.verification_reserve.minor_units,
        });
    }
    let thresholds_json = serde_json::to_string(input.thresholds).unwrap_or_default();
    let needs = format!("{:?}", input.needs);
    let residencies = input.allowed_residencies.join(",");
    let cap = format!("{:?}", input.request_cap);
    let reserve = format!("{:?}", input.verification_reserve);
    let assurance = input.assurance_available.to_string();
    let pin = format!("{:?}", input.manual_pin);
    let expected = input.expected_input_tokens.to_string();
    let mut parts: Vec<&str> = vec![
        &registry.document_digest,
        &input.evidence.stats_version,
        &thresholds_json,
        &needs,
        &input.execution_profile,
        &residencies,
        &cap,
        &reserve,
        &assurance,
        &pin,
        &input.harness,
        &expected,
    ];
    // Only a re-evaluation carries a binding in force; a fresh compile
    // digests exactly as before the field existed.
    let current = input
        .current_binding
        .as_ref()
        .map(|(e, m)| format!("current:{e}/{m}"));
    if let Some(c) = &current {
        parts.push(c);
    }
    let input_digest = digest(&parts);
    let zero = Money::zero(&input.request_cap.currency, input.request_cap.scale);
    let money = |m: u64| Money {
        minor_units: m,
        currency: input.request_cap.currency.clone(),
        scale: input.request_cap.scale,
    };
    // 1. Which bindings may open a plan, and why the others may not.
    let mut exclusions: Vec<Exclusion> = Vec::new();
    let mut openers: Vec<&RegistryEntry> = Vec::new();
    for e in &registry.document.entries {
        let name = format!("{}/{}", e.endpoint, e.model);
        if let Some(pin) = &input.manual_pin
            && (pin.0 != e.endpoint || pin.1 != e.model)
        {
            exclusions.push(Exclusion {
                plan_id: name,
                reason: format!("not the manual pin {}/{}", pin.0, pin.1),
            });
            continue;
        }
        match binding_exclusion(e, input, "solver") {
            Some(reason) => exclusions.push(Exclusion {
                plan_id: name,
                reason,
            }),
            None => openers.push(e),
        }
    }
    if openers.is_empty() {
        if let Some(pin) = &input.manual_pin {
            let pin_name = format!("{}/{}", pin.0, pin.1);
            let reason = exclusions
                .iter()
                .find(|x| x.plan_id == pin_name)
                .map_or_else(
                    || "not bound in the active registry".to_owned(),
                    |x| x.reason.clone(),
                );
            return Err(CompileRefused::PinNotEligible {
                pin: pin_name,
                reason,
            });
        }
        return Err(CompileRefused::NoEligibleBinding { exclusions });
    }
    // 2. Enumerate the bounded plans: one slot per opener, and one
    //    continuation per stronger solver when assurance can trigger it.
    let mut candidates: Vec<(ConditionalExecutionPlan, CandidateRecord)> = Vec::new();
    let output_tokens = |e: &RegistryEntry| u64::from(e.max_output_tokens.min(4096));
    let make_slot =
        |id: &str, e: &RegistryEntry, pred: Option<&str>, trigger: Trigger, reserved: u64| Slot {
            slot_id: id.to_owned(),
            predecessor: pred.map(str::to_owned),
            trigger,
            max_activations: 1,
            endpoint: e.endpoint.clone(),
            model: e.model.clone(),
            role: "solver".into(),
            budget: Budget {
                timeout_ms: 120_000,
                max_output_tokens: e.max_output_tokens.min(4096),
                max_retries: 1,
                reserved: money(reserved),
            },
        };
    let provenance = Provenance {
        policy_version: input.policy_version.clone(),
        registry_generation: registry.generation().to_owned(),
        profiler_version: input.profiler_version.clone(),
        statistics_version: input.evidence.stats_version.clone(),
        compiler_version: COMPILER_VERSION.into(),
        gate_version: input.gate_version.clone(),
        risk_version: input.risk_version.clone(),
        legacy_decode: None,
    };
    let scope = &input.scope;
    let solver_key = |e: &RegistryEntry| format!("solver|{}|none|{}", e.model, input.harness);
    let escalation_key = |a: &RegistryEntry, b: &RegistryEntry| {
        format!(
            "escalation|{}|{}|acceptance|workspace|configured",
            a.model, b.model
        )
    };
    for opener in &openers {
        let first_cost =
            worst_case_minor(opener, input.expected_input_tokens, output_tokens(opener))?;
        let mut shapes: Vec<(String, Vec<Slot>, Option<&RegistryEntry>)> = vec![(
            format!("{}/{}", opener.endpoint, opener.model),
            vec![make_slot(
                "initial",
                opener,
                None,
                Trigger::Initial,
                first_cost,
            )],
            None,
        )];
        if input.assurance_available {
            for stronger in &registry.document.entries {
                if stronger.endpoint == opener.endpoint && stronger.model == opener.model {
                    continue;
                }
                if let Some(reason) = binding_exclusion(stronger, input, "solver") {
                    exclusions.push(Exclusion {
                        plan_id: format!(
                            "{}/{} -> {}/{}",
                            opener.endpoint, opener.model, stronger.endpoint, stronger.model
                        ),
                        reason: format!(
                            "continuation {}/{}: {reason}",
                            stronger.endpoint, stronger.model
                        ),
                    });
                    continue;
                }
                let second_cost = worst_case_minor(
                    stronger,
                    input.expected_input_tokens,
                    output_tokens(stronger),
                )?;
                // A continuation is a stronger binding: one that costs more.
                // A cheaper "continuation" is not an escalation, it is a
                // different opener, and it is enumerated as one.
                if second_cost <= first_cost {
                    continue;
                }
                shapes.push((
                    format!(
                        "{}/{} -> {}/{}",
                        opener.endpoint, opener.model, stronger.endpoint, stronger.model
                    ),
                    vec![
                        make_slot("initial", opener, None, Trigger::Initial, first_cost),
                        make_slot(
                            "stronger",
                            stronger,
                            Some("initial"),
                            Trigger::QualityRejected,
                            second_cost,
                        ),
                    ],
                    Some(stronger),
                ));
            }
        }
        for (label, slots, continuation) in shapes {
            let reserved: u64 = slots.iter().map(|s| s.budget.reserved.minor_units).sum();
            let worst_case = reserved
                .checked_add(input.verification_reserve.minor_units)
                .ok_or_else(|| CompileRefused::Overflow {
                    field: "worst_case".into(),
                })?;
            let plan_id = format!("compiled:{}", &digest(&[&input_digest, &label])[..24]);
            let mut plan = ConditionalExecutionPlan {
                schema_version: ROUTING_SCHEMA_VERSION,
                plan_id: plan_id.clone(),
                tenant_id: scope.tenant_id,
                session_id: scope.session_id,
                task_id: scope.task_id,
                run_id: scope.run_id,
                routing_epoch: input.routing_epoch,
                lease_generation: input.lease_generation,
                created_at_ms: scope.created_at_ms,
                provenance: provenance.clone(),
                input_digest: input_digest.clone(),
                slots,
                max_total_attempts: 4,
                max_revisions: if continuation.is_some() { 1 } else { 0 },
                verification_reserve: input.verification_reserve.clone(),
                total_budget: money(worst_case.max(zero.minor_units)),
                content_digest: String::new(),
            };
            let (hard_eligible, ineligible_reason) = if worst_case > input.request_cap.minor_units {
                (
                    false,
                    format!(
                        "worst-case cost {worst_case} exceeds the request cap {}",
                        input.request_cap.minor_units
                    ),
                )
            } else {
                // The plan's cap is the request's: no slot gets a new budget.
                plan.total_budget = input.request_cap.clone();
                match plan.validate(scope.tenant_id, input.routing_epoch) {
                    Ok(()) => (true, String::new()),
                    Err(e) => (false, format!("prevalidation: {e:?}")),
                }
            };
            let plan = plan.sealed();
            let initial_key = solver_key(opener);
            let continuation_key = continuation.map(|c| escalation_key(opener, c));
            let quality = plan_quality(
                input.evidence.for_key(&initial_key),
                continuation_key
                    .as_deref()
                    .and_then(|k| input.evidence.for_key(k)),
                &initial_key,
                continuation_key.as_deref(),
                input.thresholds,
            );
            // The expected complete cost: the continuation is paid only when
            // the initial leg is rejected, and the lower bound of the initial
            // leg's success is the conservative estimate of how often that is
            // not. With no evidence the continuation is expected in full.
            let expected = {
                let first = plan.slots[0].budget.reserved.minor_units;
                let rest: u64 = plan.slots[1..]
                    .iter()
                    .map(|s| s.budget.reserved.minor_units)
                    .sum();
                let p_first = input
                    .evidence
                    .for_key(&initial_key)
                    .filter(|l| l.samples >= input.thresholds.min_samples)
                    .map_or(0.0, |l| l.lcb.clamp(0.0, 1.0));
                let weighted = ((1.0 - p_first) * rest as f64).ceil();
                let weighted = if weighted.is_finite() && weighted >= 0.0 {
                    weighted as u64
                } else {
                    rest
                };
                first
                    .saturating_add(weighted)
                    .saturating_add(input.verification_reserve.minor_units)
            };
            candidates.push((
                plan,
                CandidateRecord {
                    plan_id,
                    bindings: label.split(" -> ").map(str::to_owned).collect(),
                    worst_case_cost_minor: worst_case,
                    expected_cost_minor: expected,
                    quality,
                    hard_eligible,
                    ineligible_reason,
                },
            ));
        }
    }
    // 3. Feasibility first, then lowest expected complete cost; at a
    //    re-evaluation the plan opening with the binding in force is the
    //    incumbent (docs/27 §7.6: switch only when the saving clears the
    //    switch cost).
    let incumbent: Option<String> = input.current_binding.as_ref().and_then(|(e, m)| {
        let label = format!("{e}/{m}");
        candidates
            .iter()
            .filter(|(_, c)| c.bindings.first() == Some(&label))
            .map(|(_, c)| c.plan_id.clone())
            .next()
    });
    let selection = select(
        &candidates
            .iter()
            .map(|(_, c)| Candidate {
                plan_id: c.plan_id.clone(),
                worst_case_cost_minor: c.worst_case_cost_minor,
                expected_cost_minor: c.expected_cost_minor,
                quality: c.quality.clone(),
                hard_eligible: c.hard_eligible,
                ineligible_reason: c.ineligible_reason.clone(),
            })
            .collect::<Vec<_>>(),
        input.thresholds,
        incumbent.as_deref(),
    );
    let Some(chosen) = selection.selected.as_ref() else {
        let mut all = exclusions;
        all.extend(selection.exclusions);
        return Err(CompileRefused::NoEligibleBinding { exclusions: all });
    };
    let (plan, _) = candidates
        .iter()
        .find(|(p, _)| &p.plan_id == chosen)
        .cloned()
        .expect("the selection names a candidate");
    let mut selection = selection;
    selection.exclusions.extend(exclusions);
    Ok(Compiled {
        plan,
        selection,
        candidates: candidates.into_iter().map(|(_, c)| c).collect(),
        input_digest,
    })
}
