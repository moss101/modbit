//! PX-099 against the real broker binary over the real socket, with real
//! processes and PTYs (docs/21 "Durable modbit-execd"): acknowledgement
//! windows, resize, the person's input lease and the typed refusals.

use std::io::{BufRead, BufReader};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::time::Duration;

use modbit_protocol::local::{ReadyLine, decode_hex};
use modbit_protocol::v1::ExecRequest;
use modbit_terminal::{AttachOptions, Error, Event, ExecClient};

const KIB: u64 = 1024;
const MIB: u64 = 1024 * KIB;

struct Execd {
    child: Child,
    ready: ReadyLine,
}

impl Execd {
    fn spawn_with(dir: &Path, extra: &[&str]) -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_modbit-execd"))
            .arg("--data-dir")
            .arg(dir)
            .args(extra)
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap();
        let mut lines = BufReader::new(child.stdout.take().unwrap()).lines();
        let ready = loop {
            let l = lines.next().expect("ready line").unwrap();
            if let Some(r) = ReadyLine::parse(&l) {
                break r;
            }
        };
        std::thread::spawn(move || for _ in lines {});
        Execd { child, ready }
    }

    fn spawn(dir: &Path) -> Self {
        Self::spawn_with(dir, &[])
    }

    async fn client(&self) -> ExecClient {
        ExecClient::connect(
            &self.ready.endpoint,
            &decode_hex(&self.ready.boot_secret_hex).unwrap(),
        )
        .await
        .unwrap()
    }
}

