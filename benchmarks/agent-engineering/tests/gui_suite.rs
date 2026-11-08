//! QUAL-PX-179 / PX-E2E-179: the GUI regression suite — Modbit-owned GUI
//! tasks on the fixture desktop, each verified by the GUI verifier row
//! (PX-178), with a retained baseline, a re-run that reports the difference
//! and a gate that fails on a regression, naming the tasks.
//!
//! The offline proofs cover the suite's shape (at least twenty tasks over two
//! axes, each with its own GUI verifier whose evidence does not hold before
//! the run), the GUI verifier reading the Core's records and the desktop
//! (never the model's claim, never an easier predicate) and the per-task
//! gate. The real-Core proof runs the `competence-baseline` binary over every
//! task against the real CLI, the real Core and the real fixture actuator
//! process with a scripted OpenAI-compatible model, compares the run with the
//! retained baseline (no difference), then injects a model regression in
//! three named tasks and shows the gate failing with exactly those tasks
//! named.

use std::collections::{BTreeMap, BTreeSet};
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};

use modbit_bench_agent_engineering::axes::{self, TrialUsage, recorded_usage};
use modbit_bench_agent_engineering::verifier::desktop_value;
use modbit_bench_agent_engineering::{
    APPROVE_COMPUTER_INPUTS, Bundle, Counts, DesktopCheck, Economics, Environment, EventLog, Gate,
    GuiCheck, Observed, Protocol, ReportRefused, Suite, TrialOutcome, Verifier, VerifierOutcome,
    compare, evaluate, metrics, parse_events, report_axes,
};
use serde_json::{Value, json};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap()
}

fn suite_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("suites/gui")
}

fn baseline_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("baselines/gui")
}

const AXES: [&str; 2] = ["gui_element", "gui_input"];

fn axis_names() -> Vec<String> {
    AXES.iter().map(|a| (*a).to_owned()).collect()
}

fn element_value(desktop: &Value, app: &str, name: &str) -> Option<String> {
    desktop_value(desktop, app, name)
}

/// The desktop file as the fixture actuator reads it, in the shape of its
/// `--state-out` file (applications, windows, elements with their values).
fn initial_state(desktop: &Value) -> Value {
    json!({"apps": desktop["apps"].as_array().unwrap().iter().map(|a| json!({
        "bundle_id": a["bundle_id"],
        "windows": a["windows"].as_array().unwrap().iter().map(|w| json!({
            "id": w["id"],
            "elements": w["elements"].as_array().unwrap().iter().map(|e| json!({
                "name": e["name"],
                "value": e["value"].as_str().unwrap_or(""),
            })).collect::<Vec<_>>(),
        })).collect::<Vec<_>>(),
    })).collect::<Vec<_>>()})
}

// ------------------------------------------------------------ the suite

