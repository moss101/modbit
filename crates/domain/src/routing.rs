//! Versioned routing contracts (REQ-EPR-001, docs/38 "Shared serialization and
//! validation"): the records a conditional transaction is compiled from, the
//! plan itself, and what a run of it is allowed to say afterwards.
//!
//! Every record carries its schema version, its identity, when it was created,
//! a digest of its own content and where it came from. Validation is not
//! decoration here: money is integer minor units in a named currency,
//! probabilities are finite numbers in `[0, 1]`, counts and budgets are bounded
//! integers, references must resolve inside the caller's tenant, and an unknown
//! incompatible major version fails closed before anything executes. Nothing in
//! this module dispatches, decides or authorizes; it defines what may be
//! written down.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::ids::{RunId, SessionId, TaskId, TenantId};

/// The schema version this build writes for every routing contract.
pub const ROUTING_SCHEMA_VERSION: u32 = 2;

/// The lowest version this build can decode. An older record is read through an
/// explicit adapter that records its provenance; a newer major version is
/// refused rather than guessed at.
pub const MIN_DECODABLE_VERSION: u32 = 1;

/// Why a routing record was refused. Every variant names the field, because a
/// validation failure a caller cannot locate is not a validation failure.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "code", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Invalid {
    /// The record's schema version is newer than this build understands.
    IncompatibleVersion {
        /// Field that carried it.
        field: String,
        /// The version found.
        found: u32,
        /// The newest this build writes.
        supported: u32,
    },
    /// A required version, id or reference was empty.
    Missing {
        /// Field.
        field: String,
    },
    /// A number was not finite, not in range, or would overflow.
    OutOfRange {
        /// Field.
        field: String,
        /// What was expected.
        expected: String,
        /// What was found, as text.
        found: String,
    },
    /// Two slots, legs or ids collided.
    Duplicate {
        /// Field.
        field: String,
        /// The value.
        value: String,
    },
    /// A slot names a predecessor that is not in the plan, or the graph loops.
    UnresolvedReference {
        /// Field.
        field: String,
        /// The reference.
        reference: String,
    },
    /// A reference belongs to another tenant.
    ForeignTenant {
        /// Field.
        field: String,
        /// The tenant the reference belongs to.
        found: TenantId,
        /// The tenant of the record.
        expected: TenantId,
    },
    /// A generation moved backwards.
    StaleGeneration {
        /// Field.
        field: String,
        /// The generation offered.
        offered: u64,
        /// The generation in force.
        current: u64,
    },
}

/// An amount of money: integer minor units in a named currency at a named
/// scale. There is no floating-point budget arithmetic anywhere in routing.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Money {
    /// Minor units (for USD at scale 2, cents).
    pub minor_units: u64,
    /// ISO-like currency code, uppercase.
    pub currency: String,
    /// Decimal places the minor units are expressed in.
    pub scale: u8,
}

impl Money {
    /// Zero in a currency.
    #[must_use]
    pub fn zero(currency: &str, scale: u8) -> Self {
        Self {
            minor_units: 0,
            currency: currency.to_owned(),
            scale,
        }
    }

    /// Validate the currency and scale.
    ///
    /// # Errors
    /// Empty or non-uppercase currency, or a scale beyond four places.
    pub fn validate(&self, field: &str) -> Result<(), Invalid> {
        if self.currency.is_empty() {
            return Err(Invalid::Missing {
                field: format!("{field}.currency"),
            });
        }
        if !self
            .currency
            .chars()
            .all(|c| c.is_ascii_uppercase() && c.is_ascii_alphabetic())
        {
            return Err(Invalid::OutOfRange {
                field: format!("{field}.currency"),
                expected: "uppercase ASCII letters".into(),
                found: self.currency.clone(),
            });
        }
        if self.scale > 4 {
            return Err(Invalid::OutOfRange {
                field: format!("{field}.scale"),
                expected: "at most 4".into(),
                found: self.scale.to_string(),
            });
        }
        Ok(())
    }

