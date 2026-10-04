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

/// The object is read back verified: bytes that no longer match the digest
/// that names them are `OBJECT_MISMATCH` (corrupt state), never served.
#[tokio::test]
async fn a_tampered_execd_object_is_refused_on_read_not_served() {
    let (_repo, root) = plain_repo();
    let dir = tempfile::tempdir().unwrap();
    let core = CoreProcess::spawn(dir.path(), &[]);
    let mut c = core.client().await;
    let (session, g) = create_session(&mut c, 0x30).await;
    let task = create_task(&mut c, &session, g, &root, 0x31).await;
    let mut calls = Calls(0);
    let r = calls
        .invoke(
            &mut c,
            &task,
            g,
            "shell.exec",
            &json!({"argv": ["sh", "-c", "echo honest; echo noise >&2"], "inherit_env": true}),
        )
        .await;
    assert_eq!(r.status, "SUCCESS", "{r:?}");
    let output_ref = so(&r)["output_ref"].as_str().unwrap().to_owned();
    let object = dir
        .path()
        .join("core")
        .join("objects")
        .join(&output_ref[..2])
        .join(&output_ref[2..]);
    assert!(object.exists(), "the broker sealed into the Core's store");
    std::fs::write(&object, b"forged bytes").unwrap();
    let r = calls
        .invoke(
            &mut c,
            &task,
            g,
            "artifact.range",
            &json!({"ref": output_ref, "offset": 0, "max_bytes": 64}),
        )
        .await;
    assert_eq!(
        (r.status.as_str(), r.error_code.as_str()),
        ("INFRA_FAILURE", "OBJECT_MISMATCH"),
        "{r:?}"
    );
}

/// Objects a previous version's broker left in its own store
/// (`<data>/execd/objects`, unreadable by the Core) are adopted into the
/// Core's store at start, digest-verified; a file whose bytes do not match
/// its name is not adopted.
#[tokio::test]
async fn objects_left_in_the_old_broker_store_are_adopted_into_the_core_store() {
    let dir = tempfile::tempdir().unwrap();
    let good = b"old broker output\n";
    let hash = hex::encode(Sha256::digest(good));
    let legacy = dir.path().join("execd").join("objects").join(&hash[..2]);
    std::fs::create_dir_all(&legacy).unwrap();
    std::fs::write(legacy.join(&hash[2..]), good).unwrap();
    let bad_hash = hex::encode(Sha256::digest(b"claimed"));
    let bad_dir = dir
        .path()
        .join("execd")
        .join("objects")
        .join(&bad_hash[..2]);
    std::fs::create_dir_all(&bad_dir).unwrap();
    std::fs::write(bad_dir.join(&bad_hash[2..]), b"not what the name says").unwrap();
    let core = CoreProcess::spawn(dir.path(), &[]);
    let mut c = core.client().await;
    assert_eq!(
        read_object_range(&mut c, 0x50, &hash).await.as_deref(),
        Ok(&good[..])
    );
    assert!(
        read_object_range(&mut c, 0x51, &bad_hash).await.is_err(),
        "a mismatching file is not adopted"
    );
}

fn dir_bytes(path: &std::path::Path) -> u64 {
    let mut total = 0;
    if let Ok(entries) = std::fs::read_dir(path) {
        for e in entries.flatten() {
            match e.metadata() {
                Ok(m) if m.is_dir() => total += dir_bytes(&e.path()),
                Ok(m) => total += m.len(),
                Err(_) => {}
            }
        }
    }
    total
}

/// A host-level connection to the broker (the user's own terminal lease: no
/// task principal), found through the ready file the Core's broker wrote.
async fn host_client(data_dir: &std::path::Path) -> modbit_terminal::ExecClient {
    let ready = std::fs::read_to_string(data_dir.join("execd").join("execd.ready")).unwrap();
    let ready = ReadyLine::parse(ready.trim()).unwrap();
    modbit_terminal::ExecClient::connect(
        &ready.endpoint,
        &decode_hex(&ready.boot_secret_hex).unwrap(),
    )
    .await
    .unwrap()
}

