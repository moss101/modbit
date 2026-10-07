//! Shared harness for the PX-114 / PX-115 / PX-117 real-boundary tests: the
//! actual `modbit-core` binary over its real socket, a scripted
//! OpenAI-compatible model server that records every request body it
//! receives, a real git repository fixture and the few commands the tests
//! drive. The model is a stand-in; everything the Core does with it is real.
#![allow(dead_code)]

pub mod mcp_support;

use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};
use std::time::Duration;

use modbit_protocol::client::Client;
use modbit_protocol::local::{ReadyLine, decode_hex};
use modbit_protocol::v1::{
    AcquireSessionLease, ClientKind, CommandEnvelope, CreateSession, CreateTask, GetTaskStatus, Id,
    SessionCreated, SessionLeaseAcquired, StartTask, TaskCreated, TaskRunStarted, TaskStatus,
};
use prost::Message;

pub struct CoreProcess {
    child: Child,
    ready: ReadyLine,
}

impl CoreProcess {
    pub fn spawn_with_env(data_dir: &std::path::Path, env: &[(&str, &str)]) -> Self {
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

    pub fn pid(&self) -> u32 {
        self.child.id()
    }

    pub async fn client(&self) -> Client {
        Client::connect(
            &self.ready.endpoint,
            &decode_hex(&self.ready.boot_secret_hex).unwrap(),
            ClientKind::Cli,
            "test",
        )
        .await
        .unwrap()
    }

    pub fn kill(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Drop for CoreProcess {
    fn drop(&mut self) {
        self.kill();
    }
}

pub fn id16(b: u8) -> Id {
    Id { value: vec![b; 16] }
}

pub fn rand_id() -> Id {
    Id {
        value: (0..16).map(|_| rand::random::<u8>()).collect(),
    }
}

pub fn envelope(command_id: Id, command_type: &str, payload: Vec<u8>) -> CommandEnvelope {
    CommandEnvelope {
        command_id: Some(command_id),
        tenant_id: Some(id16(0xA1)),
        user_id: Some(id16(0xB1)),
        session_id: None,
        aggregate_id: None,
        expected_generation: None,
        command_type: command_type.into(),
        schema_version: 1,
        payload,
        issued_at: None,
    }
}

pub fn envelope_fenced(
    command_id: Id,
    command_type: &str,
    payload: Vec<u8>,
    generation: Option<u64>,
) -> CommandEnvelope {
    let mut e = envelope(command_id, command_type, payload);
    e.expected_generation = generation;
    e
}

/// A session with its lease acquired; returns the session id and the lease
/// generation commands must present.
pub async fn session_with_lease(c: &mut Client, n: u8) -> (Id, Option<u64>) {
    let ack = c
        .command(envelope(
            id16(n),
            "CreateSession",
            CreateSession { space_id: None }.encode_to_vec(),
        ))
        .await
        .unwrap();
    let r: SessionCreated = Client::result(&ack).unwrap();
    let session = r.session_id.unwrap();
    let ack = c
        .command(envelope(
            id16(n ^ 0x5A),
            "AcquireSessionLease",
            AcquireSessionLease {
                session_id: Some(session.clone()),
                owner: "test".into(),
            }
            .encode_to_vec(),
        ))
        .await
        .unwrap();
    let l: SessionLeaseAcquired = Client::result(&ack).unwrap();
    (session, Some(l.lease_generation))
}

pub async fn create_task(
    c: &mut Client,
    session: &Id,
    g: Option<u64>,
    root: &str,
    id: u8,
    profile: &str,
    goal: &str,
) -> Id {
    let ack = c
        .command(envelope_fenced(
            id16(id),
            "CreateTask",
            CreateTask {
                session_id: Some(session.clone()),
                goal_text: goal.into(),
                workspace_id: None,
                execution_profile: profile.into(),
                origin: "cli".into(),
                workspace_root: root.into(),
                issue_url: String::new(),
                issue_json: String::new(),
            }
            .encode_to_vec(),
            g,
        ))
        .await
        .unwrap();
    Client::result::<TaskCreated>(&ack)
        .unwrap()
        .task_id
        .unwrap()
}

pub async fn start_task(c: &mut Client, task: &Id, g: Option<u64>, id: u8, model: &str) {
    let ack = c
        .command(envelope_fenced(
            id16(id),
            "StartTask",
            StartTask {
                task_id: Some(task.clone()),
                endpoint: String::new(),
                model: model.into(),
                max_turns: 14,
                max_tool_calls: 0,
                max_no_progress_turns: 6,
                skills: vec![],
            }
            .encode_to_vec(),
            g,
        ))
        .await
        .unwrap();
    let _: TaskRunStarted = Client::result(&ack).unwrap();
}

pub async fn wait_task(c: &mut Client, task: &Id, secs: u64) -> TaskStatus {
    let deadline = std::time::Instant::now() + Duration::from_secs(secs);
    loop {
        let ack = c
            .command(envelope(
                rand_id(),
                "GetTaskStatus",
                GetTaskStatus {
                    task_id: Some(task.clone()),
                }
                .encode_to_vec(),
            ))
            .await
            .unwrap();
        let st: TaskStatus = Client::result(&ack).unwrap();
        if !st.loop_alive || std::time::Instant::now() > deadline {
            return st;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

/// Every event of a session as `{event_type, task_id, payload, ...}` JSON.
pub async fn replay(core: &CoreProcess, session: &Id) -> Vec<serde_json::Value> {
    let mut s = core.client().await;
    s.subscribe(session.clone(), 0).await.unwrap();
    let mut out = Vec::new();
    while let Ok(Ok(Some(e))) =
        tokio::time::timeout(Duration::from_millis(600), s.next_event()).await
    {
        let ev = e.event.unwrap();
        out.push(serde_json::json!({
            "offset": e.offset,
            "aggregate_type": ev.aggregate_type,
            "aggregate_id": hex::encode(&ev.aggregate_id.unwrap().value),
            "event_type": ev.event_type,
            "task_id": ev.task_id.map(|t| hex::encode(&t.value)),
            "payload": serde_json::from_slice::<serde_json::Value>(&ev.payload).unwrap_or_default(),
        }));
    }
    out
}

pub fn hex_id(id: &Id) -> String {
    hex::encode(&id.value)
}

pub const NOOP_CHECK: (&str, &str) = (
    ".modbit/verification.json",
    "{\"commands\": [{\"id\": \"fixture-noop\", \"argv\": [\"git\", \"--version\"]}]}",
);

/// A committed repository of `files` with the no-op mandatory check (FIX-03).
pub fn plain_repo(files: &[(&str, &str)]) -> (tempfile::TempDir, String) {
    let mut with_check = files.to_vec();
    if !files.iter().any(|(p, _)| *p == NOOP_CHECK.0) {
        with_check.push(NOOP_CHECK);
    }
    let repo = tempfile::tempdir().unwrap();
    for (p, c) in &with_check {
        let path = repo.path().join(p);
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).unwrap();
        }
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

pub type Seen = std::sync::Arc<std::sync::Mutex<Vec<serde_json::Value>>>;

static SCRIPTED_REQUESTS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

const RESPONSE_HEAD: &str = "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\nx-request-id: {id}\r\nconnection: close\r\ntransfer-encoding: chunked\r\n\r\n";

/// A scripted OpenAI-compatible server. Reply `n` answers the request that
/// carries `n` tool results (the repository's standard model stand-in);
/// `rules` answer by a needle in the last tool result instead. Every request
/// body is recorded. This is not a live provider.
pub async fn scripted_model(
    script: Vec<serde_json::Value>,
    rules: Vec<(String, serde_json::Value)>,
) -> (String, Seen) {
    scripted_model_fn(std::sync::Arc::new(move |body, results| {
        let last_tool_text = body["messages"]
            .as_array()
            .and_then(|m| m.iter().rev().find(|x| x["role"] == "tool"))
            .and_then(|m| m["content"].as_str())
            .unwrap_or_default()
            .to_owned();
        rules
            .iter()
            .find(|(needle, _)| last_tool_text.contains(needle.as_str()))
            .map(|(_, r)| r.clone())
            .unwrap_or_else(|| {
                script
                    .get(results)
                    .cloned()
                    .unwrap_or_else(|| serde_json::json!({"text": "I have nothing further to do."}))
            })
    }))
    .await
}

/// The reply function of a scripted server: the request body and the number
/// of tool results it carries.
pub type Reply =
    std::sync::Arc<dyn Fn(&serde_json::Value, usize) -> serde_json::Value + Send + Sync>;

/// A scripted server whose reply is computed from the request.
pub async fn scripted_model_fn(reply_fn: Reply) -> (String, Seen) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let seen: Seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let seen2 = std::sync::Arc::clone(&seen);
    tokio::spawn(async move {
        loop {
            let Ok((mut sock, _)) = listener.accept().await else {
                return;
            };
            let seen = std::sync::Arc::clone(&seen2);
            let reply_fn = std::sync::Arc::clone(&reply_fn);
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
                let prompt_tokens = serde_json::to_string(&body["messages"])
                    .map(|m| m.len().div_ceil(4))
                    .unwrap_or(0);
                let reply = reply_fn(&body, results);
                seen.lock().unwrap().push(body);
                // A reply may be slow: `{"delay_ms": N, ...}`.
                if let Some(ms) = reply["delay_ms"].as_u64() {
                    tokio::time::sleep(Duration::from_millis(ms)).await;
                }
                let mut frames: Vec<String> = Vec::new();
                if let Some(t) = reply["text"].as_str() {
                    frames.push(serde_json::json!({"id":"c","model":"scripted-1","choices":[{"index":0,"delta":{"content":t},"finish_reason":null}]}).to_string());
                }
                let calls = reply["calls"].as_array().cloned().unwrap_or_default();
                for (i, c) in calls.iter().enumerate() {
                    frames.push(serde_json::json!({"id":"c","model":"scripted-1","choices":[{"index":0,"delta":{"tool_calls":[{"index":i,"id":format!("call_{results}_{i}"),"type":"function","function":{"name":c["name"],"arguments":c["args"].to_string()}}]},"finish_reason":null}]}).to_string());
                }
                let finish = if calls.is_empty() {
                    "stop"
                } else {
                    "tool_calls"
                };
                let completion_tokens = reply.to_string().len().div_ceil(4);
                let request_id = format!(
                    "req_px_{results}_{}",
                    SCRIPTED_REQUESTS.fetch_add(1, std::sync::atomic::Ordering::SeqCst)
                );
                frames.push(serde_json::json!({"id":"c","model":"scripted-1","choices":[{"index":0,"delta":{},"finish_reason":finish}],"usage":{"prompt_tokens":prompt_tokens,"completion_tokens":completion_tokens,"prompt_tokens_details":{"cached_tokens":0}}}).to_string());
                frames.push("[DONE]".into());
                let _ = sock
                    .write_all(RESPONSE_HEAD.replace("{id}", &request_id).as_bytes())
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
    (format!("http://127.0.0.1:{port}"), seen)
}

/// The bytes the `tools` of one recorded request body cost, by the same
/// measure the Core's projection event reports (name, description, schema
/// as the provider received them).
pub fn request_schema_bytes(body: &serde_json::Value) -> usize {
    body["tools"]
        .as_array()
        .map(|a| {
            a.iter()
                .map(|t| {
                    let f = &t["function"];
                    f["name"].as_str().unwrap_or_default().len()
                        + f["description"].as_str().unwrap_or_default().len()
                        + f["parameters"].to_string().len()
                })
                .sum()
        })
        .unwrap_or(0)
}

/// The tool names of one recorded request body, sorted.
pub fn request_tool_names(body: &serde_json::Value) -> Vec<String> {
    let mut v: Vec<String> = body["tools"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|t| t["function"]["name"].as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default();
    v.sort();
    v
}

/// The environment that points a Core at a scripted model server.
pub fn model_env(base: &str) -> Vec<(String, String)> {
    vec![
        ("MODBIT_OPENAI_BASE_URL".into(), base.into()),
        ("OPENAI_API_KEY".into(), String::new()),
        ("ANTHROPIC_API_KEY".into(), String::new()),
    ]
}
