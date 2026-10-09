//! Real-boundary tests for the terminal stream and the background-terminal
//! registry (PX-043, PX-099): the actual `modbit-core` binary spawns the
//! actual `modbit-execd`, which runs real PTY processes; a real
//! SurfaceProtocol client attaches, acknowledges, detaches, reattaches and
//! types; the agent's `shell.input` / `shell.attach` tools go through the
//! Core's registry, classifier and kernel to the same broker.
//!
//! Unix only: the processes are `sh` and `seq` on a PTY and the size
//! observer is `stty`. The Windows ConPTY path is covered by the broker's own
//! suite (`services/modbit-execd/tests`); this file says so on Windows.

#[cfg(not(unix))]
#[test]
fn terminal_stream_core_tests_are_unix_only() {
    eprintln!(
        "SKIPPED terminal_stream: the Core-level terminal suite drives `sh`, `seq` and `stty` on a PTY; the Windows ConPTY path is exercised by services/modbit-execd/tests"
    );
}

#[cfg(unix)]
mod unix {
    use std::io::{BufRead, BufReader};
    use std::process::{Child, Command, Stdio};
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::Duration;

    use modbit_protocol::client::{Client, ClientError};
    use modbit_protocol::local::{ReadyLine, decode_hex};
    use modbit_protocol::v1::terminal_frame::Body as TBody;
    use modbit_protocol::v1::{
        AckTerminal, AcquireSessionLease, AttachTerminal, ClientKind, CommandEnvelope,
        CreateSession, CreateTask, DetachTerminal, GetSessionSnapshot, Id, InvokeTool,
        ListTerminals, ResizeTerminal, SessionCreated, SessionLeaseAcquired, SessionSnapshot,
        TaskCreated, TerminalAttached, TerminalDetached, TerminalExit, TerminalList,
        TerminalResizeDone, TerminalWritten, ToolInvoked, WriteTerminal,
    };
    use prost::Message;
    use serde_json::{Value, json};

    struct CoreProcess {
        child: Child,
        ready: ReadyLine,
    }

