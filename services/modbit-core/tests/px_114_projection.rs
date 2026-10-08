//! PX-114 (QUAL-PX-114) on the real Core: the `exec_only` tool projection
//! mode, the schema-bytes budget and the per-task / per-model selection.
//!
//! Real: the `modbit-core` binary over its socket, the SQLite event store, a
//! git repository, the QuickJS isolate, the Capability Kernel and the tool
//! pipeline. Stand-in: the model, a scripted OpenAI-compatible server that
//! records every request body. Nothing here is a live-provider result.

mod px_common;

use modbit_protocol::client::Client;
use modbit_protocol::v1::{SetTaskToolProjection, TaskToolProjectionSet};
use prost::Message;
use px_common::*;
use serde_json::json;

const EXEC_SET: [&str; 6] = [
    "plan.update",
    "proc.exec",
    "proc.wait",
    "task.complete",
    "tool.search",
    "user.ask",
];

const PROGRAM: &str = r#"
    const f = await tools.fs.read({ path: "a.txt" });
    await tools.change.apply({ path: "b.txt", op: "create", content: f.content + "beta\n" });
    const sh = await tools.shell.exec({ argv: ["git", "--version"], inherit_env: true });
    return { exit: sh.exit_code };
"#;

fn exec_script() -> Vec<serde_json::Value> {
    vec![
        json!({"calls": [{"name": "plan.update", "args": {"outcome": "derive b.txt from a.txt", "expected_files": ["b.txt"]}}]}),
        json!({"calls": [{"name": "proc.exec", "args": {"program": PROGRAM}}]}),
        json!({"calls": [{"name": "task.complete", "args": {"summary": "derived", "self_review": {"findings": []}}}]}),
    ]
}

fn exec_rules() -> Vec<(String, serde_json::Value)> {
    // On a slow runner the program may outlive proc.exec's inline grace.
    vec![
        (
            "status: RUNNING\nhandle: call_1_0".into(),
            json!({"calls": [{"name": "proc.wait", "args": {"handle": "call_1_0", "timeout_ms": 60000}}]}),
        ),
        (
            "status: COMPLETED\nhandle: call_1_0".into(),
            json!({"calls": [{"name": "task.complete", "args": {"summary": "derived", "self_review": {"findings": []}}}]}),
        ),
    ]
}

fn direct_script() -> Vec<serde_json::Value> {
    vec![
        json!({"calls": [{"name": "plan.update", "args": {"outcome": "derive b.txt from a.txt", "expected_files": ["b.txt"]}}]}),
        json!({"calls": [{"name": "fs.read", "args": {"path": "a.txt"}}]}),
        json!({"calls": [{"name": "change.apply", "args": {"path": "b.txt", "op": "create", "content": "alpha\nbeta\n"}}]}),
        json!({"calls": [{"name": "shell.exec", "args": {"argv": ["git", "--version"], "inherit_env": true}}]}),
        json!({"calls": [{"name": "task.complete", "args": {"summary": "derived", "self_review": {"findings": []}}}]}),
    ]
}

struct Run {
    bodies: Vec<serde_json::Value>,
    events: Vec<serde_json::Value>,
    state: String,
    attention: String,
    failure_code: String,
    b_txt: String,
    core: CoreProcess,
    task: modbit_protocol::v1::Id,
    session: modbit_protocol::v1::Id,
    client: Client,
    _repo: tempfile::TempDir,
    _dir: tempfile::TempDir,
}

