//! Capability Kernel (docs/23 "Capability kernel", "Approval policy",
//! "Emergency stop"; docs/16 "Effect classes"): the host authorization
//! boundary every tool call crosses before any effect (REQ-EV-0080).
//!
//! Decision order, each step able only to tighten (monotonic deny):
//! 1. emergency stop blocks every non-read effect in the session;
//! 2. a valid lease must be presented and must cover every required
//!    operation (tool name alone is not authority) and every resource the
//!    host resolved for the call: the lease's resource selectors
//!    (`capability:glob`, docs/23) are evaluated against the call's
//!    [`ResourceTarget`]s, and a selector that cannot be read fails closed;
//! 3. the [`PolicyEnvelope`] (admin/device authority) denies capabilities and
//!    caps the effect class per execution profile; nothing downstream widens it
//!    (REQ-EV-0091, REQ-EV-0093, REQ-EV-0045). The task's mode posture
//!    (PX-051) follows the envelope: a mode can only narrow it (an effect
//!    ceiling, an allowed-capability set), never widen it;
//! 4. the lease's own effect ceiling;
//! 5. the resolved configuration's per-capability permission (`DENY` denies,
//!    `ASK` escalates);
//! 6. effect classes on the approval list escalate unless an approval bound to
//!    this exact intent hash is present, unexpired and not yet spent: an
//!    approval authorizes one execution, and the dispatch that runs it spends
//!    it (`Approval::consumed_at`), so the same approval never authorizes a
//!    second dispatch — that asks again.
//!
//! The kernel never sees argument text: only the tool's registered effect
//! class, its required capability ids and the intent hash reach it.

use std::collections::{BTreeMap, BTreeSet};

use modbit_domain::Timestamp;
use modbit_domain::approval::Approval;
use modbit_domain::lease::CapabilityLease;
use modbit_domain::mode::TaskMode;
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
/// REQ-EV-0117 plan mode: reads only — no write, no shell, no worktree; the
/// task's product is its plan, reviewed like any candidate.
pub const PROFILE_PLAN: &str = "plan";
/// Execution profile: a task a Cloud Core Worker runs inside a tenant-bound
/// isolated sandbox (docs/21 "Execution profiles", M8): the sandbox confines
/// effects, so the profile's ceiling is the trusted one; its tools are the
/// guest-backed ones (M8.3–M8.5) — a direct host tool does not serve it.
pub const PROFILE_CLOUD_ISOLATED: &str = "cloud_isolated";

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
        profile_ceilings.insert(PROFILE_PLAN.to_owned(), EffectClass::ReadOnly);
        profile_ceilings.insert(PROFILE_CLOUD_ISOLATED.to_owned(), EffectClass::Destructive);
        let mut profile_denied_capabilities = BTreeMap::new();
        profile_denied_capabilities.insert(
            PROFILE_REVIEW_ISOLATED.to_owned(),
            [
                "network.egress",
                "secret.use",
                "git.commit",
                "git.push",
                "deploy",
                // docs/17: `external.call` is excluded from the reviewer
                // projection and kernel-denied under this profile. Listing
                // stays: discovery never grants authority.
                "external.call",
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

/// One resource a call touches, as the host resolved it: the capability that
/// reaches it and its name in the lease's selector vocabulary (for files, the
/// absolute path).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResourceTarget {
    /// Capability id (`fs.write`).
    pub capability: String,
    /// Resource (`/repo/src/lib.rs`).
    pub resource: String,
}

/// A lease resource selector, `capability:glob` (docs/23 examples:
/// `fs.write:/repo/src/**`, `network.egress:api.github.com:443`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResourceSelector<'a> {
    /// Capability the selector constrains.
    pub capability: &'a str,
    /// Glob over the resource name: `**` any run of characters, `*` any run
    /// within one path segment, `?` one character within a segment; a
    /// trailing `/**` also covers the directory itself.
    pub pattern: &'a str,
}

impl<'a> ResourceSelector<'a> {
    /// Parse one selector. `None` for anything that is not
    /// `capability:pattern` with a plain capability id and a non-empty
    /// pattern: an unreadable selector is not read generously.
    #[must_use]
    pub fn parse(text: &'a str) -> Option<Self> {
        let (capability, pattern) = text.split_once(':')?;
        let plain = !capability.is_empty()
            && capability
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'));
        (plain && !pattern.is_empty() && pattern.len() <= 4096 && !pattern.contains('\0'))
            .then_some(Self {
                capability,
                pattern,
            })
    }

