//! Real-boundary proof of REQ-PX-161 (read-only calls of one turn run
//! concurrently and are journaled in call order): the actual `modbit-core`
//! binary over its real socket, a real git repository, a real SQLite event
//! store and a scripted OpenAI-compatible server standing in for the model.

// Some imports and helpers are used only by the Unix-only tests below.
#![cfg_attr(not(unix), allow(unused_imports, dead_code))]

use std::collections::BTreeMap;
use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};
use std::time::Duration;

use modbit_protocol::client::{Client, ClientError};
use modbit_protocol::local::{ReadyLine, decode_hex};
use modbit_protocol::v1::{
    AcquireSessionLease, ClientKind, CommandEnvelope, CommandStatus, CreateSession, CreateTask,
    GetSessionSnapshot, GetTaskStatus, Id, SessionCreated, SessionLeaseAcquired, SessionSnapshot,
    StartTask, TaskCreated, TaskRunStarted, TaskStatus,
};
use prost::Message;
use serde_json::{Value, json};

const NOOP_CHECK: (&str, &str) = (
    ".modbit/verification.json",
    "{\"commands\": [{\"id\": \"fixture-noop\", \"argv\": [\"git\", \"--version\"]}]}",
);

// ---- the real Core ---------------------------------------------------------

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
                .expect("core exited before its ready line")
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

    /// A real `SIGKILL`.
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

fn rand_id() -> Id {
    Id {
        value: (0..16).map(|_| rand::random::<u8>()).collect(),
    }
}

fn envelope(id: Id, ty: &str, payload: Vec<u8>, g: Option<u64>) -> CommandEnvelope {
    CommandEnvelope {
        command_id: Some(id),
        tenant_id: Some(Id {
            value: vec![0xA1; 16],
        }),
        user_id: Some(Id {
            value: vec![0xB1; 16],
        }),
        session_id: None,
        aggregate_id: None,
        expected_generation: g,
        command_type: ty.into(),
        schema_version: 1,
        payload,
        issued_at: None,
    }
}

/// One command, answered or refused.
async fn send<M: Message, R: Message + Default>(
    c: &mut Client,
    id: Option<Id>,
    ty: &str,
    msg: M,
    g: Option<u64>,
) -> Result<(R, bool), ClientError> {
    let ack = c
        .command(envelope(
            id.unwrap_or_else(rand_id),
            ty,
            msg.encode_to_vec(),
            g,
        ))
        .await?;
    let replayed = ack.status == CommandStatus::Replayed as i32;
    Ok((Client::result::<R>(&ack).unwrap(), replayed))
}

async fn ok<M: Message, R: Message + Default>(
    c: &mut Client,
    ty: &str,
    msg: M,
    g: Option<u64>,
) -> R {
    send::<M, R>(c, None, ty, msg, g).await.unwrap().0
}

async fn lease(c: &mut Client, session: &Id, owner: &str) -> u64 {
    ok::<_, SessionLeaseAcquired>(
        c,
        "AcquireSessionLease",
        AcquireSessionLease {
            session_id: Some(session.clone()),
            owner: owner.into(),
        },
        None,
    )
    .await
    .lease_generation
}

// ---- a real repository --------------------------------------------------------

