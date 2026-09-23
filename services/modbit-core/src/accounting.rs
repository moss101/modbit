//! Request, leg and gate accounting and outcomes (REQ-EPR-010; docs/27 §10
//! and §11, docs/38 "CompleteAccountingAndAttribution", docs/34 "Verified
//! workflow economics and routing observability").
//!
//! A request is a task. Its record is derived from the canonical log alone —
//! the task's runs, the review tasks its reviewer legs ran on and the late
//! invoices reconciled into it — so it can be rebuilt at any time and is not
//! a second authority. The durable `RequestOutcomeRecorded` pins the record
//! as of a run's end, a review's conclusion or a reconciliation; a later
//! record supersedes an earlier one and records are never summed.
//!
//! What the record will not do matters as much as what it does:
//!
//! * Unknown cost is never zero. An attempt whose usage the provider never
//!   reported, and an invocation a crash cut off before its attempt was
//!   recorded, hold the reservation of the slot they ran in until a late
//!   invoice settles them; one with nothing to hold is counted unpriced and
//!   the record says its cost is incomplete.
//! * Nothing is counted twice. Attempts are keyed by run, plan, slot and
//!   ordinal; a replayed record of the same attempt, or a second invoice for
//!   it, is named and ignored.
//! * A request's success is not its legs' success. A failed initial leg
//!   stays failed when an escalation succeeds; reviewer findings, revisions
//!   and gate verdicts are kept apart, and a gate's later correction is an
//!   observation of its own.
//! * Raw signals are kept by reference — the event that carried them, when
//!   and whose — never by content, so the content's retention governs them.
//!   A signal whose source can no longer be read is unavailable, not absent.
//! * Only this Core's tenant is read, and no saving is claimed: a direct
//!   frontier comparison is an estimate unless an alternative actually ran.

use std::collections::{HashMap, HashSet};

use modbit_domain::event::{Actor, AggregateType};
use modbit_domain::routing::{ConditionalExecutionPlan, RoutingCandidate};
use modbit_domain::task::{TaskEvent, TaskState, WaitReason};
use modbit_domain::{RunId, TaskId, TenantId};
use modbit_event_store::{EventStore, NewEvent, StoredEvent};
use modbit_protocol::v1 as wire;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::runtime::{Lineage, append, typed};
use crate::server::Core;

/// Accounting rules version, carried by every record.
pub(crate) const ACCOUNTING_VERSION: &str = "request-accounting/1";

/// What the record does not claim.
const NOT_CLAIMED: &[&str] = &[
    "Held money is a reservation kept for work whose usage is unknown; it is settled by a late invoice, never refunded as free.",
    "Deterministic verification runs as local processes and has no price; its time is reported, not a cost.",
    "The direct-frontier comparison is an estimate from the compiler's own expected cost; no alternative was executed, so no saving is claimed.",
    "Signals are kept by reference to the event that carried them; an unavailable one could not be read, which is not the same as not having happened.",
    "No reward model version is active: there is no composite reward, only the raw signals.",
];

/// What an attempt cost at the registry in force.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Priced {
    /// Minor units.
    pub minor: u64,
    /// Registry generation of the prices.
    pub registry_generation: String,
}

/// Price reported usage at the registry's prices for a binding: plain input
/// at the input price, the cached subset at the cached price (never above
/// the input price), cache writes at the input price plus the write
/// surcharge, and output at the output price — each rounded up, the same
/// basis the switch economics use (REQ-EPR-009). `None` when no registry is
/// active or it does not price the binding.
pub(crate) fn price(
    registry: Option<&modbit_providers::registry::ModelRegistry>,
    endpoint: &str,
    model: &str,
    usage: &modbit_providers::Usage,
) -> Option<Priced> {
    let registry = registry?;
    let e = &registry.entry(endpoint, model)?.economics;
    Some(Priced {
        minor: price_at(e, usage),
        registry_generation: registry.generation().to_owned(),
    })
}

/// `price` at one binding's economics.
fn price_at(e: &modbit_providers::registry::Economics, usage: &modbit_providers::Usage) -> u64 {
    use modbit_providers::economics::cost_minor;
    let cached = usage.cached_input_tokens.min(usage.input_tokens);
    let write = usage
        .cache_write_input_tokens
        .min(usage.input_tokens - cached);
    let plain = usage.input_tokens - cached - write;
    let cached_price = e
        .cached_input_per_mtok_minor
        .unwrap_or(e.input_per_mtok_minor)
        .min(e.input_per_mtok_minor);
    cost_minor(plain.saturating_add(write), e.input_per_mtok_minor)
        .saturating_add(cost_minor(cached, cached_price))
        .saturating_add(cost_minor(write, e.cache_write_per_mtok_minor.unwrap_or(0)))
        .saturating_add(cost_minor(usage.output_tokens, e.output_per_mtok_minor))
}

// ---- The record ----

/// The request's accounting and outcome record (docs/27 §10 RoutingTrace and
/// §11.2 OutcomeRecord, kept apart by docs/27 §11.3).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub(crate) struct OutcomeRecord {
    /// Rules version.
    pub accounting_version: String,
    /// Tenant, session and task (the request).
    pub tenant_id: String,
    /// Session.
    pub session_id: String,
    /// Task.
    pub task_id: String,
    /// Every version the request ran under.
    pub versions: Versions,
    /// One routing decision per plan admitted for the request.
    pub decisions: Vec<DecisionRecord>,
    /// What ran, in order: legs with their slots and attempts.
    pub executed_path: Vec<Leg>,
    /// Derived label of what ran (never a template).
    pub path_label: String,
    /// Complete cost.
    pub cost: Cost,
    /// Reservations and their settlement.
    pub reservations: Reservations,
    /// Request-level observation.
    pub request: RequestOutcome,
    /// Leg-level observations.
    pub legs: LegOutcomes,
    /// Gate-level observations.
    pub gates: Vec<GateObservation>,
    /// Realized-risk observations.
    pub risks: Vec<RiskObservation>,
    /// Raw signals, by reference.
    pub signals: Signals,
    /// Counterfactual, labelled.
    pub counterfactual: Counterfactual,
    /// Reward.
    pub reward: Reward,
    /// Signals the record could not observe, by name.
    pub missing_signals: Vec<String>,
    /// Records ignored as duplicates, by key.
    pub duplicates_ignored: Vec<String>,
    /// What the record does not claim.
    pub not_claimed: Vec<String>,
}

/// Versions (docs/27 §10).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub(crate) struct Versions {
    /// Per plan: its pinned provenance.
    pub plans: Vec<PlanVersions>,
    /// Request Profiler `profiler@cohort`, per profile recorded.
    pub profiler: Vec<String>,
    /// Skills selected, `name@version#content_hash`.
    pub skills: Vec<String>,
    /// Acceptance Gate rules versions that judged the request.
    pub gate: Vec<String>,
    /// Realized-risk `policy/rules` versions.
    pub risk: Vec<String>,
    /// Outcome Statistics versions the admissions measured against.
    pub statistics: Vec<String>,
    /// Threshold versions.
    pub thresholds: Vec<String>,
    /// Switch economics versions.
    pub economics: Vec<String>,
    /// Registry generations attempts were priced under.
    pub priced_under: Vec<String>,
}

/// One plan's provenance.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub(crate) struct PlanVersions {
    /// Plan.
    pub plan_id: String,
    /// Run.
    pub run_id: String,
    /// Routing epoch.
    pub routing_epoch: u64,
    /// Policy version.
    pub policy: String,
    /// Registry generation.
    pub registry_generation: String,
    /// Profiler version.
    pub profiler: String,
    /// Statistics version.
    pub statistics: String,
    /// Compiler version.
    pub compiler: String,
    /// Gate version.
    pub gate: String,
    /// Risk version.
    pub risk: String,
    /// Content digest of the plan.
    pub content_digest: String,
}

/// One routing decision.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub(crate) struct DecisionRecord {
    /// Plan.
    pub plan_id: String,
    /// Run.
    pub run_id: String,
    /// Epoch.
    pub routing_epoch: u64,
    /// Whether a decision record was on the log (plans admitted before
    /// REQ-EPR-010 have none, and nothing is invented for them).
    pub recorded: bool,
    /// Selector code, `OPERATOR` or `DIRECT`.
    pub selection: String,
    /// Admission feasibility.
    pub feasibility: String,
    /// Whether the plan may be described as meeting the target.
    pub target_met: bool,
    /// Chosen plan's quality mean, basis points, when recorded.
    pub chosen_quality_mean_bp: Option<u32>,
    /// Chosen plan's quality lower bound, basis points.
    pub chosen_quality_lcb_bp: u32,
    /// Choice probability, basis points, when recorded.
    pub choice_probability_bp: Option<u32>,
    /// Compile time, when recorded.
    pub routing_latency_ms: Option<u64>,
    /// Every candidate considered.
    pub candidates: Vec<RoutingCandidate>,
    /// Money admission reserved for the plan.
    pub reserved_minor: u64,
}

/// One leg: consecutive slots of one role and binding within one run (a
/// reconciled transaction on the same binding is the same leg), or one
/// reviewer activation with the review task it ran on.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub(crate) struct Leg {
    /// 1-based position in the executed path.
    pub ordinal: u32,
    /// `solver` | `reviewer` | `reviser`.
    pub role: String,
    /// `INITIAL` | `QUALITY_REJECTED` | `REVIEW_REQUIRED` | `REVISION` |
    /// `RETURNED`.
    pub trigger: String,
    /// `endpoint/model`.
    pub binding: String,
    /// The task it ran on (the request, or the review task).
    pub task_id: String,
    /// The slots it ran in.
    pub segments: Vec<Segment>,
    /// Every attempt, the failed, cancelled and cut-off ones included.
    pub attempts: Vec<AttemptRecord>,
    /// Total input tokens reported.
    pub input_tokens: u64,
    /// Cached subset.
    pub cached_input_tokens: u64,
    /// Cache-write subset.
    pub cache_write_tokens: u64,
    /// Output tokens reported.
    pub output_tokens: u64,
    /// Priced spend.
    pub cost_minor: u64,
    /// Held for attempts of unknown usage.
    pub held_minor: u64,
    /// Attempts whose cost cannot be stated.
    pub unpriced_attempts: u32,
    /// Sum of attempt latencies.
    pub latency_ms: u64,
    /// Attempts beyond the first.
    pub retries: u32,
    /// Tool calls on its task during the leg (reviewer legs: the review
    /// task's own; solver legs: none attributed per leg — see `cost.tools`).
    pub tool_calls: u32,
    /// Their time.
    pub tool_ms: u64,
    /// First event of the leg.
    pub started_at_ms: i64,
    /// Last event of the leg.
    pub ended_at_ms: i64,
    /// `SUCCEEDED` | `REJECTED` | `REVISED` | `SUPERSEDED` | `FAILED` |
    /// `UNVERIFIED` | `CANCELLED` | `OPEN` for solvers; the verdict, or
    /// `NO_REPORT` | `OPEN`, for reviewers.
    pub outcome: String,
}

/// One slot a leg ran in.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub(crate) struct Segment {
    /// Run.
    pub run_id: String,
    /// Plan.
    pub plan_id: String,
    /// Slot.
    pub slot_id: String,
    /// Activations recorded.
    pub activations: u32,
    /// Money the activations reserved.
    pub reserved_minor: u64,
}