    impl CoreProcess {
        fn spawn(data_dir: &std::path::Path, env: &[(&str, &str)]) -> Self {
            let mut child = Command::new(env!("CARGO_BIN_EXE_modbit-core"))
                .arg("--data-dir")
                .arg(data_dir)
                // A broker left by a finished test stops itself soon.
                .env("MODBIT_EXECD_ORPHAN_GRACE_SECS", "4")
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

        fn pid(&self) -> u32 {
            self.child.id()
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

    /// A command id no other command in the test has.
    fn fresh() -> Id {
        static N: AtomicU64 = AtomicU64::new(1);
        let n = N.fetch_add(1, Ordering::SeqCst);
        let mut v = vec![0xE5; 8];
        v.extend_from_slice(&n.to_be_bytes());
        Id { value: v }
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

    fn fenced(mut e: CommandEnvelope, generation: u64) -> CommandEnvelope {
        e.expected_generation = Some(generation);
        e
    }

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
        let g = acquire_lease(c, &session, "test").await;
        (session, g)
    }

    async fn acquire_lease(c: &mut Client, session: &Id, owner: &str) -> u64 {
        let ack = c
            .command(envelope(
                fresh(),
                "AcquireSessionLease",
                AcquireSessionLease {
                    session_id: Some(session.clone()),
                    owner: owner.into(),
                }
                .encode_to_vec(),
            ))
            .await
            .unwrap();
        Client::result::<SessionLeaseAcquired>(&ack)
            .unwrap()
            .lease_generation
    }

    async fn create_task(c: &mut Client, session: &Id, g: u64, root: &str) -> Id {
        let ack = c
            .command(fenced(
                envelope(
                    fresh(),
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
                        ..Default::default()
                    }
                    .encode_to_vec(),
                ),
                g,
            ))
            .await
            .unwrap();
        Client::result::<TaskCreated>(&ack)
            .unwrap()
            .task_id
            .unwrap()
    }

    /// A workspace whose root a task may run commands in.
    fn workspace() -> (tempfile::TempDir, String) {
        let repo = tempfile::tempdir().unwrap();
        std::fs::write(repo.path().join("a.txt"), "a\n").unwrap();
        std::fs::create_dir_all(repo.path().join(".modbit")).unwrap();
        std::fs::write(
            repo.path().join(".modbit").join("verification.json"),
            r#"{"commands":[{"id":"fixture-noop","argv":["git","--version"]}]}"#,
        )
        .unwrap();
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
            .into_owned();
        (repo, root)
    }

    async fn invoke(c: &mut Client, task: &Id, g: u64, tool: &str, args: &Value) -> ToolInvoked {
        let ack = c
            .command(fenced(
                envelope(
                    fresh(),
                    "InvokeTool",
                    InvokeTool {
                        task_id: Some(task.clone()),
                        tool_name: tool.into(),
                        arguments_json: args.to_string(),
                        tool_call_id: Some(fresh()),
                        output_budget_bytes: 65536,
                    }
                    .encode_to_vec(),
                ),
                g,
            ))
            .await
            .unwrap();
        Client::result(&ack).unwrap()
    }

    fn so(r: &ToolInvoked) -> Value {
        serde_json::from_str(&r.structured_output_json).unwrap_or(Value::Null)
    }

    /// A background command on a PTY (`sh -c <script>`), as the agent starts
    /// one; returns its durable handle.
    async fn start_shell(c: &mut Client, task: &Id, g: u64, script: &str) -> String {
        let r = invoke(
            c,
            task,
            g,
            "shell.start",
            &json!({"argv": ["sh", "-c", script], "inherit_env": true, "pty": true, "timeout_ms": 600000}),
        )
        .await;
        assert_eq!(r.status, "SUCCESS", "{r:?}");
        so(&r)["session_id"].as_str().unwrap().to_owned()
    }

    async fn list_terminals(c: &mut Client, task: Option<&Id>) -> TerminalList {
        let ack = c
            .command(envelope(
                fresh(),
                "ListTerminals",
                ListTerminals {
                    task_id: task.cloned(),
                }
                .encode_to_vec(),
            ))
            .await
            .unwrap();
        Client::result(&ack).unwrap()
    }

    #[allow(clippy::too_many_arguments)]
    async fn attach(
        c: &mut Client,
        g: u64,
        task: &Id,
        session_id: &str,
        after: u64,
        window: u64,
        lease: bool,
        steal: bool,
    ) -> Result<TerminalAttached, ClientError> {
        let env = envelope(
            fresh(),
            "AttachTerminal",
            AttachTerminal {
                task_id: Some(task.clone()),
                session_id: session_id.into(),
                after_cursor: after,
                window_bytes: window,
                take_input_lease: lease,
                steal_input_lease: steal,
            }
            .encode_to_vec(),
        );
        let env = if lease { fenced(env, g) } else { env };
        c.command(env)
            .await
            .map(|ack| Client::result(&ack).unwrap())
    }

    async fn ack_cursor(c: &mut Client, attach_id: &str, cursor: u64) {
        c.command(envelope(
            fresh(),
            "AckTerminal",
            AckTerminal {
                attach_id: attach_id.into(),
                cursor,
            }
            .encode_to_vec(),
        ))
        .await
        .unwrap();
    }

    async fn detach(c: &mut Client, attach_id: &str) -> TerminalDetached {
        let ack = c
            .command(envelope(
                fresh(),
                "DetachTerminal",
                DetachTerminal {
                    attach_id: attach_id.into(),
                }
                .encode_to_vec(),
            ))
            .await
            .unwrap();
        Client::result(&ack).unwrap()
    }

    async fn write_terminal(
        c: &mut Client,
        g: u64,
        attach_id: &str,
        data: &[u8],
    ) -> Result<TerminalWritten, ClientError> {
        c.command(fenced(
            envelope(
                fresh(),
                "WriteTerminal",
                WriteTerminal {
                    attach_id: attach_id.into(),
                    data: data.to_vec(),
                }
                .encode_to_vec(),
            ),
            g,
        ))
        .await
        .map(|ack| Client::result(&ack).unwrap())
    }

    fn rejected(r: Result<impl std::fmt::Debug, ClientError>) -> (String, String) {
        match r {
            Err(ClientError::Rejected { code, message }) => (code, message),
            other => panic!("expected a typed rejection, got {other:?}"),
        }
    }

    /// Why a consume loop stopped.
    #[derive(Debug)]
    #[allow(dead_code)] // the fields are what a failing assertion prints
    enum Stop {
        Satisfied,
        Exited(TerminalExit),
        Ended {
            reason: String,
            message: String,
            resume: u64,
            oldest: u64,
        },
    }

    /// Read a terminal stream, checking every frame continues the one before
    /// it (no gap, no overlap), acknowledging as it consumes, until `done`
    /// says the bytes so far are enough, the process exits or the attachment
    /// ends. `cursor` is the cursor after the last consumed byte.
    async fn consume(
        c: &mut Client,
        attach_id: &str,
        cursor: &mut u64,
        bytes: &mut Vec<u8>,
        ack: bool,
        done: impl Fn(&[u8]) -> bool,
    ) -> Stop {
        let step = ack.then_some(0);
        consume_every(c, attach_id, cursor, bytes, step, done).await
    }

    /// As `consume`, acknowledging once `ack_step` bytes are unacknowledged
    /// (a client acknowledges in steps, not per frame); `None` never does.
    async fn consume_every(
        c: &mut Client,
        attach_id: &str,
        cursor: &mut u64,
        bytes: &mut Vec<u8>,
        ack_step: Option<u64>,
        done: impl Fn(&[u8]) -> bool,
    ) -> Stop {
        let mut acked = *cursor;
        let deadline = tokio::time::Instant::now() + Duration::from_secs(120);
        loop {
            if done(bytes) {
                return Stop::Satisfied;
            }
            let f = match tokio::time::timeout_at(deadline, c.next_terminal_frame()).await {
                Ok(f) => f.unwrap().expect("the Core closed the connection"),
                Err(_) => panic!(
                    "the terminal stream stalled at cursor {cursor} after {} bytes",
                    bytes.len()
                ),
            };
            assert_eq!(f.attach_id, attach_id);
            match f.body.expect("a frame has a body") {
                TBody::Output(o) => {
                    assert_eq!(o.cursor, *cursor, "no gap and no duplicate");
                    *cursor += o.data.len() as u64;
                    bytes.extend_from_slice(&o.data);
                    if let Some(step) = ack_step
                        && *cursor - acked >= step
                    {
                        ack_cursor(c, attach_id, *cursor).await;
                        acked = *cursor;
                    }
                }
                TBody::Exited(x) => return Stop::Exited(x),
                TBody::Ended(e) => {
                    return Stop::Ended {
                        reason: e.reason,
                        message: e.message,
                        resume: e.resume_cursor,
                        oldest: e.oldest_cursor,
                    };
                }
                TBody::Lease(_) => {}
            }
        }
    }

    /// What `seq 1 n` prints, line feeds only; a PTY may add carriage returns
    /// of its own to what a process wrote, so the stream is compared to this
    /// with them removed and to the broker's own log byte for byte.
    fn seq_lf(n: u64) -> Vec<u8> {
        (1..=n)
            .flat_map(|i| format!("{i}\n").into_bytes())
            .collect()
    }

    fn without_cr(bytes: &[u8]) -> Vec<u8> {
        bytes.iter().copied().filter(|b| *b != b'\r').collect()
    }

    /// The first `len` bytes of a session's output as the broker's log holds
    /// them, read straight from the broker (the ground truth the stream is
    /// compared with).
    async fn broker_log(data_dir: &std::path::Path, session_id: &str, len: usize) -> Vec<u8> {
        let ready = std::fs::read_to_string(data_dir.join("execd").join("execd.ready")).unwrap();
        let ready = ReadyLine::parse(ready.trim()).unwrap();
        let secret = decode_hex(&ready.boot_secret_hex).unwrap();
        let mut c = modbit_terminal::ExecClient::connect(&ready.endpoint, &secret)
            .await
            .unwrap();
        c.attach(session_id, 0).await.unwrap();
        let mut out = Vec::new();
        while out.len() < len {
            match c.next().await.unwrap().expect("broker closed") {
                modbit_terminal::Event::Output(o) => {
                    assert_eq!(o.cursor, out.len() as u64);
                    out.extend(o.data);
                }
                modbit_terminal::Event::Exited(_) => break,
                _ => {}
            }
        }
        out.truncate(len);
        out
    }

    fn has(bytes: &[u8], needle: &str) -> bool {
        String::from_utf8_lossy(bytes).contains(needle)
    }

    /// Every event of a task, as (type, payload).
    async fn task_events(core: &CoreProcess, session: &Id, task: &Id) -> Vec<(String, Value)> {
        let mut s = core.client().await;
        let floor = {
            let ack = s
                .command(envelope(
                    fresh(),
                    "GetSessionSnapshot",
                    GetSessionSnapshot {
                        session_id: Some(session.clone()),
                    }
                    .encode_to_vec(),
                ))
                .await
                .unwrap();
            Client::result::<SessionSnapshot>(&ack)
                .map(|snap| snap.last_offset)
                .unwrap_or(0)
        };
        s.subscribe(session.clone(), 0).await.unwrap();
        let mut out = Vec::new();
        let mut seen = 0u64;
        loop {
            let wait = if seen < floor {
                Duration::from_secs(30)
            } else {
                Duration::from_millis(400)
            };
            let Ok(Ok(Some(e))) = tokio::time::timeout(wait, s.next_event()).await else {
                break;
            };
            seen = e.offset;
            let ev = e.event.unwrap();
            if ev.task_id.as_ref() == Some(task) {
                let p: Value = serde_json::from_slice(&ev.payload).unwrap_or_default();
                out.push((ev.event_type, p["payload"].clone()));
            }
        }
        out
    }

    fn rss_kib(pid: u32) -> u64 {
        let out = Command::new("ps")
            .args(["-o", "rss=", "-p", &pid.to_string()])
            .output()
            .unwrap();
        String::from_utf8_lossy(&out.stdout)
            .trim()
            .parse()
            .unwrap_or(0)
    }

    /// The pid of the broker serving a data directory.
    fn broker_pid(data_dir: &std::path::Path) -> u32 {
        let needle = data_dir.join("execd").to_string_lossy().into_owned();
        let out = Command::new("ps")
            .args(["-axo", "pid=,command="])
            .output()
            .unwrap();
        String::from_utf8_lossy(&out.stdout)
            .lines()
            .find(|l| l.contains("modbit-execd") && l.contains(&needle))
            .and_then(|l| l.split_whitespace().next()?.parse().ok())
            .expect("the broker of this data directory is running")
    }

    // ---------------------------------------------------------------------

    /// PX-043: the registry lists every task's terminals the same for every
    /// client — owner task, state, start, exit, the running timer, the replay
    /// window — and a filter narrows it to one task.
    #[tokio::test]
    async fn px_043_the_registry_lists_every_terminal_with_owner_state_and_timer() {
        let (_repo, root) = workspace();
        let dir = tempfile::tempdir().unwrap();
        let core = CoreProcess::spawn(dir.path(), &[]);
        let mut c = core.client().await;
        let (session, g) = create_session(&mut c, 0x10).await;
        let task_a = create_task(&mut c, &session, g, &root).await;
        let task_b = create_task(&mut c, &session, g, &root).await;
        let running = start_shell(&mut c, &task_a, g, "echo a-out; sleep 30").await;
        let r = invoke(
            &mut c,
            &task_b,
            g,
            "shell.start",
            &json!({"argv": ["sh", "-c", "echo b-out; exit 3"], "inherit_env": true}),
        )
        .await;
        assert_eq!(r.status, "SUCCESS", "{r:?}");
        let finished = so(&r)["session_id"].as_str().unwrap().to_owned();
        // Wait for B's command to end.
        let list = loop {
            let l = list_terminals(&mut c, None).await;
            if l.terminals
                .iter()
                .any(|t| t.session_id == finished && t.state != "RUNNING")
            {
                break l;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        };
        assert_eq!(list.terminals.len(), 2, "{list:?}");
        let a = list
            .terminals
            .iter()
            .find(|t| t.session_id == running)
            .unwrap();
        let b = list
            .terminals
            .iter()
            .find(|t| t.session_id == finished)
            .unwrap();
        assert_eq!(a.task_id.as_ref(), Some(&task_a));
        assert_eq!(b.task_id.as_ref(), Some(&task_b));
        assert_eq!((a.owner.as_str(), a.state.as_str()), ("agent", "RUNNING"));
        assert_eq!((b.state.as_str(), b.exit_code), ("EXITED", Some(3)));
        assert!(a.pty && !b.pty);
        assert!(a.title.contains("sleep 30"), "{}", a.title);
        assert!(a.started_at_ms > 0 && list.now_ms >= a.started_at_ms);
        assert!(a.replay_window_bytes > 0 && a.oldest_cursor == 0);
        assert_eq!(b.output_ref.len(), 64);
        assert_eq!(a.input_lease_holder, "");
        // The running timer runs; the finished one is its duration.
        tokio::time::sleep(Duration::from_millis(500)).await;
        let later = list_terminals(&mut c, None).await;
        let a2 = later
            .terminals
            .iter()
            .find(|t| t.session_id == running)
            .unwrap();
        assert!(
            a2.elapsed_ms >= a.elapsed_ms + 400,
            "{} -> {}",
            a.elapsed_ms,
            a2.elapsed_ms
        );
        let b2 = later
            .terminals
            .iter()
            .find(|t| t.session_id == finished)
            .unwrap();
        assert_eq!(
            b2.elapsed_ms, b.elapsed_ms,
            "an ended terminal's timer stopped"
        );
        // Every client reads the same registry; the filter narrows it.
        let mut other = core.client().await;
        let theirs = list_terminals(&mut other, None).await;
        let mut ours: Vec<&str> = later
            .terminals
            .iter()
            .map(|t| t.session_id.as_str())
            .collect();
        let mut seen: Vec<&str> = theirs
            .terminals
            .iter()
            .map(|t| t.session_id.as_str())
            .collect();
        ours.sort_unstable();
        seen.sort_unstable();
        assert_eq!(ours, seen);
        let only_b = list_terminals(&mut other, Some(&task_b)).await;
        assert_eq!(only_b.terminals.len(), 1);
        assert_eq!(only_b.terminals[0].session_id, finished);
        // Running first, as a tray shows them.
        assert_eq!(later.terminals[0].session_id, running);
    }

    /// PX-043 / PX-099: attach from a cursor, consume under an
    /// acknowledgement window smaller than the output, detach, reattach from
    /// the acknowledged cursor: what was read is the process's output, byte for
    /// byte, once.
    #[tokio::test]
    async fn px_043_attach_detach_reattach_resumes_from_the_cursor_byte_exactly() {
        let (_repo, root) = workspace();
        let dir = tempfile::tempdir().unwrap();
        let core = CoreProcess::spawn(dir.path(), &[]);
        let mut c = core.client().await;
        let (session, g) = create_session(&mut c, 0x20).await;
        let task = create_task(&mut c, &session, g, &root).await;
        let sid = start_shell(&mut c, &task, g, "seq 1 20000; sleep 30").await;
        // A window (4 KiB, the least) far under the 108 KB the command prints.
        let a1 = attach(&mut c, g, &task, &sid, 0, 4096, false, false)
            .await
            .unwrap();
        assert_eq!(a1.window_bytes, 4096);
        assert!(!a1.input_lease_held);
        let t = a1.terminal.as_ref().unwrap();
        assert_eq!(t.session_id, sid);
        assert!(
            t.replay_window_bytes > 0,
            "the replay-window policy is surfaced"
        );
        let (mut cursor, mut first) = (0u64, Vec::new());
        let stop = consume(&mut c, &a1.attach_id, &mut cursor, &mut first, true, |b| {
            b.len() >= 30_000
        })
        .await;
        assert!(matches!(stop, Stop::Satisfied), "{stop:?}");
        let d = detach(&mut c, &a1.attach_id).await;
        assert!(d.was_attached);
        assert_eq!(
            d.acked_cursor, cursor,
            "the Core resumes exactly where the client was"
        );
        // Reattach, from a client that is not the same connection.
        let mut c2 = core.client().await;
        let a2 = attach(&mut c2, g, &task, &sid, cursor, 4096, false, false)
            .await
            .unwrap();
        let mut second = Vec::new();
        let mut cursor2 = cursor;
        let stop = consume(
            &mut c2,
            &a2.attach_id,
            &mut cursor2,
            &mut second,
            true,
            |b| b.ends_with(b"20000\r\n"),
        )
        .await;
        assert!(matches!(stop, Stop::Satisfied), "{stop:?}");
        let mut all = first;
        all.extend(second);
        assert!(
            all == broker_log(dir.path(), &sid, all.len()).await,
            "the two attachments together are the broker's output, byte for byte"
        );
        assert!(
            without_cr(&all) == seq_lf(20000),
            "and it is what the command printed, once, in order"
        );
        // A cursor nobody consumed to is not a place to resume: past the
        // head the answer is typed.
        let (code, message) = rejected(
            attach(
                &mut c2,
                g,
                &task,
                &sid,
                cursor2 + 1_000_000,
                4096,
                false,
                false,
            )
            .await,
        );
        assert_eq!(code, "CURSOR_BEYOND_HEAD", "{message}");
        assert!(message.contains(&format!("head={cursor2}")), "{message}");
    }

    /// PX-043: a cursor older than the replay window is the typed
    /// CURSOR_EXPIRED naming where the window starts; the window's own start
    /// attaches.
    #[tokio::test]
    async fn px_043_an_expired_cursor_is_typed_and_the_window_start_attaches() {
        let (_repo, root) = workspace();
        let dir = tempfile::tempdir().unwrap();
        let core = CoreProcess::spawn(
            dir.path(),
            &[
                ("MODBIT_EXECD_REPLAY_WINDOW_BYTES", "65536"),
                ("MODBIT_EXECD_SEGMENT_BYTES", "65536"),
            ],
        );
        let mut c = core.client().await;
        let (session, g) = create_session(&mut c, 0x30).await;
        let task = create_task(&mut c, &session, g, &root).await;
        let sid = start_shell(&mut c, &task, g, "seq 1 100000; sleep 30").await;
        // Wait until the output has outrun the window and has stopped: the
        // last segment rolls the window's start a few hundred bytes before
        // the command ends, and a start sampled before that roll is expired
        // by the time it is used.
        let mut last = 0;
        let view = loop {
            let l = list_terminals(&mut c, Some(&task)).await;
            let t = l.terminals[0].clone();
            if t.oldest_cursor > 0 && t.bytes_so_far >= 588_000 && t.bytes_so_far == last {
                break t;
            }
            last = t.bytes_so_far;
            tokio::time::sleep(Duration::from_millis(150)).await;
        };
        assert_eq!(view.replay_window_bytes, 65536);
        let (code, message) = rejected(attach(&mut c, g, &task, &sid, 0, 0, false, false).await);
        assert_eq!(code, "CURSOR_EXPIRED");
        assert!(
            message.contains(&format!("oldest_cursor={}", view.oldest_cursor)),
            "{message}"
        );
        let a = attach(&mut c, g, &task, &sid, view.oldest_cursor, 0, false, false)
            .await
            .unwrap();
        let (mut cursor, mut bytes) = (view.oldest_cursor, Vec::new());
        let stop = consume(&mut c, &a.attach_id, &mut cursor, &mut bytes, true, |b| {
            b.ends_with(b"100000\r\n")
        })
        .await;
        assert!(matches!(stop, Stop::Satisfied), "{stop:?}");
        assert_eq!(cursor, view.bytes_so_far.max(cursor));
    }

    /// PX-043 / PX-099: a terminal belongs to the task that started it. A
    /// client naming another task, and that other task's own tools, are
    /// refused SESSION_NOT_OWNED and are given nothing.
    #[tokio::test]
    async fn px_099_a_second_task_cannot_read_input_or_attach_to_anothers_terminal() {
        let (_repo, root) = workspace();
        let dir = tempfile::tempdir().unwrap();
        let core = CoreProcess::spawn(dir.path(), &[]);
        let mut c = core.client().await;
        let (session, g) = create_session(&mut c, 0x40).await;
        let task_a = create_task(&mut c, &session, g, &root).await;
        let task_b = create_task(&mut c, &session, g, &root).await;
        let sid = start_shell(
            &mut c,
            &task_a,
            g,
            "echo a-secret-output; while read l; do echo \"got:$l\"; done",
        )
        .await;
        // The owner reads it; a client naming the wrong task does not.
        let a = attach(&mut c, g, &task_a, &sid, 0, 0, false, false)
            .await
            .unwrap();
        let (mut cursor, mut bytes) = (0, Vec::new());
        consume(&mut c, &a.attach_id, &mut cursor, &mut bytes, true, |b| {
            has(b, "a-secret-output")
        })
        .await;
        let (code, _) = rejected(attach(&mut c, g, &task_b, &sid, 0, 0, false, false).await);
        assert_eq!(code, "SESSION_NOT_OWNED");
        let (code, _) = rejected(attach(&mut c, g, &task_b, &sid, 0, 0, true, false).await);
        assert_eq!(code, "SESSION_NOT_OWNED");
        // Task B's own tools: nothing is read, written or listed.
        for (tool, args) in [
            (
                "shell.attach",
                json!({"session_id": sid, "after_cursor": 0}),
            ),
            (
                "shell.input",
                json!({"session_id": sid, "text": "echo owned"}),
            ),
            ("shell.read", json!({"session_id": sid})),
            ("shell.cancel", json!({"session_id": sid})),
        ] {
            let r = invoke(&mut c, &task_b, g, tool, &args).await;
            assert_eq!(r.status, "APPLICATION_FAILURE", "{tool}: {r:?}");
            assert_eq!(r.error_code, "SESSION_NOT_OWNED", "{tool}: {r:?}");
            assert!(
                !r.structured_output_json.contains("a-secret-output"),
                "{tool} gave task B the other task's output"
            );
        }
        let r = invoke(&mut c, &task_b, g, "shell.list", &json!({})).await;
        assert_eq!(so(&r)["sessions"].as_array().unwrap().len(), 0);
        // Task B's attempts wrote nothing into A's shell.
        let r = invoke(
            &mut c,
            &task_a,
            g,
            "shell.input",
            &json!({"session_id": sid, "text": "from-a"}),
        )
        .await;
        assert_eq!(r.status, "SUCCESS", "{r:?}");
        consume(&mut c, &a.attach_id, &mut cursor, &mut bytes, true, |b| {
            has(b, "got:from-a")
        })
        .await;
        assert!(
            !has(&bytes, "owned"),
            "{:?}",
            String::from_utf8_lossy(&bytes)
        );
    }

    /// PX-099: the agent answers an interactive prompt through `shell.input`
    /// and reads the reply through `shell.attach` from a cursor; typing is
    /// classified like running (a privileged command needs an approval and
    /// writes nothing until it has one); a value this Core holds in custody
    /// is never typed and appears in no event, log or artifact.
    #[tokio::test]
    async fn px_099_the_agent_answers_a_prompt_through_shell_input_and_attach() {
        const KEY: &str = "sk-test-px099-secret-never-typed-0001";
        let (_repo, root) = workspace();
        let dir = tempfile::tempdir().unwrap();
        let core = CoreProcess::spawn(
            dir.path(),
            &[
                ("OPENAI_API_KEY", KEY),
                ("MODBIT_OPENAI_BASE_URL", "http://127.0.0.1:9/v1"),
            ],
        );
        let mut c = core.client().await;
        let (session, g) = create_session(&mut c, 0x50).await;
        let task = create_task(&mut c, &session, g, &root).await;
        let sid = start_shell(
            &mut c,
            &task,
            g,
            "printf 'name? '; read n; echo \"hello $n\"; while read l; do echo \"got:$l\"; done",
        )
        .await;
        // The prompt, read live (no cursor: the tail).
        let r = invoke(
            &mut c,
            &task,
            g,
            "shell.attach",
            &json!({"session_id": sid, "wait_ms": 5000, "quiet_ms": 300}),
        )
        .await;
        assert_eq!(r.status, "SUCCESS", "{r:?}");
        let out = so(&r);
        assert!(out["preview"].as_str().unwrap().contains("name?"), "{out}");
        assert_eq!(out["running"], true);
        assert_eq!(out["input_lease_holder"], "");
        assert_eq!(
            (out["pty_rows"].as_u64(), out["pty_cols"].as_u64()),
            (Some(40), Some(120))
        );
        // Answer it; read the reply from where the input landed.
        let r = invoke(
            &mut c,
            &task,
            g,
            "shell.input",
            &json!({"session_id": sid, "text": "ada"}),
        )
        .await;
        assert_eq!(r.status, "SUCCESS", "{r:?}");
        let input = so(&r);
        assert_eq!(input["bytes_written"], 4);
        let cursor = input["output_cursor"].as_u64().unwrap();
        let r = invoke(
            &mut c,
            &task,
            g,
            "shell.attach",
            &json!({"session_id": sid, "after_cursor": cursor, "wait_ms": 5000, "quiet_ms": 300}),
        )
        .await;
        assert_eq!(r.status, "SUCCESS", "{r:?}");
        let reply = so(&r);
        assert!(
            reply["preview"].as_str().unwrap().contains("hello ada"),
            "{reply}"
        );
        assert!(reply["next_cursor"].as_u64().unwrap() > cursor);
        // A cursor past the output is typed, never a silent empty read.
        let r = invoke(
            &mut c,
            &task,
            g,
            "shell.attach",
            &json!({"session_id": sid, "after_cursor": 99_999_999}),
        )
        .await;
        assert_eq!(r.error_code, "CURSOR_BEYOND_HEAD", "{r:?}");
        // Typing is running: a privileged command is not typed on the agent's say.
        let r = invoke(
            &mut c,
            &task,
            g,
            "shell.input",
            &json!({"session_id": sid, "text": "sudo whoami"}),
        )
        .await;
        assert_eq!(r.status, "APPROVAL_PENDING", "{r:?}");
        // Control characters and over-long text are refused before the broker.
        let r = invoke(
            &mut c,
            &task,
            g,
            "shell.input",
            &json!({"session_id": sid, "text": "\u{1b}[A"}),
        )
        .await;
        assert!(
            r.status == "INVALID_ARGUMENTS" || r.error_code == "CONTROL_CHARACTER_REFUSED",
            "{r:?}"
        );
        let r = invoke(
            &mut c,
            &task,
            g,
            "shell.input",
            &json!({"session_id": sid, "text": "a".repeat(4097)}),
        )
        .await;
        assert!(
            r.status == "INVALID_ARGUMENTS" || r.error_code == "INPUT_TOO_LARGE",
            "{r:?}"
        );
        // A credential in custody is not typed: refused, recorded, nowhere.
        let r = invoke(
            &mut c,
            &task,
            g,
            "shell.input",
            &json!({"session_id": sid, "text": format!("echo {KEY}")}),
        )
        .await;
        assert_eq!(r.error_code, "SECRET_EXFILTRATION_BLOCKED", "{r:?}");
        // Nothing but the answer reached the shell.
        let r = invoke(
            &mut c,
            &task,
            g,
            "shell.attach",
            &json!({"session_id": sid, "after_cursor": 0, "wait_ms": 500, "quiet_ms": 200}),
        )
        .await;
        let shown = so(&r)["preview"].as_str().unwrap().to_owned();
        assert!(!shown.contains("whoami") && !shown.contains(KEY), "{shown}");
        // The key is in no event, no broker log and no sealed object.
        let r = invoke(
            &mut c,
            &task,
            g,
            "shell.cancel",
            &json!({"session_id": sid}),
        )
        .await;
        assert_eq!(r.status, "SUCCESS", "{r:?}");
        for (_, payload) in task_events(&core, &session, &task).await {
            assert!(!payload.to_string().contains(KEY), "{payload}");
        }
        let mut stack = vec![dir.path().to_owned()];
        while let Some(p) = stack.pop() {
            for e in std::fs::read_dir(&p).unwrap().flatten() {
                let path = e.path();
                if path.is_dir() {
                    stack.push(path);
                } else if let Ok(bytes) = std::fs::read(&path) {
                    assert!(
                        !bytes.windows(KEY.len()).any(|w| w == KEY.as_bytes()),
                        "the credential is in {}",
                        path.display()
                    );
                }
            }
        }
    }

    /// PX-099: the person's input lease. A user attach takes it; the agent's
    /// `shell.input` is refused INPUT_LEASED and nothing is typed; the person
    /// types and is answered; another person cannot type or take it; letting
    /// go (detach) gives it back; the task's log records the lease, the size
    /// and the count of keystrokes but never the keystrokes.
    #[tokio::test]
    async fn px_099_the_user_input_lease_blocks_agent_input_and_is_journaled_without_content() {
        let (_repo, root) = workspace();
        let dir = tempfile::tempdir().unwrap();
        let core = CoreProcess::spawn(dir.path(), &[]);
        let mut c = core.client().await;
        let (session, g) = create_session(&mut c, 0x60).await;
        let task = create_task(&mut c, &session, g, &root).await;
        let sid = start_shell(
            &mut c,
            &task,
            g,
            "stty size; while read l; do echo \"got:$l\"; stty size; done",
        )
        .await;
        let agent_input = |text: &'static str| json!({"session_id": sid.clone(), "text": text});
        let r = invoke(&mut c, &task, g, "shell.input", &agent_input("before")).await;
        assert_eq!(r.status, "SUCCESS", "{r:?}");
        // The person attaches and takes the lease.
        let mut user = core.client().await;
        let a = attach(&mut user, g, &task, &sid, 0, 0, true, false)
            .await
            .unwrap();
        assert!(a.input_lease_held);
        assert!(
            a.terminal
                .as_ref()
                .unwrap()
                .input_lease_holder
                .starts_with("user:")
        );
        let listed = list_terminals(&mut c, Some(&task)).await;
        assert!(listed.terminals[0].input_lease_holder.starts_with("user:"));
        let (mut cursor, mut seen) = (0u64, Vec::new());
        consume(&mut user, &a.attach_id, &mut cursor, &mut seen, true, |b| {
            has(b, "got:before")
        })
        .await;
        // The agent is refused with the typed error and writes nothing.
        let r = invoke(&mut c, &task, g, "shell.input", &agent_input("agent-typed")).await;
        assert_eq!(r.status, "APPLICATION_FAILURE", "{r:?}");
        assert_eq!(r.error_code, "INPUT_LEASED", "{r:?}");
        assert!(r.error_message.contains("user:"), "{r:?}");
        // It can still read, and is told who holds the lease.
        let r = invoke(
            &mut c,
            &task,
            g,
            "shell.attach",
            &json!({"session_id": sid, "wait_ms": 300}),
        )
        .await;
        assert!(
            so(&r)["input_lease_holder"]
                .as_str()
                .unwrap()
                .starts_with("user:")
        );
        // The person types; the shell answers; the bytes are the person's.
        let w = write_terminal(&mut user, g, &a.attach_id, b"from-user\n")
            .await
            .unwrap();
        assert_eq!(w.bytes, 10);
        consume(&mut user, &a.attach_id, &mut cursor, &mut seen, true, |b| {
            has(b, "got:from-user")
        })
        .await;
        assert!(
            !has(&seen, "agent-typed"),
            "{:?}",
            String::from_utf8_lossy(&seen)
        );
        // Another person watches but cannot type, and cannot take the lease.
        let mut other = core.client().await;
        let b = attach(&mut other, g, &task, &sid, 0, 0, false, false)
            .await
            .unwrap();
        assert!(!b.input_lease_held);
        let (code, _) = rejected(write_terminal(&mut other, g, &b.attach_id, b"intruder\n").await);
        assert_eq!(code, "LEASE_REQUIRED");
        let (code, message) = rejected(attach(&mut other, g, &task, &sid, 0, 0, true, false).await);
        assert_eq!(code, "LEASE_HELD", "{message}");
        // The person lets go by detaching; the agent types again.
        let d = detach(&mut user, &a.attach_id).await;
        assert!(d.was_attached);
        let mut typed = false;
        for _ in 0..100 {
            let r = invoke(&mut c, &task, g, "shell.input", &agent_input("after")).await;
            if r.status == "SUCCESS" {
                typed = true;
                break;
            }
            assert_eq!(r.error_code, "INPUT_LEASED", "{r:?}");
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        assert!(typed, "the lease outlived the person's attachment");
        // The journal: lease taken, bytes counted, lease released — no content.
        let events = task_events(&core, &session, &task).await;
        let control: Vec<&Value> = events
            .iter()
            .filter(|(t, _)| t == "TerminalControlRecorded")
            .map(|(_, p)| p)
            .collect();
        let kinds: Vec<&str> = control
            .iter()
            .map(|p| p["kind"].as_str().unwrap())
            .collect();
        assert_eq!(
            kinds,
            ["INPUT_LEASE_TAKEN", "INPUT_WRITTEN", "INPUT_LEASE_RELEASED"],
            "{control:?}"
        );
        assert_eq!(control[1]["bytes"], 10);
        assert!(
            events
                .iter()
                .all(|(_, p)| !p.to_string().contains("from-user")),
            "the person's keystrokes are not in the log"
        );
    }

    /// PX-099: a resize through the stream is applied to the real PTY — the
    /// child reads the new size from the terminal — and is recorded on the
    /// task; a bad size is typed and changes nothing.
    #[tokio::test]
    async fn px_099_resize_through_the_stream_reaches_the_child_and_is_recorded() {
        let (_repo, root) = workspace();
        let dir = tempfile::tempdir().unwrap();
        let core = CoreProcess::spawn(dir.path(), &[]);
        let mut c = core.client().await;
        let (session, g) = create_session(&mut c, 0x70).await;
        let task = create_task(&mut c, &session, g, &root).await;
        let sid = start_shell(
            &mut c,
            &task,
            g,
            "echo size=$(stty size); while read l; do echo size=$(stty size); done",
        )
        .await;
        let a = attach(&mut c, g, &task, &sid, 0, 0, false, false)
            .await
            .unwrap();
        let (mut cursor, mut seen) = (0u64, Vec::new());
        consume(&mut c, &a.attach_id, &mut cursor, &mut seen, true, |b| {
            has(b, "size=40 120")
        })
        .await;
        let resize = |rows: u32, cols: u32| {
            fenced(
                envelope(
                    fresh(),
                    "ResizeTerminal",
                    ResizeTerminal {
                        attach_id: a.attach_id.clone(),
                        rows,
                        cols,
                    }
                    .encode_to_vec(),
                ),
                g,
            )
        };
        let done: TerminalResizeDone =
            Client::result(&c.command(resize(50, 132)).await.unwrap()).unwrap();
        assert_eq!((done.rows, done.cols), (50, 132));
        let r = invoke(
            &mut c,
            &task,
            g,
            "shell.input",
            &json!({"session_id": sid, "text": "go"}),
        )
        .await;
        assert_eq!(r.status, "SUCCESS", "{r:?}");
        consume(&mut c, &a.attach_id, &mut cursor, &mut seen, true, |b| {
            has(b, "size=50 132")
        })
        .await;
        let listed = list_terminals(&mut c, Some(&task)).await;
        assert_eq!(
            (listed.terminals[0].rows, listed.terminals[0].cols),
            (50, 132)
        );
        // A bad size is a typed refusal.
        let (code, _) = rejected(c.command(resize(0, 80)).await);
        assert_eq!(code, "BAD_SIZE");
        let (code, _) = rejected(c.command(resize(24, 5000)).await);
        assert_eq!(code, "BAD_SIZE");
        let events = task_events(&core, &session, &task).await;
        let resized: Vec<&Value> = events
            .iter()
            .filter(|(t, p)| t == "TerminalControlRecorded" && p["kind"] == "RESIZED")
            .map(|(_, p)| p)
            .collect();
        assert_eq!(resized.len(), 1, "{resized:?}");
        assert_eq!(
            (resized[0]["rows"].as_u64(), resized[0]["cols"].as_u64()),
            (Some(50), Some(132))
        );
    }

    /// PX-099 failure injection: the Core is killed (SIGKILL) while a client
    /// is attached mid-stream and holds the input lease. The process keeps
    /// running in the broker, the lease ends with the Core's connection, the
    /// restarted Core reattaches the client at its stored cursor, and the two
    /// halves together are the output, exactly.
    #[tokio::test]
    async fn px_099_killing_the_core_mid_stream_leaves_the_shell_running_and_reattach_resumes() {
        let (_repo, root) = workspace();
        let dir = tempfile::tempdir().unwrap();
        let mut core = CoreProcess::spawn(dir.path(), &[]);
        let mut c = core.client().await;
        let (session, g) = create_session(&mut c, 0x80).await;
        let task = create_task(&mut c, &session, g, &root).await;
        let n = 150_000u64;
        let sid = start_shell(
            &mut c,
            &task,
            g,
            &format!("seq -f %.0f 1 {n}; while read l; do echo \"got:$l\"; done"),
        )
        .await;
        let a = attach(&mut c, g, &task, &sid, 0, 16 * 1024, true, false)
            .await
            .unwrap();
        let (mut cursor, mut first) = (0u64, Vec::new());
        let stop = consume_every(
            &mut c,
            &a.attach_id,
            &mut cursor,
            &mut first,
            Some(4096),
            |b| b.len() >= 200_000,
        )
        .await;
        assert!(matches!(stop, Stop::Satisfied), "{stop:?}");
        let stored = cursor;
        // Kill the Core outright, mid-stream.
        let _ = core.child.kill();
        let _ = core.child.wait();
        drop(c);
        // The restarted Core finds the broker (and the shell) where they were.
        let core2 = CoreProcess::spawn(dir.path(), &[]);
        let mut c2 = core2.client().await;
        let listed = list_terminals(&mut c2, Some(&task)).await;
        assert_eq!(listed.terminals.len(), 1, "{listed:?}");
        assert_eq!(
            listed.terminals[0].state, "RUNNING",
            "the shell survived the Core"
        );
        assert_eq!(
            listed.terminals[0].input_lease_holder, "",
            "the lease ended with the dead Core's connection"
        );
        let g2 = acquire_lease(&mut c2, &session, "test-2").await;
        // Reattach at the stored cursor: the rest of the stream, exactly.
        let a2 = attach(&mut c2, g2, &task, &sid, stored, 16 * 1024, false, false)
            .await
            .unwrap();
        let mut rest_cursor = stored;
        let mut rest = Vec::new();
        // Read to the end of what the command prints before anything is
        // typed: the terminal echoes typed text into the stream wherever the
        // output has got to, and how far that is depends on the pace.
        let last = n.to_string();
        let stop = consume_every(
            &mut c2,
            &a2.attach_id,
            &mut rest_cursor,
            &mut rest,
            Some(4096),
            |b| has(&b[b.len().saturating_sub(24)..], &last),
        )
        .await;
        assert!(matches!(stop, Stop::Satisfied), "{stop:?}");
        // The agent types again at once: nobody holds the lease any more.
        let r = invoke(
            &mut c2,
            &task,
            g2,
            "shell.input",
            &json!({"session_id": sid, "text": "after-restart"}),
        )
        .await;
        assert_eq!(r.status, "SUCCESS", "{r:?}");
        let stop = consume_every(
            &mut c2,
            &a2.attach_id,
            &mut rest_cursor,
            &mut rest,
            Some(4096),
            |b| has(b, "got:after-restart"),
        )
        .await;
        assert!(matches!(stop, Stop::Satisfied), "{stop:?}");
        let mut all = first;
        all.extend(rest);
        let printed = seq_lf(n);
        let seen = without_cr(&all);
        let at = seen.iter().zip(&printed).position(|(a, b)| a != b);
        assert!(
            seen.starts_with(&printed),
            "the stream across the Core's death is the output, once and in order: read {} bytes ({} without CR), expected prefix {}, first difference at {at:?}, around {:02x?} vs {:02x?}",
            all.len(),
            seen.len(),
            printed.len(),
            at.map(|i| &seen[i.saturating_sub(16)..(i + 16).min(seen.len())]),
            at.map(|i| &printed[i.saturating_sub(16)..(i + 16).min(printed.len())]),
        );
        // The bytes themselves are the broker's, with nothing missing or doubled.
        assert!(all == broker_log(dir.path(), &sid, all.len()).await);
        // End the shell.
        let r = invoke(
            &mut c2,
            &task,
            g2,
            "shell.cancel",
            &json!({"session_id": sid}),
        )
        .await;
        assert_eq!(r.status, "SUCCESS", "{r:?}");
    }

    /// PX-099: a client that attaches and never reads or acknowledges cannot
    /// grow the Core's or the broker's memory while a process writes ~44 MB;
    /// it is dropped to cursor-pull (SLOW_CONSUMER, naming the cursor), and
    /// then catches up from its own cursor with no gap and no duplicate.
    #[tokio::test]
    async fn px_099_a_slow_client_does_not_grow_the_core_or_the_broker_and_catches_up() {
        let (_repo, root) = workspace();
        let dir = tempfile::tempdir().unwrap();
        let core = CoreProcess::spawn(dir.path(), &[("MODBIT_TERMINAL_STALL_MS", "1500")]);
        let mut c = core.client().await;
        let (session, g) = create_session(&mut c, 0x90).await;
        let task = create_task(&mut c, &session, g, &root).await;
        let n = 3_000_000u64;
        let t0 = std::time::Instant::now();
        let sid = start_shell(&mut c, &task, g, &format!("seq -f %.0f 1 {n}; sleep 300")).await;
        let broker = broker_pid(dir.path());
        let (core_base, broker_base) = (rss_kib(core.pid()), rss_kib(broker));
        // The slow client: attached with a 64 KiB window, then it neither
        // reads its socket nor acknowledges.
        let mut slow = core.client().await;
        let a = attach(&mut slow, g, &task, &sid, 0, 64 * 1024, false, false)
            .await
            .unwrap();
        let (mut core_peak, mut broker_peak) = (core_base, broker_base);
        loop {
            core_peak = core_peak.max(rss_kib(core.pid()));
            broker_peak = broker_peak.max(rss_kib(broker));
            let l = list_terminals(&mut c, Some(&task)).await;
            if l.terminals[0].bytes_so_far >= 20_000_000 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        {
            let v = list_terminals(&mut c, Some(&task)).await.terminals[0].clone();
            eprintln!(
                "px_099 slow client: head {} oldest {} window {}",
                v.bytes_so_far, v.oldest_cursor, v.replay_window_bytes
            );
        }
        // Past the stall time with the window still full.
        tokio::time::sleep(Duration::from_millis(2500)).await;
        core_peak = core_peak.max(rss_kib(core.pid()));
        broker_peak = broker_peak.max(rss_kib(broker));
        eprintln!(
            "px_099 slow client: Core RSS {core_base} -> {core_peak} KiB, broker RSS {broker_base} -> {broker_peak} KiB over ~44 MB of output"
        );
        assert!(
            core_peak < core_base + 48 * 1024,
            "Core RSS grew from {core_base} to {core_peak} KiB"
        );
        assert!(
            broker_peak < broker_base + 48 * 1024,
            "broker RSS grew from {broker_base} to {broker_peak} KiB"
        );
        {
            let v = list_terminals(&mut c, Some(&task)).await.terminals[0].clone();
            eprintln!(
                "px_099 slow client (after): head {} oldest {}",
                v.bytes_so_far, v.oldest_cursor
            );
        }
        // Now it reads: what was in flight (about the window), then the
        // typed drop naming where to resume.
        let (mut cursor, mut got) = (0u64, Vec::new());
        let stop = consume(
            &mut slow,
            &a.attach_id,
            &mut cursor,
            &mut got,
            false,
            |_| false,
        )
        .await;
        let Stop::Ended {
            reason,
            resume,
            message,
            ..
        } = stop
        else {
            panic!("expected the drop to cursor-pull, got {stop:?}");
        };
        assert_eq!(reason, "SLOW_CONSUMER", "{message}");
        assert_eq!(resume, 0, "nothing was acknowledged");
        assert!(
            cursor <= 64 * 1024 + 64 * 1024 + 64 * 1024,
            "{cursor} bytes were in flight against a 64 KiB window"
        );
        eprintln!(
            "px_099 slow client: dropped to cursor-pull at {:?}",
            t0.elapsed()
        );
        // Cursor-pull: from the cursor it consumed to, acknowledging as it goes.
        let a2 = attach(&mut slow, g, &task, &sid, cursor, 256 * 1024, false, false)
            .await
            .unwrap();
        let last = n.to_string();
        let stop = consume_every(
            &mut slow,
            &a2.attach_id,
            &mut cursor,
            &mut got,
            Some(64 * 1024),
            |b| has(&b[b.len().saturating_sub(24)..], &last),
        )
        .await;
        eprintln!("px_099 slow client: caught up at {:?}", t0.elapsed());
        assert!(
            matches!(stop, Stop::Satisfied),
            "{stop:?}; the stream ended …{:?}",
            String::from_utf8_lossy(&got[got.len().saturating_sub(40)..])
        );
        assert!(
            got == broker_log(dir.path(), &sid, got.len()).await,
            "no gap and no duplicate across the drop: the broker's bytes exactly"
        );
        assert!(
            without_cr(&got) == seq_lf(n),
            "and the command's output, in order"
        );
        let r = invoke(
            &mut c,
            &task,
            g,
            "shell.cancel",
            &json!({"session_id": sid}),
        )
        .await;
        assert_eq!(r.status, "SUCCESS", "{r:?}");
    }

    // ------------------------------------------------------- KillTerminal

    async fn kill_with(
        c: &mut Client,
        generation: u64,
        task: &Id,
        session_id: &str,
        reason: &str,
        command_id: Id,
    ) -> Result<
        (
            modbit_protocol::v1::CommandAck,
            modbit_protocol::v1::TerminalKilled,
        ),
        ClientError,
    > {
        let ack = c
            .command(fenced(
                envelope(
                    command_id,
                    "KillTerminal",
                    modbit_protocol::v1::KillTerminal {
                        task_id: Some(task.clone()),
                        session_id: session_id.into(),
                        reason: reason.into(),
                    }
                    .encode_to_vec(),
                ),
                generation,
            ))
            .await?;
        let view = Client::result(&ack).unwrap();
        Ok((ack, view))
    }

    async fn set_mode(c: &mut Client, g: u64, task: &Id, mode: modbit_protocol::v1::TaskMode) {
        c.command(fenced(
            envelope(
                fresh(),
                "SetTaskMode",
                modbit_protocol::v1::SetTaskMode {
                    task_id: Some(task.clone()),
                    mode: mode as i32,
                    reason: "test".into(),
                }
                .encode_to_vec(),
            ),
            g,
        ))
        .await
        .unwrap();
    }

    fn alive(pid: u32) -> bool {
        Command::new("kill")
            .args(["-0", &pid.to_string()])
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|s| s.success())
    }

    async fn events_of(core: &CoreProcess, session: &Id, task: &Id, kind: &str) -> Vec<Value> {
        task_events(core, session, task)
            .await
            .into_iter()
            .filter(|(t, _)| t == kind)
            .map(|(_, p)| p)
            .collect()
    }

    /// PX-043 `KillTerminal` (QUAL-PX-043): a client stops a task's
    /// background terminal. The real process on the real PTY dies; the kill
    /// is recorded on the task once, with who, why and how it ended, and the
    /// transcript carries a Stopped row; the Capability Kernel decides it
    /// (a task in ASK mode, a stale lease and a task that does not own the
    /// terminal are each refused with a typed code and signal nothing); a
    /// retry of the same command id kills once and records once, and a
    /// second command on the dead terminal is answered ALREADY_ENDED.
    #[tokio::test]
    async fn px_043_kill_terminal_ends_the_process_records_who_and_why_and_acts_once() {
        let (_repo, root) = workspace();
        let dir = tempfile::tempdir().unwrap();
        let pidfile = dir.path().join("pid");
        let core = CoreProcess::spawn(dir.path(), &[]);
        let mut c = core.client().await;
        let (session, g) = create_session(&mut c, 0x10).await;
        let task = create_task(&mut c, &session, g, &root).await;
        let stranger = create_task(&mut c, &session, g, &root).await;
        let sid = start_shell(
            &mut c,
            &task,
            g,
            &format!("echo $$ > '{}'; exec sleep 600", pidfile.display()),
        )
        .await;
        let pid = loop {
            if let Some(p) = std::fs::read_to_string(&pidfile)
                .ok()
                .and_then(|t| t.trim().parse::<u32>().ok())
            {
                break p;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        };
        assert!(alive(pid));

        // Refused, and nothing was signalled.
        let r = kill_with(&mut c, g, &stranger, &sid, "not mine", fresh()).await;
        assert_eq!(rejected(r).0, "SESSION_NOT_OWNED");
        let r = kill_with(&mut c, g, &task, "no-such-terminal", "x", fresh()).await;
        assert_eq!(rejected(r).0, "UNKNOWN_SESSION");
        let r = kill_with(&mut c, g + 99, &task, &sid, "stale", fresh()).await;
        assert_eq!(rejected(r).0, "STALE_LEASE");
        set_mode(&mut c, g, &task, modbit_protocol::v1::TaskMode::Ask).await;
        let r = kill_with(&mut c, g, &task, &sid, "ask mode", fresh()).await;
        assert_eq!(rejected(r).0, "MODE_POSTURE");
        set_mode(&mut c, g, &task, modbit_protocol::v1::TaskMode::Agent).await;
        assert!(alive(pid), "every refusal left the process alone");
        assert!(
            events_of(&core, &session, &task, "BackgroundProcessEnded")
                .await
                .is_empty()
        );

        // The kill.
        let command = fresh();
        let (ack, killed) = kill_with(
            &mut c,
            g,
            &task,
            &sid,
            "the dev server is wedged",
            command.clone(),
        )
        .await
        .unwrap();
        assert_eq!(
            ack.status,
            modbit_protocol::v1::CommandStatus::Accepted as i32
        );
        assert_eq!(killed.outcome, "KILLED", "{killed:?}");
        assert!(killed.ended_by.starts_with("user:"), "{killed:?}");
        assert_eq!(killed.reason, "the dev server is wedged");
        assert!(
            killed.decision.starts_with("allow:") || killed.decision.starts_with("user-command:"),
            "{killed:?}"
        );
        assert_eq!(killed.output_ref.len(), 64);
        assert!(killed.offset > 0);
        // The real process is gone, and the registry says KILLED.
        let mut gone = false;
        for _ in 0..100 {
            if !alive(pid) {
                gone = true;
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        assert!(gone, "the process {pid} is still alive");
        let listed = list_terminals(&mut c, Some(&task)).await;
        let t = listed
            .terminals
            .iter()
            .find(|t| t.session_id == sid)
            .unwrap();
        assert_eq!(t.state, "KILLED", "{t:?}");

        // One typed record, with who and why.
        let ended = events_of(&core, &session, &task, "BackgroundProcessEnded").await;
        assert_eq!(ended.len(), 1, "{ended:#?}");
        let e = &ended[0];
        assert_eq!(e["handle_id"], sid.as_str());
        assert_eq!(e["how"], "KILLED");
        assert_eq!(e["source"], "KILL_COMMAND");
        assert_eq!(e["reason"], "the dev server is wedged");
        assert!(e["ended_by"].as_str().unwrap().starts_with("user:"));
        assert_eq!(e["output_ref"], killed.output_ref.as_str());

        // The same command again: answered from the record, one kill, one record.
        let (ack, again) = kill_with(&mut c, g, &task, &sid, "the dev server is wedged", command)
            .await
            .unwrap();
        assert_eq!(
            ack.status,
            modbit_protocol::v1::CommandStatus::Replayed as i32
        );
        assert_eq!(again.offset, killed.offset);
        assert_eq!(again.outcome, "KILLED");
        // A different command on the dead terminal adds nothing.
        let (_, late) = kill_with(&mut c, g, &task, &sid, "again", fresh())
            .await
            .unwrap();
        assert_eq!(late.outcome, "ALREADY_ENDED", "{late:?}");
        assert_eq!(late.offset, 0);
        assert_eq!(
            events_of(&core, &session, &task, "BackgroundProcessEnded")
                .await
                .len(),
            1
        );

        // The transcript records it as Stopped, as a row of its own.
        let ack = c
            .command(envelope(
                fresh(),
                "GetTranscript",
                modbit_protocol::v1::GetTranscript {
                    task_id: Some(task.clone()),
                    density: modbit_protocol::v1::TranscriptDensity::Detailed as i32,
                    after_row: 0,
                    limit: 500,
                    as_of_offset: 0,
                }
                .encode_to_vec(),
            ))
            .await
            .unwrap();
        let page: modbit_protocol::v1::TranscriptPage = Client::result(&ack).unwrap();
        let row = page
            .rows
            .iter()
            .find(|r| r.row_id == format!("terminal:{sid}"))
            .unwrap_or_else(|| panic!("no Stopped row: {:#?}", page.rows));
        assert_eq!(row.hints.as_ref().unwrap().status, "STOPPED");
        assert!(
            row.text.contains("the dev server is wedged"),
            "{}",
            row.text
        );
        assert!(row.text.contains("by user:"), "{}", row.text);
    }

    /// PX-043: a background process that ends on its own is recorded by the
    /// Core once, with no killer; a client's kill of it afterwards changes
    /// nothing (ALREADY_ENDED) and an end the agent saw itself is not
    /// recorded as a surprise to it (the agent's own `ProcessExited` stands).
    #[tokio::test]
    async fn px_043_a_process_that_ends_by_itself_is_recorded_once_without_a_killer() {
        let (_repo, root) = workspace();
        let dir = tempfile::tempdir().unwrap();
        let core = CoreProcess::spawn(dir.path(), &[("MODBIT_BACKGROUND_WATCH_MS", "100")]);
        let mut c = core.client().await;
        let (session, g) = create_session(&mut c, 0x10).await;
        let task = create_task(&mut c, &session, g, &root).await;
        let sid = start_shell(&mut c, &task, g, "sleep 1; echo finished; exit 3").await;
        let mut ended = Vec::new();
        for _ in 0..80 {
            ended = events_of(&core, &session, &task, "BackgroundProcessEnded").await;
            if !ended.is_empty() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        assert_eq!(ended.len(), 1, "{ended:#?}");
        assert_eq!(ended[0]["handle_id"], sid.as_str());
        assert_eq!(ended[0]["how"], "EXITED");
        assert_eq!(ended[0]["exit_code"], 3);
        assert_eq!(ended[0]["source"], "WATCHER");
        assert_eq!(ended[0]["ended_by"], "");
        // Still one a while later.
        tokio::time::sleep(Duration::from_millis(600)).await;
        assert_eq!(
            events_of(&core, &session, &task, "BackgroundProcessEnded")
                .await
                .len(),
            1
        );
        let (_, late) = kill_with(&mut c, g, &task, &sid, "too late", fresh())
            .await
            .unwrap();
        assert_eq!(late.outcome, "ALREADY_ENDED");
        assert_eq!(late.exit_code, Some(3));
        assert_eq!(late.ended_by, "");
        assert_eq!(
            events_of(&core, &session, &task, "BackgroundProcessEnded")
                .await
                .len(),
            1
        );
    }
}
