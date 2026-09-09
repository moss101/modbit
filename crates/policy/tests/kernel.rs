//! Capability Kernel qualification: the host boundary decides from registered
//! effect class, lease, envelope, configuration and intent-bound approvals.

use std::collections::BTreeMap;

use modbit_domain::approval::{Approval, ApprovalEvent};
use modbit_domain::lease::{CapabilityLease, CapabilityLeaseEvent};
use modbit_domain::toolcall::EffectClass;
use modbit_domain::{ApprovalId, CapabilityLeaseId, TaskId, TenantId, Timestamp, ToolCallId};
use modbit_policy::{
    Authority, CapabilityKernel, KernelDecision, KernelRequest, Layer, Permission, PolicyEnvelope,
    default_lease_for_profile, resolve,
};

fn lease(profile: &str) -> CapabilityLease {
    let (resources, operations, ceiling) = default_lease_for_profile(profile, Some("/repo"));
    CapabilityLease::create(
        CapabilityLeaseId::new(),
        &CapabilityLeaseEvent::CapabilityLeaseGranted {
            tenant_id: TenantId::new(),
            task_id: TaskId::new(),
            agent_id: None,
            resources,
            operations,
            effect_ceiling: ceiling,
            execution_profile: profile.into(),
            generation: 1,
            expires_at: None,
        },
        Timestamp(1),
    )
    .unwrap()
}

fn req<'a>(
    tool: &'a str,
    effect: EffectClass,
    caps: &'a [String],
    profile: &'a str,
    lease: Option<&'a CapabilityLease>,
) -> KernelRequest<'a> {
    KernelRequest {
        tool_name: tool,
        effect_class: effect,
        required_capabilities: caps,
        execution_profile: profile,
        lease,
        approval: None,
        intent_hash: "h1",
        config: None,
        emergency_stopped: false,
        now: Timestamp(10),
    }
}

fn caps(v: &[&str]) -> Vec<String> {
    v.iter().map(|s| (*s).to_owned()).collect()
}

fn code(d: &KernelDecision) -> String {
    match d {
        KernelDecision::Deny { code, .. } => code.clone(),
        KernelDecision::Allow { .. } => "ALLOW".into(),
        KernelDecision::ApprovalRequired { .. } => "APPROVAL_REQUIRED".into(),
    }
}

#[test]
fn lease_is_authority_not_tool_name() {
    let k = CapabilityKernel::default();
    let c = caps(&["fs.read"]);
    assert_eq!(
        code(&k.decide(&req(
            "fs.read",
            EffectClass::ReadOnly,
            &c,
            "local_trusted",
            None
        ))),
        "LEASE_REQUIRED"
    );
    let l = lease("local_trusted");
    assert_eq!(
        code(&k.decide(&req(
            "fs.read",
            EffectClass::ReadOnly,
            &c,
            "local_trusted",
            Some(&l)
        ))),
        "ALLOW"
    );
    let net = caps(&["network.egress"]);
    assert_eq!(
        code(&k.decide(&req(
            "http.get",
            EffectClass::ReadOnly,
            &net,
            "local_trusted",
            Some(&l)
        ))),
        "CAPABILITY_NOT_LEASED"
    );
    let mut revoked = l.clone();
    revoked
        .apply(
            &CapabilityLeaseEvent::CapabilityLeaseRevoked {
                reason: "task ended".into(),
            },
            Timestamp(5),
        )
        .unwrap();
    assert_eq!(
        code(&k.decide(&req(
            "fs.read",
            EffectClass::ReadOnly,
            &c,
            "local_trusted",
            Some(&revoked)
        ))),
        "LEASE_INVALID"
    );
    let other = lease("review_isolated");
    assert_eq!(
        code(&k.decide(&req(
            "fs.read",
            EffectClass::ReadOnly,
            &c,
            "local_trusted",
            Some(&other)
        ))),
        "LEASE_PROFILE_MISMATCH"
    );
}

