//! PX-057 qualification at the kernel: a run mode answers only the approval
//! question, never widens the envelope, and never overrides a person's denial;
//! the always-ask classes ask in the widest mode; an allowing rule is named.

use modbit_domain::approval::{Approval, ApprovalEvent};
use modbit_domain::lease::{CapabilityLease, CapabilityLeaseEvent};
use modbit_domain::mode::TaskMode;
use modbit_domain::runmode::{AllowRule, AskClass, RuleScope, RunMode};
use modbit_domain::toolcall::EffectClass;
use modbit_domain::{ApprovalId, CapabilityLeaseId, TaskId, TenantId, Timestamp, ToolCallId};
use modbit_policy::{
    CapabilityKernel, KernelDecision, KernelRequest, PolicyEnvelope, RunPolicy,
    default_lease_for_profile,
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
    effect: EffectClass,
    caps: &'a [String],
    profile: &'a str,
    lease: Option<&'a CapabilityLease>,
    run: Option<&'a RunPolicy>,
) -> KernelRequest<'a> {
    KernelRequest {
        tool_name: "shell.exec",
        effect_class: effect,
        required_capabilities: caps,
        execution_profile: profile,
        mode: TaskMode::Agent,
        lease,
        targets: &[],
        approval: None,
        intent_hash: "h1",
        config: None,
        run,
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

fn rule(id: &str, pattern: &[&str], scope: RuleScope, key: &str) -> AllowRule {
    AllowRule {
        rule_id: id.into(),
        pattern: pattern.iter().map(|s| (*s).to_owned()).collect(),
        scope,
        scope_key: key.into(),
        created_by: "user:u".into(),
        created_at_ms: 1,
        expires_at_ms: None,
        covers_always_ask: false,
        origin_task: String::new(),
    }
}

fn policy(mode: RunMode, rules: Vec<AllowRule>, argv: &[&str]) -> RunPolicy {
    RunPolicy {
        mode,
        task_id: "t1".into(),
        repo_root: "/repo".into(),
        rules,
        argv: Some(argv.iter().map(|s| (*s).to_owned()).collect()),
        ask_classes: vec![],
        contained: false,
    }
}

#[test]
fn a_run_mode_only_answers_the_approval_question_and_never_widens_the_envelope() {
    let k = CapabilityKernel::default();
    let c = caps(&["shell.exec"]);
    let l = lease("local_trusted");
    let widest = policy(RunMode::RunEverything, vec![], &["tool"]);
    // A protected effect that would ask is approved by the widest mode...
    assert_eq!(
        code(&k.decide(&req(
            EffectClass::ProtectedWrite,
            &c,
            "local_trusted",
            Some(&l),
            None
        ))),
        "APPROVAL_REQUIRED"
    );
    assert_eq!(
        code(&k.decide(&req(
            EffectClass::ProtectedWrite,
            &c,
            "local_trusted",
            Some(&l),
            Some(&widest)
        ))),
        "ALLOW"
    );
    // ...but a capability the lease does not grant, a profile ceiling, a
    // denied capability, an unattended profile and an emergency stop are all
    // decided before the mode is asked.
    let no_cap = caps(&["deploy"]);
    assert_eq!(
        code(&k.decide(&req(
            EffectClass::ProtectedWrite,
            &no_cap,
            "local_trusted",
            Some(&l),
            Some(&widest)
        ))),
        "CAPABILITY_NOT_LEASED"
    );
    let auto = lease("local_autonomous");
    assert_eq!(
        code(&k.decide(&req(
            EffectClass::ProtectedWrite,
            &c,
            "local_autonomous",
            Some(&auto),
            Some(&widest)
        ))),
        "PROFILE_CEILING",
        "no mode lifts an effect ceiling"
    );
    let mut stopped = req(
        EffectClass::ProtectedWrite,
        &c,
        "local_trusted",
        Some(&l),
        Some(&widest),
    );
    stopped.emergency_stopped = true;
    assert_eq!(code(&k.decide(&stopped)), "EMERGENCY_STOP");
    let mut denied_by_org = PolicyEnvelope::default();
    denied_by_org
        .denied_capabilities
        .insert("shell.exec".into());
    let strict = CapabilityKernel::new(denied_by_org);
    assert_eq!(
        code(&strict.decide(&req(
            EffectClass::ProtectedWrite,
            &c,
            "local_trusted",
            Some(&l),
            Some(&widest)
        ))),
        "CAPABILITY_DENIED_BY_POLICY"
    );
    // A task mode's posture narrows first, whatever the run mode.
    let mut ask_mode = req(
        EffectClass::ProtectedWrite,
        &c,
        "local_trusted",
        Some(&l),
        Some(&widest),
    );
    ask_mode.mode = TaskMode::Ask;
    assert_eq!(code(&k.decide(&ask_mode)), "MODE_POSTURE");
}

#[test]
fn an_allowing_rule_is_named_on_the_decision_and_ask_mode_never_approves() {
    let k = CapabilityKernel::default();
    let c = caps(&["shell.exec"]);
    let l = lease("local_trusted");
    let list = policy(
        RunMode::Allowlist,
        vec![rule("rule-7", &["make", "check"], RuleScope::Task, "t1")],
        &["make", "check", "-j4"],
    );
    match k.decide(&req(
        EffectClass::ProtectedWrite,
        &c,
        "local_trusted",
        Some(&l),
        Some(&list),
    )) {
        KernelDecision::Allow { rule, approval_id } => {
            assert_eq!(rule, "allowlist:rule-7");
            assert!(approval_id.is_none());
        }
        other => panic!("{other:?}"),
    }
    // The same rule under ASK is never consulted.
    let mut ask = list.clone();
    ask.mode = RunMode::Ask;
    let d = k.decide(&req(
        EffectClass::ProtectedWrite,
        &c,
        "local_trusted",
        Some(&l),
        Some(&ask),
    ));
    let KernelDecision::ApprovalRequired { scope_json, .. } = d else {
        panic!("{d:?}");
    };
    let scope: serde_json::Value = serde_json::from_str(&scope_json).unwrap();
    assert_eq!(scope["ask_reason"], "MODE_ASK");
    assert_eq!(scope["run_mode"], "ASK");
    // A command outside the rule asks, and says it is not in the allowlist.
    let mut other = list.clone();
    other.argv = Some(vec!["make".into(), "install".into()]);
    let d = k.decide(&req(
        EffectClass::ProtectedWrite,
        &c,
        "local_trusted",
        Some(&l),
        Some(&other),
    ));
    let KernelDecision::ApprovalRequired { scope_json, .. } = d else {
        panic!("{d:?}");
    };
    let scope: serde_json::Value = serde_json::from_str(&scope_json).unwrap();
    assert_eq!(scope["ask_reason"], "NOT_IN_ALLOWLIST");
}

#[test]
fn always_ask_classes_ask_under_the_widest_mode() {
    let k = CapabilityKernel::default();
    let c = caps(&["shell.exec"]);
    let l = lease("local_trusted");
    for (class, effect) in [
        (AskClass::Network, EffectClass::ExternalSideEffect),
        (AskClass::Deletion, EffectClass::Destructive),
        (AskClass::Secret, EffectClass::SecretAccess),
        (AskClass::OutsideWorkspaceWrite, EffectClass::ProtectedWrite),
        (AskClass::Push, EffectClass::ExternalSideEffect),
        (AskClass::ProtectedPath, EffectClass::ProtectedWrite),
        (AskClass::Escalation, EffectClass::ProtectedWrite),
    ] {
        let mut p = policy(RunMode::RunEverything, vec![], &["tool"]);
        p.ask_classes = vec![class];
        p.contained = true;
        match k.decide(&req(effect, &c, "local_trusted", Some(&l), Some(&p))) {
            KernelDecision::ApprovalRequired { reason, scope_json } => {
                let scope: serde_json::Value = serde_json::from_str(&scope_json).unwrap();
                assert_eq!(scope["run_mode"], "RUN_EVERYTHING");
                assert_eq!(scope["ask_reason"], format!("ALWAYS_ASK:{}", class.name()));
                assert_eq!(scope["ask_classes"][0], class.name());
                assert!(reason.contains("ALWAYS_ASK"), "{reason}");
            }
            other => panic!("{class:?}: {other:?}"),
        }
    }
}

fn approval(l: &CapabilityLease, approved: bool, intent: &str) -> Approval {
    let mut a = Approval::create(
        ApprovalId::new(),
        &ApprovalEvent::ApprovalRequested {
            task_id: l.task_id,
            tool_call_id: ToolCallId::new(),
            tool_name: "shell.exec".into(),
            effect_class: EffectClass::ProtectedWrite,
            intent_hash: intent.into(),
            scope_json: "{}".into(),
            expires_at: None,
        },
        Timestamp(2),
    )
    .unwrap();
    a.apply(
        &ApprovalEvent::ApprovalResolved {
            approved,
            resolver: "user".into(),
            reason: String::new(),
        },
        Timestamp(3),
    )
    .unwrap();
    a
}

#[test]
fn a_persons_denial_and_a_mismatched_approval_are_not_overridden_by_a_mode() {
    let k = CapabilityKernel::default();
    let c = caps(&["shell.exec"]);
    let l = lease("local_trusted");
    let widest = policy(RunMode::RunEverything, vec![], &["tool"]);
    let denied = approval(&l, false, "h1");
    let mut r = req(
        EffectClass::ProtectedWrite,
        &c,
        "local_trusted",
        Some(&l),
        Some(&widest),
    );
    r.approval = Some(&denied);
    assert_eq!(code(&k.decide(&r)), "APPROVAL_DENIED");
    let other_intent = approval(&l, true, "h2");
    r.approval = Some(&other_intent);
    assert_eq!(code(&k.decide(&r)), "APPROVAL_MISMATCH");
}

#[test]
fn a_spent_approval_asks_again_unless_the_mode_approves_the_same_call() {
    let k = CapabilityKernel::default();
    let c = caps(&["shell.exec"]);
    let l = lease("local_trusted");
    let mut spent = approval(&l, true, "h1");
    spent.consumed_at = Some(Timestamp(5));
    let mut r = req(
        EffectClass::ProtectedWrite,
        &c,
        "local_trusted",
        Some(&l),
        None,
    );
    r.approval = Some(&spent);
    assert_eq!(code(&k.decide(&r)), "APPROVAL_REQUIRED");
    let widest = policy(RunMode::RunEverything, vec![], &["tool"]);
    r.run = Some(&widest);
    assert_eq!(code(&k.decide(&r)), "ALLOW");
}

/// PX-070 (CUC-C01): computer control asks in every mode, with or without a rule, whatever the
/// envelope says about approvals; an unattended profile cannot ask, so it cannot have it.
#[test]
fn computer_control_asks_in_every_run_mode_and_no_envelope_setting_removes_the_ask() {
    let c = caps(&["computer.act"]);
    let l = lease("local_trusted");
    let widest = policy(RunMode::RunEverything, vec![], &["computer.press"]);
    let ruled = policy(
        RunMode::Allowlist,
        vec![AllowRule {
            covers_always_ask: true,
            ..rule("r1", &["computer.press"], RuleScope::Task, "t1")
        }],
        &["computer.press"],
    );
    let k = CapabilityKernel::default();
    for p in [None, Some(&widest), Some(&ruled)] {
        let d = k.decide(&req(
            EffectClass::ExternalSideEffect,
            &c,
            "local_trusted",
            Some(&l),
            p,
        ));
        assert_eq!(code(&d), "APPROVAL_REQUIRED", "{p:?}");
        let KernelDecision::ApprovalRequired { scope_json, .. } = d else {
            unreachable!()
        };
        let scope: serde_json::Value = serde_json::from_str(&scope_json).unwrap();
        assert_eq!(scope["allowlistable"], false);
        assert_eq!(
            scope["ask_classes"],
            serde_json::json!(["COMPUTER_CONTROL"])
        );
    }
    // An envelope that no longer lists external side effects among the effects that ask changes
    // nothing for the capability: it asks.
    let mut env = PolicyEnvelope::default();
    env.approval_effects.clear();
    let lax = CapabilityKernel::new(env);
    assert_eq!(
        code(&lax.decide(&req(
            EffectClass::ExternalSideEffect,
            &c,
            "local_trusted",
            Some(&l),
            Some(&widest)
        ))),
        "APPROVAL_REQUIRED"
    );
    // Observation is a read: no approval, and no mode is consulted.
    let observe = caps(&["computer.observe"]);
    assert_eq!(
        code(&k.decide(&req(
            EffectClass::ReadOnly,
            &observe,
            "local_trusted",
            Some(&l),
            None
        ))),
        "ALLOW"
    );
    // An unattended profile cannot ask, and its lease does not carry the capability.
    let auto = lease("local_autonomous");
    assert_eq!(
        code(&k.decide(&req(
            EffectClass::ExternalSideEffect,
            &c,
            "local_autonomous",
            Some(&auto),
            Some(&widest)
        ))),
        "CAPABILITY_NOT_LEASED"
    );
    // A read-only mode carries no computer capability at all.
    let mut ask_mode = req(
        EffectClass::ReadOnly,
        &observe,
        "local_trusted",
        Some(&l),
        None,
    );
    ask_mode.mode = TaskMode::Ask;
    assert_eq!(code(&k.decide(&ask_mode)), "MODE_POSTURE");
    // The policy may deny any of the three capabilities.
    let mut layers = std::collections::BTreeMap::new();
    layers.insert(
        modbit_policy::Authority::Admin,
        modbit_policy::Layer {
            permissions: [(
                "computer.screen".to_owned(),
                modbit_policy::Permission::Deny,
            )]
            .into_iter()
            .collect(),
            ..Default::default()
        },
    );
    let cfg = modbit_policy::resolve(&layers);
    let screen = caps(&["computer.act", "computer.screen"]);
    let mut denied = req(
        EffectClass::ExternalSideEffect,
        &screen,
        "local_trusted",
        Some(&l),
        None,
    );
    denied.config = Some(&cfg);
    assert_eq!(code(&k.decide(&denied)), "CAPABILITY_DENIED_BY_CONFIG");
}

/// PX-070: a durable rule can never name a computer tool.
#[test]
fn no_allowlist_rule_can_name_a_computer_tool() {
    for pattern in [
        vec!["computer.press"],
        vec!["computer"],
        vec!["x", "computer.type"],
        vec!["COMPUTER.CLICK"],
    ] {
        let r = rule("r", &pattern, RuleScope::Task, "t1");
        let e = modbit_policy::runmode::validate_rule(&r).unwrap_err();
        assert_eq!(
            e.code,
            modbit_policy::runmode::COMPUTER_NOT_ALLOWLISTABLE,
            "{pattern:?}"
        );
    }
    assert!(
        modbit_policy::runmode::validate_rule(&rule(
            "r",
            &["cargo", "test"],
            RuleScope::Task,
            "t1"
        ))
        .is_ok()
    );
}
