//! The report logic of the paired trial (PX-136 / PX-133): synthetic runs, no
//! Core. The real-Core runs against a scripted provider are
//! `services/modbit-core/tests/px_136_paired.rs`.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Duration;

use modbit_bench_context_economics::paired::{
    Accounting, Arm, Binding, GateObservation, PairedConfig, PairedRun, Requirements, StatSample,
    build_registry, builtin_taskset, cascade_plan, live_bindings, load_taskset, missing, reconcile,
    report, rescore, tasks_digest,
};
use modbit_bench_context_economics::spend::Price;
use modbit_bench_context_economics::suite::builtin_suite;

fn binding(endpoint: &str) -> Binding {
    Binding {
        endpoint: endpoint.into(),
        model: "m".into(),
        price: Price {
            input_per_mtok_usd: 1.0,
            output_per_mtok_usd: 2.0,
        },
    }
}

fn cfg() -> PairedConfig {
    PairedConfig {
        cli: PathBuf::new(),
        core: PathBuf::new(),
        work: PathBuf::new(),
        opener: binding("openai"),
        second: binding("anthropic"),
        env: vec![],
        max_turns: 10,
        trial_timeout: Duration::from_secs(1),
        parallel: 1,
        repeats: 1,
        live: true,
        question_answer: String::new(),
    }
}

fn run(task: &str, arm: Arm, ok: bool, cost: f64) -> PairedRun {
    let accounted = (cost / 1e-4).round() as u64;
    PairedRun {
        task: task.into(),
        arm,
        repeat: 0,
        session_id: "s".into(),
        task_id: "t".into(),
        run_id: format!("run-{task}-{}", arm.label()),
        state: "ReadyForReview".into(),
        core_verified: ok,
        oracle_pass: ok,
        check_untouched: true,
        verified_success: ok,
        wall_ms: 10_000,
        cost_usd: Some(cost),
        calls_without_usage: 0,
        unpriced_usage: 0,
        model_calls: 3,
        input_tokens: 1000,
        output_tokens: 100,
        answered_by: BTreeMap::new(),
        escalated: false,
        escalation_attempted: false,
        reviewed: false,
        review_verdicts: vec![],
        revisions: 0,
        gates: vec![GateObservation {
            leg: "final".into(),
            verdict: if ok { "ACCEPT" } else { "REJECT" }.into(),
            oracle_pass: ok,
        }],
        needed_person: !ok,
        interactions: 0,
        accounting: Some(Accounting {
            total_minor: accounted,
            unknown_minor: 0,
            currency: "USD".into(),
            scale: 4,
            path_label: "DIRECT".into(),
            final_outcome: if ok { "pass" } else { "partial" }.into(),
        }),
        stats: vec![],
        stats_version: "none".into(),
        stats_digest: String::new(),
        events_sha256: String::new(),
        events_file: String::new(),
        timed_out: false,
        error: None,
    }
}

fn req(tasks: usize, esc: u32, rev: u32) -> Requirements {
    Requirements {
        tasks,
        repeats: 1,
        min_escalation_samples: esc,
        min_reviewer_samples: rev,
        min_tasks: 30,
    }
}

fn registry() -> modbit_bench_context_economics::paired::RegistryBundle {
    build_registry(&binding("openai"), &binding("anthropic"), [7u8; 32])
}

fn full(tasks: usize) -> Vec<PairedRun> {
    let mut v = Vec::new();
    for i in 0..tasks {
        let t = format!("t{i}");
        // The cascade fixes what direct missed on every fifth task and costs more.
        let direct_ok = i % 5 != 0;
        v.push(run(&t, Arm::Direct, direct_ok, 0.010));
        let mut c = run(
            &t,
            Arm::Cascade,
            true,
            if direct_ok { 0.010 } else { 0.020 },
        );
        c.escalated = !direct_ok;
        c.escalation_attempted = !direct_ok;
        v.push(c);
        let mut k = run(&t, Arm::Critique, direct_ok, 0.016);
        k.reviewed = true;
        v.push(k);
    }
    v
}

