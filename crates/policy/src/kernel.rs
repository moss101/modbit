//! Capability Kernel (docs/23 "Capability kernel", "Approval policy",
//! "Emergency stop"; docs/16 "Effect classes"): the host authorization
//! boundary every tool call crosses before any effect (REQ-EV-0080).
//!
//! Decision order, each step able only to tighten (monotonic deny):
//! 1. emergency stop blocks every non-read effect in the session;
//! 2. a valid lease must be presented and must cover every required
//!    operation (tool name alone is not authority);
//! 3. the [`PolicyEnvelope`] (admin/device authority) denies capabilities and
//!    caps the effect class per execution profile; nothing downstream widens it
//!    (REQ-EV-0091, REQ-EV-0093, REQ-EV-0045);
//! 4. the lease's own effect ceiling;
//! 5. the resolved configuration's per-capability permission (`DENY` denies,
//!    `ASK` escalates);
//! 6. effect classes on the approval list escalate unless an approval bound to
//!    this exact intent hash is present and unexpired.
//!
//! The kernel never sees argument text: only the tool's registered effect
//! class, its required capability ids and the intent hash reach it.

use std::collections::{BTreeMap, BTreeSet};

use modbit_domain::Timestamp;
use modbit_domain::approval::Approval;
use modbit_domain::lease::CapabilityLease;
use modbit_domain::toolcall::EffectClass;
use serde::{Deserialize, Serialize};

use crate::config::{Permission, ResolvedConfig};

/// Execution profile: unattended, bounded, never above `REVERSIBLE_WRITE`
/// and never able to ask (REQ-EV-0045).
pub const PROFILE_LOCAL_AUTONOMOUS: &str = "local_autonomous";
/// Execution profile: interactive local workspace under host policy.
pub const PROFILE_LOCAL_TRUSTED: &str = "local_trusted";
/// Execution profile: isolated non-committing reviewer (docs/21).
pub const PROFILE_REVIEW_ISOLATED: &str = "review_isolated";

/// Admin/device-level policy that lower authorities cannot weaken.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PolicyEnvelope {
    /// Execution profile → highest effect class ever allowed under it.
    pub profile_ceilings: BTreeMap<String, EffectClass>,
    /// Execution profile → capability ids denied outright under it.
    pub profile_denied_capabilities: BTreeMap<String, BTreeSet<String>>,
    /// Capability ids denied for every profile (org deny rule).
    pub denied_capabilities: BTreeSet<String>,
    /// Effect classes that always need an approval (docs/23 "Approval policy").
    pub approval_effects: BTreeSet<EffectClass>,
    /// Profiles that can never wait for an approval (unattended).
    pub no_approval_profiles: BTreeSet<String>,
    /// The assurance section (REQ-EPR-008): minimum assurance, protected
    /// surfaces, review and human conditions, forbidden effects.
    pub assurance: crate::assurance::AssurancePolicy,
}

impl Default for PolicyEnvelope {
    fn default() -> Self {
        let mut profile_ceilings = BTreeMap::new();
        profile_ceilings.insert(PROFILE_LOCAL_TRUSTED.to_owned(), EffectClass::Destructive);
        profile_ceilings.insert(
            PROFILE_REVIEW_ISOLATED.to_owned(),
            EffectClass::ReversibleWrite,
        );
        profile_ceilings.insert(
            PROFILE_LOCAL_AUTONOMOUS.to_owned(),
            EffectClass::ReversibleWrite,
        );
        let mut profile_denied_capabilities = BTreeMap::new();
        profile_denied_capabilities.insert(
            PROFILE_REVIEW_ISOLATED.to_owned(),
            [
                "network.egress",
                "secret.use",
                "git.commit",
                "git.push",
                "deploy",
            ]
            .into_iter()
            .map(str::to_owned)
            .collect(),
        );
        profile_denied_capabilities.insert(
            PROFILE_LOCAL_AUTONOMOUS.to_owned(),
            ["secret.use", "deploy"]
                .into_iter()
                .map(str::to_owned)
                .collect(),
        );
        Self {
            profile_ceilings,
            profile_denied_capabilities,
            denied_capabilities: BTreeSet::new(),
            approval_effects: [
                EffectClass::ProtectedWrite,
                EffectClass::ExternalSideEffect,
                EffectClass::SecretAccess,
                EffectClass::Destructive,
            ]
            .into_iter()
            .collect(),
            no_approval_profiles: [PROFILE_LOCAL_AUTONOMOUS.to_owned()].into_iter().collect(),
            assurance: crate::assurance::AssurancePolicy::default(),
        }
    }
}

