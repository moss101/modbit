//! PX-069, PX-070, PX-075, PX-076 (QUAL-PX-069/070/075/076) on the real Core:
//! the real `modbit-core` binary over its real socket and SQLite store, the
//! real fixture actuator process (`modbit-actuator-fixture`, which speaks the
//! same authenticated RPC as the macOS helper over an in-memory desktop - no
//! display, no operating-system permission), a person who approves through
//! `ResolveApproval`, and a scripted OpenAI-compatible model.
mod px_common;

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use modbit_protocol::client::Client;
use modbit_protocol::v1::{ApprovalList, ApprovalView, Id, ListApprovals, ResolveApproval};
use prost::Message;
use px_common::*;
use serde_json::{Value, json};

/// The fixture actuator, built if the test binary was built without it.
fn fixture_bin() -> PathBuf {
    let core = PathBuf::from(env!("CARGO_BIN_EXE_modbit-core"));
    let dir = core.parent().unwrap().to_path_buf();
    let name = if cfg!(windows) {
        "modbit-actuator-fixture.exe"
    } else {
        "modbit-actuator-fixture"
    };
    let p = dir.join(name);
    if !p.exists() {
        let status = std::process::Command::new(env!("CARGO"))
            .args(["build", "-p", "modbit-actuator-fixture", "--target-dir"])
            .arg(dir.parent().unwrap())
            .current_dir(env!("CARGO_MANIFEST_DIR"))
            .status()
            .unwrap();
        assert!(status.success(), "building the fixture actuator");
    }
    p
}

fn kill_pid(pid: u32) {
    #[cfg(unix)]
    let _ = std::process::Command::new("kill")
        .args(["-9", &pid.to_string()])
        .status();
    #[cfg(windows)]
    let _ = std::process::Command::new("taskkill")
        .args(["/F", "/PID", &pid.to_string()])
        .status();
}

/// What the person decides about one approval.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Decision {
    Approve,
    Deny,
    Ignore,
}

type Decider = Arc<dyn Fn(&ApprovalView, &Value) -> Decision + Send + Sync>;

/// The person at the keyboard for computer approvals: sees each one, decides.
async fn spawn_person(
    core: &CoreProcess,
    session: &Id,
    generation: Option<u64>,
    decide: Decider,
) -> (
    tokio::task::JoinHandle<()>,
    Arc<Mutex<Vec<(ApprovalView, Value, Decision)>>>,
) {
    let seen: Arc<Mutex<Vec<(ApprovalView, Value, Decision)>>> = Arc::default();
    let seen2 = seen.clone();
    let session = session.clone();
    let mut c = core.client().await;
    let h = tokio::spawn(async move {
        let mut done: std::collections::HashSet<Vec<u8>> = Default::default();
        loop {
            let ack = c
                .command(envelope(
                    rand_id(),
                    "ListApprovals",
                    ListApprovals {
                        session_id: Some(session.clone()),
                    }
                    .encode_to_vec(),
                ))
                .await;
            let Ok(ack) = ack else { return };
            let list: ApprovalList = Client::result(&ack).unwrap();
            for a in list
                .approvals
                .iter()
                .filter(|a| a.status == "REQUESTED" && a.tool_name.starts_with("computer."))
            {
                let id = a.approval_id.clone().unwrap();
                if !done.insert(id.value.clone()) {
                    continue;
                }
                let scope: Value = serde_json::from_str(&a.scope_json).unwrap_or_default();
                let d = decide(a, &scope);
                seen2.lock().unwrap().push((a.clone(), scope, d));
                match d {
                    Decision::Ignore => {}
                    Decision::Approve | Decision::Deny => {
                        let _ = c
                            .command(envelope_fenced(
                                rand_id(),
                                "ResolveApproval",
                                ResolveApproval {
                                    approval_id: Some(id),
                                    approve: d == Decision::Approve,
                                    reason: "test person".into(),
                                    intent_hash: a.intent_hash.clone(),
                                }
                                .encode_to_vec(),
                                generation,
                            ))
                            .await;
                    }
                }
            }
            tokio::time::sleep(Duration::from_millis(60)).await;
        }
    });
    (h, seen)
}

fn approve_all() -> Decider {
    Arc::new(|_, _| Decision::Approve)
}

/// What the model has been shown so far, parsed: the latest snapshot id, frame token and tree.
#[derive(Clone, Default)]
struct St {
    texts: Vec<String>,
    outs: Vec<Value>,
    snapshot: String,
    frame: String,
    tree: String,
    control: String,
}

fn call(name: &str, args: Value) -> Value {
    json!({"calls": [{"name": name, "args": args}]})
}

fn parse_tool_text(text: &str) -> Value {
    text.find('{')
        .and_then(|i| {
            let mut de = serde_json::Deserializer::from_str(&text[i..]).into_iter::<Value>();
            de.next().and_then(Result::ok)
        })
        .unwrap_or(Value::Null)
}

fn st_of(body: &Value) -> St {
    let mut st = St::default();
    for m in body["messages"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|m| m["role"] == "tool")
    {
        let text = m["content"].as_str().unwrap_or_default().to_owned();
        let j = parse_tool_text(&text);
        if let Some(s) = j["snapshot"].as_str() {
            st.snapshot = s.to_owned();
        }
        if let Some(s) = j["frame"].as_str() {
            st.frame = s.to_owned();
        }
        if let Some(s) = j["tree"].as_str() {
            st.tree = s.to_owned();
        }
        if let Some(s) = j["control_id"].as_str() {
            st.control = s.to_owned();
        }
        st.texts.push(text);
        st.outs.push(j);
    }
    st
}

impl St {
    /// The snapshot id the `i`-th tool result (counting the plan's as 0) carried.
    fn snapshot_at(&self, i: usize) -> String {
        self.outs[i]["snapshot"]
            .as_str()
            .unwrap_or_default()
            .to_owned()
    }

    /// The `<snapshot>/<id>` of the element named `name` in the latest tree.
    fn el(&self, name: &str) -> String {
        let needle = format!("\"{name}\"");
        self.tree
            .lines()
            .find(|l| l.contains(&format!("| {needle} |")))
            .and_then(|l| l.split_whitespace().next())
            .unwrap_or_else(|| panic!("no element {name} in\n{}", self.tree))
            .to_owned()
    }
}

type Step = Arc<dyn Fn(&St) -> Value + Send + Sync>;

fn st(f: impl Fn(&St) -> Value + Send + Sync + 'static) -> Step {
    Arc::new(f)
}

fn fx(v: Value) -> Step {
    Arc::new(move |_| v.clone())
}

async fn dynamic_model(steps: Vec<Step>) -> (String, Seen) {
    scripted_model_fn(Arc::new(move |body, results| {
        let st = st_of(body);
        steps
            .get(results)
            .map(|f| f(&st))
            .unwrap_or_else(|| json!({"text": "I have nothing further to do."}))
    }))
    .await
}

fn error_code(text: &str) -> String {
    text.lines()
        .find_map(|l| l.strip_prefix("error_code: "))
        .unwrap_or_default()
        .trim()
        .to_owned()
}

struct Rig {
    core: CoreProcess,
    client: Client,
    session: Id,
    g: Option<u64>,
    task: Id,
    seen: Seen,
    approvals: Arc<Mutex<Vec<(ApprovalView, Value, Decision)>>>,
    person: tokio::task::JoinHandle<()>,
    dir: tempfile::TempDir,
    _repo: tempfile::TempDir,
    desktop_dir: tempfile::TempDir,
    env: Vec<(String, String)>,
}

/// Options for one run.
struct Opts {
    desktop: Option<Value>,
    env: Vec<(&'static str, String)>,
    profile: &'static str,
    actuator: bool,
    declare: bool,
}

impl Default for Opts {
    fn default() -> Self {
        Self {
            desktop: None,
            env: vec![],
            profile: "local_trusted",
            actuator: true,
            declare: true,
        }
    }
}

/// Start the run with room for a long script: computer refusals are not progress, and a
/// script of them must not be stopped by the stall rule of the whole loop.
async fn start_task_long(c: &mut Client, task: &Id, g: Option<u64>, id: u8) {
    use modbit_protocol::v1::{StartTask, TaskRunStarted};
    let ack = c
        .command(envelope_fenced(
            id16(id),
            "StartTask",
            StartTask {
                task_id: Some(task.clone()),
                endpoint: String::new(),
                model: "gpt-5".into(),
                max_turns: 120,
                max_tool_calls: 0,
                max_no_progress_turns: 100,
                skills: vec![],
                ..Default::default()
            }
            .encode_to_vec(),
            g,
        ))
        .await
        .unwrap();
    let _: TaskRunStarted = Client::result(&ack).unwrap();
}

fn plan() -> Value {
    call(
        "plan.update",
        json!({"outcome": "operate a native application", "expected_files": [], "protected_effects": ["computer"]}),
    )
}

fn complete() -> Value {
    json!({"calls": [{"name": "task.complete", "args": {"summary": "done", "self_review": {"findings": []}}}]})
}

/// A Core with the fixture actuator attached, a task, a scripted model and a person - the run
/// not yet started, so a test can set a run mode or try a rule first.
async fn setup(opts: Opts, mut steps: Vec<Step>, decide: Decider) -> Rig {
    if opts.declare {
        steps.insert(0, fx(plan()));
    }
    steps.push(fx(complete()));
    let (base, seen) = dynamic_model(steps).await;
    setup_model(opts, base, seen, decide).await
}

/// [`setup`] over a model the caller built (a parent and a child share one).
async fn setup_model(opts: Opts, base: String, seen: Seen, decide: Decider) -> Rig {
    let (repo, root) = plain_repo(&[("README.md", "# fixture\n")]);
    let desktop_dir = tempfile::tempdir().unwrap();
    let mut args = vec![];
    if let Some(d) = &opts.desktop {
        let p = desktop_dir.path().join("desktop.json");
        std::fs::write(&p, d.to_string()).unwrap();
        args.push("--desktop".to_owned());
        args.push(p.to_string_lossy().into_owned());
    }
    args.push("--pid-file".into());
    args.push(
        desktop_dir
            .path()
            .join("actuator.pid")
            .to_string_lossy()
            .into_owned(),
    );
    let dir = tempfile::tempdir().unwrap();
    let mut env: Vec<(String, String)> = model_env(&base);
    if opts.actuator {
        env.push((
            "MODBIT_COMPUTER_ACTUATOR".into(),
            fixture_bin().to_string_lossy().into_owned(),
        ));
        env.push((
            "MODBIT_COMPUTER_ACTUATOR_ARGS".into(),
            serde_json::to_string(&args).unwrap(),
        ));
    }
    for (k, v) in &opts.env {
        env.push(((*k).into(), v.clone()));
    }
    let env_refs: Vec<(&str, &str)> = env.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
    let core = CoreProcess::spawn_with_env(dir.path(), &env_refs);
    let mut c = core.client().await;
    let (session, g) = session_with_lease(&mut c, 0x10).await;
    let task = create_task(
        &mut c,
        &session,
        g,
        &root,
        0x11,
        opts.profile,
        "operate the fixture app",
    )
    .await;
    let (person, approvals) = spawn_person(&core, &session, g, decide).await;
    Rig {
        core,
        client: c,
        session,
        g,
        task,
        seen,
        approvals,
        person,
        dir,
        _repo: repo,
        desktop_dir,
        env,
    }
}

/// [`setup`], then the run starts.
async fn run(opts: Opts, steps: Vec<Step>, decide: Decider) -> Rig {
    let mut rig = setup(opts, steps, decide).await;
    rig.begin().await;
    rig
}

impl Rig {
    async fn begin(&mut self) {
        let (task, g) = (self.task.clone(), self.g);
        start_task_long(&mut self.client, &task, g, 0x12).await;
    }

    async fn finish(&mut self) {
        let st = wait_task(&mut self.client, &self.task, 120).await;
        assert!(!st.loop_alive, "the run did not end: {st:?}");
    }

    /// The tool results the model was shown, after the plan's, in call order.
    fn outs(&self) -> Vec<(String, Value)> {
        let bodies = self.seen.lock().unwrap().clone();
        let Some(last) = bodies.last() else {
            return vec![];
        };
        st_of(last)
            .texts
            .into_iter()
            .zip(st_of(last).outs)
            .skip(1)
            .collect()
    }

    async fn events(&self) -> Vec<Value> {
        replay(&self.core, &self.session).await
    }

    /// Print what the model was shown (for a failing run).
    #[allow(dead_code)]
    async fn dump(&self) {
        for (i, b) in self.seen.lock().unwrap().iter().enumerate() {
            let n = b["messages"]
                .as_array()
                .map_or(0, |m| m.iter().filter(|x| x["role"] == "tool").count());
            let last = b["messages"]
                .as_array()
                .and_then(|m| m.last())
                .map(|m| m.to_string().chars().take(200).collect::<String>());
            println!("REQ {i}: {n} tool results; last message {last:?}");
        }
        for (i, (t, _)) in self.outs().iter().enumerate() {
            println!(
                "--- result {i}: {}",
                t.chars().take(700).collect::<String>()
            );
        }
        for e in self.events().await {
            let t = e["event_type"].as_str().unwrap_or_default();
            if t.starts_with("Computer")
                || t.contains("Approval")
                || t.contains("Attention")
                || t == "TaskWaiting"
            {
                println!(
                    "EV {t} {}",
                    e["payload"]["payload"]
                        .to_string()
                        .chars()
                        .take(400)
                        .collect::<String>()
                );
            }
        }
    }

    fn actuator_pid(&self) -> u32 {
        std::fs::read_to_string(self.desktop_dir.path().join("actuator.pid"))
            .unwrap()
            .trim()
            .parse()
            .unwrap()
    }

