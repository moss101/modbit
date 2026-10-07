//! Real-boundary proofs of REQ-PX-101 (PauseTask / ResumeTask over the
//! runtime park), REQ-PX-061 (a checkpoint at every turn, an exactly
//! reversible restore, a fork at a turn) and REQ-PX-102 (names, retention and
//! collection): the actual `modbit-core` binary over its real socket, a real
//! git repository, a real SQLite event store and object directory, and a
//! scripted OpenAI-compatible server standing in for the model. Failure
//! injection kills the Core (a real `SIGKILL`, or an abort at a named point
//! of a capture, a restore or a collection) and checks what the next Core
//! finds.

use std::collections::BTreeMap;
use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};
use std::time::Duration;

use modbit_protocol::client::{Client, ClientError};
use modbit_protocol::local::{ReadyLine, decode_hex};
use modbit_protocol::v1::{
    AcquireSessionLease, ApprovalList, CancelTask, CheckpointCreated, CheckpointGcReport,
    CheckpointList, CheckpointNamed, CheckpointRestoreResult, ClientKind, CommandEnvelope,
    CommandStatus, CreateCheckpoint, CreateSession, CreateTask, ForkTask, GetSessionSnapshot,
    GetTaskStatus, Id, ListApprovals, ListCheckpoints, NameCheckpoint, PauseTask, ResolveApproval,
    RestoreCheckpoint, ResumeTask, RunCheckpointGc, SessionCreated, SessionLeaseAcquired,
    SessionSnapshot, StartTask, TaskCreated, TaskForked, TaskPauseResult, TaskResumeResult,
    TaskRunStarted, TaskStatus,
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

    /// Wait for the process to end on its own (an abort at an injected fault).
    fn wait_exit(&mut self, secs: u64) -> bool {
        let deadline = std::time::Instant::now() + Duration::from_secs(secs);
        loop {
            if let Ok(Some(_)) = self.child.try_wait() {
                return true;
            }
            if std::time::Instant::now() >= deadline {
                return false;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
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

fn code_of<T: std::fmt::Debug>(r: Result<T, ClientError>) -> String {
    match r {
        Err(ClientError::Rejected { code, .. }) => code,
        other => panic!("expected a refusal, got {other:?}"),
    }
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

/// Every file of a worktree (not `.git`) by content hash: what "exactly" is
/// compared with.
fn tree(root: &str) -> BTreeMap<String, String> {
    use sha2::Digest;
    let mut out = BTreeMap::new();
    let mut stack = vec![std::path::PathBuf::from(root)];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d).unwrap().flatten() {
            let p = e.path();
            if p.file_name().is_some_and(|n| n == ".git") {
                continue;
            }
            if p.is_dir() {
                stack.push(p);
            } else if let Ok(b) = std::fs::read(&p) {
                out.insert(
                    p.strip_prefix(root)
                        .unwrap()
                        .to_string_lossy()
                        .replace('\\', "/"),
                    hex::encode(sha2::Sha256::digest(b)),
                );
            }
        }
    }
    out
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

/// A plan, `n` files created one per turn, then the completion.
fn files_script(n: usize, delay_ms: Option<(usize, u64)>) -> Vec<Value> {
    let names: Vec<String> = (1..=n).map(|i| format!("f{i}.txt")).collect();
    let mut s = vec![
        json!({"calls": [{"name": "plan.update", "args": {"outcome": "write the files", "expected_files": names}}]}),
    ];
    for (i, name) in names.iter().enumerate() {
        let mut step = json!({"calls": [{"name": "change.apply", "args": {"path": name, "op": "create", "content": format!("file {}\n", i + 1)}}]});
        if let Some((at, ms)) = delay_ms
            && at == i + 1
        {
            step["delay_ms"] = json!(ms);
        }
        s.push(step);
    }
    s.push(json!({"calls": [{"name": "task.complete", "args": {"summary": "done", "self_review": {"findings": []}}}]}));
    s
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

    async fn run_to_review(&mut self) {
        self.start().await;
        let st = self
            .until("ReadyForReview", 120, |s| s.state == "ReadyForReview")
            .await;
        assert!(!st.loop_alive);
    }

    async fn checkpoints(&mut self) -> CheckpointList {
        ok(
            &mut self.c,
            "ListCheckpoints",
            ListCheckpoints {
                task_id: Some(self.task.clone()),
            },
            None,
        )
        .await
    }

    async fn restore(
        &mut self,
        mut req: RestoreCheckpoint,
    ) -> Result<(CheckpointRestoreResult, bool), ClientError> {
        req.task_id = Some(self.task.clone());
        send(&mut self.c, None, "RestoreCheckpoint", req, Some(self.g)).await
    }

    /// The task's events of the given types, with payloads.
    async fn events(&self, types: &[&str]) -> Vec<(String, Value)> {
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
                out.push((ev.event_type, p["payload"].clone()));
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

fn turn_checkpoint(l: &CheckpointList, ordinal: u32) -> modbit_protocol::v1::CheckpointView {
    l.checkpoints
        .iter()
        .rev()
        .find(|c| c.turn_ordinal == ordinal && c.reason == "turn_boundary")
        .unwrap_or_else(|| panic!("no checkpoint for turn {ordinal}: {l:?}"))
        .clone()
}

fn base_tree(root: &str) -> BTreeMap<String, String> {
    tree(root)
}

// ============================================================================
// REQ-PX-061: a checkpoint at every turn, an exactly reversible restore, a
// fork at a turn.
// ============================================================================

/// QUAL-PX-061 (real Core, real worktree): every turn leaves a checkpoint;
/// restoring turn 2 reverts the files turns 3-5 made byte for byte and records
/// an event without erasing history; the redo restores the pre-restore state
/// exactly (file hashes); replaying the restore's command id restores once;
/// a redo after work moved on and a restore against a stale epoch are refused
/// with typed reasons; the user's choice keeps an edited file.
#[tokio::test]
async fn px_061_every_turn_leaves_a_checkpoint_and_a_restore_is_exactly_reversible() {
    let (mut fx, _seen) = Fx::new(files_script(4, None), &[("a.txt", "a\n")], &[]).await;
    fx.run_to_review().await;
    let at_end = tree(&fx.root);
    assert!(at_end.contains_key("f4.txt"), "{at_end:?}");
    let l = fx.checkpoints().await;
    // Turns 1..=6: the plan, four files, the completion's — each a checkpoint.
    for t in 1..=6 {
        let c = turn_checkpoint(&l, t);
        assert!(c.turn_id.is_some(), "{c:?}");
        assert!(c.cost.is_some(), "{c:?}");
    }
    // The state the turn-2 checkpoint holds: a.txt and f1.txt.
    let turn2 = turn_checkpoint(&l, 3); // turn 1 plans; turn 3 has created f2
    let mut at_turn2 = base_tree(&fx.root);
    at_turn2.retain(|p, _| !matches!(p.as_str(), "f3.txt" | "f4.txt"));
    assert!(at_turn2.contains_key("f2.txt") && !at_turn2.contains_key("f3.txt"));

    // Restore to that turn by ordinal.
    let cmd = rand_id();
    let (r, replayed): (CheckpointRestoreResult, bool) = send(
        &mut fx.c,
        Some(cmd.clone()),
        "RestoreCheckpoint",
        RestoreCheckpoint {
            task_id: Some(fx.task.clone()),
            turn_ordinal: 3,
            ..Default::default()
        },
        Some(fx.g),
    )
    .await
    .unwrap();
    assert!(r.restored && !replayed, "{r:?}");
    assert_eq!(r.checkpoint_id, turn2.checkpoint_id);
    assert!(!r.pre_restore_checkpoint_id.is_empty(), "{r:?}");
    assert_eq!(tree(&fx.root), at_turn2, "restore reverts files exactly");
    // Replaying the command id restores once: nothing is written again.
    std::fs::write(fx.root.clone() + "/probe.txt", "x").unwrap();
    let (r2, replayed): (CheckpointRestoreResult, bool) = send(
        &mut fx.c,
        Some(cmd),
        "RestoreCheckpoint",
        RestoreCheckpoint {
            task_id: Some(fx.task.clone()),
            turn_ordinal: 3,
            ..Default::default()
        },
        Some(fx.g),
    )
    .await
    .unwrap();
    assert!(replayed && r2.replayed, "{r2:?}");
    assert_eq!(r2.pre_restore_checkpoint_id, r.pre_restore_checkpoint_id);
    assert!(
        std::path::Path::new(&(fx.root.clone() + "/probe.txt")).exists(),
        "a replay writes nothing"
    );
    std::fs::remove_file(fx.root.clone() + "/probe.txt").unwrap();
    // History is kept: every earlier checkpoint is still listed, and the
    // restore is an event beside them.
    let after = fx.checkpoints().await;
    assert!(after.checkpoints.len() > l.checkpoints.len());
    for c in &l.checkpoints {
        assert!(
            after
                .checkpoints
                .iter()
                .any(|a| a.checkpoint_id == c.checkpoint_id)
        );
    }
    let restores = fx.events(&["CheckpointRestored"]).await;
    assert_eq!(restores.len(), 1, "{restores:?}");
    assert_eq!(
        restores[0].1["pre_restore_checkpoint_id"],
        json!(r.pre_restore_checkpoint_id)
    );

    // A stale epoch is refused before anything is written.
    let stale = fx
        .restore(RestoreCheckpoint {
            turn_ordinal: 3,
            expected_current_epoch: 1,
            ..Default::default()
        })
        .await
        .unwrap()
        .0;
    assert_eq!(stale.refusal, "STALE_EPOCH", "{stale:?}");

    // The redo restores the state before the restore, byte for byte.
    let (redo, _) = fx
        .restore(RestoreCheckpoint {
            checkpoint_id: r.pre_restore_checkpoint_id.clone(),
            redo: true,
            ..Default::default()
        })
        .await
        .unwrap();
    assert!(redo.restored && redo.redo, "{redo:?}");
    assert_eq!(
        tree(&fx.root),
        at_end,
        "redo restores the pre-restore state exactly"
    );
    // The redo is itself undoable, and a second redo of the same checkpoint is
    // refused: work moved on since that restore.
    let again = fx
        .restore(RestoreCheckpoint {
            checkpoint_id: r.pre_restore_checkpoint_id.clone(),
            redo: true,
            ..Default::default()
        })
        .await
        .unwrap()
        .0;
    assert_eq!(again.refusal, "REDO_SUPERSEDED", "{again:?}");
    let not_pre = fx
        .restore(RestoreCheckpoint {
            checkpoint_id: turn2.checkpoint_id.clone(),
            redo: true,
            ..Default::default()
        })
        .await
        .unwrap()
        .0;
    assert_eq!(not_pre.refusal, "NOT_A_PRE_RESTORE", "{not_pre:?}");
    let (undo_redo, _) = fx
        .restore(RestoreCheckpoint {
            checkpoint_id: redo.pre_restore_checkpoint_id.clone(),
            redo: true,
            ..Default::default()
        })
        .await
        .unwrap();
    assert!(undo_redo.restored, "{undo_redo:?}");
    assert_eq!(tree(&fx.root), at_turn2);

    // The user's choice: a file edited by hand keeps its content.
    std::fs::write(fx.root.clone() + "/f1.txt", "my edit\n").unwrap();
    let (kept, _) = fx
        .restore(RestoreCheckpoint {
            turn_ordinal: 2,
            keep_paths: vec!["f1.txt".into()],
            ..Default::default()
        })
        .await
        .unwrap();
    assert!(kept.restored, "{kept:?}");
    assert_eq!(
        std::fs::read_to_string(fx.root.clone() + "/f1.txt").unwrap(),
        "my edit\n"
    );
    assert!(kept.kept_paths.contains(&"f1.txt".to_owned()), "{kept:?}");
}

/// QUAL-PX-061 / PX-102 fork: a fork at turn N has that turn's file contents
/// and the parent is untouched; the fork may be taken by turn id, by ordinal
/// or by label; a fork at a turn that left no checkpoint is refused typed.
#[tokio::test]
async fn px_061_a_fork_at_a_turn_has_that_turns_files_and_leaves_the_parent_alone() {
    let (mut fx, _seen) = Fx::new(files_script(3, None), &[("a.txt", "a\n")], &[]).await;
    fx.run_to_review().await;
    let parent_before = tree(&fx.root);
    let l = fx.checkpoints().await;
    let t3 = turn_checkpoint(&l, 3); // after f2 was created
    let fork: TaskForked = ok(
        &mut fx.c,
        "ForkTask",
        ForkTask {
            task_id: Some(fx.task.clone()),
            turn_ordinal: 3,
            ..Default::default()
        },
        Some(fx.g),
    )
    .await;
    assert_eq!(fork.checkpoint_id, t3.checkpoint_id);
    assert_eq!(fork.turn_ordinal, 3);
    assert_eq!(fork.turn_id, t3.turn_id);
    let fork_tree = tree(&fork.worktree);
    assert!(fork_tree.contains_key("f1.txt") && fork_tree.contains_key("f2.txt"));
    assert!(!fork_tree.contains_key("f3.txt"), "{fork_tree:?}");
    for (p, h) in &fork_tree {
        assert_eq!(
            parent_before.get(p),
            Some(h),
            "{p} is the parent's content at that turn"
        );
    }
    assert_eq!(tree(&fx.root), parent_before, "the parent is unaffected");
    // By turn id.
    let by_id: TaskForked = ok(
        &mut fx.c,
        "ForkTask",
        ForkTask {
            task_id: Some(fx.task.clone()),
            turn_id: t3.turn_id.clone(),
            ..Default::default()
        },
        Some(fx.g),
    )
    .await;
    assert_eq!(by_id.checkpoint_id, t3.checkpoint_id);
    // By label.
    let named: CheckpointNamed = ok(
        &mut fx.c,
        "NameCheckpoint",
        NameCheckpoint {
            task_id: Some(fx.task.clone()),
            checkpoint_id: t3.checkpoint_id.clone(),
            name: "after f2".into(),
        },
        Some(fx.g),
    )
    .await;
    assert_eq!(named.name, "after f2");
    let by_name: TaskForked = ok(
        &mut fx.c,
        "ForkTask",
        ForkTask {
            task_id: Some(fx.task.clone()),
            checkpoint_name: "after f2".into(),
            ..Default::default()
        },
        Some(fx.g),
    )
    .await;
    assert_eq!(by_name.checkpoint_id, t3.checkpoint_id);
    // A turn that left no checkpoint, and an ambiguous fork point, are refused.
    let none = send::<_, TaskForked>(
        &mut fx.c,
        None,
        "ForkTask",
        ForkTask {
            task_id: Some(fx.task.clone()),
            turn_ordinal: 999,
            ..Default::default()
        },
        Some(fx.g),
    )
    .await;
    assert_eq!(code_of(none), "NO_CHECKPOINT_FOR_TURN");
    let both = send::<_, TaskForked>(
        &mut fx.c,
        None,
        "ForkTask",
        ForkTask {
            task_id: Some(fx.task.clone()),
            turn_ordinal: 2,
            checkpoint_name: "after f2".into(),
            ..Default::default()
        },
        Some(fx.g),
    )
    .await;
    assert_eq!(code_of(both), "BAD_PAYLOAD");
    // The listing says why the collector keeps what it keeps.
    let l = fx.checkpoints().await;
    let held = l
        .checkpoints
        .iter()
        .find(|c| c.checkpoint_id == t3.checkpoint_id)
        .unwrap();
    assert!(held.retention.contains(&"NAMED".to_owned()), "{held:?}");
    assert!(
        held.retention.contains(&"FORK_PARENT".to_owned()),
        "{held:?}"
    );
    assert_eq!(held.name, "after f2");
}

/// REQ-PX-061 bound: a turn that changed nothing reads nothing and writes no
/// content blob. A repository with thousands of tracked files and hundreds of
/// dirty ones: the second capture is served from the cache (every dirty file a
/// hit, none hashed, no blob written), its delta lists nothing, and it stays
/// inside a stated, generous bound.
#[tokio::test]
async fn px_061_a_capture_of_unchanged_files_is_cheap_and_bounded() {
    let tracked: Vec<(String, String)> = (0..3000)
        .map(|i| {
            (
                format!("src/d{}/t{i}.txt", i % 50),
                format!("tracked {i}\n"),
            )
        })
        .collect();
    let refs: Vec<(&str, &str)> = tracked
        .iter()
        .map(|(a, b)| (a.as_str(), b.as_str()))
        .collect();
    let (mut fx, _seen) = Fx::new(vec![], &refs, &[]).await;
    // 400 dirty files: 200 edited tracked, 200 new untracked.
    for i in 0..200 {
        std::fs::write(
            format!("{}/src/d{}/t{i}.txt", fx.root, i % 50),
            format!("edited {i}\n"),
        )
        .unwrap();
        std::fs::write(format!("{}/new{i}.txt", fx.root), format!("new {i}\n")).unwrap();
    }
    // Past the racy window, so the first capture's hashes may be reused.
    tokio::time::sleep(Duration::from_millis(2500)).await;
    let first: CheckpointCreated = ok(
        &mut fx.c,
        "CreateCheckpoint",
        CreateCheckpoint {
            task_id: Some(fx.task.clone()),
            kind: "BASELINE".into(),
            reason: "cost".into(),
            ..Default::default()
        },
        Some(fx.g),
    )
    .await;
    let cost1 = first.checkpoint.unwrap().cost.unwrap();
    assert_eq!(cost1.hashed_files, 400, "{cost1:?}");
    assert_eq!(cost1.blobs_written, 400);
    let started = std::time::Instant::now();
    let second: CheckpointCreated = ok(
        &mut fx.c,
        "CreateCheckpoint",
        CreateCheckpoint {
            task_id: Some(fx.task.clone()),
            kind: "DELTA".into(),
            reason: "cost".into(),
            ..Default::default()
        },
        Some(fx.g),
    )
    .await;
    let wall = started.elapsed();
    let v = second.checkpoint.unwrap();
    let cost2 = v.cost.unwrap();
    assert_eq!(v.kind, "DELTA");
    assert_eq!(v.files, 0, "the delta of nothing lists nothing: {v:?}");
    assert_eq!(cost2.cache_hits, 400, "{cost2:?}");
    assert_eq!(
        (cost2.hashed_files, cost2.blobs_written, cost2.bytes_written),
        (0, 0, 0)
    );
    // The stated bound: 3000 tracked and 400 dirty files, captured unchanged
    // in well under ten seconds on any CI host (a typical run is tens of ms).
    assert!(wall < Duration::from_secs(10), "{wall:?} {cost2:?}");
    assert!(cost2.capture_ms < 10_000, "{cost2:?}");
    // A change is seen: one file edited is the delta's one entry.
    std::fs::write(format!("{}/new7.txt", fx.root), "changed\n").unwrap();
    let third: CheckpointCreated = ok(
        &mut fx.c,
        "CreateCheckpoint",
        CreateCheckpoint {
            task_id: Some(fx.task.clone()),
            kind: "DELTA".into(),
            reason: "cost".into(),
            ..Default::default()
        },
        Some(fx.g),
    )
    .await;
    let v3 = third.checkpoint.unwrap();
    assert_eq!(v3.files, 1, "{v3:?}");
    assert_eq!(v3.cost.unwrap().hashed_files, 1);
}

/// Failure injection (REQ-PX-061): the Core dies in the middle of a capture,
/// and in a restore between the pre-restore checkpoint and the write, and in
/// the middle of the write. The next Core finds a consistent task each time:
/// the worktree is exactly the pre-restore state (never a mixture), the
/// interrupted restore is rolled back on the log, and the same restore command
/// then completes.
#[tokio::test]
async fn px_061_kill_during_capture_and_during_restore_recovers_consistently() {
    let (mut fx, _seen) = Fx::new(files_script(3, None), &[("a.txt", "a\n")], &[]).await;
    fx.run_to_review().await;
    let at_end = tree(&fx.root);
    let l = fx.checkpoints().await;
    let target = turn_checkpoint(&l, 2); // f1 only
    let mut at_target = at_end.clone();
    at_target.retain(|p, _| !matches!(p.as_str(), "f2.txt" | "f3.txt"));

    // 1. Killed in the middle of a capture: a started epoch with no commit.
    fx.restart(&[("MODBIT_FAULT_CHECKPOINT", "MID_CAPTURE")])
        .await;
    let died =
        fx.c.command(envelope(
            rand_id(),
            "CreateCheckpoint",
            CreateCheckpoint {
                task_id: Some(fx.task.clone()),
                reason: "doomed".into(),
                ..Default::default()
            }
            .encode_to_vec(),
            Some(fx.g),
        ))
        .await;
    assert!(died.is_err());
    assert!(
        fx.core.wait_exit(10),
        "the Core aborted at the injected fault"
    );
    fx.restart(&[]).await;
    let l2 = fx.checkpoints().await;
    assert_eq!(
        l2.current_checkpoint_id, l.current_checkpoint_id,
        "the half capture changed nothing"
    );
    assert!(
        !l2.checkpoints
            .iter()
            .any(|c| c.reason == "doomed" && c.status == "CURRENT"),
        "{l2:?}"
    );
    let fine: CheckpointCreated = ok(
        &mut fx.c,
        "CreateCheckpoint",
        CreateCheckpoint {
            task_id: Some(fx.task.clone()),
            reason: "after the crash".into(),
            ..Default::default()
        },
        Some(fx.g),
    )
    .await;
    assert!(fine.committed, "{fine:?}");
    assert_eq!(tree(&fx.root), at_end);

    // 2. Killed after the pre-restore checkpoint, before the first byte.
    fx.restart(&[("MODBIT_FAULT_CHECKPOINT", "AFTER_PRE_RESTORE")])
        .await;
    let cmd = rand_id();
    let died = send::<_, CheckpointRestoreResult>(
        &mut fx.c,
        Some(cmd.clone()),
        "RestoreCheckpoint",
        RestoreCheckpoint {
            task_id: Some(fx.task.clone()),
            checkpoint_id: target.checkpoint_id.clone(),
            ..Default::default()
        },
        Some(fx.g),
    )
    .await;
    assert!(died.is_err());
    assert!(fx.core.wait_exit(10));
    assert_eq!(tree(&fx.root), at_end, "nothing was written");
    fx.restart(&[]).await;
    assert_eq!(tree(&fx.root), at_end);
    let l3 = fx.checkpoints().await;
    assert!(
        l3.checkpoints
            .iter()
            .any(|c| c.reason == "pre_restore" && c.status != "STARTED"),
        "the pre-restore checkpoint is committed: {l3:?}"
    );
    // The same command completes the restore.
    let (done, _) = send::<_, CheckpointRestoreResult>(
        &mut fx.c,
        Some(cmd),
        "RestoreCheckpoint",
        RestoreCheckpoint {
            task_id: Some(fx.task.clone()),
            checkpoint_id: target.checkpoint_id.clone(),
            ..Default::default()
        },
        Some(fx.g),
    )
    .await
    .unwrap();
    assert!(done.restored, "{done:?}");
    assert_eq!(tree(&fx.root), at_target);
    // Back to the end for the next kill.
    let (redo, _) = fx
        .restore(RestoreCheckpoint {
            checkpoint_id: done.pre_restore_checkpoint_id.clone(),
            redo: true,
            ..Default::default()
        })
        .await
        .unwrap();
    assert!(redo.restored);
    assert_eq!(tree(&fx.root), at_end);

    // 3. Killed with half the files written: a mixture on disk, resolved by
    // the next Core to exactly the pre-restore state.
    fx.restart(&[("MODBIT_FAULT_CHECKPOINT", "MID_RESTORE_WRITE")])
        .await;
    let cmd = rand_id();
    let died = send::<_, CheckpointRestoreResult>(
        &mut fx.c,
        Some(cmd.clone()),
        "RestoreCheckpoint",
        RestoreCheckpoint {
            task_id: Some(fx.task.clone()),
            checkpoint_id: target.checkpoint_id.clone(),
            ..Default::default()
        },
        Some(fx.g),
    )
    .await;
    assert!(died.is_err());
    assert!(fx.core.wait_exit(10));
    let mixture = tree(&fx.root);
    assert_ne!(mixture, at_end, "half the restore reached the disk");
    assert_ne!(mixture, at_target, "and not all of it");
    fx.restart(&[]).await;
    assert_eq!(
        tree(&fx.root),
        at_end,
        "the next Core rolled the interrupted restore back to the pre-restore state"
    );
    let trail = fx
        .events(&["CheckpointRestoreStarted", "CheckpointRestoreRolledBack"])
        .await;
    assert!(
        trail.iter().any(|(t, p)| t == "CheckpointRestoreRolledBack"
            && p["reason"] == "CORE_RESTARTED_MID_RESTORE"),
        "{trail:?}"
    );
    // The same command id then does the restore, once.
    let (done, replayed) = send::<_, CheckpointRestoreResult>(
        &mut fx.c,
        Some(cmd),
        "RestoreCheckpoint",
        RestoreCheckpoint {
            task_id: Some(fx.task.clone()),
            checkpoint_id: target.checkpoint_id.clone(),
            ..Default::default()
        },
        Some(fx.g),
    )
    .await
    .unwrap();
    assert!(done.restored && !replayed, "{done:?}");
    assert_eq!(tree(&fx.root), at_target);
}

// ============================================================================
// REQ-PX-102: names, retention and collection.
// ============================================================================

/// A finished task with a plan-and-files run, then `extra` hand-made
/// checkpoints each holding content nothing else on the log names (a file the
/// user wrote), so the collector has blobs of its own to free.
async fn many_checkpoints(extra: usize) -> Fx {
    let (mut fx, _seen) = Fx::new(files_script(6, None), &[("a.txt", "a\n")], &[]).await;
    fx.run_to_review().await;
    for i in 0..extra {
        // Unique content, and enough elapsed time that the mtime is not racy.
        std::fs::write(
            format!("{}/hand.txt", fx.root),
            format!("hand-written version {i}\n").repeat(40),
        )
        .unwrap();
        let _: CheckpointCreated = ok(
            &mut fx.c,
            "CreateCheckpoint",
            CreateCheckpoint {
                task_id: Some(fx.task.clone()),
                reason: format!("hand {i}"),
                ..Default::default()
            },
            Some(fx.g),
        )
        .await;
    }
    // A finished task: the collector thins only what no live task can need.
    let _: modbit_protocol::v1::TaskCancelRequested = ok(
        &mut fx.c,
        "CancelTask",
        CancelTask {
            task_id: Some(fx.task.clone()),
        },
        Some(fx.g),
    )
    .await;
    let st = fx.status().await;
    assert_eq!(st.state, "Cancelled", "{st:?}");
    fx
}

async fn name(fx: &mut Fx, id: &str, name: &str) -> Result<(CheckpointNamed, bool), ClientError> {
    let (task, g) = (fx.task.clone(), fx.g);
    send(
        &mut fx.c,
        None,
        "NameCheckpoint",
        NameCheckpoint {
            task_id: Some(task),
            checkpoint_id: id.into(),
            name: name.into(),
        },
        Some(g),
    )
    .await
}

async fn gc(
    fx: &mut Fx,
    keep_recent: u32,
    dry_run: bool,
) -> Result<(CheckpointGcReport, bool), ClientError> {
    let (task, session, g) = (fx.task.clone(), fx.session.clone(), fx.g);
    send(
        &mut fx.c,
        None,
        "RunCheckpointGc",
        RunCheckpointGc {
            session_id: Some(session),
            task_id: Some(task),
            reason: "test".into(),
            policy: Some(modbit_protocol::v1::CheckpointRetentionPolicy {
                keep_recent,
                min_age_ms: 1,
                max_bytes: 0,
            }),
            dry_run,
        },
        Some(g),
    )
    .await
}

fn object_files(data: &std::path::Path) -> (usize, u64) {
    let mut n = 0;
    let mut bytes = 0;
    let mut stack = vec![data.join("core").join("objects")];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d).into_iter().flatten().flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else if let Ok(m) = e.metadata() {
                n += 1;
                bytes += m.len();
            }
        }
    }
    (n, bytes)
}

/// QUAL-PX-102 (real Core, real worktree, the real object store): a finished
/// task with forty-odd checkpoints, three named and one forked from; the
/// collector thins it by policy and the named, fork-parent, latest and
/// pre-restore checkpoints survive with their chains; every survivor restores;
/// a collected one answers COLLECTED; the report's counts equal the change on
/// the filesystem; a second run changes nothing; names are unique.
#[tokio::test]
async fn px_102_the_collector_thins_a_finished_task_and_never_removes_what_is_referenced() {
    let mut fx = many_checkpoints(34).await;
    let l = fx.checkpoints().await;
    assert!(l.checkpoints.len() >= 40, "{}", l.checkpoints.len());
    let hand: Vec<_> = l
        .checkpoints
        .iter()
        .filter(|c| c.reason.starts_with("hand "))
        .collect();
    // Three labels; one is refused as a duplicate; one checkpoint holds one name.
    let (n1, _) = name(&mut fx, &turn_checkpoint(&l, 3).checkpoint_id, "plan done")
        .await
        .unwrap();
    assert_eq!(n1.name, "plan done");
    name(&mut fx, &hand[5].checkpoint_id, "hand five")
        .await
        .unwrap();
    name(&mut fx, &hand[20].checkpoint_id, "hand twenty")
        .await
        .unwrap();
    assert_eq!(
        code_of(name(&mut fx, &hand[6].checkpoint_id, "hand five").await),
        "NAME_TAKEN"
    );
    assert_eq!(
        code_of(name(&mut fx, &hand[6].checkpoint_id, "").await),
        "BAD_NAME"
    );
    let (again, replayed) = name(&mut fx, &hand[5].checkpoint_id, "hand five")
        .await
        .unwrap();
    assert!(replayed, "{again:?}");
    // A fork is taken from one checkpoint: it must outlive the collection.
    let forked: TaskForked = ok(
        &mut fx.c,
        "ForkTask",
        ForkTask {
            task_id: Some(fx.task.clone()),
            checkpoint_id: hand[11].checkpoint_id.clone(),
            ..Default::default()
        },
        Some(fx.g),
    )
    .await;
    // Restoring once leaves a pre-restore checkpoint a redo needs.
    let (restored, _) = fx
        .restore(RestoreCheckpoint {
            checkpoint_id: hand[30].checkpoint_id.clone(),
            ..Default::default()
        })
        .await
        .unwrap();
    assert!(restored.restored, "{restored:?}");
    let before = fx.checkpoints().await;
    let latest = before.current_checkpoint_id.clone();
    let (objects_before, bytes_before) = object_files(&fx.data);

    // A dry run decides and removes nothing.
    let (dry, _) = gc(&mut fx, 1, true).await.unwrap();
    assert!(
        dry.dry_run && !dry.removed_ids.is_empty() && dry.blobs_removed == 0,
        "{dry:?}"
    );
    assert_eq!(object_files(&fx.data), (objects_before, bytes_before));

    let (report, _) = gc(&mut fx, 1, false).await.unwrap();
    assert_eq!(
        report.removed_ids, dry.removed_ids,
        "the dry run said what the run did"
    );
    assert!(report.removed > 10, "{report:?}");
    assert!(
        report.blobs_removed > 0 && report.bytes_freed > 0,
        "{report:?}"
    );
    let (objects_after, bytes_after) = object_files(&fx.data);
    assert_eq!(
        (objects_before - objects_after) as u32,
        report.blobs_removed,
        "the report's counts equal the change on the filesystem"
    );
    assert_eq!(bytes_before - bytes_after, report.bytes_freed);
    // The next run finds nothing to do, and leaves no record.
    let (second, _) = gc(&mut fx, 1, false).await.unwrap();
    assert_eq!(
        (second.removed, second.blobs_removed, second.bytes_freed),
        (0, 0, 0),
        "{second:?}"
    );
    assert_eq!(
        fx.events(&["CheckpointGcStarted"]).await.len(),
        1,
        "an idle run leaves no record"
    );

    let after = fx.checkpoints().await;
    let alive = |id: &str| {
        after
            .checkpoints
            .iter()
            .any(|c| c.checkpoint_id == id && matches!(c.status.as_str(), "CURRENT" | "SUPERSEDED"))
    };
    for must in [
        turn_checkpoint(&l, 3).checkpoint_id,
        hand[5].checkpoint_id.clone(),
        hand[20].checkpoint_id.clone(),
        hand[11].checkpoint_id.clone(),
        latest.clone(),
        restored.pre_restore_checkpoint_id.clone(),
    ] {
        assert!(alive(&must), "{must} must survive: {after:?}");
    }
    assert!(!after.checkpoints.is_empty());
    assert!(
        after.checkpoints.iter().any(|c| c.status == "COLLECTED"),
        "{after:?}"
    );
    // Every survivor restores, and what it restores to is what it recorded:
    // the named hand checkpoints hold `hand.txt` at their version.
    for (idx, label) in [(5usize, "hand five"), (20, "hand twenty")] {
        let (r, _) = fx
            .restore(RestoreCheckpoint {
                name: label.into(),
                ..Default::default()
            })
            .await
            .unwrap();
        assert!(r.restored, "{label}: {r:?}");
        assert_eq!(
            std::fs::read_to_string(format!("{}/hand.txt", fx.root)).unwrap(),
            format!("hand-written version {idx}\n").repeat(40),
            "{label} restores what it recorded"
        );
    }
    // The fork taken before the collection still has its worktree and its
    // checkpoint is still restorable.
    assert!(std::path::Path::new(&forked.worktree).exists());
    let again = fx
        .restore(RestoreCheckpoint {
            checkpoint_id: hand[11].checkpoint_id.clone(),
            ..Default::default()
        })
        .await
        .unwrap()
        .0;
    assert!(again.restored, "{again:?}");
    assert_eq!(
        std::fs::read_to_string(format!("{}/hand.txt", fx.root)).unwrap(),
        "hand-written version 11\n".repeat(40)
    );
    // A collected checkpoint is refused, typed.
    let gone = after
        .checkpoints
        .iter()
        .find(|c| c.status == "COLLECTED")
        .unwrap()
        .checkpoint_id
        .clone();
    let refused = fx
        .restore(RestoreCheckpoint {
            checkpoint_id: gone.clone(),
            ..Default::default()
        })
        .await
        .unwrap()
        .0;
    assert_eq!(refused.refusal, "COLLECTED", "{refused:?}");
    let fork_of_gone = send::<_, TaskForked>(
        &mut fx.c,
        None,
        "ForkTask",
        ForkTask {
            task_id: Some(fx.task.clone()),
            checkpoint_id: gone,
            ..Default::default()
        },
        Some(fx.g),
    )
    .await;
    assert_eq!(code_of(fork_of_gone), "COLLECTED");
    // The log says what ran.
    let trail = fx
        .events(&[
            "CheckpointGcStarted",
            "CheckpointCollected",
            "CheckpointGcCompleted",
        ])
        .await;
    assert_eq!(trail.len(), 3, "{trail:?}");
    assert_eq!(trail[2].1["blobs_removed"], json!(report.blobs_removed));
}

/// Failure injection (REQ-PX-102): the Core dies right after the collection
/// took checkpoints out of the restorable set and before a single blob was
/// deleted, and again in the middle of the sweep. The next Core has no broken
/// chain, every survivor restores, and the next run finishes the work.
#[tokio::test]
async fn px_102_kill_mid_collection_leaves_no_broken_chain_and_the_next_run_finishes() {
    for point in ["AFTER_GC_COLLECT", "MID_GC_SWEEP"] {
        let mut fx = many_checkpoints(20).await;
        let l = fx.checkpoints().await;
        let hand: Vec<_> = l
            .checkpoints
            .iter()
            .filter(|c| c.reason.starts_with("hand "))
            .cloned()
            .collect();
        name(&mut fx, &hand[4].checkpoint_id, "keeper")
            .await
            .unwrap();
        let (objects_before, _) = object_files(&fx.data);
        fx.restart(&[("MODBIT_FAULT_CHECKPOINT", point)]).await;
        let died = gc(&mut fx, 1, false).await;
        assert!(died.is_err(), "{point}");
        assert!(
            fx.core.wait_exit(15),
            "{point}: the Core aborted at the fault"
        );
        fx.restart(&[]).await;
        let mid = fx.checkpoints().await;
        assert!(
            mid.checkpoints.iter().any(|c| c.status == "COLLECTED"),
            "{point}: the collection of the restorable set was atomic and is on the log"
        );
        // Every surviving checkpoint restores now, whatever the sweep did.
        let (r, _) = fx
            .restore(RestoreCheckpoint {
                name: "keeper".into(),
                ..Default::default()
            })
            .await
            .unwrap();
        assert!(r.restored, "{point}: {r:?}");
        assert_eq!(
            std::fs::read_to_string(format!("{}/hand.txt", fx.root)).unwrap(),
            "hand-written version 4\n".repeat(40)
        );
        for c in mid
            .checkpoints
            .iter()
            .filter(|c| matches!(c.status.as_str(), "CURRENT" | "SUPERSEDED"))
        {
            let preview = send::<_, modbit_protocol::v1::RewindPreview>(
                &mut fx.c,
                None,
                "PreviewRewind",
                modbit_protocol::v1::PreviewRewind {
                    task_id: Some(fx.task.clone()),
                    checkpoint_id: c.checkpoint_id.clone(),
                    ..Default::default()
                },
                None,
            )
            .await
            .unwrap()
            .0;
            assert_eq!(
                preview.refusal, "",
                "{point}: {} has a broken chain: {preview:?}",
                c.checkpoint_id
            );
        }
        // The next run completes the work: no collected checkpoint's blob is left
        // that nothing names.
        let (finish, _) = gc(&mut fx, 1, false).await.unwrap();
        let (objects_after, _) = object_files(&fx.data);
        assert!(objects_after <= objects_before, "{point}");
        if point == "AFTER_GC_COLLECT" {
            assert!(finish.blobs_removed > 0, "{point}: {finish:?}");
        }
        let (again, _) = gc(&mut fx, 1, false).await.unwrap();
        assert_eq!(
            (again.blobs_removed, again.bytes_freed),
            (0, 0),
            "{point}: {again:?}"
        );
    }
}

/// A second collector run is refused while the first holds the lease, and a
/// restore is refused while a collection is in flight (typed, nothing
/// changed).
#[tokio::test]
async fn px_102_a_second_collector_and_a_restore_are_refused_while_a_collection_holds_its_lease() {
    let mut fx = many_checkpoints(6).await;
    let l = fx.checkpoints().await;
    fx.restart(&[("MODBIT_FAULT_CHECKPOINT_GC_HOLD_MS", "2500")])
        .await;
    let (task, session, g) = (fx.task.clone(), fx.session.clone(), fx.g);
    // The first collection holds its lease for the injected hold.
    let mut holder = fx.core.client().await;
    let first = tokio::spawn(async move {
        send::<_, CheckpointGcReport>(
            &mut holder,
            None,
            "RunCheckpointGc",
            RunCheckpointGc {
                session_id: Some(session),
                task_id: Some(task),
                reason: "first".into(),
                policy: Some(modbit_protocol::v1::CheckpointRetentionPolicy {
                    keep_recent: 1,
                    min_age_ms: 1,
                    max_bytes: 0,
                }),
                dry_run: false,
            },
            Some(g),
        )
        .await
    });
    tokio::time::sleep(Duration::from_millis(800)).await;
    let second = gc(&mut fx, 1, false).await;
    assert_eq!(code_of(second), "GC_LEASE_HELD");
    let during = fx
        .restore(RestoreCheckpoint {
            checkpoint_id: l.checkpoints[0].checkpoint_id.clone(),
            ..Default::default()
        })
        .await
        .unwrap()
        .0;
    assert_eq!(during.refusal, "GC_IN_FLIGHT", "{during:?}");
    let (done, _) = first.await.unwrap().unwrap();
    assert!(done.removed > 0, "{done:?}");
    // And the lease is free again.
    assert!(gc(&mut fx, 1, false).await.is_ok());
}

// ============================================================================
// REQ-PX-101: PauseTask and ResumeTask.
// ============================================================================

async fn pause(
    fx: &mut Fx,
    id: Option<Id>,
    wait_ms: u32,
) -> Result<(TaskPauseResult, bool), ClientError> {
    let (task, g) = (fx.task.clone(), fx.g);
    send(
        &mut fx.c,
        id,
        "PauseTask",
        PauseTask {
            task_id: Some(task),
            reason: "lunch".into(),
            wait_ms,
        },
        Some(g),
    )
    .await
}

async fn resume(
    fx: &mut Fx,
    id: Option<Id>,
    g: Option<u64>,
) -> Result<(TaskResumeResult, bool), ClientError> {
    let task = fx.task.clone();
    let g = g.unwrap_or(fx.g);
    send(
        &mut fx.c,
        id,
        "ResumeTask",
        ResumeTask {
            task_id: Some(task),
            ..Default::default()
        },
        Some(g),
    )
    .await
}

/// QUAL-PX-101 (real Core, scripted model): a pause lands during a streaming
/// turn — the answer says the turn is in flight, the turn finishes, the run
/// parks at the boundary typed `Waiting(Paused)` with a record of who and why;
/// a duplicate pause acts once; the resume continues the same run and
/// completes; nothing the paused run held is lost.
#[tokio::test]
async fn px_101_pause_during_a_streaming_turn_parks_at_the_boundary_and_resume_completes() {
    // Turn 3's model reply is slow: the stream is in flight when the pause arrives.
    let (mut fx, seen) = Fx::new(files_script(4, Some((3, 1500))), &[("a.txt", "a\n")], &[]).await;
    let started = fx.start().await;
    // The request for turn 4 (three tool results in the conversation) has
    // reached the model, whose reply is held for 1.5 s: the stream is in
    // flight when the pause arrives.
    let deadline = std::time::Instant::now() + Duration::from_secs(30);
    while seen.lock().unwrap().len() < 4 {
        assert!(std::time::Instant::now() < deadline);
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    tokio::time::sleep(Duration::from_millis(200)).await;
    let cmd = rand_id();
    let (p, replayed) = pause(&mut fx, Some(cmd.clone()), 0).await.unwrap();
    assert!(!replayed);
    assert_eq!(p.state, "PAUSING", "{p:?}");
    assert!(p.waits_for_boundary && !p.note.is_empty(), "{p:?}");
    // The in-flight turn finished (f3 exists), and the run parked before turn 4.
    let st = fx
        .until("the park", 30, |s| {
            s.wait_reason == "Paused" && !s.loop_alive
        })
        .await;
    assert_eq!(st.state, "Waiting", "{st:?}");
    assert!(std::path::Path::new(&format!("{}/f3.txt", fx.root)).exists());
    assert!(!std::path::Path::new(&format!("{}/f4.txt", fx.root)).exists());
    assert_eq!(st.failure_class, "", "a pause is not a fault: {st:?}");
    // The record: who, why, which checkpoint it holds.
    let paused = fx
        .events(&["TaskPaused", "TaskPauseRequested", "RunSuspended"])
        .await;
    let rec = paused
        .iter()
        .find(|(t, _)| t == "TaskPaused")
        .expect("TaskPaused")
        .1
        .clone();
    assert_eq!(rec["reason"], "lunch");
    assert_eq!(rec["boundary"], "TURN_BOUNDARY");
    assert!(!rec["checkpoint_id"].as_str().unwrap().is_empty(), "{rec}");
    // A duplicate pause (same command id) acts once; a fresh pause on a paused task is a no-op answer.
    let (dup, replayed) = pause(&mut fx, Some(cmd), 0).await.unwrap();
    assert!(replayed, "{dup:?}");
    assert_eq!(
        fx.events(&["TaskPauseRequested"]).await.len(),
        1,
        "the duplicate recorded nothing"
    );
    let (already, _) = pause(&mut fx, None, 0).await.unwrap();
    assert_eq!(already.state, "ALREADY_PAUSED");
    // Resume: the same run continues and completes; a duplicate resume acts once.
    let rid = rand_id();
    let (r, replayed) = resume(&mut fx, Some(rid.clone()), None).await.unwrap();
    assert!(r.resumed && !replayed, "{r:?}");
    assert_eq!(r.run_id, started.run_id, "the same run");
    let (_, replayed) = resume(&mut fx, Some(rid), None).await.unwrap();
    assert!(replayed);
    fx.until("ReadyForReview", 60, |s| s.state == "ReadyForReview")
        .await;
    assert!(std::path::Path::new(&format!("{}/f4.txt", fx.root)).exists());
    // Resuming a task that is not paused is refused, typed.
    assert_eq!(code_of(resume(&mut fx, None, None).await), "NOT_PAUSED");
}

/// QUAL-PX-101: `SIGKILL` the Core while the task is paused. The restarted
/// Core finds the task paused — not running, not failed — with its checkpoint
/// and no lost event; a resume with the stale lease generation is refused and
/// changes nothing; the resume under the new generation completes the task
/// exactly once.
#[tokio::test]
async fn px_101_a_pause_survives_a_killed_core_and_a_stale_resume_is_refused() {
    let (mut fx, _seen) = Fx::new(files_script(4, Some((3, 800))), &[("a.txt", "a\n")], &[]).await;
    fx.start().await;
    fx.until("the run", 30, |s| s.loop_alive).await;
    pause(&mut fx, None, 20_000).await.unwrap();
    let st = fx
        .until("the park", 30, |s| {
            s.wait_reason == "Paused" && !s.loop_alive
        })
        .await;
    let checkpoints_before = fx.checkpoints().await;
    let events_before = fx
        .events(&["TaskPaused", "TaskWaiting", "RunSuspended"])
        .await
        .len();
    let old_generation = fx.g;
    // The real kill, then a new Core on the same profile.
    fx.restart(&[]).await;
    let after = fx.status().await;
    assert_eq!(
        (after.state.as_str(), after.wait_reason.as_str()),
        ("Waiting", "Paused"),
        "{after:?}"
    );
    assert!(!after.loop_alive, "{after:?}");
    assert_eq!(after.failure_class, "");
    assert_eq!(
        st.run_state, after.run_state,
        "the run is as it was: {after:?}"
    );
    let checkpoints_after = fx.checkpoints().await;
    assert_eq!(
        checkpoints_after.current_checkpoint_id, checkpoints_before.current_checkpoint_id,
        "the same checkpoint is held"
    );
    assert_eq!(
        fx.events(&["TaskPaused", "TaskWaiting", "RunSuspended"])
            .await
            .len(),
        events_before,
        "no event was lost or invented by the restart"
    );
    // A second client with the previous generation: refused, nothing changes.
    let stale = resume(&mut fx, None, Some(old_generation)).await;
    assert_eq!(code_of(stale), "STALE_LEASE");
    let still = fx.status().await;
    assert_eq!(
        (still.state.as_str(), still.wait_reason.as_str()),
        ("Waiting", "Paused")
    );
    // The current generation resumes it, and it completes.
    let (r, _) = resume(&mut fx, None, None).await.unwrap();
    assert!(r.resumed, "{r:?}");
    fx.until("ReadyForReview", 90, |s| s.state == "ReadyForReview")
        .await;
    assert_eq!(
        fx.events(&["TaskPaused"]).await.len(),
        1,
        "one pause, one completion"
    );
    for i in 1..=4 {
        assert!(std::path::Path::new(&format!("{}/f{i}.txt", fx.root)).exists());
    }
}

/// QUAL-PX-101: a pause while a real shell command runs waits for the command
/// — it is neither killed nor abandoned — and the command's result reaches the
/// model; a pause with a pending approval leaves the approval pending and
/// answerable, and parks once it is answered.
#[cfg(unix)]
#[tokio::test]
async fn px_101_pause_waits_for_a_running_command_and_leaves_an_approval_pending() {
    let script = vec![
        json!({"calls": [{"name": "plan.update", "args": {"outcome": "run things", "expected_files": ["out.txt", "out2.txt"]}}]}),
        json!({"calls": [{"name": "shell.exec", "args": {"argv": ["sh", "-c", "sleep 2; echo finished-before-park > out.txt; echo finished-before-park"], "inherit_env": true, "timeout_ms": 30000}}]}),
        json!({"calls": [{"name": "shell.exec", "args": {"argv": ["rm", "-r", "doomed"], "inherit_env": true}}]}),
        json!({"calls": [{"name": "change.apply", "args": {"path": "out2.txt", "op": "create", "content": "after\n"}}]}),
        json!({"calls": [{"name": "task.complete", "args": {"summary": "done", "self_review": {"findings": []}}}]}),
    ];
    let (mut fx, seen) = Fx::new(script, &[("a.txt", "a\n")], &[]).await;
    // An untracked directory the approved `rm -r` removes: no tracked file is
    // deleted, so the completion's diff invariants hold.
    std::fs::create_dir_all(format!("{}/doomed", fx.root)).unwrap();
    std::fs::write(format!("{}/doomed/x.txt", fx.root), "x\n").unwrap();
    fx.start().await;
    // The command is running when the pause arrives.
    let deadline = std::time::Instant::now() + Duration::from_secs(30);
    loop {
        let n = seen.lock().unwrap().len();
        if n >= 2 {
            break;
        }
        assert!(std::time::Instant::now() < deadline);
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    tokio::time::sleep(Duration::from_millis(400)).await;
    let (p, _) = pause(&mut fx, None, 0).await.unwrap();
    assert_eq!(p.state, "PAUSING", "{p:?}");
    // The run goes on to the approval of the next call? No: the pause lands at
    // the boundary after the command, before the next turn.
    let st = fx
        .until("the park", 60, |s| {
            s.wait_reason == "Paused" && !s.loop_alive
        })
        .await;
    assert_eq!(st.state, "Waiting");
    assert_eq!(
        std::fs::read_to_string(format!("{}/out.txt", fx.root))
            .unwrap()
            .trim(),
        "finished-before-park",
        "the command ran to its end"
    );
    // Resume: the next turn proposes `rm -r`, which waits for an approval.
    let (_, _) = resume(&mut fx, None, None).await.unwrap();
    let session = fx.session.clone();
    let deadline = std::time::Instant::now() + Duration::from_secs(60);
    let approval = loop {
        let list: ApprovalList = ok(
            &mut fx.c,
            "ListApprovals",
            ListApprovals {
                session_id: Some(session.clone()),
            },
            None,
        )
        .await;
        if let Some(a) = list.approvals.iter().find(|a| a.status == "REQUESTED") {
            break a.clone();
        }
        assert!(
            std::time::Instant::now() < deadline,
            "no approval was requested"
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    };
    // Pause with the approval pending: the approval stays pending.
    tokio::time::sleep(Duration::from_millis(400)).await;
    let (p, _) = pause(&mut fx, None, 0).await.unwrap();
    assert_eq!(p.state, "PAUSING");
    assert!(p.note.contains("pending approval"), "{p:?}");
    let list: ApprovalList = ok(
        &mut fx.c,
        "ListApprovals",
        ListApprovals {
            session_id: Some(session.clone()),
        },
        None,
    )
    .await;
    assert!(
        list.approvals
            .iter()
            .any(|a| a.approval_id == approval.approval_id && a.status == "REQUESTED"),
        "the approval is still pending and answerable: {list:?}"
    );
    let g = fx.g;
    let _: modbit_protocol::v1::ApprovalResolvedAck = ok(
        &mut fx.c,
        "ResolveApproval",
        ResolveApproval {
            approval_id: approval.approval_id.clone(),
            approve: true,
            reason: "ok".into(),
            intent_hash: String::new(),
        },
        Some(g),
    )
    .await;
    // Answered: the call ran, and the run parks at the boundary after it.
    let st = fx
        .until("the park after the answer", 60, |s| {
            s.wait_reason == "Paused" && !s.loop_alive
        })
        .await;
    assert_eq!(st.state, "Waiting");
    assert!(
        !std::path::Path::new(&format!("{}/doomed", fx.root)).exists(),
        "the approved call ran"
    );
    resume(&mut fx, None, None).await.unwrap();
    fx.until("ReadyForReview", 60, |s| s.state == "ReadyForReview")
        .await;
}

/// A pause for a task that is not running is refused, typed; a pause for a
/// finished one too.
#[tokio::test]
async fn px_101_only_a_running_task_pauses() {
    let (mut fx, _seen) = Fx::new(files_script(1, None), &[("a.txt", "a\n")], &[]).await;
    assert_eq!(code_of(pause(&mut fx, None, 0).await), "NOT_PAUSABLE");
    fx.run_to_review().await;
    assert_eq!(code_of(pause(&mut fx, None, 0).await), "NOT_PAUSABLE");
    assert_eq!(code_of(resume(&mut fx, None, None).await), "NOT_PAUSED");
}