#[test]
fn the_report_counts_every_arm_and_every_failed_run_with_intervals() {
    let runs = full(30);
    let reg = registry();
    let rep = report(&runs, &builtin_taskset(), &cfg(), &reg);
    assert_eq!(rep.arms.len(), 3);
    for a in &rep.arms {
        assert_eq!(a.runs, 30);
        assert_eq!(a.verified_success.n, 30);
        assert!(
            a.verified_success.lo <= a.verified_success.p
                && a.verified_success.p <= a.verified_success.hi
        );
        assert_eq!(a.failed_runs.len(), 30 - a.verified_success.k);
    }
    let direct = &rep.arms[0];
    assert_eq!(direct.verified_success.k, 24, "6 of 30 direct runs failed");
    assert_eq!(direct.failed_runs.len(), 6);
    let cascade = rep.arms.iter().find(|a| a.arm == Arm::Cascade).unwrap();
    assert_eq!(cascade.verified_success.k, 30);
    assert_eq!(cascade.escalation_rate.k, 6);
    assert!(cascade.cost_usd.mean > direct.cost_usd.mean);
    // Paired: cascade is better on quality and costs more, or says so.
    let c = rep
        .comparisons
        .iter()
        .find(|c| c.arm == Arm::Cascade)
        .unwrap();
    assert_eq!(c.pairs, 30);
    assert_eq!(c.quality, "BETTER", "{c:?}");
    assert_eq!(c.cost, "MORE", "{c:?}");
    // Critique: the same success as direct, dearer.
    let k = rep
        .comparisons
        .iter()
        .find(|c| c.arm == Arm::Critique)
        .unwrap();
    assert_eq!(k.quality, "NO_DIFFERENCE_DETECTED");
    assert_eq!(k.cost, "MORE");
    // A complete run has nothing missing; the digest reproduces.
    let m = missing(&rep, &runs, &Arm::ALL, &req(30, 0, 0));
    assert!(m.is_empty(), "{m:?}");
    let again = rescore(&runs, &builtin_taskset(), &cfg(), &reg, &rep.digest).unwrap();
    assert_eq!(again.digest, rep.digest);
    assert!(rep.verdict.contains("the default route stays DIRECT"));
}

#[test]
fn a_run_that_drops_a_legs_cost_does_not_reconcile_and_the_run_is_incomplete() {
    let mut runs = full(30);
    // The mutation: a failed leg's cost is left out of the run's cost.
    let k = runs.iter_mut().find(|r| r.arm == Arm::Critique).unwrap();
    assert!(reconcile(k).is_none());
    k.cost_usd = Some(k.cost_usd.unwrap() * 0.5);
    let why = reconcile(k).expect("a halved cost cannot reconcile");
    assert!(why.contains("does not reconcile"), "{why}");
    let rep = report(&runs, &builtin_taskset(), &cfg(), &registry());
    let m = missing(&rep, &runs, &Arm::ALL, &req(30, 0, 0));
    assert!(m.iter().any(|x| x.contains("does not reconcile")), "{m:?}");
}

#[test]
fn a_missing_arm_run_a_short_task_set_and_a_failed_trial_are_each_named() {
    let mut runs = full(30);
    runs.retain(|r| !(r.arm == Arm::Critique && r.task == "t3"));
    runs[0].error = Some("the Core did not start".into());
    let rep = report(&runs, &builtin_taskset(), &cfg(), &registry());
    let m = missing(&rep, &runs, &Arm::ALL, &req(30, 0, 0));
    assert!(
        m.iter()
            .any(|x| x.contains("arm critique has 29 of 30 runs")),
        "{m:?}"
    );
    assert!(
        m.iter().any(|x| x.contains("no valid measurement")),
        "{m:?}"
    );
    let few = missing(&rep, &runs, &Arm::ALL, &req(12, 0, 0));
    assert!(few.iter().any(|x| x.contains("at least 30")), "{few:?}");
    // The arm stays in the report with its failed run counted, not dropped.
    assert_eq!(
        rep.arms
            .iter()
            .find(|a| a.arm == Arm::Direct)
            .unwrap()
            .errored,
        1
    );
}