/// QUAL-EV-0045: an autonomous run cannot request higher privilege than the
/// profile ceiling, and it cannot ask for it either.
#[test]
fn qual_ev_0045_autonomous_profile_cannot_exceed_or_ask_past_its_ceiling() {
    let k = CapabilityKernel::default();
    let l = lease("local_autonomous");
    let c = caps(&["git.worktree"]);
    let d = k.decide(&req(
        "git.worktree.close",
        EffectClass::Destructive,
        &c,
        "local_autonomous",
        Some(&l),
    ));
    assert_eq!(code(&d), "PROFILE_CEILING", "{d:?}");
    // Even a hand-built lease claiming a higher ceiling is capped by the envelope.
    let mut wide = l.clone();
    wide.effect_ceiling = EffectClass::Destructive;
    let d = k.decide(&req(
        "git.worktree.close",
        EffectClass::Destructive,
        &c,
        "local_autonomous",
        Some(&wide),
    ));
    assert_eq!(code(&d), "PROFILE_CEILING", "{d:?}");
    // A reversible write under the ceiling that configuration marks ASK is denied, not queued.
    let mut layers = BTreeMap::new();
    let mut user = Layer::default();
    user.permissions
        .insert("shell.exec".into(), Permission::Ask);
    layers.insert(Authority::User, user);
    let cfg = resolve(&layers);
    let sh = caps(&["shell.exec"]);
    let mut r = req(
        "shell.exec",
        EffectClass::ReversibleWrite,
        &sh,
        "local_autonomous",
        Some(&l),
    );
    r.config = Some(&cfg);
    assert_eq!(code(&k.decide(&r)), "APPROVAL_UNAVAILABLE");
    // Ordinary reversible writes proceed.
    let d = k.decide(&req(
        "change.apply",
        EffectClass::ReversibleWrite,
        &caps(&["fs.write"]),
        "local_autonomous",
        Some(&l),
    ));
    assert_eq!(code(&d), "ALLOW", "{d:?}");
}

/// QUAL-EV-0091: a project instruction cannot weaken an org deny rule.
#[test]
fn qual_ev_0091_project_allow_cannot_weaken_admin_deny() {
    let k = CapabilityKernel::default();
    let l = lease("local_trusted");
    let mut layers = BTreeMap::new();
    let mut admin = Layer::default();
    admin
        .permissions
        .insert("shell.exec".into(), Permission::Deny);
    let mut project = Layer::default();
    project
        .permissions
        .insert("shell.exec".into(), Permission::Allow);
    layers.insert(Authority::Admin, admin);
    layers.insert(Authority::Project, project);
    let cfg = resolve(&layers);
    assert!(
        !cfg.rejected_widenings.is_empty(),
        "the widening is recorded"
    );
    let sh = caps(&["shell.exec"]);
    let mut r = req(
        "shell.exec",
        EffectClass::ReversibleWrite,
        &sh,
        "local_trusted",
        Some(&l),
    );
    r.config = Some(&cfg);
    let d = k.decide(&r);
    assert_eq!(code(&d), "CAPABILITY_DENIED_BY_CONFIG", "{d:?}");
    // And an envelope-level org deny holds regardless of lease and config.
    let mut env = PolicyEnvelope::default();
    env.denied_capabilities.insert("shell.exec".into());
    let k2 = CapabilityKernel::new(env);
    let d = k2.decide(&req(
        "shell.exec",
        EffectClass::ReversibleWrite,
        &sh,
        "local_trusted",
        Some(&l),
    ));
    assert_eq!(code(&d), "CAPABILITY_DENIED_BY_POLICY", "{d:?}");
}

/// QUAL-EV-0093: workspace trust is not a sandbox grant; a trusted repo still
/// cannot use a denied network/secret capability.
#[test]
fn qual_ev_0093_trusted_workspace_cannot_use_denied_network_or_secret() {
    let k = CapabilityKernel::default();
    let mut l = lease("review_isolated");
    // Suppose a lease over-granted these operations; the profile envelope still denies them.
    l.operations.push("network.egress".into());
    l.operations.push("secret.use".into());
    let net = caps(&["network.egress"]);
    let d = k.decide(&req(
        "http.get",
        EffectClass::ReadOnly,
        &net,
        "review_isolated",
        Some(&l),
    ));
    assert_eq!(code(&d), "CAPABILITY_DENIED_FOR_PROFILE", "{d:?}");
    let sec = caps(&["secret.use"]);
    let d = k.decide(&req(
        "secret.read",
        EffectClass::SecretAccess,
        &sec,
        "review_isolated",
        Some(&l),
    ));
    assert_eq!(code(&d), "CAPABILITY_DENIED_FOR_PROFILE", "{d:?}");
    // Reads and reversible writes in the review worktree are fine.
    let d = k.decide(&req(
        "change.apply",
        EffectClass::ReversibleWrite,
        &caps(&["fs.write"]),
        "review_isolated",
        Some(&l),
    ));
    assert_eq!(code(&d), "ALLOW", "{d:?}");
}

