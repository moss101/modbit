//! CapabilityLease aggregate (docs/23 "Capability kernel", docs/31
//! `capability_leases`): binds tenant/task/agent, resource selectors, an
//! operation set, an effect ceiling, an execution profile, expiry and a
//! generation. Tools must present a valid lease; a tool name is not authority.

use serde::{Deserialize, Serialize};

use crate::ids::{CapabilityLeaseId, TaskId, TenantId};
use crate::state::StateMachine;
use crate::time::Timestamp;
use crate::toolcall::EffectClass;

/// Lease lifecycle.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum LeaseState {
    /// Usable.
    Active,
    /// Revoked (emergency stop, task end, policy tightening).
    Revoked,
}

impl StateMachine for LeaseState {
    const AGGREGATE: &'static str = "CapabilityLease";

    fn can_transition(self, to: Self) -> bool {
        matches!((self, to), (LeaseState::Active, LeaseState::Revoked))
    }

    fn is_terminal(self) -> bool {
        matches!(self, LeaseState::Revoked)
    }
}

/// Lease projection.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapabilityLease {
    /// Identity.
    pub lease_id: CapabilityLeaseId,
    /// Tenant.
    pub tenant_id: TenantId,
    /// Task.
    pub task_id: TaskId,
    /// Agent label (`None` = the task itself).
    pub agent_id: Option<String>,
    /// Resource selectors, e.g. `fs.read:/repo/**`.
    pub resources: Vec<String>,
    /// Operation set (capability ids), e.g. `fs.read`, `shell.exec`.
    pub operations: Vec<String>,
    /// Highest effect class this lease can ever authorize.
    pub effect_ceiling: EffectClass,
    /// Execution profile the lease is valid under.
    pub execution_profile: String,
    /// Generation (bumped on regrant).
    pub generation: u64,
    /// State.
    pub state: LeaseState,
    /// Expiry; `None` = task lifetime.
    pub expires_at: Option<Timestamp>,
    /// Revocation time.
    pub revoked_at: Option<Timestamp>,
    /// Revocation reason.
    pub revoke_reason: Option<String>,
}

/// Lease events (docs/30 "Security/effects").
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "event_type")]
pub enum CapabilityLeaseEvent {
    /// `CapabilityLeaseGranted`.
    CapabilityLeaseGranted {
        /// Tenant.
        tenant_id: TenantId,
        /// Task.
        task_id: TaskId,
        /// Agent.
        agent_id: Option<String>,
        /// Resources.
        resources: Vec<String>,
        /// Operations.
        operations: Vec<String>,
        /// Ceiling.
        effect_ceiling: EffectClass,
        /// Profile.
        execution_profile: String,
        /// Generation.
        generation: u64,
        /// Expiry.
        expires_at: Option<Timestamp>,
    },
    /// `CapabilityLeaseRevoked`.
    CapabilityLeaseRevoked {
        /// Reason.
        reason: String,
    },
}

impl CapabilityLeaseEvent {
    /// Canonical event type name.
    #[must_use]
    pub const fn event_type(&self) -> &'static str {
        match self {
            Self::CapabilityLeaseGranted { .. } => "CapabilityLeaseGranted",
            Self::CapabilityLeaseRevoked { .. } => "CapabilityLeaseRevoked",
        }
    }
}

impl CapabilityLease {
    /// Build from `CapabilityLeaseGranted`.
    pub fn create(
        id: CapabilityLeaseId,
        event: &CapabilityLeaseEvent,
        _at: Timestamp,
    ) -> Result<Self, crate::InvalidTransition> {
        match event {
            CapabilityLeaseEvent::CapabilityLeaseGranted {
                tenant_id,
                task_id,
                agent_id,
                resources,
                operations,
                effect_ceiling,
                execution_profile,
                generation,
                expires_at,
            } => Ok(Self {
                lease_id: id,
                tenant_id: *tenant_id,
                task_id: *task_id,
                agent_id: agent_id.clone(),
                resources: resources.clone(),
                operations: operations.clone(),
                effect_ceiling: *effect_ceiling,
                execution_profile: execution_profile.clone(),
                generation: *generation,
                state: LeaseState::Active,
                expires_at: *expires_at,
                revoked_at: None,
                revoke_reason: None,
            }),
            other => Err(crate::InvalidTransition {
                aggregate: "CapabilityLease",
                from: "<none>".into(),
                to: other.event_type().into(),
            }),
        }
    }

    /// Apply a subsequent event.
    pub fn apply(
        &mut self,
        event: &CapabilityLeaseEvent,
        at: Timestamp,
    ) -> Result<(), crate::InvalidTransition> {
        match event {
            CapabilityLeaseEvent::CapabilityLeaseGranted { .. } => Err(crate::InvalidTransition {
                aggregate: "CapabilityLease",
                from: format!("{:?}", self.state),
                to: "CapabilityLeaseGranted".into(),
            }),
            CapabilityLeaseEvent::CapabilityLeaseRevoked { reason } => {
                self.state = self.state.transition(LeaseState::Revoked)?;
                self.revoked_at = Some(at);
                self.revoke_reason = Some(reason.clone());
                Ok(())
            }
        }
    }

    /// Whether the lease is usable at `now`.
    #[must_use]
    pub fn is_valid(&self, now: Timestamp) -> bool {
        self.state == LeaseState::Active && self.expires_at.is_none_or(|e| now.0 < e.0)
    }

    /// Whether the lease covers every operation in `required`.
    #[must_use]
    pub fn covers(&self, required: &[String]) -> bool {
        required
            .iter()
            .all(|r| self.operations.iter().any(|o| o == r))
    }
}
