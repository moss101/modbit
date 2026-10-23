//! Approval aggregate (docs/23 "Approval policy", docs/31 `approvals`):
//! an approval binds the normalized intent hash + scope + expiry of one tool
//! call. Changing the parameters changes the intent hash and invalidates it.

use serde::{Deserialize, Serialize};

use crate::ids::{ApprovalId, TaskId, ToolCallId};
use crate::state::StateMachine;
use crate::time::Timestamp;
use crate::toolcall::EffectClass;

/// Approval lifecycle.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ApprovalState {
    /// Waiting for a resolver.
    Requested,
    /// Approved: the bound intent may execute once before expiry. "Once" is
    /// [`Approval::consumed_at`]: the dispatch of the bound call spends it.
    Approved,
    /// Denied.
    Denied,
    /// Expired before resolution or use.
    Expired,
}

impl StateMachine for ApprovalState {
    const AGGREGATE: &'static str = "Approval";

    fn can_transition(self, to: Self) -> bool {
        use ApprovalState::*;
        matches!((self, to), (Requested, Approved | Denied | Expired))
    }

    fn is_terminal(self) -> bool {
        !matches!(self, ApprovalState::Requested)
    }
}

/// Approval projection.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Approval {
    /// Identity.
    pub approval_id: ApprovalId,
    /// Task.
    pub task_id: TaskId,
    /// Tool call the approval is bound to.
    pub tool_call_id: ToolCallId,
    /// Tool name.
    pub tool_name: String,
    /// Effect class of the intent.
    pub effect_class: EffectClass,
    /// sha256 of the normalized arguments (the intent).
    pub intent_hash: String,
    /// Scope description (JSON text; e.g. workspace, capability selectors).
    pub scope_json: String,
    /// State.
    pub state: ApprovalState,
    /// Aggregate generation.
    pub generation: u64,
    /// Requested time.
    pub requested_at: Timestamp,
    /// Resolution time.
    pub resolved_at: Option<Timestamp>,
    /// Resolver label.
    pub resolver: Option<String>,
    /// Expiry (millis since epoch); `None` = never.
    pub expires_at: Option<Timestamp>,
    /// When the approval was spent: the bound tool call was dispatched under
    /// it (FIX-15). A spent approval authorizes nothing further — a second
    /// dispatch of the same intent asks again. Derived by the projection from
    /// the call's `ToolCallDispatched`, never set by an event of its own, so
    /// the spend and the dispatch cannot disagree.
    #[serde(default)]
    pub consumed_at: Option<Timestamp>,
}

/// Approval events (docs/30 "Security/effects").
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "event_type")]
pub enum ApprovalEvent {
    /// `ApprovalRequested`.
    ApprovalRequested {
        /// Task.
        task_id: TaskId,
        /// Tool call.
        tool_call_id: ToolCallId,
        /// Tool.
        tool_name: String,
        /// Effect class.
        effect_class: EffectClass,
        /// Intent hash.
        intent_hash: String,
        /// Scope.
        scope_json: String,
        /// Expiry.
        expires_at: Option<Timestamp>,
    },
    /// `ApprovalResolved`.
    ApprovalResolved {
        /// Approved or denied.
        approved: bool,
        /// Who resolved.
        resolver: String,
        /// Reason text.
        reason: String,
    },
    /// `ApprovalExpired`.
    ApprovalExpired,
}

impl ApprovalEvent {
    /// Canonical event type name.
    #[must_use]
    pub const fn event_type(&self) -> &'static str {
        match self {
            Self::ApprovalRequested { .. } => "ApprovalRequested",
            Self::ApprovalResolved { .. } => "ApprovalResolved",
            Self::ApprovalExpired => "ApprovalExpired",
        }
    }
}

