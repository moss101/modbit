//! PX-114: the paired projection trial harness run end to end against the
//! real Core through `modbit-cli`, with a SCRIPTED provider. This checks the
//! harness (arms, measurements from the event log and the economics view,
//! the report, the rule). It is not a live result and the report says so;
//! the live paired trial needs a provider the owner configures.

mod px_common;

use modbit_bench_context_economics::projection_trial::{
    Arm, Direction, RunConfig, report, run_matrix, validate,
};
use modbit_bench_context_economics::suite::{TrialTask, builtin_suite};
use px_common::*;
use serde_json::json;
use std::sync::Arc;

fn solve_script(task: &TrialTask, arm: Arm, sabotage: bool) -> Vec<serde_json::Value> {
    let paths: Vec<&str> = task.reference.iter().map(|(p, _)| p.as_str()).collect();
    let content = |c: &str| {
        if sabotage {
            format!("{c}\nraise SystemExit('sabotaged')\n")
        } else {
            c.to_owned()
        }
    };
    let mut steps = vec![
        json!({"calls": [{"name": "plan.update", "args": {"outcome": task.id, "expected_files": paths}}]}),
    ];
    match arm {
        Arm::Direct => {
            for (p, c) in &task.reference {
                steps.push(json!({"calls": [{"name": "fs.read", "args": {"path": p}}]}));
                steps.push(json!({"calls": [{"name": "change.apply", "args": {"path": p, "op": "replace", "content": content(c)}}]}));
            }
        }
        Arm::Procedural | Arm::ExecOnly => {
            let mut program = String::new();
            for (p, c) in &task.reference {
                program.push_str(&format!(
                    "await tools.fs.read({{ path: {} }});\nawait tools.change.apply({{ path: {}, op: 'replace', content: {} }});\n",
                    serde_json::to_string(p).unwrap(),
                    serde_json::to_string(p).unwrap(),
                    serde_json::to_string(&content(c)).unwrap()
                ));
            }
            program.push_str("return 'done';\n");
            steps.push(json!({"calls": [{"name": "proc.exec", "args": {"program": program}}]}));
        }
    }
    steps.push(json!({"calls": [{"name": "task.complete", "args": {"summary": "done", "self_review": {"findings": []}}}]}));
    steps
}

fn arm_of(body: &serde_json::Value) -> Arm {
    let names = request_tool_names(body);
    if !names.iter().any(|n| n == "proc.exec") {
        Arm::Direct
    } else if names.len() <= 8 {
        Arm::ExecOnly
    } else {
        Arm::Procedural
    }
}

fn task_of<'a>(body: &serde_json::Value, tasks: &'a [TrialTask]) -> &'a TrialTask {
    let text = body["messages"].to_string();
    tasks
        .iter()
        .find(|t| text.contains(&serde_json::to_string(&t.goal).unwrap()[1..t.goal.len().min(60)]))
        .expect("the request names a suite task")
}

#[tokio::test(flavor = "multi_thread")]
async fn the_trial_harness_runs_three_arms_through_the_real_core_and_reports_every_run() {
    let tasks: Vec<TrialTask> = builtin_suite().into_iter().take(2).collect();
    let suite = Arc::new(tasks.clone());
    // The second task is sabotaged under exec_only only: a failed arm must
    // appear in the report, counted and named.
    let reply_suite = Arc::clone(&suite);
    let (base, seen) = scripted_model_fn(Arc::new(move |body, results| {
        let arm = arm_of(body);
        let task = task_of(body, &reply_suite);
        let sabotage = arm == Arm::ExecOnly && task.id == reply_suite[1].id;
        let last = body["messages"]
            .as_array()
            .and_then(|m| m.iter().rev().find(|x| x["role"] == "tool"))
            .and_then(|m| m["content"].as_str())
            .unwrap_or_default()
            .to_owned();
        if last.contains("status: RUNNING") {
            return json!({"calls": [{"name": "proc.wait", "args": {"handle": "call_1_0", "timeout_ms": 60000}}]});
        }
        solve_script(task, arm, sabotage)
            .get(results)
            .cloned()
            .unwrap_or_else(|| json!({"text": "nothing further"}))
    }))
    .await;
    let core_bin = std::path::PathBuf::from(env!("CARGO_BIN_EXE_modbit-core"));
    let cli_name = if cfg!(windows) {
        "modbit-cli.exe"
    } else {
        "modbit-cli"
    };
    let cli = core_bin.with_file_name(cli_name);
    assert!(cli.exists(), "build modbit-cli first: {}", cli.display());
    let cfg = RunConfig {
        cli,
        core: core_bin,
        model: "gpt-5-mini".into(),
        env: model_env(&base),
        max_turns: 10,
        live: false,
    };
    let runs = tokio::task::spawn_blocking({
        let cfg = cfg.clone();
        let tasks = tasks.clone();
        move || run_matrix(&cfg, &tasks, 2)
    })
    .await
    .unwrap();
    assert_eq!(runs.len(), 12, "2 tasks x 2 repeats x 3 arms");
    for r in &runs {
        assert!(r.error.is_none(), "{:?} {:?}", r.arm, r.error);
        assert!(r.trial.model_calls > 0 && r.trial.input_tokens > 0, "{r:?}");
        assert!(!r.schema_bytes_per_request.is_empty(), "{r:?}");
        let sabotaged = r.arm == Arm::ExecOnly && r.trial.task == tasks[1].id;
        assert_eq!(
            r.trial.verified, !sabotaged,
            "{:?} {} -> {}",
            r.arm, r.trial.task, r.state
        );
    }
    // The measured schema bytes follow the surface: the Core's recorded
    // per-request bytes equal what the provider received.
    let bodies = seen.lock().unwrap().clone();
    let recorded: u64 = runs
        .iter()
        .flat_map(|r| r.schema_bytes_per_request.iter())
        .sum();
    let received: u64 = bodies.iter().map(|b| request_schema_bytes(b) as u64).sum();
    assert_eq!(recorded, received, "recorded bytes equal received bytes");
    let per_request = |arm: Arm| -> f64 {
        let v: Vec<u64> = runs
            .iter()
            .filter(|r| r.arm == arm)
            .flat_map(|r| r.schema_bytes_per_request.clone())
            .collect();
        v.iter().sum::<u64>() as f64 / v.len() as f64
    };
    let (d, p, e) = (
        per_request(Arm::Direct),
        per_request(Arm::Procedural),
        per_request(Arm::ExecOnly),
    );
    eprintln!(
        "PX114_TRIAL_SCHEMA_BYTES_PER_REQUEST direct(typed)={d:.0} procedural(default)={p:.0} exec_only={e:.0}"
    );
    assert!(d < p, "typed carries no program runtime: {d} vs {p}");
    assert!(e * 2.0 < p, "exec_only is far smaller: {e} vs {p}");
    // The report: scripted, so nothing is decided, and every arm and every
    // run (the failed ones too) is in it.
    let rep = report(&runs, "gpt-5-mini", 2, 2, false);
    validate(&rep, 2, 2).unwrap();
    let exec = rep.arms.iter().find(|a| a.arm == Arm::ExecOnly).unwrap();
    assert_eq!((exec.runs, exec.accuracy.k), (4, 2));
    assert_eq!(exec.failed_runs.len(), 2, "{:?}", exec.failed_runs);
    assert!(
        rep.comparisons
            .iter()
            .all(|c| c.direction == Direction::NotEvaluated)
    );
    assert!(rep.method.contains("not a live result"));
}
