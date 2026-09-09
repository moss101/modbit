//! Canonical event envelope (docs/13 "Canonical event envelope", docs/31
//! `events`). Ordering is guaranteed per aggregate sequence, never globally;
//! projection consumers must be idempotent.

use serde::{Deserialize, Serialize};

use crate::ids::{EventId, RunId, RunStepId, SessionId, TaskId, TenantId, TurnId, UserId};
use crate::time::Timestamp;

/// Schema version written by this build for every event payload.
pub const SCHEMA_VERSION: u32 = 1;

/// Aggregate kinds that own an event sequence.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AggregateType {
    /// Session.
    Session,
    /// Task.
    Task,
    /// Run.
    Run,
    /// Turn.
    Turn,
    /// RunStep.
    RunStep,
    /// Tool call.
    ToolCall,
    /// Approval.
    Approval,
    /// Capability lease.
    CapabilityLease,
    /// Checkpoint.
    Checkpoint,
    /// Compaction epoch.
    CompactionEpoch,
    /// Workspace (worktree) change stream.
    Workspace,
}

impl AggregateType {
    /// Stable storage name.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Session => "session",
            Self::Task => "task",
            Self::Run => "run",
            Self::Turn => "turn",
            Self::RunStep => "run_step",
            Self::ToolCall => "tool_call",
            Self::Approval => "approval",
            Self::CapabilityLease => "capability_lease",
            Self::Checkpoint => "checkpoint",
            Self::CompactionEpoch => "compaction_epoch",
            Self::Workspace => "workspace",
        }
    }

    /// Parse a storage name.
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "session" => Self::Session,
            "task" => Self::Task,
            "run" => Self::Run,
            "turn" => Self::Turn,
            "run_step" => Self::RunStep,
            "tool_call" => Self::ToolCall,
            "approval" => Self::Approval,
            "capability_lease" => Self::CapabilityLease,
            "checkpoint" => Self::Checkpoint,
            "compaction_epoch" => Self::CompactionEpoch,
            "workspace" => Self::Workspace,
            _ => return None,
        })
    }
}

/// Who caused an event.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "actor_type", content = "actor_id", rename_all = "snake_case")]
pub enum Actor {
    /// A human user.
    User(UserId),
    /// The Core runtime itself (scheduler, recovery).
    Core(String),
    /// A logical agent node.
    Agent(String),
    /// An external system (forge webhook, CI).
    External(String),
}

/// Event payload: inline JSON up to the store's inline ceiling, otherwise a
/// content-addressed object hash (docs/33 "Backpressure", docs/31 `events`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PayloadRef {
    /// Inline JSON payload.
    Inline {
        /// The payload.
        payload: serde_json::Value,
    },
    /// SHA-256 hex of the payload bytes in the object store.
    Object {
        /// Lowercase hex digest.
        object_hash: String,
        /// Payload byte length.
        byte_length: u64,
    },
}

/// One immutable, stored event.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventEnvelope {
    /// Identity.
    pub event_id: EventId,
    /// Tenant scope.
    pub tenant_id: TenantId,
    /// Session the event belongs to (every aggregate lives inside a session).
    pub session_id: SessionId,
    /// Task lineage.
    pub task_id: Option<TaskId>,
    /// Run lineage.
    pub run_id: Option<RunId>,
    /// Turn lineage.
    pub turn_id: Option<TurnId>,
    /// Step lineage.
    pub step_id: Option<RunStepId>,
    /// Aggregate kind.
    pub aggregate_type: AggregateType,
    /// Aggregate identity (16 canonical bytes of the typed id).
    pub aggregate_id: [u8; 16],
    /// Per-aggregate monotonic sequence starting at 1.
    pub sequence: u64,
    /// Canonical event type name.
    pub event_type: String,
    /// Payload schema version.
    pub schema_version: u32,
    /// When it happened.
    pub occurred_at: Timestamp,
    /// Who caused it.
    pub actor: Actor,
    /// The event that caused this one.
    pub causation_id: Option<EventId>,
    /// Correlation across a request.
    pub correlation_id: Option<EventId>,
    /// Payload.
    pub payload: PayloadRef,
    /// SHA-256 hex over the previous event's hash (same aggregate) and this
    /// event's canonical content; the store verifies the chain.
    pub integrity_hash: String,
}
