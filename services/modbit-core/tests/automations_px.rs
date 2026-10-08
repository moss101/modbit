//! QUAL-PX-082 .. QUAL-PX-084 on the real Core: the actual `modbit-core`
//! binary over its real socket, its real SQLite event store, the real
//! Capability Kernel and task runtime, a real Git repository, and the
//! repository's standard model stand-in (a scripted OpenAI-compatible
//! server). Time is controlled through the Core's time-source file, so a
//! schedule is exercised exactly, never by waiting for a wall clock.
//!
//! * PX-082: definitions are typed, bounded data; validation rejects what it
//!   cannot prove safe; a person's approval of the exact hash enables one; a
//!   repository-supplied definition is inert until its exact bytes are
//!   approved; a model has no way to create, edit or enable one.
//! * PX-083: a schedule fires once per slot, a firing is idempotent per event
//!   id, a Core killed between the firing and the dispatch neither loses nor
//!   repeats the run, missed slots follow the declared policy, concurrency
//!   and budgets are enforced.
//! * PX-084: an unattended run is held to its principal's ceiling, an
//!   approval it needs parks and expires into a typed cancellation, a
//!   hostile trigger payload changes nothing, and the kill switches stop it.
#![cfg(unix)]

mod px_common;

use std::path::PathBuf;
use std::time::{Duration, Instant};

use modbit_protocol::client::{Client, ClientError};
use modbit_protocol::v1::{
    AckAutomationAttention, ApprovalList, AutomationDetail, AutomationFireReport, AutomationList,
    AutomationRunList, AutomationRunStarted, AutomationRunView, AutomationValidation,
    AutomationView, CreateAutomation, CreateTask, DisableAutomation, EnableAutomation,
    FireAutomationEvent, Id, KillAutomation, KillReport, ListApprovals, ListAutomationRuns,
    ListAutomations, LoadRepositoryAutomations, PauseAutomation, RepositoryAutomations,
    RunAutomation, TaskCreated, UpdateAutomation, ValidateAutomation,
};
use prost::Message;
use px_common::*;
use serde_json::{Value, json};

// ---- commands ----

async fn cmd<R: Message + Default>(
    c: &mut Client,
    command_type: &str,
    msg: impl Message,
) -> Result<R, (String, String)> {
    cmd_g(c, command_type, msg, None).await
}

async fn cmd_g<R: Message + Default>(
    c: &mut Client,
    command_type: &str,
    msg: impl Message,
    g: Option<u64>,
) -> Result<R, (String, String)> {
    match c
        .command(envelope_fenced(
            rand_id(),
            command_type,
            msg.encode_to_vec(),
            g,
        ))
        .await
    {
        Ok(ack) => Ok(Client::result(&ack).unwrap()),
        Err(ClientError::Rejected { code, message }) => Err((code, message)),
        Err(e) => panic!("{command_type}: {e}"),
    }
}

async fn cmd_id<R: Message + Default>(
    c: &mut Client,
    id: Id,
    command_type: &str,
    msg: impl Message,
) -> Result<(R, i32), (String, String)> {
    match c
        .command(envelope(id, command_type, msg.encode_to_vec()))
        .await
    {
        Ok(ack) => Ok((Client::result(&ack).unwrap(), ack.status)),
        Err(ClientError::Rejected { code, message }) => Err((code, message)),
        Err(e) => panic!("{command_type}: {e}"),
    }
}

// ---- the fixture ----

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64
}

struct Fx {
    core: CoreProcess,
    c: Client,
    session: Id,
    g: Option<u64>,
    repo: tempfile::TempDir,
    root: String,
    data: tempfile::TempDir,
    clock: PathBuf,
    seen: Seen,
    base: String,
    env: Vec<(String, String)>,
}

fn set_clock(path: &PathBuf, ms: i64) {
    std::fs::write(path, ms.to_string()).unwrap();
}

fn spawn_core(
    data: &std::path::Path,
    base: &str,
    clock: &PathBuf,
    extra: &[(&str, &str)],
) -> (CoreProcess, Vec<(String, String)>) {
    let mut env = model_env(base);
    for (k, v) in [
        ("MODBIT_DEFAULT_MODEL", "gpt-5-mini"),
        ("MODBIT_AUTOMATION_TICK_MS", "100"),
        ("MODBIT_AUTOMATION_CLOCK_FILE", clock.to_str().unwrap()),
    ] {
        env.push((k.into(), v.into()));
    }
    for (k, v) in extra {
        env.push(((*k).into(), (*v).into()));
    }
    let refs: Vec<(&str, &str)> = env.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
    (CoreProcess::spawn_with_env(data, &refs), env)
}

