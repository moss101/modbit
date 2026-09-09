//! Capability Kernel port (docs/23): the boundary the pipeline calls before
//! any effect. The kernel itself lives in `modbit-policy::kernel` and is
//! adapted per call by the Core (it needs the task's lease, the approval bound
//! to the call and the session's emergency-stop state). [`ProfilePolicy`] is
//! the lease-less default used by tests and by tools running without a Core:
//! routine reads and reversible writes inside an approved workspace are
//! allowed, everything else requires an approval.

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
    /// sha256 of the normalized arguments: the intent an approval binds to.
    pub intent_hash: String,
}

/// Decision.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "verdict", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PolicyDecision {
    /// Allowed.
    Allow {
        /// Rule that allowed.
        rule: String,
        /// Approval consumed, if the rule was an approval.
        #[serde(default)]
        approval_id: Option<String>,
    },
    /// Not now: an approval bound to the intent hash could unlock it.
    ApprovalRequired {
        /// Reason.
        reason: String,
        /// Scope the approval would bind (JSON text).
        scope_json: String,
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
        if !["local_trusted", "review_isolated", "local_autonomous"]
            .contains(&req.execution_profile.as_str())
        {
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
                approval_id: None,
            },
            EffectClass::ReversibleWrite if req.has_workspace => PolicyDecision::Allow {
                rule: "local_trusted:reversible_write_in_workspace".into(),
                approval_id: None,
            },
            EffectClass::ReversibleWrite => PolicyDecision::Deny {
                code: "NO_WORKSPACE".into(),
                reason: "reversible writes need an approved workspace root".into(),
                approval_required: false,
            },
            EffectClass::ProtectedWrite
            | EffectClass::ExternalSideEffect
            | EffectClass::SecretAccess
            | EffectClass::Destructive => PolicyDecision::ApprovalRequired {
                reason: format!(
                    "{:?} effects require an approval bound to the intent hash",
                    req.effect_class
                ),
                scope_json: serde_json::json!({
                    "tool": req.tool_name,
                    "effect_class": req.effect_class,
                    "capabilities": req.required_capabilities,
                    "execution_profile": req.execution_profile,
                    "intent_hash": req.intent_hash,
                })
                .to_string(),
            },
        }
    }
}