/// One attempt.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub(crate) struct AttemptRecord {
    /// Run.
    pub run_id: String,
    /// Plan.
    pub plan_id: String,
    /// Slot.
    pub slot_id: String,
    /// Ordinal within the slot; 0 for an invocation cut off before its
    /// attempt was recorded.
    pub attempt: u32,
    /// `SUCCEEDED` | `FAILED` | `CANCELLED` | `INTERRUPTED` | `TIMED_OUT` |
    /// `CUT_OFF` | `IN_FLIGHT`.
    pub outcome: String,
    /// `REPORTED` | `RECONCILED` | `UNKNOWN`.
    pub usage: String,
    /// Total input, when known.
    pub input_tokens: Option<u64>,
    /// Cached subset, when known.
    pub cached_input_tokens: Option<u64>,
    /// Cache-write subset, when known.
    pub cache_write_tokens: Option<u64>,
    /// Output, when known.
    pub output_tokens: Option<u64>,
    /// Priced cost, when priced.
    pub cost_minor: Option<u64>,
    /// Held reservation, when the usage is unknown.
    pub held_minor: u64,
    /// Registry generation of the price.
    pub priced_under: String,
    /// Latency, when recorded.
    pub latency_ms: Option<u64>,
    /// Provider request id, when given.
    pub provider_request_id: Option<String>,
    /// The event that recorded it, `event:<offset>`.
    pub source: String,
}

/// Complete cost (docs/27 §10.1: every leg, not the winning one).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub(crate) struct Cost {
    /// Currency of the plans.
    pub currency: String,
    /// Scale.
    pub scale: u8,
    /// Priced inference, reported and reconciled.
    pub inference_minor: u64,
    /// Held for attempts of unknown usage.
    pub held_unknown_minor: u64,
    /// `inference_minor + held_unknown_minor`.
    pub total_minor: u64,
    /// Attempts whose cost cannot be stated at all.
    pub unpriced_attempts: u32,
    /// Whether every attempt is priced: nothing held, nothing unpriced.
    pub complete: bool,
    /// Solver inference (initial legs, escalations and revisions).
    pub solver_minor: u64,
    /// Reviewer inference.
    pub reviewer_minor: u64,
    /// Total input tokens.
    pub input_tokens: u64,
    /// Cached subset.
    pub cached_input_tokens: u64,
    /// Cache-write subset.
    pub cache_write_tokens: u64,
    /// Output tokens.
    pub output_tokens: u64,
    /// Deterministic verification.
    pub verification: VerificationCost,
    /// Tool and process time.
    pub tools: ToolCost,
    /// First to last event of the request, milliseconds.
    pub wall_clock_ms: u64,
}

/// Deterministic verification cost.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub(crate) struct VerificationCost {
    /// Verification runs recorded (the request's and its reviewers').
    pub runs: u32,
    /// Checks in them.
    pub checks: u32,
    /// Process time of the checks.
    pub process_ms: u64,
    /// Money, when priced; local processes are not.
    pub cost_minor: Option<u64>,
    /// `LOCAL_PROCESS_UNPRICED`.
    pub basis: String,
}

/// Tool and process time.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub(crate) struct ToolCost {
    /// Tool calls on the request task.
    pub request_calls: u32,
    /// Their time.
    pub request_ms: u64,
    /// Tool calls on review tasks.
    pub reviewer_calls: u32,
    /// Their time.
    pub reviewer_ms: u64,
}

/// Reservations and settlement (docs/38 step 3).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub(crate) struct Reservations {
    /// Reserved by slot activations on the request.
    pub activated_minor: u64,
    /// Settled at priced spend.
    pub settled_minor: u64,
    /// Held for unknown usage until reconciled.
    pub held_minor: u64,
    /// Released once the request stopped: activated less settled and held;
    /// `None` while it may still spend.
    pub released_minor: Option<u64>,
}

/// Request-level observation (docs/27 §11.3).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub(crate) struct RequestOutcome {
    /// Task state.
    pub task_state: String,
    /// Whether the task can do nothing more.
    pub terminal: bool,
    /// Why the last run stopped, when it stopped short.
    pub stop: String,
    /// `pass` | `fail` | `partial` | `cancelled` | `open`.
    pub final_outcome: String,
    /// Verified: a candidate stands whose completion verification passed
    /// with checks, not rejected by the gate or the independent reviewer at
    /// its revision.
    pub verified_success: bool,
    /// Verified with no repair attempt, escalation or revision.
    pub first_pass_success: bool,
    /// Candidate revision judged last.
    pub candidate_revision: Option<u64>,
    /// `PASSED` | `FAILED` | `NO_CHECKS` | `NOT_RUN`.
    pub verification: String,
    /// Repair attempts recorded.
    pub repair_turns: u32,
    /// Escalations activated.
    pub escalations: u32,
    /// Revisions activated.
    pub revisions: u32,
    /// Human interventions (steer, input, pause, cancel, answers, patches,
    /// approvals).
    pub human_interventions: u32,
    /// An independent review is under way.
    pub review_pending: bool,
}

/// Leg-level observations (docs/27 §11.3).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub(crate) struct LegOutcomes {
    /// Whether the initial leg succeeded; `None` while undecided. A later
    /// escalation's success never makes it true.
    pub initial_leg_success: Option<bool>,
    /// The initial leg's binding.
    pub initial_leg: String,
    /// Escalations.
    pub escalations: Vec<EscalationOutcome>,
    /// Independent reviews.
    pub reviews: Vec<ReviewOutcome>,
    /// Revisions.
    pub revisions: Vec<RevisionOutcome>,
}

/// One escalation.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub(crate) struct EscalationOutcome {
    /// Plan whose slot activated.
    pub plan_id: String,
    /// Slot the failed leg ran in.
    pub from_slot: String,
    /// Slot that activated.
    pub to_slot: String,
    /// Binding before.
    pub from_binding: String,
    /// Binding after.
    pub to_binding: String,
    /// `REPAIR_ESCALATED` | `NO_PROGRESS` | `RESUMED`.
    pub cause: String,
    /// Gate result that rejected the failed leg.
    pub gate_ref: String,
    /// Whether the continuation succeeded; `None` while undecided.
    pub success: Option<bool>,
}

/// One independent review.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub(crate) struct ReviewOutcome {
    /// Reviewer slot.
    pub slot_id: String,
    /// Reviewer binding.
    pub binding: String,
    /// Review task.
    pub review_task_id: String,
    /// Revision reviewed.
    pub candidate_revision: u64,
    /// Verdict, when reported.
    pub verdict: Option<String>,
    /// Findings validated at the revision (genuine-defect evidence).
    pub validated_findings: u32,
    /// Findings the evidence did not support (false positives).
    pub unsupported_findings: u32,
    /// Highest validated severity.
    pub highest_severity: String,
    /// Whether the review found a genuine defect; `None` until it reported.
    pub genuine_defect_found: Option<bool>,
}

/// One revision.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub(crate) struct RevisionOutcome {
    /// Revision ordinal.
    pub revision: u32,
    /// Bound.
    pub max_revisions: u32,
    /// Binding it ran on.
    pub binding: String,
    /// Whether the revised candidate passed the next review; `None` while
    /// undecided.
    pub success: Option<bool>,
}

/// One Acceptance Gate evaluation and what later evidence said about it.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub(crate) struct GateObservation {
    /// Gate result object.
    pub gate_ref: String,
    /// Rules version.
    pub gate_version: String,
    /// Candidate revision.
    pub candidate_revision: u64,
    /// ACCEPT | REJECT | INCONCLUSIVE.
    pub verdict: String,
    /// What triggered it.
    pub trigger: String,
    /// Required assurance.
    pub required_assurance: String,
    /// Independent review required.
    pub independent_review_required: bool,
    /// Human required.
    pub human_required: bool,
    /// Reject reasons.
    pub reject_reasons: Vec<String>,
    /// Risk record consumed.
    pub realized_risk_ref: String,
    /// `ACCEPTED_LATER_SHOWN_WRONG` | `REJECTED_LATER_SHOWN_VALID` |
    /// `NONE_OBSERVED`.
    pub correction: String,
    /// The later evidence, `event:<offset>`, when any.
    pub correction_source: Option<String>,
}

/// One realized-risk derivation.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub(crate) struct RiskObservation {
    /// Risk record.
    pub realized_risk_ref: String,
    /// Candidate revision.
    pub candidate_revision: u64,
    /// Level.
    pub level: String,
    /// Minimum assurance.
    pub minimum_assurance: String,
    /// Policy version.
    pub policy_version: String,
    /// Rules version.
    pub rules_version: String,
    /// Later evidence of a miss or an over-classification; none is derived
    /// from the log today.
    pub correction: String,
}

/// A raw signal, by reference.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub(crate) struct Signal {
    /// Event type that carried it.
    pub kind: String,
    /// Its value where it has one (a decision), never content.
    pub value: String,
    /// `event:<offset>`.
    pub source: String,
    /// When.
    pub at_ms: i64,
    /// `user` | `reviewer` | `agent` | `core` | `external`.
    pub actor: String,
}

/// Raw signals (docs/27 §11.1).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub(crate) struct Signals {
    /// A person's accept or return.
    pub explicit: Vec<Signal>,
    /// Steering or input after a candidate was first proposed.
    pub corrections: Vec<Signal>,
    /// Restores and direct edits after a candidate was first proposed.
    pub reverts: Vec<Signal>,
    /// Every human intervention.
    pub interventions: Vec<Signal>,
    /// Tests in the last completion verification, when it ran any.
    pub tests_passed: Option<bool>,
    /// Builds in it, when it ran any.
    pub build_passed: Option<bool>,
    /// Pull request merged (`true`) or closed unmerged (`false`), when a
    /// forge reported either.
    pub merge: Option<bool>,
    /// Kept past a configured window; `None`: no window is configured.
    pub keep: Option<bool>,
    /// Signal-bearing events whose payload could not be read.
    pub unavailable: Vec<String>,
}

/// Counterfactual, labelled (docs/27 §10.2).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub(crate) struct Counterfactual {
    /// The direct frontier plan's expected cost, when the compiler
    /// considered one other than the chosen plan.
    pub direct_frontier_minor: Option<u64>,
    /// Its binding.
    pub direct_frontier_binding: Option<String>,
    /// `ESTIMATED` | `CHOSEN_WAS_FRONTIER` | `NO_CANDIDATES`.
    pub label: String,
    /// An observed alternative's cost; always `None` here.
    pub observed_minor: Option<u64>,
    /// `NOT_EXECUTED`.
    pub observed_label: String,
}

/// Reward (docs/27 §11.2): raw signals only while no version is active.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub(crate) struct Reward {
    /// `none`.
    pub reward_version: String,
    /// Composite, basis points, when a version is active.
    pub composite_bp: Option<u32>,
}

/// A run's end the record is written with, before it is on the log: the
/// record rides in the same append as the end (docs/38 step 4).
#[derive(Clone, Debug)]
pub(crate) struct Pending {
    /// Task state the batch installs.
    pub task_state: &'static str,
    /// Why the run stopped short, when it did.
    pub stop: String,
    /// A reviewer leg activates in the same batch.
    pub review_pending: bool,
}

// ---- Derivation ----