/// A Core with a trusted repository, a scripted model and a controlled clock.
async fn fx(script: Vec<Value>, files: &[(&str, &str)], extra: &[(&str, &str)]) -> Fx {
    let data = tempfile::tempdir().unwrap();
    let (repo, root) = plain_repo(files);
    let (base, seen) = scripted_model(script, vec![]).await;
    let clock = data.path().join("clock.txt");
    set_clock(&clock, now_ms());
    let (core, env) = spawn_core(data.path(), &base, &clock, extra);
    let mut c = core.client().await;
    let (session, g) = session_with_lease(&mut c, 0x31).await;
    trust(&mut c, &session, g, &root).await;
    Fx {
        core,
        c,
        session,
        g,
        repo,
        root,
        data,
        clock,
        seen,
        base,
        env,
    }
}

async fn trust(c: &mut Client, session: &Id, g: Option<u64>, root: &str) {
    let _: modbit_protocol::v1::RepositoryTrusted = cmd_g(
        c,
        "TrustRepository",
        modbit_protocol::v1::TrustRepository {
            session_id: Some(session.clone()),
            workspace_root: root.into(),
            scope: String::new(),
        },
        g,
    )
    .await
    .unwrap();
}

fn def(name: &str, triggers: Value, extra: Value) -> String {
    let mut v = json!({
        "schema": "modbit.automation/1",
        "name": name,
        "prompt": "Report whether the default branch has moved ahead of the local one.",
        "triggers": triggers,
    });
    if let Value::Object(m) = extra {
        for (k, val) in m {
            v[k] = val;
        }
    }
    v.to_string()
}

async fn create(
    c: &mut Client,
    json: &str,
    root: &str,
) -> Result<AutomationView, (String, String)> {
    cmd(
        c,
        "CreateAutomation",
        CreateAutomation {
            definition_json: json.into(),
            workspace_root: root.into(),
        },
    )
    .await
}

async fn enable_exact(
    c: &mut Client,
    v: &AutomationView,
) -> Result<AutomationView, (String, String)> {
    cmd(
        c,
        "EnableAutomation",
        EnableAutomation {
            automation_id: v.automation_id.clone(),
            version: v.current_version,
            definition_hash: v.definition_hash.clone(),
            effects: v.effects.clone(),
            capabilities: v.capabilities.clone(),
            paths: v.paths.clone(),
            hosts: v.hosts.clone(),
        },
    )
    .await
}

async fn list(c: &mut Client) -> AutomationList {
    cmd(c, "ListAutomations", ListAutomations {}).await.unwrap()
}

async fn view(c: &mut Client, id: &str) -> AutomationView {
    let d: AutomationDetail = cmd(
        c,
        "GetAutomation",
        modbit_protocol::v1::GetAutomation {
            automation_id: id.into(),
        },
    )
    .await
    .unwrap();
    d.view.unwrap()
}

async fn runs(c: &mut Client, id: &str) -> Vec<AutomationRunView> {
    let l: AutomationRunList = cmd(
        c,
        "ListAutomationRuns",
        ListAutomationRuns {
            automation_id: id.into(),
            task_id: String::new(),
            limit: 500,
        },
    )
    .await
    .unwrap();
    l.runs
}