/// What the kernel is asked.
#[derive(Clone, Debug)]
pub struct KernelRequest<'a> {
    /// Tool name (for messages only; never authority).
    pub tool_name: &'a str,
    /// Registered effect class.
    pub effect_class: EffectClass,
    /// Capability ids the tool requires.
    pub required_capabilities: &'a [String],
    /// Task execution profile.
    pub execution_profile: &'a str,
    /// Lease presented, if any.
    pub lease: Option<&'a CapabilityLease>,
    /// Approval bound to this call, if any.
    pub approval: Option<&'a Approval>,
    /// sha256 of the normalized arguments.
    pub intent_hash: &'a str,
    /// Resolved admin/project/user configuration, if any.
    pub config: Option<&'a ResolvedConfig>,
    /// Session emergency stop active.
    pub emergency_stopped: bool,
    /// Now.
    pub now: Timestamp,
}

/// Kernel verdict.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "verdict", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum KernelDecision {
    /// Execute.
    Allow {
        /// Rule that allowed.
        rule: String,
        /// Approval consumed, if any.
        approval_id: Option<modbit_domain::ApprovalId>,
    },
    /// Never, under this profile/lease/policy.
    Deny {
        /// Stable code.
        code: String,
        /// Reason.
        reason: String,
    },
    /// An approval bound to `intent_hash` could unlock it.
    ApprovalRequired {
        /// Reason.
        reason: String,
        /// Scope the approval would bind (JSON text).
        scope_json: String,
    },
}

/// The kernel.
#[derive(Clone, Debug, Default)]
pub struct CapabilityKernel {
    envelope: PolicyEnvelope,
}

impl CapabilityKernel {
    /// Kernel over an envelope.
    #[must_use]
    pub fn new(envelope: PolicyEnvelope) -> Self {
        Self { envelope }
    }

    /// The envelope.
    #[must_use]
    pub fn envelope(&self) -> &PolicyEnvelope {
        &self.envelope
    }