#[test]
fn required_leg_samples_must_exist_and_be_priced() {
    let mut runs = full(30);
    let rep = report(&runs, &builtin_taskset(), &cfg(), &registry());
    let m = missing(&rep, &runs, &Arm::ALL, &req(30, 1, 1));
    assert!(
        m.iter().any(|x| x.contains("0 escalation samples")),
        "{m:?}"
    );
    assert!(m.iter().any(|x| x.contains("0 reviewer samples")), "{m:?}");
    // An escalation sample with unreported cost is not a priced sample.
    for r in runs.iter_mut().filter(|r| r.escalated) {
        r.stats = vec![StatSample {
            key: "escalation|m|m|acceptance|workspace|configured".into(),
            samples: 1,
            successes: 1,
            cost_known: false,
            unknown_cost_samples: 1,
            mean_cost_minor: 0,
        }];
    }
    for r in runs.iter_mut().filter(|r| r.reviewed) {
        r.stats = vec![StatSample {
            key: "reviewer|m".into(),
            samples: 1,
            successes: 1,
            cost_known: true,
            unknown_cost_samples: 0,
            mean_cost_minor: 40,
        }];
    }
    let rep = report(&runs, &builtin_taskset(), &cfg(), &registry());
    let m = missing(&rep, &runs, &Arm::ALL, &req(30, 1, 1));
    assert!(
        m.iter()
            .any(|x| x.contains("escalation sample has no priced cost")),
        "{m:?}"
    );
    assert!(!m.iter().any(|x| x.contains("reviewer")), "{m:?}");
}

#[test]
fn gate_errors_are_counted_against_the_independent_check_with_intervals() {
    let mut runs = full(30);
    // A false accept: the gate said ACCEPT, the independent check failed.
    runs[0].gates[0] = GateObservation {
        leg: "final".into(),
        verdict: "ACCEPT".into(),
        oracle_pass: false,
    };
    // A false reject: the gate said REJECT on a candidate that passes.
    runs[3].gates[0] = GateObservation {
        leg: "final".into(),
        verdict: "REJECT".into(),
        oracle_pass: true,
    };
    let rep = report(&runs, &builtin_taskset(), &cfg(), &registry());
    let g = &rep.gate["all"];
    assert_eq!(g.observations, 90);
    assert_eq!(g.false_accept.k, 1);
    assert_eq!(g.false_reject.k, 1);
    assert!(g.false_accept.hi > g.false_accept.p && g.false_accept.lo <= g.false_accept.p);
    assert_eq!(g.false_accept.n, g.bad_candidates);
    assert_eq!(g.false_reject.n, g.good_candidates);
}

