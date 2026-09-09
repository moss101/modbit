//! ToolCall aggregate (docs/13 "Tool call", docs/31 `tool_calls`, docs/30
//! tool wire schema): `Proposed → Validated → PolicyChecked → ApprovalPending?
//! → Dispatched → Streaming → Succeeded | Failed | Cancelled | UnknownOutcome`.
//! `UnknownOutcome` is never automatically retried for effectful tools.

use serde::{Deserialize, Serialize};

use crate::ids::{ApprovalId, CapabilityLeaseId, EffectId, RunStepId, TaskId, ToolCallId};
use crate::state::StateMachine;
use crate::time::Timestamp;

/// Effect class of a tool (docs/16 "Effect classes").
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum EffectClass {
    /// Reads only.
    ReadOnly,
    /// Reversible write inside the workspace/worktree.
    ReversibleWrite,
    /// Write to a protected surface.
    ProtectedWrite,
    /// External side effect.
    ExternalSideEffect,
    /// Uses a secret.
    SecretAccess,
    /// Destructive.
    Destructive,
}

/// Tool call lifecycle.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ToolCallState {
    /// Proposed by a model or client.
    Proposed,
    /// Arguments validated against the schema.
    Validated,
    /// Policy decided (allow).
    PolicyChecked,
    /// Waiting for an approval.
    ApprovalPending,
    /// Sent to the effector.
    Dispatched,
    /// Streaming output.
    Streaming,
    /// Succeeded.
    Succeeded,
    /// Failed (application or infrastructure).
    Failed,
    /// Cancelled.
    Cancelled,
    /// Effect outcome unknown; reconcile before any retry.
    UnknownOutcome,
}

impl StateMachine for ToolCallState {
    const AGGREGATE: &'static str = "ToolCall";

    fn can_transition(self, to: Self) -> bool {
        use ToolCallState::*;
        matches!(
            (self, to),
            (Proposed, Validated)
                | (Validated, PolicyChecked)
                | (Validated, ApprovalPending)
                | (ApprovalPending, PolicyChecked)
                | (PolicyChecked, Dispatched)
                | (Dispatched, Streaming)
                | (Dispatched | Streaming, Succeeded | Failed | UnknownOutcome)
                | (
                    Proposed | Validated | PolicyChecked | ApprovalPending | Dispatched | Streaming,
                    Cancelled
                )
                | (Proposed | Validated | ApprovalPending, Failed)
        )
    }

    fn is_terminal(self) -> bool {
        matches!(
            self,
            ToolCallState::Succeeded
                | ToolCallState::Failed
                | ToolCallState::Cancelled
                | ToolCallState::UnknownOutcome
        )
    }
}

/// Tool call projection state.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolCall {
    /// Identity (stable across retry/reconnect).
    pub tool_call_id: ToolCallId,
    /// Task the call belongs to.
    pub task_id: TaskId,
    /// Owning step when part of a turn.
    pub step_id: Option<RunStepId>,
    /// Tool name (`namespace.name`).
    pub tool_name: String,
    /// Tool version.
    pub tool_version: String,
    /// Effect class as registered.
    pub effect_class: EffectClass,
    /// Lease presented.
    pub capability_lease_id: Option<CapabilityLeaseId>,
    /// sha256 of the normalized arguments.
    pub arguments_hash: String,
    /// State.
    pub state: ToolCallState,
    /// Aggregate generation.
    pub generation: u64,
    /// Dispatch time.
    pub dispatched_at: Option<Timestamp>,
    /// Completion time.
    pub completed_at: Option<Timestamp>,
    /// Result reference (object hash of the ToolCallResult JSON).
    pub result_ref: Option<String>,
    /// Why the outcome is unknown.
    pub unknown_outcome_reason: Option<String>,
    /// Policy decision text.
    pub policy_decision: Option<String>,
    /// Approval the call waits on / executed under.
    pub approval_id: Option<ApprovalId>,
}

/// A protected-effect receipt (docs/23 "Protected-effect receipt chain").
/// `receipt_hash` = sha256 over the canonical JSON of every other field.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EffectReceipt {
    /// Identity.
    pub effect_id: EffectId,
    /// Previous receipt hash in the tenant chain (`None` for the first).
    pub previous_receipt_hash: Option<String>,
    /// Task.
    pub task_id: TaskId,
    /// Tool call.
    pub tool_call_id: ToolCallId,
    /// Lease presented.
    pub capability_lease_id: Option<CapabilityLeaseId>,
    /// Intent hash.
    pub intent_hash: String,
    /// Policy decision text.
    pub policy_decision: String,
    /// Approval used.
    pub approval_id: Option<ApprovalId>,
    /// Where it ran.
    pub execution_target: String,
    /// Result object hash.
    pub evidence_ref: Option<String>,
    /// Outcome status.
    pub status: String,
    /// When.
    pub occurred_at: Timestamp,
    /// Hash of the receipt.
    pub receipt_hash: String,
}