    /// Whether `resource` is inside this selector.
    #[must_use]
    pub fn covers(&self, resource: &str) -> bool {
        let pattern = squeeze(&self.pattern.replace('\\', "/"));
        let resource = squeeze(&resource.replace('\\', "/"));
        if glob(pattern.as_bytes(), resource.as_bytes()) {
            return true;
        }
        pattern
            .strip_suffix("/**")
            .is_some_and(|dir| dir == resource.trim_end_matches('/'))
    }
}

/// Collapse runs of `/` (a root written with a trailing slash joins to `//`).
fn squeeze(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if c != '/' || !out.ends_with('/') {
            out.push(c);
        }
    }
    out
}

/// `**` / `*` / `?` glob over bytes (UTF-8 safe: the wildcards are ASCII).
fn glob(pattern: &[u8], text: &[u8]) -> bool {
    match pattern.split_first() {
        None => text.is_empty(),
        Some((b'*', rest)) if rest.first() == Some(&b'*') => {
            let rest = &rest[1..];
            (0..=text.len()).any(|i| glob(rest, &text[i..]))
        }
        Some((b'*', rest)) => {
            let segment = text.iter().position(|b| *b == b'/').unwrap_or(text.len());
            (0..=segment).any(|i| glob(rest, &text[i..]))
        }
        Some((b'?', rest)) => text
            .split_first()
            .is_some_and(|(c, t)| *c != b'/' && glob(rest, t)),
        Some((c, rest)) => text
            .split_first()
            .is_some_and(|(t, tail)| t == c && glob(rest, tail)),
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
    /// The task's mode, as the run's round boundary adopted it (PX-051): its
    /// posture narrows the profile's envelope and never widens it.
    pub mode: TaskMode,
    /// Lease presented, if any.
    pub lease: Option<&'a CapabilityLease>,
    /// Resources the call touches, resolved by the host (never argument
    /// text): each must be covered by a selector of the lease for its
    /// capability. Empty when the tool names no resource the host can
    /// resolve; the boundary that owns the resource (workspace path policy,
    /// egress broker) still judges it.
    pub targets: &'a [ResourceTarget],
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

    /// The denial a task's mode posture gives a call, when it gives one
    /// (PX-051). Public so the host can refuse a call the mode forbids before
    /// anything else judges it, with the same typed code the kernel gives.
    #[must_use]
    pub fn posture_denial(
        mode: TaskMode,
        effect_class: EffectClass,
        required_capabilities: &[String],
    ) -> Option<KernelDecision> {
        mode.posture()
            .refusal(effect_class, required_capabilities)
            .map(|why| KernelDecision::Deny {
                code: "MODE_POSTURE".into(),
                reason: format!(
                    "the task is in {} mode: {why}; the user changes the mode, the agent does not",
                    mode.name()
                ),
            })
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
        // 2b. the lease's resource selectors. A lease whose selectors cannot
        // be read is not authority for anything; a call that touches a
        // resource its lease does not name is outside the lease.
        let mut selectors = Vec::with_capacity(lease.resources.len());
        for text in &lease.resources {
            match ResourceSelector::parse(text) {
                Some(sel) => selectors.push(sel),
                None => {
                    return deny(
                        "LEASE_RESOURCE_MALFORMED",
                        format!(
                            "lease {} carries the selector `{text}`, which is not `capability:pattern`; a lease that cannot be read grants nothing",
                            lease.lease_id
                        ),
                    );
                }
            }
        }
        for t in req.targets {
            let mut named = selectors.iter().filter(|s| s.capability == t.capability);
            if named.clone().next().is_none() {
                return deny(
                    "LEASE_RESOURCE_NOT_COVERED",
                    format!(
                        "lease names no `{}` resource, so it does not cover `{}`",
                        t.capability, t.resource
                    ),
                );
            }
            if !named.any(|s| s.covers(&t.resource)) {
                return deny(
                    "LEASE_RESOURCE_NOT_COVERED",
                    format!(
                        "`{}` is outside the lease's `{}` resources",
                        t.resource, t.capability
                    ),
                );
            }
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
        // 3b. the mode posture: a further narrowing, never a widening.
        if let Some(d) = Self::posture_denial(req.mode, req.effect_class, req.required_capabilities)
        {
            return d;
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
            // REQ-EV-0040: a machine that requires sandboxed execution gets it —
            // execution runs only in an isolated profile, whatever any lower
            // configuration says.
            if let Some(d) = &cfg.device
                && d.value.sandbox_required == Some(true)
                && req.required_capabilities.iter().any(|c| c == "shell.exec")
                && !matches!(
                    req.execution_profile,
                    PROFILE_REVIEW_ISOLATED | PROFILE_CLOUD_ISOLATED
                )
            {
                return deny(
                    "DEVICE_REQUIRES_SANDBOX",
                    format!(
                        "this device requires sandboxed execution; `{}` is not an isolated profile",
                        req.execution_profile
                    ),
                );
            }
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
            // Spent: it authorized one execution and that dispatch has
            // happened. The same intent is asked again, never run again.
            Some(a) if a.is_consumed() => KernelDecision::ApprovalRequired {
                reason: format!(
                    "approval {} was already used by a dispatch of this call; an approval authorizes one execution, so it must be asked again",
                    a.approval_id
                ),
                scope_json: serde_json::json!({
                    "tool": req.tool_name,
                    "effect_class": req.effect_class,
                    "capabilities": req.required_capabilities,
                    "execution_profile": req.execution_profile,
                    "lease_id": lease.lease_id.to_string(),
                    "intent_hash": req.intent_hash,
                    "supersedes_approval": a.approval_id.to_string(),
                })
                .to_string(),
            },
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
        // Reading curated engineering memory is a read, allowed while
        // planning; proposing is a write (excluded here, and under review).
        PROFILE_PLAN => vec!["fs.read", "git.read", "memory.query", "external.list"],
        PROFILE_REVIEW_ISOLATED => {
            vec![
                "fs.read",
                "fs.write",
                "git.read",
                "shell.exec",
                "memory.query",
                "external.list",
            ]
        }
        // The cloud profile's tools act inside the task's sandbox (M8.5,
        // docs/21): its files and its processes, and — through the
        // gateway's egress broker (M8.6) — the configured forge's API host
        // with the token handle it holds; no host browser.
        // M8.8: and the task's browser — Chromium inside the sandbox,
        // its traffic through the same broker, its view streamed to the
        // person through the Core (docs/22 "Cloud browser").
        PROFILE_CLOUD_ISOLATED => vec![
            "fs.read",
            "fs.write",
            "git.read",
            "shell.exec",
            "network.egress",
            "secret.use",
            "browser.control",
            "memory.query",
            "memory.propose",
            "external.list",
            "external.call",
        ],
        // The autonomous profile drives the task's browser session (M7.1,
        // docs/22): the host's sandboxed view, http(s) only, under the
        // session's control lease.
        PROFILE_LOCAL_AUTONOMOUS => vec![
            "fs.read",
            "fs.write",
            "git.read",
            "git.worktree",
            "shell.exec",
            "browser.control",
            "memory.query",
            "memory.propose",
            "external.list",
            "external.call",
        ],
        // The trusted profile also reaches the configured forge (PX-006,
        // docs/23): egress to its one API host and the use of its one token
        // handle — approval-bound for every write, refused elsewhere — and
        // the task's browser session (M7.1).
        _ => vec![
            "fs.read",
            "fs.write",
            "git.read",
            "git.worktree",
            "shell.exec",
            "network.egress",
            "secret.use",
            "browser.control",
            "memory.query",
            "memory.propose",
            "external.list",
            "external.call",
        ],
    };
    let ceiling = match profile {
        PROFILE_LOCAL_TRUSTED | PROFILE_CLOUD_ISOLATED => EffectClass::Destructive,
        PROFILE_PLAN => EffectClass::ReadOnly,
        _ => EffectClass::ReversibleWrite,
    };
    let resources = ops
        .iter()
        .map(|o| match *o {
            "network.egress" => "network.egress:forge-api:443".to_owned(),
            "secret.use" => "secret.use:forge-token -> forge-api".to_owned(),
            "browser.control" => "browser.control:task-session".to_owned(),
            "memory.query" => "memory.query:scope-chain".to_owned(),
            "memory.propose" => "memory.propose:scope-chain".to_owned(),
            // The hub decides which servers a task may see; the lease grants
            // the family, and the host's configuration grants the server.
            "external.list" => "external.list:configured-servers".to_owned(),
            "external.call" => "external.call:configured-servers".to_owned(),
            _ => format!("{o}:{root}/**"),
        })
        .collect();
    (
        resources,
        ops.into_iter().map(str::to_owned).collect(),
        ceiling,
    )
}
