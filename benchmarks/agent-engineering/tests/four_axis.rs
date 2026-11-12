//! QUAL-PX-184 / PX-E2E-184: the four-axis agent benchmark — tool use,
//! memory, skill use and sub-agent coordination — through the real product.
//!
//! The offline proofs cover the suite's shape and refusals (an untagged task,
//! a task without its own verifier, an axis below ten tasks), the report's
//! refusals (a cost not derived from recorded usage, a gate that cannot fail)
//! and the verifier reading only observable outcomes. The real-Core proof runs
//! the `competence-baseline` binary over all forty tasks against the real CLI
//! and Core with a scripted OpenAI-compatible model, compares the run with
//! the retained baseline (no difference), then injects a regression on the
//! memory axis and shows that only the memory axis fails its gate.

use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};

use modbit_bench_agent_engineering::axes::{self, TrialUsage, recorded_usage};
use modbit_bench_agent_engineering::verifier::{
    FileCheck, MemoryCheck, ProposalCheck, SubagentCheck, ToolCheck,
};
use modbit_bench_agent_engineering::{
    AXES, Bundle, Counts, Economics, Environment, EventLog, Gate, Observed, Protocol,
    ReportRefused, Suite, ToolResultRecord, TrialOutcome, Verifier, VerifierOutcome, compare,
    evaluate, metrics, parse_events, report,
};
use serde_json::{Value, json};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap()
}

fn suite_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("suites/four-axis")
}

fn baseline_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("baselines/four-axis")
}

// ------------------------------------------------------------ the suite

#[test]
fn qual_px_184_the_suite_has_ten_tagged_tasks_per_axis_each_with_its_own_verifier() {
    let (suite, _) = Suite::load(&suite_dir().join("tasks.json")).unwrap();
    assert_eq!(suite.id, "four-axis");
    assert!(suite.tasks.len() >= 40, "{}", suite.tasks.len());
    let gate = suite.protocol.axes.as_ref().unwrap().gate;
    assert!(gate.check().is_ok());
    for axis in AXES {
        let n = suite
            .tasks
            .iter()
            .filter(|t| t.axis.as_deref() == Some(axis))
            .count();
        assert!(n >= 10, "{axis}: {n}");
    }
    for t in &suite.tasks {
        let v = t.verifier.as_ref().unwrap();
        assert!(v.check_count() > 0, "{}", t.id);
        // Each axis's tasks exercise their capability and verify it from the
        // Core's own records, not only from files.
        match t.axis.as_deref().unwrap() {
            "tool_use" => assert!(!v.tools.is_empty(), "{}", t.id),
            "memory" => assert!(v.memory.is_some(), "{}", t.id),
            "skill_use" => {
                assert!(!v.skills_used.is_empty(), "{}", t.id);
                assert_eq!(t.seed.skills.len(), 1, "{}", t.id);
            }
            "subagent" => assert!(v.subagents.is_some(), "{}", t.id),
            other => panic!("{other}"),
        }
        assert!(
            !v.files.is_empty(),
            "{}: repository state is checked too",
            t.id
        );
    }
    // The scripted model has a trajectory for every task, under its goal.
    let scripts: Value = serde_json::from_str(
        &std::fs::read_to_string(suite_dir().join("scripted-model.json")).unwrap(),
    )
    .unwrap();
    for t in &suite.tasks {
        assert_eq!(scripts["tasks"][&t.id]["goal"], t.goal.as_str(), "{}", t.id);
    }

    // Negative proof: an untagged task, a task without its own verifier, two
    // tasks sharing one verifier, a verifier that checks nothing, an axis
    // with fewer than ten tasks, and a gate that cannot fail are refused.
    let mut untagged = suite.clone();
    untagged.tasks[0].axis = None;
    assert!(untagged.check().unwrap_err().contains("not tagged"));
    let mut unknown = suite.clone();
    unknown.tasks[0].axis = Some("vibes".into());
    assert!(unknown.check().unwrap_err().contains("not one of"));
    let mut no_verifier = suite.clone();
    no_verifier.tasks[3].verifier = None;
    let e = no_verifier.check().unwrap_err();
    assert!(
        e.contains("names no command") || e.contains("no verifier"),
        "{e}"
    );
    no_verifier.tasks[3].acceptance.commands = vec![vec!["git".into(), "--version".into()]];
    assert!(
        no_verifier
            .check()
            .unwrap_err()
            .contains("no verifier of its own")
    );
    let mut shared_id = suite.clone();
    let id = shared_id.tasks[0].verifier.as_ref().unwrap().id.clone();
    shared_id.tasks[1].verifier.as_mut().unwrap().id = id;
    assert!(shared_id.check().unwrap_err().contains("is shared"));
    let mut shared_checks = suite.clone();
    let mut copy = shared_checks.tasks[0].verifier.clone().unwrap();
    copy.id = "px184/copy".into();
    shared_checks.tasks[1].verifier = Some(copy);
    assert!(shared_checks.check().unwrap_err().contains("its own"));
    let mut empty = suite.clone();
    empty.tasks[2].verifier = Some(Verifier {
        id: "px184/empty".into(),
        ..Verifier::default()
    });
    assert!(empty.check().unwrap_err().contains("checks nothing"));
    let mut nine = suite.clone();
    let i = nine
        .tasks
        .iter()
        .position(|t| t.axis.as_deref() == Some("skill_use"))
        .unwrap();
    nine.tasks.remove(i);
    assert!(
        nine.check()
            .unwrap_err()
            .contains("axis skill_use has 9 tasks")
    );
    let mut lax = suite.clone();
    lax.protocol.axes.as_mut().unwrap().min_tasks_per_axis = 3;
    lax.tasks.remove(i);
    assert!(
        lax.check().unwrap_err().contains("at least 10"),
        "the floor of ten is not the suite's to lower"
    );
    let mut never = suite;
    never.protocol.axes.as_mut().unwrap().gate.max_drop = 1.0;
    assert!(never.check().unwrap_err().contains("cannot fail"));
}