    /// Add, refusing overflow and a currency mismatch rather than wrapping.
    ///
    /// # Errors
    /// Different currency or scale, or an overflowing sum.
    pub fn checked_add(&self, other: &Self, field: &str) -> Result<Self, Invalid> {
        if self.currency != other.currency || self.scale != other.scale {
            return Err(Invalid::OutOfRange {
                field: field.to_owned(),
                expected: format!("{} at scale {}", self.currency, self.scale),
                found: format!("{} at scale {}", other.currency, other.scale),
            });
        }
        let minor_units = self
            .minor_units
            .checked_add(other.minor_units)
            .ok_or_else(|| Invalid::OutOfRange {
                field: field.to_owned(),
                expected: "a sum below u64::MAX".into(),
                found: format!("{} + {}", self.minor_units, other.minor_units),
            })?;
        Ok(Self {
            minor_units,
            currency: self.currency.clone(),
            scale: self.scale,
        })
    }
}

/// A probability: finite, in `[0, 1]`. Constructed only through `new`, so an
/// out-of-range or NaN value never reaches a record.
#[derive(Clone, Copy, Debug, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct Probability(f64);

impl Probability {
    /// Build one.
    ///
    /// # Errors
    /// Not finite, or outside `[0, 1]`.
    pub fn new(value: f64, field: &str) -> Result<Self, Invalid> {
        if !value.is_finite() || !(0.0..=1.0).contains(&value) {
            return Err(Invalid::OutOfRange {
                field: field.to_owned(),
                expected: "a finite number in [0, 1]".into(),
                found: format!("{value}"),
            });
        }
        Ok(Self(value))
    }

    /// The value.
    #[must_use]
    pub fn get(self) -> f64 {
        self.0
    }
}

/// Where a record came from, and under which versions it was produced.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Provenance {
    /// Policy snapshot version.
    pub policy_version: String,
    /// Model registry generation.
    pub registry_generation: String,
    /// Profiler version.
    pub profiler_version: String,
    /// Outcome statistics version.
    pub statistics_version: String,
    /// Plan compiler version.
    pub compiler_version: String,
    /// Acceptance gate version.
    pub gate_version: String,
    /// Realized-risk rule version.
    pub risk_version: String,
    /// When the record was decoded from a legacy shape, what it was and why
    /// that is not evidence of a routing decision (docs/38: legacy labels never
    /// authorize).
    #[serde(default)]
    pub legacy_decode: Option<LegacyDecode>,
}

/// The provenance of a record that was read through a legacy adapter.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LegacyDecode {
    /// The shape it was decoded from (`ExecutionPlan`, `QualityGateResult`,
    /// `CriticResult`).
    pub source_shape: String,
    /// The version that shape carried.
    pub source_version: u32,
    /// Fields that had no equivalent and are recorded as unknown rather than
    /// invented.
    pub unknown_fields: Vec<String>,
    /// The sentence that travels with it.
    pub note: String,
}

impl Provenance {
    /// Every version must be named: an empty one is a missing input, not a
    /// default.
    ///
    /// # Errors
    /// Any version string is empty.
    pub fn validate(&self, field: &str) -> Result<(), Invalid> {
        for (name, value) in [
            ("policy_version", &self.policy_version),
            ("registry_generation", &self.registry_generation),
            ("profiler_version", &self.profiler_version),
            ("statistics_version", &self.statistics_version),
            ("compiler_version", &self.compiler_version),
            ("gate_version", &self.gate_version),
            ("risk_version", &self.risk_version),
        ] {
            if value.trim().is_empty() {
                return Err(Invalid::Missing {
                    field: format!("{field}.{name}"),
                });
            }
        }
        Ok(())
    }
}

/// What a slot may spend before it is stopped.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Budget {
    /// Wall-clock ceiling in milliseconds.
    pub timeout_ms: u64,
    /// Output token ceiling.
    pub max_output_tokens: u32,
    /// Retries allowed before the first token; zero is legal.
    pub max_retries: u32,
    /// Money reserved for this slot.
    pub reserved: Money,
}

impl Budget {
    /// Validate the bounds.
    ///
    /// # Errors
    /// A non-positive timeout or output ceiling, or invalid money.
    pub fn validate(&self, field: &str) -> Result<(), Invalid> {
        if self.timeout_ms == 0 {
            return Err(Invalid::OutOfRange {
                field: format!("{field}.timeout_ms"),
                expected: "a positive timeout".into(),
                found: "0".into(),
            });
        }
        if self.max_output_tokens == 0 {
            return Err(Invalid::OutOfRange {
                field: format!("{field}.max_output_tokens"),
                expected: "a positive ceiling".into(),
                found: "0".into(),
            });
        }
        self.reserved.validate(&format!("{field}.reserved"))
    }
}