struct Ev<'a> {
    offset: u64,
    at: i64,
    ty: &'a str,
    task: Option<TaskId>,
    run: Option<RunId>,
    actor: &'a Actor,
    aggregate_id: [u8; 16],
    payload: Option<Value>,
}

static NULL: Value = Value::Null;

/// An event's payload, or null when it could not be read.
fn p<'a>(e: &'a Ev) -> &'a Value {
    e.payload.as_ref().unwrap_or(&NULL)
}

fn state_name(s: TaskState) -> &'static str {
    match s {
        TaskState::Created => "Created",
        TaskState::Queued => "Queued",
        TaskState::Running => "Running",
        TaskState::Waiting(_) => "Waiting",
        TaskState::ReadyForReview => "ReadyForReview",
        TaskState::Completed => "Completed",
        TaskState::Failed => "Failed",
        TaskState::Cancelled => "Cancelled",
    }
}

fn push_unique(v: &mut Vec<String>, s: String) {
    if !s.is_empty() && !v.contains(&s) {
        v.push(s);
    }
}

fn u(v: &Value) -> u64 {
    v.as_u64().unwrap_or(0)
}

fn s(v: &Value) -> String {
    v.as_str().unwrap_or_default().to_owned()
}

/// Derive the request's record from the log. `pending` describes a run end
/// being appended with the record; `None` reads the log as it stands.
#[allow(clippy::too_many_lines)]
pub(crate) fn derive(
    store: &EventStore,
    tenant: TenantId,
    task_id: TaskId,
    pending: Option<&Pending>,
) -> Option<OutcomeRecord> {
    let task = store.task(&task_id).ok()??;
    let all = store.read_session(&task.session_id, 0, usize::MAX).ok()?;
    // Only this tenant's log is read (a handed-off envelope keeps its
    // origin tenant as provenance and is not this request's accounting).
    let mine = |e: &&StoredEvent| e.envelope.tenant_id == tenant;
    // The review tasks this request's reviewer legs ran on.
    let mut review_tasks: HashSet<TaskId> = HashSet::new();
    for e in all.iter().filter(mine) {
        if e.envelope.task_id == Some(task_id)
            && e.envelope.event_type == "ReviewLegActivated"
            && let Ok(p) = store.payload(&e.envelope)
            && let Ok(t) = serde_json::from_value::<TaskId>(p["review_task_id"].clone())
        {
            review_tasks.insert(t);
        }
    }
    let mut unavailable: Vec<String> = Vec::new();
    let evs: Vec<Ev> = all
        .iter()
        .filter(mine)
        .filter(|e| {
            e.envelope
                .task_id
                .is_some_and(|t| t == task_id || review_tasks.contains(&t))
        })
        .map(|e| {
            let payload = match store.payload(&e.envelope) {
                Ok(p) => Some(p),
                Err(_) => {
                    push_unique(
                        &mut unavailable,
                        format!("{}@event:{}", e.envelope.event_type, e.offset),
                    );
                    None
                }
            };
            Ev {
                offset: e.offset,
                at: e.envelope.occurred_at.0,
                ty: e.envelope.event_type.as_str(),
                task: e.envelope.task_id,
                run: e.envelope.run_id,
                actor: &e.envelope.actor,
                aggregate_id: e.envelope.aggregate_id,
                payload,
            }
        })
        .collect();
    let on_request = |e: &Ev| e.task == Some(task_id);

    let mut rec = OutcomeRecord {
        accounting_version: ACCOUNTING_VERSION.to_owned(),
        tenant_id: tenant.to_string(),
        session_id: task.session_id.to_string(),
        task_id: task_id.to_string(),
        not_claimed: NOT_CLAIMED.iter().map(|s| (*s).to_owned()).collect(),
        ..Default::default()
    };

    // -- Plans, decisions, activations, attempts, in log order.
    let mut plans: HashMap<(RunId, String), ConditionalExecutionPlan> = HashMap::new();
    let mut plan_order: Vec<(RunId, String)> = Vec::new();
    let mut admissions: HashMap<(RunId, String), Value> = HashMap::new();
    let mut decisions: HashMap<(RunId, String), Value> = HashMap::new();
    // (offset, run, plan, slot, reserved)
    let mut activations: Vec<(u64, RunId, String, String, u64)> = Vec::new();
    let mut attempts: Vec<(u64, i64, TaskId, AttemptRecord)> = Vec::new();
    let mut attempt_keys: HashSet<(RunId, String, String, u32)> = HashSet::new();
    let mut reconciled: HashMap<(RunId, String, String, u32), (u64, Value)> = HashMap::new();
    // Invocations started and not yet recorded as an attempt, per run.
    let mut open_invocation: HashMap<RunId, (u64, i64, TaskId)> = HashMap::new();
    let mut cut_off: Vec<(u64, i64, TaskId, RunId)> = Vec::new();
    let mut run_first: HashMap<RunId, (u64, TaskId)> = HashMap::new();
    let mut run_ended: HashSet<RunId> = HashSet::new();

    for e in &evs {
        if let Some(r) = e.run
            && let Some(t) = e.task
        {
            run_first.entry(r).or_insert((e.offset, t));
        }
        match e.ty {
            "RoutingPlanCompiled" => {
                if let Ok(plan) =
                    serde_json::from_value::<ConditionalExecutionPlan>(p(e)["plan"].clone())
                {
                    let key = (plan.run_id, plan.plan_id.clone());
                    if !plans.contains_key(&key) {
                        plan_order.push(key.clone());
                    }
                    plans.insert(key, plan);
                }
            }
            "RoutingPlanAdmitted" => {
                if let Some(r) = e.run {
                    admissions.insert((r, s(&p(e)["plan_id"])), p(e).clone());
                }
            }
            "RoutingDecisionRecorded" => {
                if let Some(r) = e.run {
                    decisions.insert((r, s(&p(e)["plan_id"])), p(e).clone());
                }
            }
            "SlotActivated" => {
                if let Some(r) = e.run {
                    activations.push((
                        e.offset,
                        r,
                        s(&p(e)["plan_id"]),
                        s(&p(e)["slot_id"]),
                        u(&p(e)["reserved_minor"]),
                    ));
                }
            }
            "ModelInvocationStarted" => {
                if let (Some(r), Some(t)) = (e.run, e.task)
                    && let Some((o, at, t0)) = open_invocation.insert(r, (e.offset, e.at, t))
                {
                    cut_off.push((o, at, t0, r));
                }
            }
            "RoutingAttemptRecorded" => {
                let (Some(r), Some(t)) = (e.run, e.task) else {
                    continue;
                };
                open_invocation.remove(&r);
                let x = p(e);
                let key = (
                    r,
                    s(&x["plan_id"]),
                    s(&x["slot_id"]),
                    x["attempt"].as_u64().unwrap_or(0) as u32,
                );
                if !attempt_keys.insert(key.clone()) {
                    rec.duplicates_ignored.push(format!(
                        "attempt {}/{}/{}#{} at event:{}",
                        key.0, key.1, key.2, key.3, e.offset
                    ));
                    continue;
                }
                let known = x["usage_known"].as_bool().unwrap_or(false);
                let opt = |k: &str| if known { x[k].as_u64() } else { None };
                attempts.push((
                    e.offset,
                    e.at,
                    t,
                    AttemptRecord {
                        run_id: r.to_string(),
                        plan_id: key.1.clone(),
                        slot_id: key.2.clone(),
                        attempt: key.3,
                        outcome: s(&x["outcome"]),
                        usage: if known { "REPORTED" } else { "UNKNOWN" }.into(),
                        input_tokens: opt("input_tokens"),
                        cached_input_tokens: opt("cached_input_tokens"),
                        cache_write_tokens: opt("cache_write_tokens"),
                        output_tokens: opt("output_tokens"),
                        cost_minor: if known {
                            x["cost_minor"].as_u64()
                        } else {
                            None
                        },
                        held_minor: 0,
                        priced_under: s(&x["priced_under"]),
                        latency_ms: x["latency_ms"].as_u64(),
                        provider_request_id: x["provider_request_id"].as_str().map(str::to_owned),
                        source: format!("event:{}", e.offset),
                    },
                ));
            }
            "UsageReconciled" if on_request(e) => {
                let x = p(e);
                let Ok(r) = serde_json::from_value::<RunId>(x["run_id"].clone()) else {
                    continue;
                };
                let key = (
                    r,
                    s(&x["plan_id"]),
                    s(&x["slot_id"]),
                    x["attempt"].as_u64().unwrap_or(0) as u32,
                );
                let label = format!(
                    "invoice for {}/{}/{}#{} at event:{}",
                    key.0, key.1, key.2, key.3, e.offset
                );
                match reconciled.entry(key) {
                    std::collections::hash_map::Entry::Occupied(_) => {
                        rec.duplicates_ignored.push(label);
                    }
                    std::collections::hash_map::Entry::Vacant(v) => {
                        v.insert((e.offset, x.clone()));
                    }
                }
            }
            "RunCompleted" | "RunCancelled" | "RunFailed" | "RunSuspended" => {
                if let Some(r) = e.run {
                    run_ended.insert(r);
                    // A run that ended with an invocation still open lost
                    // it: nothing will record that attempt now.
                    if let Some((o, at, t)) = open_invocation.remove(&r) {
                        cut_off.push((o, at, t, r));
                    }
                }
            }
            _ => {}
        }
    }
    let run_is_live = |r: &RunId| !run_ended.contains(r) && pending.is_none();
    for (r, (o, at, t)) in open_invocation {
        // Open at the end of the log: in flight while the run is live, cut
        // off when the run is being ended with this record.
        if run_is_live(&r) {
            let slot = slot_in_force(&activations, r, o);
            attempts.push((
                o,
                at,
                t,
                AttemptRecord {
                    run_id: r.to_string(),
                    plan_id: slot.0,
                    slot_id: slot.1,
                    attempt: 0,
                    outcome: "IN_FLIGHT".into(),
                    usage: "UNKNOWN".into(),
                    source: format!("event:{o}"),
                    ..Default::default()
                },
            ));
        } else {
            cut_off.push((o, at, t, r));
        }
    }
    for (o, at, t, r) in cut_off {
        let slot = slot_in_force(&activations, r, o);
        attempts.push((
            o,
            at,
            t,
            AttemptRecord {
                run_id: r.to_string(),
                plan_id: slot.0,
                slot_id: slot.1,
                attempt: 0,
                outcome: "CUT_OFF".into(),
                usage: "UNKNOWN".into(),
                source: format!("event:{o}"),
                ..Default::default()
            },
        ));
    }
    attempts.sort_by_key(|(o, ..)| *o);

    // Late invoices settle unknown attempts, once.
    for (_, _, _, a) in &mut attempts {
        let Ok(r) = RunId::parse(&a.run_id) else {
            continue;
        };
        if a.usage == "UNKNOWN"
            && a.attempt > 0
            && let Some((o, x)) =
                reconciled.get(&(r, a.plan_id.clone(), a.slot_id.clone(), a.attempt))
        {
            a.usage = "RECONCILED".into();
            a.input_tokens = x["input_tokens"].as_u64();
            a.cached_input_tokens = x["cached_input_tokens"].as_u64();
            a.cache_write_tokens = x["cache_write_tokens"].as_u64();
            a.output_tokens = x["output_tokens"].as_u64();
            a.cost_minor = x["cost_minor"].as_u64();
            a.priced_under = s(&x["priced_under"]);
            a.source = format!("{} reconciled at event:{o}", a.source);
        }
    }
    // What is still unknown holds its slot's reservation.
    let slot_reserve = |run: &str, plan: &str, slot: &str| -> u64 {
        RunId::parse(run)
            .ok()
            .and_then(|r| plans.get(&(r, plan.to_owned())))
            .and_then(|pl| pl.slots.iter().find(|s| s.slot_id == slot))
            .map_or(0, |s| s.budget.reserved.minor_units)
    };
    for (_, _, _, a) in &mut attempts {
        if a.usage == "UNKNOWN" {
            a.held_minor = slot_reserve(&a.run_id, &a.plan_id, &a.slot_id);
        }
    }

    // -- Versions and decisions.
    for key in &plan_order {
        let Some(plan) = plans.get(key) else { continue };
        let pv = &plan.provenance;
        rec.versions.plans.push(PlanVersions {
            plan_id: plan.plan_id.clone(),
            run_id: plan.run_id.to_string(),
            routing_epoch: plan.routing_epoch,
            policy: pv.policy_version.clone(),
            registry_generation: pv.registry_generation.clone(),
            profiler: pv.profiler_version.clone(),
            statistics: pv.statistics_version.clone(),
            compiler: pv.compiler_version.clone(),
            gate: pv.gate_version.clone(),
            risk: pv.risk_version.clone(),
            content_digest: plan.content_digest.clone(),
        });
        let adm = admissions.get(key).cloned().unwrap_or_default();
        push_unique(&mut rec.versions.statistics, s(&adm["stats_version"]));
        push_unique(&mut rec.versions.thresholds, s(&adm["thresholds_version"]));
        let d = decisions.get(key);
        rec.decisions.push(DecisionRecord {
            plan_id: plan.plan_id.clone(),
            run_id: plan.run_id.to_string(),
            routing_epoch: plan.routing_epoch,
            recorded: d.is_some(),
            selection: d.map(|d| s(&d["selection"])).unwrap_or_default(),
            feasibility: s(&adm["feasibility"]),
            target_met: adm["target_met"].as_bool().unwrap_or(false),
            chosen_quality_mean_bp: d
                .and_then(|d| d["chosen_quality_mean_bp"].as_u64())
                .map(|v| v as u32),
            chosen_quality_lcb_bp: adm["quality_lcb_bp"].as_u64().unwrap_or(0) as u32,
            choice_probability_bp: d
                .and_then(|d| d["choice_probability_bp"].as_u64())
                .map(|v| v as u32),
            routing_latency_ms: d.and_then(|d| d["routing_latency_ms"].as_u64()),
            candidates: d
                .and_then(|d| serde_json::from_value(d["candidates"].clone()).ok())
                .unwrap_or_default(),
            reserved_minor: u(&adm["reserved_minor"]),
        });
    }
    if let Some(first) = plan_order.first().and_then(|k| plans.get(k)) {
        rec.cost.currency = first.total_budget.currency.clone();
        rec.cost.scale = first.total_budget.scale;
    }

    // -- Other observations, in log order.
    let mut continuations: Vec<(u64, Value)> = Vec::new();
    let mut review_legs: Vec<(u64, i64, Value)> = Vec::new();
    let mut reviewer_results: Vec<(u64, Value)> = Vec::new();
    let mut revisions: Vec<(u64, Value)> = Vec::new();
    let mut gates: Vec<(u64, Value)> = Vec::new();
    let mut decisions_by_person: Vec<(u64, i64, Value)> = Vec::new();
    let mut reviewer_decisions: Vec<(u64, Value)> = Vec::new();
    let mut completion: Option<(u64, Value)> = None;
    let mut completions: Vec<(u64, Value)> = Vec::new();
    let mut first_ready: Option<u64> = None;
    let mut returned_to_work: Vec<u64> = Vec::new();
    let mut tool_started: HashMap<[u8; 16], (i64, bool)> = HashMap::new();
    let mut stop_marks: Vec<(u64, &'static str)> = Vec::new();
    let mut starts: Vec<u64> = Vec::new();
    let (mut first_at, mut last_at) = (i64::MAX, i64::MIN);
    for e in &evs {
        first_at = first_at.min(e.at);
        last_at = last_at.max(e.at);
        let x = p(e);
        let request = on_request(e);
        match e.ty {
            "ContinuationActivated" if request => continuations.push((e.offset, x.clone())),
            "ReviewLegActivated" if request => review_legs.push((e.offset, e.at, x.clone())),
            "ReviewerResultRecorded" if request => reviewer_results.push((e.offset, x.clone())),
            "RevisionActivated" if request => revisions.push((e.offset, x.clone())),
            "AcceptanceGateEvaluated" if request => gates.push((e.offset, x.clone())),
            "RealizedRiskDerived" if request => {
                rec.risks.push(RiskObservation {
                    realized_risk_ref: s(&x["realized_risk_ref"]),
                    candidate_revision: u(&x["candidate_revision"]),
                    level: s(&x["level"]),
                    minimum_assurance: s(&x["minimum_assurance"]),
                    policy_version: s(&x["policy_version"]),
                    rules_version: s(&x["rules_version"]),
                    correction: "NONE_OBSERVED".into(),
                });
                push_unique(
                    &mut rec.versions.risk,
                    format!("{}/{}", s(&x["policy_version"]), s(&x["rules_version"])),
                );
            }
            "RequestProfiled" if request => push_unique(
                &mut rec.versions.profiler,
                format!("{}@{}", s(&x["profiler_version"]), s(&x["cohort_version"])),
            ),
            "SkillSelected" if request => push_unique(
                &mut rec.versions.skills,
                format!(
                    "{}@{}#{}",
                    s(&x["name"]),
                    s(&x["version"]),
                    s(&x["content_hash"])
                ),
            ),
            "RouteReevaluated" if request => {
                push_unique(&mut rec.versions.economics, s(&x["economics_version"]));
            }
            "VerificationRunRecorded" | "VerificationBaselineRecorded" => {
                rec.cost.verification.runs += 1;
                for c in x["checks"].as_array().into_iter().flatten() {
                    rec.cost.verification.checks += 1;
                    rec.cost.verification.process_ms += u(&c["duration_ms"]);
                }
                if request && x["stage"].as_str() == Some("COMPLETION") {
                    completion = Some((e.offset, x.clone()));
                    completions.push((e.offset, x.clone()));
                }
            }
            "RepairAttemptRecorded" if request => rec.request.repair_turns += 1,
            "ReviewDecisionRecorded" if request => {
                if s(&x["provenance"]).starts_with("user") {
                    decisions_by_person.push((e.offset, e.at, x.clone()));
                } else {
                    reviewer_decisions.push((e.offset, x.clone()));
                }
            }
            "TaskReadyForReview" if request => {
                first_ready.get_or_insert(e.offset);
            }
            "TaskReturnedToWork" if request => returned_to_work.push(e.offset),
            "RunStarted" | "RunResumed" if request => starts.push(e.offset),
            "NoProgressDetected" if request => stop_marks.push((e.offset, "NO_PROGRESS")),
            "HarnessBudgetExhausted" if request => stop_marks.push((e.offset, "BUDGET_EXHAUSTED")),
            "UserQuestionAsked" if request => stop_marks.push((e.offset, "NEEDS_INPUT")),
            "UserQuestionAnswered" if request => stop_marks.push((e.offset, "ANSWERED")),
            "ForgePullRequestUpdated" | "ForgePullRequestOpened" if request => {
                let state = s(&x["state"]).to_ascii_lowercase();
                if state == "merged" || x["result"]["merged"].as_bool() == Some(true) {
                    rec.signals.merge = Some(true);
                } else if state == "closed" {
                    rec.signals.merge = Some(false);
                }
            }
            "ToolCallProposed" => {
                tool_started.insert(e.aggregate_id, (e.at, request));
            }
            "ToolCallSucceeded" | "ToolCallFailed" | "ToolCallCancelled" => {
                if let Some((at, req)) = tool_started.remove(&e.aggregate_id) {
                    let ms = u64::try_from(e.at - at).unwrap_or(0);
                    if req {
                        rec.cost.tools.request_calls += 1;
                        rec.cost.tools.request_ms += ms;
                    } else {
                        rec.cost.tools.reviewer_calls += 1;
                        rec.cost.tools.reviewer_ms += ms;
                    }
                }
            }
            _ => {}
        }
        // Human interventions and corrections: by reference, never content.
        if request && matches!(e.actor, Actor::User(_)) {
            let kind = e.ty;
            if matches!(
                kind,
                "TaskSteered"
                    | "TaskInputQueued"
                    | "TaskPauseRequested"
                    | "TaskResumeRequested"
                    | "TaskCancelRequested"
                    | "UserQuestionAnswered"
                    | "UserPatchApplied"
                    | "ApprovalResolved"
                    | "CheckpointRestored"
            ) {
                let sig = Signal {
                    kind: kind.to_owned(),
                    value: String::new(),
                    source: format!("event:{}", e.offset),
                    at_ms: e.at,
                    actor: "user".into(),
                };
                let after_ready = first_ready.is_some_and(|r| e.offset > r);
                if after_ready && matches!(kind, "TaskSteered" | "TaskInputQueued") {
                    rec.signals.corrections.push(sig.clone());
                }
                if after_ready && matches!(kind, "CheckpointRestored" | "UserPatchApplied") {
                    rec.signals.reverts.push(sig.clone());
                }
                rec.signals.interventions.push(sig);
            }
        }
    }
    rec.signals.unavailable = unavailable;
    rec.request.human_interventions = rec.signals.interventions.len() as u32;
    for (o, at, x) in &decisions_by_person {
        rec.signals.explicit.push(Signal {
            kind: "ReviewDecisionRecorded".into(),
            value: s(&x["decision"]),
            source: format!("event:{o}"),
            at_ms: *at,
            actor: "user".into(),
        });
    }

    // -- Request outcome.
    let task_state = pending.map_or_else(|| state_name(task.state), |p| p.task_state);
    rec.request.task_state = task_state.to_owned();
    rec.request.terminal = matches!(task_state, "Completed" | "Cancelled" | "Failed");
    // Why the last run stopped short, in the vocabulary the run's end uses
    // (`Pending::stop`). Every stop waits and asks for attention, so the
    // wait reason and the attention flag do not tell them apart; the events
    // that do are read, newest first, after the run last (re)started.
    let last_start = starts.last().copied().unwrap_or(0);
    let marks: Vec<&str> = stop_marks
        .iter()
        .filter(|(o, _)| *o > last_start)
        .map(|(_, c)| *c)
        .collect();
    let asked = marks.iter().filter(|c| **c == "NEEDS_INPUT").count();
    let answered = marks.iter().filter(|c| **c == "ANSWERED").count();
    let live_stop = match task.state {
        TaskState::Waiting(reason) => {
            if asked > answered {
                "NEEDS_INPUT"
            } else if let Some(m) = marks
                .iter()
                .rev()
                .find(|c| matches!(**c, "NO_PROGRESS" | "BUDGET_EXHAUSTED"))
            {
                m
            } else {
                match reason {
                    WaitReason::Provider => "PROVIDER_FAILED",
                    WaitReason::Capacity => "CAPACITY",
                    WaitReason::Approval => "NEEDS_APPROVAL",
                    _ => "NEEDS_ATTENTION",
                }
            }
        }
        _ => "",
    };
    rec.request.stop = pending.map_or_else(|| live_stop.to_owned(), |p| p.stop.clone());
    let (verification, revision) = match &completion {
        None => ("NOT_RUN".to_owned(), None),
        Some((_, x)) => {
            let checks = x["checks"].as_array().cloned().unwrap_or_default();
            let passed = checks.iter().filter(|c| c["status"] == "PASS").count();
            let v = match s(&x["status"]).as_str() {
                "PASSED" if passed == 0 => "NO_CHECKS",
                "PASSED" => "PASSED",
                _ => "FAILED",
            };
            let kind_ok = |k: &str| -> Option<bool> {
                let of: Vec<&Value> = checks
                    .iter()
                    .filter(|c| c["kind"].as_str() == Some(k))
                    .filter(|c| {
                        matches!(
                            c["status"].as_str(),
                            Some("PASS" | "FAIL" | "ERROR" | "TIMEOUT")
                        )
                    })
                    .collect();
                (!of.is_empty()).then(|| of.iter().all(|c| c["status"] == "PASS"))
            };
            rec.signals.tests_passed = kind_ok("tests");
            rec.signals.build_passed = kind_ok("build");
            (
                v.to_owned(),
                s(&x["candidate_revision"]).parse::<u64>().ok(),
            )
        }
    };
    rec.request.verification = verification.clone();
    rec.request.candidate_revision =
        revision.or_else(|| gates.last().map(|(_, g)| u(&g["candidate_revision"])));
    let at_revision =
        |g: &Value| Some(u(&g["candidate_revision"])) == rec.request.candidate_revision;
    let gate_rejects = gates
        .iter()
        .rev()
        .find(|(_, g)| at_revision(g))
        .is_some_and(|(_, g)| g["verdict"] == "REJECT");
    let reviewer_returned = reviewer_decisions
        .iter()
        .rev()
        .find(|(_, d)| Some(u(&d["candidate_revision"])) == rec.request.candidate_revision)
        .is_some_and(|(_, d)| d["decision"] == "RETURN");
    rec.request.escalations = continuations.len() as u32;
    rec.request.revisions = revisions.len() as u32;
    let reported: HashSet<String> = reviewer_results
        .iter()
        .map(|(_, r)| s(&r["review_task_id"]))
        .collect();
    rec.request.review_pending = pending.is_some_and(|p| p.review_pending)
        || review_legs.iter().any(|(_, _, l)| {
            let t = s(&l["review_task_id"]);
            !reported.contains(&t)
                && TaskId::parse(&t)
                    .ok()
                    .and_then(|id| store.task(&id).ok().flatten())
                    .is_some_and(|rt| {
                        !matches!(
                            rt.state,
                            TaskState::Completed | TaskState::Cancelled | TaskState::Failed
                        )
                    })
        });
    let candidate_stands = matches!(task_state, "ReadyForReview" | "Completed");
    rec.request.verified_success =
        candidate_stands && verification == "PASSED" && !gate_rejects && !reviewer_returned;
    rec.request.first_pass_success = rec.request.verified_success
        && rec.request.repair_turns == 0
        && rec.request.escalations == 0
        && rec.request.revisions == 0;
    let open_stop = matches!(
        rec.request.stop.as_str(),
        "NEEDS_INPUT" | "NEEDS_APPROVAL" | "CAPACITY"
    );
    rec.request.final_outcome = if task_state == "Cancelled" {
        "cancelled"
    } else if rec.request.review_pending {
        "open"
    } else if rec.request.verified_success {
        "pass"
    } else if candidate_stands {
        "partial"
    } else if matches!(task_state, "Created" | "Queued" | "Running") || open_stop {
        "open"
    } else {
        "fail"
    }
    .to_owned();

    // -- Legs. Solver segments by first appearance on the request's runs;
    // a new run of the request is a new leg (a revision or a return to
    // work), and consecutive segments of one binding within a run are one.
    let mut runs_in_order: Vec<(u64, RunId)> = run_first
        .iter()
        .filter(|(_, (_, t))| *t == task_id)
        .map(|(r, (o, _))| (*o, *r))
        .collect();
    runs_in_order.sort();
    let run_cause = |r: RunId| -> &'static str {
        let idx = runs_in_order.iter().position(|(_, x)| *x == r).unwrap_or(0);
        if idx == 0 {
            return "INITIAL";
        }
        let (prev_start, _) = runs_in_order[idx - 1];
        let (start, _) = runs_in_order[idx];
        if revisions.iter().any(|(o, _)| *o > prev_start && *o < start) {
            "REVISION"
        } else if returned_to_work
            .iter()
            .any(|o| *o > prev_start && *o < start)
        {
            "RETURNED"
        } else {
            "INITIAL"
        }
    };
    // (first offset, run, plan, slot)
    let mut segments: Vec<(u64, RunId, String, String)> = Vec::new();
    let mut seen_seg: HashSet<(RunId, String, String)> = HashSet::new();
    for (o, r, pl, sl, _) in &activations {
        if seen_seg.insert((*r, pl.clone(), sl.clone())) {
            segments.push((*o, *r, pl.clone(), sl.clone()));
        }
    }
    for (o, _, t, a) in &attempts {
        if *t != task_id {
            continue;
        }
        let Ok(r) = RunId::parse(&a.run_id) else {
            continue;
        };
        if seen_seg.insert((r, a.plan_id.clone(), a.slot_id.clone())) {
            segments.push((*o, r, a.plan_id.clone(), a.slot_id.clone()));
        }
    }
    segments.sort_by_key(|(o, ..)| *o);
    let slot_of = |r: RunId, pl: &str, sl: &str| {
        plans
            .get(&(r, pl.to_owned()))
            .and_then(|p| p.slots.iter().find(|s| s.slot_id == sl).cloned())
    };
    let mut legs: Vec<(u64, Leg)> = Vec::new();
    for (o, r, pl, sl) in &segments {
        if run_first.get(r).is_some_and(|(_, t)| *t != task_id) {
            continue;
        }
        let slot = slot_of(*r, pl, sl);
        let role = slot
            .as_ref()
            .map_or_else(|| "solver".to_owned(), |s| s.role.clone());
        if role == "reviewer" {
            // A reviewer activation is its own leg, below.
            continue;
        }
        let binding = slot
            .as_ref()
            .map_or_else(String::new, |s| format!("{}/{}", s.endpoint, s.model));
        let trigger = match run_cause(*r) {
            "INITIAL" => slot
                .as_ref()
                .map_or_else(|| "INITIAL".to_owned(), |s| trigger_name(&s.trigger)),
            other => other.to_owned(),
        };
        let seg = Segment {
            run_id: r.to_string(),
            plan_id: pl.clone(),
            slot_id: sl.clone(),
            activations: activations
                .iter()
                .filter(|(_, ar, ap, asl, _)| ar == r && ap == pl && asl == sl)
                .count() as u32,
            reserved_minor: activations
                .iter()
                .filter(|(_, ar, ap, asl, _)| ar == r && ap == pl && asl == sl)
                .map(|(.., m)| *m)
                .sum(),
        };
        // An initial slot re-admitted in a reconciled transaction on the
        // same binding continues the leg; a continuation slot never merges
        // into the leg it continues.
        let continues = legs.last().is_some_and(|(_, l)| {
            l.role == role
                && l.binding == binding
                && l.trigger == trigger
                && l.segments.last().is_some_and(|x| x.run_id == seg.run_id)
        });
        if continues {
            if let Some((_, l)) = legs.last_mut() {
                l.segments.push(seg);
            }
        } else {
            legs.push((
                *o,
                Leg {
                    role,
                    trigger,
                    binding,
                    task_id: task_id.to_string(),
                    segments: vec![seg],
                    ..Default::default()
                },
            ));
        }
    }
    for (o, _, l) in &review_legs {
        let t = s(&l["review_task_id"]);
        legs.push((
            *o,
            Leg {
                role: "reviewer".into(),
                trigger: "REVIEW_REQUIRED".into(),
                binding: format!("{}/{}", s(&l["endpoint"]), s(&l["model"])),
                task_id: t,
                segments: vec![Segment {
                    run_id: String::new(),
                    plan_id: s(&l["plan_id"]),
                    slot_id: s(&l["slot_id"]),
                    activations: 1,
                    reserved_minor: u(&l["reserved_minor"]),
                }],
                ..Default::default()
            },
        ));
    }
    legs.sort_by_key(|(o, _)| *o);
    // Attempts to their legs.
    for (_, at, t, a) in &attempts {
        let leg = if *t == task_id {
            legs.iter_mut().find(|(_, l)| {
                l.task_id == task_id.to_string()
                    && l.role != "reviewer"
                    && l.segments.iter().any(|x| {
                        x.run_id == a.run_id && x.plan_id == a.plan_id && x.slot_id == a.slot_id
                    })
            })
        } else {
            let tid = t.to_string();
            legs.iter_mut()
                .find(|(_, l)| l.role == "reviewer" && l.task_id == tid)
        };
        let Some((_, l)) = leg else { continue };
        if l.started_at_ms == 0 || *at < l.started_at_ms {
            l.started_at_ms = *at;
        }
        l.ended_at_ms = l.ended_at_ms.max(*at);
        l.attempts.push(a.clone());
    }
    for (_, l) in &mut legs {
        for a in &l.attempts {
            l.input_tokens += a.input_tokens.unwrap_or(0);
            l.cached_input_tokens += a.cached_input_tokens.unwrap_or(0);
            l.cache_write_tokens += a.cache_write_tokens.unwrap_or(0);
            l.output_tokens += a.output_tokens.unwrap_or(0);
            l.latency_ms += a.latency_ms.unwrap_or(0);
            match (a.usage.as_str(), a.cost_minor) {
                ("REPORTED" | "RECONCILED", Some(c)) => l.cost_minor += c,
                ("UNKNOWN", _) if a.held_minor > 0 => l.held_minor += a.held_minor,
                _ => l.unpriced_attempts += 1,
            }
            push_unique(&mut rec.versions.priced_under, a.priced_under.clone());
        }
        l.retries = u32::try_from(l.attempts.iter().filter(|a| a.attempt > 1).count()).unwrap_or(0);
    }
    // Reviewer tool time, per review task.
    {
        let mut per_task: HashMap<String, (u32, u64)> = HashMap::new();
        let mut started: HashMap<[u8; 16], i64> = HashMap::new();
        for e in &evs {
            let Some(t) = e.task.filter(|t| *t != task_id) else {
                continue;
            };
            match e.ty {
                "ToolCallProposed" => {
                    started.insert(e.aggregate_id, e.at);
                }
                "ToolCallSucceeded" | "ToolCallFailed" | "ToolCallCancelled" => {
                    if let Some(at) = started.remove(&e.aggregate_id) {
                        let x = per_task.entry(t.to_string()).or_default();
                        x.0 += 1;
                        x.1 += u64::try_from(e.at - at).unwrap_or(0);
                    }
                }
                _ => {}
            }
        }
        for (_, l) in legs.iter_mut().filter(|(_, l)| l.role == "reviewer") {
            if let Some((n, ms)) = per_task.get(&l.task_id) {
                l.tool_calls = *n;
                l.tool_ms = *ms;
            }
        }
    }

    // -- Leg outcomes.
    let solver_idx: Vec<usize> = legs
        .iter()
        .enumerate()
        .filter(|(_, (_, l))| l.role != "reviewer")
        .map(|(i, _)| i)
        .collect();
    // A solver leg's outcome is what came after it: an escalation means
    // the gate rejected it, a revision means the reviewer returned it, a
    // new run otherwise means a person returned it; the last one's is the
    // request's.
    for (pos, &i) in solver_idx.iter().enumerate() {
        let outcome = {
            let next = solver_idx.get(pos + 1).map(|&j| legs[j].1.trigger.as_str());
            if next == Some("QUALITY_REJECTED") || next == Some("LEG_FAILED") {
                "REJECTED"
            } else if next == Some("REVISION") {
                "REVISED"
            } else if next.is_some() {
                "SUPERSEDED"
            } else {
                match rec.request.final_outcome.as_str() {
                    "pass" => "SUCCEEDED",
                    "cancelled" => "CANCELLED",
                    "open" => "OPEN",
                    "partial" => "UNVERIFIED",
                    _ => "FAILED",
                }
            }
        };
        legs[i].1.outcome = outcome.to_owned();
    }
    for (_, l) in legs.iter_mut().filter(|(_, l)| l.role == "reviewer") {
        l.outcome = reviewer_results
            .iter()
            .rev()
            .find(|(_, r)| s(&r["review_task_id"]) == l.task_id)
            .map(|(_, r)| s(&r["verdict"]))
            .unwrap_or_else(|| {
                let ended = TaskId::parse(&l.task_id)
                    .ok()
                    .and_then(|id| store.task(&id).ok().flatten())
                    .is_some_and(|rt| {
                        matches!(
                            rt.state,
                            TaskState::Completed | TaskState::Cancelled | TaskState::Failed
                        )
                    });
                if ended { "NO_REPORT" } else { "OPEN" }.to_owned()
            });
    }
    let decided = |o: &str| -> Option<bool> {
        match o {
            "SUCCEEDED" => Some(true),
            "OPEN" | "CANCELLED" => None,
            _ => Some(false),
        }
    };
    if let Some(&first) = solver_idx.first() {
        rec.legs.initial_leg = legs[first].1.binding.clone();
        rec.legs.initial_leg_success = decided(&legs[first].1.outcome);
    }
    for (_, c) in &continuations {
        let to = legs.iter().find(|(_, l)| {
            l.role != "reviewer"
                && l.segments
                    .iter()
                    .any(|x| x.plan_id == s(&c["plan_id"]) && x.slot_id == s(&c["slot_id"]))
        });
        let from_binding = plans
            .iter()
            .find(|((_, pid), _)| *pid == s(&c["from_plan_id"]))
            .and_then(|(_, p)| p.slots.iter().find(|x| x.slot_id == s(&c["from_slot_id"])))
            .map(|x| format!("{}/{}", x.endpoint, x.model))
            .unwrap_or_default();
        rec.legs.escalations.push(EscalationOutcome {
            plan_id: s(&c["plan_id"]),
            from_slot: s(&c["from_slot_id"]),
            to_slot: s(&c["slot_id"]),
            from_binding,
            to_binding: format!("{}/{}", s(&c["endpoint"]), s(&c["model"])),
            cause: s(&c["cause"]),
            gate_ref: s(&c["gate_ref"]),
            success: to.and_then(|(_, l)| decided(&l.outcome)),
        });
    }
    for (_, _, l) in &review_legs {
        let t = s(&l["review_task_id"]);
        let r = reviewer_results
            .iter()
            .rev()
            .find(|(_, r)| s(&r["review_task_id"]) == t)
            .map(|(_, r)| r);
        rec.legs.reviews.push(ReviewOutcome {
            slot_id: s(&l["slot_id"]),
            binding: format!("{}/{}", s(&l["endpoint"]), s(&l["model"])),
            review_task_id: t,
            candidate_revision: u(&l["candidate_revision"]),
            verdict: r.map(|r| s(&r["verdict"])),
            validated_findings: r.map_or(0, |r| u(&r["validated_findings"]) as u32),
            unsupported_findings: r.map_or(0, |r| u(&r["unsupported_findings"]) as u32),
            highest_severity: r.map(|r| s(&r["highest_severity"])).unwrap_or_default(),
            genuine_defect_found: r.map(|r| u(&r["validated_findings"]) > 0),
        });
    }
    for (o, v) in &revisions {
        let next = reviewer_results
            .iter()
            .find(|(ro, _)| ro > o)
            .map(|(_, r)| s(&r["verdict"]));
        rec.legs.revisions.push(RevisionOutcome {
            revision: u(&v["revision"]) as u32,
            max_revisions: u(&v["max_revisions"]) as u32,
            binding: format!("{}/{}", s(&v["endpoint"]), s(&v["model"])),
            success: match next.as_deref() {
                Some("PASS") => Some(true),
                Some(_) => Some(false),
                None if rec.request.final_outcome == "fail" => Some(false),
                None => None,
            },
        });
    }

    // -- Gate observations and their later corrections.
    for (o, g) in &gates {
        let rev = u(&g["candidate_revision"]);
        let verdict = s(&g["verdict"]);
        let later_return = decisions_by_person.iter().find(|(d, _, x)| {
            d > o && u(&x["candidate_revision"]) == rev && x["decision"] == "RETURN"
        });
        let later_pass = completions.iter().find(|(c, x)| {
            c > o
                && s(&x["candidate_revision"]).parse::<u64>().ok() == Some(rev)
                && x["status"] == "PASSED"
        });
        let (correction, source) = match verdict.as_str() {
            "ACCEPT" => later_return.map_or(("NONE_OBSERVED", None), |(d, ..)| {
                ("ACCEPTED_LATER_SHOWN_WRONG", Some(format!("event:{d}")))
            }),
            "REJECT" => later_pass.map_or(("NONE_OBSERVED", None), |(c, _)| {
                ("REJECTED_LATER_SHOWN_VALID", Some(format!("event:{c}")))
            }),
            _ => ("NONE_OBSERVED", None),
        };
        push_unique(&mut rec.versions.gate, s(&g["gate_version"]));
        rec.gates.push(GateObservation {
            gate_ref: s(&g["gate_ref"]),
            gate_version: s(&g["gate_version"]),
            candidate_revision: rev,
            verdict,
            trigger: s(&g["trigger"]),
            required_assurance: s(&g["required_assurance"]),
            independent_review_required: g["independent_review_required"]
                .as_bool()
                .unwrap_or(false),
            human_required: g["human_required"].as_bool().unwrap_or(false),
            reject_reasons: g["reject_reasons"]
                .as_array()
                .map(|a| a.iter().map(s).collect())
                .unwrap_or_default(),
            realized_risk_ref: s(&g["realized_risk_ref"]),
            correction: correction.to_owned(),
            correction_source: source,
        });
    }

    // -- Cost, reservations.
    for (i, (_, l)) in legs.iter_mut().enumerate() {
        l.ordinal = i as u32 + 1;
    }
    rec.executed_path = legs.into_iter().map(|(_, l)| l).collect();
    rec.path_label = crate::routing::path_label(
        &rec.executed_path
            .iter()
            .map(|l| (l.role.clone(), l.trigger.clone(), l.binding.clone()))
            .collect::<Vec<_>>(),
    );
    for l in &rec.executed_path {
        rec.cost.inference_minor += l.cost_minor;
        rec.cost.held_unknown_minor += l.held_minor;
        rec.cost.unpriced_attempts += l.unpriced_attempts;
        rec.cost.input_tokens += l.input_tokens;
        rec.cost.cached_input_tokens += l.cached_input_tokens;
        rec.cost.cache_write_tokens += l.cache_write_tokens;
        rec.cost.output_tokens += l.output_tokens;
        if l.role == "reviewer" {
            rec.cost.reviewer_minor += l.cost_minor;
        } else {
            rec.cost.solver_minor += l.cost_minor;
        }
    }
    rec.cost.total_minor = rec.cost.inference_minor + rec.cost.held_unknown_minor;
    rec.cost.complete = rec.cost.held_unknown_minor == 0 && rec.cost.unpriced_attempts == 0;
    rec.cost.verification.basis = "LOCAL_PROCESS_UNPRICED".into();
    if last_at >= first_at && first_at != i64::MAX {
        let end = if pending.is_some() {
            modbit_domain::Timestamp::now().0.max(last_at)
        } else {
            last_at
        };
        rec.cost.wall_clock_ms = u64::try_from(end - first_at).unwrap_or(0);
    }
    rec.reservations.activated_minor = activations
        .iter()
        .filter(|(_, r, ..)| run_first.get(r).is_some_and(|(_, t)| *t == task_id))
        .map(|(.., m)| *m)
        .sum::<u64>()
        + review_legs
            .iter()
            .map(|(_, _, l)| u(&l["reserved_minor"]))
            .sum::<u64>();
    rec.reservations.settled_minor = rec.cost.inference_minor;
    rec.reservations.held_minor = rec.cost.held_unknown_minor;
    rec.reservations.released_minor = (rec.request.final_outcome != "open").then(|| {
        rec.reservations
            .activated_minor
            .saturating_sub(rec.reservations.settled_minor + rec.reservations.held_minor)
    });

    // -- Counterfactual: labelled, never a saving.
    let first_decision = rec.decisions.iter().find(|d| d.recorded);
    rec.counterfactual.observed_label = "NOT_EXECUTED".into();
    rec.counterfactual.label = match first_decision {
        None => "NO_CANDIDATES".into(),
        Some(d) => {
            let frontier = d
                .candidates
                .iter()
                .filter(|c| c.hard_eligible && c.bindings.len() == 1)
                .max_by_key(|c| (c.quality_mean_bp, c.expected_cost_minor));
            match frontier {
                Some(f) if f.plan_id != d.plan_id => {
                    rec.counterfactual.direct_frontier_minor = Some(f.expected_cost_minor);
                    rec.counterfactual.direct_frontier_binding = f.bindings.first().cloned();
                    "ESTIMATED".into()
                }
                Some(_) => "CHOSEN_WAS_FRONTIER".into(),
                None => "NO_CANDIDATES".into(),
            }
        }
    };
    rec.reward.reward_version = "none".into();

    // -- What could not be observed.
    let mut missing = Vec::new();
    if rec.signals.explicit.is_empty() {
        missing.push("explicit_feedback".to_owned());
    }
    if rec.signals.tests_passed.is_none() {
        missing.push("tests_passed".to_owned());
    }
    if rec.signals.build_passed.is_none() {
        missing.push("build_passed".to_owned());
    }
    if rec.signals.merge.is_none() {
        missing.push("merge_signal".to_owned());
    }
    missing.push("keep_signal: no keep window is configured".to_owned());
    missing.push("composite_reward: no reward version is active".to_owned());
    if rec.cost.unpriced_attempts > 0 {
        missing.push(format!(
            "price: {} attempt(s) have no price and no reservation to hold",
            rec.cost.unpriced_attempts
        ));
    }
    if rec.decisions.iter().any(|d| !d.recorded) {
        missing.push("routing_decision: a plan was admitted without a decision record".to_owned());
    }
    for u in &rec.signals.unavailable {
        missing.push(format!("unavailable: {u}"));
    }
    rec.missing_signals = missing;
    Some(rec)
}

