//! The projection trial's report logic and suite (PX-114): synthetic runs,
//! no Core. The real-Core run against a scripted provider is
//! `services/modbit-core/tests/px_114_trial.rs`.

use modbit_bench_context_economics::Trial;
use modbit_bench_context_economics::projection_trial::{
    Arm, Direction, RunRecord, broken_tasks, report, validate, wilson,
};
use modbit_bench_context_economics::suite::builtin_suite;

fn run(arm: Arm, task: usize, verified: bool, tokens: u64) -> RunRecord {
    RunRecord {
        arm,
        trial: Trial {
            variant: arm.label().into(),
            task: format!("t{task}"),
            repeat: 0,
            input_tokens: tokens,
            output_tokens: 0,
            cached_input_tokens: 0,
            tool_calls: 3,
            normalized_tool_calls: 3,
            model_calls: 3,
            agent_ms: 1,
            cold_ms: 0,
            verified,
            tool_schema_bytes: 1000,
        },
        state: if verified {
            "ReadyForReview"
        } else {
            "Waiting"
        }
        .into(),
        error: None,
        schema_bytes_per_request: vec![1000],
    }
}

fn matrix(n: usize, exec_verified: impl Fn(usize) -> bool, exec_tokens: u64) -> Vec<RunRecord> {
    let mut v = Vec::new();
    for t in 0..n {
        v.push(run(Arm::Direct, t, true, 1000));
        v.push(run(Arm::Procedural, t, true, 1000));
        v.push(run(Arm::ExecOnly, t, exec_verified(t), exec_tokens));
    }
    v
}

#[test]
fn the_suite_has_thirty_tasks_and_each_fails_before_and_passes_with_its_reference() {
    let tasks = builtin_suite();
    assert_eq!(tasks.len(), 30);
    let mut ids: Vec<&str> = tasks.iter().map(|t| t.id.as_str()).collect();
    ids.sort_unstable();
    ids.dedup();
    assert_eq!(ids.len(), 30, "ids are unique");
    if std::process::Command::new("python3")
        .arg("--version")
        .output()
        .is_err()
    {
        eprintln!("python3 is not installed here; the suite self-check is skipped");
        return;
    }
    assert_eq!(broken_tasks(&tasks), Vec::<String>::new());
}

#[test]
fn wilson_intervals_bracket_the_proportion_and_stay_in_range() {
    let p = wilson(27, 30);
    assert!(
        p.lo < p.p && p.p < p.hi && p.lo > 0.7 && p.hi <= 1.0,
        "{p:?}"
    );
    let all = wilson(30, 30);
    assert!(all.hi <= 1.0 && all.lo > 0.85, "{all:?}");
    assert!(wilson(0, 0).p.is_nan());
}

#[test]
fn a_report_counts_every_run_and_names_the_failed_ones_whatever_the_direction() {
    // exec_only fails 6 of 30 and costs more tokens: the worse arm is in the
    // report, with its failures named, and the verdict is recorded.
    let runs = matrix(30, |t| t % 5 != 0, 1500);
    let r = report(&runs, "m", 30, 1, true);
    validate(&r, 30, 1).unwrap();
    let exec = r.arms.iter().find(|a| a.arm == Arm::ExecOnly).unwrap();
    assert_eq!(exec.runs, 30);
    assert_eq!(exec.accuracy.k, 24);
    assert_eq!(exec.failed_runs.len(), 6);
    let c = r
        .comparisons
        .iter()
        .find(|c| c.arm == Arm::ExecOnly && c.against == Arm::Procedural)
        .unwrap();
    assert_eq!(c.pairs, 30);
    assert_eq!(c.direction, Direction::Worse, "{c:?}");
    assert!(r.recommendation.contains("WORSE"));
}

#[test]
fn a_better_arm_needs_non_inferior_accuracy_and_clearly_fewer_tokens() {
    // Varying token savings give the bootstrap a width.
    let mut runs = Vec::new();
    for t in 0..30 {
        runs.push(run(Arm::Direct, t, true, 1000));
        runs.push(run(Arm::Procedural, t, true, 1000));
        runs.push(run(Arm::ExecOnly, t, true, 600 + (t as u64 % 7) * 20));
    }
    let r = report(&runs, "m", 30, 1, true);
    let c = r
        .comparisons
        .iter()
        .find(|c| c.arm == Arm::ExecOnly && c.against == Arm::Procedural)
        .unwrap();
    assert_eq!(c.direction, Direction::Better, "{c:?}");
    // Fewer than thirty pairs decides nothing.
    let few = report(&matrix(10, |_| true, 500), "m", 10, 1, true);
    assert!(
        few.comparisons
            .iter()
            .all(|c| c.direction == Direction::Inconclusive)
    );
}

#[test]
fn a_scripted_provider_decides_nothing() {
    let r = report(&matrix(30, |_| true, 100), "m", 30, 1, false);
    assert!(
        r.comparisons
            .iter()
            .all(|c| c.direction == Direction::NotEvaluated)
    );
    assert!(r.recommendation.starts_with("NOT_EVALUATED"));
    assert!(r.method.contains("not a live result"));
    validate(&r, 30, 1).unwrap();
}

#[test]
fn a_report_that_omits_an_arm_or_drops_a_failed_run_is_invalid() {
    let runs = matrix(30, |t| t != 3, 900);
    let mut r = report(&runs, "m", 30, 1, true);
    validate(&r, 30, 1).unwrap();
    // An omitted arm.
    let mut missing = r.clone();
    missing.arms.retain(|a| a.arm != Arm::ExecOnly);
    assert!(validate(&missing, 30, 1).unwrap_err().contains("exec_only"));
    // A failed arm quietly shortened.
    let exec = r.arms.iter_mut().find(|a| a.arm == Arm::ExecOnly).unwrap();
    exec.runs -= 1;
    assert!(validate(&r, 30, 1).is_err());
    // A failed run dropped from the list.
    let mut r = report(&runs, "m", 30, 1, true);
    let exec = r.arms.iter_mut().find(|a| a.arm == Arm::ExecOnly).unwrap();
    exec.failed_runs.clear();
    assert!(
        validate(&r, 30, 1)
            .unwrap_err()
            .contains("drops a failed run")
    );
    // A scripted run that carries a verdict.
    let mut s = report(&matrix(30, |_| true, 100), "m", 30, 1, false);
    s.comparisons[0].direction = Direction::Better;
    assert!(validate(&s, 30, 1).is_err());
}