    fn approvals_of(&self, tool: &str) -> Vec<(ApprovalView, Value, Decision)> {
        self.approvals
            .lock()
            .unwrap()
            .iter()
            .filter(|(a, ..)| a.tool_name == tool)
            .cloned()
            .collect()
    }
}

impl Drop for Rig {
    fn drop(&mut self) {
        self.person.abort();
    }
}

fn of<'a>(evs: &'a [Value], t: &str) -> Vec<&'a Value> {
    evs.iter().filter(|e| e["event_type"] == t).collect()
}

const FORM: &str = "org.example.fixture.form";

fn start(app: &'static str) -> Step {
    fx(call(
        "computer.start",
        json!({"application": app, "reason": "test"}),
    ))
}

/// QUAL-PX-069: an accessibility-first task completes with no coordinate used; every action leaves
/// its events, and an AUTHORIZED and a result receipt bound to the intent hash the person approved.
#[tokio::test(flavor = "multi_thread")]
async fn qual_px_069_an_accessibility_first_task_completes_and_leaves_its_audit() {
    let steps = vec![
        fx(call("computer.apps", json!({}))),
        start(FORM),
        fx(call("computer.state", json!({}))),
        st(|s| {
            call(
                "computer.set_value",
                json!({"element": s.el("Name"), "value": "Ada Lovelace"}),
            )
        }),
        st(|s| call("computer.press", json!({"element": s.el("Submit")}))),
        // The press opened a confirmation dialog: read it and answer it.
        fx(call("computer.state", json!({"window": "w-confirm"}))),
        st(|s| call("computer.press", json!({"element": s.el("OK")}))),
        fx(call("computer.state", json!({}))),
        fx(call("computer.release", json!({}))),
    ];
    let mut rig = run(Opts::default(), steps, approve_all()).await;
    rig.finish().await;
    let outs = rig.outs();
    for (i, (t, _)) in outs.iter().enumerate() {
        assert!(t.starts_with("status: SUCCESS"), "call {i}: {t}");
    }
    // The form read back what was set and submitted.
    let last_state = &outs[7].1;
    let tree = last_state["tree"].as_str().unwrap();
    assert!(tree.contains("value=\"Ada Lovelace\""), "{tree}");
    assert!(tree.contains("\"Status\" | value=\"submitted\""), "{tree}");
    let pressed = &outs[4].1;
    assert_eq!(pressed["structure_changed"], true);
    assert_eq!(pressed["modality"], "ax");
    // Events: the session, its observations, three inputs by kind, a close with counts only.
    let evs = rig.events().await;
    assert_eq!(of(&evs, "ComputerSessionStarted").len(), 1);
    assert_eq!(of(&evs, "ComputerObserved").len(), 3);
    let acts = of(&evs, "ComputerActionPerformed");
    assert_eq!(acts.len(), 3, "{acts:?}");
    assert!(
        acts.iter()
            .all(|a| a["payload"]["payload"]["outcome"] == "SUCCEEDED")
    );
    let closed = of(&evs, "ComputerSessionClosed");
    assert_eq!(closed.len(), 1);
    let c = &closed[0]["payload"]["payload"];
    assert_eq!(c["reason"], "RELEASED");
    assert_eq!(c["actions_by_kind"], json!({"press": 2, "set_value": 1}));
    assert_eq!(c["bundle_id"], FORM);
    // The audit holds no text the model typed.
    for e in evs.iter().filter(|e| {
        e["event_type"]
            .as_str()
            .is_some_and(|t| t.starts_with("Computer"))
    }) {
        assert!(!e.to_string().contains("Ada Lovelace"), "{e}");
    }
    // Every start and input was approved with its exact intent; the receipts carry that hash.
    let approvals = rig.approvals.lock().unwrap().clone();
    assert_eq!(approvals.len(), 4, "start + three inputs: {approvals:?}");
    for (a, scope, _) in &approvals {
        assert_eq!(scope["computer_control"], true);
        assert_eq!(scope["allowlistable"], false);
        assert_eq!(
            scope["intent"]["computer"]["application"]["bundle_id"],
            FORM
        );
        assert_eq!(
            scope["intent"]["computer"]["application"]["signing_identity"],
            "TEAM1:fixture-form"
        );
        let receipts: Vec<&Value> = of(&evs, "EffectReceiptAppended")
            .into_iter()
            .filter(|r| r["payload"]["payload"]["receipt"]["intent_hash"] == a.intent_hash)
            .collect();
        let statuses: Vec<&str> = receipts
            .iter()
            .map(|r| {
                r["payload"]["payload"]["receipt"]["status"]
                    .as_str()
                    .unwrap()
            })
            .collect();
        assert_eq!(statuses.len(), 2, "{a:?}: {statuses:?}");
        assert!(
            statuses.contains(&"AUTHORIZED") && statuses.contains(&"SUCCESS"),
            "{statuses:?}"
        );
        assert!(
            receipts
                .iter()
                .all(|r| r["payload"]["payload"]["receipt"]["reversibility"] == "IRREVERSIBLE")
        );
    }
    // The press approval names the element, the action and the window.
    let press = rig.approvals_of("computer.press");
    assert_eq!(press[0].1["intent"]["computer"]["target"]["name"], "Submit");
    assert_eq!(press[0].1["intent"]["computer"]["action"], "press");
    assert_eq!(press[0].1["intent"]["computer"]["window"]["id"], "w-form");
    let setv = rig.approvals_of("computer.set_value");
    assert_eq!(
        setv[0].1["intent"]["computer"]["arguments"]["value"], "Ada Lovelace",
        "typed text is shown in full"
    );
}

/// Assert that result `i` (after the plan's) is the refusal `code` with `escalation`.
fn expect_refusal(outs: &[(String, Value)], i: usize, code: &str, escalation: &str) {
    let (t, _) = &outs[i];
    assert_eq!(error_code(t), code, "result {i}:\n{t}");
    assert!(
        t.contains(&format!("\"escalation\":\"{escalation}\"")),
        "result {i} does not carry the escalation {escalation}:\n{t}"
    );
}

/// QUAL-PX-069: every refusal is typed with its documented escalation; nothing is actuated.
#[tokio::test(flavor = "multi_thread")]
async fn qual_px_069_every_refusal_is_typed_with_its_escalation_and_actuates_nothing() {
    let steps = vec![
        // 0 a misspelt argument, with no session: the advertised names, no default action.
        fx(call("computer.press", json!({"elemnt": "s1/n1"}))),
        // 1 no session.
        fx(call("computer.state", json!({}))),
        // 2-4 applications no approval lifts, and one the actuator cannot reach.
        fx(call(
            "computer.start",
            json!({"application": "com.apple.Terminal"}),
        )),
        fx(call(
            "computer.start",
            json!({"application": "Keychain Access"}),
        )),
        fx(call("computer.start", json!({"application": "Admin Tool"}))),
        // 5 two applications answer to this name.
        fx(call("computer.start", json!({"application": "Form App"}))),
        // 6 the one that was meant.
        start(FORM),
        // 7, 8 two reads: the first snapshot is stale after the second.
        fx(call("computer.state", json!({}))),
        fx(call("computer.state", json!({}))),
        // 9 a stale element id.
        st(|s| {
            call(
                "computer.press",
                json!({"element": format!("{}/n4", s.snapshot_at(8)) }),
            )
        }),
        // 10 a label does not take a value.
        st(|s| {
            call(
                "computer.set_value",
                json!({"element": s.el("Status"), "value": "x"}),
            )
        }),
        // 11 a coordinate with no screenshot.
        fx(call(
            "computer.click",
            json!({"frame": "f0", "x": 10, "y": 10}),
        )),
        // 12 the screenshot, 13 a click in the margin outside the window.
        fx(call("computer.screenshot", json!({}))),
        st(|s| call("computer.click", json!({"frame": s.frame, "x": 5, "y": 5}))),
        // 14 a destructive key the call did not name.
        fx(call("computer.key", json!({"keys": ["delete"]}))),
        // 15 typing with no verified focus.
        fx(call("computer.type", json!({"text": "hello"}))),
        // 16 the real pointer needs SCREEN scope.
        st(|s| call("computer.move", json!({"frame": s.frame, "x": 10, "y": 10}))),
        // 17 an action the element does not offer.
        fx(call("computer.state", json!({}))),
        st(|s| {
            call(
                "computer.press",
                json!({"element": s.el("Submit"), "action": "fly"}),
            )
        }),
        // 19 press Submit: a modal dialog opens.
        st(|s| call("computer.press", json!({"element": s.el("Submit")}))),
        // 20 read the blocked window, 21 act on it anyway.
        fx(call("computer.state", json!({}))),
        st(|s| {
            call(
                "computer.press",
                json!({"element": s.el("Quantity"), "action": "focus"}),
            )
        }),
        // 22 give the machine back, 23 and nothing is open.
        fx(call("computer.release", json!({}))),
        fx(call("computer.press", json!({"element": "s1/n1"}))),
        // 24 a tool that does not exist.
        fx(call("computer.nope", json!({}))),
    ];
    let mut rig = run(Opts::default(), steps, approve_all()).await;
    rig.finish().await;
    let o = rig.outs();
    expect_refusal(&o, 0, "INVALID_ARGUMENTS", "re_observe");
    assert!(
        o[0].0.contains("`elemnt`") && o[0].0.contains("`element`"),
        "{}",
        o[0].0
    );
    expect_refusal(&o, 1, "SESSION_REQUIRED", "re_observe");
    expect_refusal(&o, 2, "WINDOW_UNVERIFIABLE", "ask_user");
    expect_refusal(&o, 3, "WINDOW_UNVERIFIABLE", "ask_user");
    expect_refusal(&o, 4, "TARGET_ELEVATED", "ask_user");
    expect_refusal(&o, 5, "INVALID_ARGUMENTS", "re_observe");
    assert!(o[6].0.starts_with("status: SUCCESS"), "{}", o[6].0);
    expect_refusal(&o, 9, "TARGET_STALE", "re_observe");
    expect_refusal(&o, 10, "TARGET_NOT_EDITABLE", "re_observe");
    expect_refusal(&o, 11, "SCREENSHOT_REQUIRED", "re_observe");
    assert!(o[12].0.starts_with("status: SUCCESS"));
    expect_refusal(&o, 13, "TARGET_OCCLUDED", "re_observe");
    expect_refusal(&o, 14, "ACTION_UNSAFE", "abandon");
    expect_refusal(&o, 15, "TARGET_NOT_EDITABLE", "re_observe");
    expect_refusal(&o, 16, "UNSUPPORTED_REQUEST", "abandon");
    expect_refusal(&o, 18, "UNSUPPORTED_REQUEST", "abandon");
    assert!(o[19].0.starts_with("status: SUCCESS"), "{}", o[19].0);
    assert_eq!(o[19].1["structure_changed"], true);
    expect_refusal(&o, 21, "MODAL_BLOCKING", "ask_user");
    assert!(o[22].0.starts_with("status: SUCCESS"));
    expect_refusal(&o, 23, "SESSION_REQUIRED", "re_observe");
    // A name outside the family is the registry's refusal, not a guess.
    assert_eq!(error_code(&o[24].0), "TOOL_NOT_VISIBLE", "{}", o[24].0);
    // The approvals the person was asked: the start and the two inputs that were
    // approved (the margin click and the Submit press). None for what was refused first.
    let asked: Vec<String> = rig
        .approvals
        .lock()
        .unwrap()
        .iter()
        .map(|(a, ..)| a.tool_name.clone())
        .collect();
    assert_eq!(
        asked,
        vec!["computer.start", "computer.click", "computer.press"],
        "{asked:?}"
    );
    // The refused click is on the record as a failed input, not a success.
    let evs = rig.events().await;
    let acts = of(&evs, "ComputerActionPerformed");
    let click = acts
        .iter()
        .find(|a| a["payload"]["payload"]["kind"] == "click")
        .unwrap();
    assert_eq!(click["payload"]["payload"]["outcome"], "FAILED");
    assert_eq!(click["payload"]["payload"]["code"], "TARGET_OCCLUDED");
}

fn desktop_with(f: impl FnOnce(&mut Value)) -> Value {
    // The built-in desktop, as JSON, edited.
    let mut v: Value = serde_json::from_str(DEFAULT_DESKTOP).unwrap();
    f(&mut v);
    v
}

/// The fixture's built-in desktop (kept here as the tests' source of truth for edits).
const DEFAULT_DESKTOP: &str =
    include_str!("../../modbit-actuator-fixture/src/default_desktop.json");

fn app_mut<'a>(d: &'a mut Value, bundle: &str) -> &'a mut Value {
    d["apps"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|a| a["bundle_id"] == bundle)
        .unwrap()
}

/// The code a rejected command carries ("" when it was accepted).
fn rejected_code<T>(r: Result<T, modbit_protocol::client::ClientError>) -> String {
    match r {
        Ok(_) => String::new(),
        Err(modbit_protocol::client::ClientError::Rejected { code, .. }) => code,
        Err(e) => panic!("{e}"),
    }
}

async fn rule_ack(rig: &mut Rig, pattern: &[&str]) -> String {
    use modbit_protocol::v1::AddAllowRule;
    let r = rig
        .client
        .command(envelope_fenced(
            rand_id(),
            "AddAllowRule",
            AddAllowRule {
                task_id: Some(rig.task.clone()),
                rule_id: String::new(),
                pattern: pattern.iter().map(|s| (*s).to_owned()).collect(),
                scope: "TASK".into(),
                expires_at_ms: 0,
                covers_always_ask: true,
            }
            .encode_to_vec(),
            rig.g,
        ))
        .await;
    rejected_code(r)
}

