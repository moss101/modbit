//! PX-136 / PX-133: the paired DIRECT / CASCADE / CRITIQUE harness run end to
//! end against the real Core through `modbit-cli`, with a SCRIPTED provider.
//! This checks the harness (the signed registry, the arms, the operator
//! cascade plan, the review leg, the measurements, the statistics samples,
//! the report, the spend cap and the missing-measurement failure). It is not
//! a live result and the report says so; the live run is
//! `.github/workflows/live-evals.yml`.

mod px_common;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use modbit_bench_context_economics::paired::{
    Arm, Binding, Bundle, Options, PairedConfig, builtin_taskset, execute, rescore_bundle,
};
use modbit_bench_context_economics::spend::Price;
use modbit_bench_context_economics::suite::{TrialTask, builtin_suite};
use px_common::*;
use serde_json::json;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    /// The opener solves the task.
    Good,
    /// The opener makes a change that leaves the check failing; the stronger
    /// binding solves it.
    OpenerFails,
}

fn task_of<'a>(body: &serde_json::Value, tasks: &'a [TrialTask]) -> &'a TrialTask {
    let text = body["messages"].to_string();
    tasks
        .iter()
        .find(|t| text.contains(&serde_json::to_string(&t.goal).unwrap()[1..t.goal.len().min(60)]))
        .expect("the request names a suite task")
}

fn solver_steps(task: &TrialTask, wrong: bool) -> Vec<serde_json::Value> {
    let paths: Vec<&str> = task.reference.iter().map(|(p, _)| p.as_str()).collect();
    let mut steps = vec![
        json!({"calls": [{"name": "plan.update", "args": {"outcome": task.id, "expected_files": paths}}]}),
    ];
    for (p, c) in &task.reference {
        steps.push(json!({"calls": [{"name": "fs.read", "args": {"path": p}}]}));
        let content = if wrong {
            // The starting content with a comment: the check still fails.
            let original = task
                .files
                .iter()
                .find(|(fp, _)| fp == p)
                .map(|(_, fc)| fc.clone())
                .unwrap_or_default();
            format!("{original}# tried\n")
        } else {
            c.clone()
        };
        steps.push(json!({"calls": [{"name": "change.apply", "args": {"path": p, "op": "replace", "content": content}}]}));
    }
    steps.push(json!({"calls": [{"name": "task.complete", "args": {"summary": "done", "self_review": {"findings": []}}}]}));
    steps
}

fn stronger_steps(task: &TrialTask) -> Vec<serde_json::Value> {
    let mut steps = Vec::new();
    for (p, c) in &task.reference {
        steps.push(json!({"calls": [{"name": "change.apply", "args": {"path": p, "op": "replace", "content": c}}]}));
    }
    steps.push(json!({"calls": [{"name": "task.complete", "args": {"summary": "done", "self_review": {"findings": []}}}]}));
    steps
}

struct Scripted {
    base: String,
}

async fn scripted(tasks: Vec<TrialTask>, mode: Mode) -> Scripted {
    let mode = Arc::new(Mutex::new(mode));
    let counters: Arc<Mutex<HashMap<(String, String), usize>>> = Arc::default();
    let tasks = Arc::new(tasks);
    let (m2, c2) = (Arc::clone(&mode), Arc::clone(&counters));
    let (base, _seen) = scripted_model_fn(Arc::new(move |body, results| {
        let task = task_of(body, &tasks);
        let model = body["model"].as_str().unwrap_or_default().to_owned();
        let names = request_tool_names(body);
        let kind = if names.iter().any(|n| n == "review.report") {
            "review"
        } else if model == "m-strong" {
            "strong"
        } else {
            "open"
        };
        let step = {
            let mut c = c2.lock().unwrap();
            // The first request of a trial (no tool result yet) starts the
            // task's scripts over: each trial is a fresh run.
            if kind == "open" && results == 0 {
                c.retain(|(t, _), _| *t != task.id);
            }
            let n = c.entry((task.id.clone(), kind.to_owned())).or_insert(0);
            let s = *n;
            *n += 1;
            s
        };
        match kind {
            "review" => {
                if step == 0 {
                    json!({"calls": [{"name": "review.report", "args": {"verdict": "PASS", "confidence": 90, "summary": "the change makes the check pass", "findings": [], "unresolved_questions": []}}]})
                } else {
                    json!({"text": "done"})
                }
            }
            "strong" => stronger_steps(task)
                .get(step)
                .cloned()
                .unwrap_or_else(|| json!({"text": "nothing further"})),
            _ => solver_steps(task, *m2.lock().unwrap() == Mode::OpenerFails)
                .get(step)
                .cloned()
                .unwrap_or_else(|| json!({"text": "I cannot see why the check fails."})),
        }
    }))
    .await;
    Scripted { base }
}

