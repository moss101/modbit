//! Plan admission (REQ-EPR-014, docs/38 §5 and §9): the one place a
//! conditional plan is validated before anything dispatches, and the one place
//! a slot is allowed to activate.
//!
//! Three rules decide everything here.
//!
//! A plan is admitted whole or not at all. Every slot is validated before the
//! initial leg dispatches, so a continuation can never fail validation halfway
//! through a transaction, and the admitted plan is immutable from then on.
//!
//! The runtime activates slots; it never adds one. A continuation that is not
//! in the admitted plan is refused as `REQUIRED_CONTINUATION_UNAVAILABLE`
//! rather than synthesised, and a template label from a legacy shape
//! authorizes nothing at all.
//!
//! No slot receives a new budget. What a slot may spend is the request cap
//! minus what is already spent, minus what is reserved in flight, minus the
//! verification reserve — so a restart that replays an activation cannot widen
//! the request's ceiling.

use modbit_domain::TenantId;
use modbit_domain::routing::{ConditionalExecutionPlan, Invalid, Money, Trigger};
use serde::{Deserialize, Serialize};

/// Why admission refused. Every variant names what was asked for and what the
/// admitted plan actually allows, so a caller can report it without guessing.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "refusal", rename_all = "snake_case")]
pub enum Refused {
    /// The plan itself does not validate.
    Invalid(Invalid),
    /// A legacy template label was offered as authority for slots the legacy
    /// record never described.
    LegacyTemplate {
        /// The label that was leaned on.
        label: String,
    },
    /// The continuation asked for is not in the admitted plan.
    RequiredContinuationUnavailable {
        /// The slot asked for, or the trigger that has no slot.
        wanted: String,
        /// The continuations the plan does admit.
        admitted: Vec<String>,
    },
    /// The slot has already activated as many times as the plan allows.
    ActivationsExhausted {
        /// Slot.
        slot_id: String,
        /// Ceiling.
        max_activations: u32,
    },
    /// This exact activation is already recorded: a replay after a restart
    /// recovers it rather than charging for it twice.
    DuplicateActivation {
        /// Slot.
        slot_id: String,
        /// Activation ordinal.
        activation: u32,
    },
    /// The whole transaction has used its attempts.
    AttemptsExhausted {
        /// Ceiling.
        max_total_attempts: u32,
        /// Attempts already made.
        attempts: u32,
    },
    /// What is left of the request cap does not cover the slot's reservation
    /// once the verification reserve is held back.
    AllowanceExceeded {
        /// Slot.
        slot_id: String,
        /// What the slot needs.
        needs: Money,
        /// What the request has left for it.
        allowance: Money,
    },
}

impl Refused {
    /// The stable code a client sees.
    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            Self::Invalid(_) => "PLAN_INVALID",
            Self::LegacyTemplate { .. } => "LEGACY_TEMPLATE_AUTHORIZES_NOTHING",
            Self::RequiredContinuationUnavailable { .. } => "REQUIRED_CONTINUATION_UNAVAILABLE",
            Self::ActivationsExhausted { .. } => "ACTIVATIONS_EXHAUSTED",
            Self::DuplicateActivation { .. } => "DUPLICATE_ACTIVATION",
            Self::AttemptsExhausted { .. } => "ATTEMPTS_EXHAUSTED",
            Self::AllowanceExceeded { .. } => "ALLOWANCE_EXCEEDED",
        }
    }
}

/// An admitted plan: what was validated, and what it reserves.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Admission {
    /// The plan's id.
    pub plan_id: String,
    /// Digest of exactly what was validated, so a later reader can tell that
    /// the admitted plan is the plan that ran.
    pub validation_digest: String,
    /// Money reserved across every slot plus the verification reserve.
    pub reserved: Money,
    /// The epoch it was admitted in.
    pub routing_epoch: u64,
    /// The lease generation that admitted it; a stale generation is fenced out
    /// by the caller before it reaches here.
    pub lease_generation: u64,
}

/// What the run has already done, as the projection holds it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RunLedger {
    /// Activations per slot, by slot id.
    pub activations: Vec<(String, u32)>,
    /// Attempts made across the whole transaction.
    pub attempts: u32,
    /// Money already spent.
    pub spent: Money,
    /// Money reserved by activations still in flight.
    pub in_flight: Money,
}

impl RunLedger {
    /// A run that has done nothing yet, in the plan's currency. There is no
    /// default: money without a currency is not money.
    #[must_use]
    pub fn empty(currency: &str, scale: u8) -> Self {
        Self {
            activations: vec![],
            attempts: 0,
            spent: Money::zero(currency, scale),
            in_flight: Money::zero(currency, scale),
        }
    }

    fn activations_of(&self, slot_id: &str) -> u32 {
        self.activations
            .iter()
            .find(|(s, _)| s == slot_id)
            .map_or(0, |(_, n)| *n)
    }
}

/// One admitted activation of one slot.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Activation {
    /// Slot.
    pub slot_id: String,
    /// Ordinal within the slot, 1-based.
    pub activation: u32,
    /// What this activation reserves.
    pub reserved: Money,
}