/// FIX-20: through the real tools, a second task of the same session cannot
/// list, read or cancel another task's shell (a typed refusal, and the real
/// process keeps running), cannot take its handle by replaying its request
/// id; the owner can do all of it, and so can the host (the user's lease).
#[tokio::test]
async fn a_second_task_cannot_list_read_or_cancel_another_tasks_shell() {
    let (_repo, root) = plain_repo();
    let dir = tempfile::tempdir().unwrap();
    let core = CoreProcess::spawn(dir.path(), &[]);
    let mut c = core.client().await;
    let (session, g) = create_session(&mut c, 0x60).await;
    let owner = create_task(&mut c, &session, g, &root, 0x61).await;
    let intruder = create_task(&mut c, &session, g, &root, 0x62).await;
    let mut calls = Calls(0);
    let start = json!({
        "argv": ["sh", "-c", "while true; do echo tick; sleep 0.05; done"],
        "inherit_env": true, "timeout_ms": 120000, "request_id": "owned-shell"
    });
    let r = calls.invoke(&mut c, &owner, g, "shell.start", &start).await;
    assert_eq!(r.status, "SUCCESS", "{r:?}");
    let sid = so(&r)["session_id"].as_str().unwrap().to_owned();
    let refused = |r: &ToolInvoked, what: &str| {
        assert_eq!(
            (r.status.as_str(), r.error_code.as_str()),
            ("APPLICATION_FAILURE", "SESSION_NOT_OWNED"),
            "{what}: {r:?}"
        );
    };
    // The intruder: its own listing is empty, and the rest is refused.
    let r = calls
        .invoke(&mut c, &intruder, g, "shell.list", &json!({}))
        .await;
    assert_eq!(r.status, "SUCCESS", "{r:?}");
    assert_eq!(
        so(&r)["sessions"],
        json!([]),
        "another task's shell is not listed"
    );
    let r = calls
        .invoke(
            &mut c,
            &intruder,
            g,
            "shell.read",
            &json!({"session_id": sid, "after_cursor": 0}),
        )
        .await;
    refused(&r, "shell.read");
    let r = calls
        .invoke(
            &mut c,
            &intruder,
            g,
            "shell.cancel",
            &json!({"session_id": sid}),
        )
        .await;
    refused(&r, "shell.cancel");
    let r = calls
        .invoke(&mut c, &intruder, g, "shell.start", &start)
        .await;
    refused(&r, "replaying the owner's request id");
    // Nothing the intruder did touched the process.
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    let r = calls
        .invoke(&mut c, &owner, g, "shell.list", &json!({}))
        .await;
    let listed = so(&r)["sessions"].as_array().unwrap().clone();
    assert_eq!(listed.len(), 1, "{listed:?}");
    assert_eq!(listed[0]["session_id"], sid.as_str());
    assert_eq!(listed[0]["running"], true, "the refused cancel did nothing");
    // The host (the user's own lease) sees and reads it.
    let mut host = host_client(dir.path()).await;
    host.list().await.unwrap();
    let Some(modbit_terminal::Event::Sessions(all)) = host.next().await.unwrap() else {
        panic!("expected a listing")
    };
    assert!(all.iter().any(|s| s.session_id == sid && s.running));
    // The owner reads and cancels it.
    let r = calls
        .invoke(
            &mut c,
            &owner,
            g,
            "shell.read",
            &json!({"session_id": sid, "after_cursor": 0, "wait_ms": 300, "max_bytes": 64}),
        )
        .await;
    assert_eq!(r.status, "SUCCESS", "{r:?}");
    assert!(so(&r)["preview"].as_str().unwrap().starts_with("tick\n"));
    let r = calls
        .invoke(
            &mut c,
            &owner,
            g,
            "shell.cancel",
            &json!({"session_id": sid}),
        )
        .await;
    assert_eq!(r.status, "SUCCESS", "{r:?}");
    assert_eq!(so(&r)["cancelled"], true);
}

