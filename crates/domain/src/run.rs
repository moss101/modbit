//! Run aggregate (docs/13 "Run", docs/31 `runs`): one concrete execution
//! attempt or continuation of a task, fenced by the session kernel lease
//! generation.
//!
//! `Pending → Running ↔ Suspended → Completed | Failed | Cancelled`.

use serde::{Deserialize, Serialize};

use crate::ids::{RunId, TaskId};
use crate::state::StateMachine;
use crate::time::Timestamp;

/// Where the run executes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OwnerLocation {
    /// Local Core.
    Local,
    /// Cloud worker.
    Cloud,
}

/// Run lifecycle.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RunState {
    /// Created, not executing.
    Pending,
    /// Executing under a kernel lease.
    Running,
    /// Detached (handoff, restart) and resumable.
    Suspended,
    /// Finished successfully.
    Completed,
    /// Finished with failure.
    Failed,
    /// Cancelled.
    Cancelled,
}

impl StateMachine for RunState {
    const AGGREGATE: &'static str = "Run";

    fn can_transition(self, to: Self) -> bool {
        use RunState::*;
        matches!(
            (self, to),
            (Pending, Running)
                | (Running, Suspended)
                | (Suspended, Running)
                | (Running, Completed)
                | (Pending | Running | Suspended, Failed | Cancelled)
        )
    }

    fn is_terminal(self) -> bool {
        matches!(
            self,
            RunState::Completed | RunState::Failed | RunState::Cancelled
        )
    }
}

/// Run projection state.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Run {
    /// Identity.
    pub run_id: RunId,
    /// Parent task.
    pub task_id: TaskId,
    /// Attempt ordinal within the task (1-based).
    pub attempt: u32,
    /// Where it executes.
    pub owner_location: OwnerLocation,
    /// Kernel lease generation that owns the run; stale generations are rejected.
    pub kernel_lease_generation: u64,
    /// Lifecycle state.
    pub state: RunState,
    /// Aggregate generation.
    pub generation: u64,
    /// Start time.
    pub started_at: Option<Timestamp>,
    /// End time.
    pub ended_at: Option<Timestamp>,
}

