//! RunStep aggregate (docs/13 "RunStep", docs/31 `run_steps`): typed atomic
//! runtime steps. `Pending → Running → Succeeded | Failed | Cancelled |
//! UnknownOutcome`; `UnknownOutcome` is reserved for effectful steps and is
//! never retried automatically (docs/13 "Tool call").

use serde::{Deserialize, Serialize};

use crate::ids::{RunStepId, TurnId};
use crate::state::StateMachine;
use crate::time::Timestamp;

/// Verification stage carried by a `Verification` step (docs/64).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum VerificationStage {
    /// Before the first write.
    Baseline,
    /// Bounded run selected for the change.
    Targeted,
    /// Final candidate revision.
    Completion,
    /// Isolated rerun of failed checks.
    Rerun,
}

/// Step type (docs/13 RunStep list including competence and routing steps).
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum StepType {
    /// Context compile.
    ContextCompile,
    /// Model invocation.
    ModelInvoke,
    /// Tool call (effectful; may end `UnknownOutcome`).
    ToolCall,
    /// Procedural runtime run.
    ProcedureRun,
    /// Waiting for approval.
    ApprovalWait,
    /// Verification with its stage and candidate revision.
    Verification {
        /// Stage.
        stage: VerificationStage,
        /// Candidate revision verified.
        candidate_revision: String,
    },
    /// Checkpoint.
    Checkpoint,
    /// Handoff.
    Handoff,
    /// User question.
    UserQuestion,
    /// Plan.
    Plan,
    /// Repair attempt.
    RepairAttempt,
    /// Self review.
    SelfReview,
    /// Request profile (EPR).
    RequestProfile,
    /// Plan compile (EPR).
    PlanCompile,
    /// Continuation activation (EPR).
    ContinuationActivate,
    /// Acceptance gate (EPR).
    AcceptanceGate,
    /// Independent review (EPR).
    Review,
    /// Realized risk derivation (EPR).
    RealizedRisk,
    /// Accounting reconcile (EPR).
    AccountingReconcile,
}

impl StepType {
    /// Whether the step can produce protected effects and therefore may end in `UnknownOutcome`.
    #[must_use]
    pub const fn is_effectful(&self) -> bool {
        matches!(
            self,
            StepType::ToolCall | StepType::ProcedureRun | StepType::Handoff
        )
    }
}

/// Step lifecycle.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum StepState {
    /// Scheduled.
    Pending,
    /// Executing.
    Running,
    /// Succeeded.
    Succeeded,
    /// Failed.
    Failed,
    /// Cancelled.
    Cancelled,
    /// Effect outcome unknown; reconcile before any retry.
    UnknownOutcome,
}

impl StateMachine for StepState {
    const AGGREGATE: &'static str = "RunStep";

    fn can_transition(self, to: Self) -> bool {
        use StepState::*;
        matches!(
            (self, to),
            (Pending, Running)
                | (Running, Succeeded | Failed | UnknownOutcome)
                | (Pending | Running, Cancelled)
        )
    }

    fn is_terminal(self) -> bool {
        !matches!(self, StepState::Pending | StepState::Running)
    }
}

/// RunStep projection state.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunStep {
    /// Identity.
    pub step_id: RunStepId,
    /// Parent turn.
    pub turn_id: TurnId,
    /// Type.
    pub step_type: StepType,
    /// Ordinal within the turn (1-based).
    pub ordinal: u32,
    /// Lifecycle state.
    pub state: StepState,
    /// Aggregate generation.
    pub generation: u64,
    /// Input object reference.
    pub input_ref: Option<String>,
    /// Output object reference.
    pub output_ref: Option<String>,
    /// Failure code.
    pub failure_code: Option<String>,
    /// Start time.
    pub started_at: Option<Timestamp>,
    /// End time.
    pub ended_at: Option<Timestamp>,
}

/// RunStep events.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "event_type")]
pub enum StepEvent {
    /// `StepScheduled`.
    StepScheduled {
        /// Parent turn.
        turn_id: TurnId,
        /// Type.
        step_type: StepType,
        /// Ordinal.
        ordinal: u32,
        /// Input ref.
        input_ref: Option<String>,
    },
    /// `StepStarted`.
    StepStarted,
    /// `StepSucceeded`.
    StepSucceeded {
        /// Output ref.
        output_ref: Option<String>,
    },
    /// `StepFailed`.
    StepFailed {
        /// Failure code.
        failure_code: String,
        /// Output ref (partial output, diagnostics).
        output_ref: Option<String>,
    },
    /// `StepCancelled`.
    StepCancelled,
    /// `StepUnknownOutcome` (effectful steps only).
    StepUnknownOutcome {
        /// Why the outcome is unknown.
        reason: String,
    },
}