/// FIX-20 / FIX-09 through the real tools: a 20 MiB run under a 2 MiB replay
/// window. A cursor outside the window is the typed `CURSOR_EXPIRED` naming
/// the oldest cursor; from it the preview is the exact stream bytes; the
/// run's `output_ref` resolves in the Core's store to exactly the retained
/// tail; and disk stays a few windows, not 20 MiB.
#[tokio::test]
async fn a_noisy_shell_is_bounded_and_its_window_is_typed_through_the_tools() {
    let (_repo, root) = plain_repo();
    let dir = tempfile::tempdir().unwrap();
    let core = CoreProcess::spawn(
        dir.path(),
        &[
            ("MODBIT_EXECD_REPLAY_WINDOW_BYTES", "2097152"),
            ("MODBIT_EXECD_SEGMENT_BYTES", "524288"),
        ],
    );
    let mut c = core.client().await;
    let (session, g) = create_session(&mut c, 0x70).await;
    let task = create_task(&mut c, &session, g, &root, 0x71).await;
    let mut calls = Calls(0);
    const TOTAL: u64 = 20 * 1024 * 1024;
    // The stream is "abcdefghij\n" repeated: the byte at offset o is known.
    let at = |o: u64| b"abcdefghij\n"[(o % 11) as usize];
    let start = json!({
        "argv": ["sh", "-c", format!("yes abcdefghij | head -c {TOTAL}")],
        "inherit_env": true, "timeout_ms": 120000
    });
    let r = calls.invoke(&mut c, &task, g, "shell.start", &start).await;
    assert_eq!(r.status, "SUCCESS", "{r:?}");
    let sid = so(&r)["session_id"].as_str().unwrap().to_owned();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
    loop {
        let r = calls
            .invoke(&mut c, &task, g, "shell.list", &json!({}))
            .await;
        let s = so(&r)["sessions"][0].clone();
        if s["running"] == false {
            assert_eq!(s["bytes_so_far"], TOTAL);
            break;
        }
        assert!(std::time::Instant::now() < deadline, "the run did not end");
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    // Outside the window: typed, with where the window starts.
    let r = calls
        .invoke(
            &mut c,
            &task,
            g,
            "shell.read",
            &json!({"session_id": sid, "after_cursor": 0, "max_bytes": 64}),
        )
        .await;
    assert_eq!(
        (r.status.as_str(), r.error_code.as_str()),
        ("APPLICATION_FAILURE", "CURSOR_EXPIRED"),
        "{r:?}"
    );
    let oldest: u64 = r
        .error_message
        .split("oldest_cursor=")
        .nth(1)
        .unwrap()
        .trim()
        .parse()
        .unwrap();
    assert!(oldest > 0 && oldest < TOTAL, "{}", r.error_message);
    // Inside it: exact bytes, from a cursor mid-line.
    let from = oldest + 5;
    let r = calls
        .invoke(
            &mut c,
            &task,
            g,
            "shell.read",
            &json!({"session_id": sid, "after_cursor": from, "max_bytes": 64, "wait_ms": 2000}),
        )
        .await;
    assert_eq!(r.status, "SUCCESS", "{r:?}");
    let out = so(&r);
    let preview = out["preview"].as_str().unwrap().as_bytes().to_vec();
    assert_eq!(preview.len(), 64);
    for (i, b) in preview.iter().enumerate() {
        assert_eq!(*b, at(from + i as u64), "byte {i} at cursor {from}");
    }
    // The exit, with its object: the retained tail, in the Core's store.
    let exited = out["exited"].clone();
    assert!(exited.is_object(), "the read reached the exit: {out}");
    assert_eq!(exited["total_bytes"], TOTAL);
    assert_eq!(exited["retained_from"], oldest);
    let output_ref = exited["output_ref"].as_str().unwrap().to_owned();
    let tail = read_object_range(&mut c, 0x42, &output_ref).await.unwrap();
    assert_eq!(tail.len() as u64, TOTAL - oldest);
    assert!(
        tail.iter()
            .enumerate()
            .all(|(i, b)| *b == at(oldest + i as u64)),
        "the object is the retained tail, byte for byte"
    );
    assert_eq!(hex::encode(Sha256::digest(&tail)), output_ref);
    // Disk: a few windows, never the 20 MiB the process wrote.
    let used =
        dir_bytes(&dir.path().join("execd")) + dir_bytes(&dir.path().join("core").join("objects"));
    assert!(
        used < 10 * 1024 * 1024,
        "broker + objects hold {used} bytes"
    );
}
