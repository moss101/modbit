//! Real-boundary tests for the terminal broker's output objects and session
//! ownership (VER-06 / FIX-09 / FIX-20): the actual `modbit-core` binary
//! spawns the actual `modbit-execd`, runs real processes (a pipe run with
//! stderr, a PTY run, a noisy process) and resolves what the broker calls an
//! `output_ref` through the Core's two read paths — the `artifact.range`
//! tool and the SurfaceProtocol `ReadObjectRange` command — comparing the
//! bytes themselves, not the length of the reference.

use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};

use modbit_protocol::client::Client;
use modbit_protocol::local::{ReadyLine, decode_hex};
use modbit_protocol::v1::{
    AcquireSessionLease, ClientKind, CommandEnvelope, CreateSession, CreateTask, Id, InvokeTool,
    ObjectRangeChunk, ReadObjectRange, SessionCreated, SessionLeaseAcquired, TaskCreated,
    ToolInvoked,
};
use prost::Message;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

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
}

impl Drop for CoreProcess {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn id16(b: u8) -> Id {
    Id { value: vec![b; 16] }
}

fn envelope(command_id: Id, command_type: &str, payload: Vec<u8>) -> CommandEnvelope {
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

fn fenced(mut e: CommandEnvelope, generation: Option<u64>) -> CommandEnvelope {
    e.expected_generation = generation;
    e
}

/// A session with the lease acquired; returns (session id, lease generation).
async fn create_session(c: &mut Client, tag: u8) -> (Id, u64) {
    let ack = c
        .command(envelope(
            id16(tag),
            "CreateSession",
            CreateSession { space_id: None }.encode_to_vec(),
        ))
        .await
        .unwrap();
    let session = Client::result::<SessionCreated>(&ack)
        .unwrap()
        .session_id
        .unwrap();
    let ack = c
        .command(envelope(
            id16(tag ^ 0x5A),
            "AcquireSessionLease",
            AcquireSessionLease {
                session_id: Some(session.clone()),
                owner: "test".into(),
            }
            .encode_to_vec(),
        ))
        .await
        .unwrap();
    let g = Client::result::<SessionLeaseAcquired>(&ack)
        .unwrap()
        .lease_generation;
    (session, g)
}

async fn create_task(c: &mut Client, session: &Id, g: u64, root: &str, tag: u8) -> Id {
    let ack = c
        .command(fenced(
            envelope(
                id16(tag),
                "CreateTask",
                CreateTask {
                    session_id: Some(session.clone()),
                    goal_text: "x".into(),
                    workspace_id: None,
                    execution_profile: "local_trusted".into(),
                    origin: "cli".into(),
                    workspace_root: root.into(),
                    issue_url: String::new(),
                    issue_json: String::new(),
                }
                .encode_to_vec(),
            ),
            Some(g),
        ))
        .await
        .unwrap();
    Client::result::<TaskCreated>(&ack)
        .unwrap()
        .task_id
        .unwrap()
}

/// A tool call's id comes from a counter so no two calls in a test collide.
struct Calls(u8);

impl Calls {
    async fn invoke(
        &mut self,
        c: &mut Client,
        task: &Id,
        g: u64,
        tool: &str,
        args: &Value,
    ) -> ToolInvoked {
        self.0 += 1;
        let ack = c
            .command(fenced(
                envelope(
                    id16(0x80 + self.0),
                    "InvokeTool",
                    InvokeTool {
                        task_id: Some(task.clone()),
                        tool_name: tool.into(),
                        arguments_json: args.to_string(),
                        tool_call_id: Some(id16(0xC0 + self.0)),
                        output_budget_bytes: 65536,
                    }
                    .encode_to_vec(),
                ),
                Some(g),
            ))
            .await
            .unwrap();
        Client::result(&ack).unwrap()
    }
}

fn so(r: &ToolInvoked) -> Value {
    serde_json::from_str(&r.structured_output_json).unwrap()
}

fn plain_repo() -> (tempfile::TempDir, String) {
    let repo = tempfile::tempdir().unwrap();
    std::fs::write(repo.path().join("a.txt"), "a\n").unwrap();
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

/// The whole object through the SurfaceProtocol `ReadObjectRange` command,
/// paged, or the typed rejection code.
async fn read_object_range(c: &mut Client, tag: u8, hash: &str) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    loop {
        let ack = c
            .command(envelope(
                id16(tag),
                "ReadObjectRange",
                ReadObjectRange {
                    object_hash: hash.to_owned(),
                    offset: out.len() as u64,
                    length: 64 * 1024,
                }
                .encode_to_vec(),
            ))
            .await
            .map_err(|e| e.to_string())?;
        let chunk: ObjectRangeChunk = Client::result(&ack).map_err(|e| e.to_string())?;
        out.extend_from_slice(&chunk.data);
        if chunk.data.is_empty() || out.len() as u64 >= chunk.total_bytes {
            return Ok(out);
        }
    }
}

/// The whole object through the `artifact.range` tool, paged.
async fn artifact_range_all(
    calls: &mut Calls,
    c: &mut Client,
    task: &Id,
    g: u64,
    hash: &str,
) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    loop {
        let r = calls
            .invoke(
                c,
                task,
                g,
                "artifact.range",
                &json!({"ref": hash, "offset": out.len(), "max_bytes": 8192}),
            )
            .await;
        if r.status != "SUCCESS" {
            return Err(format!("{}: {}", r.error_code, r.error_message));
        }
        let page = so(&r);
        out.extend_from_slice(page["content"].as_str().unwrap().as_bytes());
        if page["eof"] == true || page["bytes_read"] == 0 {
            return Ok(out);
        }
    }
}

/// Everything the broker's output log holds for a session, from cursor 0, as
/// the broker streams it (the bytes the OutputRef names when nothing was
/// dropped).
async fn broker_log(data_dir: &std::path::Path, session_id: &str) -> Vec<u8> {
    let ready = std::fs::read_to_string(data_dir.join("execd").join("execd.ready")).unwrap();
    let ready = ReadyLine::parse(ready.trim()).unwrap();
    let secret = decode_hex(&ready.boot_secret_hex).unwrap();
    let mut c = modbit_terminal::ExecClient::connect(&ready.endpoint, &secret)
        .await
        .unwrap();
    c.attach(session_id, 0).await.unwrap();
    let mut chunks: Vec<(u64, Vec<u8>)> = Vec::new();
    loop {
        match c.next().await.unwrap().expect("broker closed") {
            modbit_terminal::Event::Output(o) => chunks.push((o.cursor, o.data)),
            modbit_terminal::Event::Exited(_) => break,
            _ => {}
        }
    }
    let mut out = Vec::new();
    for (cursor, data) in chunks {
        assert_eq!(cursor, out.len() as u64, "the replay has no gap or overlap");
        out.extend(data);
    }
    out
}

/// VER-06 / FIX-09: the `output_ref` a run reports resolves through the
/// Core's own object paths to the exact bytes the process wrote — for a pipe
/// run that wrote to stderr, for a PTY run, and for a plain stdout run.
#[tokio::test]
async fn execd_output_ref_resolves_through_the_core_object_paths_to_the_exact_bytes() {
    let (_repo, root) = plain_repo();
    let dir = tempfile::tempdir().unwrap();
    let core = CoreProcess::spawn(dir.path(), &[]);
    let mut c = core.client().await;
    let (session, g) = create_session(&mut c, 0x10).await;
    let task = create_task(&mut c, &session, g, &root, 0x11).await;
    let mut calls = Calls(0);

    let cases: [(&str, Value); 3] = [
        (
            "stdout only",
            json!({"argv": ["sh", "-c", "echo plain-stdout-line"], "inherit_env": true}),
        ),
        (
            "stderr interleaved",
            json!({"argv": ["sh", "-c", "echo to-out; echo to-err >&2; echo to-out-again"], "inherit_env": true}),
        ),
        (
            "pty",
            json!({"argv": ["sh", "-c", "echo pty-line-one; echo pty-line-two"], "inherit_env": true, "pty": true}),
        ),
    ];
    let mut failures: Vec<String> = Vec::new();
    for (name, args) in cases {
        let r = calls.invoke(&mut c, &task, g, "shell.exec", &args).await;
        assert_eq!(r.status, "SUCCESS", "{name}: {r:?}");
        let out = so(&r);
        let output_ref = out["output_ref"].as_str().unwrap().to_owned();
        let log = broker_log(dir.path(), out["session_id"].as_str().unwrap()).await;
        assert!(!log.is_empty(), "{name}: the broker logged output");
        assert_eq!(
            hex::encode(Sha256::digest(&log)),
            output_ref,
            "{name}: the reference is the digest of what the process wrote"
        );
        let via_protocol = read_object_range(&mut c, 0x40, &output_ref).await;
        if via_protocol.as_deref() != Ok(log.as_slice()) {
            failures.push(format!(
                "{name}: ReadObjectRange of {output_ref} -> {:?}",
                via_protocol.map(|b| b.len())
            ));
        }
        let via_tool = artifact_range_all(&mut calls, &mut c, &task, g, &output_ref).await;
        if via_tool.as_deref() != Ok(log.as_slice()) {
            failures.push(format!(
                "{name}: artifact.range of {output_ref} -> {:?}",
                via_tool.map(|b| b.len())
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "an output_ref must resolve to the exact bytes through both Core paths:\n{}",
        failures.join("\n")
    );
}

/// The same, for a background command: `shell.start` then `shell.read` until
/// the exit, whose `output_ref` is what the model is told to page. Nothing
/// of a background run passes through the tool result store, so the
/// reference resolves only if the broker's object is in the Core's store.
#[tokio::test]
async fn background_run_output_ref_resolves_through_the_core_object_paths() {
    let (_repo, root) = plain_repo();
    let dir = tempfile::tempdir().unwrap();
    let core = CoreProcess::spawn(dir.path(), &[]);
    let mut c = core.client().await;
    let (session, g) = create_session(&mut c, 0x20).await;
    let task = create_task(&mut c, &session, g, &root, 0x21).await;
    let mut calls = Calls(0);
    let start = json!({"argv": ["sh", "-c", "echo bg-out; echo bg-err >&2; sleep 0.2; echo bg-done"], "inherit_env": true});
    let r = calls.invoke(&mut c, &task, g, "shell.start", &start).await;
    assert_eq!(r.status, "SUCCESS", "{r:?}");
    let sid = so(&r)["session_id"].as_str().unwrap().to_owned();
    let output_ref = loop {
        let r = calls
            .invoke(
                &mut c,
                &task,
                g,
                "shell.read",
                &json!({"session_id": sid, "after_cursor": 0, "wait_ms": 2000}),
            )
            .await;
        assert_eq!(r.status, "SUCCESS", "{r:?}");
        let out = so(&r);
        if out["running"] == false {
            break out["exited"]["output_ref"].as_str().unwrap().to_owned();
        }
    };
    let log = broker_log(dir.path(), &sid).await;
    assert!(String::from_utf8_lossy(&log).contains("bg-done"), "{log:?}");
    assert_eq!(hex::encode(Sha256::digest(&log)), output_ref);
    assert_eq!(
        read_object_range(&mut c, 0x41, &output_ref)
            .await
            .as_deref(),
        Ok(log.as_slice()),
        "ReadObjectRange resolves a background run's output_ref"
    );
    assert_eq!(
        artifact_range_all(&mut calls, &mut c, &task, g, &output_ref)
            .await
            .as_deref(),
        Ok(log.as_slice()),
        "artifact.range resolves a background run's output_ref"
    );
}