/// The slot in force on a run at an offset: its latest activation before
/// it, else the direct slot.
fn slot_in_force(
    activations: &[(u64, RunId, String, String, u64)],
    run: RunId,
    offset: u64,
) -> (String, String) {
    activations
        .iter()
        .rfind(|(o, r, ..)| *r == run && *o < offset)
        .map_or_else(
            || {
                (
                    modbit_domain::routing::direct_plan_id(run),
                    modbit_domain::routing::DIRECT_SLOT.to_owned(),
                )
            },
            |(_, _, p, s, _)| (p.clone(), s.clone()),
        )
}

fn trigger_name(t: &modbit_domain::routing::Trigger) -> String {
    serde_json::to_value(t)
        .ok()
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_else(|| format!("{t:?}"))
}

// ---- Recording ----

/// The record as a `RequestOutcomeRecorded` event, with the record stored
/// as an object. `None` for a review task (its reviewer leg is part of the
/// request it reviews) and for a task the log cannot read.
pub(crate) fn record_event(
    store: &EventStore,
    tenant: TenantId,
    task_id: TaskId,
    trigger: &str,
    pending: Option<&Pending>,
    actor: &Actor,
) -> Option<NewEvent> {
    let task = store.task(&task_id).ok()??;
    if crate::critique::is_review(&task) {
        return None;
    }
    let rec = derive(store, tenant, task_id, pending)?;
    let bytes = serde_json::to_vec(&rec).ok()?;
    let record_ref = store.objects().put(&bytes).ok()?;
    Some(typed(
        "RequestOutcomeRecorded",
        &TaskEvent::RequestOutcomeRecorded {
            record_ref,
            accounting_version: ACCOUNTING_VERSION.to_owned(),
            trigger: trigger.to_owned(),
            final_outcome: rec.request.final_outcome.clone(),
            verified_success: rec.request.verified_success,
            first_pass_success: rec.request.first_pass_success,
            initial_leg_success: rec.legs.initial_leg_success,
            total_minor: rec.cost.total_minor,
            unknown_minor: rec.cost.held_unknown_minor,
            currency: rec.cost.currency.clone(),
            scale: rec.cost.scale,
            missing_signals: rec.missing_signals.clone(),
        },
        actor.clone(),
    ))
}