    /// Decide. See the module documentation for the order.
    #[must_use]
    pub fn decide(&self, req: &KernelRequest<'_>) -> KernelDecision {
        let deny = |code: &str, reason: String| KernelDecision::Deny {
            code: code.into(),
            reason,
        };
        // 1. emergency stop: reads may continue, effects may not.
        if req.emergency_stopped && req.effect_class != EffectClass::ReadOnly {
            return deny(
                "EMERGENCY_STOP",
                "session is under emergency stop; new effects are blocked".into(),
            );
        }
        // 2. lease.
        let Some(lease) = req.lease else {
            return deny(
                "LEASE_REQUIRED",
                format!("tool `{}` presented no capability lease", req.tool_name),
            );
        };
        if !lease.is_valid(req.now) {
            return deny(
                "LEASE_INVALID",
                format!(
                    "lease {} is {:?}{}",
                    lease.lease_id,
                    lease.state,
                    lease
                        .revoke_reason
                        .as_deref()
                        .map(|r| format!(" ({r})"))
                        .unwrap_or_default()
                ),
            );
        }
        if lease.execution_profile != req.execution_profile {
            return deny(
                "LEASE_PROFILE_MISMATCH",
                format!(
                    "lease is for `{}`, task runs `{}`",
                    lease.execution_profile, req.execution_profile
                ),
            );
        }
        if let Some(missing) = req
            .required_capabilities
            .iter()
            .find(|c| !lease.operations.iter().any(|o| o == *c))
        {
            return deny(
                "CAPABILITY_NOT_LEASED",
                format!("lease does not grant `{missing}`"),
            );
        }
        // 3. envelope (admin/device authority).
        let Some(ceiling) = self.envelope.profile_ceilings.get(req.execution_profile) else {
            return deny(
                "PROFILE_UNSUPPORTED",
                format!(
                    "execution profile `{}` is not served by this build",
                    req.execution_profile
                ),
            );
        };
        if let Some(c) = req
            .required_capabilities
            .iter()
            .find(|c| self.envelope.denied_capabilities.contains(*c))
        {
            return deny(
                "CAPABILITY_DENIED_BY_POLICY",
                format!("`{c}` is denied by organization policy"),
            );
        }
        if let Some(c) = req.required_capabilities.iter().find(|c| {
            self.envelope
                .profile_denied_capabilities
                .get(req.execution_profile)
                .is_some_and(|d| d.contains(*c))
        }) {
            return deny(
                "CAPABILITY_DENIED_FOR_PROFILE",
                format!("`{c}` is denied under `{}`", req.execution_profile),
            );
        }
        if req.effect_class > *ceiling {
            return deny(
                "PROFILE_CEILING",
                format!(
                    "{:?} exceeds the `{}` ceiling {:?}",
                    req.effect_class, req.execution_profile, ceiling
                ),
            );
        }
        // 4. lease ceiling.
        if req.effect_class > lease.effect_ceiling {
            return deny(
                "LEASE_CEILING",
                format!(
                    "{:?} exceeds the lease ceiling {:?}",
                    req.effect_class, lease.effect_ceiling
                ),
            );
        }
        // 5. resolved configuration.
        let mut ask_reason: Option<String> = None;
        if let Some(cfg) = req.config {
            for c in req.required_capabilities {
                match cfg.permissions.get(c).map(|p| p.value) {
                    Some(Permission::Deny) => {
                        let by = cfg
                            .permissions
                            .get(c)
                            .map(|p| format!("{:?}", p.provenance.decided_by))
                            .unwrap_or_default();
                        return deny(
                            "CAPABILITY_DENIED_BY_CONFIG",
                            format!("`{c}` is DENY by {by} configuration"),
                        );
                    }
                    Some(Permission::Ask) if ask_reason.is_none() => {
                        ask_reason = Some(format!("`{c}` is ASK by configuration"));
                    }
                    _ => {}
                }
            }
        }
        // 6. approval-gated effects.
        if self.envelope.approval_effects.contains(&req.effect_class) && ask_reason.is_none() {
            ask_reason = Some(format!(
                "{:?} effects require an approval bound to the intent",
                req.effect_class
            ));
        }
        let Some(reason) = ask_reason else {
            return KernelDecision::Allow {
                rule: format!("{}:{:?}", req.execution_profile, req.effect_class).to_lowercase(),
                approval_id: None,
            };
        };
        if self
            .envelope
            .no_approval_profiles
            .contains(req.execution_profile)
        {
            return deny(
                "APPROVAL_UNAVAILABLE",
                format!(
                    "{reason}; `{}` runs unattended and cannot ask",
                    req.execution_profile
                ),
            );
        }
        match req.approval {
            Some(a) if a.authorizes(req.intent_hash, req.now) => KernelDecision::Allow {
                rule: format!("approval:{}", a.approval_id),
                approval_id: Some(a.approval_id),
            },
            Some(a) if a.state == modbit_domain::approval::ApprovalState::Denied => deny(
                "APPROVAL_DENIED",
                format!("approval {} was denied", a.approval_id),
            ),
            Some(a) if a.state == modbit_domain::approval::ApprovalState::Approved => deny(
                "APPROVAL_MISMATCH",
                format!(
                    "approval {} binds a different intent or has expired; request a new approval",
                    a.approval_id
                ),
            ),
            _ => KernelDecision::ApprovalRequired {
                reason,
                scope_json: serde_json::json!({
                    "tool": req.tool_name,
                    "effect_class": req.effect_class,
                    "capabilities": req.required_capabilities,
                    "execution_profile": req.execution_profile,
                    "lease_id": lease.lease_id.to_string(),
                    "intent_hash": req.intent_hash,
                })
                .to_string(),
            },
        }
    }
}

/// Default lease contents for a task under `profile` (docs/23 examples).
/// Returns `(resources, operations, effect_ceiling)`.
#[must_use]
pub fn default_lease_for_profile(
    profile: &str,
    workspace_root: Option<&str>,
) -> (Vec<String>, Vec<String>, EffectClass) {
    let root = workspace_root.unwrap_or("<none>");
    let ops: Vec<&str> = match profile {
        PROFILE_REVIEW_ISOLATED => vec!["fs.read", "fs.write", "git.read", "shell.exec"],
        PROFILE_LOCAL_AUTONOMOUS => vec![
            "fs.read",
            "fs.write",
            "git.read",
            "git.worktree",
            "shell.exec",
        ],
        _ => vec![
            "fs.read",
            "fs.write",
            "git.read",
            "git.worktree",
            "shell.exec",
        ],
    };
    let ceiling = match profile {
        PROFILE_LOCAL_TRUSTED => EffectClass::Destructive,
        _ => EffectClass::ReversibleWrite,
    };
    let resources = ops.iter().map(|o| format!("{o}:{root}/**")).collect();
    (
        resources,
        ops.into_iter().map(str::to_owned).collect(),
        ceiling,
    )
}
