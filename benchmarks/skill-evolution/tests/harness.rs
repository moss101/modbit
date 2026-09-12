//! WSK-E2E-008 (paired benchmark), WSK-E2E-009 (cross-model transfer) and
//! the REQ-EV-0207 ablation on fixture trials: the harness reports per
//! arm and per model with intervals, never promotes on an aggregate over a
//! failed gate, and says what it does not establish.

use modbit_bench_context_economics::Metric;
use modbit_bench_skill_evolution::{ablation, arms_report, significant_saving, transfer_report};
use modbit_skills::evolution::{QualificationTrial, Qualifier, SkillCandidate};

fn candidate() -> SkillCandidate {
    SkillCandidate {
        candidate_id: "c".repeat(64),
        base_name: "repair-style".into(),
        base_version: "1.0.0".into(),
        base_content_hash: "b".repeat(64),
        patch: modbit_skills::evolution::AtomicPatch {
            file: "SKILL.md".into(),
            before_hash: "a".repeat(64),
            after: "---\nname: repair-style\nversion: 1.0.1\ndescription: d\n---\n".into(),
        },
        purpose: "PURPOSE: test".into(),
        motivating_patterns: vec![],
        motivating_traces: vec![],
        expected_behavior_change: "x".into(),
        required_tools: vec![],
        capability_ceiling: vec![],
        target_task_classes: vec![],
        proposer_config: "template".into(),
        created_at_ms: 1,
        proposed_version: "1.0.1".into(),
    }
}

#[allow(clippy::too_many_arguments)]
fn trial(
    arm: &str,
    task: &str,
    class: &str,
    model: &str,
    repeat: u32,
    verified: bool,
    tokens: u64,
    safety: u32,
) -> QualificationTrial {
    QualificationTrial {
        task: task.into(),
        task_class: class.into(),
        model: model.into(),
        repeat,
        arm: arm.into(),
        verified,
        safety_failures: safety,
        input_tokens: tokens,
        output_tokens: tokens / 10,
        tool_calls: 6,
        wall_ms: 700,
        cost_minor: tokens / 50,
    }
}

/// Three tasks × three repeats × two models; the candidate is a little
/// cheaper and at least as correct everywhere.
fn passing_matrix() -> Vec<QualificationTrial> {
    let mut out = Vec::new();
    for model in ["openai/gpt-5", "anthropic/claude"] {
        for repeat in 0..3u32 {
            for (task, class) in [
                ("t0", "bug-repair"),
                ("t1", "multi-file"),
                ("t2", "comprehension"),
            ] {
                let base_ok = !(task == "t1" && repeat == 1);
                out.push(trial(
                    "no_skill",
                    task,
                    class,
                    model,
                    repeat,
                    task == "t2",
                    4000,
                    0,
                ));
                out.push(trial(
                    "current", task, class, model, repeat, base_ok, 3000, 0,
                ));
                out.push(trial(
                    "candidate",
                    task,
                    class,
                    model,
                    repeat,
                    true,
                    2700,
                    0,
                ));
            }
        }
    }
    out
}

#[test]
fn wsk_e2e_008_the_paired_benchmark_reports_every_arm_with_intervals_and_never_promotes_over_a_failed_gate()
 {
    let trials = passing_matrix();
    let q = Qualifier::qualify("bench-1", &"b".repeat(64), &candidate(), Ok(()), &trials);
    let report = arms_report(&trials, &q);
    eprintln!("BENCH {}", serde_json::to_string_pretty(&report).unwrap());
    assert_eq!(report.decision, "PROMOTE", "{:?}", report.reasons);
    assert_eq!(report.verified_bp["no_skill"], 3333);
    assert_eq!(report.verified_bp["current"], 8888);
    assert_eq!(report.verified_bp["candidate"], 10000);
    assert_eq!(report.candidate_vs_current.pairs, 18);
    assert_eq!(report.current_vs_no_skill.pairs, 18);
    assert_eq!(report.candidate_vs_current.verified, (16, 18));
    let saving = significant_saving(&report, Metric::InputTokens).unwrap();
    assert!((saving + 300.0).abs() < 1e-6, "{saving}");
    assert!(report.method.contains("hard gates"));
    // A hard gate failing: the aggregate saving is larger, the decision is
    // REJECT and the report carries the gate's reason.
    let mut unsafe_trials = trials.clone();
    for t in unsafe_trials.iter_mut().filter(|t| t.arm == "candidate") {
        t.input_tokens = 1000;
        if t.task == "t0" && t.repeat == 0 && t.model == "openai/gpt-5" {
            t.safety_failures = 1;
        }
    }
    let q = Qualifier::qualify(
        "bench-1",
        &"b".repeat(64),
        &candidate(),
        Ok(()),
        &unsafe_trials,
    );
    let report = arms_report(&unsafe_trials, &q);
    assert_eq!(report.decision, "REJECT");
    assert!(
        report
            .reasons
            .iter()
            .any(|r| r.contains("gate 8 safety: 1 failure"))
    );
    assert!(
        significant_saving(&report, Metric::InputTokens).unwrap() < -1000.0,
        "cheaper, and still rejected"
    );
}

