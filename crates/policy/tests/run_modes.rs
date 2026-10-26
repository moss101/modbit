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
        program_path: format!("/bin/{}", pattern[0]),
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
        // Every program the command names is the one the rules pinned.
        programs: argv
            .iter()
            .take(1)
            .map(|t| ((*t).to_owned(), Some(format!("/bin/{t}"))))
            .collect(),
        ask_classes: vec![],
        dependency_install: false,
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

/// DR-PX-2026-10-25-017: `AUTONOMOUS` is the one mode that lets the install of
/// a project's declared dependencies pass the NETWORK class; it approves
/// nothing else that asks in every mode, and widens no part of the envelope.
#[test]
fn autonomous_approves_a_manifest_install_and_nothing_else_that_always_asks() {
    let k = CapabilityKernel::default();
    let c = caps(&["shell.exec"]);
    let l = lease("local_trusted");
    let install = |mode: RunMode, classes: Vec<AskClass>, dependency_install: bool| RunPolicy {
        ask_classes: classes,
        dependency_install,
        ..policy(mode, vec![], &["npm", "ci"])
    };
    let decide = |p: &RunPolicy| {
        code(&k.decide(&req(
            EffectClass::ExternalSideEffect,
            &c,
            "local_trusted",
            Some(&l),
            Some(p),
        )))
    };
    // The install passes in AUTONOMOUS, and the receipt names why.
    let p = install(RunMode::Autonomous, vec![AskClass::Network], true);
    match k.decide(&req(
        EffectClass::ExternalSideEffect,
        &c,
        "local_trusted",
        Some(&l),
        Some(&p),
    )) {
        KernelDecision::Allow { rule, .. } => {
            assert_eq!(rule, "run_mode:AUTONOMOUS:dependency_install");
        }
        other => panic!("{other:?}"),
    }
    // The same call asks in every other mode, whatever the flag says.
    for mode in [
        RunMode::Ask,
        RunMode::Allowlist,
        RunMode::AllowlistSandbox,
        RunMode::RunEverything,
    ] {
        let p = install(mode, vec![AskClass::Network], true);
        assert_eq!(decide(&p), "APPROVAL_REQUIRED", "{mode:?}");
    }
    // A network reach that is not a manifest install asks in AUTONOMOUS too.
    let p = install(RunMode::Autonomous, vec![AskClass::Network], false);
    assert_eq!(decide(&p), "APPROVAL_REQUIRED");
    // Another always-ask class on the same call still asks.
    for other in [
        AskClass::Secret,
        AskClass::Deletion,
        AskClass::Push,
        AskClass::OutsideWorkspaceWrite,
        AskClass::ProtectedPath,
        AskClass::Escalation,
    ] {
        let p = install(RunMode::Autonomous, vec![AskClass::Network, other], true);
        assert_eq!(decide(&p), "APPROVAL_REQUIRED", "{other:?}");
    }
    // It approves a protected write outside the always-ask classes like
    // RUN_EVERYTHING does (an unknown project script), but only inside the
    // envelope: a missing capability, a ceiling and a stop are decided first.
    let script = RunPolicy {
        argv: Some(vec!["./tools/gen".into()]),
        ..policy(RunMode::Autonomous, vec![], &[])
    };
    assert_eq!(
        code(&k.decide(&req(
            EffectClass::ProtectedWrite,
            &c,
            "local_trusted",
            Some(&l),
            Some(&script)
        ))),
        "ALLOW"
    );
    let no_cap = caps(&["deploy"]);
    assert_eq!(
        code(&k.decide(&req(
            EffectClass::ProtectedWrite,
            &no_cap,
            "local_trusted",
            Some(&l),
            Some(&script)
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
            Some(&script)
        ))),
        "PROFILE_CEILING"
    );
    let mut stopped = req(
        EffectClass::ProtectedWrite,
        &c,
        "local_trusted",
        Some(&l),
        Some(&script),
    );
    stopped.emergency_stopped = true;
    assert_eq!(code(&k.decide(&stopped)), "EMERGENCY_STOP");
    // A destructive effect (a deletion) asks in AUTONOMOUS as everywhere.
    let del = RunPolicy {
        ask_classes: vec![AskClass::Deletion],
        ..policy(RunMode::Autonomous, vec![], &["rm", "-rf", "dist"])
    };
    assert_eq!(
        code(&k.decide(&req(
            EffectClass::Destructive,
            &c,
            "local_trusted",
            Some(&l),
            Some(&del)
        ))),
        "APPROVAL_REQUIRED"
    );
}