#[test]
fn qual_px_179_twenty_four_gui_tasks_each_with_its_own_gui_verifier_that_fails_before_the_run() {
    let (suite, _) = Suite::load(&suite_dir().join("tasks.json")).unwrap();
    assert_eq!(suite.id, "gui-regression");
    assert!(suite.tasks.len() >= 20, "{}", suite.tasks.len());
    assert_eq!(suite.protocol.approvals, APPROVE_COMPUTER_INPUTS);
    let axes = suite.protocol.axes.as_ref().unwrap();
    assert_eq!(axes.axis_names(), axis_names());
    assert!(axes.gate.per_task, "a regression in any one task fails");
    for axis in AXES {
        let n = suite
            .tasks
            .iter()
            .filter(|t| t.axis.as_deref() == Some(axis))
            .count();
        assert!(n >= 10, "{axis}: {n}");
    }
    let scripts: Value = serde_json::from_str(
        &std::fs::read_to_string(suite_dir().join("scripted-model.json")).unwrap(),
    )
    .unwrap();
    for t in &suite.tasks {
        let g = t.gui.as_ref().unwrap();
        let v = t.verifier.as_ref().unwrap().gui.as_ref().unwrap();
        let desktop: Value =
            serde_json::from_str(&std::fs::read_to_string(suite_dir().join(&g.desktop)).unwrap())
                .unwrap();
        let state = initial_state(&desktop);
        // The evidence is a predicate the Core can evaluate, on an
        // application of the task's own desktop, and it does not hold before
        // the run: a verifier that passes without the effect is refused.
        assert_eq!(v.evidence["kind"], "accessibility", "{}", t.id);
        let app = v.evidence["app"].as_str().unwrap();
        let name = v.evidence["name"].as_str().unwrap();
        let before = element_value(&state, app, name)
            .unwrap_or_else(|| panic!("{}: no element {name} of {app} on its desktop", t.id));
        assert_ne!(
            before, v.evidence["value"],
            "{}: the evidence already holds before the run",
            t.id
        );
        assert!(!v.desktop.is_empty(), "{}", t.id);
        assert!(
            v.desktop
                .iter()
                .any(|d| element_value(&state, &d.app, &d.name).as_deref() != Some(&d.value)),
            "{}: every desktop check already holds before the run",
            t.id
        );
        for d in &v.desktop {
            assert!(
                element_value(&state, &d.app, &d.name).is_some(),
                "{}: {} {} is not on the desktop",
                t.id,
                d.app,
                d.name
            );
        }
        // The scripted model has the parent's and the child's trajectories,
        // and the parent declares exactly the suite's evidence.
        let s = &scripts["tasks"][&t.id];
        assert_eq!(s["goal"], t.goal.as_str(), "{}", t.id);
        let spawn = s["script"]
            .as_array()
            .unwrap()
            .iter()
            .flat_map(|step| step["calls"].as_array().unwrap())
            .find(|c| c["name"] == "agent.spawn")
            .unwrap();
        assert_eq!(spawn["args"]["completion_evidence"], v.evidence, "{}", t.id);
        assert_eq!(spawn["args"]["profile"], "computer-use", "{}", t.id);
        assert_eq!(s["children"][0]["goal"], spawn["args"]["objective"]);
    }

    // Negative proof: a GUI task without a GUI verifier, a GUI task under the
    // deny-everything protocol, a GUI verifier on a non-GUI task, a desktop
    // outside the suite, an axis the suite does not declare, a non-predicate
    // as evidence, and a gate that cannot fail are refused.
    let mut no_gui_verifier = suite.clone();
    no_gui_verifier.tasks[0].verifier.as_mut().unwrap().gui = None;
    let e = no_gui_verifier.check().unwrap_err();
    assert!(
        e.contains("needs a gui verifier") || e.contains("checks nothing"),
        "{e}"
    );
    let mut deny = suite.clone();
    deny.protocol.approvals = "deny".into();
    assert!(deny.check().unwrap_err().contains("approvals protocol"));
    let mut not_gui = suite.clone();
    not_gui.tasks[1].gui = None;
    assert!(
        not_gui
            .check()
            .unwrap_err()
            .contains("a gui verifier needs a GUI task")
    );
    let mut escape = suite.clone();
    escape.tasks[2].gui.as_mut().unwrap().desktop = "../four-axis/tasks.json".into();
    assert!(escape.check().unwrap_err().contains("not inside the suite"));
    let mut unknown = suite.clone();
    unknown.tasks[3].axis = Some("tool_use".into());
    assert!(unknown.check().unwrap_err().contains("not one of"));
    let mut not_predicate = suite.clone();
    not_predicate.tasks[4]
        .verifier
        .as_mut()
        .unwrap()
        .gui
        .as_mut()
        .unwrap()
        .evidence = json!({"kind": "summary", "says": "done"});
    assert!(
        not_predicate
            .check()
            .unwrap_err()
            .contains("not a completion-evidence predicate")
    );
    let mut nine = suite.clone();
    let mut dropped = 0;
    nine.tasks.retain(|t| {
        let drop = t.axis.as_deref() == Some("gui_input") && dropped < 3;
        dropped += usize::from(drop);
        !drop
    });
    assert!(nine.check().unwrap_err().contains("axis gui_input has 9"));
    let mut never = suite;
    never.protocol.axes.as_mut().unwrap().gate.max_drop = 1.0;
    assert!(never.check().unwrap_err().contains("cannot fail"));
}

// ------------------------------------------------------------ the verifier

fn ev(offset: u64, event_type: &str, task: &str, payload: Value) -> String {
    json!({"offset": offset, "event_type": event_type, "aggregate_type": "task", "task_id": task, "payload": payload}).to_string()
}

const FORM: &str = "org.modbit.bench.orders";

fn evidence() -> Value {
    json!({"kind": "accessibility", "app": FORM, "role": "label", "name": "Status", "value": "submitted"})
}

