//! Capability Kernel qualification: the host boundary decides from registered
//! effect class, lease, envelope, configuration and intent-bound approvals.

use std::collections::BTreeMap;

use modbit_domain::approval::{Approval, ApprovalEvent};
use modbit_domain::lease::{CapabilityLease, CapabilityLeaseEvent};
use modbit_domain::toolcall::EffectClass;
use modbit_domain::{ApprovalId, CapabilityLeaseId, TaskId, TenantId, Timestamp, ToolCallId};
use modbit_policy::{
    Authority, CapabilityKernel, KernelDecision, KernelRequest, Layer, Permission, PolicyEnvelope,
    ResourceSelector, ResourceTarget, default_lease_for_profile, resolve,
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
        targets: &[],
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
    // A capability no default lease grants (deploy) is refused however the
    // tool is named; the trusted profile's forge egress (PX-006) is leased.
    let deploy = caps(&["deploy"]);
    assert_eq!(
        code(&k.decide(&req(
            "cloud.deploy",
            EffectClass::ReadOnly,
            &deploy,
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

/// QUAL-EV-0136: permission modes compile to monotonic policy; a mode switch
/// is a session-lease action, never something a model tool call can request.
#[test]
fn qual_ev_0136_modes_compile_to_monotonic_policy_and_no_tool_switches_them() {
    let k = CapabilityKernel::default();
    // The three shipped modes: each tightens; none loosens the envelope.
    let env = k.envelope();
    let trusted = env.profile_ceilings["local_trusted"];
    let review = env.profile_ceilings["review_isolated"];
    let auto = env.profile_ceilings["local_autonomous"];
    assert!(review <= trusted && auto <= trusted);
    assert!(env.no_approval_profiles.contains("local_autonomous"));
    // Requests name a profile but cannot elevate: a lease for a narrower mode is denied wider effects.
    let l = lease("review_isolated");
    let c = caps(&["git.worktree"]);
    let d = k.decide(&req(
        "git.worktree.close",
        EffectClass::Destructive,
        &c,
        "review_isolated",
        Some(&l),
    ));
    // The narrower mode's default lease does not even carry the capability: denied, never allowed.
    assert!(matches!(d, KernelDecision::Deny { .. }), "{}", code(&d));
    // No capability id exists that changes modes: the kernel denies unknown operations outright.
    let switch = caps(&["policy.mode.switch"]);
    let l2 = lease("local_trusted");
    let d = k.decide(&req(
        "policy.switch_mode",
        EffectClass::ReadOnly,
        &switch,
        "local_trusted",
        Some(&l2),
    ));
    assert_eq!(code(&d), "CAPABILITY_NOT_LEASED");
}

fn target(capability: &str, resource: &str) -> ResourceTarget {
    ResourceTarget {
        capability: capability.into(),
        resource: resource.into(),
    }
}

/// FIX-15: a lease's resource selectors constrain what it covers. A write
/// under the leased root is inside; one outside it, a read with no selector,
/// and a sibling directory that merely shares a prefix are not.
#[test]
fn lease_resource_selectors_constrain_which_resources_a_lease_covers() {
    let k = CapabilityKernel::default();
    let l = lease("local_trusted");
    let fw = caps(&["fs.write"]);
    let decide = |lease: &CapabilityLease, c: &[String], targets: &[ResourceTarget]| {
        let mut r = req(
            "change.apply",
            EffectClass::ReversibleWrite,
            c,
            "local_trusted",
            Some(lease),
        );
        r.targets = targets;
        k.decide(&r)
    };
    // The default lease grants `fs.write:/repo/**`.
    assert_eq!(
        code(&decide(&l, &fw, &[target("fs.write", "/repo/src/lib.rs")])),
        "ALLOW"
    );
    assert_eq!(
        code(&decide(&l, &fw, &[target("fs.write", "/repo")])),
        "ALLOW",
        "the directory a `/**` selector names is covered"
    );
    for outside in ["/etc/passwd", "/repo-evil/x", "/repository/x"] {
        let d = decide(&l, &fw, &[target("fs.write", outside)]);
        assert_eq!(code(&d), "LEASE_RESOURCE_NOT_COVERED", "{outside}: {d:?}");
    }
    // One uncovered target among covered ones refuses the call.
    let d = decide(
        &l,
        &fw,
        &[
            target("fs.write", "/repo/a.txt"),
            target("fs.write", "/tmp/b.txt"),
        ],
    );
    assert_eq!(code(&d), "LEASE_RESOURCE_NOT_COVERED", "{d:?}");
    // A narrowed lease (a subagent's write scope) covers only its scope.
    let mut narrow = l.clone();
    narrow.resources = vec![
        "fs.read:/repo/**".into(),
        "fs.write:/repo/src/a/**".into(),
        "fs.write:/repo/src/a".into(),
    ];
    assert_eq!(
        code(&decide(
            &narrow,
            &fw,
            &[target("fs.write", "/repo/src/a/x.rs")]
        )),
        "ALLOW"
    );
    let d = decide(&narrow, &fw, &[target("fs.write", "/repo/src/b/x.rs")]);
    assert_eq!(code(&d), "LEASE_RESOURCE_NOT_COVERED", "{d:?}");
    // The lease grants the operation but names no resource for it: it covers
    // nothing the host resolved.
    let mut bare = l.clone();
    bare.resources.retain(|r| !r.starts_with("fs.write:"));
    let d = decide(&bare, &fw, &[target("fs.write", "/repo/a.txt")]);
    assert_eq!(code(&d), "LEASE_RESOURCE_NOT_COVERED", "{d:?}");
    // Without a resolved target the selectors do not apply (the owning
    // boundary judges the resource); the operation check still does.
    assert_eq!(code(&decide(&bare, &fw, &[])), "ALLOW");
    // Windows separators compare as `/`.
    let mut win = l.clone();
    win.resources = vec![r"fs.write:C:\repo/**".into()];
    assert_eq!(
        code(&decide(
            &win,
            &fw,
            &[target("fs.write", r"C:\repo\src\a.rs")]
        )),
        "ALLOW"
    );
}

/// FIX-15: a selector that is present but unreadable fails closed — the whole
/// lease grants nothing, even for a call that names no resource.
#[test]
fn an_unreadable_lease_selector_fails_closed() {
    let k = CapabilityKernel::default();
    let fr = caps(&["fs.read"]);
    for bad in [
        "",
        "fs.read",
        "fs.read:",
        ":/repo/**",
        "fs read:/x",
        "fs.read:/a\0b",
    ] {
        let mut l = lease("local_trusted");
        l.resources.push(bad.into());
        let d = k.decide(&req(
            "fs.read",
            EffectClass::ReadOnly,
            &fr,
            "local_trusted",
            Some(&l),
        ));
        assert_eq!(code(&d), "LEASE_RESOURCE_MALFORMED", "{bad:?}: {d:?}");
    }
    assert!(ResourceSelector::parse("network.egress:forge-api:443").is_some());
    assert!(ResourceSelector::parse("secret.use:forge-token -> forge-api").is_some());
    assert!(ResourceSelector::parse(r"fs.write:C:\repo/**").is_some());
    // Glob edges: `*` stays within a segment, `**` does not, `?` is one char.
    let sel = |p: &'static str| ResourceSelector::parse(p).unwrap();
    assert!(sel("fs.read:/a/*.rs").covers("/a/x.rs"));
    assert!(!sel("fs.read:/a/*.rs").covers("/a/b/x.rs"));
    assert!(sel("fs.read:/a/**/x.rs").covers("/a/b/c/x.rs"));
    assert!(sel("fs.read:/a/?.rs").covers("/a/x.rs"));
    assert!(!sel("fs.read:/a/?.rs").covers("/a//.rs"));
    assert!(!sel("fs.read:/a/**").covers("/ab"));
}

/// FIX-15: an approval authorizes one execution. Once the bound call has been
/// dispatched under it, the same approval — still approved, still unexpired,
/// still bound to the same intent — no longer authorizes: the kernel asks
/// again instead of letting the intent run a second time.
#[test]
fn a_consumed_approval_asks_again_instead_of_authorizing_a_second_dispatch() {
    let k = CapabilityKernel::default();
    let l = lease("local_trusted");
    let c = caps(&["git.worktree"]);
    let mut a = Approval::create(
        ApprovalId::new(),
        &ApprovalEvent::ApprovalRequested {
            task_id: l.task_id,
            tool_call_id: ToolCallId::new(),
            tool_name: "git.worktree.close".into(),
            effect_class: EffectClass::Destructive,
            intent_hash: "h1".into(),
            scope_json: "{}".into(),
            expires_at: Some(Timestamp(100)),
        },
        Timestamp(2),
    )
    .unwrap();
    a.apply(
        &ApprovalEvent::ApprovalResolved {
            approved: true,
            resolver: "user".into(),
            reason: "ok".into(),
        },
        Timestamp(3),
    )
    .unwrap();
    let decide = |a: &Approval| {
        let mut r = req(
            "git.worktree.close",
            EffectClass::Destructive,
            &c,
            "local_trusted",
            Some(&l),
        );
        r.approval = Some(a);
        k.decide(&r)
    };
    assert_eq!(code(&decide(&a)), "ALLOW", "first use");
    a.consumed_at = Some(Timestamp(5));
    let d = decide(&a);
    let KernelDecision::ApprovalRequired { reason, scope_json } = &d else {
        panic!("a spent approval must ask again, got {d:?}");
    };
    assert!(reason.contains("already used"), "{reason}");
    assert!(
        scope_json.contains(&a.approval_id.to_string()),
        "the new request names the approval it supersedes: {scope_json}"
    );
}

/// A root written with a trailing slash (`/repo/` gives the selector
/// `/repo//**`) and a target joined from it spell the same place.
#[test]
fn selectors_and_targets_compare_with_repeated_separators_collapsed() {
    let sel = ResourceSelector::parse("fs.write:/repo//**").unwrap();
    assert!(sel.covers("/repo/src/a.rs"));
    assert!(sel.covers("/repo//src//a.rs"));
    assert!(sel.covers("/repo"));
    assert!(!sel.covers("/repo2/a.rs"));
}
