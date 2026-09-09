//! Capability Kernel port (docs/23). The kernel itself is M2.5; this module
//! defines the boundary the pipeline calls and a conservative default policy
//! for `local_trusted`: routine reads and reversible writes inside an
//! approved workspace are allowed, everything else requires approval.

use serde::{Deserialize, Serialize};

use crate::EffectClass;

/// What the pipeline asks the kernel.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PolicyRequest {
    /// Tool name.
    pub tool_name: String,
    /// Effect class as registered.
    pub effect_class: EffectClass,
    /// Capability selectors the tool requires.
    pub required_capabilities: Vec<String>,
    /// Execution profile of the task.
    pub execution_profile: String,
    /// Whether the task has an approved workspace root.
    pub has_workspace: bool,
    /// Whether the caller presented a capability lease.
    pub has_lease: bool,
}

/// Decision.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "verdict", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PolicyDecision {
    /// Allowed.
    Allow {
        /// Rule that allowed.
        rule: String,
    },
    /// Denied.
    Deny {
        /// Code.
        code: String,
        /// Reason.
        reason: String,
        /// Whether an approval could unlock it.
        approval_required: bool,
    },
}

/// The boundary.
pub trait CapabilityPort: Send + Sync {
    /// Decide before any effect. Implementations must not read argument text.
    fn decide(&self, req: &PolicyRequest) -> PolicyDecision;
}

/// Default M2.4 policy (docs/23 "Approval policy" defaults).
#[derive(Debug, Default)]
pub struct ProfilePolicy;

impl CapabilityPort for ProfilePolicy {
    fn decide(&self, req: &PolicyRequest) -> PolicyDecision {
        if req.execution_profile != "local_trusted" && req.execution_profile != "review_isolated" {
            return PolicyDecision::Deny {
                code: "PROFILE_UNSUPPORTED".into(),
                reason: format!(
                    "execution profile `{}` is not served by this build",
                    req.execution_profile
                ),
                approval_required: false,
            };
        }
        match req.effect_class {
            EffectClass::ReadOnly => PolicyDecision::Allow {
                rule: "local_trusted:read_only".into(),
            },
            EffectClass::ReversibleWrite if req.has_workspace => PolicyDecision::Allow {
                rule: "local_trusted:reversible_write_in_workspace".into(),
            },
            EffectClass::ReversibleWrite => PolicyDecision::Deny {
                code: "NO_WORKSPACE".into(),
                reason: "reversible writes need an approved workspace root".into(),
                approval_required: false,
            },
            EffectClass::ProtectedWrite
            | EffectClass::ExternalSideEffect
            | EffectClass::SecretAccess
            | EffectClass::Destructive => PolicyDecision::Deny {
                code: "APPROVAL_REQUIRED".into(),
                reason: format!(
                    "{:?} effects require an approval bound to the intent hash (Capability Kernel, M2.5)",
                    req.effect_class
                ),
                approval_required: true,
            },
        }
    }
}