#[test]
fn autonomous_is_durable_and_approves_more_than_every_other_mode() {
    assert!(RunMode::Autonomous.durable());
    assert!(!RunMode::RunEverything.durable());
    assert_eq!(RunMode::parse("AUTONOMOUS"), Some(RunMode::Autonomous));
    let ranks: Vec<u8> = RunMode::ALL.iter().map(|m| m.rank()).collect();
    assert!(ranks.windows(2).all(|w| w[0] < w[1]), "{ranks:?}");
    assert_eq!(RunMode::ALL.last(), Some(&RunMode::Autonomous));
}

// ---- PX-149: allow rules pinned to the resolved program path ----

fn ask_reason(d: &KernelDecision) -> String {
    let KernelDecision::ApprovalRequired { scope_json, .. } = d else {
        panic!("expected an approval to be required, got {}", code(d));
    };
    let scope: serde_json::Value = serde_json::from_str(scope_json).unwrap();
    scope["ask_reason"].as_str().unwrap_or_default().to_owned()
}

fn executable(path: &std::path::Path) {
    use std::os::unix::fs::PermissionsExt;
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, "#!/bin/sh\nexit 0\n").unwrap();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
}

/// A rule made for `git` while it resolved to `pinned`, decided against `now`.
fn decide_git(pinned: &str, now: Option<&str>, argv: &[&str]) -> KernelDecision {
    let k = CapabilityKernel::default();
    let c = caps(&["shell.exec"]);
    let l = lease("local_trusted");
    let mut r = rule("r1", &["git", "status"], RuleScope::Task, "t1");
    r.program_path = pinned.to_owned();
    let mut p = policy(RunMode::Allowlist, vec![r], argv);
    p.programs = std::iter::once((argv[0].to_owned(), now.map(str::to_owned))).collect();
    k.decide(&req(
        EffectClass::ProtectedWrite,
        &c,
        "local_trusted",
        Some(&l),
        Some(&p),
    ))
}

#[test]
fn qual_px_149_a_rule_covers_a_command_only_while_its_program_resolves_to_the_pinned_path() {
    // The system program the rule was made for still allows.
    let d = decide_git("/usr/bin/git", Some("/usr/bin/git"), &["git", "status"]);
    assert_eq!(
        code(&d),
        "ALLOW",
        "the real program still satisfies the rule"
    );
    // A program placed earlier on the path does not, and asks with its own code.
    let d = decide_git("/usr/bin/git", Some("/repo/bin/git"), &["git", "status"]);
    assert_eq!(ask_reason(&d), "PROGRAM_PATH_CHANGED");
    // A rule made before the field existed records no path and asks.
    let d = decide_git("", Some("/usr/bin/git"), &["git", "status"]);
    assert_eq!(ask_reason(&d), "RULE_NOT_PINNED");
    // A program that resolves to nothing fails closed.
    let d = decide_git("/usr/bin/git", None, &["git", "status"]);
    assert_eq!(ask_reason(&d), "PROGRAM_NOT_FOUND");
    // A command the rule's pattern does not match is still plainly outside the allowlist.
    let d = decide_git("/usr/bin/git", Some("/usr/bin/git"), &["git", "push"]);
    assert_eq!(ask_reason(&d), "NOT_IN_ALLOWLIST");
}

#[test]
fn qual_px_149_a_pinned_rule_that_does_not_hold_does_not_widen_what_the_mode_would_do() {
    // The widest mode approves whatever is not always-ask without consulting
    // rules, so a failed pin costs nothing there; an always-ask class still asks.
    let k = CapabilityKernel::default();
    let c = caps(&["shell.exec"]);
    let l = lease("local_trusted");
    let mut r = rule("r1", &["git", "status"], RuleScope::Task, "t1");
    r.program_path = "/usr/bin/git".into();
    let mut p = policy(RunMode::RunEverything, vec![r], &["git", "status"]);
    p.programs = std::iter::once(("git".to_owned(), Some("/repo/bin/git".to_owned()))).collect();
    let d = k.decide(&req(
        EffectClass::ProtectedWrite,
        &c,
        "local_trusted",
        Some(&l),
        Some(&p),
    ));
    assert_eq!(code(&d), "ALLOW");
}

#[test]
fn qual_px_149_the_shell_that_wraps_a_script_cannot_be_a_file_in_the_workspace() {
    let k = CapabilityKernel::default();
    let c = caps(&["shell.exec"]);
    let l = lease("local_trusted");
    let argv = ["sh", "-c", "git status"];
    for (sh, expected) in [("/bin/sh", "ALLOW"), ("/repo/bin/sh", "APPROVAL_REQUIRED")] {
        let mut r = rule("r1", &["git", "status"], RuleScope::Task, "t1");
        r.program_path = "/usr/bin/git".into();
        let mut p = policy(RunMode::Allowlist, vec![r], &argv);
        p.programs = [
            ("sh".to_owned(), Some(sh.to_owned())),
            ("git".to_owned(), Some("/usr/bin/git".to_owned())),
        ]
        .into_iter()
        .collect();
        let d = k.decide(&req(
            EffectClass::ProtectedWrite,
            &c,
            "local_trusted",
            Some(&l),
            Some(&p),
        ));
        assert_eq!(code(&d), expected, "{sh}");
        if expected == "APPROVAL_REQUIRED" {
            assert_eq!(ask_reason(&d), "PROGRAM_IN_WORKSPACE");
        }
    }
}