// ------------------------------------------------------------ the report

fn trial(task: &str, axis: &str, verified: bool, out_tokens: u64) -> TrialOutcome {
    TrialOutcome {
        task: task.into(),
        trial: 1,
        state: "ReadyForReview".into(),
        exit_code: 0,
        core_verified: true,
        acceptance_passed: verified,
        test_integrity_ok: true,
        verified_success: verified,
        first_pass: verified,
        first_candidate: verified,
        counts: Counts::default(),
        economics: Economics::default(),
        event_log: EventLog::default(),
        diff_sha256: String::new(),
        interactions: 0,
        notes: vec![],
        axis: Some(axis.into()),
        verifier: Some(VerifierOutcome {
            id: format!("v/{task}"),
            passed: verified,
            checks: vec![],
        }),
        usage: Some(TrialUsage {
            tasks: vec!["t".into()],
            model_calls: 4,
            input_tokens: 400,
            output_tokens: out_tokens,
            cost_usd: Some(0.001),
            recorded_model_calls: 4,
            recorded_output_tokens: out_tokens,
        }),
    }
}

fn synthetic(failing_axis: Option<&str>) -> Bundle {
    let mut trials = Vec::new();
    for axis in AXES {
        for n in 0..10 {
            let fail = failing_axis == Some(axis) && n < 4;
            trials.push(trial(&format!("{axis}/t{n}"), axis, !fail, 100));
        }
    }
    Bundle {
        schema: modbit_bench_agent_engineering::bundle::SCHEMA.into(),
        suite_id: "four-axis".into(),
        suite_version: "1.0.0".into(),
        task_list_digest: "d".repeat(64),
        tasks: trials.iter().map(|t| t.task.clone()).collect(),
        protocol: Protocol {
            trials_per_task: 1,
            max_turns: 24,
            configuration: "direct".into(),
            endpoint: "openai".into(),
            model: "scripted".into(),
            base_url_host: "127.0.0.1".into(),
            catalog_prices_usd_per_mtok: None,
            gold_patch_access: false,
            images: vec![],
            question_answer: "proceed".into(),
            approvals: "deny".into(),
        },
        environment: Environment {
            modbit_revision: "abc".into(),
            build_digest: "b".repeat(64),
            harness_version: modbit_bench_agent_engineering::HARNESS_VERSION.into(),
            toolchains: BTreeMap::new(),
            os: "test".into(),
        },
        metrics: metrics(&trials),
        trials,
        generated_at: "2026-11-07T00:00:00Z".into(),
    }
}

const GATE: Gate = Gate {
    max_drop: 0.05,
    per_task: false,
};