/// Run events.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "event_type")]
pub enum RunEvent {
    /// `RunCreated`.
    RunCreated {
        /// Parent task.
        task_id: TaskId,
        /// Attempt ordinal.
        attempt: u32,
        /// Location.
        owner_location: OwnerLocation,
        /// Owning lease generation.
        kernel_lease_generation: u64,
    },
    /// `RunStarted`.
    RunStarted,
    /// `RunSuspended`.
    RunSuspended,
    /// `RunResumed` with the lease generation that now owns it; must not go backwards.
    RunResumed {
        /// New owning lease generation.
        kernel_lease_generation: u64,
    },
    /// `RunCompleted`.
    RunCompleted,
    /// `RunFailed`.
    RunFailed {
        /// Failure code.
        failure_code: String,
    },
    /// `RunCancelled`.
    RunCancelled,
    /// `VerificationBaselineRecorded` (docs/64 §1); no state change.
    VerificationBaselineRecorded {
        /// Verification run.
        verification_run_id: String,
        /// Derived plan object.
        plan_ref: String,
        /// Revision.
        candidate_revision: String,
        /// Environment digest.
        environment_digest: String,
        /// Overall status.
        status: String,
        /// Report objects.
        report_refs: Vec<String>,
        /// Check summaries: (check_id, kind, status, error_class, fingerprint).
        checks: Vec<CheckSummary>,
    },
    /// `VerificationRunRecorded` (TARGETED / COMPLETION / RERUN); no state change.
    VerificationRunRecorded {
        /// Verification run.
        verification_run_id: String,
        /// Stage.
        stage: String,
        /// Plan object.
        plan_ref: String,
        /// Revision.
        candidate_revision: String,
        /// Environment digest.
        environment_digest: String,
        /// Overall status.
        status: String,
        /// Report objects.
        report_refs: Vec<String>,
        /// Check summaries.
        checks: Vec<CheckSummary>,
    },
    /// `RoutingPlanCompiled` (REQ-EPR-001, docs/38): a conditional execution
    /// plan was compiled for this run and stored. The event carries what
    /// identifies and pins it; the plan itself is the object. No state change.
    RoutingPlanCompiled {
        /// The plan itself: a routing decision is a decision, so it is on the
        /// log rather than only in a side store (boxed: it is the largest run
        /// payload).
        plan: Box<crate::routing::ConditionalExecutionPlan>,
        /// Object hash of the same plan, for clients that read it by ref.
        plan_ref: String,
    },
    /// `RunFenced` (docs/13 "Fencing and epochs", docs/33 "Session kernel
    /// lease", M4.4): the execution owner found the session lease it ran under
    /// superseded and stopped advancing state; `RunSuspended` follows. No
    /// state change.
    RunFenced {
        /// The generation the run held.
        kernel_lease_generation: u64,
        /// The generation the session is at now.
        current_generation: u64,
        /// Who holds it.
        owner: String,
    },
    /// `RequestProfiled` (REQ-EPR-003, docs/38 §6): what the Request Profiler
    /// said about this run's request, recorded in shadow — nothing routes on
    /// it. A conservative fallback is recorded as one. No state change.
    RequestProfiled {
        /// Extractor version.
        profiler_version: String,
        /// Calibration cohort the answer came from.
        cohort_version: String,
        /// The slice the request fell in.
        slice: String,
        /// Digest of the intrinsic features.
        features_digest: String,
        /// Probability the floor is met, shrunk by support, in basis points
        /// of `[0, 1]`: an integer on the log never drifts the way a
        /// formatted float can.
        p_floor_success_bp: u32,
        /// How much the cohort supports that number, in basis points.
        confidence_bp: u32,
        /// The cohort has too little of this slice to say anything.
        ood: bool,
        /// Whether this is the conservative fallback rather than a measurement.
        fallback: bool,
        /// Why, when it is.
        fallback_reason: String,
    },
    /// `RoutingPlanAdmitted` (REQ-EPR-014, docs/38 §5.2): a plan passed
    /// admission — validated whole, with the digest of what was validated and
    /// the money it reserves. Nothing dispatches from a plan with no
    /// admission. No state change.
    RoutingPlanAdmitted {
        /// Plan.
        plan_id: String,
        /// Digest of exactly what admission validated.
        validation_digest: String,
        /// Money reserved across every slot plus the verification reserve.
        reserved_minor: u64,
        /// Currency of that reservation.
        currency: String,
        /// Scale of that reservation.
        scale: u8,
        /// The lease generation that admitted it.
        lease_generation: u64,
        /// What confidence-adjusted feasibility said (REQ-EPR-016):
        /// `FEASIBLE` | `QUALITY_FLOOR_INFEASIBLE` | `QUALITY_FLOOR_UNKNOWN`.
        /// Admission does not refuse on it; it records it so nothing can
        /// later claim a target the evidence did not support.
        #[serde(default)]
        feasibility: String,
        /// The plan's quality lower bound, in basis points.
        #[serde(default)]
        quality_lcb_bp: u32,
        /// The statistics snapshot the bound came from, or `none`.
        #[serde(default)]
        stats_version: String,
        /// The threshold version the bound was measured against, or `none`.
        #[serde(default)]
        thresholds_version: String,
        /// Whether the plan may be described as meeting the mode's target.
        #[serde(default)]
        target_met: bool,
    },
    /// One activation of one slot (REQ-EPR-014). A slot is bounded by its
    /// activations, not by the attempts inside them, and a restart recovers
    /// the activation it already had rather than opening a second one.
    SlotActivated {
        /// Plan.
        plan_id: String,
        /// Slot.
        slot_id: String,
        /// Ordinal within the slot, 1-based.
        activation: u32,
        /// Money this activation reserves.
        reserved_minor: u64,
    },
    /// `RoutingAttemptRecorded` (REQ-EPR-001, docs/38
    /// "CompleteAccountingAndAttribution"): one attempt against one slot,
    /// including the ones that failed, retried or were cancelled. Usage that
    /// the provider never reported stays unknown. No state change.
    RoutingAttemptRecorded {
        /// Plan the slot belongs to.
        plan_id: String,
        /// Slot.
        slot_id: String,
        /// Attempt ordinal within the slot, starting at 1.
        attempt: u32,
        /// `SUCCEEDED` | `FAILED` | `CANCELLED` | `TIMED_OUT`.
        outcome: String,
        /// Whether the provider reported usage for this attempt.
        usage_known: bool,
        /// Input tokens, when reported.
        input_tokens: Option<u64>,
        /// Output tokens, when reported.
        output_tokens: Option<u64>,
        /// The provider's own request id, when it gave one.
        provider_request_id: Option<String>,
    },
    /// `FlakyCheckQuarantined` (docs/64 §3); no state change.
    FlakyCheckQuarantined {
        /// Check.
        check_id: String,
        /// Run where it failed.
        first_run_id: String,
        /// Isolated rerun where it passed.
        rerun_id: String,
        /// Revision scope.
        candidate_revision: String,
    },
    /// `RegressionAttributed` (docs/64 §1); no state change.
    RegressionAttributed {
        /// Completion run.
        verification_run_id: String,
        /// Check.
        check_id: String,
        /// REGRESSION | KNOWN_FAILING | COLLATERAL_FIX | DECLARED_CHANGE | FLAKY | NEW_FAILING | PASS.
        attribution: String,
    },
    /// `DiffInvariantViolated` (docs/64 §4); no state change.
    DiffInvariantViolated {
        /// `DI-n`.
        invariant: String,
        /// DENY | FLAG.
        class: String,
        /// Paths.
        paths: Vec<String>,
        /// Evidence.
        evidence: String,
        /// Where it was evaluated: TRANSACTION | COMPLETION.
        stage: String,
    },
    /// `RealizedRiskDerived` (REQ-EPR-008, docs/27 §9.3): the factual,
    /// policy-owned risk of the candidate at the COMPLETION run, persisted
    /// separately from test correctness and from any gate verdict. No
    /// state change.
    RealizedRiskDerived {
        /// The completion run it was derived beside.
        verification_run_id: String,
        /// Object hash of the `RealizedRisk` record.
        realized_risk_ref: String,
        /// Candidate revision.
        candidate_revision: u64,
        /// LOW | MEDIUM | HIGH | CRITICAL.
        level: String,
        /// FAST | STANDARD | GOVERNED | HIGH_ASSURANCE.
        minimum_assurance: String,
        /// Independent review required.
        independent_review_required: bool,
        /// Human decision required.
        human_required: bool,
        /// Reason codes with their surface, `CODE[:SURFACE]`.
        reasons: Vec<String>,
        /// Policy version.
        policy_version: String,
        /// Rules version.
        rules_version: String,
        /// Forbidden effects the candidate requested.
        forbidden_effects_requested: Vec<String>,
    },
    /// `RouteReevaluated` (REQ-EPR-009, docs/27 §7.6 and §16): at a
    /// boundary — a new task, a compaction epoch — the route in force was
    /// compared with its alternatives on cache economics and confidence
    /// feasibility. `SWITCH` means a new transaction was compiled and
    /// admitted under a new routing epoch on the same run; `STAY` and
    /// `INITIAL` change nothing. No state change.
    RouteReevaluated {
        /// `TASK` | `COMPACTION` | `PROVIDER` | `QUALITY` | `MODE`.
        boundary: String,
        /// The routing epoch in force after the decision.
        route_epoch: u64,
        /// The binding in force before, `endpoint/model`; empty at a fresh start.
        current: String,
        /// The binding in force after.
        chosen: String,
        /// `INITIAL` | `STAY` | `SWITCH`.
        decision: String,
        /// Why (the economics or the feasibility reason).
        reason: String,
        /// Staying, remaining demand at the current prices, minor units.
        stay_minor: u64,
        /// Switching to the chosen alternative, switch cost included.
        switch_minor: u64,
        /// The itemized switch cost the thresholds carried (JSON).
        switch_cost: serde_json::Value,
        /// The cache state consulted (JSON), when any.
        cache_state: Option<serde_json::Value>,
        /// The plan in force after the decision.
        plan_id: String,
        /// Economics rules version.
        economics_version: String,
    },
    /// `AcceptanceGateEvaluated` (REQ-EPR-017, docs/27 §9.4): whether the
    /// evidence at the candidate revision satisfies the required assurance,
    /// decided independently of the risk classification. No state change.
    AcceptanceGateEvaluated {
        /// The completion run the evidence came from, when any.
        verification_run_id: String,
        /// Object hash of the `AcceptanceGateResult`.
        gate_ref: String,
        /// Gate rules version.
        gate_version: String,
        /// Candidate revision.
        candidate_revision: u64,
        /// ACCEPT | REJECT | INCONCLUSIVE.
        verdict: String,
        /// Required assurance level label.
        required_assurance: String,
        /// Independent review required.
        independent_review_required: bool,
        /// Human required.
        human_required: bool,
        /// Missing evidence kinds.
        missing_evidence: Vec<String>,
        /// Reject reasons.
        reject_reasons: Vec<String>,
        /// The risk record consumed.
        realized_risk_ref: String,
        /// What triggered the evaluation: `COMPLETION_RUN` | `REVIEW_DECISION`.
        trigger: String,
    },
}