/// One task through the real Core. `models_spec` is the model catalog the
/// Core is configured with (`projection=` and `schema_bytes=` keys), `set`
/// a per-task `SetTaskToolProjection` sent before the start.
async fn run(
    script: Vec<serde_json::Value>,
    rules: Vec<(String, serde_json::Value)>,
    models_spec: Option<&str>,
    set: Option<(&str, u64)>,
    extra_env: &[(&str, &str)],
) -> Run {
    let (repo, root) = plain_repo(&[("a.txt", "alpha\n")]);
    let (base, seen) = scripted_model(script, rules).await;
    let dir = tempfile::tempdir().unwrap();
    let mut env = model_env(&base);
    if let Some(spec) = models_spec {
        env.push(("MODBIT_OPENAI_MODELS".into(), spec.into()));
    }
    for (k, v) in extra_env {
        env.push(((*k).into(), (*v).into()));
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
        "local_trusted",
        "derive b.txt",
    )
    .await;
    if let Some((mode, bytes)) = set {
        let ack = c
            .command(envelope_fenced(
                id16(0x12),
                "SetTaskToolProjection",
                SetTaskToolProjection {
                    task_id: Some(task.clone()),
                    mode: mode.into(),
                    max_projection_bytes: bytes,
                }
                .encode_to_vec(),
                g,
            ))
            .await
            .unwrap();
        let r: TaskToolProjectionSet = Client::result(&ack).unwrap();
        assert!(r.offset > 0);
    }
    let model = if models_spec.is_some() {
        "px-model"
    } else {
        "gpt-5-mini"
    };
    start_task(&mut c, &task, g, 0x13, model).await;
    let st = wait_task(&mut c, &task, 120).await;
    let events = replay(&core, &session).await;
    let bodies = seen.lock().unwrap().clone();
    let b_txt =
        std::fs::read_to_string(std::path::Path::new(&root).join("b.txt")).unwrap_or_default();
    Run {
        bodies,
        events,
        state: st.state,
        attention: st.attention_reason,
        failure_code: st.failure_code,
        b_txt,
        core,
        task,
        session,
        client: c,
        _repo: repo,
        _dir: dir,
    }
}

fn projection_events(r: &Run) -> Vec<serde_json::Value> {
    r.events
        .iter()
        .filter(|e| e["event_type"] == "ToolProjectionSelected")
        .map(|e| e["payload"]["payload"].clone())
        .collect()
}

fn tool_calls(r: &Run) -> Vec<(String, String)> {
    let mut calls: Vec<(String, String, String)> = Vec::new();
    for ev in r.events.iter().filter(|e| {
        e["aggregate_type"] == "tool_call" && e["task_id"].as_str() == Some(&hex_id(&r.task))
    }) {
        let agg = ev["aggregate_id"].as_str().unwrap().to_owned();
        let et = ev["event_type"].as_str().unwrap().to_owned();
        match calls.iter_mut().find(|(a, _, _)| *a == agg) {
            Some(entry) => entry.2 = et,
            None => calls.push((
                agg,
                ev["payload"]["payload"]["tool_name"]
                    .as_str()
                    .unwrap_or_default()
                    .to_owned(),
                et,
            )),
        }
    }
    calls.into_iter().map(|(_, n, s)| (n, s)).collect()
}

const MODEL_EXEC: &str = "px-model=0.15/0.60;projection=exec_only";

/// QUAL-PX-114: in `exec_only` the request carries only the listed schemas,
/// the program reaches `fs.read`, `change.apply` and `shell.exec` through
/// the same pipeline (each its own tool call with its outcome on the log),
/// the file is written, and the measured schema bytes are recorded on the
/// projection event and equal what the request body really carried.
#[tokio::test]
async fn qual_px_114_exec_only_shows_the_small_surface_and_programs_do_the_work_through_the_pipeline()
 {
    let r = run(exec_script(), exec_rules(), Some(MODEL_EXEC), None, &[]).await;
    assert_eq!(r.state, "ReadyForReview", "{} / {}", r.state, r.attention);
    assert_eq!(r.b_txt, "alpha\nbeta\n");
    assert!(!r.bodies.is_empty());
    for body in &r.bodies {
        assert_eq!(request_tool_names(body), EXEC_SET, "every request");
    }
    // The program's calls are ordinary governed tool calls.
    let calls = tool_calls(&r);
    for (name, state) in [
        ("fs.read", "ToolCallSucceeded"),
        ("change.apply", "ToolCallSucceeded"),
        ("shell.exec", "ToolCallSucceeded"),
    ] {
        assert!(
            calls.iter().any(|(n, s)| n == name && s == state),
            "{name}: {calls:?}"
        );
    }
    // The projection event records mode, bytes and budget, and the bytes are
    // exactly what the provider received.
    let pe = projection_events(&r);
    assert_eq!(pe.len(), r.bodies.len(), "{pe:#?}");
    for (p, body) in pe.iter().zip(&r.bodies) {
        assert_eq!(p["mode"], "exec_only");
        assert_eq!(
            p["projected_bytes"].as_u64().unwrap() as usize,
            request_schema_bytes(body),
            "recorded bytes equal the request's bytes"
        );
        assert!(p["max_projection_bytes"].as_u64().unwrap() > 0);
        assert_eq!(p["dropped"], json!([]));
    }
    // The deferred harness tools are withheld on the record, not silently
    // absent.
    let withheld = pe[0]["withheld"].as_array().unwrap();
    assert!(
        withheld
            .iter()
            .any(|w| w.as_str().unwrap().starts_with("repair.attempt:DEFERRED")),
        "{withheld:?}"
    );
    // The exec description names the tools a program reaches, as signatures.
    let first = &r.bodies[0];
    let exec_of = |body: &serde_json::Value| -> String {
        body["tools"]
            .as_array()
            .unwrap()
            .iter()
            .find(|t| t["function"]["name"] == "proc.exec")
            .unwrap()["function"]["description"]
            .as_str()
            .unwrap()
            .to_owned()
    };
    assert!(
        exec_of(first).contains("tools.fs.read({"),
        "{}",
        exec_of(first)
    );
    // Writes are bound once the plan names files (the plan gate holds in
    // this mode exactly as in direct mode).
    assert!(!exec_of(first).contains("tools.change.apply"));
    assert!(
        exec_of(&r.bodies[1]).contains("tools.change.apply({"),
        "{}",
        exec_of(&r.bodies[1])
    );
    // The system segment says how the rules' tool names are reached.
    assert!(
        first["messages"][0]["content"]
            .as_str()
            .unwrap()
            .contains("you act through `proc.exec`")
    );
}