/// Append a fresh record for a request on its own (a review concluded).
pub(crate) async fn append_record(core: &Core, task_id: TaskId, trigger: &str, actor: &Actor) {
    let mut store = core.store.lock().await;
    let Ok(Some(task)) = store.task(&task_id) else {
        return;
    };
    let Some(ev) = record_event(&store, core.tenant_id, task_id, trigger, None, actor) else {
        return;
    };
    let _ = append(
        &mut store,
        core,
        Lineage::task(core.tenant_id, task.session_id, task_id),
        AggregateType::Task,
        *task_id.as_bytes(),
        vec![ev],
    );
}

// ---- Surface ----

/// The latest durable record of a request: its object ref and content.
fn latest_recorded(store: &EventStore, task_id: TaskId) -> Option<(String, String)> {
    let events = store
        .read_aggregate(task_id.as_bytes(), 0, usize::MAX)
        .ok()?;
    let e = events
        .iter()
        .rev()
        .find(|e| e.envelope.event_type == "RequestOutcomeRecorded")?;
    let p = store.payload(&e.envelope).ok()?;
    let r = s(&p["record_ref"]);
    let json = store
        .objects()
        .get(&r)
        .ok()
        .and_then(|b| String::from_utf8(b).ok())
        .unwrap_or_default();
    Some((r, json))
}