/// Tool call events (docs/30 "Tool/procedure").
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "event_type")]
pub enum ToolCallEvent {
    /// `ToolCallProposed`.
    ToolCallProposed {
        /// Task.
        task_id: TaskId,
        /// Step.
        step_id: Option<RunStepId>,
        /// Tool.
        tool_name: String,
        /// Version.
        tool_version: String,
        /// Effect class.
        effect_class: EffectClass,
        /// Lease.
        capability_lease_id: Option<CapabilityLeaseId>,
        /// Arguments hash.
        arguments_hash: String,
    },
    /// `ToolCallValidated`.
    ToolCallValidated,
    /// `ToolCallPolicyDecision`.
    ToolCallPolicyDecision {
        /// Allowed?
        allowed: bool,
        /// Decision text (rule id / reason).
        decision: String,
        /// Approval needed (moves to ApprovalPending when not allowed but approvable).
        approval_required: bool,
    },
    /// `ToolCallApprovalRequested`: validated, policy needs an approval.
    ToolCallApprovalRequested {
        /// Approval opened.
        approval_id: ApprovalId,
        /// Decision text.
        decision: String,
    },
    /// `ToolCallDispatched`.
    ToolCallDispatched,
    /// `EffectReceiptAppended`: a protected/external effect got a receipt.
    EffectReceiptAppended {
        /// Receipt.
        receipt: EffectReceipt,
    },
    /// `ToolCallSucceeded`.
    ToolCallSucceeded {
        /// Result object hash.
        result_ref: String,
    },
    /// `ToolCallFailed`.
    ToolCallFailed {
        /// Failure code.
        failure_code: String,
        /// Result object hash (may carry diagnostics).
        result_ref: Option<String>,
    },
    /// `ToolCallCancelled`.
    ToolCallCancelled,
    /// `ToolCallUnknownOutcome`.
    ToolCallUnknownOutcome {
        /// Reason.
        reason: String,
    },
}

impl ToolCallEvent {
    /// Canonical event type name.
    #[must_use]
    pub const fn event_type(&self) -> &'static str {
        match self {
            Self::ToolCallProposed { .. } => "ToolCallProposed",
            Self::ToolCallValidated => "ToolCallValidated",
            Self::ToolCallPolicyDecision { .. } => "ToolCallPolicyDecision",
            Self::ToolCallApprovalRequested { .. } => "ToolCallApprovalRequested",
            Self::ToolCallDispatched => "ToolCallDispatched",
            Self::EffectReceiptAppended { .. } => "EffectReceiptAppended",
            Self::ToolCallSucceeded { .. } => "ToolCallSucceeded",
            Self::ToolCallFailed { .. } => "ToolCallFailed",
            Self::ToolCallCancelled => "ToolCallCancelled",
            Self::ToolCallUnknownOutcome { .. } => "ToolCallUnknownOutcome",
        }
    }
}

fn invalid(from: ToolCallState, to: &str) -> crate::InvalidTransition {
    crate::InvalidTransition {
        aggregate: "ToolCall",
        from: format!("{from:?}"),
        to: to.into(),
    }
}

impl ToolCall {
    /// Build from `ToolCallProposed`.
    pub fn create(
        id: ToolCallId,
        event: &ToolCallEvent,
        _at: Timestamp,
    ) -> Result<Self, crate::InvalidTransition> {
        match event {
            ToolCallEvent::ToolCallProposed {
                task_id,
                step_id,
                tool_name,
                tool_version,
                effect_class,
                capability_lease_id,
                arguments_hash,
            } => Ok(Self {
                tool_call_id: id,
                task_id: *task_id,
                step_id: *step_id,
                tool_name: tool_name.clone(),
                tool_version: tool_version.clone(),
                effect_class: *effect_class,
                capability_lease_id: *capability_lease_id,
                arguments_hash: arguments_hash.clone(),
                state: ToolCallState::Proposed,
                generation: 1,
                dispatched_at: None,
                completed_at: None,
                result_ref: None,
                unknown_outcome_reason: None,
                policy_decision: None,
                approval_id: None,
            }),
            other => Err(crate::InvalidTransition {
                aggregate: "ToolCall",
                from: "<none>".into(),
                to: other.event_type().into(),
            }),
        }
    }

