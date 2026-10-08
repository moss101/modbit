//! Real-effect tests for the task mode and the execution preference
//! (PX-051, PX-053; docs/65 AFW-D03, AFW-D05, AFW-D13): the real `modbit-core`
//! process over its real socket, a real Git repository, the real SQLite log,
//! and the repository's standard model stand-in — a scripted OpenAI-compatible
//! server. A mode that is shown but not enforced, or a preference that is
//! recorded but never read, fails here.

use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use modbit_protocol::client::{Client, ClientError};
use modbit_protocol::local::{ReadyLine, decode_hex};
use modbit_protocol::v1::{
    AcquireSessionLease, CancelTask, ClientKind, CommandEnvelope, CreateSession, CreateTask,
    EffectivePolicyView, ExecutionPreference, ExecutionPreferenceSet, GetEffectivePolicy,
    GetRoutingPlan, GetSessionSnapshot, GetTaskPosture, GetTaskStatus, Id, InvokeTool,
    ObjectiveProfile, RoutingPlanView, SessionCreated, SessionLeaseAcquired, SessionSnapshot,
    SetExecutionPreference, SetTaskMode, StartTask, TaskCreated, TaskMode, TaskModeChanged,
    TaskPostureView, TaskRunStarted, TaskStatus, ToolInvoked,
};
use prost::Message;
use serde_json::json;
use tokio::sync::Notify;

// ---- the real Core process ----

struct CoreProcess {
    child: Child,
    ready: ReadyLine,
}

impl CoreProcess {
    fn spawn(data_dir: &std::path::Path, env: &[(&str, &str)]) -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_modbit-core"))
            .arg("--data-dir")
            .arg(data_dir)
            .envs(env.iter().map(|(k, v)| (k.to_string(), v.to_string())))
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap();
        let stdout = child.stdout.take().unwrap();
        let mut lines = BufReader::new(stdout).lines();
        let ready = loop {
            let line = lines
                .next()
                .expect("core exited before ready line")
                .unwrap();
            if let Some(r) = ReadyLine::parse(&line) {
                break r;
            }
        };
        std::thread::spawn(move || for _ in lines {});
        CoreProcess { child, ready }
    }

    async fn client(&self) -> Client {
        Client::connect(
            &self.ready.endpoint,
            &decode_hex(&self.ready.boot_secret_hex).unwrap(),
            ClientKind::Cli,
            "test",
        )
        .await
        .unwrap()
    }

    /// SIGKILL: no shutdown path runs.
    fn kill(&mut self) {
        self.child.kill().unwrap();
        self.child.wait().unwrap();
    }
}

impl Drop for CoreProcess {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

// ---- protocol helpers ----

fn id16(b: u8) -> Id {
    Id { value: vec![b; 16] }
}

fn envelope(
    command_id: Id,
    command_type: &str,
    payload: Vec<u8>,
    g: Option<u64>,
) -> CommandEnvelope {
    CommandEnvelope {
        command_id: Some(command_id),
        tenant_id: Some(id16(0xA1)),
        user_id: Some(id16(0xB1)),
        session_id: None,
        aggregate_id: None,
        expected_generation: g,
        command_type: command_type.into(),
        schema_version: 1,
        payload,
        issued_at: None,
    }
}

/// One command, decoded; a typed refusal comes back as its code.
async fn send<R: Message + Default>(
    c: &mut Client,
    id: u8,
    command_type: &str,
    msg: impl Message,
    g: Option<u64>,
) -> Result<R, String> {
    match c
        .command(envelope(id16(id), command_type, msg.encode_to_vec(), g))
        .await
    {
        Ok(ack) => Ok(Client::result(&ack).unwrap()),
        Err(ClientError::Rejected { code, .. }) => Err(code),
        Err(e) => panic!("{command_type}: {e}"),
    }
}

async fn create_session(c: &mut Client, id: u8) -> (Id, Option<u64>) {
    let ack = c
        .command(envelope(
            id16(id),
            "CreateSession",
            CreateSession { space_id: None }.encode_to_vec(),
            None,
        ))
        .await
        .unwrap();
    let r: SessionCreated = Client::result(&ack).unwrap();
    let session = r.session_id.unwrap();
    let g = acquire_lease(c, id ^ 0x5A, &session).await;
    (session, g)
}

async fn acquire_lease(c: &mut Client, id: u8, session: &Id) -> Option<u64> {
    let r: SessionLeaseAcquired = send(
        c,
        id,
        "AcquireSessionLease",
        AcquireSessionLease {
            session_id: Some(session.clone()),
            owner: "test".into(),
        },
        None,
    )
    .await
    .unwrap();
    Some(r.lease_generation)
}

fn pref(objective: ObjectiveProfile, effort: &str, tier: &str) -> ExecutionPreference {
    ExecutionPreference {
        objective: objective as i32,
        effort: effort.into(),
        service_tier: tier.into(),
        ..Default::default()
    }
}

#[allow(clippy::too_many_arguments)]
async fn create_task(
    c: &mut Client,
    g: Option<u64>,
    session: &Id,
    id: u8,
    root: &str,
    goal: &str,
    mode: i32,
    profile: &str,
    preference: Option<ExecutionPreference>,
) -> Result<Id, String> {
    let r: TaskCreated = send(
        c,
        id,
        "CreateTask",
        CreateTask {
            session_id: Some(session.clone()),
            goal_text: goal.into(),
            origin: "cli".into(),
            workspace_root: root.into(),
            execution_profile: profile.into(),
            mode,
            preference,
            ..Default::default()
        },
        g,
    )
    .await?;
    Ok(r.task_id.unwrap())
}

async fn set_mode(
    c: &mut Client,
    g: Option<u64>,
    id: u8,
    task: &Id,
    mode: TaskMode,
) -> Result<TaskModeChanged, String> {
    send(
        c,
        id,
        "SetTaskMode",
        SetTaskMode {
            task_id: Some(task.clone()),
            mode: mode as i32,
            reason: "test".into(),
        },
        g,
    )
    .await
}

async fn set_preference(
    c: &mut Client,
    g: Option<u64>,
    id: u8,
    task: &Id,
    preference: ExecutionPreference,
) -> Result<ExecutionPreferenceSet, String> {
    send(
        c,
        id,
        "SetExecutionPreference",
        SetExecutionPreference {
            task_id: Some(task.clone()),
            preference: Some(preference),
        },
        g,
    )
    .await
}

async fn posture(c: &mut Client, id: u8, task: &Id) -> TaskPostureView {
    send(
        c,
        id,
        "GetTaskPosture",
        GetTaskPosture {
            task_id: Some(task.clone()),
        },
        None,
    )
    .await
    .unwrap()
}

async fn status(c: &mut Client, id: u8, task: &Id) -> TaskStatus {
    send(
        c,
        id,
        "GetTaskStatus",
        GetTaskStatus {
            task_id: Some(task.clone()),
        },
        None,
    )
    .await
    .unwrap()
}

async fn routing(c: &mut Client, id: u8, task: &Id) -> RoutingPlanView {
    send(
        c,
        id,
        "GetRoutingPlan",
        GetRoutingPlan {
            task_id: Some(task.clone()),
        },
        None,
    )
    .await
    .unwrap()
}

async fn invoke(
    c: &mut Client,
    g: Option<u64>,
    id: u8,
    task: &Id,
    tool: &str,
    args: serde_json::Value,
) -> ToolInvoked {
    send(
        c,
        id,
        "InvokeTool",
        InvokeTool {
            task_id: Some(task.clone()),
            tool_name: tool.into(),
            arguments_json: args.to_string(),
            tool_call_id: Some(id16(id ^ 0xC0)),
            output_budget_bytes: 65536,
        },
        g,
    )
    .await
    .unwrap()
}

async fn start(
    c: &mut Client,
    g: Option<u64>,
    id: u8,
    task: &Id,
    model: &str,
    preference: Option<ExecutionPreference>,
) -> Result<TaskRunStarted, String> {
    send(
        c,
        id,
        "StartTask",
        StartTask {
            task_id: Some(task.clone()),
            model: model.into(),
            max_turns: 12,
            max_no_progress_turns: 4,
            preference,
            ..Default::default()
        },
        g,
    )
    .await
}

async fn cancel(c: &mut Client, g: Option<u64>, id: u8, task: &Id) {
    let _: modbit_protocol::v1::TaskCancelRequested = send(
        c,
        id,
        "CancelTask",
        CancelTask {
            task_id: Some(task.clone()),
        },
        g,
    )
    .await
    .unwrap();
}

/// A committed repository with a configured check (FIX-03: a task cannot
/// complete in a repository with none).
fn repo(files: &[(&str, &str)]) -> (tempfile::TempDir, String) {
    let repo = tempfile::tempdir().unwrap();
    let mut all = files.to_vec();
    all.push((
        ".modbit/verification.json",
        r#"{"commands":[{"id":"fixture-noop","argv":["git","--version"]}]}"#,
    ));
    for (p, content) in &all {
        let path = repo.path().join(p);
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).unwrap();
        }
        std::fs::write(path, content).unwrap();
    }
    for args in [
        vec!["init", "-q", "-b", "main"],
        vec!["config", "core.autocrlf", "false"],
        vec!["add", "-A"],
        vec![
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@e",
            "commit",
            "-q",
            "-m",
            "base",
        ],
    ] {
        assert!(
            Command::new("git")
                .arg("-C")
                .arg(repo.path())
                .args(&args)
                .status()
                .unwrap()
                .success()
        );
    }
    let root = repo
        .path()
        .canonicalize()
        .unwrap()
        .to_string_lossy()
        .trim_start_matches(r"\\?\")
        .to_owned();
    (repo, root)
}