fn repo(files: &[(&str, &str)]) -> (tempfile::TempDir, String) {
    let repo = tempfile::tempdir().unwrap();
    let mut all = files.to_vec();
    all.push(NOOP_CHECK);
    for (p, c) in all {
        let path = repo.path().join(p);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, c).unwrap();
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

// ---- the scripted model --------------------------------------------------------

/// A scripted OpenAI-compatible server: the reply is the script step indexed
/// by the tool results already in the conversation; a step may carry
/// `delay_ms` (the stream is in flight for that long).
async fn model(script: Vec<Value>) -> (String, std::sync::Arc<std::sync::Mutex<Vec<Value>>>) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let seen2 = std::sync::Arc::clone(&seen);
    let script = std::sync::Arc::new(script);
    let counter = std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0));
    tokio::spawn(async move {
        loop {
            let Ok((mut sock, _)) = listener.accept().await else {
                return;
            };
            let (seen, script, counter) = (
                std::sync::Arc::clone(&seen2),
                std::sync::Arc::clone(&script),
                std::sync::Arc::clone(&counter),
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
                let body: Value =
                    serde_json::from_slice(&buf[head_end..head_end + len]).unwrap_or_default();
                let results = body["messages"]
                    .as_array()
                    .map(|m| m.iter().filter(|x| x["role"] == "tool").count())
                    .unwrap_or(0);
                seen.lock().unwrap().push(body);
                let reply = script
                    .get(results)
                    .cloned()
                    .unwrap_or_else(|| json!({"text": "I have nothing further to do."}));
                if let Some(ms) = reply["delay_ms"].as_u64() {
                    tokio::time::sleep(Duration::from_millis(ms)).await;
                }
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
                frames.push(json!({"id":"c","model":"scripted-1","choices":[{"index":0,"delta":{},"finish_reason":finish}],"usage":{"prompt_tokens":100,"completion_tokens":10,"prompt_tokens_details":{"cached_tokens":0}}}).to_string());
                frames.push("[DONE]".into());
                let request_id = format!(
                    "req_px_{results}_{}",
                    counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst)
                );
                let head = format!(
                    "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\nx-request-id: {request_id}\r\nconnection: close\r\ntransfer-encoding: chunked\r\n\r\n"
                );
                let _ = sock.write_all(head.as_bytes()).await;
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
    (format!("http://127.0.0.1:{port}"), seen)
}

fn core_env(base: &str) -> Vec<(&str, &str)> {
    vec![
        ("MODBIT_OPENAI_BASE_URL", base),
        ("OPENAI_API_KEY", ""),
        ("ANTHROPIC_API_KEY", ""),
    ]
}

// ---- a task, a session, a lease --------------------------------------------------

struct Fx {
    _data: tempfile::TempDir,
    data: std::path::PathBuf,
    _repo: tempfile::TempDir,
    root: String,
    core: CoreProcess,
    c: Client,
    session: Id,
    g: u64,
    task: Id,
    base: String,
}

impl Fx {
    /// A Core, a session with its lease, and one task on a fresh repository.
    async fn new(
        script: Vec<Value>,
        files: &[(&str, &str)],
        extra_env: &[(&str, &str)],
    ) -> (Self, std::sync::Arc<std::sync::Mutex<Vec<Value>>>) {
        let (base, seen) = model(script).await;
        let data = tempfile::tempdir().unwrap();
        let (repo, root) = repo(files);
        let mut env = core_env(&base);
        env.extend_from_slice(extra_env);
        let core = CoreProcess::spawn(data.path(), &env);
        let mut c = core.client().await;
        let created: SessionCreated = ok(
            &mut c,
            "CreateSession",
            CreateSession { space_id: None },
            None,
        )
        .await;
        let session = created.session_id.unwrap();
        let g = lease(&mut c, &session, "test").await;
        let task = ok::<_, TaskCreated>(
            &mut c,
            "CreateTask",
            CreateTask {
                session_id: Some(session.clone()),
                goal_text: "write the files".into(),
                workspace_id: None,
                execution_profile: "local_trusted".into(),
                origin: "cli".into(),
                workspace_root: root.clone(),
                issue_url: String::new(),
                issue_json: String::new(),
                ..Default::default()
            },
            Some(g),
        )
        .await
        .task_id
        .unwrap();
        let data_path = data.path().to_path_buf();
        (
            Fx {
                _data: data,
                data: data_path,
                _repo: repo,
                root,
                core,
                c,
                session,
                g,
                task,
                base,
            },
            seen,
        )
    }

    async fn start(&mut self) -> TaskRunStarted {
        ok(
            &mut self.c,
            "StartTask",
            StartTask {
                task_id: Some(self.task.clone()),
                endpoint: "openai".into(),
                model: "gpt-5".into(),
                max_turns: 0,
                max_tool_calls: 0,
                max_no_progress_turns: 20,
                skills: vec![],
                ..Default::default()
            },
            Some(self.g),
        )
        .await
    }

    async fn status(&mut self) -> TaskStatus {
        ok(
            &mut self.c,
            "GetTaskStatus",
            GetTaskStatus {
                task_id: Some(self.task.clone()),
            },
            None,
        )
        .await
    }

    /// Poll until `done` says so.
    async fn until(
        &mut self,
        what: &str,
        secs: u64,
        done: impl Fn(&TaskStatus) -> bool,
    ) -> TaskStatus {
        let deadline = std::time::Instant::now() + Duration::from_secs(secs);
        loop {
            let st = self.status().await;
            if done(&st) {
                return st;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "timed out waiting for {what}: {st:?}"
            );
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }

    /// The task's events of the given types as (type, payload, recorded ms,
    /// offset), in log order.
    async fn timed(&self, types: &[&str]) -> Vec<(String, Value, i64, u64, String)> {
        let mut s = self.core.client().await;
        let ack = s
            .command(envelope(
                rand_id(),
                "GetSessionSnapshot",
                GetSessionSnapshot {
                    session_id: Some(self.session.clone()),
                }
                .encode_to_vec(),
                None,
            ))
            .await
            .unwrap();
        let floor = Client::result::<SessionSnapshot>(&ack).unwrap().last_offset;
        s.subscribe(self.session.clone(), 0).await.unwrap();
        let mut out = Vec::new();
        let mut seen = 0u64;
        let deadline = std::time::Instant::now() + Duration::from_secs(60);
        loop {
            let wait = if seen < floor {
                deadline.saturating_duration_since(std::time::Instant::now())
            } else {
                Duration::from_millis(300)
            };
            let Ok(Ok(Some(e))) = tokio::time::timeout(wait, s.next_event()).await else {
                break;
            };
            seen = e.offset;
            let ev = e.event.unwrap();
            if ev.task_id.as_ref() == Some(&self.task) && types.contains(&ev.event_type.as_str()) {
                let p: Value = serde_json::from_slice(&ev.payload).unwrap_or_default();
                let ms = ev
                    .recorded_at
                    .as_ref()
                    .map_or(0, |t| t.seconds * 1000 + i64::from(t.nanos) / 1_000_000);
                let agg = ev
                    .aggregate_id
                    .as_ref()
                    .map(|i| {
                        i.value
                            .iter()
                            .map(|b| format!("{b:02x}"))
                            .collect::<String>()
                    })
                    .unwrap_or_default();
                out.push((ev.event_type, p["payload"].clone(), ms, e.offset, agg));
            }
        }
        out
    }

    /// A new Core on the same profile, with `extra_env`; the lease is taken
    /// again (a new generation).
    async fn restart(&mut self, extra_env: &[(&str, &str)]) {
        self.core.kill();
        let mut env = core_env(&self.base);
        env.extend_from_slice(extra_env);
        self.core = CoreProcess::spawn(&self.data, &env);
        self.c = self.core.client().await;
        self.g = lease(&mut self.c, &self.session, "test-restart").await;
    }
}

const READS: usize = 6;
const TOOL_EVENTS: [&str; 4] = [
    "ToolCallProposed",
    "ToolCallDispatched",
    "ToolCallSucceeded",
    "ToolCallFailed",
];

fn big(i: usize) -> String {
    format!("file {i}\n").repeat(300)
}

fn owned_files() -> Vec<(String, String)> {
    (0..READS)
        .map(|i| (format!("big{i}.txt"), big(i)))
        .collect()
}

/// A plan, then one turn of `READS` reads (`missing` names one that does not
/// exist) and a write, then the completion.
fn script(missing: Option<usize>) -> Vec<Value> {
    let mut calls: Vec<Value> = (0..READS)
        .map(|i| {
            if missing == Some(i) {
                json!({"name": "fs.read", "args": {"path": "absent.txt"}})
            } else {
                // `git.diff` ignores a `base` given without a `target`, so the
                // calls differ (as a model's would) while doing the same work.
                json!({"name": "git.diff", "args": {"base": format!("HEAD~{i}")}})
            }
        })
        .collect();
    calls.push(json!({"name": "change.apply", "args": {"path": "out.txt", "op": "create", "content": "x\n"}}));
    // The script is indexed by the tool results the model has seen: none, the
    // plan's (1), then the batch's (READS + 1 more).
    let complete = json!({"calls": [{"name": "task.complete", "args": {"summary": "done", "self_review": {"findings": []}}}]});
    let mut script = vec![
        json!({"calls": [{"name": "plan.update", "args": {"outcome": "read and write", "expected_files": ["out.txt"]}}]}),
        json!({"calls": calls}),
    ];
    script.extend(std::iter::repeat_n(complete, READS + 1));
    script
}

async fn fixture(missing: Option<usize>) -> (Fx, std::sync::Arc<std::sync::Mutex<Vec<Value>>>) {
    let owned = owned_files();
    let files: Vec<(&str, &str)> = owned
        .iter()
        .map(|(a, b)| (a.as_str(), b.as_str()))
        .collect();
    let (fx, seen) = Fx::new(script(missing), &files, &[]).await;
    // Uncommitted changes to every large file, so each `git.diff` has real
    // work to do (git reads and compares them all, outside the workspace
    // service that serializes the file tools).
    for (name, _) in &owned {
        use std::io::Write;
        let mut f = std::fs::OpenOptions::new()
            .append(true)
            .open(std::path::Path::new(&fx.root).join(name))
            .unwrap();
        f.write_all(b"changed after the commit\n").unwrap();
    }
    (fx, seen)
}

/// The tool results the model was shown after the batch, in the order it saw
/// them: (tool_call_id, content).
fn results_after_batch(
    seen: &std::sync::Arc<std::sync::Mutex<Vec<Value>>>,
) -> Vec<(String, String)> {
    let reqs = seen.lock().unwrap();
    let last = reqs
        .iter()
        .rev()
        .find(|b| {
            b["messages"]
                .as_array()
                .is_some_and(|m| m.iter().filter(|x| x["role"] == "tool").count() > READS)
        })
        .expect("a request after the batch");
    last["messages"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|m| m["role"] == "tool")
        .skip(1) // the plan's result
        .take(READS + 1)
        .map(|m| {
            (
                m["tool_call_id"].as_str().unwrap_or_default().to_owned(),
                m["content"].to_string(),
            )
        })
        .collect()
}

#[tokio::test]
async fn px_161_reads_overlap_the_write_waits_and_results_come_back_in_call_order() {
    let (mut fx, seen) = fixture(None).await;
    fx.start().await;
    fx.until("the batch's results reach the model", 120, |s| {
        s.state == "Waiting" || s.state == "ReadyForReview"
    })
    .await;
    let ev = fx.timed(&TOOL_EVENTS).await;
    // The plan is not a tool call; the batch is the first READS aggregates, the write the last.
    let proposed: Vec<&(String, Value, i64, u64, String)> =
        ev.iter().filter(|e| e.0 == "ToolCallProposed").collect();
    assert_eq!(proposed.len(), READS + 1, "{ev:#?}");
    let reads: Vec<&str> = proposed[..READS].iter().map(|e| e.4.as_str()).collect();
    let write = proposed[READS].4.as_str();
    let off = |kind: &str, agg: &str| ev.iter().find(|e| e.0 == kind && e.4 == agg).map(|e| e.3);
    // Together, in wall time: run one after another the batch takes the sum
    // of its calls' own times (each from proposal to result); run together it
    // takes the span from the first proposal to the last result.
    let at = |kind: &str, agg: &str| {
        ev.iter()
            .find(|e| e.0 == kind && e.4 == agg)
            .map(|e| e.2)
            .unwrap()
    };
    let own: i64 = reads
        .iter()
        .map(|a| at("ToolCallSucceeded", a) - at("ToolCallProposed", a))
        .sum();
    let span = reads
        .iter()
        .map(|a| at("ToolCallSucceeded", a))
        .max()
        .unwrap()
        - reads
            .iter()
            .map(|a| at("ToolCallProposed", a))
            .min()
            .unwrap();
    assert!(
        span * 10 < own * 8,
        "the {READS} reads were not in flight together: {span} ms from first proposal to last result against {own} ms of their own"
    );
    // The write starts only after every read finished and runs alone.
    let last_success = reads
        .iter()
        .map(|a| off("ToolCallSucceeded", a).unwrap())
        .max()
        .unwrap();
    assert!(
        off("ToolCallProposed", write).unwrap() > last_success,
        "{ev:#?}"
    );
    // What the model reads is in the order it asked.
    let got = results_after_batch(&seen);
    let ids: Vec<&str> = got.iter().map(|(i, _)| i.as_str()).collect();
    let want: Vec<String> = (0..=READS).map(|i| format!("call_1_{i}")).collect();
    assert_eq!(ids, want.iter().map(String::as_str).collect::<Vec<_>>());
    for (i, (_, text)) in got.iter().take(READS).enumerate() {
        assert!(text.contains("SUCCESS"), "call {i}: {text}");
    }
    assert!(got[READS].1.contains("out.txt"), "{:?}", got[READS]);
    assert!(std::path::Path::new(&fx.root).join("out.txt").exists());
    // The run's steps are recorded in the order the calls were issued: the
    // batch's turn has a TOOL_CALL step for each call, with rising ordinals.
    let steps = fx.timed(&["StepScheduled"]).await;
    let mut by_turn: BTreeMap<String, Vec<u64>> = BTreeMap::new();
    for e in &steps {
        if e.1["step_type"]["kind"] == "TOOL_CALL" {
            by_turn
                .entry(e.1["turn_id"].as_str().unwrap_or_default().to_owned())
                .or_default()
                .push(e.1["ordinal"].as_u64().unwrap());
        }
    }
    let batch_turn = by_turn
        .values()
        .find(|o| o.len() > READS)
        .unwrap_or_else(|| panic!("no turn has a step for each call: {by_turn:?}"));
    assert!(
        batch_turn.windows(2).all(|w| w[0] < w[1]),
        "steps out of order: {batch_turn:?}"
    );
}

#[tokio::test]
async fn px_161_a_failing_read_reports_its_error_and_the_other_results_are_present() {
    let (mut fx, seen) = fixture(Some(2)).await;
    fx.start().await;
    fx.until("the batch's results reach the model", 120, |s| {
        s.state == "Waiting" || s.state == "ReadyForReview"
    })
    .await;
    let got = results_after_batch(&seen);
    assert_eq!(got.len(), READS + 1);
    for (i, (id, text)) in got.iter().take(READS).enumerate() {
        assert_eq!(id, &format!("call_1_{i}"));
        if i == 2 {
            assert!(!text.contains("status: SUCCESS"), "{text}");
            assert!(text.contains("error"), "{text}");
        } else {
            assert!(text.contains("status: SUCCESS"), "read {i}: {text}");
        }
    }
    assert!(got[READS].1.contains("out.txt"));
}

#[cfg(unix)]
#[tokio::test]
async fn px_161_a_sigkill_during_the_batch_recovers_with_no_result_recorded_twice() {
    let (mut fx, seen) = fixture(None).await;
    fx.start().await;
    // Wait until some of the batch is in flight, then kill the Core.
    let deadline = std::time::Instant::now() + Duration::from_secs(60);
    loop {
        let ev = fx.timed(&["ToolCallDispatched"]).await;
        if ev.len() >= 3 {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "the batch never started"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    fx.restart(&[]).await;
    fx.start().await;
    fx.until("the task finishes after the restart", 120, |s| {
        s.state == "Waiting" || s.state == "ReadyForReview"
    })
    .await;
    let ev = fx.timed(&TOOL_EVENTS).await;
    let mut done: BTreeMap<String, usize> = BTreeMap::new();
    for e in ev
        .iter()
        .filter(|e| e.0 == "ToolCallSucceeded" || e.0 == "ToolCallFailed")
    {
        *done.entry(e.4.clone()).or_default() += 1;
    }
    assert!(!done.is_empty());
    assert!(
        done.values().all(|n| *n == 1),
        "a result was recorded twice: {done:#?}"
    );
    // The model, once it hears the batch, hears each call once and in order.
    let got = results_after_batch(&seen);
    let ids: Vec<&str> = got.iter().map(|(i, _)| i.as_str()).collect();
    let want: Vec<String> = (0..=READS).map(|i| format!("call_1_{i}")).collect();
    assert_eq!(ids, want.iter().map(String::as_str).collect::<Vec<_>>());
}