/// What activates a slot.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Trigger {
    /// The first leg of the transaction; it has no predecessor.
    Initial,
    /// The acceptance gate rejected the predecessor's candidate.
    QualityRejected,
    /// The gate could not decide and asked for independent review.
    ReviewRequired,
    /// The predecessor's leg failed outright.
    LegFailed,
}

/// One prevalidated slot of a conditional plan.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Slot {
    /// Stable id inside the plan.
    pub slot_id: String,
    /// The slot that must run before this one (`None` for the initial slot).
    pub predecessor: Option<String>,
    /// What activates it.
    pub trigger: Trigger,
    /// How many times it may activate.
    pub max_activations: u32,
    /// Endpoint the leg dispatches to.
    pub endpoint: String,
    /// Model the leg dispatches to.
    pub model: String,
    /// The role it plays (`solver`, `reviewer`, `reviser`).
    pub role: String,
    /// Its budget.
    pub budget: Budget,
}

/// A compiled conditional execution plan (schema 2): the only executable plan
/// shape this build writes.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConditionalExecutionPlan {
    /// Schema version.
    pub schema_version: u32,
    /// Immutable plan id.
    pub plan_id: String,
    /// Tenant the plan belongs to.
    pub tenant_id: TenantId,
    /// Session.
    pub session_id: SessionId,
    /// Task.
    pub task_id: TaskId,
    /// Run the plan was compiled for.
    pub run_id: RunId,
    /// Routing epoch: a plan compiled in an older epoch never installs over a
    /// newer one.
    pub routing_epoch: u64,
    /// The lease generation the plan was compiled under.
    pub lease_generation: u64,
    /// When it was compiled (milliseconds since the epoch).
    pub created_at_ms: i64,
    /// The versions it was compiled from.
    pub provenance: Provenance,
    /// Digest of the inputs the compiler saw.
    pub input_digest: String,
    /// The slots, initial first.
    pub slots: Vec<Slot>,
    /// Total attempts allowed across the whole request.
    pub max_total_attempts: u32,
    /// Revisions allowed across the whole request.
    pub max_revisions: u32,
    /// Money held back for verification, inside the total.
    pub verification_reserve: Money,
    /// The whole request's ceiling.
    pub total_budget: Money,
    /// Digest over every field above.
    pub content_digest: String,
}

fn digest(parts: &[&str]) -> String {
    let mut h = Sha256::new();
    for p in parts {
        h.update(p.as_bytes());
        h.update([0]);
    }
    hex::encode(h.finalize())
}

/// The digest of a plan's content, computed with the digest field empty.
#[must_use]
pub fn plan_digest(plan: &ConditionalExecutionPlan) -> String {
    let mut p = plan.clone();
    p.content_digest = String::new();
    digest(&[&serde_json::to_string(&p).unwrap_or_default()])
}

impl ConditionalExecutionPlan {
    /// Seal the plan by writing its content digest.
    #[must_use]
    pub fn sealed(mut self) -> Self {
        self.content_digest = plan_digest(&self);
        self
    }