#[test]
fn the_pin_must_match_the_suite_and_the_cascade_plan_is_the_compiled_plan_plus_a_stronger_slot() {
    let suite = builtin_suite();
    let dir = tempfile::tempdir().unwrap();
    let pin = builtin_taskset();
    let file = dir.path().join("taskset.json");
    std::fs::write(&file, serde_json::to_string(&pin).unwrap()).unwrap();
    let (loaded, tasks) = load_taskset(&file, &suite, 30).unwrap();
    assert_eq!((loaded.tasks.len(), tasks.len()), (30, 30));
    assert_eq!(pin.digest, tasks_digest(&tasks));
    // Too few, a changed digest and an unknown task are each refused.
    assert!(load_taskset(&file, &suite, 31).is_err());
    let mut bad = pin.clone();
    bad.digest = "0".repeat(64);
    std::fs::write(&file, serde_json::to_string(&bad).unwrap()).unwrap();
    assert!(
        load_taskset(&file, &suite, 1)
            .unwrap_err()
            .contains("digest")
    );
    let mut unknown = pin.clone();
    unknown.tasks[0] = "no-such-task".into();
    std::fs::write(&file, serde_json::to_string(&unknown).unwrap()).unwrap();
    assert!(
        load_taskset(&file, &suite, 1)
            .unwrap_err()
            .contains("does not have")
    );
    // The committed pin is this suite.
    let committed: modbit_bench_context_economics::paired::TaskSet = serde_json::from_str(
        &std::fs::read_to_string(
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("paired/taskset.json"),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(committed, pin);
    // A cascade plan is refused when the compiled plan is not a plan.
    assert!(cascade_plan(&serde_json::json!({"nope": 1}), &binding("anthropic"), 1).is_err());
}

#[test]
fn live_mode_is_refused_without_every_part_of_it() {
    let env = |pairs: &[(&str, &str)]| {
        let m: BTreeMap<String, String> = pairs
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect();
        move |k: &str| m.get(k).cloned()
    };
    let key = "sk-live-0123456789abcdef0123456789";
    let full: Vec<(&str, &str)> = vec![
        ("MODBIT_LIVE", "1"),
        ("MODBIT_LIVE_MODEL", "glm"),
        ("OPENAI_API_KEY", key),
        ("ANTHROPIC_API_KEY", key),
        ("MODBIT_OPENAI_BASE_URL", "https://gw.example.com/v4"),
        (
            "MODBIT_ANTHROPIC_BASE_URL",
            "https://gw.example.com/anthropic",
        ),
        ("MODBIT_OPENAI_MODELS", "glm=0.15/0.50"),
        ("MODBIT_ANTHROPIC_MODELS", "glm=0.15/0.50"),
    ];
    let (o, s, e) = live_bindings(&env(&full), None, "anthropic", None).unwrap();
    assert_eq!(
        (o.label().as_str(), s.label().as_str()),
        ("openai/glm", "anthropic/glm")
    );
    assert!(e.contains_key("MODBIT_OPENAI_MODELS") && !format!("{e:?}").contains(key));
    let without = |name: &str| -> Vec<(&str, &str)> {
        full.iter().filter(|(k, _)| *k != name).copied().collect()
    };
    for missing_var in [
        "MODBIT_LIVE",
        "OPENAI_API_KEY",
        "ANTHROPIC_API_KEY",
        "MODBIT_OPENAI_MODELS",
        "MODBIT_LIVE_MODEL",
    ] {
        let err = live_bindings(&env(&without(missing_var)), None, "anthropic", None).unwrap_err();
        assert!(err.starts_with("LIVE: NOT RUN"), "{missing_var}: {err}");
        assert!(!err.contains(key));
    }
    let mut loop_back = full.clone();
    loop_back.retain(|(k, _)| *k != "MODBIT_OPENAI_BASE_URL");
    loop_back.push(("MODBIT_OPENAI_BASE_URL", "http://127.0.0.1:9/v4"));
    assert!(live_bindings(&env(&loop_back), None, "anthropic", None).is_err());
    let mut placeholder = full.clone();
    placeholder.retain(|(k, _)| *k != "OPENAI_API_KEY");
    placeholder.push(("OPENAI_API_KEY", "dummy-key-dummy-key"));
    assert!(live_bindings(&env(&placeholder), None, "anthropic", None).is_err());
    // An unpriced model is refused: an unknown price is not free.
    assert!(live_bindings(&env(&full), Some("other"), "anthropic", None).is_err());
}

#[test]
fn the_paired_binary_refuses_without_a_live_provider_and_writes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("bundle");
    let run = std::process::Command::new(env!("CARGO_BIN_EXE_paired-trial"))
        .args(["--row", "px136", "--max-cost-usd", "5", "--out-dir"])
        .arg(&out)
        .env_remove("MODBIT_LIVE")
        .output()
        .unwrap();
    assert_eq!(run.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&run.stderr).contains("LIVE: NOT RUN"));
    assert!(!out.exists(), "a refused run records nothing");
    // Asked for, with a key, but no cap: still nothing.
    let run = std::process::Command::new(env!("CARGO_BIN_EXE_paired-trial"))
        .args(["--row", "px136", "--out-dir"])
        .arg(&out)
        .env("MODBIT_LIVE", "1")
        .env("MODBIT_LIVE_MODEL", "glm")
        .env("OPENAI_API_KEY", "sk-live-0123456789abcdef0123456789")
        .env("ANTHROPIC_API_KEY", "sk-live-0123456789abcdef0123456789")
        .env("MODBIT_OPENAI_MODELS", "glm=0.15/0.50")
        .env("MODBIT_ANTHROPIC_MODELS", "glm=0.15/0.50")
        .env_remove("MODBIT_LIVE_MAX_COST_USD")
        .output()
        .unwrap();
    assert_eq!(
        run.status.code(),
        Some(2),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert!(String::from_utf8_lossy(&run.stderr).contains("--max-cost-usd"));
    assert!(!out.exists());
}