/// QUAL-PX-070: every start and every input asks, with its exact intent, whatever the run mode; no
/// rule stands for it; an application that replaced the approved one is refused with no approval.
#[tokio::test(flavor = "multi_thread")]
async fn qual_px_070_every_action_is_approved_per_call_and_nothing_stands_for_the_approval() {
    use modbit_protocol::v1::SetRunMode;
    let steps = vec![
        start(FORM),
        fx(call("computer.state", json!({}))),
        st(|s| {
            call(
                "computer.set_value",
                json!({"element": s.el("Name"), "value": "a"}),
            )
        }),
        // The identical call again: asked again, not covered by the first approval.
        st(|s| {
            call(
                "computer.set_value",
                json!({"element": s.el("Name"), "value": "a"}),
            )
        }),
        // The application the session was approved for is replaced by a look-alike.
        st(|s| {
            call(
                "computer.press",
                json!({"element": s.el("Swap the application")}),
            )
        }),
        st(|s| call("computer.press", json!({"element": s.el("Submit")}))),
        fx(call("computer.state", json!({}))),
    ];
    let mut rig = setup(Opts::default(), steps, approve_all()).await;
    // Run everything: the loosest mode there is.
    let ack = rig
        .client
        .command(envelope_fenced(
            rand_id(),
            "SetRunMode",
            SetRunMode {
                task_id: Some(rig.task.clone()),
                mode: "RUN_EVERYTHING".into(),
                acknowledge_risk: true,
            }
            .encode_to_vec(),
            rig.g,
        ))
        .await;
    assert_eq!(rejected_code(ack), "");
    // No rule can be made for a computer tool, whatever it asks to cover.
    for p in [
        vec!["computer.press"],
        vec!["computer"],
        vec!["computer.set_value", "x"],
    ] {
        let code = rule_ack(&mut rig, &p).await;
        assert_eq!(code, "COMPUTER_NOT_ALLOWLISTABLE", "{p:?}");
    }
    // A rule for an ordinary command is still a rule.
    assert_eq!(rule_ack(&mut rig, &["cargo", "test"]).await, "");
    rig.begin().await;
    rig.finish().await;
    let o = rig.outs();
    assert!(o[0].0.starts_with("status: SUCCESS"));
    expect_refusal(&o, 5, "WINDOW_UNVERIFIABLE", "ask_user");
    expect_refusal(&o, 6, "WINDOW_UNVERIFIABLE", "ask_user");
    assert!(
        o[3].0.starts_with("status: SUCCESS"),
        "the swap press itself ran: {}",
        o[3].0
    );
    let asked = rig.approvals.lock().unwrap().clone();
    let tools: Vec<&str> = asked.iter().map(|(a, ..)| a.tool_name.as_str()).collect();
    assert_eq!(
        tools,
        vec![
            "computer.start",
            "computer.set_value",
            "computer.set_value",
            "computer.press"
        ],
        "run everything asked every time; the refused press and read asked nothing"
    );
    for (a, scope, _) in &asked {
        assert_eq!(scope["run_mode"], "RUN_EVERYTHING");
        assert_eq!(scope["ask_classes"], json!(["COMPUTER_CONTROL"]), "{scope}");
        assert_eq!(scope["allowlistable"], false);
        // The approval expires in minutes, not a day.
        assert!(
            a.expires_at_ms - a.requested_at_ms <= 5 * 60 * 1000 + 1000,
            "{a:?}"
        );
    }
    // The same call twice: the same intent, two approvals.
    assert_eq!(asked[1].0.intent_hash, asked[2].0.intent_hash);
    assert_ne!(asked[1].0.approval_id, asked[2].0.approval_id);
}