#[test]
fn qual_px_184_the_report_refuses_cost_not_derived_from_recorded_usage_and_a_gate_that_cannot_fail()
{
    let clean = synthetic(None);
    let r = report(&clean, &"1".repeat(64), GATE, 10).unwrap();
    for axis in AXES {
        assert!(r.axes[axis].verified.successes == 10 && r.axes[axis].verified.n == 10);
        assert_eq!(r.axes[axis].output_tokens, 1000);
        assert!((r.axes[axis].output_tokens_per_thousand_tasks - 100_000.0).abs() < 1e-9);
    }
    assert_eq!(r.cost.trials, 40);
    assert_eq!(r.cost.output_tokens, 4000);
    assert!((r.cost.output_tokens_per_thousand_tasks - 100_000.0).abs() < 1e-9);
    assert!((r.cost.cost_usd_per_thousand_tasks.unwrap() - 1.0).abs() < 1e-9);
    r.verify_against(&clean, &"1".repeat(64)).unwrap();

    // A cost figure not derived from recorded usage: no usage, usage the
    // event log does not confirm, or a report whose figure was edited.
    let mut no_usage = clean.clone();
    no_usage.trials[5].usage = None;
    assert!(matches!(
        report(&no_usage, "x", GATE, 10),
        Err(ReportRefused::NoRecordedUsage(_))
    ));
    let mut zero = clean.clone();
    zero.trials[5].usage.as_mut().unwrap().model_calls = 0;
    zero.trials[5].usage.as_mut().unwrap().recorded_model_calls = 0;
    assert!(matches!(
        report(&zero, "x", GATE, 10),
        Err(ReportRefused::NoRecordedUsage(_))
    ));
    let mut mismatch = clean.clone();
    mismatch.trials[7].usage.as_mut().unwrap().output_tokens = 1;
    assert!(matches!(
        report(&mismatch, "x", GATE, 10),
        Err(ReportRefused::UsageMismatch(_))
    ));
    let mut edited = r.clone();
    edited.cost.output_tokens_per_thousand_tasks = 1.0;
    assert!(matches!(
        edited.verify_against(&clean, &"1".repeat(64)),
        Err(ReportRefused::NotDerived(_))
    ));
    assert!(matches!(
        r.verify_against(&clean, &"2".repeat(64)),
        Err(ReportRefused::NotDerived(_))
    ));
    // A trial without its own verifier, an untagged trial, an axis below ten.
    let mut unverified = clean.clone();
    unverified.trials[0].verifier = None;
    assert!(matches!(
        report(&unverified, "x", GATE, 10),
        Err(ReportRefused::NoVerifier(_))
    ));
    let mut untagged = clean.clone();
    untagged.trials[0].axis = None;
    assert!(matches!(
        report(&untagged, "x", GATE, 10),
        Err(ReportRefused::Untagged(_))
    ));
    let mut nine = clean.clone();
    nine.trials.retain(|t| t.task != "memory/t9");
    assert!(matches!(
        report(&nine, "x", GATE, 3),
        Err(ReportRefused::TooFewTasks {
            found: 9,
            required: 10,
            ..
        })
    ));

    // A clean re-run: no difference, every gate passes.
    let again = report(&clean, &"3".repeat(64), GATE, 10).unwrap();
    let c = compare(&r, &again).unwrap();
    assert!(c.passed && c.failed_axes.is_empty(), "{c:?}");
    assert!(c.axes.iter().all(|a| a.delta.abs() < 1e-12));
    // An injected regression on one axis fails that axis's gate only.
    for axis in AXES {
        let bad = report(&synthetic(Some(axis)), &"4".repeat(64), GATE, 10).unwrap();
        let c = compare(&r, &bad).unwrap();
        assert!(!c.passed);
        assert_eq!(c.failed_axes, [axis], "{c:?}");
    }
    // One task in ten is a drop of 0.1: more than max_drop, so it fails too.
    let mut one = synthetic(None);
    one.trials[0].verified_success = false;
    let c = compare(&r, &report(&one, "5", GATE, 10).unwrap()).unwrap();
    assert_eq!(c.failed_axes, ["tool_use"]);

    // A gate that cannot fail is refused: a baseline axis at or below
    // max_drop leaves no rate under the floor, and max_drop >= 1 never fails.
    let mut dead = synthetic(None);
    for t in dead
        .trials
        .iter_mut()
        .filter(|t| t.axis.as_deref() == Some("skill_use"))
    {
        t.verified_success = false;
    }
    let dead_report = report(&dead, "6", GATE, 10).unwrap();
    assert!(matches!(
        compare(&dead_report, &r),
        Err(ReportRefused::GateCannotFail(w)) if w.contains("skill_use")
    ));
    let mut wide = r.clone();
    wide.gate = Gate {
        max_drop: 1.0,
        per_task: false,
    };
    let mut wide2 = again.clone();
    wide2.gate = wide.gate;
    assert!(matches!(
        compare(&wide, &wide2),
        Err(ReportRefused::GateCannotFail(_))
    ));
    // Different suites are not compared.
    let mut other = again;
    other.task_list_digest = "e".repeat(64);
    assert!(matches!(
        compare(&r, &other),
        Err(ReportRefused::DifferentSuite(_))
    ));
}

// ------------------------------------------------------------ the verifier

fn ev(offset: u64, event_type: &str, task: &str, payload: Value) -> String {
    json!({"offset": offset, "event_type": event_type, "aggregate_type": "task", "task_id": task, "payload": payload}).to_string()
}