fn state_with(status: &str) -> Value {
    json!({"apps": [{"bundle_id": FORM, "windows": [{"id": "w", "elements": [
        {"name": "Customer", "value": "Ada"},
        {"name": "Status", "value": status},
    ]}]}]})
}

#[test]
fn qual_px_179_the_gui_verifier_reads_the_cores_records_and_the_desktop_never_the_models_claim() {
    let parent = "aaaa";
    let child = "bbbb-cc";
    let log = |eval_evidence: Value, status: &str, verdict: &str, eval_task: &str| -> String {
        [
            ev(
                1,
                "SubagentAdmitted",
                parent,
                json!({"child_task_id": child}),
            ),
            ev(
                2,
                "TaskCompleted",
                child,
                json!({"summary": "The order is submitted; Status reads submitted."}),
            ),
            ev(
                3,
                "GuiEvidenceEvaluated",
                eval_task,
                json!({"evidence": eval_evidence, "status": status, "detail": "d", "read": "state read of org.modbit.bench.orders", "at": "COMPLETION"}),
            ),
            ev(
                4,
                "GuiOutcomeJudged",
                eval_task,
                json!({"run_id": "r", "verdict": verdict, "evidence_status": status, "basis": [], "run_end": "READY_FOR_REVIEW"}),
            ),
        ]
        .join("\n")
    };
    let v = Verifier {
        id: "px179/v".into(),
        gui: Some(GuiCheck {
            evidence: evidence(),
            desktop: vec![
                DesktopCheck {
                    app: FORM.into(),
                    name: "Status".into(),
                    value: "submitted".into(),
                },
                DesktopCheck {
                    app: FORM.into(),
                    name: "Customer".into(),
                    value: "Ada".into(),
                },
            ],
        }),
        ..Verifier::default()
    };
    let ws = tempfile::tempdir().unwrap();
    let empty = BTreeMap::new();
    let run = |events: &str, desktop: Option<&Value>| {
        let events = parse_events(events);
        evaluate(
            &v,
            &Observed {
                events: &events,
                task: parent,
                tool_results: &[],
                workspace: ws.path(),
                memory_ids: &empty,
                skill_hashes: &empty,
                desktop,
            },
        )
    };
    let done = state_with("submitted");
    let good = run(&log(evidence(), "PASS", "VERIFIED", child), Some(&done));
    assert!(good.passed, "{:#?}", good.checks);
    assert_eq!(good.checks.len(), v.check_count());

    // Each failing observation fails the trial.
    let easier = json!({"kind": "accessibility", "app": FORM, "name": "Customer", "value": "Ada"});
    let cases: Vec<(&str, String, Option<Value>)> = vec![
        (
            "the child was held to an easier predicate",
            log(easier, "PASS", "VERIFIED", child),
            Some(done.clone()),
        ),
        (
            "the evidence failed (the model claimed it was done)",
            log(evidence(), "FAIL", "MODEL_FAILURE", child),
            Some(done.clone()),
        ),
        (
            "the Core judged an environment failure",
            log(evidence(), "PASS", "ENVIRONMENT_FAILURE", child),
            Some(done.clone()),
        ),
        (
            "the evaluation was of a task that is not this task's child",
            log(evidence(), "PASS", "VERIFIED", "dddd"),
            Some(done.clone()),
        ),
        (
            "the desktop does not hold the effect",
            log(evidence(), "PASS", "VERIFIED", child),
            Some(state_with("idle")),
        ),
        (
            "no desktop state was recorded",
            log(evidence(), "PASS", "VERIFIED", child),
            None,
        ),
        (
            "only the model's summary",
            [
                ev(
                    1,
                    "SubagentAdmitted",
                    parent,
                    json!({"child_task_id": child}),
                ),
                ev(
                    2,
                    "TaskCompleted",
                    child,
                    json!({"summary": "Status reads submitted"}),
                ),
            ]
            .join("\n"),
            Some(done.clone()),
        ),
    ];
    for (what, log, desktop) in cases {
        let out = run(&log, desktop.as_ref());
        assert!(!out.passed, "{what}: {:#?}", out.checks);
    }
    // An ambiguous element name on the desktop is no value.
    let twice = json!({"apps": [{"bundle_id": FORM, "windows": [
        {"id": "a", "elements": [{"name": "Status", "value": "submitted"}]},
        {"id": "b", "elements": [{"name": "Status", "value": "idle"}]},
    ]}]});
    assert_eq!(desktop_value(&twice, FORM, "Status"), None);
}