impl Approval {
    /// Build from `ApprovalRequested`.
    pub fn create(
        id: ApprovalId,
        event: &ApprovalEvent,
        at: Timestamp,
    ) -> Result<Self, crate::InvalidTransition> {
        match event {
            ApprovalEvent::ApprovalRequested {
                task_id,
                tool_call_id,
                tool_name,
                effect_class,
                intent_hash,
                scope_json,
                expires_at,
            } => Ok(Self {
                approval_id: id,
                task_id: *task_id,
                tool_call_id: *tool_call_id,
                tool_name: tool_name.clone(),
                effect_class: *effect_class,
                intent_hash: intent_hash.clone(),
                scope_json: scope_json.clone(),
                state: ApprovalState::Requested,
                generation: 1,
                requested_at: at,
                resolved_at: None,
                resolver: None,
                expires_at: *expires_at,
                consumed_at: None,
            }),
            other => Err(crate::InvalidTransition {
                aggregate: "Approval",
                from: "<none>".into(),
                to: other.event_type().into(),
            }),
        }
    }

    /// Apply a subsequent event.
    pub fn apply(
        &mut self,
        event: &ApprovalEvent,
        at: Timestamp,
    ) -> Result<(), crate::InvalidTransition> {
        let to = match event {
            ApprovalEvent::ApprovalRequested { .. } => {
                return Err(crate::InvalidTransition {
                    aggregate: "Approval",
                    from: format!("{:?}", self.state),
                    to: "ApprovalRequested".into(),
                });
            }
            ApprovalEvent::ApprovalResolved {
                approved, resolver, ..
            } => {
                self.resolver = Some(resolver.clone());
                if *approved {
                    ApprovalState::Approved
                } else {
                    ApprovalState::Denied
                }
            }
            ApprovalEvent::ApprovalExpired => ApprovalState::Expired,
        };
        self.state = self.state.transition(to)?;
        self.resolved_at = Some(at);
        self.generation += 1;
        Ok(())
    }

    /// Whether the approval was approved and then spent by a dispatch.
    #[must_use]
    pub fn is_consumed(&self) -> bool {
        self.state == ApprovalState::Approved && self.consumed_at.is_some()
    }

    /// Whether this approval authorizes `intent_hash` at `now`: approved,
    /// bound to exactly this intent, unexpired and not yet spent.
    #[must_use]
    pub fn authorizes(&self, intent_hash: &str, now: Timestamp) -> bool {
        self.state == ApprovalState::Approved
            && self.consumed_at.is_none()
            && self.intent_hash == intent_hash
            && self.expires_at.is_none_or(|e| now.0 < e.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn approval_binds_intent_hash_and_expiry() {
        let mut a = Approval::create(
            ApprovalId::new(),
            &ApprovalEvent::ApprovalRequested {
                task_id: TaskId::new(),
                tool_call_id: ToolCallId::new(),
                tool_name: "git.worktree.close".into(),
                effect_class: EffectClass::Destructive,
                intent_hash: "h1".into(),
                scope_json: "{}".into(),
                expires_at: Some(Timestamp(100)),
            },
            Timestamp(1),
        )
        .unwrap();
        assert!(!a.authorizes("h1", Timestamp(2)), "not yet approved");
        a.apply(
            &ApprovalEvent::ApprovalResolved {
                approved: true,
                resolver: "user".into(),
                reason: "ok".into(),
            },
            Timestamp(3),
        )
        .unwrap();
        assert!(a.authorizes("h1", Timestamp(50)));
        assert!(!a.authorizes("h2", Timestamp(50)), "changed parameters");
        assert!(!a.authorizes("h1", Timestamp(100)), "expired");
        assert!(
            a.apply(&ApprovalEvent::ApprovalExpired, Timestamp(4))
                .is_err()
        );
        // Spent by a dispatch: the same intent, still unexpired, no longer
        // authorizes.
        a.consumed_at = Some(Timestamp(60));
        assert!(a.is_consumed());
        assert!(
            !a.authorizes("h1", Timestamp(61)),
            "an approval is single-use"
        );
    }
}