/// `GetRequestOutcome`.
pub(crate) async fn view(core: &Core, task_id: TaskId) -> wire::RequestOutcomeView {
    let store = core.store.lock().await;
    let Some(rec) = derive(&store, core.tenant_id, task_id, None) else {
        return wire::RequestOutcomeView::default();
    };
    let (recorded_ref, recorded_json) = latest_recorded(&store, task_id).unwrap_or_default();
    wire::RequestOutcomeView {
        found: true,
        accounting_version: rec.accounting_version.clone(),
        record_json: serde_json::to_string(&rec).unwrap_or_default(),
        recorded_ref,
        recorded_json,
        final_outcome: rec.request.final_outcome.clone(),
        verified_success: rec.request.verified_success,
        first_pass_success: rec.request.first_pass_success,
        initial_leg_success: rec
            .legs
            .initial_leg_success
            .map(|b| b.to_string())
            .unwrap_or_default(),
        total_minor: rec.cost.total_minor,
        unknown_minor: rec.cost.held_unknown_minor,
        currency: rec.cost.currency.clone(),
        scale: u32::from(rec.cost.scale),
        missing_signals: rec.missing_signals.clone(),
        path_label: rec.path_label.clone(),
    }
}

/// `ReconcileUsage`: a late invoice for an attempt of unknown usage.
pub(crate) async fn reconcile(
    core: &Core,
    p: &wire::ReconcileUsage,
    task_id: TaskId,
    run_id: RunId,
    actor: &Actor,
) -> Result<wire::UsageReconciliationView, (String, String)> {
    let mut store = core.store.lock().await;
    let task = store
        .task(&task_id)
        .ok()
        .flatten()
        .ok_or_else(|| ("UNKNOWN_TASK".to_owned(), task_id.to_string()))?;
    if crate::critique::is_review(&task) {
        return Err((
            "REVIEW_TASK".into(),
            "a review task's usage is accounted on the request it reviews; reconcile against that request".into(),
        ));
    }
    if p.cached_input_tokens.saturating_add(p.cache_write_tokens) > p.input_tokens {
        return Err((
            "BAD_PAYLOAD".into(),
            "cached and cache-written input are subsets of the total input".into(),
        ));
    }
    // The attempt, on this tenant's log for this request's run (or a
    // review task's run of this request).
    let events = store
        .read_session(&task.session_id, 0, usize::MAX)
        .unwrap_or_default();
    let mut attempt: Option<Value> = None;
    let mut already: Option<Value> = None;
    for e in events
        .iter()
        .filter(|e| e.envelope.tenant_id == core.tenant_id)
    {
        match e.envelope.event_type.as_str() {
            "RoutingAttemptRecorded" if e.envelope.run_id == Some(run_id) => {
                let Ok(x) = store.payload(&e.envelope) else {
                    continue;
                };
                if s(&x["plan_id"]) == p.plan_id
                    && s(&x["slot_id"]) == p.slot_id
                    && x["attempt"].as_u64() == Some(u64::from(p.attempt))
                    && attempt.is_none()
                {
                    attempt = Some(x);
                }
            }
            "UsageReconciled" if e.envelope.task_id == Some(task_id) => {
                let Ok(x) = store.payload(&e.envelope) else {
                    continue;
                };
                if serde_json::from_value::<RunId>(x["run_id"].clone()).ok() == Some(run_id)
                    && s(&x["plan_id"]) == p.plan_id
                    && s(&x["slot_id"]) == p.slot_id
                    && x["attempt"].as_u64() == Some(u64::from(p.attempt))
                    && already.is_none()
                {
                    already = Some(x);
                }
            }
            _ => {}
        }
    }
    let run_task = store
        .runs_for_task(&task_id)
        .unwrap_or_default()
        .iter()
        .any(|r| r.run_id == run_id);
    let Some(attempt) = attempt.filter(|_| run_task) else {
        return Err((
            "NO_SUCH_ATTEMPT".into(),
            format!(
                "no attempt {}/{}#{} on run {run_id} of this request",
                p.plan_id, p.slot_id, p.attempt
            ),
        ));
    };
    if let Some(prior) = &already {
        // Delivered before: charged once, and not again.
        return Ok(wire::UsageReconciliationView {
            reconciled: false,
            duplicate: true,
            cost_minor: prior["cost_minor"].as_u64().unwrap_or(0),
            priced: prior["cost_minor"].as_u64().is_some(),
            priced_under: s(&prior["priced_under"]),
            record_ref: String::new(),
        });
    }
    if attempt["usage_known"].as_bool() == Some(true) {
        return Err((
            "ALREADY_KNOWN".into(),
            "the provider reported this attempt's usage when it ended; a second figure would charge it twice".into(),
        ));
    }
    let recorded_id = attempt["provider_request_id"].as_str().unwrap_or_default();
    if !p.provider_request_id.is_empty()
        && !recorded_id.is_empty()
        && recorded_id != p.provider_request_id
    {
        return Err((
            "REQUEST_ID_MISMATCH".into(),
            format!(
                "the invoice names provider request {} and the attempt recorded {recorded_id}",
                p.provider_request_id
            ),
        ));
    }
    // Priced at the binding of the slot the attempt ran in.
    let binding = store
        .routing_plans(&run_id)
        .unwrap_or_default()
        .into_iter()
        .find(|row| row.plan_id == p.plan_id)
        .and_then(|row| {
            row.slots
                .iter()
                .find(|x| x.slot_id == p.slot_id)
                .map(|x| (x.endpoint.clone(), x.model.clone()))
        });
    let usage = modbit_providers::Usage {
        input_tokens: p.input_tokens,
        output_tokens: p.output_tokens,
        cached_input_tokens: p.cached_input_tokens,
        cache_write_input_tokens: p.cache_write_tokens,
    };
    let priced =
        binding.and_then(|(ep, m)| price(core.gateway.registry().as_ref(), &ep, &m, &usage));
    let invoice_ref = store
        .objects()
        .put(p.invoice_json.as_bytes())
        .map_err(|e| ("STORE".to_owned(), e.to_string()))?;
    let reconciled = typed(
        "UsageReconciled",
        &TaskEvent::UsageReconciled {
            run_id,
            plan_id: p.plan_id.clone(),
            slot_id: p.slot_id.clone(),
            attempt: p.attempt,
            provider_request_id: (!recorded_id.is_empty())
                .then(|| recorded_id.to_owned())
                .or_else(|| {
                    (!p.provider_request_id.is_empty()).then(|| p.provider_request_id.clone())
                }),
            input_tokens: p.input_tokens,
            cached_input_tokens: p.cached_input_tokens,
            cache_write_tokens: p.cache_write_tokens,
            output_tokens: p.output_tokens,
            cost_minor: priced.as_ref().map(|x| x.minor),
            priced_under: priced
                .as_ref()
                .map(|x| x.registry_generation.clone())
                .unwrap_or_default(),
            source: p.source.clone(),
            invoice_ref,
        },
        actor.clone(),
    );
    let lt = Lineage::task(core.tenant_id, task.session_id, task_id);
    append(
        &mut store,
        core,
        lt,
        AggregateType::Task,
        *task_id.as_bytes(),
        vec![reconciled],
    )
    .map_err(|e| ("STORE".to_owned(), e))?;
    let record = record_event(
        &store,
        core.tenant_id,
        task_id,
        "USAGE_RECONCILED",
        None,
        actor,
    );
    let mut record_ref = String::new();
    if let Some(ev) = record {
        record_ref = s(&ev.payload["record_ref"]);
        let _ = append(
            &mut store,
            core,
            lt,
            AggregateType::Task,
            *task_id.as_bytes(),
            vec![ev],
        );
    }
    Ok(wire::UsageReconciliationView {
        reconciled: true,
        duplicate: false,
        cost_minor: priced.as_ref().map_or(0, |x| x.minor),
        priced: priced.is_some(),
        priced_under: priced.map(|x| x.registry_generation).unwrap_or_default(),
        record_ref,
    })
}