// ------------------------------------------------------------ the gate

fn trial(task: &str, axis: &str, verified: bool) -> TrialOutcome {
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
            tasks: vec!["t".into(), "c".into()],
            model_calls: 10,
            input_tokens: 1000,
            output_tokens: 320,
            cost_usd: None,
            recorded_model_calls: 10,
            recorded_output_tokens: 320,
        }),
    }
}

fn synthetic(failing: &[&str]) -> Bundle {
    let mut trials = Vec::new();
    for axis in AXES {
        for n in 0..12 {
            let task = format!("{axis}/t{n:02}");
            let ok = !failing.contains(&task.as_str());
            trials.push(trial(&task, axis, ok));
        }
    }
    Bundle {
        schema: modbit_bench_agent_engineering::bundle::SCHEMA.into(),
        suite_id: "gui-regression".into(),
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
            approvals: APPROVE_COMPUTER_INPUTS.into(),
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
        generated_at: "2026-11-08T00:00:00Z".into(),
    }
}

#[test]
fn qual_px_179_the_per_task_gate_names_every_regressed_task_and_cannot_be_made_unfailable() {
    // A lenient axis floor (0.1) that one failing task in twelve stays above:
    // the per-task gate still fails, and names the task.
    let gate = Gate {
        max_drop: 0.1,
        per_task: true,
    };
    let rep = |b: &Bundle, d: &str| report_axes(b, d, gate, 10, &axis_names()).unwrap();
    let base = synthetic(&[]);
    let b = rep(&base, "1");
    assert_eq!(b.axes["gui_element"].task_results.len(), 12);
    b.verify_against(&base, "1").unwrap();
    let same = compare(&b, &rep(&base, "2")).unwrap();
    assert!(same.passed && same.regressed_tasks.is_empty() && same.failed_axes.is_empty());

    let one = synthetic(&["gui_input/t07"]);
    let c = rep(&one, "3");
    let cmp = compare(&b, &c).unwrap();
    assert!(
        cmp.failed_axes.is_empty(),
        "the axis stays above its floor: {cmp:#?}"
    );
    assert_eq!(cmp.regressed_tasks, vec!["gui_input/t07".to_owned()]);
    assert!(!cmp.passed);
    let text = axes::render_comparison(&cmp);
    assert!(text.contains("regressed tasks: gui_input/t07"), "{text}");
    assert!(
        text.contains("gate failed for: task gui_input/t07"),
        "{text}"
    );

    // Without the per-task gate the same run passes: the per-task gate is
    // what catches it.
    let axis_only = Gate {
        max_drop: 0.1,
        per_task: false,
    };
    let b2 = report_axes(&base, "1", axis_only, 10, &axis_names()).unwrap();
    let c2 = report_axes(&one, "3", axis_only, 10, &axis_names()).unwrap();
    assert!(compare(&b2, &c2).unwrap().passed);

    // A task that verifies now and did not in the baseline is named as
    // recovered and fails nothing.
    let worse_base = synthetic(&["gui_element/t03"]);
    let wb = rep(&worse_base, "4");
    let rec = compare(&wb, &b).unwrap();
    assert!(rec.passed, "{rec:#?}");
    assert_eq!(rec.recovered_tasks, vec!["gui_element/t03".to_owned()]);

    // A report whose per-task results were edited is not derived from its bundle.
    let mut forged = rep(&one, "3");
    forged
        .axes
        .get_mut("gui_input")
        .unwrap()
        .task_results
        .insert("gui_input/t07".into(), (1, 1));
    assert!(matches!(
        forged.verify_against(&one, "3"),
        Err(ReportRefused::NotDerived(_))
    ));
    // A trial on an axis the suite does not declare is refused.
    let mut stray = base.clone();
    stray.trials[0].axis = Some("tool_use".into());
    assert!(matches!(
        report_axes(&stray, "5", gate, 10, &axis_names()),
        Err(ReportRefused::UnknownAxis(_))
    ));
    // A baseline in which nothing verified leaves nothing to regress: refused.
    let all_failed: Vec<String> = base.trials.iter().map(|t| t.task.clone()).collect();
    let refs: Vec<&str> = all_failed.iter().map(String::as_str).collect();
    let zero = synthetic(&refs);
    let z = report_axes(
        &zero,
        "6",
        Gate {
            max_drop: 0.0,
            per_task: true,
        },
        10,
        &axis_names(),
    )
    .unwrap();
    assert!(matches!(
        compare(&z, &z),
        Err(ReportRefused::GateCannotFail(_))
    ));
}