#[test]
fn qual_px_184_a_verifier_reads_the_workspace_and_the_cores_records_never_the_models_claim() {
    let ws = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(ws.path().join("out")).unwrap();
    std::fs::write(ws.path().join("out/port.txt"), "8431\r\n").unwrap();
    let task = "aa11";
    let other = "bb22";
    let agent_doc = json!({"author": "agent:solver:aa11", "topic": "Port files", "record_type": "convention", "content": "A port file holds one number."}).to_string();
    let user_doc = json!({"author": "user:1", "topic": "lint cache", "record_type": "failure_pattern", "content": ".cache"}).to_string();
    let log = [
        ev(1, "MemoryUsed", task, json!({"memory_id": "m-used"})),
        ev(2, "MemoryUsed", other, json!({"memory_id": "m-other-task"})),
        ev(
            3,
            "MemoryProposed",
            task,
            json!({"row": {"doc": agent_doc}}),
        ),
        ev(4, "MemoryProposed", task, json!({"row": {"doc": user_doc}})),
        ev(
            5,
            "SkillSelected",
            task,
            json!({"name": "sel", "content_hash": "h-sel"}),
        ),
        ev(6, "SubagentAdmitted", task, json!({"child_task_id": "c1"})),
        ev(
            7,
            "SubagentResultRecorded",
            task,
            json!({"child_task_id": "c1"}),
        ),
        ev(
            8,
            "MergeTransactionRecorded",
            task,
            json!({"child_task_id": "c1", "state": "STAGED"}),
        ),
        ev(
            9,
            "MergeTransactionRecorded",
            task,
            json!({"child_task_id": "c1", "state": "COMMITTED"}),
        ),
        ev(
            10,
            "SubagentAdmitted",
            other,
            json!({"child_task_id": "c9"}),
        ),
        // The model's own claim is never read.
        ev(
            11,
            "TaskCompletionClaimed",
            task,
            json!({"summary": "skill wrong-hash used; memory m-forgotten used"}),
        ),
    ]
    .join("\n");
    let events = parse_events(&log);
    let tools = vec![
        ToolResultRecord {
            tool_name: "fs.read".into(),
            status: "SUCCESS".into(),
            structured_output: json!({"content": "4\n42\n"}),
        },
        ToolResultRecord {
            tool_name: "fs.read".into(),
            status: "FAILED".into(),
            structured_output: json!({"content": "secret"}),
        },
        ToolResultRecord {
            tool_name: "skill.load".into(),
            status: "SUCCESS".into(),
            structured_output: json!({"name": "loaded", "content_hash": "h-loaded"}),
        },
        ToolResultRecord {
            tool_name: "skill.load".into(),
            status: "SUCCESS".into(),
            structured_output: json!({"name": "wrong-hash", "content_hash": "tampered"}),
        },
        ToolResultRecord {
            tool_name: "memory.query".into(),
            status: "SUCCESS".into(),
            structured_output: json!({"items": [{"id": "m-queried"}]}),
        },
    ];
    let memory_ids: BTreeMap<String, String> = [
        ("used", "m-used"),
        ("queried", "m-queried"),
        ("forgotten", "m-forgotten"),
        ("other", "m-other-task"),
    ]
    .into_iter()
    .map(|(a, b)| (a.to_owned(), b.to_owned()))
    .collect();
    let skill_hashes: BTreeMap<String, String> = [
        ("sel", "h-sel"),
        ("loaded", "h-loaded"),
        ("wrong-hash", "h-reviewed"),
    ]
    .into_iter()
    .map(|(a, b)| (a.to_owned(), b.to_owned()))
    .collect();
    let o = Observed {
        events: &events,
        task,
        tool_results: &tools,
        workspace: ws.path(),
        memory_ids: &memory_ids,
        skill_hashes: &skill_hashes,
        desktop: None,
    };
    let good = Verifier {
        id: "good".into(),
        files: vec![FileCheck {
            path: "out/port.txt".into(),
            equals: Some("8431\n".into()),
            contains: vec![],
        }],
        tools: vec![ToolCheck {
            tool: "fs.read".into(),
            min: 1,
            output_contains: Some("42".into()),
        }],
        memory: Some(MemoryCheck {
            used: vec!["used".into(), "queried".into()],
            not_used: vec!["forgotten".into()],
            proposed: vec![ProposalCheck {
                topic: "port files".into(),
                record_type: Some("convention".into()),
                content_contains: Some("ONE NUMBER".into()),
            }],
        }),
        skills_used: vec!["sel".into(), "loaded".into()],
        subagents: Some(SubagentCheck {
            admitted: 1,
            results: 1,
            merged: 1,
        }),
        gui: None,
    };
    let out = evaluate(&good, &o);
    assert!(out.passed, "{:#?}", out.checks);
    assert_eq!(out.checks.len(), good.check_count());
    // Each failing observation fails its check.
    let cases: Vec<(&str, Verifier)> = vec![
        (
            "wrong content",
            Verifier {
                files: vec![FileCheck {
                    path: "out/port.txt".into(),
                    equals: Some("8432\n".into()),
                    contains: vec![],
                }],
                ..Verifier::default()
            },
        ),
        (
            "missing file",
            Verifier {
                files: vec![FileCheck {
                    path: "out/none.txt".into(),
                    equals: None,
                    contains: vec!["x".into()],
                }],
                ..Verifier::default()
            },
        ),
        (
            "failed tool output does not count",
            Verifier {
                tools: vec![ToolCheck {
                    tool: "fs.read".into(),
                    min: 1,
                    output_contains: Some("secret".into()),
                }],
                ..Verifier::default()
            },
        ),
        (
            "too few calls",
            Verifier {
                tools: vec![ToolCheck {
                    tool: "fs.read".into(),
                    min: 2,
                    output_contains: None,
                }],
                ..Verifier::default()
            },
        ),
        (
            "another task's memory",
            Verifier {
                memory: Some(MemoryCheck {
                    used: vec!["other".into()],
                    ..MemoryCheck::default()
                }),
                ..Verifier::default()
            },
        ),
        (
            "forgotten memory used",
            Verifier {
                memory: Some(MemoryCheck {
                    not_used: vec!["used".into()],
                    ..MemoryCheck::default()
                }),
                ..Verifier::default()
            },
        ),
        (
            "unseeded key",
            Verifier {
                memory: Some(MemoryCheck {
                    used: vec!["nobody".into()],
                    ..MemoryCheck::default()
                }),
                ..Verifier::default()
            },
        ),
        (
            "a person's proposal is not the agent's",
            Verifier {
                memory: Some(MemoryCheck {
                    proposed: vec![ProposalCheck {
                        topic: "lint cache".into(),
                        ..ProposalCheck::default()
                    }],
                    ..MemoryCheck::default()
                }),
                ..Verifier::default()
            },
        ),
        (
            "a skill loaded under other bytes",
            Verifier {
                skills_used: vec!["wrong-hash".into()],
                ..Verifier::default()
            },
        ),
        (
            "a merge only staged is not merged",
            Verifier {
                subagents: Some(SubagentCheck {
                    admitted: 2,
                    results: 1,
                    merged: 1,
                }),
                ..Verifier::default()
            },
        ),
    ];
    for (why, mut v) in cases {
        v.id = why.into();
        let out = evaluate(&v, &o);
        assert!(!out.passed, "{why}: {:#?}", out.checks);
    }
}