fn options(
    s: &Scripted,
    out: &std::path::Path,
    arms: Vec<Arm>,
    prices: (f64, f64),
    cap: f64,
) -> Options {
    let core_bin = PathBuf::from(env!("CARGO_BIN_EXE_modbit-core"));
    let cli_name = if cfg!(windows) {
        "modbit-cli.exe"
    } else {
        "modbit-cli"
    };
    let cli = core_bin.with_file_name(cli_name);
    assert!(cli.exists(), "build modbit-cli first: {}", cli.display());
    let price = Price {
        input_per_mtok_usd: prices.0,
        output_per_mtok_usd: prices.1,
    };
    let mut env = model_env(&s.base);
    env.push((
        "MODBIT_OPENAI_MODELS".into(),
        format!(
            "m-open={}/{},m-strong={}/{}",
            prices.0, prices.1, prices.0, prices.1
        ),
    ));
    Options {
        row: "px136".into(),
        cfg: PairedConfig {
            cli,
            core: core_bin,
            work: out.join("work"),
            opener: Binding {
                endpoint: "openai".into(),
                model: "m-open".into(),
                price,
            },
            second: Binding {
                endpoint: "openai".into(),
                model: "m-strong".into(),
                price,
            },
            env,
            max_turns: 12,
            trial_timeout: Duration::from_secs(240),
            parallel: 1,
            repeats: 1,
            live: false,
            question_answer: "proceed".into(),
        },
        taskset: PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../benchmarks/context-economics/paired/taskset.json"),
        arms,
        max_cost_usd: cap,
        out_dir: out.join("bundle"),
        min_tasks: 1,
        limit_tasks: Some(1),
        min_escalation_samples: 0,
        min_reviewer_samples: 0,
        environment: Default::default(),
    }
}

/// The notable events of every trial and the CLI exit lines, for a failure
/// message (never the whole log).
fn logs(out: &std::path::Path) -> String {
    let mut text = String::new();
    if let Ok(rd) = std::fs::read_dir(out.join("bundle/trials")) {
        for e in rd.flatten() {
            text.push_str(&format!("==== {}\n", e.file_name().to_string_lossy()));
            for l in std::fs::read_to_string(e.path().join("cli.log"))
                .unwrap_or_default()
                .lines()
                .filter(|l| {
                    l.starts_with("$ modbit-cli") && !l.contains("events tail")
                        || l.starts_with("[exit")
                        || l.starts_with("task state")
                        || l.contains("rejected")
                })
            {
                text.push_str(&format!("{}\n", l.chars().take(200).collect::<String>()));
            }
            let events = std::fs::read_to_string(e.path().join("events.jsonl")).unwrap_or_default();
            let names: Vec<String> = events
                .lines()
                .filter_map(|l| serde_json::from_str::<serde_json::Value>(l).ok())
                .filter_map(|v| v["event_type"].as_str().map(str::to_owned))
                .filter(|n| {
                    n.contains("Gate")
                        || n.contains("Review")
                        || n.contains("Slot")
                        || n.contains("Continuation")
                        || n.contains("Route")
                        || n.contains("Risk")
                        || n.contains("Revision")
                        || n.contains("Waiting")
                        || n.contains("Attention")
                })
                .collect();
            text.push_str(&format!("{}\n", names.join(" ")));
            for l in events.lines() {
                if l.contains("\"TaskNeedsAttention\"")
                    || l.contains("\"UserQuestionAsked\"")
                    || l.contains("\"TaskWaiting\"")
                {
                    text.push_str(&format!("{}\n", l.chars().take(700).collect::<String>()));
                }
            }
        }
    }
    text
}

fn load(out: &std::path::Path) -> Bundle {
    let text = std::fs::read_to_string(out.join("bundle/result.json")).unwrap();
    serde_json::from_str(&text).unwrap()
}