#[cfg(test)]
mod tests {
    //! The parts of REQ-EPR-010 / EPR-FI-010 a Core run cannot stage: the
    //! pricing arithmetic, a replayed attempt record, another tenant's
    //! envelopes in the session, and a payload retention removed. Each runs
    //! on the real SQLite store and object directory.
    use super::*;
    use modbit_domain::routing::{
        Budget, Money, Provenance, ROUTING_SCHEMA_VERSION, Slot, Trigger,
    };
    use modbit_domain::run::{OwnerLocation, RunEvent};
    use modbit_domain::{SessionId, Timestamp};
    use modbit_event_store::AppendRequest;

    fn ev<E: Serialize>(t: &str, e: &E, actor: Actor) -> NewEvent {
        let mut n = NewEvent::new(t, serde_json::to_value(e).unwrap(), actor);
        n.occurred_at = Some(Timestamp::now());
        n
    }

    struct Fixture {
        _dir: tempfile::TempDir,
        root: std::path::PathBuf,
        store: EventStore,
        tenant: TenantId,
        session: SessionId,
        task: TaskId,
        run: RunId,
    }

    impl Fixture {
        fn append(
            &mut self,
            tenant: TenantId,
            aggregate_type: AggregateType,
            run: bool,
            events: Vec<NewEvent>,
        ) -> Result<Vec<StoredEvent>, modbit_event_store::Error> {
            let aggregate_id = match aggregate_type {
                AggregateType::Session => *self.session.as_bytes(),
                AggregateType::Task => *self.task.as_bytes(),
                _ => *self.run.as_bytes(),
            };
            self.store.append(AppendRequest {
                tenant_id: tenant,
                session_id: self.session,
                task_id: (aggregate_type != AggregateType::Session).then_some(self.task),
                run_id: run.then_some(self.run),
                turn_id: None,
                step_id: None,
                aggregate_type,
                aggregate_id,
                expected_sequence: None,
                events,
            })
        }