    /// Validate everything a plan must satisfy before anything dispatches.
    ///
    /// # Errors
    /// Any of the conditions in `Invalid`: an incompatible version, a missing
    /// id or version, a duplicate or unresolved slot, a cycle, a budget that
    /// exceeds the request total, or a foreign-tenant reference.
    pub fn validate(&self, tenant: TenantId, current_epoch: u64) -> Result<(), Invalid> {
        if self.schema_version > ROUTING_SCHEMA_VERSION
            || self.schema_version < MIN_DECODABLE_VERSION
        {
            return Err(Invalid::IncompatibleVersion {
                field: "schema_version".into(),
                found: self.schema_version,
                supported: ROUTING_SCHEMA_VERSION,
            });
        }
        if self.plan_id.trim().is_empty() {
            return Err(Invalid::Missing {
                field: "plan_id".into(),
            });
        }
        if self.input_digest.len() != 64 {
            return Err(Invalid::OutOfRange {
                field: "input_digest".into(),
                expected: "a 64-character sha256".into(),
                found: self.input_digest.len().to_string(),
            });
        }
        if self.tenant_id != tenant {
            return Err(Invalid::ForeignTenant {
                field: "tenant_id".into(),
                found: self.tenant_id,
                expected: tenant,
            });
        }
        if self.routing_epoch < current_epoch {
            return Err(Invalid::StaleGeneration {
                field: "routing_epoch".into(),
                offered: self.routing_epoch,
                current: current_epoch,
            });
        }
        self.provenance.validate("provenance")?;
        self.total_budget.validate("total_budget")?;
        self.verification_reserve.validate("verification_reserve")?;
        if self.verification_reserve.currency != self.total_budget.currency
            || self.verification_reserve.minor_units > self.total_budget.minor_units
        {
            return Err(Invalid::OutOfRange {
                field: "verification_reserve".into(),
                expected: format!(
                    "at most the total budget ({} {})",
                    self.total_budget.minor_units, self.total_budget.currency
                ),
                found: format!(
                    "{} {}",
                    self.verification_reserve.minor_units, self.verification_reserve.currency
                ),
            });
        }
        if self.max_total_attempts == 0 {
            return Err(Invalid::OutOfRange {
                field: "max_total_attempts".into(),
                expected: "at least one attempt".into(),
                found: "0".into(),
            });
        }
        if self.slots.is_empty() {
            return Err(Invalid::Missing {
                field: "slots".into(),
            });
        }
        // Slot ids are unique, budgets are sane, and the reserved money of all
        // slots plus the verification reserve stays inside the request total.
        let mut ids: BTreeSet<&str> = BTreeSet::new();
        let mut reserved = Money::zero(&self.total_budget.currency, self.total_budget.scale);
        let mut initial = 0usize;
        for s in &self.slots {
            if s.slot_id.trim().is_empty() {
                return Err(Invalid::Missing {
                    field: "slots[].slot_id".into(),
                });
            }
            if !ids.insert(s.slot_id.as_str()) {
                return Err(Invalid::Duplicate {
                    field: "slots[].slot_id".into(),
                    value: s.slot_id.clone(),
                });
            }
            if s.endpoint.trim().is_empty() || s.model.trim().is_empty() {
                return Err(Invalid::Missing {
                    field: format!("slots[{}].endpoint/model", s.slot_id),
                });
            }
            if s.max_activations == 0 {
                return Err(Invalid::OutOfRange {
                    field: format!("slots[{}].max_activations", s.slot_id),
                    expected: "at least one activation".into(),
                    found: "0".into(),
                });
            }
            s.budget.validate(&format!("slots[{}].budget", s.slot_id))?;
            reserved = reserved.checked_add(&s.budget.reserved, "slots[].budget.reserved")?;
            if s.trigger == Trigger::Initial {
                initial += 1;
                if s.predecessor.is_some() {
                    return Err(Invalid::UnresolvedReference {
                        field: format!("slots[{}].predecessor", s.slot_id),
                        reference: "an initial slot has no predecessor".into(),
                    });
                }
            } else if s.predecessor.is_none() {
                return Err(Invalid::Missing {
                    field: format!("slots[{}].predecessor", s.slot_id),
                });
            }
        }
        if initial != 1 {
            return Err(Invalid::OutOfRange {
                field: "slots[].trigger".into(),
                expected: "exactly one initial slot".into(),
                found: initial.to_string(),
            });
        }
        let total = reserved.checked_add(&self.verification_reserve, "reserved+verification")?;
        if total.minor_units > self.total_budget.minor_units {
            return Err(Invalid::OutOfRange {
                field: "total_budget".into(),
                expected: format!("at least the reserved {}", total.minor_units),
                found: self.total_budget.minor_units.to_string(),
            });
        }
        // Every predecessor resolves, and the graph is acyclic: walking up from
        // each slot must reach the initial one without repeating.
        let by_id: BTreeMap<&str, &Slot> =
            self.slots.iter().map(|s| (s.slot_id.as_str(), s)).collect();
        for s in &self.slots {
            let mut seen: BTreeSet<&str> = BTreeSet::new();
            let mut cur = s;
            while let Some(p) = cur.predecessor.as_deref() {
                if !seen.insert(cur.slot_id.as_str()) {
                    return Err(Invalid::UnresolvedReference {
                        field: format!("slots[{}].predecessor", s.slot_id),
                        reference: format!("cycle through {p}"),
                    });
                }
                cur = by_id
                    .get(p)
                    .copied()
                    .ok_or_else(|| Invalid::UnresolvedReference {
                        field: format!("slots[{}].predecessor", s.slot_id),
                        reference: p.to_owned(),
                    })?;
                if seen.contains(cur.slot_id.as_str()) {
                    return Err(Invalid::UnresolvedReference {
                        field: format!("slots[{}].predecessor", s.slot_id),
                        reference: format!("cycle through {}", cur.slot_id),
                    });
                }
            }
        }
        // The digest, when present, must be the digest of this content.
        if !self.content_digest.is_empty() && self.content_digest != plan_digest(self) {
            return Err(Invalid::OutOfRange {
                field: "content_digest".into(),
                expected: plan_digest(self),
                found: self.content_digest.clone(),
            });
        }
        Ok(())
    }