impl Drop for Execd {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn child_bin() -> String {
    let deps = std::env::current_exe().unwrap();
    let debug = deps.parent().unwrap().parent().unwrap();
    let name = if cfg!(windows) {
        "execd_child.exe"
    } else {
        "execd_child"
    };
    let p = debug.join("examples").join(name);
    assert!(
        p.exists(),
        "{} missing; run `cargo build -p modbit-execd --examples`",
        p.display()
    );
    p.to_string_lossy().into_owned()
}

fn req(id: &str, role: &str) -> ExecRequest {
    let mut env = std::collections::HashMap::new();
    env.insert("MODBIT_EXECD_TEST_ROLE".to_owned(), role.to_owned());
    ExecRequest {
        request_id: id.into(),
        argv: vec![child_bin(), "--".into()],
        cwd: String::new(),
        env,
        inherit_env: false,
        timeout_ms: 0,
        pty: false,
        stdin_mode: "closed".into(),
        output_budget_bytes: 4096,
        execution_profile: "local_trusted".into(),
        capability_lease_id: None,
        terminal_session_id: None,
        owner: String::new(),
        pty_rows: 0,
        pty_cols: 0,
    }
}

fn noisy_req(id: &str, mib: u64) -> ExecRequest {
    let mut r = req(id, "noisy-live");
    r.env
        .insert("MODBIT_EXECD_TEST_MIB".into(), mib.to_string());
    r
}

/// The bytes `[from, from + len)` of the `noisy` role's stream: line `i` is
/// `{i:063}\n`, 64 bytes, so any offset's content is known without storing it.
fn noisy_bytes(from: u64, len: usize) -> Vec<u8> {
    let first = from / 64;
    let mut out = Vec::with_capacity(len + 128);
    let mut i = first;
    while (out.len() as u64) < from % 64 + len as u64 {
        out.extend_from_slice(format!("{i:063}\n").as_bytes());
        i += 1;
    }
    let skip = (from % 64) as usize;
    out[skip..skip + len].to_vec()
}

fn rss_kib(pid: u32) -> Option<u64> {
    if cfg!(windows) {
        return None;
    }
    let out = Command::new("ps")
        .args(["-o", "rss=", "-p", &pid.to_string()])
        .output()
        .ok()?;
    String::from_utf8_lossy(&out.stdout).trim().parse().ok()
}

async fn start(c: &mut ExecClient, r: ExecRequest) -> String {
    c.exec(r).await.unwrap();
    loop {
        if let Event::Started(s) = c.next().await.unwrap().unwrap() {
            return s.session_id;
        }
    }
}

/// Read a windowed attachment until `quiet` passes with nothing new, checking
/// every chunk against the `noisy` stream and acknowledging nothing; returns
/// the cursor after the last byte received.
async fn drain_unacked(c: &mut ExecClient, from: u64, quiet: Duration) -> u64 {
    drain_unacked_after(c, from, quiet, false).await
}

/// As `drain_unacked`; with `expect_output` the first chunk is awaited for as
/// long as a slow machine's child needs to start writing.
async fn drain_unacked_after(
    c: &mut ExecClient,
    from: u64,
    quiet: Duration,
    expect_output: bool,
) -> u64 {
    let mut next = from;
    loop {
        let wait = if expect_output && next == from {
            Duration::from_secs(30)
        } else {
            quiet
        };
        match tokio::time::timeout(wait, c.next()).await {
            Err(_) => return next,
            Ok(Ok(Some(Event::Output(o)))) => {
                assert_eq!(o.cursor, next, "no gap and no duplicate");
                assert_eq!(o.data, noisy_bytes(o.cursor, o.data.len()));
                next += o.data.len() as u64;
            }
            Ok(other) => panic!("unexpected {other:?}"),
        }
    }
}

fn expect_err(r: Result<Option<Event>, Error>, code: &str) -> String {
    match r {
        Err(Error::Exec { code: got, message }) => {
            assert_eq!(got, code, "{message}");
            message
        }
        other => panic!("expected {code}, got {other:?}"),
    }
}

async fn next_within(c: &mut ExecClient, what: &str) -> Result<Option<Event>, Error> {
    match tokio::time::timeout(Duration::from_secs(20), c.next()).await {
        Ok(r) => r,
        Err(_) => panic!("{what}: nothing within 20s"),
    }
}

/// PX-099: a windowed attachment never has more than the window (plus one
/// chunk) of unacknowledged bytes pushed; the rest waits in the log, not in
/// memory; each acknowledgement opens the window and the stream resumes
/// byte-exact, with no gap and no duplicate.
#[tokio::test]
async fn px_099_the_window_pauses_the_push_until_the_client_acknowledges() {
    let dir = tempfile::tempdir().unwrap();
    let execd = Execd::spawn(dir.path());
    let mut starter = execd.client().await;
    let sid = start(&mut starter, noisy_req("win", 4)).await;
    drop(starter);
    let mut c = execd.client().await;
    let window = 256 * KIB;
    c.attach_with(
        &sid,
        0,
        AttachOptions {
            window_bytes: window,
            stall_ms: 120_000,
            strict_cursor: true,
            ..AttachOptions::default()
        },
    )
    .await
    .unwrap();
    // Nothing acknowledged: the push stops at the window.
    let got = drain_unacked_after(&mut c, 0, Duration::from_millis(700), true).await;
    assert!(
        (window..window + 64 * KIB).contains(&got),
        "pushed {got} bytes against a {window}-byte window"
    );
    // Still paused a while later: nothing was pushed in the meantime.
    assert_eq!(
        drain_unacked(&mut c, got, Duration::from_millis(500)).await,
        got
    );
    // Acknowledge as it is consumed: the whole 4 MiB arrives, exactly once.
    let mut cursor = got;
    c.ack(&sid, cursor).await.unwrap();
    while cursor < 4 * MIB {
        match next_within(&mut c, "acknowledged stream").await {
            Ok(Some(Event::Output(o))) => {
                assert_eq!(o.cursor, cursor, "no gap and no duplicate");
                assert_eq!(o.data, noisy_bytes(o.cursor, o.data.len()));
                cursor += o.data.len() as u64;
                c.ack(&sid, cursor).await.unwrap();
            }
            other => panic!("{other:?}"),
        }
    }
    assert_eq!(cursor, 4 * MIB);
    c.attach(&sid, 0).await.ok();
}

/// PX-099: a client that never acknowledges is dropped to cursor-pull after
/// the stall time, told the cursor to resume from; its own cursor resumes the
/// stream with no gap and no duplicate.
#[tokio::test]
async fn px_099_a_stalled_client_is_dropped_to_cursor_pull_and_resumes_exactly() {
    let dir = tempfile::tempdir().unwrap();
    let execd = Execd::spawn(dir.path());
    let mut starter = execd.client().await;
    let sid = start(&mut starter, noisy_req("stall", 4)).await;
    drop(starter);
    let mut c = execd.client().await;
    c.attach_with(
        &sid,
        0,
        AttachOptions {
            window_bytes: 64 * KIB,
            stall_ms: 400,
            strict_cursor: true,
            ..AttachOptions::default()
        },
    )
    .await
    .unwrap();
    // Consumed but never acknowledged.
    let mut consumed = 0u64;
    let message = loop {
        match next_within(&mut c, "stalled attachment").await {
            Ok(Some(Event::Output(o))) => {
                assert_eq!(o.cursor, consumed);
                assert_eq!(o.data, noisy_bytes(o.cursor, o.data.len()));
                consumed += o.data.len() as u64;
            }
            other => break expect_err(other, "ATTACH_STALLED"),
        }
    };
    assert!(
        message.contains("resume_cursor=0"),
        "the broker names the last acknowledged cursor: {message}"
    );
    assert!((64 * KIB..128 * KIB).contains(&consumed), "{consumed}");
    // Cursor-pull: the client resumes from the cursor it consumed.
    let mut d = execd.client().await;
    d.attach_with(
        &sid,
        consumed,
        AttachOptions {
            window_bytes: 64 * KIB,
            strict_cursor: true,
            ..AttachOptions::default()
        },
    )
    .await
    .unwrap();
    let mut cursor = consumed;
    d.ack(&sid, cursor).await.unwrap();
    while cursor < 4 * MIB {
        match next_within(&mut d, "resumed stream").await {
            Ok(Some(Event::Output(o))) => {
                assert_eq!(o.cursor, cursor, "no gap and no duplicate");
                assert_eq!(o.data, noisy_bytes(o.cursor, o.data.len()));
                cursor += o.data.len() as u64;
                d.ack(&sid, cursor).await.unwrap();
            }
            other => panic!("{other:?}"),
        }
    }
}

/// PX-099: an acknowledgement beyond the output that exists does not widen
/// the window; a cursor beyond the head is refused with the typed error when
/// the attach is strict.
#[tokio::test]
async fn px_099_cursor_beyond_the_head_is_typed_and_a_false_ack_widens_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let execd = Execd::spawn(dir.path());
    let mut starter = execd.client().await;
    let sid = start(&mut starter, noisy_req("head", 2)).await;
    drop(starter);
    let mut c = execd.client().await;
    c.attach_with(
        &sid,
        100 * MIB,
        AttachOptions {
            strict_cursor: true,
            ..AttachOptions::default()
        },
    )
    .await
    .unwrap();
    let message = expect_err(
        next_within(&mut c, "beyond head").await,
        "CURSOR_BEYOND_HEAD",
    );
    assert!(message.contains("head="), "{message}");
    // A client that acknowledges bytes that do not exist gets no more window.
    let mut d = execd.client().await;
    d.attach_with(
        &sid,
        0,
        AttachOptions {
            window_bytes: 64 * KIB,
            stall_ms: 120_000,
            ..AttachOptions::default()
        },
    )
    .await
    .unwrap();
    let got = drain_unacked_after(&mut d, 0, Duration::from_millis(600), true).await;
    assert!(got < 64 * KIB + 64 * KIB, "{got}");
    d.ack(&sid, u64::MAX).await.unwrap();
    let more = drain_unacked(&mut d, got, Duration::from_millis(800)).await;
    assert!(
        more - got <= 2 * MIB,
        "the clamped ack opened at most the {} bytes there are",
        2 * MIB
    );
}