/// docs/23: approval binds intent hash + scope + expiry; changing parameters
/// invalidates it; an emergency stop blocks new effects but not reads.
#[test]
fn approval_binds_intent_and_emergency_stop_blocks_effects() {
    let k = CapabilityKernel::default();
    let l = lease("local_trusted");
    let c = caps(&["git.worktree"]);
    let d = k.decide(&req(
        "git.worktree.close",
        EffectClass::Destructive,
        &c,
        "local_trusted",
        Some(&l),
    ));
    let KernelDecision::ApprovalRequired { scope_json, .. } = &d else {
        panic!("{d:?}");
    };
    assert!(scope_json.contains("\"intent_hash\":\"h1\""));
    let mut a = Approval::create(
        ApprovalId::new(),
        &ApprovalEvent::ApprovalRequested {
            task_id: l.task_id,
            tool_call_id: ToolCallId::new(),
            tool_name: "git.worktree.close".into(),
            effect_class: EffectClass::Destructive,
            intent_hash: "h1".into(),
            scope_json: scope_json.clone(),
            expires_at: Some(Timestamp(100)),
        },
        Timestamp(2),
    )
    .unwrap();
    {
        let mut pending = req(
            "git.worktree.close",
            EffectClass::Destructive,
            &c,
            "local_trusted",
            Some(&l),
        );
        pending.approval = Some(&a);
        assert_eq!(
            code(&k.decide(&pending)),
            "APPROVAL_REQUIRED",
            "still pending"
        );
    }
    a.apply(
        &ApprovalEvent::ApprovalResolved {
            approved: true,
            resolver: "user".into(),
            reason: "ok".into(),
        },
        Timestamp(3),
    )
    .unwrap();
    let mut r = req(
        "git.worktree.close",
        EffectClass::Destructive,
        &c,
        "local_trusted",
        Some(&l),
    );
    r.approval = Some(&a);
    let d = k.decide(&r);
    assert!(
        matches!(&d, KernelDecision::Allow { approval_id: Some(id), .. } if *id == a.approval_id),
        "{d:?}"
    );
    // Changed parameters: different intent hash → the approval does not carry over.
    r.intent_hash = "h2";
    assert_eq!(code(&k.decide(&r)), "APPROVAL_MISMATCH");
    // Expired.
    r.intent_hash = "h1";
    r.now = Timestamp(100);
    assert_eq!(code(&k.decide(&r)), "APPROVAL_MISMATCH");
    // Denied approval is a final deny.
    let mut denied = Approval::create(
        ApprovalId::new(),
        &ApprovalEvent::ApprovalRequested {
            task_id: l.task_id,
            tool_call_id: ToolCallId::new(),
            tool_name: "git.worktree.close".into(),
            effect_class: EffectClass::Destructive,
            intent_hash: "h1".into(),
            scope_json: "{}".into(),
            expires_at: None,
        },
        Timestamp(2),
    )
    .unwrap();
    denied
        .apply(
            &ApprovalEvent::ApprovalResolved {
                approved: false,
                resolver: "user".into(),
                reason: "no".into(),
            },
            Timestamp(3),
        )
        .unwrap();
    r.now = Timestamp(10);
    r.approval = Some(&denied);
    assert_eq!(code(&k.decide(&r)), "APPROVAL_DENIED");
    // Emergency stop.
    let fw = caps(&["fs.write"]);
    let fr = caps(&["fs.read"]);
    let mut stopped = req(
        "change.apply",
        EffectClass::ReversibleWrite,
        &fw,
        "local_trusted",
        Some(&l),
    );
    stopped.emergency_stopped = true;
    assert_eq!(code(&k.decide(&stopped)), "EMERGENCY_STOP");
    let mut read = req(
        "fs.read",
        EffectClass::ReadOnly,
        &fr,
        "local_trusted",
        Some(&l),
    );
    read.emergency_stopped = true;
    assert_eq!(code(&k.decide(&read)), "ALLOW");
}