// ------------------------------------------------- the real Core, forty tasks

/// Calls whose absence is the injected regression: the agent stops using the
/// capability its axis measures.
const CAPABILITY_CALLS: &[&str] = &[
    "memory.query",
    "memory.propose",
    "skill.load",
    "agent.spawn",
    "agent.wait",
    "git.merge.prepare",
    "git.merge.commit",
];

/// The regressed trajectory: the capability calls dropped and every written
/// content replaced by a stale value, as a model that stopped recalling,
/// loading or delegating would write.
fn regressed(script: &[Value]) -> Vec<Value> {
    script
        .iter()
        .filter(|step| {
            !step["calls"].as_array().is_some_and(|c| {
                c.iter()
                    .any(|c| CAPABILITY_CALLS.contains(&c["name"].as_str().unwrap_or("")))
            })
        })
        .map(|step| {
            let mut s = step.clone();
            for c in s["calls"].as_array_mut().into_iter().flatten() {
                let args = &mut c["args"];
                if args.get("content").is_some() {
                    args["content"] = json!("stale\n");
                }
                for e in args["text_edits"].as_array_mut().into_iter().flatten() {
                    e["new"] = json!("stale\n");
                }
            }
            s
        })
        .collect()
}

/// The scripted model: every request is answered from the trajectory whose
/// goal is the request's `Task goal:` line (a parent's or a child's), one
/// step per assistant turn already in the conversation.
struct Model {
    base: String,
    regress: Arc<Mutex<Option<String>>>,
}

fn goal_of(body: &Value) -> Option<String> {
    body["messages"].as_array()?.iter().find_map(|m| {
        let text = m["content"].as_str()?;
        let rest = text.strip_prefix("Task goal: ")?;
        Some(rest.split("\n\n").next().unwrap_or(rest).trim().to_owned())
    })
}