/// One check as recorded on the log (the full CheckResult lives in the report object).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CheckSummary {
    /// Stable id.
    pub check_id: String,
    /// Kind.
    pub kind: String,
    /// Status.
    pub status: String,
    /// Duration.
    pub duration_ms: u64,
    /// Error class.
    pub error_class: Option<String>,
    /// Fingerprint.
    pub message_fingerprint: Option<String>,
    /// Location path.
    pub path: Option<String>,
}

impl RunEvent {
    /// Canonical event type name.
    #[must_use]
    pub const fn event_type(&self) -> &'static str {
        match self {
            Self::RunCreated { .. } => "RunCreated",
            Self::RunStarted => "RunStarted",
            Self::RunSuspended => "RunSuspended",
            Self::RunResumed { .. } => "RunResumed",
            Self::RunCompleted => "RunCompleted",
            Self::RunFailed { .. } => "RunFailed",
            Self::RunCancelled => "RunCancelled",
            Self::VerificationBaselineRecorded { .. } => "VerificationBaselineRecorded",
            Self::VerificationRunRecorded { .. } => "VerificationRunRecorded",
            Self::RoutingPlanCompiled { .. } => "RoutingPlanCompiled",
            Self::RequestProfiled { .. } => "RequestProfiled",
            Self::RoutingPlanAdmitted { .. } => "RoutingPlanAdmitted",
            Self::SlotActivated { .. } => "SlotActivated",
            Self::RoutingAttemptRecorded { .. } => "RoutingAttemptRecorded",
            Self::RunFenced { .. } => "RunFenced",
            Self::FlakyCheckQuarantined { .. } => "FlakyCheckQuarantined",
            Self::RegressionAttributed { .. } => "RegressionAttributed",
            Self::DiffInvariantViolated { .. } => "DiffInvariantViolated",
            Self::RealizedRiskDerived { .. } => "RealizedRiskDerived",
            Self::AcceptanceGateEvaluated { .. } => "AcceptanceGateEvaluated",
            Self::RouteReevaluated { .. } => "RouteReevaluated",
        }
    }
}