    /// Apply a subsequent event.
    pub fn apply(
        &mut self,
        event: &ToolCallEvent,
        at: Timestamp,
    ) -> Result<(), crate::InvalidTransition> {
        use ToolCallState::*;
        let to = match event {
            ToolCallEvent::ToolCallProposed { .. } => {
                return Err(invalid(self.state, "ToolCallProposed"));
            }
            ToolCallEvent::ToolCallValidated => Validated,
            ToolCallEvent::ToolCallPolicyDecision {
                allowed,
                decision,
                approval_required,
            } => {
                self.policy_decision = Some(decision.clone());
                if *allowed {
                    PolicyChecked
                } else if *approval_required {
                    ApprovalPending
                } else {
                    Failed
                }
            }
            ToolCallEvent::ToolCallApprovalRequested {
                approval_id,
                decision,
            } => {
                self.policy_decision = Some(decision.clone());
                self.approval_id = Some(*approval_id);
                ApprovalPending
            }
            ToolCallEvent::ToolCallDispatched => Dispatched,
            ToolCallEvent::EffectReceiptAppended { receipt } => {
                // Receipts attach to a dispatched call; they do not move the state.
                if !matches!(self.state, Dispatched | Streaming) {
                    return Err(invalid(self.state, "EffectReceiptAppended"));
                }
                self.generation += 1;
                let _ = receipt;
                return Ok(());
            }
            ToolCallEvent::ToolCallSucceeded { result_ref } => {
                self.state.transition(Succeeded)?;
                self.result_ref = Some(result_ref.clone());
                Succeeded
            }
            ToolCallEvent::ToolCallFailed { result_ref, .. } => {
                self.state.transition(Failed)?;
                self.result_ref = result_ref.clone();
                Failed
            }
            ToolCallEvent::ToolCallCancelled => Cancelled,
            ToolCallEvent::ToolCallUnknownOutcome { reason } => {
                if self.effect_class == EffectClass::ReadOnly {
                    return Err(crate::InvalidTransition {
                        aggregate: "ToolCall",
                        from: format!("{:?} (READ_ONLY)", self.state),
                        to: "UnknownOutcome (only effectful tools)".into(),
                    });
                }
                self.state.transition(UnknownOutcome)?;
                self.unknown_outcome_reason = Some(reason.clone());
                UnknownOutcome
            }
        };
        self.state = self.state.transition(to)?;
        if to == Dispatched {
            self.dispatched_at = Some(at);
        }
        if to.is_terminal() {
            self.completed_at = Some(at);
        }
        self.generation += 1;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn denied_policy_fails_the_call_and_read_only_calls_cannot_be_unknown() {
        let mut c = ToolCall::create(
            ToolCallId::new(),
            &ToolCallEvent::ToolCallProposed {
                task_id: TaskId::new(),
                step_id: None,
                tool_name: "fs.read".into(),
                tool_version: "1".into(),
                effect_class: EffectClass::ReadOnly,
                capability_lease_id: None,
                arguments_hash: "h".into(),
            },
            Timestamp(1),
        )
        .unwrap();
        c.apply(&ToolCallEvent::ToolCallValidated, Timestamp(2))
            .unwrap();
        c.apply(
            &ToolCallEvent::ToolCallPolicyDecision {
                allowed: false,
                decision: "DENY: protected".into(),
                approval_required: false,
            },
            Timestamp(3),
        )
        .unwrap();
        assert_eq!(c.state, ToolCallState::Failed);
        assert!(
            c.apply(&ToolCallEvent::ToolCallDispatched, Timestamp(4))
                .is_err(),
            "a denied call cannot be dispatched"
        );
        let mut r = ToolCall::create(
            ToolCallId::new(),
            &ToolCallEvent::ToolCallProposed {
                task_id: TaskId::new(),
                step_id: None,
                tool_name: "fs.read".into(),
                tool_version: "1".into(),
                effect_class: EffectClass::ReadOnly,
                capability_lease_id: None,
                arguments_hash: "h".into(),
            },
            Timestamp(1),
        )
        .unwrap();
        r.apply(&ToolCallEvent::ToolCallValidated, Timestamp(2))
            .unwrap();
        r.apply(
            &ToolCallEvent::ToolCallPolicyDecision {
                allowed: true,
                decision: "ALLOW".into(),
                approval_required: false,
            },
            Timestamp(3),
        )
        .unwrap();
        r.apply(&ToolCallEvent::ToolCallDispatched, Timestamp(4))
            .unwrap();
        assert!(
            r.apply(
                &ToolCallEvent::ToolCallUnknownOutcome { reason: "x".into() },
                Timestamp(5)
            )
            .is_err()
        );
        r.apply(
            &ToolCallEvent::ToolCallSucceeded {
                result_ref: "abc".into(),
            },
            Timestamp(5),
        )
        .unwrap();
        assert_eq!(r.result_ref.as_deref(), Some("abc"));
    }
}