/// PX-099: a broker whose client never acknowledges holds memory to a bound
/// while a process writes 128 MiB, then the client catches up by cursor,
/// byte-exact. (QUAL-PX-099 states 200 MiB; the bound is what is measured.)
#[tokio::test]
async fn px_099_a_slow_client_does_not_grow_the_brokers_memory() {
    let dir = tempfile::tempdir().unwrap();
    let execd = Execd::spawn_with(dir.path(), &["--replay-window-bytes", "201326592"]);
    let pid = execd.child.id();
    let mut starter = execd.client().await;
    let total = 128 * MIB;
    let sid = start(&mut starter, noisy_req("mem", total / MIB)).await;
    drop(starter);
    // The slow client: attached, windowed, never reads and never acknowledges.
    let mut slow = execd.client().await;
    slow.attach_with(
        &sid,
        0,
        AttachOptions {
            window_bytes: 256 * KIB,
            stall_ms: 600_000,
            strict_cursor: true,
            ..AttachOptions::default()
        },
    )
    .await
    .unwrap();
    let baseline = rss_kib(pid);
    let mut peak = 0u64;
    let mut probe = execd.client().await;
    loop {
        peak = peak.max(rss_kib(pid).unwrap_or(0));
        probe.list().await.unwrap();
        let Some(Event::Sessions(l)) = probe.next().await.unwrap() else {
            panic!()
        };
        if l[0].bytes_so_far >= total {
            break;
        }
        tokio::time::sleep(Duration::from_millis(40)).await;
    }
    peak = peak.max(rss_kib(pid).unwrap_or(0));
    eprintln!("px_099 slow client: {total} B written; broker RSS {baseline:?} -> peak {peak} KiB");
    if let Some(base) = baseline {
        assert!(
            peak < base + 48 * 1024,
            "broker RSS peaked at {peak} KiB over a {base} KiB start"
        );
    }
    drop(slow);
    // Catch up by cursor: the whole stream, once, in order.
    let mut c = execd.client().await;
    c.attach_with(
        &sid,
        0,
        AttachOptions {
            window_bytes: MIB,
            strict_cursor: true,
            ..AttachOptions::default()
        },
    )
    .await
    .unwrap();
    let mut cursor = 0u64;
    while cursor < total {
        match next_within(&mut c, "catch-up").await {
            Ok(Some(Event::Output(o))) => {
                assert_eq!(o.cursor, cursor, "no gap and no duplicate");
                if o.cursor % (8 * MIB) < 64 * KIB {
                    assert_eq!(o.data, noisy_bytes(o.cursor, o.data.len()));
                }
                cursor += o.data.len() as u64;
                c.ack(&sid, cursor).await.unwrap();
            }
            other => panic!("{other:?}"),
        }
    }
    peak = peak.max(rss_kib(pid).unwrap_or(0));
    if let Some(base) = baseline {
        assert!(peak < base + 48 * 1024, "RSS after catch-up {peak} KiB");
    }
}

