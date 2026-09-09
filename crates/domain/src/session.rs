//! Session aggregate (docs/13 "Core aggregates", docs/31 `sessions`).

use serde::{Deserialize, Serialize};

use crate::ids::{SessionId, SpaceId, TaskId, TenantId, UserId};
use crate::state::StateMachine;
use crate::time::Timestamp;

/// Session lifecycle. A session is a long-lived durable container that
/// survives restarts; archiving is the only terminal state.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SessionState {
    /// Accepting tasks and turns.
    Active,
    /// Retained but not accepting mutation until reactivated.
    Suspended,
    /// Closed for mutation; readable forever.
    Archived,
}

impl StateMachine for SessionState {
    const AGGREGATE: &'static str = "Session";

    fn can_transition(self, to: Self) -> bool {
        use SessionState::*;
        matches!(
            (self, to),
            (Active, Suspended) | (Suspended, Active) | (Active, Archived) | (Suspended, Archived)
        )
    }

    fn is_terminal(self) -> bool {
        self == SessionState::Archived
    }
}

/// Session projection state.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Session {
    /// Identity.
    pub session_id: SessionId,
    /// Tenant scope.
    pub tenant_id: TenantId,
    /// Owning user.
    pub user_id: UserId,
    /// Space.
    pub space_id: SpaceId,
    /// Lifecycle state.
    pub state: SessionState,
    /// Aggregate generation, incremented by every applied event.
    pub generation: u64,
    /// Creation time.
    pub created_at: Timestamp,
    /// Last mutation time.
    pub updated_at: Timestamp,
    /// The task currently in focus, if any.
    pub current_task_id: Option<TaskId>,
    /// Kernel lease generation (docs/13 "Fencing and epochs", docs/33 "Session
    /// kernel lease"): the single mutation owner presents this generation; a
    /// stale writer is rejected. `0` until a lease is acquired.
    pub lease_generation: u64,
    /// Opaque identity of the lease owner (client build/kind label).
    pub lease_owner: Option<String>,
}

/// Session events (docs/30 "Session/task").
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "event_type")]
pub enum SessionEvent {
    /// `SessionCreated`.
    SessionCreated {
        /// Tenant scope.
        tenant_id: TenantId,
        /// Owning user.
        user_id: UserId,
        /// Space.
        space_id: SpaceId,
    },
    /// `SessionSuspended`.
    SessionSuspended,
    /// `SessionResumed`.
    SessionResumed,
    /// `SessionArchived`.
    SessionArchived,
    /// `SessionFocusChanged`: the task in focus changed.
    SessionFocusChanged {
        /// New focus.
        task_id: Option<TaskId>,
    },
    /// `SessionLeaseAcquired`: a client became the single mutation owner with
    /// a new, strictly greater lease generation (REQ-EV-0054, REQ-EV-0273).
    SessionLeaseAcquired {
        /// New generation.
        lease_generation: u64,
        /// Owner label.
        owner: String,
    },
}

impl SessionEvent {
    /// Canonical event type name.
    #[must_use]
    pub const fn event_type(&self) -> &'static str {
        match self {
            Self::SessionCreated { .. } => "SessionCreated",
            Self::SessionSuspended => "SessionSuspended",
            Self::SessionResumed => "SessionResumed",
            Self::SessionArchived => "SessionArchived",
            Self::SessionFocusChanged { .. } => "SessionFocusChanged",
            Self::SessionLeaseAcquired { .. } => "SessionLeaseAcquired",
        }
    }
}

impl Session {
    /// Build the initial projection from the creation event.
    pub fn create(
        session_id: SessionId,
        event: &SessionEvent,
        at: Timestamp,
    ) -> Result<Self, crate::InvalidTransition> {
        match event {
            SessionEvent::SessionCreated {
                tenant_id,
                user_id,
                space_id,
            } => Ok(Self {
                session_id,
                tenant_id: *tenant_id,
                user_id: *user_id,
                space_id: *space_id,
                state: SessionState::Active,
                generation: 1,
                created_at: at,
                updated_at: at,
                current_task_id: None,
                lease_generation: 0,
                lease_owner: None,
            }),
            other => Err(crate::InvalidTransition {
                aggregate: "Session",
                from: "<none>".into(),
                to: other.event_type().into(),
            }),
        }
    }

    /// Apply a subsequent event, enforcing the state machine.
    pub fn apply(
        &mut self,
        event: &SessionEvent,
        at: Timestamp,
    ) -> Result<(), crate::InvalidTransition> {
        let next = match event {
            SessionEvent::SessionCreated { .. } => {
                return Err(crate::InvalidTransition {
                    aggregate: "Session",
                    from: format!("{:?}", self.state),
                    to: "SessionCreated".into(),
                });
            }
            SessionEvent::SessionSuspended => Some(SessionState::Suspended),
            SessionEvent::SessionResumed => Some(SessionState::Active),
            SessionEvent::SessionArchived => Some(SessionState::Archived),
            SessionEvent::SessionFocusChanged { task_id } => {
                if self.state.is_terminal() {
                    return Err(crate::InvalidTransition {
                        aggregate: "Session",
                        from: format!("{:?}", self.state),
                        to: "SessionFocusChanged".into(),
                    });
                }
                self.current_task_id = *task_id;
                None
            }
            SessionEvent::SessionLeaseAcquired {
                lease_generation,
                owner,
            } => {
                // Fencing: generations only move forward; an equal or older
                // generation is a stale writer and is never applied silently.
                if self.state.is_terminal() || *lease_generation <= self.lease_generation {
                    return Err(crate::InvalidTransition {
                        aggregate: "Session",
                        from: format!("lease generation {}", self.lease_generation),
                        to: format!("stale lease generation {lease_generation}"),
                    });
                }
                self.lease_generation = *lease_generation;
                self.lease_owner = Some(owner.clone());
                None
            }
        };
        if let Some(to) = next {
            self.state = self.state.transition(to)?;
        }
        self.generation += 1;
        self.updated_at = at;
        Ok(())
    }
}