/// The resolution the host does against a real repository and real search
/// paths feeds the same decision: the shadowing case from the qualification.
#[cfg(unix)]
#[test]
fn qual_px_149_a_repository_bin_first_on_the_path_does_not_satisfy_a_rule_made_for_the_system_program()
 {
    let dir = tempfile::tempdir().unwrap();
    let system = dir.path().join("system/bin");
    let repo = dir.path().join("repo");
    executable(&system.join("git"));
    executable(&repo.join("bin/git"));
    let sys_git = system.join("git").to_string_lossy().into_owned();
    let cwd = repo.to_string_lossy().into_owned();
    let path_system = system.to_string_lossy().into_owned();
    let path_shadowed = format!("{}:{path_system}", repo.join("bin").to_string_lossy());
    let argv: Vec<String> = ["git", "status"].iter().map(|s| (*s).to_owned()).collect();

    // At rule-creation time the program resolved to the system git.
    let made = modbit_policy::runmode::resolve_program("git", &cwd, &path_system).unwrap();
    assert_eq!(made, sys_git);

    let decide_with = |path: &str| {
        let mut r = rule("r1", &["git", "status"], RuleScope::Task, "t1");
        r.program_path = made.clone();
        let mut p = policy(RunMode::Allowlist, vec![r], &["git", "status"]);
        p.programs = modbit_policy::runmode::resolve_programs(&argv, &cwd, path);
        CapabilityKernel::default().decide(&req(
            EffectClass::ProtectedWrite,
            &caps(&["shell.exec"]),
            "local_trusted",
            Some(&lease("local_trusted")),
            Some(&p),
        ))
    };
    assert_eq!(code(&decide_with(&path_system)), "ALLOW");
    let d = decide_with(&path_shadowed);
    assert_eq!(ask_reason(&d), "PROGRAM_PATH_CHANGED", "{d:?}");
    // A directory with no git at all: fail closed.
    let empty = dir.path().join("empty");
    std::fs::create_dir_all(&empty).unwrap();
    let d = decide_with(&empty.to_string_lossy());
    assert_eq!(ask_reason(&d), "PROGRAM_NOT_FOUND");
}

#[cfg(unix)]
#[test]
fn qual_px_149_resolution_follows_the_search_path_the_way_the_process_will() {
    use modbit_policy::runmode::resolve_program;
    let dir = tempfile::tempdir().unwrap();
    let a = dir.path().join("a");
    let b = dir.path().join("b");
    executable(&a.join("tool"));
    executable(&b.join("tool"));
    executable(&b.join("only-b"));
    // A file without the execute bit is not a program.
    std::fs::write(a.join("data"), "x").unwrap();
    let cwd = dir.path().to_string_lossy().into_owned();
    let path = format!("{}:{}", a.to_string_lossy(), b.to_string_lossy());
    assert_eq!(
        resolve_program("tool", &cwd, &path),
        Some(a.join("tool").to_string_lossy().into_owned()),
        "the first entry wins"
    );
    assert_eq!(
        resolve_program("only-b", &cwd, &path),
        Some(b.join("only-b").to_string_lossy().into_owned())
    );
    assert_eq!(resolve_program("data", &cwd, &path), None, "not executable");
    assert_eq!(resolve_program("missing", &cwd, &path), None);
    assert_eq!(resolve_program("", &cwd, &path), None);
    // A token with a separator is taken against the working directory.
    assert_eq!(
        resolve_program("./a/tool", &cwd, "/nonexistent"),
        Some(a.join("tool").to_string_lossy().into_owned())
    );
    assert_eq!(
        resolve_program("a/../b/tool", &cwd, "/nonexistent"),
        Some(b.join("tool").to_string_lossy().into_owned()),
        "lexically cleaned"
    );
    // A relative entry on the path is read against the working directory.
    assert_eq!(
        resolve_program("tool", &cwd, "b"),
        Some(b.join("tool").to_string_lossy().into_owned())
    );
    // A symlink keeps its own path (the pin survives a package upgrade).
    std::os::unix::fs::symlink(b.join("tool"), a.join("linked")).unwrap();
    assert_eq!(
        resolve_program("linked", &cwd, &a.to_string_lossy()),
        Some(a.join("linked").to_string_lossy().into_owned())
    );
}