async fn wait_runs(
    c: &mut Client,
    id: &str,
    secs: u64,
    what: &str,
    pred: impl Fn(&[AutomationRunView]) -> bool,
) -> Vec<AutomationRunView> {
    let deadline = Instant::now() + Duration::from_secs(secs);
    loop {
        let r = runs(c, id).await;
        if pred(&r) {
            return r;
        }
        assert!(
            Instant::now() < deadline,
            "timed out waiting for {what}; runs: {:?}",
            r.iter()
                .map(|x| (x.status.clone(), x.reason.clone(), x.event_id.clone()))
                .collect::<Vec<_>>()
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

fn done(r: &[AutomationRunView]) -> usize {
    r.iter()
        .filter(|x| x.status == "succeeded" || x.status == "failed" || x.status == "cancelled")
        .count()
}

async fn run_now(c: &mut Client, id: &str) -> Result<AutomationRunStarted, (String, String)> {
    cmd(
        c,
        "RunAutomation",
        RunAutomation {
            automation_id: id.into(),
            ..Default::default()
        },
    )
    .await
}

/// The reference run: read, then propose completion with a summary.
fn finish_script() -> Vec<Value> {
    vec![
        json!({"text": "Looking at the repository.", "calls": [{"name": "fs.read", "args": {"path": "README.md"}}]}),
        json!({"calls": [{"name": "plan.update", "args": {"outcome": "report the drift of the default branch", "expected_files": [], "verification": [], "protected_effects": []}}]}),
        json!({"calls": [{"name": "task.complete", "args": {
            "summary": "The default branch has not moved ahead of the local one.",
            "self_review": {"findings": [{"text": "git status read; nothing changed", "resolved": true}], "verification": []}
        }}]}),
    ]
}

fn manual() -> Value {
    json!([{"kind": "manual", "id": "now"}])
}

// =====================================================================
// PX-082
// =====================================================================

/// Definitions are typed, bounded data. The editor's live validation names
/// every problem; a client cannot create a task as an automation; the owner's
/// approval must name the exact version and hash and list the exact reach.
#[tokio::test(flavor = "multi_thread")]
async fn px_082_definitions_validate_version_and_need_the_owners_exact_approval() {
    let mut f = fx(finish_script(), &[("README.md", "x")], &[]).await;
    // Unknown field / trigger / input, a schedule under the floor, an unsafe
    // expression, a capability no automation may hold: each is a typed issue
    // and none is saved.
    let bad = json!({
        "schema": "modbit.automation/1", "name": "bad", "prompt": "p",
        "triggers": [
            {"kind": "schedule", "id": "fast", "cron": "*/2 * * * *"},
            {"kind": "event", "id": "pr", "source": "forge", "event": "pull_request",
             "filters": {"title_regex": "(unclosed"}}
        ],
        "profile": {"effects": "read_only", "capabilities": ["computer.control"]},
    })
    .to_string();
    let v: AutomationValidation = cmd(
        &mut f.c,
        "ValidateAutomation",
        ValidateAutomation {
            definition_json: bad.clone(),
        },
    )
    .await
    .unwrap();
    assert!(!v.ok);
    let codes: Vec<&str> = v.issues.iter().map(|i| i.code.as_str()).collect();
    for want in ["BELOW_FLOOR", "BAD_REGEX", "UNKNOWN_CAPABILITY"] {
        assert!(codes.contains(&want), "{want} in {codes:?}");
    }
    for (doc, code) in [
        (
            json!({"schema":"modbit.automation/1","name":"x","prompt":"p","triggers":[{"kind":"chat_message","id":"c"}]}),
            "UNKNOWN_TRIGGER",
        ),
        (
            json!({"schema":"modbit.automation/1","name":"x","prompt":"p","triggers":[{"kind":"manual","id":"m"}],"shell":"rm -rf /"}),
            "UNKNOWN_FIELD",
        ),
    ] {
        let v: AutomationValidation = cmd(
            &mut f.c,
            "ValidateAutomation",
            ValidateAutomation {
                definition_json: doc.to_string(),
            },
        )
        .await
        .unwrap();
        assert!(!v.ok && v.issues[0].code == code, "{code}: {:?}", v.issues);
    }
    let (code, _) = create(&mut f.c, &bad, &f.root).await.unwrap_err();
    assert_eq!(code, "INVALID_DEFINITION");
    assert!(list(&mut f.c).await.automations.is_empty(), "nothing saved");

    // A valid definition: saved disabled, hash stable and returned.
    let good = def("drift", manual(), json!({}));
    let v1 = create(&mut f.c, &good, &f.root).await.unwrap();
    assert_eq!(v1.state, "NEEDS_APPROVAL");
    assert_eq!(v1.current_version, 1);
    assert_eq!(v1.definition_hash.len(), 64);
    assert!(!v1.needs_listed_approval && v1.effects == "read_only");
    // A run before approval is refused.
    assert_eq!(
        run_now(&mut f.c, &v1.automation_id).await.unwrap_err().0,
        "NOT_ENABLED"
    );

    // The approval names the exact hash; any other is refused.
    let mut wrong = EnableAutomation {
        automation_id: v1.automation_id.clone(),
        version: 1,
        definition_hash: "0".repeat(64),
        effects: "read_only".into(),
        ..Default::default()
    };
    let (code, _) = cmd::<AutomationView>(&mut f.c, "EnableAutomation", wrong.clone())
        .await
        .unwrap_err();
    assert_eq!(code, "APPROVAL_MISMATCH");
    wrong.definition_hash = v1.definition_hash.clone();
    wrong.capabilities = vec!["fs.write".into()];
    let (code, _) = cmd::<AutomationView>(&mut f.c, "EnableAutomation", wrong)
        .await
        .unwrap_err();
    assert_eq!(code, "APPROVAL_LISTS_DIFFER");
    let on = enable_exact(&mut f.c, &v1).await.unwrap();
    assert_eq!(on.state, "ENABLED");
    assert_eq!(
        on.enabled.as_ref().unwrap().definition_hash,
        v1.definition_hash
    );

    // An edit is a new version, disabled until its own approval.
    let edited = def(
        "drift",
        manual(),
        json!({"description": "now with a description"}),
    );
    let v2: AutomationView = cmd(
        &mut f.c,
        "UpdateAutomation",
        UpdateAutomation {
            automation_id: v1.automation_id.clone(),
            definition_json: edited,
        },
    )
    .await
    .unwrap();
    assert_eq!(v2.current_version, 2);
    assert_ne!(v2.definition_hash, v1.definition_hash);
    assert_eq!(
        v2.state, "NEEDS_APPROVAL",
        "an edit disables until approved"
    );
    assert_eq!(
        run_now(&mut f.c, &v1.automation_id).await.unwrap_err().0,
        "NOT_ENABLED"
    );
    let stale = cmd::<AutomationView>(
        &mut f.c,
        "EnableAutomation",
        EnableAutomation {
            automation_id: v1.automation_id.clone(),
            version: 1,
            definition_hash: v1.definition_hash.clone(),
            effects: "read_only".into(),
            ..Default::default()
        },
    )
    .await
    .unwrap_err();
    assert_eq!(
        stale.0, "APPROVAL_MISMATCH",
        "the old version's approval cannot enable the new"
    );
    assert_eq!(enable_exact(&mut f.c, &v2).await.unwrap().state, "ENABLED");

    // A profile that writes needs the approval to list exactly what it asks.
    let writer = def(
        "writer",
        manual(),
        json!({"profile": {"effects": "reversible_write", "capabilities": ["fs.write"], "paths": ["reports/**"]}}),
    );
    let w = create(&mut f.c, &writer, &f.root).await.unwrap();
    assert!(w.needs_listed_approval);
    let mut ask = EnableAutomation {
        automation_id: w.automation_id.clone(),
        version: 1,
        definition_hash: w.definition_hash.clone(),
        effects: "reversible_write".into(),
        capabilities: vec!["fs.write".into()],
        paths: vec![],
        hosts: vec![],
    };
    assert_eq!(
        cmd::<AutomationView>(&mut f.c, "EnableAutomation", ask.clone())
            .await
            .unwrap_err()
            .0,
        "APPROVAL_LISTS_DIFFER"
    );
    ask.paths = vec!["reports/**".into()];
    assert_eq!(
        cmd::<AutomationView>(&mut f.c, "EnableAutomation", ask)
            .await
            .unwrap()
            .state,
        "ENABLED"
    );

    // An untrusted workspace cannot be acted in.
    let other = tempfile::tempdir().unwrap();
    let o = create(
        &mut f.c,
        &def("elsewhere", manual(), json!({})),
        other.path().to_str().unwrap(),
    )
    .await
    .unwrap();
    assert_eq!(
        enable_exact(&mut f.c, &o).await.unwrap_err().0,
        "REPOSITORY_UNTRUSTED"
    );

    // The log is the record: a restarted Core has the same definitions.
    f.core.kill();
    let (core, _) = spawn_core(f.data.path(), &f.base, &f.clock, &[]);
    let mut c = core.client().await;
    let l = list(&mut c).await;
    let names: Vec<_> = l
        .automations
        .iter()
        .map(|a| (a.name.clone(), a.state.clone()))
        .collect();
    assert!(
        names.contains(&("drift".into(), "ENABLED".into())),
        "{names:?}"
    );
    assert!(
        names.contains(&("elsewhere".into(), "NEEDS_APPROVAL".into())),
        "{names:?}"
    );
    f.core = core;
}

/// Nothing a client (and so a model) can send makes a task an automation's,
/// or narrows a lease: the origin and the fields belong to the Core's host.
#[tokio::test(flavor = "multi_thread")]
async fn px_082_a_client_cannot_create_an_automation_task_and_a_tool_cannot_reach_the_ledger() {
    let mut f = fx(finish_script(), &[("README.md", "x")], &[]).await;
    let (s, g) = session_with_lease(&mut f.c, 0x41).await;
    let ack = cmd_g::<TaskCreated>(
        &mut f.c,
        "CreateTask",
        CreateTask {
            session_id: Some(s.clone()),
            goal_text: "x".into(),
            origin: "automation".into(),
            workspace_root: f.root.clone(),
            ..Default::default()
        },
        g,
    )
    .await
    .unwrap_err();
    assert_eq!(ack.0, "AUTOMATION_FIELDS_RESERVED");
    let ack = cmd_g::<TaskCreated>(
        &mut f.c,
        "CreateTask",
        CreateTask {
            session_id: Some(s.clone()),
            goal_text: "x".into(),
            origin: "cli".into(),
            workspace_root: f.root.clone(),
            lease_operations: vec!["fs.read".into()],
            lease_effect_ceiling: "READ_ONLY".into(),
            ..Default::default()
        },
        g,
    )
    .await
    .unwrap_err();
    assert_eq!(ack.0, "AUTOMATION_FIELDS_RESERVED");
    // No tool in the registry reaches an automation.
    let tools = invoke_names(&mut f.c, &s, g, &f.root).await;
    assert!(
        tools
            .iter()
            .all(|t| !t.to_lowercase().contains("automation")),
        "no tool name mentions automations: {tools:?}"
    );
}

async fn invoke_names(c: &mut Client, session: &Id, g: Option<u64>, root: &str) -> Vec<String> {
    let task = create_task(c, session, g, root, 0x61, "local_trusted", "list tools").await;
    let ack = c
        .command(envelope(
            rand_id(),
            "ListTools",
            modbit_protocol::v1::ListTools {
                task_id: Some(task),
            }
            .encode_to_vec(),
        ))
        .await
        .unwrap();
    let l: modbit_protocol::v1::ToolList = Client::result(&ack).unwrap();
    l.tools.into_iter().map(|t| t.name).collect()
}

// =====================================================================
// PX-083
// =====================================================================

const MIN: i64 = 60_000;

async fn session_events(core: &CoreProcess, run: &AutomationRunView) -> Vec<Value> {
    let sid = Id {
        value: hex::decode(&run.session_id).unwrap(),
    };
    replay(core, &sid).await
}

/// Every request body the scripted model has received so far.
fn seen_bodies(f: &Fx) -> Vec<Value> {
    f.seen.lock().unwrap().clone()
}

/// A readable account of a run's session when a test fails.
async fn dump(core: &CoreProcess, run: &AutomationRunView) -> String {
    let mut out = String::new();
    for e in session_events(core, run).await {
        let t = e["event_type"].as_str().unwrap_or_default();
        if t.starts_with("ToolCall") || t.contains("Approval") || t.starts_with("Task") {
            let p = e["payload"].to_string();
            out.push_str(&format!("{t} {}\n", &p[..p.len().min(300)]));
        }
    }
    out
}

/// A schedule fires on a controlled clock and creates exactly one task per
/// slot; the run is a task with origin `automation` whose provenance names
/// the definition, its version and the slot.
#[tokio::test(flavor = "multi_thread")]
async fn px_083_a_schedule_fires_once_per_slot_on_a_controlled_clock_with_provenance() {
    let mut f = fx(finish_script(), &[("README.md", "x")], &[]).await;
    let t0 = now_ms();
    set_clock(&f.clock, t0);
    let v = create(
        &mut f.c,
        &def(
            "nightly",
            json!([{"kind": "schedule", "id": "tick", "every_minutes": 5}]),
            json!({}),
        ),
        &f.root,
    )
    .await
    .unwrap();
    let on = enable_exact(&mut f.c, &v).await.unwrap();
    assert_eq!(on.state, "ENABLED");
    let next = on.triggers[0].next_due_ms;
    assert!(
        next > t0 && next <= t0 + 5 * MIN + 2000,
        "next due {next} after {t0}"
    );
    // Before the slot nothing fires, however long we wait.
    set_clock(&f.clock, t0 + 4 * MIN);
    tokio::time::sleep(Duration::from_millis(600)).await;
    assert!(runs(&mut f.c, &v.automation_id).await.is_empty());
    // The slot: one run.
    set_clock(&f.clock, next + 1000);
    let r = wait_runs(&mut f.c, &v.automation_id, 30, "slot 1 to finish", |r| {
        done(r) == 1
    })
    .await;
    assert_eq!(r.len(), 1);
    assert_eq!(
        r[0].status,
        "succeeded",
        "{:?}\n{}\ntools: {:?}\nlast messages: {:?}",
        r[0],
        dump(&f.core, &r[0]).await,
        request_tool_names(&seen_bodies(&f)[0]),
        seen_bodies(&f)
            .iter()
            .map(|b| b["messages"].as_array().map(|m| m.len()))
            .collect::<Vec<_>>()
    );
    assert_eq!(r[0].trigger_kind, "schedule");
    assert_eq!(r[0].event_id, format!("tick@{next}"));
    assert_eq!(r[0].slot_ms, next);
    // The clock keeps moving inside the slot: still one run, still one task.
    set_clock(&f.clock, next + 30_000);
    tokio::time::sleep(Duration::from_millis(800)).await;
    assert_eq!(runs(&mut f.c, &v.automation_id).await.len(), 1);
    let ev = session_events(&f.core, &r[0]).await;
    let created: Vec<_> = ev
        .iter()
        .filter(|e| e["event_type"] == "TaskCreated")
        .collect();
    assert_eq!(created.len(), 1, "exactly one task for the slot");
    assert_eq!(created[0]["payload"]["payload"]["origin"], "automation");
    let prov = ev
        .iter()
        .find(|e| e["event_type"] == "TaskTriggeredByAutomation")
        .expect("provenance on the task's log");
    assert_eq!(prov["payload"]["payload"]["automation_id"], v.automation_id);
    assert_eq!(prov["payload"]["payload"]["version"], 1);
    assert_eq!(
        prov["payload"]["payload"]["event_id"],
        format!("tick@{next}")
    );
    assert_eq!(
        prov["payload"]["payload"]["definition_hash"],
        v.definition_hash
    );
    assert!(
        prov["payload"]["payload"]["principal"]
            .as_str()
            .unwrap()
            .starts_with("user:")
    );
    // The next slot is the next run.
    let slot2 = next + 5 * MIN;
    set_clock(&f.clock, slot2 + 1000);
    let r = wait_runs(&mut f.c, &v.automation_id, 30, "slot 2 to finish", |r| {
        done(r) == 2
    })
    .await;
    assert_eq!(r.len(), 2);
    assert!(r.iter().all(|x| x.status == "succeeded"));
}

/// A repeated delivery of one event id creates nothing: it is recorded as a
/// duplicate.
#[tokio::test(flavor = "multi_thread")]
async fn px_083_a_duplicate_firing_creates_one_task() {
    let mut f = fx(finish_script(), &[("README.md", "x")], &[]).await;
    let v = create(&mut f.c, &def("once", manual(), json!({})), &f.root)
        .await
        .unwrap();
    enable_exact(&mut f.c, &v).await.unwrap();
    let ask = |id: &str| RunAutomation {
        automation_id: v.automation_id.clone(),
        event_id: id.into(),
        ..Default::default()
    };
    let a: AutomationRunStarted = cmd(&mut f.c, "RunAutomation", ask("delivery-7"))
        .await
        .unwrap();
    assert_eq!(a.status, "running");
    let b: AutomationRunStarted = cmd(&mut f.c, "RunAutomation", ask("delivery-7"))
        .await
        .unwrap();
    assert_eq!(b.reason, "DUPLICATE");
    assert_eq!(b.dispatch_key, a.dispatch_key);
    let r = wait_runs(
        &mut f.c,
        &v.automation_id,
        30,
        "the run and the duplicate",
        |r| done(r) == 1 && r.iter().any(|x| x.reason == "DUPLICATE"),
    )
    .await;
    let real: Vec<_> = r.iter().filter(|x| x.reason != "DUPLICATE").collect();
    assert_eq!(real.len(), 1);
    let ev = session_events(&f.core, real[0]).await;
    assert_eq!(
        ev.iter()
            .filter(|e| e["event_type"] == "TaskCreated")
            .count(),
        1
    );
    // A different delivery id is a different run.
    let c: AutomationRunStarted = cmd(&mut f.c, "RunAutomation", ask("delivery-8"))
        .await
        .unwrap();
    assert_ne!(c.dispatch_key, a.dispatch_key);
    wait_runs(&mut f.c, &v.automation_id, 30, "second delivery", |r| {
        done(r) == 2
    })
    .await;
}