        fn attempt(&mut self, tenant: TenantId, n: u32, cost: u64) {
            let e = ev(
                "RoutingAttemptRecorded",
                &RunEvent::RoutingAttemptRecorded {
                    plan_id: "plan-acct".into(),
                    slot_id: "initial".into(),
                    attempt: n,
                    outcome: "SUCCEEDED".into(),
                    usage_known: true,
                    input_tokens: Some(1_000),
                    output_tokens: Some(100),
                    provider_request_id: Some(format!("req-{n}")),
                    cached_input_tokens: Some(0),
                    cache_write_tokens: Some(0),
                    latency_ms: Some(10),
                    cost_minor: Some(cost),
                    priced_under: "registry-1".into(),
                },
                Actor::Core("test".into()),
            );
            self.append(tenant, AggregateType::Run, true, vec![e])
                .unwrap();
        }

        fn derive(&self) -> OutcomeRecord {
            derive(&self.store, self.tenant, self.task, None).unwrap()
        }
    }

    fn fixture() -> Fixture {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_path_buf();
        let store = EventStore::open(&root).unwrap();
        let mut f = Fixture {
            _dir: dir,
            root,
            store,
            tenant: TenantId::from_bytes([0xA1; 16]),
            session: SessionId::new(),
            task: TaskId::new(),
            run: RunId::new(),
        };
        let (tenant, session, task, run) = (f.tenant, f.session, f.task, f.run);
        let core = Actor::Core("test".into());
        f.append(
            tenant,
            AggregateType::Session,
            false,
            vec![ev(
                "SessionCreated",
                &modbit_domain::session::SessionEvent::SessionCreated {
                    tenant_id: tenant,
                    user_id: modbit_domain::UserId::new(),
                    space_id: modbit_domain::SpaceId::new(),
                },
                core.clone(),
            )],
        )
        .unwrap();
        f.append(
            tenant,
            AggregateType::Task,
            false,
            vec![ev(
                "TaskCreated",
                &TaskEvent::TaskCreated {
                    session_id: session,
                    goal_text: "account for it".into(),
                    workspace_id: modbit_domain::WorkspaceId::new(),
                    workspace_root: None,
                    base_revision: None,
                    execution_profile: "local_trusted".into(),
                    policy_profile_id: None,
                    origin: modbit_domain::task::TaskOrigin::Desktop,
                },
                core.clone(),
            )],
        )
        .unwrap();
        let usd = |m: u64| Money {
            minor_units: m,
            currency: "USD".into(),
            scale: 2,
        };
        let plan = ConditionalExecutionPlan {
            schema_version: ROUTING_SCHEMA_VERSION,
            plan_id: "plan-acct".into(),
            tenant_id: tenant,
            session_id: session,
            task_id: task,
            run_id: run,
            routing_epoch: 0,
            lease_generation: 1,
            created_at_ms: 1_700_000_000_000,
            provenance: Provenance {
                policy_version: "policy-1".into(),
                registry_generation: "registry-1".into(),
                profiler_version: "profiler-1".into(),
                statistics_version: "stats-1".into(),
                compiler_version: "compiler-1".into(),
                gate_version: "gate-1".into(),
                risk_version: "risk-1".into(),
                legacy_decode: None,
            },
            input_digest: "b".repeat(64),
            slots: vec![Slot {
                slot_id: "initial".into(),
                predecessor: None,
                trigger: Trigger::Initial,
                max_activations: 1,
                endpoint: "openai".into(),
                model: "gpt-5-mini".into(),
                role: "solver".into(),
                budget: Budget {
                    timeout_ms: 120_000,
                    max_output_tokens: 4096,
                    max_retries: 1,
                    reserved: usd(200),
                },
            }],
            max_total_attempts: 4,
            max_revisions: 0,
            verification_reserve: usd(50),
            total_budget: usd(1_000),
            content_digest: String::new(),
        }
        .sealed();
        f.append(
            tenant,
            AggregateType::Run,
            true,
            vec![
                ev(
                    "RunCreated",
                    &RunEvent::RunCreated {
                        task_id: task,
                        attempt: 1,
                        owner_location: OwnerLocation::Local,
                        kernel_lease_generation: 1,
                    },
                    core.clone(),
                ),
                ev(
                    "RoutingPlanCompiled",
                    &RunEvent::RoutingPlanCompiled {
                        plan_ref: modbit_domain::routing::plan_digest(&plan),
                        plan: Box::new(plan),
                    },
                    core.clone(),
                ),
                ev(
                    "SlotActivated",
                    &RunEvent::SlotActivated {
                        plan_id: "plan-acct".into(),
                        slot_id: "initial".into(),
                        activation: 1,
                        reserved_minor: 200,
                    },
                    core,
                ),
            ],
        )
        .unwrap();
        f
    }

    #[test]
    fn each_subset_of_the_input_is_charged_at_its_own_price() {
        let e = modbit_providers::registry::Economics {
            input_per_mtok_minor: 1_000_000,
            output_per_mtok_minor: 2_000_000,
            currency: "USD".into(),
            scale: 2,
            cached_input_per_mtok_minor: Some(100_000),
            cache_write_per_mtok_minor: Some(250_000),
            cache_ttl_ms: None,
        };
        let usage = |input, cached, write, output| modbit_providers::Usage {
            input_tokens: input,
            output_tokens: output,
            cached_input_tokens: cached,
            cache_write_input_tokens: write,
        };
        // 50 plain + 10 written at 1.00 each, 10 written surcharge 0.25,
        // 40 cached at 0.10, 12 out at 2.00.
        assert_eq!(price_at(&e, &usage(100, 40, 10, 12)), 60 + 3 + 4 + 24);
        // A cached price above the input price is held at the input price.
        let dear = modbit_providers::registry::Economics {
            cached_input_per_mtok_minor: Some(5_000_000),
            ..e.clone()
        };
        assert_eq!(price_at(&dear, &usage(100, 100, 0, 0)), 100);
        // Subsets larger than the total are clamped, never negative.
        assert_eq!(price_at(&e, &usage(10, 40, 40, 0)), 1);
    }

    #[test]
    fn a_replayed_attempt_record_is_counted_once_and_named() {
        let mut f = fixture();
        let tenant = f.tenant;
        f.attempt(tenant, 1, 7);
        f.attempt(tenant, 1, 7);
        f.attempt(tenant, 2, 9);
        let r = f.derive();
        assert_eq!(r.cost.inference_minor, 16, "{:#?}", r.cost);
        assert_eq!(r.executed_path[0].attempts.len(), 2);
        assert_eq!(r.duplicates_ignored.len(), 1, "{:?}", r.duplicates_ignored);
        assert!(r.duplicates_ignored[0].contains("plan-acct/initial#1"));
        assert_eq!(r.reservations.activated_minor, 200);
    }

    #[test]
    fn another_tenants_envelopes_in_the_session_are_not_this_requests_accounting() {
        let mut f = fixture();
        let tenant = f.tenant;
        f.attempt(tenant, 1, 7);
        // A foreign envelope lands in the same session and run (the store
        // keeps an imported envelope's origin tenant as provenance).
        f.attempt(TenantId::from_bytes([0xB2; 16]), 2, 900);
        let r = f.derive();
        assert_eq!(r.cost.inference_minor, 7, "{:#?}", r.cost);
        assert_eq!(r.executed_path[0].attempts.len(), 1);
        assert_eq!(r.tenant_id, tenant.to_string());
        assert!(!serde_json::to_string(&r).unwrap().contains("req-2"));
    }

    #[test]
    fn a_signal_whose_payload_retention_removed_is_unavailable_not_absent() {
        let mut f = fixture();
        let tenant = f.tenant;
        f.attempt(tenant, 1, 7);
        let before = f.derive();
        // A person's steering, large enough to be kept as an object.
        let stored = f
            .append(
                tenant,
                AggregateType::Task,
                false,
                vec![ev(
                    "TaskSteered",
                    &TaskEvent::TaskSteered {
                        text: "x".repeat(70 * 1024),
                        provenance: String::new(),
                        untrusted: false,
                    },
                    Actor::User(modbit_domain::UserId::new()),
                )],
            )
            .unwrap();
        let offset = stored[0].offset;
        let modbit_domain::event::PayloadRef::Object { object_hash, .. } =
            &stored[0].envelope.payload
        else {
            panic!("a 70 KiB payload is an object");
        };
        // Retention removes the content; the envelope stays for audit.
        std::fs::remove_file(
            f.root
                .join("objects")
                .join(&object_hash[..2])
                .join(&object_hash[2..]),
        )
        .unwrap();
        let after = f.derive();
        let label = format!("TaskSteered@event:{offset}");
        assert_eq!(after.signals.unavailable, vec![label.clone()]);
        assert!(
            after
                .missing_signals
                .iter()
                .any(|m| m == &format!("unavailable: {label}")),
            "{:?}",
            after.missing_signals
        );
        // The intervention still counts: its envelope says who and when.
        assert_eq!(after.request.human_interventions, 1);
        assert_eq!(
            after.signals.interventions[0].source,
            format!("event:{offset}")
        );
        // Nothing about the money or the outcome moved (the request's
        // wall-clock span did: the steering is part of it).
        assert_eq!(
            (
                after.cost.inference_minor,
                after.cost.held_unknown_minor,
                after.cost.total_minor
            ),
            (
                before.cost.inference_minor,
                before.cost.held_unknown_minor,
                before.cost.total_minor
            )
        );
        assert_eq!(after.request.final_outcome, before.request.final_outcome);
        assert!(!after.request.verified_success);
    }
}