/// Read output from a PTY session until it has shown `needle` (bounded).
async fn until_output(c: &mut ExecClient, seen: &mut String, needle: &str) {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
    while !seen.contains(needle) {
        match tokio::time::timeout_at(deadline, c.next()).await {
            Err(_) => panic!("never saw {needle:?}; output so far {seen:?}"),
            Ok(Ok(Some(Event::Output(o)))) => seen.push_str(&String::from_utf8_lossy(&o.data)),
            Ok(Ok(Some(_))) => {}
            Ok(other) => panic!("{needle:?}: {other:?}"),
        }
    }
}

/// PX-099: a resize is applied to the PTY — the child reads the new size from
/// the terminal — and bad requests are typed.
#[tokio::test]
async fn px_099_resize_changes_the_size_the_child_reads() {
    if cfg!(windows) {
        eprintln!(
            "SKIPPED px_099 resize: the observer is `stty size`, which a Windows ConPTY child does not have; ConPTY resize (ResizePseudoConsole) is exercised by the broker but not asserted through a child on this platform"
        );
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let execd = Execd::spawn(dir.path());
    let mut c = execd.client().await.act_as("task:alpha");
    let mut r = req("size", "stty-size");
    r.pty = true;
    r.pty_rows = 30;
    r.pty_cols = 100;
    let sid = start(&mut c, r).await;
    let mut seen = String::new();
    until_output(&mut c, &mut seen, "size=30 100").await;
    c.resize(&sid, 55, 200).await.unwrap();
    // The reply interleaves with the child's own output on this connection.
    loop {
        match next_within(&mut c, "resize").await {
            Ok(Some(Event::Resized(r))) => {
                assert_eq!((r.rows, r.cols), (55, 200));
                break;
            }
            Ok(Some(Event::Output(o))) => seen.push_str(&String::from_utf8_lossy(&o.data)),
            other => panic!("{other:?}"),
        }
    }
    c.write_stdin(&sid, b"size\r").await.unwrap();
    until_output(&mut c, &mut seen, "size=55 200").await;
    // Listed with the size it has now.
    let mut l = execd.client().await;
    l.list().await.unwrap();
    let Some(Event::Sessions(sessions)) = l.next().await.unwrap() else {
        panic!()
    };
    let info = &sessions[0];
    assert_eq!((info.pty, info.pty_rows, info.pty_cols), (true, 55, 200));
    // Typed refusals; nothing changes.
    for (rows, cols) in [(0, 80), (80, 0), (501, 80), (24, 1001)] {
        c.resize(&sid, rows, cols).await.unwrap();
        loop {
            match next_within(&mut c, "bad size").await {
                Ok(Some(Event::Output(_))) => {}
                other => {
                    expect_err(other, "BAD_SIZE");
                    break;
                }
            }
        }
    }
    // A task that does not own it cannot resize it.
    let mut other = execd.client().await.act_as("task:beta");
    other.resize(&sid, 10, 10).await.unwrap();
    expect_err(
        next_within(&mut other, "not owned").await,
        "SESSION_NOT_OWNED",
    );
    // A piped session has no terminal to size.
    let mut p = execd.client().await;
    let pipe = start(&mut p, req("pipe", "ticker")).await;
    p.resize(&pipe, 24, 80).await.unwrap();
    loop {
        match next_within(&mut p, "not a pty").await {
            Ok(Some(Event::Output(_))) => {}
            other => {
                expect_err(other, "NOT_A_PTY");
                break;
            }
        }
    }
    c.write_stdin(&sid, b"quit\r").await.unwrap();
    p.cancel(&pipe).await.unwrap();
}

/// PX-099: the person's input lease. A person attaching takes it; the agent's
/// writes are refused while it is held; no other person's keystrokes are
/// accepted; it ends with the holder's connection; a task cannot take it.
#[tokio::test]
async fn px_099_the_input_lease_blocks_the_agent_and_ends_with_its_connection() {
    let dir = tempfile::tempdir().unwrap();
    let execd = Execd::spawn(dir.path());
    let mut agent = execd.client().await.act_as("task:alpha");
    let mut r = req("lease", "cat");
    r.pty = true;
    let sid = start(&mut agent, r).await;
    // Before anyone holds the lease the agent types (and is answered).
    agent.write_stdin_acked(&sid, b"first\r").await.unwrap();
    let mut seen = String::new();
    loop {
        match next_within(&mut agent, "agent input").await {
            Ok(Some(Event::StdinWritten(w))) => assert_eq!(w.bytes, 6),
            Ok(Some(Event::Output(o))) => {
                seen.push_str(&String::from_utf8_lossy(&o.data));
                if seen.contains("got:first") {
                    break;
                }
            }
            other => panic!("{other:?}"),
        }
    }
    // A task cannot take the person's lease.
    agent.acquire_lease(&sid, "user:x", false).await.unwrap();
    loop {
        match next_within(&mut agent, "task lease").await {
            Ok(Some(Event::Output(_) | Event::StdinWritten(_))) => {}
            other => {
                expect_err(other, "LEASE_NOT_PERMITTED");
                break;
            }
        }
    }
    // A person attaches and takes it.
    let mut alice = execd.client().await;
    alice
        .acquire_lease(&sid, "user:alice", false)
        .await
        .unwrap();
    let Some(Event::Lease(l)) = alice.next().await.unwrap() else {
        panic!()
    };
    assert!(l.held);
    assert_eq!(l.holder, "user:alice");
    // The agent is refused with the typed error and nothing is written.
    agent
        .write_stdin_acked(&sid, b"agent-typed\r")
        .await
        .unwrap();
    loop {
        match next_within(&mut agent, "leased").await {
            Ok(Some(Event::Output(o))) => seen.push_str(&String::from_utf8_lossy(&o.data)),
            other => {
                let m = expect_err(other, "INPUT_LEASED");
                assert!(m.contains("user:alice"), "{m}");
                break;
            }
        }
    }
    // Another person's keystrokes, and the holder's own non-user writes, are refused.
    let mut bob = execd.client().await;
    bob.write_user_input(&sid, b"bob\r").await.unwrap();
    expect_err(next_within(&mut bob, "bob input").await, "LEASE_REQUIRED");
    bob.acquire_lease(&sid, "user:bob", false).await.unwrap();
    expect_err(next_within(&mut bob, "bob lease").await, "LEASE_HELD");
    // The holder types; it is answered; the bytes are exactly hers.
    alice.write_user_input(&sid, b"from-alice\r").await.unwrap();
    let mut a_seen = String::new();
    // (the holder must watch the session to see the answer)
    let mut watch = execd.client().await;
    watch.attach(&sid, 0).await.unwrap();
    match next_within(&mut alice, "alice input").await {
        Ok(Some(Event::StdinWritten(w))) => assert_eq!(w.bytes, 11),
        other => panic!("{other:?}"),
    }
    until_output(&mut watch, &mut a_seen, "got:from-alice").await;
    assert!(
        !a_seen.contains("agent-typed") && !a_seen.contains("bob"),
        "only the lease holder's bytes reached the terminal: {a_seen:?}"
    );
    // Taking it over is explicit, and the previous holder is told.
    bob.acquire_lease(&sid, "user:bob", true).await.unwrap();
    let Some(Event::Lease(l)) = bob.next().await.unwrap() else {
        panic!()
    };
    assert!(l.held && l.holder == "user:bob");
    let Some(Event::Lease(told)) = alice.next().await.unwrap() else {
        panic!()
    };
    assert!(!told.held && told.holder == "user:bob");
    alice.write_user_input(&sid, b"late\r").await.unwrap();
    expect_err(
        next_within(&mut alice, "alice lost").await,
        "LEASE_REQUIRED",
    );
    // The lease is in the registry.
    let mut l = execd.client().await;
    l.list().await.unwrap();
    let Some(Event::Sessions(s)) = l.next().await.unwrap() else {
        panic!()
    };
    assert_eq!(s[0].input_lease_holder, "user:bob");
    // The holder's connection ends: the lease ends with it; the agent types again.
    drop(bob);
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    loop {
        agent.write_stdin_acked(&sid, b"after\r").await.unwrap();
        let mut done = false;
        loop {
            match next_within(&mut agent, "lease released").await {
                Ok(Some(Event::StdinWritten(_))) => {
                    done = true;
                    break;
                }
                Ok(Some(Event::Output(_))) => {}
                Err(Error::Exec { code, .. }) if code == "INPUT_LEASED" => break,
                other => panic!("{other:?}"),
            }
        }
        if done {
            break;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "the lease outlived its connection"
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    // A second task cannot write to it at all; an oversized write writes nothing.
    let mut beta = execd.client().await.act_as("task:beta");
    beta.write_stdin_acked(&sid, b"x").await.unwrap();
    expect_err(next_within(&mut beta, "beta").await, "SESSION_NOT_OWNED");
    agent
        .write_stdin_acked(&sid, &vec![b'a'; 65 * 1024])
        .await
        .unwrap();
    loop {
        match next_within(&mut agent, "too large").await {
            Ok(Some(Event::Output(_))) => {}
            other => {
                expect_err(other, "INPUT_TOO_LARGE");
                break;
            }
        }
    }
    // After the process ends, input is refused with a typed reason.
    agent.write_stdin_acked(&sid, b"quit\r").await.unwrap();
    loop {
        match next_within(&mut agent, "exit").await {
            Ok(Some(Event::Exited(_))) => break,
            Ok(Some(_)) => {}
            other => panic!("{other:?}"),
        }
    }
    agent.write_stdin_acked(&sid, b"ghost\r").await.unwrap();
    expect_err(
        next_within(&mut agent, "finished").await,
        "SESSION_FINISHED",
    );
}