    /// The slot that runs first.
    #[must_use]
    pub fn initial_slot(&self) -> Option<&Slot> {
        self.slots.iter().find(|s| s.trigger == Trigger::Initial)
    }
}

/// A legacy plan shape, decoded explicitly and never executed as one.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LegacyExecutionPlan {
    /// The version the legacy record carried.
    pub version: u32,
    /// The template label it used (`DIRECT`, `CASCADE`, `CRITIQUE`).
    pub template: String,
    /// Endpoint.
    pub endpoint: String,
    /// Model.
    pub model: String,
    /// Timeout, when it had one.
    pub timeout_ms: Option<u64>,
}

/// The sentence a legacy decode carries with it.
pub const LEGACY_NOTE: &str = "Decoded from a legacy plan shape for provenance only. A template label authorizes no slot and grants no capability; absent statistics, budgets and confidence are recorded unknown, never invented (docs/38).";

/// Where a decoded plan belongs: the identity a legacy record never carried.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlanScope {
    /// Tenant.
    pub tenant_id: TenantId,
    /// Session.
    pub session_id: SessionId,
    /// Task.
    pub task_id: TaskId,
    /// Run.
    pub run_id: RunId,
    /// When the decode happened.
    pub created_at_ms: i64,
}

/// Decode a legacy plan into a schema-2 plan with one initial slot, recording
/// what had no equivalent. The result still has to pass `validate`.
#[must_use]
pub fn decode_legacy(
    legacy: &LegacyExecutionPlan,
    plan_id: &str,
    scope: PlanScope,
    provenance: Provenance,
    total_budget: Money,
) -> ConditionalExecutionPlan {
    let PlanScope {
        tenant_id,
        session_id,
        task_id,
        run_id,
        created_at_ms,
    } = scope;
    let mut provenance = provenance;
    provenance.legacy_decode = Some(LegacyDecode {
        source_shape: "ExecutionPlan".into(),
        source_version: legacy.version,
        unknown_fields: vec![
            "quality_lcb".into(),
            "outcome_statistics".into(),
            "reserved_budget".into(),
            "continuation_slots".into(),
        ],
        note: LEGACY_NOTE.into(),
    });
    ConditionalExecutionPlan {
        schema_version: ROUTING_SCHEMA_VERSION,
        plan_id: plan_id.to_owned(),
        tenant_id,
        session_id,
        task_id,
        run_id,
        routing_epoch: 0,
        lease_generation: 0,
        created_at_ms,
        provenance,
        input_digest: digest(&[&legacy.template, &legacy.endpoint, &legacy.model]),
        slots: vec![Slot {
            slot_id: "initial".into(),
            predecessor: None,
            trigger: Trigger::Initial,
            max_activations: 1,
            endpoint: legacy.endpoint.clone(),
            model: legacy.model.clone(),
            role: "solver".into(),
            budget: Budget {
                timeout_ms: legacy.timeout_ms.unwrap_or(120_000).max(1),
                max_output_tokens: 4096,
                max_retries: 0,
                reserved: Money::zero(&total_budget.currency, total_budget.scale),
            },
        }],
        max_total_attempts: 1,
        max_revisions: 0,
        verification_reserve: Money::zero(&total_budget.currency, total_budget.scale),
        total_budget,
        content_digest: String::new(),
    }
    .sealed()
}

