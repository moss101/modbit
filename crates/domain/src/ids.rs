//! Identity types (docs/13 "Identity types"): opaque 128-bit values with a
//! canonical 16-byte encoding. New ids are UUID v7 (time-ordered, random
//! tail); no identity is ever derived from a user-visible name.

use std::fmt;

use serde::{Deserialize, Serialize};

macro_rules! id_type {
    ($($(#[$doc:meta])* $name:ident),* $(,)?) => {$(
        $(#[$doc])*
        #[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(uuid::Uuid);

        impl $name {
            /// A fresh time-ordered identifier.
            #[must_use]
            pub fn new() -> Self {
                Self(uuid::Uuid::now_v7())
            }

            /// Wrap 16 canonical bytes.
            #[must_use]
            pub const fn from_bytes(bytes: [u8; 16]) -> Self {
                Self(uuid::Uuid::from_bytes(bytes))
            }

            /// The 16 canonical bytes.
            #[must_use]
            pub const fn as_bytes(&self) -> &[u8; 16] {
                self.0.as_bytes()
            }

            /// Parse the canonical hyphenated text form.
            pub fn parse(text: &str) -> Result<Self, uuid::Error> {
                uuid::Uuid::parse_str(text).map(Self)
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}", self.0.hyphenated())
            }
        }

        impl fmt::Debug for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}({})", stringify!($name), self.0.hyphenated())
            }
        }

        impl From<$name> for [u8; 16] {
            fn from(id: $name) -> [u8; 16] {
                *id.0.as_bytes()
            }
        }
    )*};
}

id_type! {
    /// Tenant (organization or personal account).
    TenantId,
    /// Human user.
    UserId,
    /// Space (collection of workspaces) within a tenant.
    SpaceId,
    /// Workspace.
    WorkspaceId,
    /// Repository.
    RepositoryId,
    /// Session aggregate.
    SessionId,
    /// Task aggregate.
    TaskId,
    /// Run aggregate.
    RunId,
    /// Turn aggregate.
    TurnId,
    /// RunStep aggregate.
    RunStepId,
    /// Logical agent node.
    AgentId,
    /// Subagent.
    SubagentId,
    /// Tool call; stable across retry and reconnect.
    ToolCallId,
    /// Approval.
    ApprovalId,
    /// Capability lease.
    CapabilityLeaseId,
    /// Protected effect / receipt.
    EffectId,
    /// Workspace/runtime checkpoint.
    CheckpointId,
    /// Compaction epoch.
    CompactionEpochId,
    /// Immutable output reference.
    OutputRefId,
    /// Browser session.
    BrowserSessionId,
    /// Terminal session.
    TerminalSessionId,
    /// Sandbox lease.
    SandboxLeaseId,
    /// Event in the canonical Event Store.
    EventId,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_unique_ordered_and_round_trip() {
        let a = TaskId::new();
        let b = TaskId::new();
        assert_ne!(a, b);
        assert!(a < b, "v7 ids are time-ordered");
        let text = a.to_string();
        assert_eq!(TaskId::parse(&text).unwrap(), a);
        assert_eq!(TaskId::from_bytes(*a.as_bytes()), a);
        let json = serde_json::to_string(&a).unwrap();
        assert_eq!(json, format!("\"{text}\""));
        assert_eq!(serde_json::from_str::<TaskId>(&json).unwrap(), a);
    }
}