type Events = Vec<(String, String, serde_json::Value)>;

/// Every event of one task, in log order, as (aggregate, type, payload).
async fn task_events(core: &CoreProcess, session: &Id, task: &Id) -> Events {
    let mut s = core.client().await;
    let floor = {
        let snap: SessionSnapshot = send(
            &mut s,
            0xF0,
            "GetSessionSnapshot",
            GetSessionSnapshot {
                session_id: Some(session.clone()),
            },
            None,
        )
        .await
        .unwrap();
        snap.last_offset
    };
    s.subscribe(session.clone(), 0).await.unwrap();
    let mut out = Vec::new();
    let mut seen = 0u64;
    let deadline = std::time::Instant::now() + Duration::from_secs(60);
    loop {
        let wait = if seen < floor {
            deadline.saturating_duration_since(std::time::Instant::now())
        } else {
            Duration::from_millis(400)
        };
        let Ok(Ok(Some(e))) = tokio::time::timeout(wait, s.next_event()).await else {
            break;
        };
        seen = e.offset;
        let ev = e.event.unwrap();
        if ev.task_id.as_ref() == Some(task) {
            let p: serde_json::Value = serde_json::from_slice(&ev.payload).unwrap_or_default();
            out.push((ev.aggregate_type, ev.event_type, p["payload"].clone()));
        }
    }
    out
}

fn of(evs: &Events, t: &str) -> Vec<serde_json::Value> {
    evs.iter()
        .filter(|(_, x, _)| x == t)
        .map(|(_, _, p)| p.clone())
        .collect()
}

fn position(evs: &Events, f: impl Fn(&str, &serde_json::Value) -> bool) -> usize {
    evs.iter()
        .position(|(_, t, p)| f(t, p))
        .unwrap_or_else(|| panic!("no such event in {evs:#?}"))
}