/// What the direct baseline's provenance says for an input it never consulted.
/// The direct path is not routed: no policy, registry generation, profiler,
/// statistics, gate or risk model produced it, and the record says so rather
/// than borrowing a version from somewhere else.
pub const DIRECT_NOT_CONSULTED: &str = "none";

/// The compiler that produces the direct plan.
pub const DIRECT_COMPILER_VERSION: &str = "direct-baseline-1";

/// What a direct plan does not claim, carried with it so no reader has to
/// remember (docs/38: unknown cost stays unknown).
pub const DIRECT_NOTE: &str = "The direct baseline expressed as a schema-2 plan: one solver slot, no conditional branches, and no cost model. Zero reserved money means unbudgeted, not free — the direct path has no price estimate, and an attempt whose provider reported no usage is recorded unknown rather than zero.";

/// The single slot every direct plan has.
pub const DIRECT_SLOT: &str = "initial";

/// The plan id of the direct plan of a run: derived from the run, so a reader
/// of an attempt never has to look the plan up to name it.
#[must_use]
pub fn direct_plan_id(run_id: RunId) -> String {
    format!("direct:{run_id}")
}

/// What the run dispatches on the direct path.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DirectPath<'a> {
    /// Endpoint name (never a URL, never a credential).
    pub endpoint: &'a str,
    /// Model.
    pub model: &'a str,
    /// Per-attempt wall-clock ceiling.
    pub timeout_ms: u64,
    /// Per-attempt output ceiling.
    pub max_output_tokens: u32,
    /// Retries the gateway may make before the first token.
    pub max_retries: u32,
    /// Turns the run may take, which bounds its attempts.
    pub max_turns: u32,
}

impl ConditionalExecutionPlan {
    /// The plan the direct path already executes, written down: one initial
    /// solver slot activated once, every model invocation of the run an
    /// attempt inside it (REQ-EPR-001, docs/38 "Preserve the direct path").
    ///
    /// This changes no dispatch. It exists so the durable routing state is
    /// produced by the product rather than by a test, and so the baseline a
    /// router will later be measured against is recorded in the same shape as
    /// the plans that replace it.
    #[must_use]
    pub fn direct(
        tenant_id: TenantId,
        session_id: SessionId,
        task_id: TaskId,
        run_id: RunId,
        lease_generation: u64,
        created_at_ms: i64,
        path: &DirectPath<'_>,
    ) -> Self {
        let unbudgeted = Money::zero("USD", 2);
        let v = DIRECT_NOT_CONSULTED.to_owned();
        Self {
            schema_version: ROUTING_SCHEMA_VERSION,
            plan_id: direct_plan_id(run_id),
            tenant_id,
            session_id,
            task_id,
            run_id,
            // The direct path is the zeroth epoch: any compiled plan is newer.
            routing_epoch: 0,
            lease_generation,
            created_at_ms,
            provenance: Provenance {
                policy_version: v.clone(),
                registry_generation: v.clone(),
                profiler_version: v.clone(),
                statistics_version: v.clone(),
                compiler_version: DIRECT_COMPILER_VERSION.to_owned(),
                gate_version: v.clone(),
                risk_version: v,
                legacy_decode: None,
            },
            input_digest: digest(&[
                path.endpoint,
                path.model,
                &path.timeout_ms.to_string(),
                &path.max_output_tokens.to_string(),
                &path.max_retries.to_string(),
                &path.max_turns.to_string(),
            ]),
            slots: vec![Slot {
                slot_id: DIRECT_SLOT.to_owned(),
                predecessor: None,
                trigger: Trigger::Initial,
                max_activations: 1,
                endpoint: path.endpoint.to_owned(),
                model: path.model.to_owned(),
                role: "solver".to_owned(),
                budget: Budget {
                    timeout_ms: path.timeout_ms,
                    max_output_tokens: path.max_output_tokens,
                    max_retries: path.max_retries,
                    reserved: unbudgeted.clone(),
                },
            }],
            max_total_attempts: path.max_turns.max(1),
            max_revisions: 0,
            verification_reserve: unbudgeted.clone(),
            total_budget: unbudgeted,
            content_digest: String::new(),
        }
        .sealed()
    }
}