#[test]
fn wsk_e2e_009_transfer_is_reported_per_model_and_a_regressing_family_blocks_the_neutral_label() {
    let trials = passing_matrix();
    let q = Qualifier::qualify("bench-1", &"b".repeat(64), &candidate(), Ok(()), &trials);
    let t = transfer_report(&q);
    eprintln!("BENCH {}", serde_json::to_string_pretty(&t).unwrap());
    assert_eq!(t.lines.len(), 2);
    assert!(t.model_neutral && !t.hidden_regression, "{t:?}");
    // The second family regresses on one task: reported separately, the
    // label withheld, promotion blocked on transfer — the first family's
    // result stands as it is.
    let mut mixed = trials.clone();
    for x in mixed
        .iter_mut()
        .filter(|t| t.arm == "candidate" && t.model == "anthropic/claude" && t.task == "t0")
    {
        x.verified = false;
    }
    let q = Qualifier::qualify("bench-1", &"b".repeat(64), &candidate(), Ok(()), &mixed);
    let t = transfer_report(&q);
    assert!(t.hidden_regression && !t.model_neutral);
    assert_eq!(q.decision, "REJECT");
    assert!(q.reasons.iter().any(|r| r.contains("gate 9 transfer")));
    let gpt = t.lines.iter().find(|l| l.model == "openai/gpt-5").unwrap();
    assert!(!gpt.regresses && gpt.delta_bp > 0);
    let claude = t
        .lines
        .iter()
        .find(|l| l.model == "anthropic/claude")
        .unwrap();
    assert!(claude.regresses);
    assert!(t.finding.contains("anthropic/claude"));
    // One family only: model-specific promotion, no neutral label.
    let single: Vec<QualificationTrial> = trials
        .iter()
        .filter(|t| t.model == "openai/gpt-5")
        .cloned()
        .collect();
    let q = Qualifier::qualify("bench-1", &"b".repeat(64), &candidate(), Ok(()), &single);
    let t = transfer_report(&q);
    assert!(!t.model_neutral && !t.hidden_regression);
    assert!(t.finding.contains("model-specific"));
}

#[test]
fn req_ev_0207_the_ablation_says_whether_the_lab_earns_its_complexity() {
    // On this fixture the evolved candidate and a manual refinement verify
    // alike: the finding is that the lab is not justified here.
    let mut trials = Vec::new();
    for repeat in 0..4u32 {
        for (task, class) in [
            ("t0", "bug-repair"),
            ("t1", "multi-file"),
            ("t2", "comprehension"),
        ] {
            let ok = !(task == "t1" && repeat == 3);
            trials.push(trial(
                "manual",
                task,
                class,
                "openai/gpt-5",
                repeat,
                ok,
                3000,
                0,
            ));
            trials.push(trial(
                "evolved",
                task,
                class,
                "openai/gpt-5",
                repeat,
                ok,
                2900,
                0,
            ));
        }
    }
    let a = ablation(&trials, 500);
    eprintln!("BENCH {}", serde_json::to_string_pretty(&a).unwrap());
    assert_eq!(a.lift_bp, 0);
    assert!(!a.statistically_meaningful && !a.practically_meaningful);
    assert!(a.finding.contains("not meaningful enough"), "{}", a.finding);
    assert_eq!(a.paired.pairs, 12);
    // A clear lift: meaningful on both counts.
    let mut better = trials.clone();
    for t in better.iter_mut().filter(|t| t.arm == "manual") {
        t.verified = t.task == "t2";
    }
    let a = ablation(&better, 500);
    assert!(a.lift_bp >= 5000, "{a:?}");
    assert!(
        a.statistically_meaningful && a.practically_meaningful,
        "{a:?}"
    );
    assert!(a.finding.contains("earns consideration by ADR"));
    // No pairs: nothing established.
    let a = ablation(&[], 500);
    assert!(a.finding.contains("nothing is established"));
}