// ------------------------------------------------------------ the scripted model

/// The calls that put a task's effect into the application.
const INPUT_CALLS: [&str; 5] = [
    "computer.press",
    "computer.set_value",
    "computer.type",
    "computer.key",
    "computer.click",
];

/// The regressed trajectory: the model stops before the input that has the
/// task's effect and still claims it is done.
fn regressed(script: &[Value]) -> Vec<Value> {
    let last_input = script.iter().rposition(|step| {
        step["calls"].as_array().is_some_and(|c| {
            c.iter()
                .any(|c| INPUT_CALLS.contains(&c["name"].as_str().unwrap_or("")))
        })
    });
    script
        .iter()
        .enumerate()
        .filter(|(i, _)| Some(*i) != last_input)
        .map(|(_, s)| s.clone())
        .collect()
}

/// What the agent has been shown so far: the latest tree and frame token.
#[derive(Default)]
struct Shown {
    tree: String,
    frame: String,
    last: String,
    waits: usize,
    agent_id: String,
}

fn shown(body: &Value) -> Shown {
    let mut s = Shown::default();
    for m in body["messages"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|m| m["role"] == "tool")
    {
        let text = m["content"].as_str().unwrap_or_default().to_owned();
        if text.contains("not done within") {
            s.waits += 1;
        }
        if let Some(id) = text.lines().find_map(|l| l.strip_prefix("agent_id: ")) {
            id.trim().clone_into(&mut s.agent_id);
        }
        if let Some(j) = text.find('{').and_then(|i| {
            serde_json::Deserializer::from_str(&text[i..])
                .into_iter::<Value>()
                .next()
                .and_then(Result::ok)
        }) {
            if let Some(t) = j["tree"].as_str() {
                t.clone_into(&mut s.tree);
            }
            if let Some(f) = j["frame"].as_str() {
                f.clone_into(&mut s.frame);
            }
        }
        s.last = text;
    }
    s
}

/// The handle of the element named `name` in the latest tree.
fn handle(tree: &str, name: &str) -> String {
    let needle = format!("| \"{name}\" |");
    tree.lines()
        .find(|l| l.contains(&needle))
        .and_then(|l| l.split_whitespace().next())
        .unwrap_or("no-such-element")
        .to_owned()
}

/// Fill `{{el:<name>}}` and `{{frame}}` from what the agent was shown.
fn fill(v: &Value, s: &Shown) -> Value {
    match v {
        Value::String(t) => {
            if let Some(name) = t.strip_prefix("{{el:").and_then(|r| r.strip_suffix("}}")) {
                Value::String(handle(&s.tree, name))
            } else if t == "{{frame}}" {
                Value::String(s.frame.clone())
            } else {
                v.clone()
            }
        }
        Value::Array(a) => Value::Array(a.iter().map(|x| fill(x, s)).collect()),
        Value::Object(o) => Value::Object(o.iter().map(|(k, x)| (k.clone(), fill(x, s))).collect()),
        _ => v.clone(),
    }
}

struct Model {
    base: String,
    /// Task ids whose child trajectory is regressed.
    regress: Arc<Mutex<BTreeSet<String>>>,
}

fn goal_of(body: &Value) -> Option<String> {
    body["messages"].as_array()?.iter().find_map(|m| {
        let text = m["content"].as_str()?;
        let rest = text.strip_prefix("Task goal: ")?;
        Some(rest.split("\n\n").next().unwrap_or(rest).trim().to_owned())
    })
}