impl Run {
    /// Build from `RunCreated`.
    pub fn create(
        run_id: RunId,
        event: &RunEvent,
        _at: Timestamp,
    ) -> Result<Self, crate::InvalidTransition> {
        match event {
            RunEvent::RunCreated {
                task_id,
                attempt,
                owner_location,
                kernel_lease_generation,
            } => Ok(Self {
                run_id,
                task_id: *task_id,
                attempt: *attempt,
                owner_location: *owner_location,
                kernel_lease_generation: *kernel_lease_generation,
                state: RunState::Pending,
                generation: 1,
                started_at: None,
                ended_at: None,
            }),
            other => Err(crate::InvalidTransition {
                aggregate: "Run",
                from: "<none>".into(),
                to: other.event_type().into(),
            }),
        }
    }

    /// Apply a subsequent event.
    pub fn apply(
        &mut self,
        event: &RunEvent,
        at: Timestamp,
    ) -> Result<(), crate::InvalidTransition> {
        use RunState::*;
        let to = match event {
            RunEvent::RunCreated { .. } => {
                return Err(crate::InvalidTransition {
                    aggregate: "Run",
                    from: format!("{:?}", self.state),
                    to: "RunCreated".into(),
                });
            }
            RunEvent::RunStarted => Running,
            RunEvent::RunSuspended => Suspended,
            RunEvent::RunResumed {
                kernel_lease_generation,
            } => {
                // Fencing (docs/13): a resume under an older lease generation is stale.
                if *kernel_lease_generation < self.kernel_lease_generation {
                    return Err(crate::InvalidTransition {
                        aggregate: "Run",
                        from: format!("lease generation {}", self.kernel_lease_generation),
                        to: format!("stale lease generation {kernel_lease_generation}"),
                    });
                }
                self.state.transition(Running)?;
                self.kernel_lease_generation = *kernel_lease_generation;
                Running
            }
            RunEvent::RunCompleted => Completed,
            RunEvent::RunFailed { .. } => Failed,
            RunEvent::RunCancelled => Cancelled,
            RunEvent::VerificationBaselineRecorded { .. }
            | RunEvent::VerificationRunRecorded { .. }
            | RunEvent::RoutingPlanCompiled { .. }
            | RunEvent::RequestProfiled { .. }
            | RunEvent::RoutingPlanAdmitted { .. }
            | RunEvent::SlotActivated { .. }
            | RunEvent::RoutingAttemptRecorded { .. }
            | RunEvent::RunFenced { .. }
            | RunEvent::FlakyCheckQuarantined { .. }
            | RunEvent::RegressionAttributed { .. }
            | RunEvent::DiffInvariantViolated { .. }
            | RunEvent::RealizedRiskDerived { .. }
            | RunEvent::RouteReevaluated { .. } => {
                if self.state.is_terminal() {
                    return Err(crate::InvalidTransition {
                        aggregate: "Run",
                        from: format!("{:?}", self.state),
                        to: event.event_type().into(),
                    });
                }
                self.generation += 1;
                return Ok(());
            }
            // The gate is re-evaluated at the review decision, after the
            // run that produced the candidate has completed: an audit record
            // about the run's candidate, valid in every state.
            RunEvent::AcceptanceGateEvaluated { .. } => {
                self.generation += 1;
                return Ok(());
            }
        };
        self.state = self.state.transition(to)?;
        if to == Running && self.started_at.is_none() {
            self.started_at = Some(at);
        }
        if to.is_terminal() {
            self.ended_at = Some(at);
        }
        self.generation += 1;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stale_lease_generation_cannot_resume_a_run() {
        let mut r = Run::create(
            RunId::new(),
            &RunEvent::RunCreated {
                task_id: TaskId::new(),
                attempt: 1,
                owner_location: OwnerLocation::Local,
                kernel_lease_generation: 5,
            },
            Timestamp(1),
        )
        .unwrap();
        r.apply(&RunEvent::RunStarted, Timestamp(2)).unwrap();
        r.apply(&RunEvent::RunSuspended, Timestamp(3)).unwrap();
        let err = r
            .apply(
                &RunEvent::RunResumed {
                    kernel_lease_generation: 4,
                },
                Timestamp(4),
            )
            .unwrap_err();
        assert!(err.to_string().contains("stale lease generation 4"));
        assert_eq!(r.state, RunState::Suspended);
        r.apply(
            &RunEvent::RunResumed {
                kernel_lease_generation: 6,
            },
            Timestamp(5),
        )
        .unwrap();
        assert_eq!(r.kernel_lease_generation, 6);
        r.apply(&RunEvent::RunCompleted, Timestamp(6)).unwrap();
        assert!(r.apply(&RunEvent::RunStarted, Timestamp(7)).is_err());
    }
}