/// QUAL-PX-070: the decision binds to the intent the person saw; an approval is short-lived.
#[tokio::test(flavor = "multi_thread")]
async fn qual_px_070_a_decision_on_another_intent_is_refused() {
    let steps = vec![start(FORM), fx(call("computer.state", json!({})))];
    let ignore: Decider = Arc::new(|_, _| Decision::Ignore);
    let mut rig = run(Opts::default(), steps, ignore).await;
    // Wait for the approval to be asked.
    let mut approval = None;
    for _ in 0..100 {
        let ack = rig
            .client
            .command(envelope(
                rand_id(),
                "ListApprovals",
                ListApprovals {
                    session_id: Some(rig.session.clone()),
                }
                .encode_to_vec(),
            ))
            .await
            .unwrap();
        let list: ApprovalList = Client::result(&ack).unwrap();
        if let Some(a) = list.approvals.into_iter().find(|a| a.status == "REQUESTED") {
            approval = Some(a);
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    let a = approval.expect("the start asks");
    let decide = |intent: &str, approve: bool| {
        envelope_fenced(
            rand_id(),
            "ResolveApproval",
            ResolveApproval {
                approval_id: a.approval_id.clone(),
                approve,
                reason: "t".into(),
                intent_hash: intent.into(),
            }
            .encode_to_vec(),
            rig.g,
        )
    };
    // Another application's intent: nothing is decided.
    let r = rig.client.command(decide(&"0".repeat(64), true)).await;
    assert_eq!(rejected_code(r), "INTENT_MISMATCH");
    let scope: Value = serde_json::from_str(&a.scope_json).unwrap();
    assert_eq!(
        scope["intent"]["computer"]["application"]["bundle_id"],
        FORM
    );
    // The person saw this one.
    let r = rig.client.command(decide(&a.intent_hash, true)).await;
    assert_eq!(rejected_code(r), "");
    rig.finish().await;
    assert!(rig.outs()[0].0.starts_with("status: SUCCESS"));
}

/// QUAL-PX-070: administrative policy restricts applications and forbids SCREEN scope; the person is
/// asked about neither.
#[tokio::test(flavor = "multi_thread")]
async fn qual_px_070_policy_restricts_applications_and_forbids_screen_scope_with_no_approval_offered()
 {
    let cfg = tempfile::tempdir().unwrap();
    let admin = cfg.path().join("admin.json");
    std::fs::write(
        &admin,
        json!({"permissions": {"computer.screen": "DENY"}, "computer_apps_allow": ["org.example.fixture.docs"]}).to_string(),
    )
    .unwrap();
    let steps = vec![
        // Not on the allow-list.
        start(FORM),
        // SCREEN scope is forbidden outright.
        fx(call(
            "computer.start_screen",
            json!({"application": "org.example.fixture.docs"}),
        )),
        // The allowed application starts.
        fx(call(
            "computer.start",
            json!({"application": "org.example.fixture.docs"}),
        )),
        fx(call("computer.state", json!({}))),
    ];
    let opts = Opts {
        env: vec![("MODBIT_ADMIN_CONFIG", admin.to_string_lossy().into_owned())],
        ..Opts::default()
    };
    let mut rig = run(opts, steps, approve_all()).await;
    rig.finish().await;
    let o = rig.outs();
    expect_refusal(&o, 0, "WINDOW_UNVERIFIABLE", "ask_user");
    assert!(
        o[0].0.contains("policy allows computer control only for"),
        "{}",
        o[0].0
    );
    // SCREEN scope is forbidden: refused before anyone is asked.
    expect_refusal(&o, 1, "ACTION_UNSAFE", "abandon");
    assert!(o[1].0.contains("policy forbids SCREEN scope"), "{}", o[1].0);
    assert!(o[2].0.starts_with("status: SUCCESS"), "{}", o[2].0);
    assert!(o[3].0.starts_with("status: SUCCESS"));
    let asked: Vec<String> = rig
        .approvals
        .lock()
        .unwrap()
        .iter()
        .map(|(a, ..)| a.tool_name.clone())
        .collect();
    assert_eq!(
        asked,
        vec!["computer.start"],
        "only the allowed application was asked about"
    );
}

/// The person approved a `tool` call and gave the application a moment to be in the middle of it.
async fn when_in_flight(
    rig_approvals: Arc<Mutex<Vec<(ApprovalView, Value, Decision)>>>,
    tool: &'static str,
) {
    for _ in 0..300 {
        if rig_approvals
            .lock()
            .unwrap()
            .iter()
            .any(|(a, ..)| a.tool_name == tool)
        {
            tokio::time::sleep(Duration::from_millis(700)).await;
            return;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("no {tool} approval was asked");
}

fn receipts(evs: &[Value]) -> Vec<Value> {
    of(evs, "EffectReceiptAppended")
        .into_iter()
        .map(|r| r["payload"]["payload"]["receipt"].clone())
        .collect()
}

/// QUAL-PX-069: an actuator that dies after an input was applied and before it answered leaves the
/// outcome UNKNOWN: one UNKNOWN receipt, the task latched, nothing retried or repeated, and only a
/// new session with a fresh observation lifts the refusal.
#[tokio::test(flavor = "multi_thread")]
async fn qual_px_069_a_crash_after_delivery_latches_and_only_a_new_session_observing_reconciles() {
    let steps = vec![
        start(FORM),
        fx(call("computer.state", json!({}))),
        st(|s| {
            call(
                "computer.press",
                json!({"element": s.el("Crash after delivery")}),
            )
        }),
        // The model tries the next input anyway: refused, never sent.
        st(|s| {
            call(
                "computer.set_value",
                json!({"element": s.el("Name"), "value": "x"}),
            )
        }),
        fx(call("computer.state", json!({}))),
        // A new session needs a new approval.
        start(FORM),
        // Still latched until something is observed.
        st(|s| {
            call(
                "computer.set_value",
                json!({"element": s.el("Name"), "value": "x"}),
            )
        }),
        fx(call("computer.state", json!({}))),
        st(|s| {
            call(
                "computer.set_value",
                json!({"element": s.el("Name"), "value": "after"}),
            )
        }),
    ];
    let mut rig = run(Opts::default(), steps, approve_all()).await;
    rig.finish().await;
    let o = rig.outs();
    assert!(o[2].0.starts_with("status: UNKNOWNOUTCOME"), "{}", o[2].0);
    assert!(o[2].0.contains("OUTCOME_UNKNOWN"), "{}", o[2].0);
    expect_refusal(&o, 3, "OUTCOME_UNKNOWN", "ask_user");
    assert!(o[3].0.contains("\"input_sent\":false"), "{}", o[3].0);
    // The closed session says why there is none (an observation after the crash).
    expect_refusal(&o, 4, "SESSION_REQUIRED", "re_observe");
    assert!(
        o[5].0.starts_with("status: SUCCESS") && o[5].1["reconciling"] == true,
        "{}",
        o[5].0
    );
    expect_refusal(&o, 6, "OUTCOME_UNKNOWN", "ask_user");
    assert!(o[7].1["reconciled"].is_object(), "{}", o[7].0);
    assert!(
        o[8].0.starts_with("status: SUCCESS"),
        "the input after reconciliation runs: {}",
        o[8].0
    );
    let evs = rig.events().await;
    assert_eq!(of(&evs, "ComputerLatched").len(), 1);
    let resolved = of(&evs, "ComputerLatchResolved");
    assert_eq!(resolved.len(), 1);
    assert_eq!(resolved[0]["payload"]["payload"]["by"], "fresh_observation");
    // The crashed input is on the chain as AUTHORIZED and, after it, exactly one UNKNOWN receipt.
    let r = receipts(&evs);
    let unknown: Vec<&Value> = r
        .iter()
        .filter(|x| x["status"] == "UNKNOWN_OUTCOME")
        .collect();
    assert_eq!(unknown.len(), 1, "{r:?}");
    assert_eq!(unknown[0]["reversibility"], "IRREVERSIBLE");
    assert!(
        r.iter()
            .any(|x| x["status"] == "AUTHORIZED" && x["intent_hash"] == unknown[0]["intent_hash"])
    );
    let act = of(&evs, "ComputerActionPerformed")
        .into_iter()
        .find(|a| a["payload"]["payload"]["outcome"] == "UNKNOWN")
        .expect("the unknown input is recorded");
    assert_eq!(act["payload"]["payload"]["kind"], "press");
}

fn alive(pid: u32) -> bool {
    #[cfg(unix)]
    {
        std::process::Command::new("kill")
            .args(["-0", &pid.to_string()])
            .stderr(std::process::Stdio::null())
            .status()
            .is_ok_and(|s| s.success())
    }
    #[cfg(windows)]
    {
        std::process::Command::new("tasklist")
            .args(["/FI", &format!("PID eq {pid}")])
            .output()
            .is_ok_and(|o| String::from_utf8_lossy(&o.stdout).contains(&pid.to_string()))
    }
}

/// QUAL-PX-069 (failure injection): the actuator process is killed from outside while an input is in
/// flight - the outcome is UNKNOWN, the latch holds, the actuator comes back and the model is not
/// let through until the person-approved new session observes.
#[tokio::test(flavor = "multi_thread")]
async fn qual_px_069_killing_the_actuator_mid_action_leaves_the_outcome_unknown_and_the_latch_holds()
 {
    let steps = vec![
        start(FORM),
        fx(call("computer.state", json!({}))),
        st(|s| {
            call(
                "computer.press",
                json!({"element": s.el("Hang after delivery")}),
            )
        }),
        st(|s| call("computer.press", json!({"element": s.el("Submit")}))),
        fx(call("computer.state", json!({}))),
    ];
    let mut rig = run(Opts::default(), steps, approve_all()).await;
    let pid_before = {
        // The fixture writes its pid before it answers the handshake.
        tokio::time::sleep(Duration::from_millis(300)).await;
        rig.actuator_pid()
    };
    when_in_flight(rig.approvals.clone(), "computer.press").await;
    // Skip the start's own approval: the press is the second computer approval.
    for _ in 0..100 {
        if rig
            .approvals
            .lock()
            .unwrap()
            .iter()
            .filter(|(a, ..)| a.tool_name == "computer.press")
            .count()
            >= 1
        {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    kill_pid(pid_before);
    rig.finish().await;
    let o = rig.outs();
    assert!(o[2].0.starts_with("status: UNKNOWNOUTCOME"), "{}", o[2].0);
    assert!(
        o[2].0.contains("ACTUATOR_LOST") || o[2].0.contains("TIMEOUT"),
        "the reason is named: {}",
        o[2].0
    );
    expect_refusal(&o, 3, "OUTCOME_UNKNOWN", "ask_user");
    assert!(o[3].0.contains("\"input_sent\":false"));
    // No session survived the actuator.
    expect_refusal(&o, 4, "SESSION_REQUIRED", "re_observe");
    // The person was asked once for the input that was in flight and never for the refused one.
    let presses = rig.approvals_of("computer.press");
    assert_eq!(presses.len(), 1, "{presses:?}");
    let evs = rig.events().await;
    let r = receipts(&evs);
    assert_eq!(
        r.iter()
            .filter(|x| x["status"] == "UNKNOWN_OUTCOME")
            .count(),
        1,
        "{r:?}"
    );
    let closed = of(&evs, "ComputerSessionClosed");
    assert_eq!(closed.len(), 1);
    assert_eq!(closed[0]["payload"]["payload"]["outcome"], "LATCHED");
    // The actuator that was killed is gone, and a new one is attached (the tools stay offered).
    assert!(!alive(pid_before));
    let view = rig.view().await;
    assert!(view.attached, "{view:?}");
    assert!(view.latch.contains("press"), "{view:?}");
}

/// QUAL-PX-069 (failure injection, the watchdog): an input that does not finish within its deadline
/// is of unknown outcome; the actuator is stopped, the session closed, the task latched.
#[tokio::test(flavor = "multi_thread")]
async fn qual_px_069_the_watchdog_times_an_input_out_into_an_unknown_outcome_and_stops_the_actuator()
 {
    let steps = vec![
        start(FORM),
        fx(call("computer.state", json!({}))),
        st(|s| {
            call(
                "computer.press",
                json!({"element": s.el("Hang after delivery")}),
            )
        }),
        fx(call("computer.state", json!({}))),
        // A new approved session works: the stopped actuator released its lease.
        start(FORM),
        fx(call("computer.state", json!({}))),
    ];
    let opts = Opts {
        env: vec![("MODBIT_COMPUTER_ACTION_MS", "1200".into())],
        ..Opts::default()
    };
    let mut rig = run(opts, steps, approve_all()).await;
    rig.finish().await;
    let o = rig.outs();
    assert!(o[2].0.starts_with("status: UNKNOWNOUTCOME"), "{}", o[2].0);
    assert!(o[2].0.contains("TIMEOUT"), "{}", o[2].0);
    expect_refusal(&o, 3, "SESSION_REQUIRED", "re_observe");
    assert!(
        o[4].0.starts_with("status: SUCCESS") && o[4].1["reconciling"] == true,
        "{}",
        o[4].0
    );
    assert!(o[5].1["reconciled"].is_object(), "{}", o[5].0);
    let evs = rig.events().await;
    let closed = of(&evs, "ComputerSessionClosed");
    assert_eq!(closed[0]["payload"]["payload"]["reason"], "LATCHED");
    // The actuator was not killed: it was stopped, then used again.
    assert_eq!(of(&evs, "ComputerSessionStarted").len(), 2);
}

impl Rig {
    async fn view(&self) -> ViewOut {
        use modbit_protocol::v1::{ComputerRuntimeView, GetComputerRuntime};
        let mut c = self.core.client().await;
        let ack = c
            .command(envelope(
                rand_id(),
                "GetComputerRuntime",
                GetComputerRuntime {
                    task_id: Some(self.task.clone()),
                }
                .encode_to_vec(),
            ))
            .await
            .unwrap();
        let v: ComputerRuntimeView = Client::result(&ack).unwrap();
        ViewOut {
            attached: v.attached,
            configured: v.configured,
            control_id: v.control_id,
            latch: v.latch,
            user_aborted: v.user_aborted,
            non_drivable: v.non_drivable,
            actuator_pid: v.actuator_pid,
            controller: v.controller,
        }
    }

    async fn side_command(
        &self,
        command: &str,
        payload: Vec<u8>,
    ) -> Result<modbit_protocol::v1::CommandAck, modbit_protocol::client::ClientError> {
        let mut c = self.core.client().await;
        c.command(envelope_fenced(rand_id(), command, payload, self.g))
            .await
    }
}

#[derive(Debug)]
#[allow(dead_code)]
struct ViewOut {
    attached: bool,
    configured: bool,
    control_id: String,
    latch: String,
    user_aborted: bool,
    non_drivable: Vec<String>,
    actuator_pid: u32,
    controller: String,
}

/// QUAL-PX-069 (CUC-D05): the person's Stop reaches the actuator at once and is final for the turn.
#[tokio::test(flavor = "multi_thread")]
async fn qual_px_069_the_on_screen_stop_halts_the_actuator_and_is_sticky_for_the_turn() {
    use modbit_protocol::v1::{ComputerStop, ComputerStopped};
    let steps = vec![
        start(FORM),
        fx(call("computer.state", json!({}))),
        st(|s| {
            call(
                "computer.press",
                json!({"element": s.el("Hang after delivery")}),
            )
        }),
        // After the stop: nothing is accepted this turn.
        fx(call("computer.state", json!({}))),
        fx(call("computer.start", json!({"application": FORM}))),
        fx(call("computer.release", json!({}))),
        st(|s| {
            call(
                "computer.press",
                json!({"element": "x/n1", "action": s.control.clone()}),
            )
        }),
    ];
    let mut rig = run(Opts::default(), steps, approve_all()).await;
    when_in_flight(rig.approvals.clone(), "computer.press").await;
    let started = std::time::Instant::now();
    let ack = rig
        .side_command(
            "ComputerStop",
            ComputerStop {
                task_id: Some(rig.task.clone()),
                reason: "on-screen Stop".into(),
            }
            .encode_to_vec(),
        )
        .await
        .unwrap();
    let stopped: ComputerStopped = Client::result(&ack).unwrap();
    assert_eq!(stopped.sessions_closed, 1);
    assert!(
        stopped.actuator_stopped,
        "the actuator confirmed it stopped"
    );
    rig.finish().await;
    // The in-flight input came back within the stop's bound, not its 60 s stall.
    assert!(
        started.elapsed() < Duration::from_secs(8),
        "{:?}",
        started.elapsed()
    );
    let o = rig.outs();
    assert!(
        o[2].0.starts_with("status: UNKNOWNOUTCOME"),
        "stopped while in flight: {}",
        o[2].0
    );
    expect_refusal(&o, 3, "USER_ABORTED", "abandon");
    expect_refusal(&o, 4, "USER_ABORTED", "abandon");
    expect_refusal(&o, 5, "USER_ABORTED", "abandon");
    expect_refusal(&o, 6, "USER_ABORTED", "abandon");
    let evs = rig.events().await;
    assert_eq!(of(&evs, "ComputerUserAborted").len(), 1);
    let closed = of(&evs, "ComputerSessionClosed");
    assert_eq!(closed[0]["payload"]["payload"]["reason"], "USER_ABORTED");
    assert_eq!(closed[0]["payload"]["payload"]["outcome"], "STOPPED");
}

/// QUAL-PX-069 (docs/23): the session's emergency stop stops the actuator and refuses control the same way.
#[tokio::test(flavor = "multi_thread")]
async fn qual_px_069_the_emergency_stop_halts_control_and_stays_in_force() {
    use modbit_protocol::v1::EmergencyStop;
    let steps = vec![
        start(FORM),
        fx(call("computer.state", json!({}))),
        st(|s| {
            call(
                "computer.press",
                json!({"element": s.el("Hang after delivery")}),
            )
        }),
    ];
    let mut rig = run(Opts::default(), steps, approve_all()).await;
    when_in_flight(rig.approvals.clone(), "computer.press").await;
    let started = std::time::Instant::now();
    let ack = rig
        .side_command(
            "EmergencyStop",
            EmergencyStop {
                session_id: Some(rig.session.clone()),
                reason: "test".into(),
            }
            .encode_to_vec(),
        )
        .await
        .unwrap();
    assert_eq!(ack.error_code, "");
    // The run is cancelled with its session's control: the hanging input does not run out its minute.
    let _ = wait_task(&mut rig.client, &rig.task, 30).await;
    assert!(
        started.elapsed() < Duration::from_secs(20),
        "{:?}",
        started.elapsed()
    );
    let mut closed = vec![];
    for _ in 0..50 {
        closed = of(&rig.events().await, "ComputerSessionClosed")
            .into_iter()
            .cloned()
            .collect();
        if !closed.is_empty() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert_eq!(closed.len(), 1, "{closed:?}");
    assert!(
        matches!(
            closed[0]["payload"]["payload"]["reason"].as_str(),
            Some("USER_ABORTED" | "EMERGENCY_STOP" | "RUN_ENDED" | "LATCHED")
        ),
        "{closed:?}"
    );
    assert_eq!(closed[0]["payload"]["payload"]["outcome"], "STOPPED");
    // The actuator holds nothing: a new Core-side session on it can start, which the stop released.
    let v = rig.view().await;
    assert_eq!(v.control_id, "");
}

/// QUAL-PX-070 (CUC-C02): a grant ends with its time, its call cap, the turn and a mode switch, and
/// the watchdog closes a session nobody uses.
#[tokio::test(flavor = "multi_thread")]
async fn qual_px_070_a_grant_ends_with_its_time_its_cap_and_the_watchdog_closes_an_idle_session() {
    // Time.
    let steps = vec![
        start(FORM),
        fx(call("computer.state", json!({}))),
        fx(json!({"delay_ms": 2200, "calls": [{"name": "computer.state", "args": {}}]})),
        fx(call("computer.state", json!({}))),
    ];
    let opts = Opts {
        env: vec![
            ("MODBIT_COMPUTER_GRANT_MS", "1500".into()),
            ("MODBIT_COMPUTER_TICK_MS", "100".into()),
        ],
        ..Opts::default()
    };
    let mut rig = run(opts, steps, approve_all()).await;
    rig.finish().await;
    let o = rig.outs();
    expect_refusal(&o, 2, "GRANT_EXPIRED", "ask_user");
    expect_refusal(&o, 3, "GRANT_EXPIRED", "ask_user");
    let evs = rig.events().await;
    assert_eq!(
        of(&evs, "ComputerSessionClosed")[0]["payload"]["payload"]["reason"],
        "GRANT_EXPIRED"
    );

    // Call cap: three reads against a grant of two.
    let steps = vec![
        start(FORM),
        fx(call("computer.state", json!({}))),
        fx(call("computer.state", json!({}))),
        fx(call("computer.state", json!({}))),
        fx(call("computer.screenshot", json!({}))),
    ];
    let opts = Opts {
        env: vec![("MODBIT_COMPUTER_GRANT_CALLS", "2".into())],
        ..Opts::default()
    };
    let mut rig = run(opts, steps, approve_all()).await;
    rig.finish().await;
    let o = rig.outs();
    assert!(o[1].0.starts_with("status: SUCCESS") && o[2].0.starts_with("status: SUCCESS"));
    expect_refusal(&o, 3, "GRANT_EXPIRED", "ask_user");
    assert!(o[3].0.contains("call cap"), "{}", o[3].0);
    expect_refusal(&o, 4, "GRANT_EXPIRED", "ask_user");

    // Idle: the watchdog closes the session, and the machine is free for the next one.
    let steps = vec![
        start(FORM),
        fx(json!({"delay_ms": 2000, "calls": [{"name": "computer.state", "args": {}}]})),
        start("org.example.fixture.docs"),
        fx(call("computer.state", json!({}))),
    ];
    let opts = Opts {
        env: vec![
            ("MODBIT_COMPUTER_IDLE_MS", "800".into()),
            ("MODBIT_COMPUTER_TICK_MS", "100".into()),
        ],
        ..Opts::default()
    };
    let mut rig = run(opts, steps, approve_all()).await;
    rig.finish().await;
    let o = rig.outs();
    expect_refusal(&o, 1, "SESSION_REQUIRED", "re_observe");
    assert!(o[1].0.contains("watchdog"), "{}", o[1].0);
    assert!(
        o[2].0.starts_with("status: SUCCESS"),
        "the lock was freed: {}",
        o[2].0
    );
    let evs = rig.events().await;
    let closed = of(&evs, "ComputerSessionClosed");
    assert_eq!(closed[0]["payload"]["payload"]["reason"], "WATCHDOG");
}

/// QUAL-PX-070 (CUC-H02): a mode switch never carries a grant across.
#[tokio::test(flavor = "multi_thread")]
async fn qual_px_070_a_mode_switch_ends_the_grant() {
    use modbit_protocol::v1::SetTaskMode;
    let steps = vec![
        start(FORM),
        fx(call("computer.state", json!({}))),
        fx(json!({"delay_ms": 1800, "calls": [{"name": "computer.state", "args": {}}]})),
        // The mode a person set is adopted at the next round boundary.
        fx(call("computer.state", json!({}))),
    ];
    let mut rig = run(Opts::default(), steps, approve_all()).await;
    when_in_flight(rig.approvals.clone(), "computer.start").await;
    // DEBUG carries the same capabilities as AGENT; the grant was given in AGENT.
    let ack = rig
        .side_command(
            "SetTaskMode",
            SetTaskMode {
                task_id: Some(rig.task.clone()),
                mode: modbit_protocol::v1::TaskMode::Debug as i32,
                reason: "test".into(),
            }
            .encode_to_vec(),
        )
        .await;
    assert_eq!(rejected_code(ack), "");
    rig.finish().await;
    let o = rig.outs();
    expect_refusal(&o, 3, "GRANT_EXPIRED", "ask_user");
    assert!(o[3].0.contains("mode"), "{}", o[3].0);
}

/// QUAL-PX-069 (CUC-A02): one controller at a time; a second task's start is refused SESSION_BUSY and
/// succeeds once the first releases.
#[tokio::test(flavor = "multi_thread")]
async fn qual_px_069_a_second_controller_is_refused_until_the_first_lets_go() {
    let (repo, root) = plain_repo(&[("README.md", "# two\n")]);
    let desktop_dir = tempfile::tempdir().unwrap();
    let args = vec![
        "--pid-file".to_owned(),
        desktop_dir
            .path()
            .join("a.pid")
            .to_string_lossy()
            .into_owned(),
    ];
    // Two tasks share one scripted model; the goal tells them apart.
    let a_steps: Vec<Step> = vec![
        fx(plan()),
        start(FORM),
        fx(json!({"delay_ms": 3000, "calls": [{"name": "computer.state", "args": {}}]})),
        fx(call("computer.release", json!({}))),
        fx(complete()),
    ];
    let b_steps: Vec<Step> = vec![
        fx(plan()),
        fx(
            json!({"delay_ms": 1200, "calls": [{"name": "computer.start", "args": {"application": "org.example.fixture.docs"}}]}),
        ),
        fx(
            json!({"delay_ms": 3500, "calls": [{"name": "computer.start", "args": {"application": "org.example.fixture.docs"}}]}),
        ),
        fx(call("computer.state", json!({}))),
        fx(complete()),
    ];
    let (base, seen) = scripted_model_fn(Arc::new(move |body, results| {
        let text = body["messages"].to_string();
        let steps = if text.contains("task A goal") {
            &a_steps
        } else {
            &b_steps
        };
        let st = st_of(body);
        steps
            .get(results)
            .map(|f| f(&st))
            .unwrap_or_else(|| json!({"text": "done"}))
    }))
    .await;
    let dir = tempfile::tempdir().unwrap();
    let mut env: Vec<(String, String)> = model_env(&base);
    env.push((
        "MODBIT_COMPUTER_ACTUATOR".into(),
        fixture_bin().to_string_lossy().into_owned(),
    ));
    env.push((
        "MODBIT_COMPUTER_ACTUATOR_ARGS".into(),
        serde_json::to_string(&args).unwrap(),
    ));
    let env_refs: Vec<(&str, &str)> = env.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
    let core = CoreProcess::spawn_with_env(dir.path(), &env_refs);
    let mut c = core.client().await;
    let (session, g) = session_with_lease(&mut c, 0x10).await;
    let a = create_task(
        &mut c,
        &session,
        g,
        &root,
        0x11,
        "local_trusted",
        "task A goal",
    )
    .await;
    let b = create_task(
        &mut c,
        &session,
        g,
        &root,
        0x13,
        "local_trusted",
        "task B goal",
    )
    .await;
    let (person, _approvals) = spawn_person(&core, &session, g, approve_all()).await;
    start_task_long(&mut c, &a, g, 0x12).await;
    start_task_long(&mut c, &b, g, 0x14).await;
    let _ = wait_task(&mut c, &a, 60).await;
    let _ = wait_task(&mut c, &b, 60).await;
    let bodies = seen.lock().unwrap().clone();
    // The last request of task B carries its results.
    let b_last = bodies
        .iter()
        .rev()
        .find(|x| x["messages"].to_string().contains("task B goal"))
        .unwrap();
    let sb = st_of(b_last);
    let outs: Vec<(String, Value)> = sb.texts.into_iter().zip(sb.outs).skip(1).collect();
    expect_refusal(&outs, 0, "SESSION_BUSY", "retry");
    assert!(
        outs[1].0.starts_with("status: SUCCESS"),
        "once the first released: {}",
        outs[1].0
    );
    person.abort();
    drop(repo);
}

/// A run reduced to its tool results, for the one-fault-at-a-time tests.
async fn quick(opts: Opts, steps: Vec<Step>, decide: Decider) -> (Rig, Vec<(String, Value)>) {
    let mut rig = run(opts, steps, decide).await;
    rig.finish().await;
    let o = rig.outs();
    (rig, o)
}

/// QUAL-PX-069: each member of the taxonomy the actuator and the operating system can raise comes back
/// typed with its documented escalation: a missing permission, a tree that is not there, a capture
/// that is not the canvas, the secure desktop, the person on the keyboard, an input refused before
/// delivery, a read that does not finish.
#[tokio::test(flavor = "multi_thread")]
async fn qual_px_069_the_faults_an_actuator_can_raise_are_typed_with_their_escalations() {
    // PERMISSION_REQUIRED
    let d = desktop_with(|d| d["faults"] = json!({"permission_missing": true}));
    let (_rig, o) = quick(
        Opts {
            desktop: Some(d),
            ..Opts::default()
        },
        vec![
            start(FORM),
            fx(call("computer.state", json!({}))),
            fx(call("computer.screenshot", json!({}))),
        ],
        approve_all(),
    )
    .await;
    expect_refusal(&o, 1, "PERMISSION_REQUIRED", "ask_user");
    expect_refusal(&o, 2, "PERMISSION_REQUIRED", "ask_user");

    // ACCESSIBILITY_UNAVAILABLE
    let d = desktop_with(|d| app_mut(d, FORM)["no_accessibility"] = json!(true));
    let (_rig, o) = quick(
        Opts {
            desktop: Some(d),
            ..Opts::default()
        },
        vec![start(FORM), fx(call("computer.state", json!({})))],
        approve_all(),
    )
    .await;
    expect_refusal(&o, 1, "ACCESSIBILITY_UNAVAILABLE", "re_observe");

    // CAPTURE_FAILED: a frame that is not the canvas, and a fit that is not the one scaler's.
    for fault in [json!({"wrong_size": true}), json!({"bad_letterbox": true})] {
        let d = desktop_with(|d| d["faults"] = fault.clone());
        let (rig, o) = quick(
            Opts {
                desktop: Some(d),
                ..Opts::default()
            },
            vec![start(FORM), fx(call("computer.screenshot", json!({})))],
            approve_all(),
        )
        .await;
        expect_refusal(&o, 1, "CAPTURE_FAILED", "retry");
        // No frame was kept.
        let evs = rig.events().await;
        assert!(
            of(&evs, "ComputerObserved")
                .iter()
                .all(|e| e["payload"]["payload"]["artifact_ref"] == ""),
            "a refused frame leaves no artifact"
        );
    }

    // SECURE_DESKTOP: nothing is captured, no artifact.
    let (rig, o) = quick(
        Opts::default(),
        vec![
            start(FORM),
            fx(call("computer.state", json!({}))),
            st(|s| {
                call(
                    "computer.press",
                    json!({"element": s.el("Lock the screen")}),
                )
            }),
            fx(call("computer.screenshot", json!({}))),
            fx(call("computer.state", json!({}))),
        ],
        approve_all(),
    )
    .await;
    expect_refusal(&o, 3, "SECURE_DESKTOP", "ask_user");
    expect_refusal(&o, 4, "SECURE_DESKTOP", "ask_user");
    assert!(
        of(&rig.events().await, "ComputerObserved")
            .iter()
            .all(|e| e["payload"]["payload"]["artifact_ref"] == "")
    );

    // HUMAN_ACTIVE: a physical key parks the controller; observing after the cooldown takes it back.
    let (_rig, o) = quick(
        Opts {
            env: vec![("MODBIT_COMPUTER_HUMAN_COOLDOWN_MS", "700".into())],
            ..Opts::default()
        },
        vec![
            start(FORM),
            fx(call("computer.state", json!({}))),
            st(|s| {
                call(
                    "computer.press",
                    json!({"element": s.el("Simulate the person")}),
                )
            }),
            st(|s| {
                call(
                    "computer.set_value",
                    json!({"element": s.el("Name"), "value": "x"}),
                )
            }),
            fx(json!({"delay_ms": 1200, "calls": [{"name": "computer.state", "args": {}}]})),
            st(|s| {
                call(
                    "computer.set_value",
                    json!({"element": s.el("Name"), "value": "y"}),
                )
            }),
        ],
        approve_all(),
    )
    .await;
    expect_refusal(&o, 3, "HUMAN_ACTIVE", "retry");
    assert!(o[4].0.starts_with("status: SUCCESS"));
    assert!(
        o[5].0.starts_with("status: SUCCESS"),
        "control is back after observing: {}",
        o[5].0
    );

    // INPUT_FAILED: refused by the application before anything was injected.
    let d = desktop_with(|d| {
        let w = &mut app_mut(d, FORM)["windows"][0]["elements"];
        for e in w.as_array_mut().unwrap() {
            if e["id"] == "submit" {
                e["behaviors"] = json!({"press": [{"op": "FailInput", "code": "INPUT_FAILED"}]});
            }
        }
    });
    let (rig, o) = quick(
        Opts {
            desktop: Some(d),
            ..Opts::default()
        },
        vec![
            start(FORM),
            fx(call("computer.state", json!({}))),
            st(|s| call("computer.press", json!({"element": s.el("Submit")}))),
            fx(call("computer.state", json!({}))),
        ],
        approve_all(),
    )
    .await;
    expect_refusal(&o, 2, "INPUT_FAILED", "re_observe");
    assert!(
        o[3].1["tree"]
            .as_str()
            .unwrap()
            .contains("\"Status\" | value=\"idle\""),
        "nothing happened"
    );
    assert!(
        of(&rig.events().await, "ComputerLatched").is_empty(),
        "a refusal before delivery does not latch"
    );

    // TIMEOUT: a read that does not finish is retryable and latches nothing.
    let d = desktop_with(|d| d["faults"] = json!({"slow_read_ms": 3000}));
    let (rig, o) = quick(
        Opts {
            desktop: Some(d),
            env: vec![("MODBIT_COMPUTER_OBSERVE_MS", "600".into())],
            ..Opts::default()
        },
        vec![start(FORM), fx(call("computer.state", json!({})))],
        approve_all(),
    )
    .await;
    expect_refusal(&o, 1, "TIMEOUT", "retry");
    assert!(of(&rig.events().await, "ComputerLatched").is_empty());
}

/// QUAL-PX-070: a person who says no is believed: nothing happens, and it is not retried blind.
#[tokio::test(flavor = "multi_thread")]
async fn qual_px_070_a_declined_approval_actuates_nothing() {
    let deny_press: Decider = Arc::new(|a, _| {
        if a.tool_name == "computer.press" {
            Decision::Deny
        } else {
            Decision::Approve
        }
    });
    let (rig, o) = quick(
        Opts::default(),
        vec![
            start(FORM),
            fx(call("computer.state", json!({}))),
            st(|s| call("computer.press", json!({"element": s.el("Submit")}))),
            fx(call("computer.state", json!({}))),
        ],
        deny_press,
    )
    .await;
    assert_eq!(error_code(&o[2].0), "APPROVAL_DENIED", "{}", o[2].0);
    assert!(
        o[3].1["tree"]
            .as_str()
            .unwrap()
            .contains("\"Status\" | value=\"idle\"")
    );
    let evs = rig.events().await;
    assert!(of(&evs, "ComputerActionPerformed").is_empty());
    assert!(!receipts(&evs).iter().any(|r| r["status"] == "SUCCESS"
        && r["tool_call_id"] != Value::Null
        && r["status"] == "AUTHORIZED"));
}

fn tool_names_offered(rig: &Rig) -> Vec<Vec<String>> {
    rig.seen
        .lock()
        .unwrap()
        .iter()
        .map(request_tool_names)
        .collect()
}

fn search_catalog(rig: &Rig) -> String {
    rig.seen
        .lock()
        .unwrap()
        .iter()
        .flat_map(|b| b["tools"].as_array().cloned().unwrap_or_default())
        .filter(|t| t["function"]["name"] == "tool.search")
        .map(|t| {
            t["function"]["description"]
                .as_str()
                .unwrap_or_default()
                .to_owned()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// QUAL-PX-069 (fail closed): the tools are not offered to a model unless a configured actuator is
/// attached and the task's profile and mode carry them; a call anyway answers ACTUATOR_UNAVAILABLE.
#[tokio::test(flavor = "multi_thread")]
async fn qual_px_069_the_tools_are_offered_only_with_an_attached_actuator_a_trusted_profile_and_a_mode_that_carries_them()
 {
    use modbit_protocol::v1::{InvokeTool, SetTaskMode, ToolInvoked};
    let probe = vec![fx(call("fs.read", json!({"path": "README.md"})))];

    // With an actuator, in a trusted task in AGENT mode: offered (deferred behind tool.search or declared).
    let (rig, _) = quick(Opts::default(), probe.clone(), approve_all()).await;
    let with = tool_names_offered(&rig).concat().join(" ") + &search_catalog(&rig);
    assert!(
        with.contains("computer.state") || with.contains("computer:"),
        "offered: {with}"
    );

    // No actuator configured: absent from every projection and from the deferred catalog.
    let (rig, _) = quick(
        Opts {
            actuator: false,
            ..Opts::default()
        },
        probe.clone(),
        approve_all(),
    )
    .await;
    let without = tool_names_offered(&rig).concat().join(" ") + &search_catalog(&rig);
    assert!(
        !without.contains("computer"),
        "absent without an actuator: {without}"
    );
    // A call anyway - the person's InvokeTool - answers ACTUATOR_UNAVAILABLE, typed.
    let ack = rig
        .side_command(
            "InvokeTool",
            InvokeTool {
                task_id: Some(rig.task.clone()),
                tool_name: "computer.apps".into(),
                arguments_json: "{}".into(),
                tool_call_id: Some(rand_id()),
                output_budget_bytes: 65536,
            }
            .encode_to_vec(),
        )
        .await
        .unwrap();
    let r: ToolInvoked = Client::result(&ack).unwrap();
    assert_eq!(r.status, "INFRA_FAILURE", "{r:?}");
    assert!(
        r.structured_output_json.contains("ACTUATOR_UNAVAILABLE"),
        "{r:?}"
    );
    let v = rig.view().await;
    assert!(!v.attached && !v.configured, "{v:?}");

    // A configured actuator that cannot be attached is as good as none.
    let (rig, _) = quick(
        Opts {
            env: vec![(
                "MODBIT_COMPUTER_ACTUATOR",
                "/nonexistent/modbit-actuator".into(),
            )],
            actuator: false,
            ..Opts::default()
        },
        probe.clone(),
        approve_all(),
    )
    .await;
    let broken = tool_names_offered(&rig).concat().join(" ") + &search_catalog(&rig);
    assert!(
        !broken.contains("computer"),
        "absent when the actuator cannot start: {broken}"
    );
    let v = rig.view().await;
    assert!(v.configured && !v.attached, "{v:?}");

    // An unattended profile never carries them, actuator or not.
    let (rig, _) = quick(
        Opts {
            profile: "local_autonomous",
            ..Opts::default()
        },
        probe.clone(),
        approve_all(),
    )
    .await;
    let unattended = tool_names_offered(&rig).concat().join(" ") + &search_catalog(&rig);
    assert!(!unattended.contains("computer"), "{unattended}");

    // ASK mode: read-only, no native control; naming a tool anyway is refused by the mode.
    let mut rig = setup(
        Opts::default(),
        vec![
            fx(call("computer.apps", json!({}))),
            fx(call("computer.start", json!({"application": FORM}))),
        ],
        approve_all(),
    )
    .await;
    let ack = rig
        .side_command(
            "SetTaskMode",
            SetTaskMode {
                task_id: Some(rig.task.clone()),
                mode: modbit_protocol::v1::TaskMode::Ask as i32,
                reason: "t".into(),
            }
            .encode_to_vec(),
        )
        .await;
    assert_eq!(rejected_code(ack), "");
    rig.begin().await;
    rig.finish().await;
    let o = rig.outs();
    assert!(
        o[0].0.contains("MODE_POSTURE") || o[0].0.contains("TOOL_NOT_PROJECTED"),
        "{}",
        o[0].0
    );
    assert!(
        o[1].0.contains("MODE_POSTURE") || o[1].0.contains("TOOL_NOT_PROJECTED"),
        "{}",
        o[1].0
    );
    assert!(rig.approvals.lock().unwrap().is_empty());
}

fn object_path(rig: &Rig, hash: &str) -> PathBuf {
    rig.dir
        .path()
        .join("core")
        .join("objects")
        .join(&hash[..2])
        .join(&hash[2..])
}

fn stored_frame(rig: &Rig, out: &Value) -> modbit_computer::frame::Frame {
    let digest = out["artifact"]["digest"]
        .as_str()
        .unwrap_or_else(|| panic!("no artifact in {out}"));
    let bytes = std::fs::read(object_path(rig, digest)).expect("the frame is in the object store");
    // Digest verification: the object is what it is named by.
    use sha2::Digest;
    assert_eq!(hex::encode(sha2::Sha256::digest(&bytes)), digest);
    modbit_computer::frame::decode_png(&bytes).unwrap()
}

/// Every object under the Core's store that contains `needle`.
fn objects_containing(rig: &Rig, needle: &str) -> usize {
    fn walk(dir: &std::path::Path, needle: &[u8], n: &mut usize) {
        for e in std::fs::read_dir(dir).into_iter().flatten().flatten() {
            let p = e.path();
            if p.is_dir() {
                walk(&p, needle, n);
            } else if let Ok(b) = std::fs::read(&p)
                && b.windows(needle.len()).any(|w| w == needle)
            {
                *n += 1;
            }
        }
    }
    let mut n = 0;
    walk(
        &rig.dir.path().join("core").join("objects"),
        needle.as_bytes(),
        &mut n,
    );
    n
}

const LOGIN: &str = "org.example.fixture.login";

fn rect_in(f: &modbit_computer::frame::Frame, x: i32, y: i32, w: i32, h: i32) -> bool {
    // A region the secure field covers, shrunk by a margin so the letterbox rounding is not the test.
    f.is_masked(&modbit_computer::model::Rect {
        x: x + 6,
        y: y + 6,
        width: w - 12,
        height: h - 12,
    })
}

/// QUAL-PX-076: a secure or password-like field is masked at the pixel level before the model sees the
/// frame - by the actuator, and by the Core when the actuator did not; a planted secret in a visible
/// label is in no event, object or answer; the codec is content-aware; an identical frame is encoded once.
#[tokio::test(flavor = "multi_thread")]
async fn qual_px_076_secure_fields_are_masked_at_the_pixel_level_and_a_planted_secret_reaches_nothing()
 {
    let steps = vec![
        start(LOGIN),
        fx(call("computer.state", json!({}))),
        fx(call("computer.screenshot", json!({}))),
        fx(call("computer.screenshot", json!({}))),
    ];
    let (rig, o) = quick(Opts::default(), steps, approve_all()).await;
    // The tree withholds the secure values and redacts the label.
    let tree = o[1].1["tree"].as_str().unwrap();
    assert!(!tree.contains("hunter2"), "{tree}");
    assert!(
        !tree.contains("sk-live-ABCDEF0123456789ABCDEF0123456789"),
        "{tree}"
    );
    assert!(tree.contains("value=(withheld)"));
    assert_eq!(
        o[1].1["secure_withheld"], 2,
        "the flagged field and the password-like one"
    );
    // The frame: exactly the canvas; both fields masked; the rest untouched.
    let first = &o[2].1;
    assert_eq!(
        (first["width"].as_u64(), first["height"].as_u64()),
        (Some(1280), Some(800))
    );
    assert_eq!(first["masked"]["by_actuator"], 1, "{first}");
    assert_eq!(
        first["masked"]["by_core"], 1,
        "the password-like field the actuator did not know: {first}"
    );
    let f = stored_frame(&rig, first);
    assert_eq!((f.width, f.height), (1280, 800));
    // Login window 600x400 at (300,200): scale 2, left margin 40.
    assert!(rect_in(&f, 80, 200, 720, 60), "the Password field is black");
    assert!(
        rect_in(&f, 80, 300, 720, 60),
        "the Confirm password field is black"
    );
    assert!(!rect_in(&f, 80, 80, 720, 60), "the Username field is not");
    // Flat user interface is lossless; the identical second capture is not encoded again.
    assert_eq!(first["content"], "flat");
    assert_eq!(first["codec"], "lossless");
    assert_eq!(first["reused_encoding"], false);
    assert_eq!(o[3].1["reused_encoding"], true, "{}", o[3].1);
    assert_eq!(o[3].1["artifact"]["digest"], first["artifact"]["digest"]);
    // The media pipeline carries provenance and the untrusted label.
    assert_eq!(first["media"]["trust"], "UNTRUSTED_WORKSPACE_CONTENT");
    assert!(
        first["media"]["provenance"]["source"]
            .as_str()
            .unwrap()
            .starts_with("computer:org.example.fixture.login")
    );
    // No secret anywhere: not in an event, not in any stored object (results included).
    let evs = rig.events().await;
    let all = serde_json::to_string(&evs).unwrap();
    for secret in ["hunter2", "sk-live-ABCDEF0123456789ABCDEF0123456789"] {
        assert!(!all.contains(secret), "{secret} in an event");
        assert_eq!(
            objects_containing(&rig, secret),
            0,
            "{secret} in a stored object"
        );
    }
    // Events: the frame by digest, the codec, the mask counts.
    let obs: Vec<&Value> = of(&evs, "ComputerObserved")
        .into_iter()
        .filter(|e| e["payload"]["payload"]["kind"] == "screenshot")
        .collect();
    assert_eq!(obs.len(), 2);
    assert_eq!(obs[0]["payload"]["payload"]["masked_by_core"], 1);
    assert_eq!(obs[1]["payload"]["payload"]["reused"], true);
}

/// QUAL-PX-076: an actuator that masks nothing and reports nothing is not believed: the Core masks
/// every secure region from the tree.
#[tokio::test(flavor = "multi_thread")]
async fn qual_px_076_the_core_masks_when_the_actuator_did_not() {
    let d = desktop_with(|d| d["faults"] = json!({"skip_mask": true, "hide_secure_regions": true}));
    let (rig, o) = quick(
        Opts {
            desktop: Some(d),
            ..Opts::default()
        },
        vec![start(LOGIN), fx(call("computer.screenshot", json!({})))],
        approve_all(),
    )
    .await;
    let out = &o[1].1;
    assert_eq!(out["masked"]["by_actuator"], 0);
    assert_eq!(out["masked"]["by_core"], 2, "{out}");
    let f = stored_frame(&rig, out);
    assert!(rect_in(&f, 80, 200, 720, 60) && rect_in(&f, 80, 300, 720, 60));
    assert!(!rect_in(&f, 80, 80, 720, 60));
}

/// QUAL-PX-076: a photographic window is stored lossy at the canvas size.
#[tokio::test(flavor = "multi_thread")]
async fn qual_px_076_a_photographic_window_is_lossy_at_the_same_pixel_size() {
    let (rig, o) = quick(
        Opts::default(),
        vec![
            start("org.example.fixture.docs"),
            fx(call("computer.screenshot", json!({}))),
        ],
        approve_all(),
    )
    .await;
    let out = &o[1].1;
    assert_eq!(out["content"], "photographic");
    assert_eq!(out["lossless"], false);
    assert!(
        out["codec"].as_str().unwrap().starts_with("lossy-q"),
        "{out}"
    );
    let f = stored_frame(&rig, out);
    assert_eq!(
        (f.width, f.height),
        (1280, 800),
        "a lossy rung never changes the pixel size"
    );
}

/// QUAL-PX-076 (CUC-F03): the audit of a 12-input session holds counts by kind, the screenshot count
/// and the outcome - no text typed, no pixel - and every frame follows the retention setting.
#[tokio::test(flavor = "multi_thread")]
async fn qual_px_076_the_audit_of_a_session_holds_counts_and_no_content_and_frames_follow_retention()
 {
    let mut steps: Vec<Step> = vec![start(FORM), fx(call("computer.state", json!({})))];
    for i in 0..6 {
        steps.push(st(move |s| {
            call(
                "computer.set_value",
                json!({"element": s.el("Name"), "value": format!("typed-secret-{i}")}),
            )
        }));
    }
    for _ in 0..5 {
        steps.push(st(|s| {
            call(
                "computer.press",
                json!({"element": s.el("Quantity"), "action": "focus"}),
            )
        }));
    }
    steps.push(st(|s| {
        call(
            "computer.press",
            json!({"element": s.el("Simulate the person")}),
        )
    }));
    steps.push(fx(call("computer.screenshot", json!({}))));
    steps.push(fx(call("computer.release", json!({}))));
    let opts = Opts {
        env: vec![
            ("MODBIT_COMPUTER_RETENTION_SECS", "10".into()),
            // The person pressing a key parks the controller; keep the cooldown short.
            ("MODBIT_COMPUTER_HUMAN_COOLDOWN_MS", "200".into()),
        ],
        ..Opts::default()
    };
    let (rig, o) = quick(opts, steps, approve_all()).await;
    assert!(
        o.iter().all(|(t, _)| t.starts_with("status: SUCCESS")),
        "{:?}",
        o.iter().map(|x| &x.0).collect::<Vec<_>>()
    );
    let evs = rig.events().await;
    let closed = of(&evs, "ComputerSessionClosed");
    assert_eq!(closed.len(), 1);
    let c = &closed[0]["payload"]["payload"];
    assert_eq!(c["actions_by_kind"], json!({"press": 6, "set_value": 6}));
    assert_eq!(c["screenshots"], 1);
    assert_eq!(c["outcome"], "OK");
    assert_eq!(
        c["approval_ids"].as_array().unwrap().len(),
        13,
        "the grant and twelve inputs"
    );
    let scan: String = evs
        .iter()
        .filter(|e| {
            e["event_type"]
                .as_str()
                .is_some_and(|t| t.starts_with("Computer"))
        })
        .map(|e| e.to_string())
        .collect();
    assert!(!scan.contains("typed-secret"), "no text typed in the audit");
    // Retention: the frame is removed from the object store, the record of it stays.
    let digest = o[o.len() - 2].1["artifact"]["digest"]
        .as_str()
        .unwrap()
        .to_owned();
    assert!(object_path(&rig, &digest).exists());
    let mut gone = false;
    for _ in 0..120 {
        if !object_path(&rig, &digest).exists() {
            gone = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
    assert!(gone, "the frame outlived its retention");
    let mut expired = vec![];
    for _ in 0..40 {
        expired = of(&rig.events().await, "ComputerArtifactExpired")
            .into_iter()
            .cloned()
            .collect();
        if !expired.is_empty() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
    assert_eq!(expired.len(), 1, "{expired:?}");
    assert_eq!(expired[0]["payload"]["payload"]["artifact_ref"], digest);
}

/// A restart with a session open and an input in flight.
#[tokio::test(flavor = "multi_thread")]
async fn qual_px_069_a_core_killed_with_a_session_and_an_input_in_flight_leaves_nothing_held() {
    use modbit_protocol::v1::{
        AcquireSessionLease, ComputerLatchResolved, ResolveComputerLatch, SessionLeaseAcquired,
    };
    let steps = vec![
        start(FORM),
        fx(call("computer.state", json!({}))),
        st(|s| {
            call(
                "computer.press",
                json!({"element": s.el("Hang after delivery")}),
            )
        }),
    ];
    let mut rig = run(Opts::default(), steps, approve_all()).await;
    when_in_flight(rig.approvals.clone(), "computer.press").await;
    let old_actuator = rig.actuator_pid();
    rig.core.kill();
    // The actuator stops injecting when the Core's pipe closes: it does not outlive the Core.
    let mut gone = false;
    for _ in 0..80 {
        if !alive(old_actuator) {
            gone = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert!(gone, "the actuator outlived its Core");
    // A new Core on the same data directory.
    let env_refs: Vec<(&str, &str)> = rig
        .env
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();
    rig.core = CoreProcess::spawn_with_env(rig.dir.path(), &env_refs);
    let mut c = rig.core.client().await;
    let ack = c
        .command(envelope(
            rand_id(),
            "AcquireSessionLease",
            AcquireSessionLease {
                session_id: Some(rig.session.clone()),
                owner: "test2".into(),
            }
            .encode_to_vec(),
        ))
        .await
        .unwrap();
    let l: SessionLeaseAcquired = Client::result(&ack).unwrap();
    rig.g = Some(l.lease_generation);
    let evs = replay(&rig.core, &rig.session).await;
    // The session the dead Core left open is closed on the record; the in-flight input is latched.
    let closed = of(&evs, "ComputerSessionClosed");
    assert_eq!(closed.len(), 1, "{closed:?}");
    assert_eq!(closed[0]["payload"]["payload"]["reason"], "CORE_RESTART");
    let latched = of(&evs, "ComputerLatched");
    assert_eq!(latched.len(), 1, "{latched:?}");
    assert!(
        latched[0]["payload"]["payload"]["reason"]
            .as_str()
            .unwrap()
            .contains("CORE_RESTART")
    );
    let v = rig.view().await;
    assert!(v.attached, "a new actuator is attached: {v:?}");
    assert_eq!(v.control_id, "", "no session survived: {v:?}");
    assert!(v.latch.contains("press"), "{v:?}");
    assert_ne!(v.actuator_pid, old_actuator);
    // The person resolves it; the refusal that stood ends.
    let ack = c
        .command(envelope_fenced(
            rand_id(),
            "ResolveComputerLatch",
            ResolveComputerLatch {
                task_id: Some(rig.task.clone()),
                note: "looked".into(),
            }
            .encode_to_vec(),
            rig.g,
        ))
        .await
        .unwrap();
    let r: ComputerLatchResolved = Client::result(&ack).unwrap();
    assert!(r.was_latched);
    assert_eq!(rig.view().await.latch, "");
    let evs = replay(&rig.core, &rig.session).await;
    assert_eq!(
        of(&evs, "ComputerLatchResolved")[0]["payload"]["payload"]["by"],
        "person"
    );
}

const CHILD_MARKER: &str = "You operate one native application";

/// One scripted model for a parent and the child it spawns: the child's context carries the
/// profile's own words, which the parent's never do.
async fn routed(parent: Vec<Step>, child: Vec<Step>) -> (String, Seen) {
    routed_on(CHILD_MARKER, parent, child).await
}

async fn routed_on(marker: &'static str, parent: Vec<Step>, child: Vec<Step>) -> (String, Seen) {
    scripted_model_fn(Arc::new(move |body, results| {
        let steps = if body["messages"].to_string().contains(marker) {
            &child
        } else {
            &parent
        };
        let st = st_of(body);
        steps
            .get(results)
            .map(|f| f(&st))
            .unwrap_or_else(|| json!({"text": "I have nothing further to do."}))
    }))
    .await
}

fn spawn_gui(objective: &str, healthy: Option<&str>, mode: &str) -> Step {
    let mut args = json!({
        "idempotency_key": "gui-1",
        "objective": objective,
        "write_scope": [],
        "mode": mode,
        "profile": "computer-use",
    });
    if let Some(h) = healthy {
        args["environment_healthy"] = json!(h);
    }
    fx(call("agent.spawn", args))
}

fn parent_plan() -> Step {
    fx(call(
        "plan.update",
        json!({"outcome": "have a child operate the application", "expected_files": [], "protected_effects": ["agent", "computer"]}),
    ))
}

/// The `SubagentResult` the parent's log holds for its child, read from the object store.
async fn child_envelope(rig: &Rig) -> Value {
    let evs = rig.events().await;
    let r = of(&evs, "SubagentResultRecorded");
    let r = r.last().unwrap_or_else(|| panic!("no result envelope"));
    let hash = r["payload"]["payload"]["result_ref"].as_str().unwrap();
    serde_json::from_slice(&std::fs::read(object_path(rig, hash)).unwrap()).unwrap()
}

/// QUAL-PX-075: the parent spawns the profile for a GUI task; the child's tools are the profile's
/// and nothing else, a shell that would drive the GUI is refused, its handles are its own and its
/// approvals are the person's, and its envelope carries the final state, the actions and the evidence.
#[tokio::test(flavor = "multi_thread")]
async fn qual_px_075_a_computer_use_child_operates_the_application_inside_its_profile_and_reports()
{
    let parent = vec![
        parent_plan(),
        spawn_gui(
            "submit the order form for Ada",
            Some("the fixture order form is running and idle (computer.apps lists it)"),
            "FOREGROUND",
        ),
        fx(call("agent.result", json!({"idempotency_key": "gui-1"}))),
        // The child's handles are not the parent's: this task has no control session.
        fx(call("computer.press", json!({"element": "s00000000/n1"}))),
    ];
    let child = vec![
        fx(call(
            "plan.update",
            json!({"outcome": "submit the form", "expected_files": [], "protected_effects": ["computer"]}),
        )),
        start(FORM),
        fx(call("computer.state", json!({}))),
        // Driving the interface through the shell is refused before any effector.
        fx(call(
            "shell.exec",
            json!({"argv": ["osascript", "-e", "tell application \"System Events\" to keystroke \"a\""]}),
        )),
        // A shell for non-GUI work is fine.
        fx(call("shell.exec", json!({"argv": ["git", "--version"]}))),
        st(|s| {
            call(
                "computer.set_value",
                json!({"element": s.el("Name"), "value": "Ada"}),
            )
        }),
        fx(call("computer.screenshot", json!({}))),
        st(|s| call("computer.press", json!({"element": s.el("Submit")}))),
        fx(call("computer.state", json!({}))),
        fx(call("computer.release", json!({}))),
        fx(
            json!({"calls": [{"name": "task.complete", "args": {"summary": "submitted the order for Ada; the form reads submitted", "self_review": {"findings": []}}}]}),
        ),
    ];
    let (base, seen) = routed(parent, child).await;
    let mut rig = setup_model(Opts::default(), base, seen, approve_all()).await;
    rig.begin().await;
    rig.finish().await;
    let bodies = rig.seen.lock().unwrap().clone();
    // The child's tools: read, search, non-GUI shell and computer.*; nothing that writes or delegates.
    let child_bodies: Vec<&Value> = bodies
        .iter()
        .filter(|b| b["messages"].to_string().contains(CHILD_MARKER))
        .collect();
    assert!(!child_bodies.is_empty());
    let allowed: Vec<&str> = vec![
        "fs.read",
        "fs.list",
        "fs.glob",
        "fs.stat",
        "search.exact",
        "search.regex",
        "search.paths",
        "search.retrieve",
        "shell.exec",
        "shell.read",
        "shell.cancel",
    ];
    for b in &child_bodies {
        for n in request_tool_names(b) {
            let harness = [
                "plan.update",
                "task.complete",
                "user.ask",
                "tool.search",
                "context.fast",
                "context.pack",
                "verify.run",
                "repair.attempt",
                "proc.exec",
            ];
            assert!(
                allowed.contains(&n.as_str())
                    || n.starts_with("computer.")
                    || n.starts_with("proc.")
                    || harness.contains(&n.as_str()),
                "the child was offered `{n}`"
            );
        }
    }
    let offered: String = child_bodies
        .iter()
        .flat_map(|b| request_tool_names(b))
        .collect::<Vec<_>>()
        .join(" ");
    assert!(
        offered.contains("computer.press") || offered.contains("computer.state"),
        "{offered}"
    );
    assert!(
        !offered.contains("change.apply")
            && !offered.contains("agent.")
            && !offered.contains("browser."),
        "{offered}"
    );
    // The approvals were the person's, asked of the child's task, bound to the application.
    let asked = rig.approvals.lock().unwrap().clone();
    assert_eq!(asked.len(), 3, "start + set_value + press: {asked:?}");
    assert!(
        asked
            .iter()
            .all(|(a, _, _)| a.task_id != Some(rig.task.clone())),
        "the child's task, not the parent's"
    );
    // The shell attempt was refused, nothing ran.
    let evs = rig.events().await;
    let child_task = {
        let admitted = of(&evs, "SubagentAdmitted");
        admitted[0]["payload"]["payload"]["child_task_id"]
            .as_str()
            .unwrap()
            .to_owned()
    };
    let child_sid = hex::decode(child_task.replace('-', "")).unwrap();
    // The child's own transcript (its last request) shows the refusal.
    let last_child = child_bodies.last().unwrap();
    let cst = st_of(last_child);
    assert!(
        cst.texts
            .iter()
            .any(|t| t.contains("GUI_DRIVE_VIA_SHELL_REFUSED")),
        "{:?}",
        cst.texts
    );
    let _ = child_sid;
    // The envelope: final state, actions, evidence.
    let env = child_envelope(&rig).await;
    assert_eq!(env["status"], "COMPLETED", "{env}");
    let c = &env["computer"];
    assert_eq!(c["applications"], json!([FORM]), "{env}");
    assert_eq!(c["actions_by_kind"], json!({"press": 1, "set_value": 1}));
    assert_eq!(c["screenshots"], 1);
    assert!(
        c["final_state"]["summary"]
            .as_str()
            .unwrap()
            .contains("submitted")
    );
    assert!(
        c["evidence_refs"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e.as_str().unwrap().starts_with("computer_frame:")),
        "{c}"
    );
    assert!(
        env["evidence_refs"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e.as_str().unwrap().starts_with("computer_frame:"))
    );
    // The parent holds no handle of the child's: its own press found no session.
    let pb = bodies
        .iter()
        .rev()
        .find(|b| !b["messages"].to_string().contains(CHILD_MARKER))
        .unwrap();
    let pst = st_of(pb);
    assert_eq!(
        error_code(pst.texts.last().unwrap()),
        "SESSION_REQUIRED",
        "{:?}",
        pst.texts.last()
    );
}

/// QUAL-PX-075: the profile starts only where a person can approve each input, an actuator is attached,
/// the mode carries computer use and the parent has recorded a healthy environment.
#[tokio::test(flavor = "multi_thread")]
async fn qual_px_075_the_profile_is_refused_where_it_cannot_be_attended_or_was_not_prepared() {
    let healthy = Some("the fixture order form is running and idle");
    let spawn_steps = |mode_a: &str| -> Vec<Step> {
        vec![
            parent_plan(),
            // Detached: its approvals could not be asked of the person while it works.
            spawn_gui("operate the form", healthy, "BACKGROUND"),
            // No word from the parent that the environment is fit.
            spawn_gui("operate the form", None, mode_a),
            // A word that says nothing.
            spawn_gui("operate the form", Some("ok"), mode_a),
        ]
    };
    let (base, seen) = routed(spawn_steps("FOREGROUND"), vec![]).await;
    let mut rig = setup_model(Opts::default(), base, seen, approve_all()).await;
    rig.begin().await;
    rig.finish().await;
    let o = rig.outs();
    assert_eq!(error_code(&o[0].0), "PROFILE_MODE", "{}", o[0].0);
    assert_eq!(
        error_code(&o[1].0),
        "ENVIRONMENT_NOT_RECORDED",
        "{}",
        o[1].0
    );
    assert_eq!(
        error_code(&o[2].0),
        "ENVIRONMENT_NOT_RECORDED",
        "{}",
        o[2].0
    );
    assert!(rig.approvals.lock().unwrap().is_empty());
    assert!(
        of(&rig.events().await, "SubagentAdmitted").is_empty(),
        "nothing was admitted"
    );

    // No actuator on this machine.
    let (base, seen) = routed(
        vec![
            parent_plan(),
            spawn_gui("operate the form", healthy, "FOREGROUND"),
        ],
        vec![],
    )
    .await;
    let mut rig = setup_model(
        Opts {
            actuator: false,
            ..Opts::default()
        },
        base,
        seen,
        approve_all(),
    )
    .await;
    rig.begin().await;
    rig.finish().await;
    assert_eq!(
        error_code(&rig.outs()[0].0),
        "ACTUATOR_UNAVAILABLE",
        "{}",
        rig.outs()[0].0
    );

    // An unattended run: nobody to approve.
    let (base, seen) = routed(
        vec![
            parent_plan(),
            spawn_gui("operate the form", healthy, "FOREGROUND"),
        ],
        vec![],
    )
    .await;
    let mut rig = setup_model(
        Opts {
            profile: "local_autonomous",
            ..Opts::default()
        },
        base,
        seen,
        approve_all(),
    )
    .await;
    rig.begin().await;
    rig.finish().await;
    let t = rig.outs()[0].0.clone();
    assert!(
        error_code(&t) == "PROFILE_UNATTENDED"
            || t.contains("TOOL_NOT_PROJECTED")
            || t.contains("PROFILE"),
        "{t}"
    );

    // ASK mode: read-only, no children at all.
    use modbit_protocol::v1::SetTaskMode;
    let (base, seen) = routed(
        vec![
            parent_plan(),
            spawn_gui("operate the form", healthy, "FOREGROUND"),
        ],
        vec![],
    )
    .await;
    let mut rig = setup_model(Opts::default(), base, seen, approve_all()).await;
    let ack = rig
        .side_command(
            "SetTaskMode",
            SetTaskMode {
                task_id: Some(rig.task.clone()),
                mode: modbit_protocol::v1::TaskMode::Ask as i32,
                reason: "t".into(),
            }
            .encode_to_vec(),
        )
        .await;
    assert_eq!(rejected_code(ack), "");
    rig.begin().await;
    rig.finish().await;
    let t = rig.outs()[0].0.clone();
    assert!(
        t.contains("MODE_POSTURE")
            || t.contains("TOOL_NOT_PROJECTED")
            || t.contains("PROFILE_MODE"),
        "{t}"
    );
    assert!(of(&rig.events().await, "SubagentAdmitted").is_empty());
}

/// QUAL-PX-075: an ordinary child never inherits native control: it has no computer capability.
#[tokio::test(flavor = "multi_thread")]
async fn qual_px_075_an_ordinary_child_holds_no_computer_capability() {
    let parent = vec![
        parent_plan(),
        fx(call(
            "agent.spawn",
            json!({"idempotency_key": "plain-1", "objective": "look around", "write_scope": [], "mode": "FOREGROUND"}),
        )),
    ];
    let child = vec![
        fx(call(
            "plan.update",
            json!({"outcome": "look", "expected_files": [], "protected_effects": ["computer"]}),
        )),
        fx(call("computer.apps", json!({}))),
        fx(call("computer.start", json!({"application": FORM}))),
        fx(
            json!({"calls": [{"name": "task.complete", "args": {"summary": "nothing to do", "self_review": {"findings": []}}}]}),
        ),
    ];
    let (base, seen) = routed_on("you are a subagent inside this capsule", parent, child).await;
    let mut rig = setup_model(Opts::default(), base, seen, approve_all()).await;
    rig.begin().await;
    rig.finish().await;
    let bodies = rig.seen.lock().unwrap().clone();
    let child_bodies: Vec<&Value> = bodies
        .iter()
        .filter(|b| {
            b["messages"]
                .to_string()
                .contains("you are a subagent inside this capsule")
        })
        .collect();
    assert!(!child_bodies.is_empty());
    let offered: String = child_bodies
        .iter()
        .flat_map(|b| request_tool_names(b))
        .collect::<Vec<_>>()
        .join(" ");
    assert!(!offered.contains("computer."), "{offered}");
    let cst = st_of(child_bodies.last().unwrap());
    // Refused: not offered, not leased; and nobody was asked.
    assert!(
        cst.texts
            .iter()
            .skip(1)
            .take(2)
            .all(|t| !t.starts_with("status: SUCCESS")),
        "{:?}",
        cst.texts
    );
    assert!(rig.approvals.lock().unwrap().is_empty());
}

/// QUAL-PX-075 (CUC-D06): four failed attempts in a row stop the child, which returns the blocker report.
#[tokio::test(flavor = "multi_thread")]
async fn qual_px_075_a_child_that_fails_four_times_stops_and_returns_the_blocker_report() {
    let healthy = Some("the fixture order form is running and idle");
    let bad = |n: u32| json!({"name": "computer.press", "args": {"element": format!("s0/n{n}")}});
    let parent = vec![
        parent_plan(),
        spawn_gui("press the buttons", healthy, "FOREGROUND"),
    ];
    let child = vec![
        fx(call(
            "plan.update",
            json!({"outcome": "press", "expected_files": [], "protected_effects": ["computer"]}),
        )),
        start(FORM),
        fx(call("computer.state", json!({}))),
        // Four attempts in one turn, all refused: the fourth stops the child.
        fx(json!({"calls": [bad(1), bad(2), bad(3), bad(4)]})),
        // Never reached.
        fx(call("computer.state", json!({}))),
    ];
    let (base, seen) = routed(parent, child).await;
    let mut rig = setup_model(Opts::default(), base, seen, approve_all()).await;
    rig.begin().await;
    rig.finish().await;
    let env = child_envelope(&rig).await;
    assert_eq!(env["status"], "WAITING", "{env}");
    let stopped = &env["computer"]["stopped"];
    assert_eq!(stopped["stopped"], true, "{env}");
    assert_eq!(stopped["reason"], "FOUR_FAILED_ATTEMPTS");
    assert!(
        stopped["blocked_by"]
            .as_str()
            .unwrap()
            .contains("TARGET_STALE"),
        "{stopped}"
    );
    assert_eq!(stopped["observed"]["window"], "Order form");
    assert!(
        stopped["observed"]["tree_head"]
            .as_str()
            .unwrap()
            .contains("Submit")
    );
    assert!(!stopped["best_next_step"].as_str().unwrap().is_empty());
    // The child asked nothing more of anyone: one start approval, no input approval.
    assert_eq!(rig.approvals.lock().unwrap().len(), 1);
}

/// QUAL-PX-075 (CUC-D06): three turns with no new observation or action stop the child too.
#[tokio::test(flavor = "multi_thread")]
async fn qual_px_075_a_child_that_goes_three_turns_without_looking_or_acting_stops() {
    let healthy = Some("the fixture order form is running and idle");
    let parent = vec![
        parent_plan(),
        spawn_gui("wait around", healthy, "FOREGROUND"),
    ];
    let read = || fx(call("fs.read", json!({"path": "README.md"})));
    let child = vec![
        fx(call(
            "plan.update",
            json!({"outcome": "x", "expected_files": [], "protected_effects": ["computer"]}),
        )),
        start(FORM),
        fx(call("computer.state", json!({}))),
        read(),
        read(),
        read(),
        read(),
    ];
    let (base, seen) = routed(parent, child).await;
    let mut rig = setup_model(Opts::default(), base, seen, approve_all()).await;
    rig.begin().await;
    rig.finish().await;
    let env = child_envelope(&rig).await;
    assert_eq!(env["status"], "WAITING", "{env}");
    assert_eq!(
        env["computer"]["stopped"]["reason"], "NO_NEW_OBSERVATION_OR_ACTION",
        "{env}"
    );
}

/// QUAL-PX-075 (CUC-D06): a login prompt is a blocker to report, not to enter.
#[tokio::test(flavor = "multi_thread")]
async fn qual_px_075_a_login_prompt_is_reported_as_a_blocker_and_never_entered() {
    let healthy = Some("the fixture login window is open");
    let parent = vec![
        parent_plan(),
        spawn_gui(
            "sign in to the app and read the dashboard",
            healthy,
            "FOREGROUND",
        ),
    ];
    let child = vec![
        fx(call(
            "plan.update",
            json!({"outcome": "sign in", "expected_files": [], "protected_effects": ["computer"]}),
        )),
        start(LOGIN),
        fx(call("computer.state", json!({}))),
        // The model is tempted: the username, the password, the button - in one turn.
        st(|s| {
            json!({"calls": [
                {"name": "computer.set_value", "args": {"element": s.el("Username"), "value": "alice"}},
                {"name": "computer.set_value", "args": {"element": s.el("Password"), "value": "guess"}},
                {"name": "computer.press", "args": {"element": s.el("Sign in")}},
            ]})
        }),
        fx(call("computer.release", json!({}))),
        fx(
            json!({"calls": [{"name": "task.complete", "args": {"summary": "blocked: the app shows a login prompt (username and password); I did not enter credentials", "self_review": {"findings": []}}}]}),
        ),
    ];
    let (base, seen) = routed(parent, child).await;
    let mut rig = setup_model(Opts::default(), base, seen, approve_all()).await;
    rig.begin().await;
    rig.finish().await;
    let bodies = rig.seen.lock().unwrap().clone();
    let child_last = bodies
        .iter()
        .rev()
        .find(|b| b["messages"].to_string().contains(CHILD_MARKER))
        .unwrap();
    let cst = st_of(child_last);
    // The state flagged the blocker; the three inputs were refused, not approved.
    assert_eq!(
        cst.outs[2]["blockers"][0]["kind"], "login",
        "{:?}",
        cst.texts[2]
    );
    for i in 3..=5 {
        assert_eq!(
            error_code(&cst.texts[i]),
            "ACTION_UNSAFE",
            "{}",
            cst.texts[i]
        );
        assert!(cst.texts[i].contains("blocker"), "{}", cst.texts[i]);
    }
    let asked: Vec<String> = rig
        .approvals
        .lock()
        .unwrap()
        .iter()
        .map(|(a, ..)| a.tool_name.clone())
        .collect();
    assert_eq!(
        asked,
        vec!["computer.start"],
        "nobody was asked to approve a credential"
    );
    let env = child_envelope(&rig).await;
    assert_eq!(env["computer"]["actions_by_kind"], json!({}));
    let evs = rig.events().await;
    assert!(of(&evs, "ComputerActionPerformed").is_empty());
}

/// QUAL-PX-075: the child resumes by identity after a Core restart; its control session was closed by
/// the restart, so it needs a new approval, and what it held before is gone.
#[tokio::test(flavor = "multi_thread")]
async fn qual_px_075_the_child_resumes_by_identity_after_a_restart_and_needs_a_new_approval() {
    use modbit_protocol::v1::{
        AcquireSessionLease, SessionLeaseAcquired, StartTask, TaskRunStarted,
    };
    let healthy = Some("the fixture order form is running and idle");
    let parent = vec![
        parent_plan(),
        spawn_gui(
            "press the hanging button, then recover and read the form",
            healthy,
            "FOREGROUND",
        ),
        fx(call("agent.result", json!({"idempotency_key": "gui-1"}))),
    ];
    let child = vec![
        fx(call(
            "plan.update",
            json!({"outcome": "x", "expected_files": [], "protected_effects": ["computer"]}),
        )),
        start(FORM),
        fx(call("computer.state", json!({}))),
        st(|s| {
            call(
                "computer.press",
                json!({"element": s.el("Hang after delivery")}),
            )
        }),
        // After the restart: nothing is held any more.
        fx(call("computer.state", json!({}))),
        start(FORM),
        fx(call("computer.state", json!({}))),
        fx(
            json!({"calls": [{"name": "task.complete", "args": {"summary": "recovered after the restart and read the form", "self_review": {"findings": []}}}]}),
        ),
    ];
    let (base, seen) = routed(parent, child).await;
    let mut rig = setup_model(Opts::default(), base, seen, approve_all()).await;
    rig.begin().await;
    when_in_flight(rig.approvals.clone(), "computer.press").await;
    let child_task = {
        let evs = rig.events().await;
        let admitted = of(&evs, "SubagentAdmitted");
        assert_eq!(admitted.len(), 1);
        admitted[0]["payload"]["payload"]["child_task_id"]
            .as_str()
            .unwrap()
            .to_owned()
    };
    rig.person.abort();
    rig.core.kill();
    let env_refs: Vec<(&str, &str)> = rig
        .env
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();
    rig.core = CoreProcess::spawn_with_env(rig.dir.path(), &env_refs);
    let mut c = rig.core.client().await;
    let ack = c
        .command(envelope(
            rand_id(),
            "AcquireSessionLease",
            AcquireSessionLease {
                session_id: Some(rig.session.clone()),
                owner: "resumer".into(),
            }
            .encode_to_vec(),
        ))
        .await
        .unwrap();
    let l: SessionLeaseAcquired = Client::result(&ack).unwrap();
    rig.g = Some(l.lease_generation);
    // The restart closed the child's control session on its record, and latched the input in flight.
    let evs = replay(&rig.core, &rig.session).await;
    let closed = of(&evs, "ComputerSessionClosed");
    assert_eq!(closed.len(), 1);
    assert_eq!(closed[0]["payload"]["payload"]["reason"], "CORE_RESTART");
    assert_eq!(
        closed[0]["task_id"].as_str().unwrap().replace('-', ""),
        child_task.replace('-', "")
    );
    // A new person for the new Core; resume the parent, and the child comes back with it.
    let (person, approvals) = spawn_person(&rig.core, &rig.session, rig.g, approve_all()).await;
    rig.person = person;
    rig.approvals = approvals;
    rig.client = c;
    let ack = rig
        .client
        .command(envelope_fenced(
            rand_id(),
            "StartTask",
            StartTask {
                task_id: Some(rig.task.clone()),
                endpoint: String::new(),
                model: "gpt-5".into(),
                max_turns: 120,
                max_tool_calls: 0,
                max_no_progress_turns: 100,
                skills: vec![],
                ..Default::default()
            }
            .encode_to_vec(),
            rig.g,
        ))
        .await
        .unwrap();
    let started: TaskRunStarted = Client::result(&ack).unwrap();
    assert!(started.resumed);
    rig.finish().await;
    let evs = rig.events().await;
    // The same child, admitted once; a second session, asked for again by the person.
    assert_eq!(of(&evs, "SubagentAdmitted").len(), 1, "no second admission");
    let starts = of(&evs, "ComputerSessionStarted");
    assert_eq!(starts.len(), 2, "{starts:?}");
    assert_ne!(
        starts[0]["payload"]["payload"]["control_id"],
        starts[1]["payload"]["payload"]["control_id"]
    );
    assert_eq!(starts[1]["payload"]["payload"]["reconciling"], true);
    let new_asks = rig.approvals_of("computer.start");
    assert_eq!(
        new_asks.len(),
        1,
        "the new session needed a new approval: {new_asks:?}"
    );
    assert_eq!(
        of(&evs, "ComputerLatchResolved")[0]["payload"]["payload"]["by"],
        "fresh_observation"
    );
    let env = child_envelope(&rig).await;
    assert_eq!(env["status"], "COMPLETED", "{env}");
    assert_eq!(env["child_task_id"], child_task);
}