fn serve(scripts: &Value, suite: &Suite) -> Model {
    // goal → (axis, trajectory)
    let mut by_goal: BTreeMap<String, (String, Vec<Value>)> = BTreeMap::new();
    for t in &suite.tasks {
        let entry = &scripts["tasks"][&t.id];
        let axis = t.axis.clone().unwrap_or_default();
        by_goal.insert(
            t.goal.clone(),
            (
                axis.clone(),
                entry["script"].as_array().cloned().unwrap_or_default(),
            ),
        );
        for c in entry["children"].as_array().into_iter().flatten() {
            by_goal.insert(
                c["goal"].as_str().unwrap().to_owned(),
                (
                    axis.clone(),
                    c["script"].as_array().cloned().unwrap_or_default(),
                ),
            );
        }
    }
    let by_goal = Arc::new(by_goal);
    let regress: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let reg = Arc::clone(&regress);
    std::thread::spawn(move || {
        for sock in listener.incoming() {
            let Ok(mut sock) = sock else { return };
            let by_goal = Arc::clone(&by_goal);
            let reg = Arc::clone(&reg);
            std::thread::spawn(move || {
                let mut buf = Vec::new();
                let mut tmp = [0u8; 8192];
                let (head_end, len) = loop {
                    let n = sock.read(&mut tmp).unwrap_or(0);
                    if n == 0 {
                        return;
                    }
                    buf.extend_from_slice(&tmp[..n]);
                    if let Some(i) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
                        let head = String::from_utf8_lossy(&buf[..i]).to_string();
                        let len = head
                            .lines()
                            .find_map(|l| {
                                let (k, v) = l.split_once(':')?;
                                k.eq_ignore_ascii_case("content-length")
                                    .then(|| v.trim().parse::<usize>().ok())
                                    .flatten()
                            })
                            .unwrap_or(0);
                        break (i + 4, len);
                    }
                };
                while buf.len() < head_end + len {
                    let n = sock.read(&mut tmp).unwrap_or(0);
                    if n == 0 {
                        break;
                    }
                    buf.extend_from_slice(&tmp[..n]);
                }
                let body: Value =
                    serde_json::from_slice(&buf[head_end..head_end + len]).unwrap_or_default();
                let turn = body["messages"]
                    .as_array()
                    .map(|m| m.iter().filter(|x| x["role"] == "assistant").count())
                    .unwrap_or(0);
                let regressed_axis = reg.lock().unwrap().clone();
                let reply = goal_of(&body)
                    .and_then(|g| by_goal.get(&g).cloned())
                    .and_then(|(axis, script)| {
                        let script = if regressed_axis.as_deref() == Some(axis.as_str()) {
                            regressed(&script)
                        } else {
                            script
                        };
                        script.get(turn).cloned()
                    })
                    .unwrap_or_else(|| json!({"text": "I have nothing further to do."}));
                let mut frames: Vec<String> = Vec::new();
                if let Some(t) = reply["text"].as_str() {
                    frames.push(json!({"id":"c","model":"scripted-1","choices":[{"index":0,"delta":{"content":t},"finish_reason":null}]}).to_string());
                }
                let calls = reply["calls"].as_array().cloned().unwrap_or_default();
                for (i, c) in calls.iter().enumerate() {
                    frames.push(json!({"id":"c","model":"scripted-1","choices":[{"index":0,"delta":{"tool_calls":[{"index":i,"id":format!("call_{turn}_{i}"),"type":"function","function":{"name":c["name"],"arguments":c["args"].to_string()}}]},"finish_reason":null}]}).to_string());
                }
                let finish = if calls.is_empty() {
                    "stop"
                } else {
                    "tool_calls"
                };
                let prompt_tokens = body["messages"].to_string().len().div_ceil(4);
                frames.push(json!({"id":"c","model":"scripted-1","choices":[{"index":0,"delta":{},"finish_reason":finish}],"usage":{"prompt_tokens":prompt_tokens,"completion_tokens":32}}).to_string());
                frames.push("[DONE]".into());
                let mut payload = String::new();
                for f in frames {
                    payload.push_str(&format!("data: {f}\n\n"));
                }
                let _ = sock.write_all(
                    format!(
                        "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\nx-request-id: req_{turn}\r\nconnection: close\r\ncontent-length: {}\r\n\r\n{payload}",
                        payload.len()
                    )
                    .as_bytes(),
                );
                let _ = sock.flush();
                let _ = sock.shutdown(std::net::Shutdown::Both);
            });
        }
    });
    Model {
        base: format!("http://127.0.0.1:{port}"),
        regress,
    }
}

fn product_binaries() -> (PathBuf, PathBuf) {
    let dir = root().join("target/debug");
    let cli = dir.join(format!("modbit-cli{}", std::env::consts::EXE_SUFFIX));
    let core = dir.join(format!("modbit-core{}", std::env::consts::EXE_SUFFIX));
    let execd = dir.join(format!("modbit-execd{}", std::env::consts::EXE_SUFFIX));
    if !cli.is_file() || !core.is_file() || !execd.is_file() {
        let st = Command::new("cargo")
            .args([
                "build",
                "-p",
                "modbit-cli",
                "-p",
                "modbit-core",
                "-p",
                "modbit-execd",
            ])
            .current_dir(root())
            .status()
            .unwrap();
        assert!(st.success(), "building the product binaries");
    }
    (cli, core)
}

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_competence-baseline")
}