/// Validate a plan whole and compute its reservation (docs/38 §5.2: validate
/// every slot before the initial dispatch, then persist plan, slot table,
/// validation digest and reservation together).
///
/// # Errors
/// The plan does not validate, or it carries a legacy template label as if it
/// authorized a slot.
pub fn admit_plan(
    plan: &ConditionalExecutionPlan,
    tenant: TenantId,
    current_epoch: u64,
) -> Result<Admission, Refused> {
    // A decoded legacy plan is recorded for provenance, and its template label
    // authorizes nothing: it admits exactly the leg the legacy record actually
    // described. A CASCADE label does not buy a second slot.
    if let Some(legacy) = plan.provenance.legacy_decode.as_ref()
        && plan.slots.len() > 1
    {
        return Err(Refused::LegacyTemplate {
            label: legacy.template_label.clone(),
        });
    }
    plan.validate(tenant, current_epoch)
        .map_err(Refused::Invalid)?;
    let mut reserved = Money::zero(&plan.total_budget.currency, plan.total_budget.scale);
    for s in &plan.slots {
        reserved = reserved
            .checked_add(&s.budget.reserved, "slots[].budget.reserved")
            .map_err(Refused::Invalid)?;
    }
    let reserved = reserved
        .checked_add(&plan.verification_reserve, "verification_reserve")
        .map_err(Refused::Invalid)?;
    Ok(Admission {
        plan_id: plan.plan_id.clone(),
        validation_digest: modbit_domain::routing::plan_digest(plan),
        reserved,
        routing_epoch: plan.routing_epoch,
        lease_generation: plan.lease_generation,
    })
}

/// Admit one activation of one slot: the runtime asks for a continuation the
/// plan already contains, and gets an ordinal and an allowance or a refusal.
///
/// `want` is the activation the caller believes is next. A caller replaying
/// after a restart passes the ordinal it already recorded and is told the
/// activation is a duplicate, so a kill during activation recovers exactly one
/// activation with the budget unchanged.
///
/// # Errors
/// The slot is not in the plan, its trigger does not match, it has activated
/// as many times as the plan allows, the transaction is out of attempts, the
/// activation is already recorded, or what is left of the request cap does not
/// cover the slot once the verification reserve is held back.
pub fn admit_activation(
    plan: &ConditionalExecutionPlan,
    ledger: &RunLedger,
    slot_id: &str,
    trigger: Trigger,
) -> Result<Activation, Refused> {
    let admitted: Vec<String> = plan.slots.iter().map(|s| s.slot_id.clone()).collect();
    let Some(slot) = plan.slots.iter().find(|s| s.slot_id == slot_id) else {
        return Err(Refused::RequiredContinuationUnavailable {
            wanted: slot_id.to_owned(),
            admitted,
        });
    };
    if slot.trigger != trigger {
        return Err(Refused::RequiredContinuationUnavailable {
            wanted: format!("{slot_id} on {trigger:?}"),
            admitted: plan
                .slots
                .iter()
                .map(|s| format!("{} on {:?}", s.slot_id, s.trigger))
                .collect(),
        });
    }
    let done = ledger.activations_of(slot_id);
    if done >= slot.max_activations {
        return Err(Refused::ActivationsExhausted {
            slot_id: slot_id.to_owned(),
            max_activations: slot.max_activations,
        });
    }
    if ledger.attempts >= plan.max_total_attempts {
        return Err(Refused::AttemptsExhausted {
            max_total_attempts: plan.max_total_attempts,
            attempts: ledger.attempts,
        });
    }
    // The allowance is the request cap minus what is spent, what is in flight
    // and the verification reserve. No slot is ever given a fresh budget.
    let held = ledger
        .spent
        .checked_add(&ledger.in_flight, "spent+in_flight")
        .and_then(|m| m.checked_add(&plan.verification_reserve, "held+verification"))
        .map_err(Refused::Invalid)?;
    let allowance = Money {
        minor_units: plan
            .total_budget
            .minor_units
            .saturating_sub(held.minor_units),
        currency: plan.total_budget.currency.clone(),
        scale: plan.total_budget.scale,
    };
    if slot.budget.reserved.minor_units > allowance.minor_units {
        return Err(Refused::AllowanceExceeded {
            slot_id: slot_id.to_owned(),
            needs: slot.budget.reserved.clone(),
            allowance,
        });
    }
    Ok(Activation {
        slot_id: slot_id.to_owned(),
        activation: done + 1,
        reserved: slot.budget.reserved.clone(),
    })
}

/// Whether an activation the caller wants to record is one the ledger already
/// holds: the restart case, where replaying must recover rather than charge.
///
/// # Errors
/// The activation ordinal is already recorded for that slot.
pub fn check_not_replayed(
    ledger: &RunLedger,
    slot_id: &str,
    activation: u32,
) -> Result<(), Refused> {
    if activation <= ledger.activations_of(slot_id) {
        return Err(Refused::DuplicateActivation {
            slot_id: slot_id.to_owned(),
            activation,
        });
    }
    Ok(())
}