/// The same task in `direct` and `exec_only`: schema bytes per request are
/// measured from the requests the model server received and recorded. The
/// scripted provider is a stand-in; the bytes are what the Core really sent.
#[tokio::test]
async fn qual_px_114_schema_bytes_per_request_are_measured_in_each_mode() {
    let direct = run(direct_script(), vec![], None, None, &[]).await;
    assert_eq!(direct.state, "ReadyForReview", "{}", direct.attention);
    let exec = run(exec_script(), exec_rules(), Some(MODEL_EXEC), None, &[]).await;
    assert_eq!(exec.state, "ReadyForReview", "{}", exec.attention);
    assert_eq!(direct.b_txt, exec.b_txt, "the same effect");
    let per_request =
        |r: &Run| -> Vec<usize> { r.bodies.iter().map(request_schema_bytes).collect() };
    let (d, e) = (per_request(&direct), per_request(&exec));
    let (d_first, e_first) = (d[0], e[0]);
    eprintln!(
        "PX114_MEASURE {}",
        json!({
            "provider": "scripted OpenAI-compatible server (not a live provider)",
            "direct": {"requests": d.len(), "schema_bytes_first_request": d_first, "schema_bytes_total": d.iter().sum::<usize>(), "tools_first_request": request_tool_names(&direct.bodies[0]).len(), "tool_names_first_request": request_tool_names(&direct.bodies[0])},
            "exec_only": {"requests": e.len(), "schema_bytes_first_request": e_first, "schema_bytes_total": e.iter().sum::<usize>(), "tools_first_request": request_tool_names(&exec.bodies[0]).len(), "tool_names_first_request": request_tool_names(&exec.bodies[0])},
        })
    );
    // Direct carries the whole catalog (the audit's ~28-30 schemas).
    assert!(
        request_tool_names(&direct.bodies[0]).len() >= 20,
        "{:?}",
        request_tool_names(&direct.bodies[0])
    );
    assert_eq!(request_tool_names(&exec.bodies[0]).len(), 6);
    assert!(
        e_first * 2 < d_first,
        "exec_only per-request schema bytes {e_first} vs direct {d_first}"
    );
    // The projection event of the direct run records the same measure.
    for (p, body) in projection_events(&direct).iter().zip(&direct.bodies) {
        assert_eq!(p["mode"], "direct");
        assert_eq!(
            p["projected_bytes"].as_u64().unwrap() as usize,
            request_schema_bytes(body)
        );
    }
}