impl StepEvent {
    /// Canonical event type name.
    #[must_use]
    pub const fn event_type(&self) -> &'static str {
        match self {
            Self::StepScheduled { .. } => "StepScheduled",
            Self::StepStarted => "StepStarted",
            Self::StepSucceeded { .. } => "StepSucceeded",
            Self::StepFailed { .. } => "StepFailed",
            Self::StepCancelled => "StepCancelled",
            Self::StepUnknownOutcome { .. } => "StepUnknownOutcome",
        }
    }
}

impl RunStep {
    /// Build from `StepScheduled`.
    pub fn create(
        step_id: RunStepId,
        event: &StepEvent,
        _at: Timestamp,
    ) -> Result<Self, crate::InvalidTransition> {
        match event {
            StepEvent::StepScheduled {
                turn_id,
                step_type,
                ordinal,
                input_ref,
            } => Ok(Self {
                step_id,
                turn_id: *turn_id,
                step_type: step_type.clone(),
                ordinal: *ordinal,
                state: StepState::Pending,
                generation: 1,
                input_ref: input_ref.clone(),
                output_ref: None,
                failure_code: None,
                started_at: None,
                ended_at: None,
            }),
            other => Err(crate::InvalidTransition {
                aggregate: "RunStep",
                from: "<none>".into(),
                to: other.event_type().into(),
            }),
        }
    }

    /// Apply a subsequent event.
    pub fn apply(
        &mut self,
        event: &StepEvent,
        at: Timestamp,
    ) -> Result<(), crate::InvalidTransition> {
        use StepState::*;
        let to = match event {
            StepEvent::StepScheduled { .. } => {
                return Err(crate::InvalidTransition {
                    aggregate: "RunStep",
                    from: format!("{:?}", self.state),
                    to: "StepScheduled".into(),
                });
            }
            StepEvent::StepStarted => Running,
            StepEvent::StepSucceeded { output_ref } => {
                self.state.transition(Succeeded)?;
                self.output_ref = output_ref.clone();
                Succeeded
            }
            StepEvent::StepFailed {
                failure_code,
                output_ref,
            } => {
                self.state.transition(Failed)?;
                self.failure_code = Some(failure_code.clone());
                self.output_ref = output_ref.clone();
                Failed
            }
            StepEvent::StepCancelled => Cancelled,
            StepEvent::StepUnknownOutcome { reason } => {
                if !self.step_type.is_effectful() {
                    return Err(crate::InvalidTransition {
                        aggregate: "RunStep",
                        from: format!("{:?} ({:?})", self.state, self.step_type),
                        to: "UnknownOutcome (only effectful steps)".into(),
                    });
                }
                self.state.transition(UnknownOutcome)?;
                self.failure_code = Some(format!("UNKNOWN_OUTCOME:{reason}"));
                UnknownOutcome
            }
        };
        self.state = self.state.transition(to)?;
        if to == Running {
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

    fn step(kind: StepType) -> RunStep {
        RunStep::create(
            RunStepId::new(),
            &StepEvent::StepScheduled {
                turn_id: TurnId::new(),
                step_type: kind,
                ordinal: 1,
                input_ref: None,
            },
            Timestamp(1),
        )
        .unwrap()
    }

    #[test]
    fn unknown_outcome_is_only_for_effectful_steps_and_is_terminal() {
        let mut s = step(StepType::ToolCall);
        s.apply(&StepEvent::StepStarted, Timestamp(2)).unwrap();
        s.apply(
            &StepEvent::StepUnknownOutcome {
                reason: "transport lost".into(),
            },
            Timestamp(3),
        )
        .unwrap();
        assert_eq!(s.state, StepState::UnknownOutcome);
        assert!(
            s.apply(&StepEvent::StepStarted, Timestamp(4)).is_err(),
            "no automatic retry from UnknownOutcome"
        );

        let mut c = step(StepType::ContextCompile);
        c.apply(&StepEvent::StepStarted, Timestamp(2)).unwrap();
        let err = c
            .apply(
                &StepEvent::StepUnknownOutcome { reason: "x".into() },
                Timestamp(3),
            )
            .unwrap_err();
        assert!(err.to_string().contains("only effectful steps"));
        assert_eq!(c.state, StepState::Running);
    }

    #[test]
    fn verification_step_carries_stage_and_revision() {
        let s = step(StepType::Verification {
            stage: VerificationStage::Baseline,
            candidate_revision: "abc".into(),
        });
        let json = serde_json::to_value(&s.step_type).unwrap();
        assert_eq!(json["kind"], "VERIFICATION");
        assert_eq!(json["stage"], "BASELINE");
    }
}