/// Run the whole suite once; the bundle directory and the binary's output.
fn run_suite(base: &str, out: &Path) -> (i32, String) {
    let (cli, core) = product_binaries();
    let o = Command::new(bin())
        .current_dir(root())
        .args([
            "--suite",
            suite_dir().join("tasks.json").to_str().unwrap(),
            "--fixtures",
            suite_dir().join("fixtures").to_str().unwrap(),
            "--out",
            out.to_str().unwrap(),
            "--endpoint",
            "openai",
            "--model",
            "gpt-5-mini",
            "--trials",
            "1",
            "--jobs",
            "4",
            "--trial-timeout-secs",
            "300",
            "--cli",
            cli.to_str().unwrap(),
            "--core",
            core.to_str().unwrap(),
        ])
        .env("MODBIT_OPENAI_BASE_URL", base)
        .env("OPENAI_API_KEY", "")
        .env("ANTHROPIC_API_KEY", "")
        .env_remove("MODBIT_OPENAI_MODELS")
        .env_remove("MODBIT_LIVE_MODEL")
        .output()
        .unwrap();
    (
        o.status.code().unwrap_or(-1),
        format!(
            "{}{}",
            String::from_utf8_lossy(&o.stdout),
            String::from_utf8_lossy(&o.stderr)
        ),
    )
}

fn run_compare(baseline: &Path, current: &Path) -> (i32, String, Value) {
    let out = current.join("comparison.json");
    let o = Command::new(bin())
        .args([
            "compare",
            "--baseline",
            baseline.to_str().unwrap(),
            "--current",
            current.to_str().unwrap(),
            "--out",
            out.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&o.stdout),
        String::from_utf8_lossy(&o.stderr)
    );
    let cmp = std::fs::read_to_string(&out)
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or(Value::Null);
    (o.status.code().unwrap_or(-1), text, cmp)
}

fn load_bundle(dir: &Path) -> (Bundle, axes::AxisReport) {
    let bundle: Bundle =
        serde_json::from_str(&std::fs::read_to_string(dir.join("baseline.json")).unwrap()).unwrap();
    let report: axes::AxisReport =
        serde_json::from_str(&std::fs::read_to_string(dir.join("axes.json")).unwrap()).unwrap();
    (bundle, report)
}

/// The failing trials of a run, with their verifier checks and decisive log
/// lines, for an assertion message.
fn failures(dir: &Path, bundle: &Bundle) -> String {
    let mut s = String::new();
    for t in bundle.trials.iter().filter(|t| !t.verified_success) {
        let slug = t.task.replace('/', "__");
        let acc = std::fs::read_to_string(dir.join(format!("trials/{slug}-1/acceptance.log")))
            .unwrap_or_default();
        let cli = std::fs::read_to_string(dir.join(format!("trials/{slug}-1/cli.log")))
            .unwrap_or_default();
        let tail: String = cli
            .lines()
            .rev()
            .take(25)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect::<Vec<_>>()
            .join("\n");
        s.push_str(&format!(
            "--- {} state={} core_verified={}\n{acc}\n{tail}\n",
            t.task, t.state, t.core_verified
        ));
    }
    s
}