fn read_request(sock: &mut std::net::TcpStream) -> Option<Value> {
    let mut buf = Vec::new();
    let mut tmp = [0u8; 8192];
    let (head_end, len) = loop {
        let n = sock.read(&mut tmp).unwrap_or(0);
        if n == 0 {
            return None;
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
    serde_json::from_slice(&buf[head_end..(head_end + len).min(buf.len())]).ok()
}

fn serve(scripts: &Value) -> Model {
    // goal → (task id, is child, trajectory)
    let mut by_goal: BTreeMap<String, (String, bool, Vec<Value>)> = BTreeMap::new();
    for (id, entry) in scripts["tasks"].as_object().unwrap() {
        by_goal.insert(
            entry["goal"].as_str().unwrap().to_owned(),
            (
                id.clone(),
                false,
                entry["script"].as_array().cloned().unwrap_or_default(),
            ),
        );
        for c in entry["children"].as_array().into_iter().flatten() {
            by_goal.insert(
                c["goal"].as_str().unwrap().to_owned(),
                (
                    id.clone(),
                    true,
                    c["script"].as_array().cloned().unwrap_or_default(),
                ),
            );
        }
    }
    let by_goal = Arc::new(by_goal);
    let regress: Arc<Mutex<BTreeSet<String>>> = Arc::default();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let reg = Arc::clone(&regress);
    std::thread::spawn(move || {
        for sock in listener.incoming() {
            let Ok(mut sock) = sock else { return };
            let by_goal = Arc::clone(&by_goal);
            let reg = Arc::clone(&reg);
            std::thread::spawn(move || {
                let Some(body) = read_request(&mut sock) else {
                    return;
                };
                let turn = body["messages"]
                    .as_array()
                    .map(|m| m.iter().filter(|x| x["role"] == "assistant").count())
                    .unwrap_or(0);
                let s = shown(&body);
                let regressed_tasks = reg.lock().unwrap().clone();
                let reply = goal_of(&body)
                    .and_then(|g| by_goal.get(&g).cloned())
                    .and_then(|(id, child, script)| {
                        // A parent told its child is not done yet waits again;
                        // the wait is not a step of the script.
                        if !child && s.last.contains("not done within") {
                            return Some(json!({"calls": [{"name": "agent.wait", "args": {"agent_id": s.agent_id, "timeout_ms": 120_000}}]}));
                        }
                        let script = if child && regressed_tasks.contains(&id) {
                            regressed(&script)
                        } else {
                            script
                        };
                        let at = if child { turn } else { turn.saturating_sub(s.waits) };
                        script.get(at).map(|step| fill(step, &s))
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

// ------------------------------------------------------------ the real run

fn product_binaries() -> (PathBuf, PathBuf, PathBuf) {
    let dir = root().join("target/debug");
    let exe = |n: &str| dir.join(format!("{n}{}", std::env::consts::EXE_SUFFIX));
    let (cli, core, execd, actuator) = (
        exe("modbit-cli"),
        exe("modbit-core"),
        exe("modbit-execd"),
        exe("modbit-actuator-fixture"),
    );
    if !cli.is_file() || !core.is_file() || !execd.is_file() || !actuator.is_file() {
        let st = Command::new("cargo")
            .args([
                "build",
                "-p",
                "modbit-cli",
                "-p",
                "modbit-core",
                "-p",
                "modbit-execd",
                "-p",
                "modbit-actuator-fixture",
            ])
            .current_dir(root())
            .status()
            .unwrap();
        assert!(st.success(), "building the product binaries");
    }
    (cli, core, actuator)
}

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_competence-baseline")
}

/// Run the whole suite once; the binary's exit code and output.
fn run_suite(base: &str, out: &Path) -> (i32, String) {
    let (cli, core, actuator) = product_binaries();
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
            "--actuator",
            actuator.to_str().unwrap(),
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

/// The failing trials of a run, with their verifier checks and the end of
/// their CLI log, for an assertion message.
fn failures(dir: &Path, bundle: &Bundle) -> String {
    let mut s = String::new();
    for t in bundle.trials.iter().filter(|t| !t.verified_success) {
        let slug = t.task.replace('/', "__");
        let acc = std::fs::read_to_string(dir.join(format!("trials/{slug}-1/acceptance.log")))
            .unwrap_or_default();
        let cli = std::fs::read_to_string(dir.join(format!("trials/{slug}-1/cli.log")))
            .unwrap_or_default();
        let tail: Vec<&str> = cli.lines().rev().take(20).collect();
        s.push_str(&format!(
            "--- {} state={} core_verified={} notes={:?}\n{acc}\n{}\n",
            t.task,
            t.state,
            t.core_verified,
            t.notes,
            tail.into_iter().rev().collect::<Vec<_>>().join("\n")
        ));
    }
    s
}

/// The tasks the injected regression hits: two element tasks and one input task.
const REGRESSED: [&str; 3] = [
    "gui_element/files-archive-confirm",
    "gui_element/orders-submit-for-ada",
    "gui_input/notes-save-shortcut",
];

#[test]
#[allow(clippy::too_many_lines)]
fn qual_px_179_gui_tasks_through_the_real_core_and_fixture_actuator_a_rerun_matches_the_baseline_and_a_regression_fails_naming_the_tasks()
 {
    let (suite, digest) = Suite::load(&suite_dir().join("tasks.json")).unwrap();
    let scripts: Value = serde_json::from_str(
        &std::fs::read_to_string(suite_dir().join("scripted-model.json")).unwrap(),
    )
    .unwrap();
    let model = serve(&scripts);
    let tmp = tempfile::tempdir().unwrap();

    // 1. The whole suite through the real CLI, Core and fixture actuator.
    let clean = tmp.path().join("clean");
    let (code, text) = run_suite(&model.base, &clean);
    assert!(
        clean.join("baseline.json").is_file(),
        "no bundle was written: {text}"
    );
    let (bundle, rep) = load_bundle(&clean);
    assert_eq!(code, 0, "{text}\n{}", failures(&clean, &bundle));
    bundle.validate().unwrap();
    assert_eq!(bundle.task_list_digest, digest);
    assert_eq!(bundle.trials.len(), suite.tasks.len());
    // Results carry the model, the build and the cost.
    assert_eq!(bundle.protocol.model, "gpt-5-mini");
    assert_eq!(bundle.protocol.approvals, APPROVE_COMPUTER_INPUTS);
    assert_eq!(bundle.environment.build_digest.len(), 64);
    assert!(!bundle.environment.modbit_revision.is_empty());
    for axis in AXES {
        let a = &rep.axes[axis];
        assert!(a.tasks.len() >= 10, "{axis}");
        assert_eq!(
            a.verified.successes,
            a.verified.n,
            "{axis}\n{}",
            failures(&clean, &bundle)
        );
        assert_eq!(a.task_results.len(), a.tasks.len());
    }
    let bytes = std::fs::read(clean.join("baseline.json")).unwrap();
    let file_digest = modbit_bench_agent_engineering::digest_of(&bytes);
    rep.verify_against(&bundle, &file_digest).unwrap();
    for t in &bundle.trials {
        // The cost is the recorded usage of the parent and its GUI child.
        let u = t.usage.as_ref().unwrap();
        assert!(u.tasks.len() >= 2, "{}: the child's usage counted", t.task);
        let log = std::fs::read_to_string(clean.join(&t.event_log.file)).unwrap();
        let events = parse_events(&log);
        assert_eq!(
            recorded_usage(&events),
            (u.model_calls, u.output_tokens),
            "{}",
            t.task
        );
        // Every input the child made was approved by the person (the harness
        // at the fixture desktop), and the application holds the effect.
        assert!(
            t.notes
                .iter()
                .any(|n| n.contains("computer input(s) approved")),
            "{}: {:?}",
            t.task,
            t.notes
        );
        let slug = t.task.replace('/', "__");
        let state: Value = serde_json::from_str(
            &std::fs::read_to_string(clean.join(format!("trials/{slug}-1/desktop-state.json")))
                .unwrap(),
        )
        .unwrap();
        let spec = suite.tasks.iter().find(|s| s.id == t.task).unwrap();
        for d in &spec
            .verifier
            .as_ref()
            .unwrap()
            .gui
            .as_ref()
            .unwrap()
            .desktop
        {
            assert_eq!(
                desktop_value(&state, &d.app, &d.name).as_deref(),
                Some(d.value.as_str()),
                "{}",
                t.task
            );
        }
        let v = t.verifier.as_ref().unwrap();
        assert!(v.passed, "{}: {:#?}", t.task, v.checks);
        assert!(
            events
                .iter()
                .any(|e| e.event_type == "GuiOutcomeJudged" && e.payload["verdict"] == "VERIFIED"),
            "{}",
            t.task
        );
    }
    assert!(
        std::fs::read_to_string(clean.join("summary.md"))
            .unwrap()
            .contains("## Axes of suite gui-regression (PX-179)")
    );

    // The retained baseline is refreshed from this run on request only
    // (`PX179_RETAIN_BASELINE=1`), when the suite, harness or Core changed.
    if std::env::var("PX179_RETAIN_BASELINE").as_deref() == Ok("1") {
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
    //    none, on any axis or any task; every gate passes.
    let (code, text, cmp) = run_compare(&baseline_dir(), &clean);
    assert_eq!(code, 0, "{text}");
    assert_eq!(cmp["passed"], true, "{cmp:#}");
    assert!(cmp.get("regressed_tasks").is_none(), "{cmp:#}");
    assert!(cmp.get("recovered_tasks").is_none(), "{cmp:#}");
    for a in cmp["axes"].as_array().unwrap() {
        assert_eq!(a["delta"].as_f64(), Some(0.0), "{a}");
    }

    // 3. An injected model regression in three named tasks: the same Core,
    //    actuator, desktops and suite; the child stops before the input that
    //    has the effect and still claims it is done.
    model
        .regress
        .lock()
        .unwrap()
        .extend(REGRESSED.iter().map(|t| (*t).to_owned()));
    let regressed_dir = tmp.path().join("regressed");
    let (code, text) = run_suite(&model.base, &regressed_dir);
    assert_eq!(code, 0, "the bundle and report are still written: {text}");
    let (bundle, _) = load_bundle(&regressed_dir);
    for t in &bundle.trials {
        let hit = REGRESSED.contains(&t.task.as_str());
        assert_eq!(
            t.verified_success,
            !hit,
            "{}\n{}",
            t.task,
            failures(&regressed_dir, &bundle)
        );
        if hit {
            // The child claimed it was done; the Core read the application
            // itself at completion, found the effect missing and judged the
            // run a model failure (the GUI verifier row), and the gate reads
            // that record.
            let log = std::fs::read_to_string(regressed_dir.join(&t.event_log.file)).unwrap();
            let events = parse_events(&log);
            assert!(
                events.iter().any(|e| e.event_type == "GuiEvidenceEvaluated"
                    && e.payload["at"] == "COMPLETION"
                    && e.payload["status"] == "FAIL"),
                "{}",
                t.task
            );
            assert!(
                events.iter().any(|e| e.event_type == "GuiOutcomeJudged"
                    && e.payload["verdict"] == "MODEL_FAILURE"),
                "{}",
                t.task
            );
            assert!(
                !events
                    .iter()
                    .any(|e| e.event_type == "GuiOutcomeJudged"
                        && e.payload["verdict"] == "VERIFIED"),
                "{}",
                t.task
            );
        }
    }
    let (code, text, cmp) = run_compare(&baseline_dir(), &regressed_dir);
    assert_eq!(code, 3, "a failed gate exits 3: {text}");
    assert_eq!(cmp["passed"], false);
    let mut want: Vec<&str> = REGRESSED.to_vec();
    want.sort_unstable();
    assert_eq!(cmp["regressed_tasks"], json!(want), "{cmp:#}");
    assert!(
        text.contains(&format!("regressed tasks: {}", want.join(", "))),
        "{text}"
    );
}

#[test]
fn qual_px_179_the_retained_baseline_is_a_valid_bundle_of_this_suite_with_model_build_and_cost() {
    let (suite, digest) = Suite::load(&suite_dir().join("tasks.json")).unwrap();
    let bytes = std::fs::read(baseline_dir().join("baseline.json")).unwrap();
    let file_digest = modbit_bench_agent_engineering::digest_of(&bytes);
    let recorded = std::fs::read_to_string(baseline_dir().join("baseline.sha256")).unwrap();
    assert!(recorded.starts_with(&file_digest), "{recorded}");
    let bundle: Bundle = serde_json::from_slice(&bytes).unwrap();
    bundle.validate().unwrap();
    assert_eq!(
        bundle.task_list_digest, digest,
        "the suite changed: refresh the baseline with PX179_RETAIN_BASELINE=1"
    );
    assert_eq!(bundle.trials.len(), suite.tasks.len());
    assert!(!bundle.protocol.model.is_empty());
    assert_eq!(bundle.environment.build_digest.len(), 64);
    let rep: axes::AxisReport =
        serde_json::from_str(&std::fs::read_to_string(baseline_dir().join("axes.json")).unwrap())
            .unwrap();
    rep.verify_against(&bundle, &file_digest).unwrap();
    assert!(rep.gate.per_task);
    for axis in AXES {
        let a = &rep.axes[axis];
        assert!(a.verified.n >= 10, "{axis}");
        assert!(rep.gate.can_fail(a.verified.rate), "{axis}");
        assert_eq!(a.task_results.len(), a.tasks.len(), "{axis}");
    }
    assert!(rep.cost.output_tokens_per_thousand_tasks > 0.0);
}