#[test]
fn the_pinned_task_set_is_the_suite_it_names() {
    let pin: modbit_bench_context_economics::paired::TaskSet = serde_json::from_str(
        &std::fs::read_to_string(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../benchmarks/context-economics/paired/taskset.json"),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(pin, builtin_taskset());
    assert!(pin.tasks.len() >= 30);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_direct_run_and_a_critique_run_are_scored_and_the_review_leg_is_a_priced_sample() {
    let tasks: Vec<TrialTask> = builtin_suite().into_iter().take(1).collect();
    let s = scripted(tasks, Mode::Good).await;
    let dir = tempfile::tempdir().unwrap();
    let mut o = options(
        &s,
        dir.path(),
        vec![Arm::Direct, Arm::Critique],
        (1.0, 2.0),
        1.0e9,
    );
    // No sandbox for a review on Windows: the review slot cannot activate.
    o.min_reviewer_samples = u32::from(!cfg!(windows));
    let out = dir.path().to_path_buf();
    let status = tokio::task::spawn_blocking(move || execute(&o).unwrap())
        .await
        .unwrap();
    let b = load(&out);
    assert!(status.complete, "{:?}\n{}", status.missing, logs(&out));
    assert_eq!(b.runs.len(), 2);
    let direct = b.runs.iter().find(|r| r.arm == Arm::Direct).unwrap();
    assert!(direct.error.is_none(), "{direct:?}");
    assert!(direct.core_verified && direct.oracle_pass && direct.check_untouched);
    assert!(direct.verified_success, "{direct:?}");
    assert_eq!(
        direct.accounting.as_ref().unwrap().path_label,
        "DIRECT",
        "{direct:?}"
    );
    assert!(!direct.escalated && !direct.reviewed);
    assert!(direct.cost_usd.unwrap() > 0.0 && direct.model_calls > 0);
    assert_eq!(direct.gates.last().unwrap().verdict, "ACCEPT");
    assert!(direct.gates.last().unwrap().oracle_pass);
    let critique = b.runs.iter().find(|r| r.arm == Arm::Critique).unwrap();
    assert!(critique.error.is_none(), "{critique:?}");
    if !cfg!(windows) {
        assert!(critique.reviewed, "{critique:?}");
        assert_eq!(critique.review_verdicts, vec!["PASS"]);
        assert_eq!(
            critique.accounting.as_ref().unwrap().path_label,
            "CRITIQUE",
            "{critique:?}"
        );
        // The review leg's cost is in the run's cost: the second binding
        // answered at least one call.
        assert!(
            critique
                .answered_by
                .get("openai/m-strong")
                .copied()
                .unwrap_or(0)
                > 0,
            "{critique:?}"
        );
        assert!(critique.cost_usd.unwrap() > direct.cost_usd.unwrap() * 0.5);
        let reviewer = b
            .report
            .samples
            .iter()
            .find(|f| f.kind == "reviewer")
            .unwrap();
        assert!(reviewer.samples >= 1, "{reviewer:?}");
        assert_eq!(reviewer.unknown_cost_samples, 0, "{reviewer:?}");
        assert!(reviewer.total_cost_minor > 0, "{reviewer:?}");
    }
    assert!(critique.verified_success, "{critique:?}");
    // Every run id, the registry and the statistics snapshots are recorded.
    assert!(
        b.runs
            .iter()
            .all(|r| !r.run_id.is_empty() && !r.stats_version.is_empty())
    );
    assert!(!b.report.registry_digest.is_empty() && !b.report.registry_generation.is_empty());
    assert!(b.report.method.contains("not a result"));
    assert!(b.report.verdict.starts_with("NOT_EVALUATED"));
    // The analysis reproduces from the retained data; an altered log does not.
    assert_eq!(
        rescore_bundle(&out.join("bundle")).unwrap(),
        b.report.digest
    );
    let log = out.join("bundle").join(&direct.events_file);
    let mut text = std::fs::read_to_string(&log).unwrap();
    text.push('\n');
    std::fs::write(&log, text).unwrap();
    assert!(rescore_bundle(&out.join("bundle")).is_err());
    drop(s);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_cascade_escalates_on_the_operator_plan_and_the_continuation_is_a_priced_escalation_sample()
 {
    let tasks: Vec<TrialTask> = builtin_suite().into_iter().take(1).collect();
    let s = scripted(tasks, Mode::OpenerFails).await;
    let dir = tempfile::tempdir().unwrap();
    let mut o = options(&s, dir.path(), vec![Arm::Cascade], (1.0, 2.0), 1.0e9);
    o.min_escalation_samples = 1;
    let out = dir.path().to_path_buf();
    let status = tokio::task::spawn_blocking(move || execute(&o).unwrap())
        .await
        .unwrap();
    let b = load(&out);
    let run = &b.runs[0];
    assert!(
        status.complete,
        "{:?}\n{}",
        status.missing,
        serde_json::to_string_pretty(run).unwrap()
    );
    assert!(run.error.is_none(), "{run:?}");
    assert!(run.escalation_attempted && run.escalated, "{run:?}");
    assert_eq!(
        run.accounting.as_ref().unwrap().path_label,
        "CASCADE",
        "{run:?}"
    );
    assert!(run.verified_success, "{run:?}");
    // The failed first leg is a gate rejection of a candidate the
    // independent check also fails; the continuation's candidate is accepted
    // and passes.
    assert_eq!(run.gates.len(), 2, "{:?}", run.gates);
    assert_eq!(
        (
            run.gates[0].leg.as_str(),
            run.gates[0].verdict.as_str(),
            run.gates[0].oracle_pass
        ),
        ("first", "REJECT", false)
    );
    assert_eq!(
        (
            run.gates[1].leg.as_str(),
            run.gates[1].verdict.as_str(),
            run.gates[1].oracle_pass
        ),
        ("final", "ACCEPT", true)
    );
    assert!(run.answered_by.get("openai/m-strong").copied().unwrap_or(0) > 0);
    // The escalation sample: one observation that succeeded, priced; the
    // failed first leg earns the escalation nothing.
    let esc = b
        .report
        .samples
        .iter()
        .find(|f| f.kind == "escalation")
        .unwrap();
    assert_eq!((esc.samples, esc.successes), (1, 1), "{esc:?}");
    assert_eq!(esc.unknown_cost_samples, 0);
    let gate = &b.report.gate["cascade"];
    assert_eq!((gate.false_accept.k, gate.false_reject.k), (0, 0));
    assert_eq!(gate.observations, 2);
    drop(s);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_required_measurement_that_never_happened_fails_the_run() {
    // The opener solves the task, so a cascade never escalates: there is no
    // escalation sample, and a run that requires one is incomplete.
    let tasks: Vec<TrialTask> = builtin_suite().into_iter().take(1).collect();
    let s = scripted(tasks, Mode::Good).await;
    let dir = tempfile::tempdir().unwrap();
    let mut o = options(&s, dir.path(), vec![Arm::Cascade], (1.0, 2.0), 1.0e9);
    o.min_escalation_samples = 1;
    let out = dir.path().to_path_buf();
    let status = tokio::task::spawn_blocking(move || execute(&o).unwrap())
        .await
        .unwrap();
    assert!(!status.complete);
    assert!(
        status
            .missing
            .iter()
            .any(|m| m.contains("escalation samples")),
        "{:?}",
        status.missing
    );
    // The partial bundle is still written, with the run in it.
    let b = load(&out);
    assert_eq!(b.runs.len(), 1);
    assert!(b.runs[0].verified_success);
    assert!(!b.status.complete);
    // A pilot of fewer than the required tasks is incomplete too.
    assert!(
        {
            let mut o2 = options(&s, dir.path(), vec![Arm::Direct], (1.0, 2.0), 1.0e9);
            o2.min_tasks = 30;
            o2.out_dir = dir.path().join("bundle2");
            let st = tokio::task::spawn_blocking(move || execute(&o2).unwrap())
                .await
                .unwrap();
            st.missing.iter().any(|m| m.contains("at least 30"))
        },
        "a one-task pilot cannot be complete"
    );
    drop(s);
}

#[tokio::test(flavor = "multi_thread")]
async fn the_spend_cap_stops_the_run_and_reports_what_it_has() {
    let tasks: Vec<TrialTask> = builtin_suite().into_iter().take(3).collect();
    let s = scripted(tasks, Mode::Good).await;
    let dir = tempfile::tempdir().unwrap();
    // $5000 per million tokens: one trial costs tens of cents; the cap lets
    // a few trials through and refuses the rest.
    let mut o = options(&s, dir.path(), vec![Arm::Direct], (10.0, 20.0), 0.15);
    o.limit_tasks = Some(3);
    let out = dir.path().to_path_buf();
    let status = tokio::task::spawn_blocking(move || execute(&o).unwrap())
        .await
        .unwrap();
    assert!(status.spend.stopped_by_cap, "{status:?}");
    assert!(!status.complete);
    let b = load(&out);
    assert!(
        !b.runs.is_empty() && b.runs.len() < 3,
        "{} runs",
        b.runs.len()
    );
    // The bound: what was spent is at most the cap plus one trial's cost.
    let one = b.runs.iter().filter_map(|r| r.cost_usd).fold(0.0, f64::max);
    assert!(
        status.spend.spent_usd <= status.spend.cap_usd + one * 1.01,
        "{status:?} one trial {one}"
    );
    assert!(
        status.missing.iter().any(|m| m.contains("spend cap")),
        "{:?}",
        status.missing
    );
    drop(s);
}