/// A hidden tool is not in the request and is not callable directly: the
/// call is refused at the pipeline (`TOOL_NOT_PROJECTED`), and nothing ran.
#[tokio::test]
async fn qual_px_114_a_hidden_tool_called_directly_is_refused_and_a_program_still_reaches_it() {
    let script = vec![
        json!({"calls": [{"name": "fs.read", "args": {"path": "a.txt"}}]}),
        json!({"calls": [{"name": "agent.spawn", "args": {"idempotency_key": "k", "objective": "o", "write_scope": ["x"]}}]}),
        json!({"calls": [{"name": "plan.update", "args": {"outcome": "read a.txt", "expected_files": ["b.txt"]}}]}),
        json!({"calls": [{"name": "proc.exec", "args": {"program": "const f = await tools.fs.read({ path: 'a.txt' }); return f.content;"}}]}),
        json!({"calls": [{"name": "task.complete", "args": {"summary": "done", "self_review": {"findings": []}}}]}),
    ];
    let r = run(script, vec![], Some(MODEL_EXEC), None, &[]).await;
    assert_eq!(r.state, "ReadyForReview", "{}", r.attention);
    let results: Vec<String> = r.bodies.last().unwrap()["messages"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|m| m["role"] == "tool")
        .map(|m| m["content"].as_str().unwrap_or_default().to_owned())
        .collect();
    assert!(results[0].contains("TOOL_NOT_PROJECTED"), "{}", results[0]);
    assert!(
        results[1].contains("TOOL_NOT_PROJECTED"),
        "agent.spawn is deferred in exec_only: {}",
        results[1]
    );
    assert!(results[3].contains("alpha"), "{}", results[3]);
    // Nothing was spawned and the direct fs.read never reached an effector.
    let calls = tool_calls(&r);
    assert!(!calls.iter().any(|(n, _)| n == "agent.spawn"), "{calls:?}");
    assert_eq!(
        calls
            .iter()
            .filter(|(n, s)| n == "fs.read" && s == "ToolCallSucceeded")
            .count(),
        1,
        "only the program's read ran: {calls:?}"
    );
    assert!(
        calls
            .iter()
            .any(|(n, s)| n == "fs.read" && s == "ToolCallPolicyDecision"),
        "the direct call stopped at the policy stage: {calls:?}"
    );
}

