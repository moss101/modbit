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
    /// Workspace paths the call names (never argument text: the paths the
    /// pipeline resolved from the validated arguments), each under the
    /// capability that reaches it. The host joins them to the task root and
    /// the kernel checks them against the lease's resource selectors.
    pub paths: Vec<PathTarget>,
    /// The argv of a command-shaped call, for the run mode's rules (PX-057);
    /// `None` for a call that is not a command.
    pub command: Option<Vec<String>>,
    /// The tool's classifier found a write outside the workspace.
    pub outside_workspace_write: bool,
    /// The tool's classifier found a write to configuration.
    pub protected_path: bool,
    /// The escalation the call declares it needs: `none` | `network` | `all`
    /// (empty = not declared). A typed field, not a reading of the command.
    pub declared_escalation: String,
    /// The facts the tool bound into the intent beyond the arguments
    /// (`Tool::prepare`): shown on the approval, and already part of
    /// `intent_hash`. `None` for a tool that binds nothing.
    pub bound_intent: Option<serde_json::Value>,
}

/// A workspace-relative path a call names, and the capability reaching it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PathTarget {
    /// Capability id (`fs.write`, `fs.read`).
    pub capability: String,
    /// Path as the call wrote it (workspace-relative).
    pub path: String,
}

/// The workspace paths a call's validated arguments name, from the
/// conventional path-valued arguments (`path`, `paths`, `ops[].path`), under
/// the file capability the tool requires (`fs.write` over `fs.read`). A tool
/// that requires no file capability names none; a path-valued argument the
/// convention does not cover is judged by the workspace's own path policy, as
/// it always was.
#[must_use]
pub fn path_targets(required_capabilities: &[String], args: &serde_json::Value) -> Vec<PathTarget> {
    let capability = ["fs.write", "fs.read"]
        .into_iter()
        .find(|c| required_capabilities.iter().any(|r| r == c));
    let Some(capability) = capability else {
        return vec![];
    };
    let mut paths: Vec<&str> = Vec::new();
    if let Some(p) = args.get("path").and_then(serde_json::Value::as_str) {
        paths.push(p);
    }
    if let Some(list) = args.get("paths").and_then(serde_json::Value::as_array) {
        paths.extend(list.iter().filter_map(serde_json::Value::as_str));
    }
    if let Some(ops) = args.get("ops").and_then(serde_json::Value::as_array) {
        paths.extend(
            ops.iter()
                .filter_map(|o| o.get("path").and_then(serde_json::Value::as_str)),
        );
    }
    paths
        .into_iter()
        .map(|p| PathTarget {
            capability: capability.to_owned(),
            path: p.to_owned(),
        })
        .collect()
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
        if ![
            "local_trusted",
            "review_isolated",
            "local_autonomous",
            "plan",
        ]
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
        // REQ-EV-0117 plan mode: nothing but reads, whatever the workspace.
        if req.execution_profile == "plan" && req.effect_class != EffectClass::ReadOnly {
            return PolicyDecision::Deny {
                code: "PLAN_MODE".into(),
                reason: format!(
                    "{:?} effects are absent in plan mode; the task's product is its plan",
                    req.effect_class
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