#[test]
#[allow(clippy::too_many_lines)]
fn qual_px_184_forty_tasks_through_the_real_core_a_rerun_matches_the_baseline_and_a_memory_regression_fails_only_memory()
 {
    let (suite, digest) = Suite::load(&suite_dir().join("tasks.json")).unwrap();
    let scripts: Value = serde_json::from_str(
        &std::fs::read_to_string(suite_dir().join("scripted-model.json")).unwrap(),
    )
    .unwrap();
    let model = serve(&scripts, &suite);
    let tmp = tempfile::tempdir().unwrap();

    // 1. The whole suite, forty tasks, through the real CLI and Core.
    let clean = tmp.path().join("clean");
    let (code, text) = run_suite(&model.base, &clean);
    let (bundle, rep) = load_bundle(&clean);
    assert_eq!(code, 0, "{text}\n{}", failures(&clean, &bundle));
    bundle.validate().unwrap();
    assert_eq!(bundle.task_list_digest, digest);
    assert_eq!(bundle.trials.len(), suite.tasks.len());
    assert!(bundle.trials.len() >= 40);
    for t in &bundle.trials {
        assert!(t.axis.is_some() && t.verifier.is_some(), "{}", t.task);
    }
    // Each axis has a score, and with the scripted model every task verifies.
    for axis in AXES {
        let a = &rep.axes[axis];
        assert!(a.tasks.len() >= 10, "{axis}");
        assert_eq!(
            (a.verified.successes, a.verified.n),
            (10, 10),
            "{axis}\n{}",
            failures(&clean, &bundle)
        );
    }
    // The cost per thousand tasks is computed from recorded usage: each
    // trial's economics (task and children) equal its log's
    // ModelUsageRecorded events, and the figure is their sum per trial.
    let bytes = std::fs::read(clean.join("baseline.json")).unwrap();
    let file_digest = modbit_bench_agent_engineering::digest_of(&bytes);
    rep.verify_against(&bundle, &file_digest).unwrap();
    let mut out_tokens = 0u64;
    for t in &bundle.trials {
        let u = t.usage.as_ref().unwrap();
        assert!(u.model_calls > 0 && u.output_tokens > 0, "{}", t.task);
        let log = std::fs::read_to_string(clean.join(&t.event_log.file)).unwrap();
        let (calls, out) = recorded_usage(&parse_events(&log));
        assert_eq!((calls, out), (u.model_calls, u.output_tokens), "{}", t.task);
        // The scripted model reports 32 completion tokens per call.
        assert_eq!(u.output_tokens, 32 * u64::from(u.model_calls), "{}", t.task);
        if t.axis.as_deref() == Some("subagent") {
            assert!(u.tasks.len() >= 2, "{}: children's usage counted", t.task);
        }
        out_tokens += u.output_tokens;
    }
    #[allow(clippy::cast_precision_loss)]
    let per_thousand = out_tokens as f64 * 1000.0 / bundle.trials.len() as f64;
    assert!((rep.cost.output_tokens_per_thousand_tasks - per_thousand).abs() < 1e-6);
    assert!(
        std::fs::read_to_string(clean.join("summary.md"))
            .unwrap()
            .contains("## Four axes (PX-184)")
    );

    // The retained baseline is refreshed from this run on request only
    // (`PX184_RETAIN_BASELINE=1`), when the suite or harness changed.
    if std::env::var("PX184_RETAIN_BASELINE").as_deref() == Ok("1") {
        std::fs::create_dir_all(baseline_dir()).unwrap();
        for f in [
            "baseline.json",
            "baseline.sha256",
            "axes.json",
            "summary.md",
        ] {
            std::fs::copy(clean.join(f), baseline_dir().join(f)).unwrap();
        }
    }

    // 2. The re-run against the retained baseline reports the difference:
    //    none on any axis, every gate passes.
    let (code, text, cmp) = run_compare(&baseline_dir(), &clean);
    assert_eq!(code, 0, "{text}");
    assert_eq!(cmp["passed"], true, "{cmp:#}");
    for a in cmp["axes"].as_array().unwrap() {
        assert_eq!(a["delta"].as_f64(), Some(0.0), "{a}");
        assert_eq!(a["passed"], true, "{a}");
    }
    assert!(
        cmp["output_tokens_per_thousand_tasks"][2].is_number(),
        "{cmp:#}"
    );

    // 3. An injected regression on the memory axis: the same Core, fixtures
    //    and suite; the model stops recalling and writes stale values.
    *model.regress.lock().unwrap() = Some("memory".into());
    let regressed_dir = tmp.path().join("regressed");
    let (code, text) = run_suite(&model.base, &regressed_dir);
    assert_eq!(code, 0, "the bundle and report are still written: {text}");
    let (bundle, rep) = load_bundle(&regressed_dir);
    assert_eq!(
        rep.axes["memory"].verified.successes, 0,
        "{:#?}",
        rep.axes["memory"]
    );
    for axis in ["tool_use", "skill_use", "subagent"] {
        assert_eq!(
            rep.axes[axis].verified.successes,
            10,
            "{axis}\n{}",
            failures(&regressed_dir, &bundle)
        );
    }
    // The Core may still call a memory task done; its own verifier does not.
    let core_done = bundle
        .trials
        .iter()
        .filter(|t| t.axis.as_deref() == Some("memory") && t.core_verified)
        .count();
    assert!(
        core_done > 0,
        "the regression is caught by the verifiers, not by the Core"
    );
    let (code, text, cmp) = run_compare(&baseline_dir(), &regressed_dir);
    assert_eq!(code, 3, "a failed gate exits 3: {text}");
    assert_eq!(cmp["passed"], false);
    assert_eq!(cmp["failed_axes"], json!(["memory"]), "{cmp:#}");
    for a in cmp["axes"].as_array().unwrap() {
        assert_eq!(a["passed"], a["axis"] != "memory", "{a}");
    }
    assert!(text.contains("gate failed for: memory"), "{text}");
}

#[test]
fn qual_px_184_the_retained_baseline_is_a_valid_four_axis_bundle_of_this_suite() {
    let (_, digest) = Suite::load(&suite_dir().join("tasks.json")).unwrap();
    let bytes = std::fs::read(baseline_dir().join("baseline.json")).unwrap();
    let file_digest = modbit_bench_agent_engineering::digest_of(&bytes);
    let recorded = std::fs::read_to_string(baseline_dir().join("baseline.sha256")).unwrap();
    assert!(recorded.starts_with(&file_digest), "{recorded}");
    let bundle: Bundle = serde_json::from_slice(&bytes).unwrap();
    bundle.validate().unwrap();
    assert_eq!(
        bundle.task_list_digest, digest,
        "the suite changed: refresh the baseline with PX184_RETAIN_BASELINE=1"
    );
    let rep: axes::AxisReport =
        serde_json::from_str(&std::fs::read_to_string(baseline_dir().join("axes.json")).unwrap())
            .unwrap();
    rep.verify_against(&bundle, &file_digest).unwrap();
    for axis in AXES {
        assert!(rep.axes[axis].verified.n >= 10, "{axis}");
        assert!(rep.gate.can_fail(rep.axes[axis].verified.rate), "{axis}");
    }
    assert!(rep.cost.output_tokens_per_thousand_tasks > 0.0);
}