/// Delegation: the `agent.*` family is deferred, found through `tool.search`,
/// offered for the next round only and gone after. Discovery authorises
/// nothing.
#[tokio::test]
async fn qual_px_114_delegating_adds_agent_tools_for_that_turn_and_removes_them_after() {
    let script = vec![
        json!({"calls": [{"name": "tool.search", "args": {"query": "delegate a subtask to a child agent"}}]}),
        json!({"calls": [{"name": "plan.update", "args": {"outcome": "work alone", "expected_files": ["b.txt"]}}]}),
        json!({"calls": [{"name": "task.complete", "args": {"summary": "done", "self_review": {"findings": []}}}]}),
    ];
    let r = run(script, vec![], Some(MODEL_EXEC), None, &[]).await;
    assert_eq!(r.state, "ReadyForReview", "{}", r.attention);
    assert_eq!(r.bodies.len(), 3);
    let names: Vec<Vec<String>> = r.bodies.iter().map(request_tool_names).collect();
    assert_eq!(names[0], EXEC_SET);
    assert_eq!(
        names[1].iter().filter(|n| n.starts_with("agent.")).count(),
        8,
        "offered for the round after the search: {:?}",
        names[1]
    );
    assert_eq!(names[2], EXEC_SET, "removed after: {:?}", names[2]);
    // The search result says what it did and that it grants nothing.
    let search = r.bodies[1]["messages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["role"] == "tool")
        .unwrap()["content"]
        .as_str()
        .unwrap()
        .to_owned();
    assert!(search.contains("agent.*"), "{search}");
    assert!(search.contains("does not authorize"), "{search}");
}

/// Tool search in `exec_only` returns a deferred host tool's schema on
/// demand and binds it for programs; the program's call is a governed call.
#[tokio::test]
async fn qual_px_114_tool_search_finds_a_deferred_tool_and_a_program_calls_it_through_the_pipeline()
{
    let script = vec![
        json!({"calls": [{"name": "tool.search", "args": {"query": "git status"}}]}),
        json!({"calls": [{"name": "plan.update", "args": {"outcome": "look at the repo", "expected_files": ["b.txt"]}}]}),
        json!({"calls": [{"name": "proc.exec", "args": {"program": "const s = await tools.git.status({}); return s;"}}]}),
        json!({"calls": [{"name": "task.complete", "args": {"summary": "done", "self_review": {"findings": []}}}]}),
    ];
    let r = run(script, vec![], Some(MODEL_EXEC), None, &[]).await;
    assert_eq!(r.state, "ReadyForReview", "{}", r.attention);
    let search = r.bodies[1]["messages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["role"] == "tool")
        .unwrap()["content"]
        .as_str()
        .unwrap()
        .to_owned();
    assert!(search.contains("git.status"), "{search}");
    assert!(
        search.contains("call it from a program: tools.git.status("),
        "{search}"
    );
    assert!(search.contains("schema: {"), "{search}");
    // The next request still shows only the small surface; the activated
    // tool is in the program's signatures, not a schema of its own.
    assert_eq!(request_tool_names(&r.bodies[1]), EXEC_SET);
    let exec = r.bodies[2]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["function"]["name"] == "proc.exec")
        .unwrap();
    assert!(
        exec["function"]["description"]
            .as_str()
            .unwrap()
            .contains("tools.git.status(")
    );
    let calls = tool_calls(&r);
    assert!(
        calls
            .iter()
            .any(|(n, s)| n == "git.status" && s == "ToolCallSucceeded"),
        "{calls:?}"
    );
}

/// A per-task command selects the mode over the model's catalog entry, and a
/// bad mode is refused.
#[tokio::test]
async fn qual_px_114_the_mode_is_selectable_per_task_and_per_model() {
    // Per task: the model's catalog says direct (nothing), the task says exec_only.
    let r = run(
        exec_script(),
        exec_rules(),
        None,
        Some(("exec_only", 0)),
        &[],
    )
    .await;
    assert_eq!(r.state, "ReadyForReview", "{}", r.attention);
    assert_eq!(request_tool_names(&r.bodies[0]), EXEC_SET);
    assert!(
        r.events
            .iter()
            .any(|e| e["event_type"] == "ToolProjectionConfigured")
    );
    // Per task beats per model: the catalog says exec_only, the task says direct.
    let r = run(
        direct_script(),
        vec![],
        Some(MODEL_EXEC),
        Some(("direct", 0)),
        &[],
    )
    .await;
    assert_eq!(r.state, "ReadyForReview", "{}", r.attention);
    assert!(request_tool_names(&r.bodies[0]).len() >= 20);
    // Core default through the environment, when neither says anything.
    let r = run(
        exec_script(),
        exec_rules(),
        None,
        None,
        &[("MODBIT_TOOL_PROJECTION", "exec_only")],
    )
    .await;
    assert_eq!(request_tool_names(&r.bodies[0]), EXEC_SET);
    // A mode that does not exist is refused, not guessed at.
    let mut c = r.client;
    let (_, g) = (0, Some(1u64));
    let refused = c
        .command(envelope_fenced(
            id16(0x40),
            "SetTaskToolProjection",
            SetTaskToolProjection {
                task_id: Some(r.task.clone()),
                mode: "exec-only".into(),
                max_projection_bytes: 0,
            }
            .encode_to_vec(),
            g,
        ))
        .await;
    assert!(
        matches!(&refused, Err(modbit_protocol::client::ClientError::Rejected { code, .. }) if code == "BAD_PAYLOAD"),
        "{refused:?}"
    );
}

/// The budget is enforced at projection time: a request never exceeds it,
/// the lowest-priority tools go first, the safety tools stay, and what was
/// dropped is on the projection event.
#[tokio::test]
async fn qual_px_114_the_schema_budget_drops_the_lowest_priority_tools_first_and_records_it() {
    let full = run(direct_script(), vec![], None, None, &[]).await;
    let full_bytes = request_schema_bytes(&full.bodies[0]);
    // A budget 2.5 KB under the full projection: agent.* has to go.
    let budget = (full_bytes - 2500) as u64;
    let r = run(direct_script(), vec![], None, Some(("direct", budget)), &[]).await;
    assert_eq!(r.state, "ReadyForReview", "{}", r.attention);
    for body in &r.bodies {
        assert!(request_schema_bytes(body) as u64 <= budget, "over budget");
        let names = request_tool_names(body);
        for kept in ["plan.update", "user.ask", "task.complete", "tool.search"] {
            assert!(names.iter().any(|n| n == kept), "{kept} kept: {names:?}");
        }
    }
    let pe = projection_events(&r);
    let dropped: Vec<&str> = pe[0]["dropped"]
        .as_array()
        .unwrap()
        .iter()
        .map(|d| d.as_str().unwrap())
        .collect();
    assert!(!dropped.is_empty(), "{pe:#?}");
    assert!(
        dropped[0].starts_with("agent."),
        "delegation is the first to go: {dropped:?}"
    );
    assert_eq!(pe[0]["max_projection_bytes"].as_u64().unwrap(), budget);
    assert!(pe[0]["requested_bytes"].as_u64().unwrap() > budget);
    assert!(pe[0]["projected_bytes"].as_u64().unwrap() <= budget);
    // Dropped is withheld and fenced: a crafted call to a dropped tool is
    // refused (the same task, a different script).
    let script = vec![
        json!({"calls": [{"name": "agent.steer", "args": {"message": "x"}}]}),
        json!({"calls": [{"name": "plan.update", "args": {"outcome": "o", "expected_files": ["b.txt"]}}]}),
        json!({"calls": [{"name": "task.complete", "args": {"summary": "done", "self_review": {"findings": []}}}]}),
    ];
    let r2 = run(script, vec![], None, Some(("direct", budget)), &[]).await;
    let first_result = r2.bodies[1]["messages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["role"] == "tool")
        .unwrap()["content"]
        .as_str()
        .unwrap()
        .to_owned();
    assert!(
        first_result.contains("TOOL_NOT_PROJECTED"),
        "{first_result}"
    );
}

/// A budget the tools a run cannot do without already exceed is a typed
/// over-budget result: the round fails closed, no request is sent over the
/// budget, and the task says why.
#[tokio::test]
async fn qual_px_114_a_budget_below_the_safety_tools_fails_closed_with_a_typed_code() {
    let r = run(direct_script(), vec![], None, Some(("direct", 600)), &[]).await;
    assert_eq!(r.state, "Waiting", "{}", r.attention);
    assert_eq!(r.failure_code, "PROJECTION_OVER_BUDGET", "{}", r.attention);
    assert!(r.attention.contains("600 bytes"), "{}", r.attention);
    assert!(r.bodies.is_empty(), "no request left the Core over budget");
}

/// The default projection is unchanged: no setting anywhere means `direct`
/// under the default budget, and the projection event says so.
#[tokio::test]
async fn qual_px_114_the_default_stays_direct_until_the_trial_decides() {
    let r = run(direct_script(), vec![], None, None, &[]).await;
    assert_eq!(r.state, "ReadyForReview", "{}", r.attention);
    let pe = projection_events(&r);
    assert_eq!(pe[0]["mode"], "direct");
    assert_eq!(pe[0]["dropped"], json!([]));
    assert!(pe[0]["max_projection_bytes"].as_u64().unwrap() >= 32 * 1024);
    let _ = (&r.core, &r.session);
}

/// The `typed` mode (the trial's `direct` arm) withholds the program
/// runtime, and a crafted `proc.exec` call is refused.
#[tokio::test]
async fn qual_px_114_typed_mode_offers_no_program_runtime_and_refuses_a_crafted_exec() {
    let script = vec![
        json!({"calls": [{"name": "proc.exec", "args": {"program": "return 1;"}}]}),
        json!({"calls": [{"name": "plan.update", "args": {"outcome": "o", "expected_files": ["b.txt"]}}]}),
        json!({"calls": [{"name": "task.complete", "args": {"summary": "done", "self_review": {"findings": []}}}]}),
    ];
    let r = run(script, vec![], None, Some(("typed", 0)), &[]).await;
    assert_eq!(r.state, "ReadyForReview", "{}", r.attention);
    let names = request_tool_names(&r.bodies[0]);
    assert!(!names.iter().any(|n| n.starts_with("proc.")), "{names:?}");
    assert!(names.iter().any(|n| n == "fs.read"));
    let first = r.bodies[1]["messages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["role"] == "tool")
        .unwrap()["content"]
        .as_str()
        .unwrap()
        .to_owned();
    assert!(first.contains("TOOL_NOT_PROJECTED"), "{first}");
}