async fn wait_loop_end(c: &mut Client, id: u8, task: &Id, secs: u64) -> TaskStatus {
    let deadline = std::time::Instant::now() + Duration::from_secs(secs);
    loop {
        let st = status(c, id, task).await;
        if !st.loop_alive || std::time::Instant::now() > deadline {
            return st;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

async fn until(secs: u64, what: &str, mut f: impl FnMut() -> bool) {
    let deadline = std::time::Instant::now() + Duration::from_secs(secs);
    while !f() {
        assert!(std::time::Instant::now() < deadline, "timed out: {what}");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

// ---- the scripted model ----

/// An OpenAI-compatible server answering request `n` (counted by the tool
/// results the request carries) from `script[n]`; the request with
/// `hold_at` tool results is held open until `release` fires, so a test can
/// act on the Core while a model call is in flight.
struct Scripted {
    base: String,
    seen: Arc<Mutex<Vec<serde_json::Value>>>,
    release: Arc<Notify>,
}

async fn scripted(script: Vec<serde_json::Value>, hold_at: Option<usize>) -> Scripted {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let seen = Arc::new(Mutex::new(Vec::new()));
    let release = Arc::new(Notify::new());
    let script = Arc::new(script);
    let (seen2, release2) = (Arc::clone(&seen), Arc::clone(&release));
    tokio::spawn(async move {
        loop {
            let Ok((mut sock, _)) = listener.accept().await else {
                return;
            };
            let (seen, release, script) = (
                Arc::clone(&seen2),
                Arc::clone(&release2),
                Arc::clone(&script),
            );
            tokio::spawn(async move {
                let mut buf = Vec::new();
                let mut tmp = [0u8; 8192];
                let (head_end, len) = loop {
                    let n = sock.read(&mut tmp).await.unwrap_or(0);
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
                    let n = sock.read(&mut tmp).await.unwrap_or(0);
                    if n == 0 {
                        break;
                    }
                    buf.extend_from_slice(&tmp[..n]);
                }
                let body: serde_json::Value =
                    serde_json::from_slice(&buf[head_end..head_end + len]).unwrap_or_default();
                let results = body["messages"]
                    .as_array()
                    .map(|m| m.iter().filter(|x| x["role"] == "tool").count())
                    .unwrap_or(0);
                seen.lock().unwrap().push(body);
                if hold_at == Some(results) {
                    release.notified().await;
                }
                let reply = script
                    .get(results)
                    .cloned()
                    .unwrap_or_else(|| json!({"text": "I have nothing further to do."}));
                let mut frames: Vec<String> = Vec::new();
                if let Some(t) = reply["text"].as_str() {
                    frames.push(json!({"id":"c","model":"scripted-1","choices":[{"index":0,"delta":{"content":t},"finish_reason":null}]}).to_string());
                }
                let calls = reply["calls"].as_array().cloned().unwrap_or_default();
                for (i, c) in calls.iter().enumerate() {
                    frames.push(json!({"id":"c","model":"scripted-1","choices":[{"index":0,"delta":{"tool_calls":[{"index":i,"id":format!("call_{results}_{i}"),"type":"function","function":{"name":c["name"],"arguments":c["args"].to_string()}}]},"finish_reason":null}]}).to_string());
                }
                let finish = if calls.is_empty() {
                    "stop"
                } else {
                    "tool_calls"
                };
                frames.push(json!({"id":"c","model":"scripted-1","choices":[{"index":0,"delta":{},"finish_reason":finish}],"usage":{"prompt_tokens":100,"completion_tokens":20}}).to_string());
                frames.push("[DONE]".into());
                let _ = sock
                    .write_all(b"HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\nconnection: close\r\ntransfer-encoding: chunked\r\n\r\n")
                    .await;
                for f in frames {
                    let frame = format!("data: {f}\n\n");
                    let _ = sock
                        .write_all(format!("{:x}\r\n{}\r\n", frame.len(), frame).as_bytes())
                        .await;
                }
                let _ = sock.write_all(b"0\r\n\r\n").await;
                let _ = sock.flush().await;
                let _ = sock.shutdown().await;
            });
        }
    });
    Scripted {
        base: format!("http://127.0.0.1:{port}"),
        seen,
        release,
    }
}

fn tool_names(body: &serde_json::Value) -> Vec<String> {
    body["tools"]
        .as_array()
        .map(|t| {
            t.iter()
                .filter_map(|t| t["function"]["name"].as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default()
}

fn tool_messages(body: &serde_json::Value) -> Vec<String> {
    body["messages"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|m| m["role"] == "tool")
        .map(|m| m["content"].as_str().unwrap_or_default().to_owned())
        .collect()
}

fn env_for<'a>(base: &'a str, extra: &[(&'a str, &'a str)]) -> Vec<(&'a str, &'a str)> {
    let mut env = vec![
        ("MODBIT_OPENAI_BASE_URL", base),
        ("OPENAI_API_KEY", ""),
        ("ANTHROPIC_API_KEY", ""),
    ];
    env.extend_from_slice(extra);
    env
}

// ---- PX-051 ----

/// ASK and PLAN refuse every write and every execution at the Capability
/// Kernel, with the mode's own typed code, and the file is untouched; a read
/// still works; AGENT is the control that writes; a mode change is typed,
/// takes effect with no run in flight at once, and outlives a SIGKILL of the
/// Core. Every refusal is a typed error and changes nothing.
#[tokio::test]
async fn px_051_ask_and_plan_are_read_only_at_the_kernel_and_the_mode_outlives_a_kill() {
    let (repo, root) = repo(&[("a.txt", "a\n")]);
    let dir = tempfile::tempdir().unwrap();
    let mut core = CoreProcess::spawn(dir.path(), &[]);
    let mut c = core.client().await;
    let (session, mut g) = create_session(&mut c, 0x10).await;
    let create = |id: u8, mode: i32| (id, mode);
    let _ = create;
    let ask = create_task(
        &mut c,
        g,
        &session,
        0x11,
        &root,
        "look",
        TaskMode::Ask as i32,
        "",
        None,
    )
    .await
    .unwrap();
    let plan = create_task(
        &mut c,
        g,
        &session,
        0x12,
        &root,
        "look",
        TaskMode::Plan as i32,
        "",
        None,
    )
    .await
    .unwrap();
    let agent = create_task(&mut c, g, &session, 0x13, &root, "look", 0, "", None)
        .await
        .unwrap();

    // The mode is a typed field of the task, in every read of it.
    for (task, mode, writes) in [
        (&ask, TaskMode::Ask, false),
        (&plan, TaskMode::Plan, false),
        (&agent, TaskMode::Agent, true),
    ] {
        let p = posture(&mut c, 0x20, task).await;
        assert_eq!(p.mode, mode as i32, "{p:?}");
        assert_eq!(p.mode_in_force, mode as i32, "{p:?}");
        assert_eq!(p.posture.as_ref().unwrap().writes, writes, "{p:?}");
        let st = status(&mut c, 0x21, task).await;
        assert_eq!(st.posture.unwrap().mode, mode as i32);
    }
    let snap: SessionSnapshot = send(
        &mut c,
        0x22,
        "GetSessionSnapshot",
        GetSessionSnapshot {
            session_id: Some(session.clone()),
        },
        None,
    )
    .await
    .unwrap();
    let modes: Vec<i32> = snap.tasks.iter().map(|t| t.mode).collect();
    assert_eq!(
        modes,
        [
            TaskMode::Ask as i32,
            TaskMode::Plan as i32,
            TaskMode::Agent as i32
        ],
        "{snap:?}"
    );
    let policy: EffectivePolicyView = send(
        &mut c,
        0x23,
        "GetEffectivePolicy",
        GetEffectivePolicy {
            task_id: Some(ask.clone()),
        },
        None,
    )
    .await
    .unwrap();
    assert_eq!(policy.posture.unwrap().mode, TaskMode::Ask as i32);

    // ASK and PLAN: the kernel refuses a write with the mode's typed code,
    // however the call is made, and nothing is written.
    let write = json!({"path": "new.txt", "op": "replace", "content": "x\n"});
    for (n, task) in [(0u8, &ask), (1, &plan)] {
        let r = invoke(&mut c, g, 0x30 + n * 8, task, "change.apply", write.clone()).await;
        assert_eq!(r.status, "POLICY_DENIED", "{r:?}");
        assert_eq!(r.error_code, "MODE_POSTURE", "{r:?}");
        assert!(r.error_message.contains("mode"), "{r:?}");
        // Execution is refused too, including a command that only reads.
        let r = invoke(
            &mut c,
            g,
            0x31 + n * 8,
            task,
            "shell.exec",
            json!({"argv": ["git", "--version"]}),
        )
        .await;
        assert_eq!(r.status, "POLICY_DENIED", "{r:?}");
        assert_eq!(r.error_code, "MODE_POSTURE", "{r:?}");
        // A read is not touched by the posture.
        let r = invoke(
            &mut c,
            g,
            0x32 + n * 8,
            task,
            "fs.read",
            json!({"path": "a.txt"}),
        )
        .await;
        assert_eq!(r.status, "SUCCESS", "{r:?}");
    }
    assert!(!std::path::Path::new(&root).join("new.txt").exists());
    // The control: the same call on an AGENT task writes.
    let r = invoke(&mut c, g, 0x40, &agent, "change.apply", write.clone()).await;
    assert_eq!(r.status, "SUCCESS", "{r:?}");
    assert_eq!(
        std::fs::read_to_string(repo.path().join("new.txt")).unwrap(),
        "x\n"
    );
    std::fs::remove_file(repo.path().join("new.txt")).unwrap();

    // A mode change is typed; with no run in flight it takes effect at once.
    let changed = set_mode(&mut c, g, 0x41, &ask, TaskMode::Agent)
        .await
        .unwrap();
    assert_eq!(
        (
            changed.mode,
            changed.previous_mode,
            changed.effective.as_str()
        ),
        (TaskMode::Agent as i32, TaskMode::Ask as i32, "IMMEDIATE"),
        "{changed:?}"
    );
    let r = invoke(&mut c, g, 0x42, &ask, "change.apply", write.clone()).await;
    assert_eq!(r.status, "SUCCESS", "now AGENT: {r:?}");
    std::fs::remove_file(repo.path().join("new.txt")).unwrap();
    // …and back: a narrowing is as immediate as a widening, and it is the
    // Core's: nothing in the call said so.
    set_mode(&mut c, g, 0x43, &ask, TaskMode::Ask)
        .await
        .unwrap();
    let r = invoke(&mut c, g, 0x44, &ask, "change.apply", write.clone()).await;
    assert_eq!(r.error_code, "MODE_POSTURE", "{r:?}");
    // Leaving PLAN is accepting it: the event names the plan version (none here).
    let accepted = set_mode(&mut c, g, 0x45, &plan, TaskMode::Agent)
        .await
        .unwrap();
    assert_eq!(accepted.accepted_plan_version, 0, "{accepted:?}");
    set_mode(&mut c, g, 0x46, &plan, TaskMode::Plan)
        .await
        .unwrap();

    // Typed refusals; each changes nothing.
    assert_eq!(
        set_mode(&mut c, g, 0x50, &ask, TaskMode::Unspecified)
            .await
            .unwrap_err(),
        "MODE_REQUIRED"
    );
    let forged = send::<TaskModeChanged>(
        &mut c,
        0x51,
        "SetTaskMode",
        SetTaskMode {
            task_id: Some(ask.clone()),
            mode: 99,
            reason: String::new(),
        },
        g,
    )
    .await;
    assert_eq!(forged.unwrap_err(), "UNKNOWN_MODE");
    assert_eq!(
        set_mode(&mut c, g, 0x52, &id16(0x77), TaskMode::Ask)
            .await
            .unwrap_err(),
        "UNKNOWN_TASK"
    );
    assert_eq!(
        set_mode(&mut c, None, 0x53, &ask, TaskMode::Agent)
            .await
            .unwrap_err(),
        "LEASE_REQUIRED"
    );
    assert_eq!(
        set_mode(&mut c, Some(g.unwrap() + 5), 0x54, &ask, TaskMode::Agent)
            .await
            .unwrap_err(),
        "STALE_LEASE"
    );
    assert_eq!(
        create_task(&mut c, g, &session, 0x55, &root, "x", 77, "", None)
            .await
            .unwrap_err(),
        "UNKNOWN_MODE"
    );
    assert_eq!(
        create_task(&mut c, g, &session, 0x56, &root, "x", 0, "godmode", None)
            .await
            .unwrap_err(),
        "UNKNOWN_PROFILE"
    );
    assert_eq!(posture(&mut c, 0x57, &ask).await.mode, TaskMode::Ask as i32);
    // A replayed command id changes state once.
    let first = set_mode(&mut c, g, 0x58, &agent, TaskMode::Debug)
        .await
        .unwrap();
    let again = send::<TaskModeChanged>(
        &mut c,
        0x58,
        "SetTaskMode",
        SetTaskMode {
            task_id: Some(agent.clone()),
            mode: TaskMode::Debug as i32,
            reason: "test".into(),
        },
        g,
    )
    .await
    .unwrap();
    assert_eq!(first.offset, again.offset);
    set_mode(&mut c, g, 0x59, &agent, TaskMode::Agent)
        .await
        .unwrap();
    let evs = task_events(&core, &session, &agent).await;
    assert_eq!(
        of(&evs, "TaskModeSet")
            .iter()
            .map(|e| e["mode"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["DEBUG", "AGENT"],
        "the replayed command was one event: {evs:#?}"
    );

    // A task that has ended no longer changes mode.
    cancel(&mut c, g, 0x5A, &agent).await;
    assert_eq!(
        set_mode(&mut c, g, 0x5B, &agent, TaskMode::Ask)
            .await
            .unwrap_err(),
        "TASK_TERMINAL"
    );

    // SIGKILL the Core: the modes are on the log, and enforced after it.
    drop(c);
    core.kill();
    let core = CoreProcess::spawn(dir.path(), &[]);
    let mut c = core.client().await;
    g = acquire_lease(&mut c, 0x60, &session).await;
    assert_eq!(posture(&mut c, 0x61, &ask).await.mode, TaskMode::Ask as i32);
    assert_eq!(
        posture(&mut c, 0x62, &plan).await.mode,
        TaskMode::Plan as i32
    );
    for (n, task) in [(0u8, &ask), (1, &plan)] {
        let r = invoke(&mut c, g, 0x70 + n, task, "change.apply", write.clone()).await;
        assert_eq!(r.error_code, "MODE_POSTURE", "after the restart: {r:?}");
    }
    assert!(!std::path::Path::new(&root).join("new.txt").exists());
    // The hello says the Core serves these commands.
    let ack = c
        .command(envelope(
            id16(0x80),
            "GetTaskPosture",
            GetTaskPosture {
                task_id: Some(ask.clone()),
            }
            .encode_to_vec(),
            None,
        ))
        .await
        .unwrap();
    assert!(Client::result::<TaskPostureView>(&ack).is_ok());
}

/// A plan task: the model is offered no write or shell tool, a write it asks
/// for anyway is refused before any effect, the file is untouched and the plan
/// is on the log; once the user accepts the plan (PLAN to AGENT) the same task
/// carries on as AGENT.
#[tokio::test]
async fn px_051_a_plan_mode_run_offers_no_write_refuses_one_asked_for_and_acceptance_makes_it_an_agent()
 {
    let (repo, root) = repo(&[("a.txt", "a\n")]);
    let script = vec![
        json!({"calls": [{"name": "fs.read", "args": {"path": "a.txt"}}]}),
        json!({"calls": [{"name": "plan.update", "args": {"outcome": "b.txt mirrors a.txt", "expected_files": ["b.txt"]}}]}),
        // Asked for anyway.
        json!({"calls": [{"name": "change.apply", "args": {"path": "b.txt", "op": "replace", "content": "a\n"}}]}),
        json!({"calls": [{"name": "task.complete", "args": {"summary": "planned: b.txt mirrors a.txt", "self_review": {"findings": []}}}]}),
    ];
    let model = scripted(script, None).await;
    let dir = tempfile::tempdir().unwrap();
    let core = CoreProcess::spawn(dir.path(), &env_for(&model.base, &[]));
    let mut c = core.client().await;
    let (session, g) = create_session(&mut c, 0x10).await;
    let task = create_task(
        &mut c,
        g,
        &session,
        0x11,
        &root,
        "mirror a.txt",
        TaskMode::Plan as i32,
        "",
        None,
    )
    .await
    .unwrap();
    start(&mut c, g, 0x12, &task, "gpt-5-mini", None)
        .await
        .unwrap();
    let st = wait_loop_end(&mut c, 0x13, &task, 90).await;
    let evs = task_events(&core, &session, &task).await;
    assert_eq!(st.state, "ReadyForReview", "{st:?}\n{evs:#?}");
    // What the model was offered: reads and the harness, never a write.
    let seen = model.seen.lock().unwrap().clone();
    let offered = tool_names(&seen[1]);
    assert!(
        offered.iter().any(|t| t == "fs.read") && offered.iter().any(|t| t == "plan.update"),
        "{offered:?}"
    );
    assert!(
        !offered.iter().any(|t| t == "change.apply"
            || t == "change.batch"
            || t == "test.run"
            || t == "agent.spawn"
            || t.starts_with("terminal.")
            || t.starts_with("git.worktree")),
        "{offered:?}"
    );
    // The write it asked for was refused, and nothing changed.
    let refusal = &tool_messages(seen.last().unwrap())[2];
    assert!(
        refusal.contains("MODE_POSTURE")
            || refusal.contains("TOOL_NOT_VISIBLE")
            || refusal.contains("TOOL_NOT_PROJECTED"),
        "{refusal}"
    );
    assert!(!std::path::Path::new(&root).join("b.txt").exists());
    assert!(of(&evs, "FileChanged").is_empty(), "{evs:#?}");
    // The model's write reached the Capability Kernel and was refused there,
    // with the mode's typed code; no effector ran.
    let decisions = of(&evs, "ToolCallPolicyDecision");
    assert!(
        decisions.iter().any(|d| d["allowed"] == false
            && d["decision"]
                .as_str()
                .is_some_and(|s| s.starts_with("MODE_POSTURE"))),
        "{decisions:#?}"
    );
    assert!(
        refusal.contains("MODE_POSTURE"),
        "the model is told the kernel's code: {refusal}"
    );
    assert!(
        of(&evs, "PlanRecorded").iter().any(|p| p["version"] == 1),
        "{evs:#?}"
    );
    // The run adopted the mode at its start, on the log.
    let applied = of(&evs, "TaskPostureApplied");
    assert_eq!(applied[0]["mode"], "PLAN", "{applied:#?}");
    assert_eq!(applied[0]["boundary"], "RUN_START", "{applied:#?}");
    // Accepting the plan: PLAN to AGENT, the accepted version on the log; the
    // direct write that the posture refused is now allowed.
    let r = invoke(
        &mut c,
        g,
        0x20,
        &task,
        "change.apply",
        json!({"path": "b.txt", "op": "replace", "content": "a\n"}),
    )
    .await;
    assert_eq!(r.error_code, "MODE_POSTURE", "{r:?}");
    let accepted = set_mode(&mut c, g, 0x21, &task, TaskMode::Agent)
        .await
        .unwrap();
    assert_eq!(accepted.accepted_plan_version, 1, "{accepted:?}");
    let r = invoke(
        &mut c,
        g,
        0x22,
        &task,
        "change.apply",
        json!({"path": "b.txt", "op": "replace", "content": "a\n"}),
    )
    .await;
    assert_eq!(r.status, "SUCCESS", "{r:?}");
    assert_eq!(
        std::fs::read_to_string(repo.path().join("b.txt")).unwrap(),
        "a\n"
    );
    let evs = task_events(&core, &session, &task).await;
    let set = of(&evs, "TaskModeSet");
    assert_eq!(set.last().unwrap()["accepted_plan_version"], 1, "{set:#?}");
}

/// A mode changed while a model call is in flight does not touch that round:
/// the write it asks for is decided under the posture the round began with
/// and lands; the next round boundary adopts the new mode (on the log, after
/// the change and after that write), and from there the posture is enforced.
#[tokio::test]
async fn px_051_a_mode_changed_mid_task_takes_effect_at_the_next_round_boundary() {
    let (repo, root) = repo(&[("a.txt", "a\n")]);
    let script = vec![
        json!({"calls": [{"name": "plan.update", "args": {"outcome": "two files", "expected_files": ["one.txt", "two.txt"]}}]}),
        json!({"calls": [{"name": "change.apply", "args": {"path": "one.txt", "op": "replace", "content": "1\n"}}]}),
        json!({"calls": [{"name": "change.apply", "args": {"path": "two.txt", "op": "replace", "content": "2\n"}}]}),
        json!({"text": "nothing further"}),
    ];
    let model = scripted(script, Some(1)).await;
    let dir = tempfile::tempdir().unwrap();
    let core = CoreProcess::spawn(dir.path(), &env_for(&model.base, &[]));
    let mut c = core.client().await;
    let (session, g) = create_session(&mut c, 0x10).await;
    let task = create_task(
        &mut c,
        g,
        &session,
        0x11,
        &root,
        "write one.txt and two.txt",
        0,
        "",
        None,
    )
    .await
    .unwrap();
    start(&mut c, g, 0x12, &task, "gpt-5-mini", None)
        .await
        .unwrap();
    // The second model call is in flight (held at the provider).
    until(60, "the second model call to arrive", || {
        model.seen.lock().unwrap().len() >= 2
    })
    .await;
    let changed = set_mode(&mut c, g, 0x13, &task, TaskMode::Plan)
        .await
        .unwrap();
    assert_eq!(changed.effective, "NEXT_ROUND_BOUNDARY", "{changed:?}");
    // Declared and in force differ until the loop reaches its boundary.
    let p = posture(&mut c, 0x14, &task).await;
    assert_eq!(
        (p.mode, p.mode_in_force),
        (TaskMode::Plan as i32, TaskMode::Agent as i32),
        "{p:?}"
    );
    model.release.notify_one();
    // The round after it asks for a second write; wait for its refusal.
    until(90, "the refused write to reach the model", || {
        model.seen.lock().unwrap().len() >= 4
    })
    .await;
    let seen = model.seen.lock().unwrap().clone();
    // The round that was in flight was offered the write tool; the next was not.
    assert!(
        tool_names(&seen[1]).iter().any(|t| t == "change.apply"),
        "{:?}",
        tool_names(&seen[1])
    );
    assert!(
        !tool_names(&seen[2]).iter().any(|t| t == "change.apply"),
        "{:?}",
        tool_names(&seen[2])
    );
    // The in-flight write was not retroactively denied; the next one was.
    assert_eq!(
        std::fs::read_to_string(repo.path().join("one.txt")).unwrap(),
        "1\n"
    );
    assert!(!std::path::Path::new(&root).join("two.txt").exists());
    let refusal = &tool_messages(&seen[3])[2];
    assert!(
        refusal.contains("MODE_POSTURE")
            || refusal.contains("TOOL_NOT_VISIBLE")
            || refusal.contains("TOOL_NOT_PROJECTED"),
        "{refusal}"
    );
    cancel(&mut c, g, 0x15, &task).await;
    wait_loop_end(&mut c, 0x16, &task, 30).await;
    let evs = task_events(&core, &session, &task).await;
    let set_at = position(&evs, |t, p| t == "TaskModeSet" && p["mode"] == "PLAN");
    let wrote_at = position(&evs, |t, p| t == "FileChanged" && p["path"] == "one.txt");
    let applied_at = position(&evs, |t, p| {
        t == "TaskPostureApplied" && p["mode"] == "PLAN"
    });
    assert!(
        set_at < wrote_at && wrote_at < applied_at,
        "the change, then the write decided before the boundary, then the boundary adopting it: {set_at} {wrote_at} {applied_at}\n{evs:#?}"
    );
    // The boundary records the move it made, once.
    let applied = of(&evs, "TaskPostureApplied");
    assert_eq!(applied.len(), 1, "{applied:#?}");
    assert_eq!(
        (
            applied[0]["mode"].as_str(),
            applied[0]["previous"].as_str(),
            applied[0]["boundary"].as_str()
        ),
        (Some("PLAN"), Some("AGENT"), Some("ROUND")),
        "{applied:#?}"
    );
}

/// DEBUG makes reproduction before a fix mandatory (PX-039) whatever the goal
/// says: the same goal and the same script, in AGENT the first fix lands, in
/// DEBUG it is refused as `HARNESS_REPRODUCTION_REQUIRED` until the failure
/// is reproduced or the plan states why it cannot be.
#[tokio::test]
async fn px_051_debug_mode_makes_reproduction_before_a_fix_mandatory() {
    let script = || {
        vec![
            json!({"calls": [{"name": "plan.update", "args": {"outcome": "set the value", "expected_files": ["app.txt"]}}]}),
            json!({"calls": [{"name": "fs.read", "args": {"path": "app.txt"}}]}),
            json!({"calls": [{"name": "change.apply", "args": {"path": "app.txt", "op": "replace", "content": "value = 2\n"}}]}),
            json!({"calls": [{"name": "task.complete", "args": {"summary": "done", "self_review": {"findings": []}}}]}),
        ]
    };
    let mut outcomes = Vec::new();
    for (case, mode) in [(0x10u8, TaskMode::Agent), (0x20, TaskMode::Debug)] {
        let (repo, root) = repo(&[("app.txt", "value = 1\n")]);
        let model = scripted(script(), None).await;
        let dir = tempfile::tempdir().unwrap();
        let core = CoreProcess::spawn(dir.path(), &env_for(&model.base, &[]));
        let mut c = core.client().await;
        let (session, g) = create_session(&mut c, case).await;
        // The goal reports no failure, so only the mode can ask for a reproduction.
        let task = create_task(
            &mut c,
            g,
            &session,
            case + 1,
            &root,
            "set the value to two",
            mode as i32,
            "",
            None,
        )
        .await
        .unwrap();
        start(&mut c, g, case + 2, &task, "gpt-5-mini", None)
            .await
            .unwrap();
        wait_loop_end(&mut c, case + 3, &task, 90).await;
        let evs = task_events(&core, &session, &task).await;
        let refused = evs
            .iter()
            .filter(|(_, t, p)| {
                t == "StepFailed" && p["failure_code"] == "HARNESS_REPRODUCTION_REQUIRED"
            })
            .count();
        let on_disk = std::fs::read_to_string(repo.path().join("app.txt")).unwrap();
        outcomes.push((
            mode,
            refused,
            on_disk,
            of(&evs, "ReproductionRecorded").len(),
        ));
        drop(c);
    }
    assert_eq!(
        (outcomes[0].1, outcomes[0].2.as_str()),
        (0, "value = 2\n"),
        "AGENT: the fix lands with no reproduction: {outcomes:?}"
    );
    assert!(
        outcomes[1].1 >= 1,
        "DEBUG: the first fix is refused: {outcomes:?}"
    );
    assert_eq!(
        outcomes[1].2, "value = 1\n",
        "DEBUG: nothing was written: {outcomes:?}"
    );
    assert!(
        outcomes[1].3 >= 1,
        "DEBUG: the baseline recorded a reproduction status: {outcomes:?}"
    );
}

// ---- PX-053 ----

/// `SetExecutionPreference`: recorded on the task as an event, visible in
/// every read of it, persisted across a SIGKILL of the Core, read by the loop
/// at its boundaries — the effort and tier reach the provider's wire at the
/// next round, not the one in flight — and, with no signed registry, the
/// routing outcome is DIRECT with a typed reason: the command is never a
/// silent no-op.
#[tokio::test]
async fn px_053_a_preference_is_recorded_survives_a_kill_reaches_the_wire_and_routing_says_direct()
{
    let (_repo, root) = repo(&[("a.txt", "a\n")]);
    let script = vec![
        json!({"calls": [{"name": "plan.update", "args": {"outcome": "look", "expected_files": ["a.txt"]}}]}),
        json!({"calls": [{"name": "fs.read", "args": {"path": "a.txt"}}]}),
        json!({"calls": [{"name": "fs.read", "args": {"path": "a.txt"}}]}),
        json!({"calls": [{"name": "task.complete", "args": {"summary": "looked", "self_review": {"findings": []}}}]}),
    ];
    let model = scripted(script, Some(1)).await;
    let dir = tempfile::tempdir().unwrap();
    // The catalog entry's own effort and tier are low and flex; the user's win.
    let spec = "gpt-fx=1/2;ctx=200000;out=65536;reasoning=true;effort=low;tier=flex,gpt-plain=1/2";
    let env = env_for(&model.base, &[("MODBIT_OPENAI_MODELS", spec)]);
    let mut core = CoreProcess::spawn(dir.path(), &env);
    let mut c = core.client().await;
    let (session, mut g) = create_session(&mut c, 0x10).await;
    let task = create_task(
        &mut c,
        g,
        &session,
        0x11,
        &root,
        "look at a.txt",
        0,
        "",
        Some(pref(ObjectiveProfile::Cost, "high", "")),
    )
    .await
    .unwrap();
    let set = set_preference(
        &mut c,
        g,
        0x12,
        &task,
        pref(ObjectiveProfile::Unspecified, "", "priority"),
    )
    .await
    .unwrap();
    let view = set.preference.clone().unwrap();
    assert_eq!(
        (
            view.objective,
            view.effort.as_str(),
            view.service_tier.as_str()
        ),
        (ObjectiveProfile::Cost as i32, "high", "priority"),
        "a patch keeps what it does not name: {set:?}"
    );
    assert_eq!(set.effective, "NEXT_RUN", "{set:?}");
    // No signed registry: the outcome is DIRECT, with a typed reason.
    let routing_out = set.routing.clone().unwrap();
    assert_eq!(
        (
            routing_out.outcome.as_str(),
            routing_out.reason_code.as_str()
        ),
        ("DIRECT", "NO_ACTIVE_REGISTRY"),
        "{set:?}"
    );
    // A patch that changes nothing records nothing and says so.
    let same = set_preference(&mut c, g, 0x13, &task, pref(ObjectiveProfile::Cost, "", ""))
        .await
        .unwrap();
    assert_eq!(same.effective, "UNCHANGED", "{same:?}");
    assert_eq!(same.offset, set.offset);

    // SIGKILL the Core; the preference is on the log.
    drop(c);
    core.kill();
    let core = CoreProcess::spawn(dir.path(), &env);
    let mut c = core.client().await;
    g = acquire_lease(&mut c, 0x14, &session).await;
    let p = posture(&mut c, 0x15, &task).await;
    let v = p.preference.clone().unwrap();
    assert_eq!(
        (
            v.objective,
            v.effort.as_str(),
            v.service_tier.as_str(),
            v.offset
        ),
        (
            ObjectiveProfile::Cost as i32,
            "high",
            "priority",
            set.offset
        ),
        "{p:?}"
    );
    assert_eq!(
        status(&mut c, 0x16, &task)
            .await
            .posture
            .unwrap()
            .preference
            .unwrap()
            .effort,
        "high"
    );
    let policy: EffectivePolicyView = send(
        &mut c,
        0x17,
        "GetEffectivePolicy",
        GetEffectivePolicy {
            task_id: Some(task.clone()),
        },
        None,
    )
    .await
    .unwrap();
    assert_eq!(
        policy.posture.unwrap().preference.unwrap().service_tier,
        "priority"
    );
    // Visible in the routing view before the task has run at all.
    let rv = routing(&mut c, 0x18, &task).await;
    assert_eq!(
        rv.preference.unwrap().objective,
        ObjectiveProfile::Cost as i32
    );
    assert_eq!(
        rv.preference_routing.unwrap().reason_code,
        "NO_ACTIVE_REGISTRY"
    );

    // The run reads it: the provider's wire carries the user's effort and tier.
    start(&mut c, g, 0x19, &task, "gpt-fx", None).await.unwrap();
    until(60, "the held model call", || {
        model.seen.lock().unwrap().len() >= 2
    })
    .await;
    {
        let seen = model.seen.lock().unwrap();
        for body in seen.iter() {
            assert_eq!(body["reasoning_effort"], "high", "{body:.300}");
            assert_eq!(body["service_tier"], "priority", "{body:.300}");
        }
    }
    // Changed while that call is in flight: the next round carries it.
    let mid = set_preference(
        &mut c,
        g,
        0x1A,
        &task,
        pref(ObjectiveProfile::Unspecified, "medium", ""),
    )
    .await
    .unwrap();
    assert_eq!(mid.effective, "NEXT_ROUND_BOUNDARY", "{mid:?}");
    model.release.notify_one();
    let st = wait_loop_end(&mut c, 0x1B, &task, 90).await;
    let evs = task_events(&core, &session, &task).await;
    assert_eq!(st.state, "ReadyForReview", "{st:?}\n{evs:#?}");
    let seen = model.seen.lock().unwrap().clone();
    assert_eq!(
        seen[1]["reasoning_effort"], "high",
        "the call in flight kept its effort"
    );
    assert_eq!(
        seen[2]["reasoning_effort"], "medium",
        "the next round carries the new one"
    );
    assert_eq!(seen[2]["service_tier"], "priority");
    // The log: what was set, and what routing read, at which boundary.
    let sets = of(&evs, "ExecutionPreferenceSet");
    assert_eq!(
        sets.iter()
            .map(|s| s["source"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["create", "user", "user"],
        "{sets:#?}"
    );
    let applied = of(&evs, "ExecutionPreferenceApplied");
    assert_eq!(
        applied
            .iter()
            .map(|a| a["boundary"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["RUN_START", "ROUND"],
        "{applied:#?}"
    );
    for a in &applied {
        assert_eq!(
            (a["outcome"].as_str(), a["reason_code"].as_str()),
            (Some("DIRECT"), Some("NO_ACTIVE_REGISTRY")),
            "{a}"
        );
    }
    assert_eq!(applied[1]["effort_applied"], "medium", "{applied:#?}");
    assert_eq!(applied[1]["preference"]["objective"], "COST");
    // Every dispatch names the preference event it ran under.
    let routes: Vec<&serde_json::Value> = evs
        .iter()
        .filter(|(_, t, _)| t == "ModelInvocationStarted")
        .map(|(_, _, p)| &p["model_route"])
        .collect();
    assert!(routes.len() >= 3);
    for r in &routes {
        assert!(
            r["preference_offset"].as_u64().unwrap() >= set.offset,
            "{r}"
        );
    }
    assert_eq!(routes[0]["reasoning_effort"], "high");
    assert_eq!(routes[2]["reasoning_effort"], "medium");
    // The routing view says what routing did with it: DIRECT, and why.
    let rv = routing(&mut c, 0x1C, &task).await;
    assert_eq!(rv.path_label, "DIRECT", "{rv:?}");
    let ro = rv.preference_routing.unwrap();
    assert_eq!(
        (ro.outcome.as_str(), ro.reason_code.as_str()),
        ("DIRECT", "NO_ACTIVE_REGISTRY")
    );
    assert_eq!(rv.preference.unwrap().effort_applied, "medium");
}

/// An effort for a model that exposes no reasoning is not sent (the provider
/// would refuse it); the boundary says so rather than dropping it silently.
#[tokio::test]
async fn px_053_an_effort_the_model_cannot_take_is_not_sent_and_the_record_says_why() {
    let (_repo, root) = repo(&[("a.txt", "a\n")]);
    let script = vec![
        json!({"calls": [{"name": "plan.update", "args": {"outcome": "look", "expected_files": ["a.txt"]}}]}),
        json!({"calls": [{"name": "task.complete", "args": {"summary": "looked", "self_review": {"findings": []}}}]}),
    ];
    let model = scripted(script, None).await;
    let dir = tempfile::tempdir().unwrap();
    let spec = "gpt-plain=1/2;ctx=200000;out=65536";
    let core = CoreProcess::spawn(
        dir.path(),
        &env_for(&model.base, &[("MODBIT_OPENAI_MODELS", spec)]),
    );
    let mut c = core.client().await;
    let (session, g) = create_session(&mut c, 0x10).await;
    let task = create_task(&mut c, g, &session, 0x11, &root, "look", 0, "", None)
        .await
        .unwrap();
    start(
        &mut c,
        g,
        0x12,
        &task,
        "gpt-plain",
        Some(pref(ObjectiveProfile::Balance, "high", "flex")),
    )
    .await
    .unwrap();
    let st = wait_loop_end(&mut c, 0x13, &task, 90).await;
    let evs = task_events(&core, &session, &task).await;
    assert_eq!(st.state, "ReadyForReview", "{st:?}\n{evs:#?}");
    let seen = model.seen.lock().unwrap().clone();
    for body in &seen {
        assert!(body.get("reasoning_effort").is_none(), "{body:.300}");
        assert_eq!(
            body["service_tier"], "flex",
            "the tier is not a reasoning option: {body:.300}"
        );
    }
    let applied = of(&evs, "ExecutionPreferenceApplied");
    assert_eq!(applied.len(), 1, "{applied:#?}");
    assert!(applied[0]["effort_applied"].is_null(), "{applied:#?}");
    assert!(
        applied[0]["detail"]
            .as_str()
            .unwrap()
            .contains("EFFORT_NOT_SUPPORTED"),
        "{applied:#?}"
    );
    assert_eq!(
        of(&evs, "ExecutionPreferenceSet")[0]["source"],
        "start_task"
    );
}

/// Invalid values and pins the policy forbids are refused with typed errors
/// and change no state; an allowed pin is recorded and is the model of a start
/// that names none, and a pin the organization policy forbids never reaches
/// the compiler.
#[tokio::test]
async fn px_053_invalid_preferences_and_forbidden_pins_are_refused_with_typed_errors_and_change_nothing()
 {
    let (_repo, root) = repo(&[("a.txt", "a\n")]);
    let script = vec![
        json!({"calls": [{"name": "plan.update", "args": {"outcome": "look", "expected_files": ["a.txt"]}}]}),
        json!({"calls": [{"name": "task.complete", "args": {"summary": "looked", "self_review": {"findings": []}}}]}),
    ];
    let model = scripted(script, None).await;
    let dir = tempfile::tempdir().unwrap();
    // The organization allows two of the three models.
    std::fs::write(
        dir.path().join("admin-config.json"),
        r#"{"models_allow": ["gpt-a", "gpt-b"]}"#,
    )
    .unwrap();
    let spec = "gpt-a=1/2,gpt-b=1/2,gpt-forbidden=1/2";
    let core = CoreProcess::spawn(
        dir.path(),
        &env_for(&model.base, &[("MODBIT_OPENAI_MODELS", spec)]),
    );
    let mut c = core.client().await;
    let (session, g) = create_session(&mut c, 0x10).await;
    let task = create_task(&mut c, g, &session, 0x11, &root, "look", 0, "", None)
        .await
        .unwrap();
    let pin = |e: &str, m: &str| ExecutionPreference {
        pin_endpoint: e.into(),
        pin_model: m.into(),
        ..Default::default()
    };
    let refusals: Vec<(&str, ExecutionPreference)> = vec![
        ("EMPTY_PREFERENCE", ExecutionPreference::default()),
        (
            "INVALID_EFFORT",
            pref(ObjectiveProfile::Unspecified, "extreme", ""),
        ),
        (
            "INVALID_TIER",
            pref(ObjectiveProfile::Unspecified, "", "not a tier!"),
        ),
        (
            "UNKNOWN_OBJECTIVE",
            ExecutionPreference {
                objective: 42,
                ..Default::default()
            },
        ),
        ("PIN_INCOMPLETE", pin("openai", "")),
        (
            "PIN_CONFLICT",
            ExecutionPreference {
                pin_endpoint: "openai".into(),
                pin_model: "gpt-a".into(),
                clear_pin: true,
                ..Default::default()
            },
        ),
        ("NO_PROVIDER", pin("nowhere", "gpt-a")),
        ("UNKNOWN_MODEL", pin("openai", "gpt-nope")),
        ("MODEL_NOT_ALLOWED", pin("openai", "gpt-forbidden")),
    ];
    for (n, (code, p)) in refusals.into_iter().enumerate() {
        let r = set_preference(&mut c, g, 0x20 + n as u8, &task, p).await;
        assert_eq!(r.unwrap_err(), code);
    }
    assert_eq!(
        set_preference(
            &mut c,
            None,
            0x30,
            &task,
            pref(ObjectiveProfile::Cost, "", "")
        )
        .await
        .unwrap_err(),
        "LEASE_REQUIRED"
    );
    assert_eq!(
        set_preference(
            &mut c,
            Some(99),
            0x31,
            &task,
            pref(ObjectiveProfile::Cost, "", "")
        )
        .await
        .unwrap_err(),
        "STALE_LEASE"
    );
    assert_eq!(
        set_preference(
            &mut c,
            g,
            0x32,
            &id16(0x66),
            pref(ObjectiveProfile::Cost, "", "")
        )
        .await
        .unwrap_err(),
        "UNKNOWN_TASK"
    );
    // StartTask refuses an invalid value the same way, and starts nothing.
    assert_eq!(
        start(
            &mut c,
            g,
            0x33,
            &task,
            "gpt-a",
            Some(pref(ObjectiveProfile::Unspecified, "extreme", ""))
        )
        .await
        .unwrap_err(),
        "INVALID_EFFORT"
    );
    assert_eq!(
        create_task(
            &mut c,
            g,
            &session,
            0x34,
            &root,
            "x",
            0,
            "",
            Some(pref(ObjectiveProfile::Unspecified, "", "no good"))
        )
        .await
        .unwrap_err(),
        "INVALID_TIER"
    );
    // None of it changed anything.
    let p = posture(&mut c, 0x35, &task).await;
    assert_eq!(p.preference.as_ref().unwrap().offset, 0, "{p:?}");
    assert_eq!(
        p.preference.unwrap().objective,
        ObjectiveProfile::Unspecified as i32
    );
    let evs = task_events(&core, &session, &task).await;
    assert!(of(&evs, "ExecutionPreferenceSet").is_empty(), "{evs:#?}");
    assert!(
        model.seen.lock().unwrap().is_empty(),
        "nothing reached a provider"
    );

    // An allowed pin is recorded, applies from the next run, and is the model
    // of a start that names none (a pin like any other: never switched away from).
    let ok = set_preference(&mut c, g, 0x40, &task, pin("openai", "gpt-b"))
        .await
        .unwrap();
    assert_eq!(ok.effective, "NEXT_RUN", "{ok:?}");
    let v = ok.preference.unwrap();
    assert_eq!(
        (v.pin_endpoint.as_str(), v.pin_model.as_str()),
        ("openai", "gpt-b")
    );
    let started = start(&mut c, g, 0x41, &task, "", None).await.unwrap();
    assert_eq!(
        (started.endpoint.as_str(), started.model.as_str()),
        ("openai", "gpt-b")
    );
    let st = wait_loop_end(&mut c, 0x42, &task, 90).await;
    assert_eq!(st.state, "ReadyForReview", "{st:?}");
    let seen = model.seen.lock().unwrap().clone();
    assert!(
        seen.iter().all(|b| b["model"] == "gpt-b"),
        "the pin dispatched: {seen:?}"
    );
    // A pin can be dropped.
    let task2 = create_task(&mut c, g, &session, 0x43, &root, "look", 0, "", None)
        .await
        .unwrap();
    set_preference(&mut c, g, 0x44, &task2, pin("openai", "gpt-b"))
        .await
        .unwrap();
    let cleared = set_preference(
        &mut c,
        g,
        0x46,
        &task2,
        ExecutionPreference {
            clear_pin: true,
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert!(cleared.preference.unwrap().pin_model.is_empty());
}

// ---- PX-053 with a signed registry: the router reads the objective ----

fn signed_registry(key: &ed25519_dalek::SigningKey, floors: &[(&str, f64)]) -> String {
    use ed25519_dalek::Signer;
    use modbit_providers::registry::{
        Economics, Governance, Latency, QualityFloor, REGISTRY_SCHEMA_VERSION, RegistryDocument,
        RegistryEntry, SignedRegistry,
    };
    let now = modbit_domain::Timestamp::now().0;
    let entry = |model: &str, input_price: u64, output_price: u64| RegistryEntry {
        endpoint: "openai".into(),
        provider: "openai".into(),
        family: "gpt-5".into(),
        model: model.into(),
        roles: vec!["solver".into(), "reviewer".into()],
        input_modalities: vec!["text".into()],
        context_tokens: 400_000,
        max_output_tokens: 64_000,
        tools: true,
        vision: false,
        reasoning: true,
        structured_output: true,
        economics: Economics {
            input_per_mtok_minor: input_price,
            output_per_mtok_minor: output_price,
            currency: "USD".into(),
            scale: 2,
            cached_input_per_mtok_minor: None,
            cache_write_per_mtok_minor: None,
            cache_ttl_ms: None,
        },
        latency: Latency {
            p50_ms: 900,
            p95_ms: 4_200,
        },
        governance: Governance {
            data_residency: "us".into(),
            retains_prompts: false,
            allowed_profiles: vec![],
        },
        revoked: false,
        fallbacks: vec![],
    };
    let doc = RegistryDocument {
        promotion: None,
        schema_version: REGISTRY_SCHEMA_VERSION,
        registry_generation: "registry-px053".into(),
        stats_version: "stats-1".into(),
        issued_at_ms: now - 60_000,
        expires_at_ms: now + 86_400_000,
        quality_floors: floors
            .iter()
            .map(|(mode, q)| QualityFloor {
                mode: (*mode).into(),
                min_quality: *q,
                max_cost_minor: 5_000,
                currency: "USD".into(),
                scale: 2,
            })
            .collect(),
        entries: vec![entry("gpt-5-mini", 25, 200), entry("gpt-5", 125, 1_000)],
    };
    let json = serde_json::to_string(&doc).unwrap();
    serde_json::to_string(&SignedRegistry {
        key_id: "ops".into(),
        signature_hex: hex::encode(key.sign(json.as_bytes()).to_bytes()),
        document_json: json,
    })
    .unwrap()
}

/// With a signed registry active the router reads the objective: it compiles
/// under the floor row the registry already defines for it (`economy` for
/// COST, `quality` for INTELLIGENCE, `auto` for none and for BALANCE) and the
/// outcome is ROUTED with the row named; a registry without the row keeps the
/// `auto` floor and says FLOOR_UNDEFINED. No preference leaves no trace.
#[tokio::test]
async fn px_053_with_a_signed_registry_the_router_reads_the_objective_and_says_which_floor() {
    let key = ed25519_dalek::SigningKey::from_bytes(&[53u8; 32]);
    let key_hex = hex::encode(key.verifying_key().to_bytes());
    let (_repo, root) = repo(&[("a.txt", "a\n")]);
    let script = || {
        vec![
            json!({"calls": [{"name": "plan.update", "args": {"outcome": "look", "expected_files": ["a.txt"]}}]}),
            json!({"calls": [{"name": "fs.read", "args": {"path": "a.txt"}}]}),
            json!({"calls": [{"name": "task.complete", "args": {"summary": "looked", "self_review": {"findings": []}}}]}),
        ]
    };
    for (floors, case) in [
        (
            vec![("auto", 0.5), ("economy", 0.0), ("quality", 0.99)],
            0x10u8,
        ),
        (vec![("auto", 0.5), ("quality", 0.99)], 0x60u8),
    ] {
        let model = scripted(script(), None).await;
        let dir = tempfile::tempdir().unwrap();
        let keys = format!("ops:{key_hex}");
        let core = CoreProcess::spawn(
            dir.path(),
            &env_for(&model.base, &[("MODBIT_REGISTRY_KEYS", &keys)]),
        );
        let mut c = core.client().await;
        let ack = c
            .command(envelope(
                id16(case),
                "ActivateModelRegistry",
                modbit_protocol::v1::ActivateModelRegistry {
                    signed_json: signed_registry(&key, &floors),
                    expected_generation: String::new(),
                }
                .encode_to_vec(),
                None,
            ))
            .await
            .unwrap();
        let r: modbit_protocol::v1::ModelRegistryView = Client::result(&ack).unwrap();
        assert!(r.active, "{r:?}");
        let (session, g) = create_session(&mut c, case + 1).await;
        let mut seen_floors = Vec::new();
        for (n, objective) in [
            ObjectiveProfile::Unspecified,
            ObjectiveProfile::Cost,
            ObjectiveProfile::Intelligence,
        ]
        .into_iter()
        .enumerate()
        {
            let id = case + 2 + (n as u8) * 8;
            let preference =
                (objective != ObjectiveProfile::Unspecified).then(|| pref(objective, "", ""));
            let task = create_task(&mut c, g, &session, id, &root, "look", 0, "", preference)
                .await
                .unwrap();
            start(&mut c, g, id + 1, &task, "", None).await.unwrap();
            let st = wait_loop_end(&mut c, id + 2, &task, 90).await;
            let evs = task_events(&core, &session, &task).await;
            assert_eq!(
                st.state, "ReadyForReview",
                "{objective:?}: {st:?}\n{evs:#?}"
            );
            let applied = of(&evs, "ExecutionPreferenceApplied");
            if objective == ObjectiveProfile::Unspecified {
                // No preference, no trace: the default path is untouched.
                assert!(applied.is_empty(), "{applied:#?}");
                assert!(of(&evs, "TaskPostureApplied").is_empty());
                seen_floors.push("auto".to_owned());
                continue;
            }
            assert_eq!(applied.len(), 1, "{applied:#?}");
            assert_eq!(applied[0]["outcome"], "ROUTED", "{applied:#?}");
            seen_floors.push(applied[0]["floor_mode"].as_str().unwrap().to_owned());
            let wanted = if objective == ObjectiveProfile::Cost {
                "economy"
            } else {
                "quality"
            };
            let has_row = floors.iter().any(|(m, _)| *m == wanted);
            assert_eq!(
                applied[0]["reason_code"],
                if has_row {
                    "FLOOR_APPLIED"
                } else {
                    "FLOOR_UNDEFINED"
                },
                "{applied:#?}"
            );
            let rv = routing(&mut c, id + 3, &task).await;
            assert_eq!(rv.path_label, "DIRECT", "one binding ran: {rv:?}");
            assert_eq!(rv.preference_routing.unwrap().outcome, "ROUTED");
        }
        if floors.len() == 3 {
            assert_eq!(seen_floors, ["auto", "economy", "quality"]);
        } else {
            // No `economy` row in this registry: COST keeps the `auto` floor.
            assert_eq!(seen_floors, ["auto", "auto", "quality"]);
        }
    }
}
