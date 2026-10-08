//! Real-process tests for `modbit-execd` (docs/21): the actual broker binary
//! over the real socket, running real child processes (this test binary in a
//! child role, so behavior is identical on macOS, Linux and Windows).

use std::io::{BufRead, BufReader};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use modbit_protocol::local::{ReadyLine, decode_hex};
use modbit_protocol::v1::ExecRequest;
use modbit_terminal::{Error, Event, ExecClient};
use sha2::{Digest, Sha256};

struct Execd {
    child: Child,
    ready: ReadyLine,
}

impl Execd {
    fn spawn(dir: &Path) -> Self {
        Self::spawn_with(dir, &[])
    }

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
    // cargo test builds examples; the binary sits beside the test's deps dir.
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

fn self_role(
    role: &str,
    extra: &[&str],
) -> (Vec<String>, std::collections::HashMap<String, String>) {
    let mut argv = vec![child_bin(), "--".into()];
    argv.extend(extra.iter().map(|s| (*s).to_owned()));
    let mut env = std::collections::HashMap::new();
    env.insert("MODBIT_EXECD_TEST_ROLE".to_owned(), role.to_owned());
    (argv, env)
}

fn req(id: &str, role: &str, extra: &[&str]) -> ExecRequest {
    let (argv, env) = self_role(role, extra);
    ExecRequest {
        request_id: id.into(),
        argv,
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

/// Drain until exit; returns (session id, stdout bytes, stderr bytes, exited).
async fn run_to_exit(
    c: &mut ExecClient,
) -> (String, Vec<u8>, Vec<u8>, modbit_terminal::ProcessExited) {
    let mut sid = String::new();
    let (mut out, mut err) = (Vec::new(), Vec::new());
    loop {
        match c.next().await.unwrap().expect("broker closed") {
            Event::Started(s) => sid = s.session_id,
            Event::Output(o) => {
                if o.stream == "stderr" {
                    err.extend(o.data)
                } else {
                    out.extend(o.data)
                }
            }
            Event::Exited(e) => return (sid, out, err, e),
            Event::Sessions(_)
            | Event::SandboxProbed(_)
            | Event::Resized(_)
            | Event::Lease(_)
            | Event::StdinWritten(_)
            | Event::Listeners(_) => {}
        }
    }
}

#[tokio::test]
async fn qual_ev_0100_argv_cwd_env_streams_exit_code_and_timeout_are_explicit() {
    let dir = tempfile::tempdir().unwrap();
    let cwd = tempfile::tempdir().unwrap();
    let execd = Execd::spawn(dir.path());
    let mut c = execd.client().await;
    let mut r = req("r1", "echo-args", &["alpha", "beta gamma"]);
    // Windows canonicalize yields a verbatim `\\?\` prefix that a child never prints back.
    let cwd_text = cwd
        .path()
        .canonicalize()
        .unwrap()
        .to_string_lossy()
        .trim_start_matches(r"\\?\")
        .to_owned();
    r.cwd = cwd_text.clone();
    r.env.insert("MODBIT_T1".into(), "one".into());
    c.exec(r).await.unwrap();
    let (_, out, err, exited) = run_to_exit(&mut c).await;
    let text = String::from_utf8(out).unwrap();
    assert!(text.contains("args=[\"alpha\", \"beta gamma\"]"), "{text}");
    assert!(text.contains(&format!("cwd={cwd_text}")), "{text}");
    assert!(text.contains("MODBIT_T1=one"), "{text}");
    assert!(
        text.contains("HOME_SET=false"),
        "environment is explicit, not inherited: {text}"
    );
    assert_eq!(String::from_utf8(err).unwrap().trim(), "to-stderr");
    assert_eq!(
        exited.exit_code,
        Some(3),
        "non-zero exit is a result, not an error"
    );
    assert!(!exited.timed_out && !exited.cancelled);
    assert_eq!(
        exited.total_bytes as usize,
        text.len() + "to-stderr\n".len()
    );

    // Timeout kills a real process.
    let mut t = req("r2", "ticker", &[]);
    t.timeout_ms = 300;
    let started = Instant::now();
    c.exec(t).await.unwrap();
    let (_, out, _, exited) = run_to_exit(&mut c).await;
    assert!(exited.timed_out, "{exited:?}");
    assert!(started.elapsed() < Duration::from_secs(10));
    assert!(String::from_utf8_lossy(&out).contains("tick 0"));

    // Bad argv is a typed error, not a crash.
    let mut bad = req("r3", "x", &[]);
    bad.argv = vec!["/definitely/not/a/binary".into()];
    c.exec(bad).await.unwrap();
    let err = c.next().await.unwrap_err();
    assert!(
        matches!(err, Error::Exec { ref code, .. } if code == "EXEC_FAILED"),
        "{err}"
    );
}

#[tokio::test]
async fn qual_ev_0019_0269_ten_megabytes_stream_in_bounded_chunks_and_output_ref_digest_matches() {
    let dir = tempfile::tempdir().unwrap();
    let execd = Execd::spawn(dir.path());
    let mut c = execd.client().await;
    c.exec(req("big", "big", &[])).await.unwrap();
    let mut sid = String::new();
    let mut hasher = Sha256::new();
    let mut total = 0usize;
    let mut chunks = 0usize;
    let mut next_cursor = 0u64;
    let exited = loop {
        match c.next().await.unwrap().unwrap() {
            Event::Started(s) => sid = s.session_id,
            Event::Output(o) => {
                assert!(o.data.len() <= 64 * 1024, "bounded frames");
                assert!(o.cursor >= next_cursor, "cursors are monotonic");
                next_cursor = o.cursor + o.data.len() as u64;
                hasher.update(&o.data);
                total += o.data.len();
                chunks += 1;
            }
            Event::Exited(e) => break e,
            Event::Sessions(_)
            | Event::SandboxProbed(_)
            | Event::Resized(_)
            | Event::Lease(_)
            | Event::StdinWritten(_)
            | Event::Listeners(_) => {}
        }
    };
    assert_eq!(total, 10 * 1024 * 1024);
    assert!(chunks >= 160, "{chunks}");
    assert_eq!(exited.total_bytes as usize, total);
    assert_eq!(
        exited.output_ref,
        hex::encode(hasher.finalize()),
        "OutputRef digest equals the raw output"
    );
    // The complete artifact is retrievable from the broker's object store by digest.
    let obj = dir
        .path()
        .join("objects")
        .join(&exited.output_ref[..2])
        .join(&exited.output_ref[2..]);
    assert_eq!(std::fs::metadata(&obj).unwrap().len() as usize, total);
    assert!(!sid.is_empty());
}

#[tokio::test]
async fn qual_ev_0271_0027_0135_detach_reattach_from_cursor_exactly_then_cancel_kills_the_real_process()
 {
    let dir = tempfile::tempdir().unwrap();
    let execd = Execd::spawn(dir.path());
    let mut a = execd.client().await;
    a.exec(req("long", "ticker", &[])).await.unwrap();
    let Event::Started(s) = a.next().await.unwrap().unwrap() else {
        panic!()
    };
    let sid = s.session_id;
    let mut seen: Vec<(u64, Vec<u8>)> = Vec::new();
    while seen.len() < 5 {
        if let Event::Output(o) = a.next().await.unwrap().unwrap() {
            seen.push((o.cursor, o.data));
        }
    }
    // "UI restart": drop the client entirely; the process keeps running.
    drop(a);
    tokio::time::sleep(Duration::from_millis(200)).await;
    let mut b = execd.client().await;
    b.list().await.unwrap();
    let Event::Sessions(list) = b.next().await.unwrap().unwrap() else {
        panic!()
    };
    let info = list
        .iter()
        .find(|s| s.session_id == sid)
        .expect("session survives client restart");
    assert!(info.running);
    assert!(
        info.bytes_so_far > seen.iter().map(|(_, d)| d.len() as u64).sum::<u64>(),
        "it kept producing while detached"
    );
    // Reattach from the cursor after the third chunk: exact continuation, no duplicates, no gaps.
    let resume_from = seen[2].0 + seen[2].1.len() as u64;
    b.attach(&sid, resume_from).await.unwrap();
    let mut replay = Vec::new();
    while replay.len() < 2 {
        if let Event::Output(o) = b.next().await.unwrap().unwrap() {
            replay.push((o.cursor, o.data));
        }
    }
    assert_eq!(replay[0], seen[3]);
    assert_eq!(replay[1], seen[4]);
    // Replay from 0 reproduces the exact prefix including record boundaries.
    let mut c = execd.client().await;
    c.attach(&sid, 0).await.unwrap();
    let mut from_zero = Vec::new();
    while from_zero.len() < 5 {
        if let Event::Output(o) = c.next().await.unwrap().unwrap() {
            from_zero.push((o.cursor, o.data));
        }
    }
    assert_eq!(from_zero, seen);
    // Cancel: the real process dies and every attached client sees the exit.
    b.cancel(&sid).await.unwrap();
    let exited = loop {
        if let Event::Exited(e) = b.next().await.unwrap().unwrap() {
            break e;
        }
    };
    assert!(exited.cancelled, "{exited:?}");
    let exited_c = loop {
        if let Event::Exited(e) = c.next().await.unwrap().unwrap() {
            break e;
        }
    };
    assert_eq!(exited_c.output_ref, exited.output_ref);
    let mut d = execd.client().await;
    d.list().await.unwrap();
    let Event::Sessions(list) = d.next().await.unwrap().unwrap() else {
        panic!()
    };
    assert!(!list.iter().find(|s| s.session_id == sid).unwrap().running);
    // Idempotent: re-sending the same request id replays the session instead of starting twice.
    d.exec(req("long", "ticker", &[])).await.unwrap();
    let Event::Started(s2) = d.next().await.unwrap().unwrap() else {
        panic!()
    };
    assert!(s2.replayed && s2.session_id == sid);
}

#[tokio::test]
async fn qual_ev_0025_stdin_is_explicit_and_two_requests_do_not_share_environment() {
    let dir = tempfile::tempdir().unwrap();
    let execd = Execd::spawn(dir.path());
    let mut c = execd.client().await;
    let mut r = req("cat", "cat", &[]);
    r.stdin_mode = "open".into();
    c.exec(r).await.unwrap();
    let Event::Started(s) = c.next().await.unwrap().unwrap() else {
        panic!()
    };
    c.write_stdin(&s.session_id, b"hello\nquit\n")
        .await
        .unwrap();
    let (_, out, _, exited) = run_to_exit(&mut c).await;
    assert_eq!(String::from_utf8(out).unwrap(), "got:hello\ngot:quit\n");
    assert_eq!(exited.exit_code, Some(0));
    // Closed stdin: writes are refused while the process is alive (a
    // long-lived child, so the refusal cannot race the exit).
    c.exec(req("closed", "ticker", &[])).await.unwrap();
    let Event::Started(s) = c.next().await.unwrap().unwrap() else {
        panic!()
    };
    c.write_stdin(&s.session_id, b"x").await.unwrap();
    let mut saw_refusal = false;
    loop {
        match c.next().await {
            Ok(Some(Event::Exited(_))) => break,
            Ok(Some(_)) => {}
            Ok(None) => break,
            Err(Error::Exec { code, .. }) => {
                saw_refusal = code == "STDIN_FAILED";
                break;
            }
            Err(e) => panic!("{e}"),
        }
    }
    assert!(saw_refusal, "writing to closed stdin is a typed error");
    c.cancel(&s.session_id).await.unwrap();
    let _ = run_to_exit(&mut c).await;
    // Environment isolation (REQ-EV-0025): the second request does not see the first's variable.
    let mut c2 = execd.client().await;
    let mut a = req("env-a", "echo-args", &[]);
    a.env.insert("MODBIT_T1".into(), "leak?".into());
    c2.exec(a).await.unwrap();
    let (_, out_a, _, _) = run_to_exit(&mut c2).await;
    assert!(String::from_utf8_lossy(&out_a).contains("MODBIT_T1=leak?"));
    c2.exec(req("env-b", "echo-args", &[])).await.unwrap();
    let (_, out_b, _, _) = run_to_exit(&mut c2).await;
    assert!(
        String::from_utf8_lossy(&out_b).contains("MODBIT_T1=\n"),
        "{}",
        String::from_utf8_lossy(&out_b)
    );
}

/// PX-030: the PTY runs on every platform — a Unix pseudo-terminal, a
/// Windows ConPTY — and the same session reads input and exits.
#[tokio::test]
async fn pty_mode_runs_a_real_terminal_session() {
    let dir = tempfile::tempdir().unwrap();
    let execd = Execd::spawn(dir.path());
    let mut c = execd.client().await;
    let mut r = req("pty", "cat", &[]);
    r.pty = true;
    c.exec(r).await.unwrap();
    let Event::Started(s) = c.next().await.unwrap().unwrap() else {
        panic!()
    };
    c.write_stdin(&s.session_id, b"hi\r").await.unwrap();
    c.write_stdin(&s.session_id, b"quit\r").await.unwrap();
    // Bounded: a terminal that never reports its exit fails here, not by
    // hanging the suite — with what it did say, so a platform difference is
    // diagnosable from the failure alone.
    let deadline = tokio::time::Instant::now() + Duration::from_secs(60);
    let mut out = Vec::new();
    let mut seen = Vec::new();
    let exited = loop {
        match tokio::time::timeout_at(deadline, c.next()).await {
            Err(_) => panic!(
                "the PTY session did not report its exit within a minute; output so far {:?}; events {seen:?}",
                String::from_utf8_lossy(&out)
            ),
            Ok(ev) => match ev {
                Ok(Some(Event::Output(o))) => out.extend(o.data),
                Ok(Some(Event::Exited(e))) => break e,
                other => seen.push(format!("{other:?}").chars().take(300).collect::<String>()),
            },
        }
    };
    let text = String::from_utf8_lossy(&out);
    assert!(
        text.contains("got:hi") && text.contains("got:quit"),
        "{text}"
    );
    assert_eq!(exited.exit_code, Some(0));
}

#[tokio::test]
async fn wrong_secret_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let execd = Execd::spawn(dir.path());
    let err = ExecClient::connect(&execd.ready.endpoint, b"nope")
        .await
        .unwrap_err();
    assert!(
        matches!(err, Error::Exec { ref code, .. } if code == "UNAUTHENTICATED"),
        "{err}"
    );
}

/// M4.5 / docs/54 fault 12 ("terminal broker killed with active PTY"),
/// docs/19 "terminal session ID + last acknowledged output cursor": the
/// broker keeps every session's metadata beside its output log, so a broker
/// that is hard-killed with a running session comes back knowing it — the
/// session is LOST (its process died with the broker, its exit unknown), its
/// output replays exactly from any cursor and is sealed under an OutputRef,
/// a retry of the same request id replays it rather than starting again,
/// and a session that had exited stays EXITED with its exit code. The ready
/// file names the live broker and goes away with it; a broker with no client
/// past its orphan grace stops its processes and exits.
#[tokio::test]
async fn qual_m4_5_a_killed_broker_comes_back_with_its_durable_sessions_and_an_orphan_exits() {
    let dir = tempfile::tempdir().unwrap();
    let mut execd = Execd::spawn(dir.path());
    let ready_path = dir.path().join("execd.ready");
    let ready = std::fs::read_to_string(&ready_path).unwrap();
    assert_eq!(
        ReadyLine::parse(ready.trim()).unwrap().endpoint,
        execd.ready.endpoint,
        "the ready file names the live broker"
    );
    // One session that exits, one that keeps running.
    let mut a = execd.client().await;
    a.exec(req("short", "echo-args", &["hello"])).await.unwrap();
    let (short_id, _, _, exited) = run_to_exit(&mut a).await;
    assert_eq!(exited.exit_code, Some(3), "the role exits 3 on purpose");
    a.exec(req("long", "ticker", &[])).await.unwrap();
    let Event::Started(s) = a.next().await.unwrap().unwrap() else {
        panic!()
    };
    let long_id = s.session_id;
    let mut seen: Vec<(u64, Vec<u8>)> = Vec::new();
    while seen.len() < 3 {
        if let Event::Output(o) = a.next().await.unwrap().unwrap() {
            seen.push((o.cursor, o.data));
        }
    }
    drop(a);
    // Hard-kill the broker with the ticker running.
    execd.child.kill().unwrap();
    let _ = execd.child.wait();
    tokio::time::sleep(Duration::from_millis(200)).await;
    let execd2 = Execd::spawn(dir.path());
    let mut b = execd2.client().await;
    b.list().await.unwrap();
    let Event::Sessions(list) = b.next().await.unwrap().unwrap() else {
        panic!()
    };
    let long = list
        .iter()
        .find(|s| s.session_id == long_id)
        .expect("recovered");
    assert!(!long.running, "{long:?}");
    assert_eq!(long.status, "LOST", "{long:?}");
    assert_eq!(long.exit_code, None);
    assert!(long.bytes_so_far >= seen.iter().map(|(_, d)| d.len() as u64).sum::<u64>());
    let short = list
        .iter()
        .find(|s| s.session_id == short_id)
        .expect("recovered");
    assert_eq!(
        (short.status.as_str(), short.exit_code),
        ("EXITED", Some(3)),
        "{short:?}"
    );
    // The lost session's log replays exactly from any cursor and is sealed.
    let resume_from = seen[1].0 + seen[1].1.len() as u64;
    b.attach(&long_id, resume_from).await.unwrap();
    let mut replay = Vec::new();
    let exit = loop {
        match b.next().await.unwrap().unwrap() {
            Event::Output(o) => replay.push((o.cursor, o.data)),
            Event::Exited(e) => break e,
            _ => {}
        }
    };
    assert_eq!(replay[0], seen[2], "exact continuation from the cursor");
    assert_eq!(exit.output_ref.len(), 64);
    assert_eq!(exit.exit_code, None, "the exit is unknown, never invented");
    let object = dir
        .path()
        .join("objects")
        .join(&exit.output_ref[..2])
        .join(&exit.output_ref[2..]);
    let bytes = std::fs::read(object).unwrap();
    assert_eq!(hex::encode(Sha256::digest(&bytes)), exit.output_ref);
    assert!(bytes.starts_with(&seen[0].1), "the object is the log");
    // A retry of the request replays the lost session; it does not start again.
    b.exec(req("long", "ticker", &[])).await.unwrap();
    let Event::Started(s2) = b.next().await.unwrap().unwrap() else {
        panic!()
    };
    assert!(s2.replayed && s2.session_id == long_id, "{s2:?}");
    drop(b);
    drop(execd2);
    // Orphan grace: a broker nobody connects to stops its running process and
    // exits, removing its ready file.
    let mut orphan = Command::new(env!("CARGO_BIN_EXE_modbit-execd"))
        .arg("--data-dir")
        .arg(dir.path())
        .arg("--orphan-grace-secs")
        .arg("1")
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .unwrap();
    let mut lines = BufReader::new(orphan.stdout.take().unwrap()).lines();
    let ready = loop {
        let l = lines.next().expect("ready line").unwrap();
        if let Some(r) = ReadyLine::parse(&l) {
            break r;
        }
    };
    std::thread::spawn(move || for _ in lines {});
    let mut c = ExecClient::connect(
        &ready.endpoint,
        &decode_hex(&ready.boot_secret_hex).unwrap(),
    )
    .await
    .unwrap();
    c.exec(req("orphaned", "ticker", &[])).await.unwrap();
    let Event::Started(s3) = c.next().await.unwrap().unwrap() else {
        panic!()
    };
    assert!(!s3.replayed);
    drop(c);
    let deadline = Instant::now() + Duration::from_secs(15);
    let status = loop {
        if let Some(st) = orphan.try_wait().unwrap() {
            break st;
        }
        assert!(
            Instant::now() < deadline,
            "the orphaned broker did not exit"
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    };
    assert!(status.success(), "{status:?}");
    assert!(
        !ready_path.exists(),
        "the ready file goes away with the broker"
    );
    // The next broker sees that session as cancelled, not lost: the orphan
    // stopped it on purpose and said so.
    let execd3 = Execd::spawn(dir.path());
    let mut d = execd3.client().await;
    d.list().await.unwrap();
    let Event::Sessions(list) = d.next().await.unwrap().unwrap() else {
        panic!()
    };
    let orphaned = list.iter().find(|s| s.session_id == s3.session_id).unwrap();
    assert_eq!(orphaned.status, "EXITED", "{orphaned:?}");
    d.attach(&s3.session_id, 0).await.unwrap();
    let exit = loop {
        if let Event::Exited(e) = d.next().await.unwrap().unwrap() {
            break e;
        }
    };
    assert!(exit.cancelled, "{exit:?}");
}

/// M9.6 (docs/52 "Shell injection"): argv-first execution. Every hostile
/// argument — metacharacters, command substitution, quoting, newlines, a
/// leading dash, an environment reference, a NUL-free control character —
/// reaches the process as exactly one literal token; no command named inside
/// it runs (the canary file it would create stays absent) and the exit code
/// is the child's, so an injection could not even fake success.
#[tokio::test]
async fn hostile_arguments_reach_the_process_as_literal_tokens_and_nothing_inside_them_runs() {
    let dir = tempfile::tempdir().unwrap();
    let canary = tempfile::tempdir().unwrap();
    let canary_file = canary.path().join("owned");
    let canary_text = canary_file.to_string_lossy().into_owned();
    let hostile: Vec<String> = vec![
        format!("; touch {canary_text}"),
        format!("$(touch {canary_text})"),
        format!("`touch {canary_text}`"),
        format!("| touch {canary_text} #"),
        format!("&& touch {canary_text}"),
        format!("\ntouch {canary_text}\n"),
        "\"quoted; arg\"".into(),
        "'single' \"double\"".into(),
        "-rf".into(),
        "--".into(),
        "$HOME".into(),
        "%PATH%".into(),
        "a\tb\x1b[31m".into(),
        "🚀 unicode ; still one token".into(),
    ];
    let execd = Execd::spawn(dir.path());
    let mut c = execd.client().await;
    let refs: Vec<&str> = hostile.iter().map(String::as_str).collect();
    c.exec(req("hostile", "echo-args", &refs)).await.unwrap();
    let (_, out, _, exited) = run_to_exit(&mut c).await;
    let text = String::from_utf8(out).unwrap();
    // The child prints its args with Debug formatting: every token, verbatim.
    let expected = format!("args={hostile:?}");
    assert!(
        text.starts_with(&expected),
        "arguments were not passed literally:\n{text}\nexpected prefix:\n{expected}"
    );
    assert_eq!(
        exited.exit_code,
        Some(3),
        "the child's own exit code, {exited:?}"
    );
    assert!(
        !canary_file.exists(),
        "a command named inside an argument ran: {canary_text}"
    );
    // The same tokens through a shell would run them: the control is argv,
    // not an escaping layer, and the broker has no shell mode to reach for.
    assert!(
        !text.contains("HOME_SET=true") || std::env::var("HOME").is_err(),
        "environment is explicit"
    );
}

// ---------------------------------------------------------------------
// FIX-20: bounded replay window, indexed log, per-owner sessions.
// ---------------------------------------------------------------------

const MIB: u64 = 1024 * 1024;

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

fn noisy_req(id: &str, role: &str, mib: u64) -> ExecRequest {
    let mut r = req(id, role, &[]);
    r.env
        .insert("MODBIT_EXECD_TEST_MIB".into(), mib.to_string());
    r
}

/// Total size of every file under `path`.
fn dir_bytes(path: &Path) -> u64 {
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

/// Resident memory of a process in KiB (Unix `ps`); `None` elsewhere.
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

async fn list_of(c: &mut ExecClient) -> Vec<modbit_terminal::SessionInfo> {
    c.list().await.unwrap();
    loop {
        if let Event::Sessions(l) = c.next().await.unwrap().unwrap() {
            return l;
        }
    }
}

/// Read a session from `from` until its exit, checking every chunk against
/// the `noisy` stream; returns (bytes read, the exit).
async fn replay_noisy(
    c: &mut ExecClient,
    sid: &str,
    from: u64,
) -> (u64, modbit_terminal::ProcessExited) {
    c.attach(sid, from).await.unwrap();
    let mut next = from;
    loop {
        match c.next().await.unwrap().expect("broker closed") {
            Event::Output(o) => {
                assert_eq!(o.cursor, next, "replay has no gap or overlap");
                assert_eq!(
                    o.data,
                    noisy_bytes(o.cursor, o.data.len()),
                    "bytes at cursor {}",
                    o.cursor
                );
                next += o.data.len() as u64;
            }
            Event::Exited(e) => return (next - from, e),
            _ => {}
        }
    }
}

/// FIX-20: a noisy real process (50 MiB) under a 4 MiB replay window. Disk
/// and broker memory stay bounded; a cursor inside the window replays
/// byte-exact (also after a hard broker restart); one outside it is the
/// typed `CURSOR_EXPIRED`; the sealed object is the retained tail, named by
/// its digest, and says where it starts.
#[tokio::test]
async fn fix_20_noisy_process_keeps_disk_and_memory_bounded_and_replay_is_exact_inside_the_window()
{
    let dir = tempfile::tempdir().unwrap();
    let args = [
        "--replay-window-bytes",
        "4194304",
        "--segment-bytes",
        "1048576",
    ];
    let mut execd = Execd::spawn_with(dir.path(), &args);
    let pid = execd.child.id();
    let baseline = rss_kib(pid);
    // The broker's memory and disk are sampled from a task of their own, so
    // measuring never slows the reader down.
    let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let sampler = {
        let (stop, root) = (std::sync::Arc::clone(&stop), dir.path().to_owned());
        tokio::spawn(async move {
            let (mut rss, mut disk) = (0u64, 0u64);
            while !stop.load(std::sync::atomic::Ordering::SeqCst) {
                rss = rss.max(rss_kib(pid).unwrap_or(0));
                disk = disk.max(dir_bytes(&root));
                tokio::time::sleep(Duration::from_millis(15)).await;
            }
            (rss, disk)
        })
    };
    let mut c = execd.client().await;
    c.exec(noisy_req("noisy", "noisy", 50)).await.unwrap();
    let mut sid = String::new();
    // A live reader that keeps up sees the exit; one that falls more than a
    // window behind is told so with the typed error, never given wrong bytes.
    loop {
        match c.next().await {
            Ok(Some(Event::Started(s))) => sid = s.session_id,
            Ok(Some(Event::Output(_))) => {}
            Ok(Some(Event::Exited(_))) => break,
            Err(Error::Exec { code, .. }) if code == "CURSOR_EXPIRED" => break,
            other => panic!("live follower: {other:?}"),
        }
    }
    let mut d = execd.client().await;
    let info = loop {
        let info = list_of(&mut d).await.into_iter().next().unwrap();
        if info.status == "EXITED" {
            break info;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    };
    stop.store(true, std::sync::atomic::Ordering::SeqCst);
    let (peak_rss, mut peak_disk) = sampler.await.unwrap();
    eprintln!(
        "fix_20 noisy: 50 MiB written; peak disk {peak_disk} B; broker RSS {baseline:?} -> {peak_rss} KiB"
    );
    // Everything still retained, checked byte for byte against the stream.
    let (_, exited) = replay_noisy(&mut d, &sid, info.oldest_cursor).await;
    assert_eq!(exited.total_bytes, 50 * MIB);
    // The retained tail: at least the window, at most window + one segment.
    let retained = exited.total_bytes - exited.retained_from;
    assert!(exited.retained_from > 0, "the head was dropped: {exited:?}");
    assert!(
        (4 * MIB..=5 * MIB).contains(&retained),
        "retained {retained} bytes"
    );
    let object = dir
        .path()
        .join("objects")
        .join(&exited.output_ref[..2])
        .join(&exited.output_ref[2..]);
    let sealed = std::fs::read(&object).unwrap();
    assert_eq!(sealed.len() as u64, retained);
    assert_eq!(hex::encode(Sha256::digest(&sealed)), exited.output_ref);
    assert_eq!(
        sealed,
        noisy_bytes(exited.retained_from, retained as usize),
        "the object is exactly the retained tail of the stream"
    );
    // Disk: 50 MiB written, a few window-sizes held (log segments + sealed
    // object + indexes), never the whole stream.
    peak_disk = peak_disk.max(dir_bytes(dir.path()));
    assert!(peak_disk < 12 * MIB, "disk peaked at {peak_disk} bytes");
    // Memory of the broker: a handful of MiB over where it started.
    if let Some(base) = baseline {
        assert!(
            peak_rss < base + 48 * 1024,
            "broker RSS peaked at {peak_rss} KiB over a {base} KiB start"
        );
    }
    // The window as the broker reports it.
    assert_eq!(info.oldest_cursor, exited.retained_from);
    let oldest = info.oldest_cursor;
    // Inside the window: byte-exact from a cursor in the middle of a record.
    let (n, e2) = replay_noisy(&mut d, &sid, oldest + 100).await;
    assert_eq!(n, exited.total_bytes - oldest - 100);
    assert_eq!(e2.output_ref, exited.output_ref);
    // Outside it: the typed error naming where the window starts.
    for stale in [0, 4 * MIB, oldest - 1] {
        d.attach(&sid, stale).await.unwrap();
        match d.next().await {
            Err(Error::Exec { code, message }) => {
                assert_eq!(code, "CURSOR_EXPIRED", "{message}");
                assert!(
                    message.contains(&format!("oldest_cursor={oldest}")),
                    "{message}"
                );
            }
            other => panic!("cursor {stale}: expected CURSOR_EXPIRED, got {other:?}"),
        }
    }
    // A hard restart of the broker keeps the same window and the same bytes.
    drop((c, d));
    execd.child.kill().unwrap();
    let _ = execd.child.wait();
    let execd2 = Execd::spawn_with(dir.path(), &args);
    let mut e = execd2.client().await;
    let info = list_of(&mut e).await.into_iter().next().unwrap();
    assert_eq!(
        (info.status.as_str(), info.oldest_cursor),
        ("EXITED", oldest)
    );
    let (n, e3) = replay_noisy(&mut e, &sid, oldest + 4096).await;
    assert_eq!(n, exited.total_bytes - oldest - 4096);
    assert_eq!(e3.output_ref, exited.output_ref);
    assert_eq!(e3.retained_from, exited.retained_from);
}

/// FIX-20: a broker killed with a noisy process running comes back with the
/// segmented log it left: LOST, the window intact, replay byte-exact from
/// the oldest cursor, and the sealed object is the retained tail.
#[tokio::test]
async fn fix_20_a_killed_broker_recovers_a_segmented_log_and_seals_the_retained_tail() {
    let dir = tempfile::tempdir().unwrap();
    let args = [
        "--replay-window-bytes",
        "2097152",
        "--segment-bytes",
        "524288",
    ];
    let mut execd = Execd::spawn_with(dir.path(), &args);
    let mut a = execd.client().await;
    a.exec(noisy_req("live", "noisy-live", 12)).await.unwrap();
    let Event::Started(s) = a.next().await.unwrap().unwrap() else {
        panic!()
    };
    let sid = s.session_id;
    let deadline = Instant::now() + Duration::from_secs(60);
    let mut b = execd.client().await;
    while list_of(&mut b).await[0].bytes_so_far < 12 * MIB {
        assert!(
            Instant::now() < deadline,
            "the process did not write 12 MiB"
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    drop((a, b));
    execd.child.kill().unwrap();
    let _ = execd.child.wait();
    let execd2 = Execd::spawn_with(dir.path(), &args);
    let mut c = execd2.client().await;
    let info = list_of(&mut c).await.into_iter().next().unwrap();
    assert_eq!(info.status, "LOST", "{info:?}");
    assert_eq!(info.bytes_so_far, 12 * MIB);
    assert!(info.oldest_cursor > 0 && info.oldest_cursor % 64 != 1);
    let (n, exit) = replay_noisy(&mut c, &sid, info.oldest_cursor + 7).await;
    assert_eq!(n, 12 * MIB - info.oldest_cursor - 7);
    assert_eq!(exit.exit_code, None, "the exit is unknown, never invented");
    assert_eq!(exit.retained_from, info.oldest_cursor);
    let sealed = std::fs::read(
        dir.path()
            .join("objects")
            .join(&exit.output_ref[..2])
            .join(&exit.output_ref[2..]),
    )
    .unwrap();
    assert_eq!(
        sealed,
        noisy_bytes(info.oldest_cursor, (12 * MIB - info.oldest_cursor) as usize)
    );
    // The window lives on: a cursor before it is still the typed error.
    c.attach(&sid, 0).await.unwrap();
    assert!(matches!(
        c.next().await,
        Err(Error::Exec { ref code, .. }) if code == "CURSOR_EXPIRED"
    ));
}

/// FIX-20: many tiny records, the shape that made every wake re-read the
/// whole index. A live follower gets all 100 000 lines exactly and in time,
/// and a replay from zero afterwards is the same bytes.
#[tokio::test]
async fn fix_20_chatty_process_is_followed_and_replayed_through_the_index() {
    let dir = tempfile::tempdir().unwrap();
    let execd = Execd::spawn(dir.path());
    let mut c = execd.client().await;
    let mut r = req("chatty", "chatty", &[]);
    r.env
        .insert("MODBIT_EXECD_TEST_LINES".into(), "100000".into());
    c.exec(r).await.unwrap();
    let started = Instant::now();
    let (mut sid, mut live) = (String::new(), Vec::new());
    let exited = loop {
        match tokio::time::timeout(Duration::from_secs(120), c.next())
            .await
            .expect("the follower stalled")
            .unwrap()
            .unwrap()
        {
            Event::Started(s) => sid = s.session_id,
            Event::Output(o) => {
                assert_eq!(o.cursor, live.len() as u64);
                live.extend(o.data);
            }
            Event::Exited(e) => break e,
            _ => {}
        }
    };
    let took = started.elapsed();
    let expected: Vec<u8> = (0..100_000)
        .flat_map(|i| format!("{i:07}\n").into_bytes())
        .collect();
    assert_eq!(live, expected);
    assert_eq!(exited.total_bytes, expected.len() as u64);
    assert!(took < Duration::from_secs(60), "followed in {took:?}");
    let mut d = execd.client().await;
    d.attach(&sid, 0).await.unwrap();
    let mut replay = Vec::new();
    loop {
        match d.next().await.unwrap().unwrap() {
            Event::Output(o) => {
                assert_eq!(o.cursor, replay.len() as u64);
                replay.extend(o.data);
            }
            Event::Exited(_) => break,
            _ => {}
        }
    }
    assert_eq!(replay, expected);
}

async fn expect_code(c: &mut ExecClient, code: &str, what: &str) {
    match tokio::time::timeout(Duration::from_secs(10), c.next()).await {
        Ok(Err(Error::Exec { code: got, message })) => {
            assert_eq!(got, code, "{what}: {message}");
        }
        other => panic!("{what}: expected {code}, got {other:?}"),
    }
}

/// FIX-20: a session belongs to the task that started it. Another task is
/// not listed it, cannot attach, write stdin, cancel, or replay its request
/// id; the owner and the host (the user's own terminal lease) can; the
/// owner survives a broker restart.
#[tokio::test]
async fn fix_20_a_second_task_cannot_see_read_write_or_cancel_another_tasks_session() {
    let dir = tempfile::tempdir().unwrap();
    let mut execd = Execd::spawn(dir.path());
    let mut owner = execd.client().await.act_as("task:alpha");
    let mut r = req("owned", "ticker", &[]);
    r.stdin_mode = "open".into();
    owner.exec(r.clone()).await.unwrap();
    let Event::Started(s) = owner.next().await.unwrap().unwrap() else {
        panic!()
    };
    let sid = s.session_id;
    // Another task: not listed, and refused everything else.
    let mut other = execd.client().await.act_as("task:beta");
    assert!(list_of(&mut other).await.is_empty(), "beta lists nothing");
    other.attach(&sid, 0).await.unwrap();
    expect_code(&mut other, "SESSION_NOT_OWNED", "attach").await;
    other.write_stdin(&sid, b"x").await.unwrap();
    expect_code(&mut other, "SESSION_NOT_OWNED", "stdin").await;
    other.cancel(&sid).await.unwrap();
    expect_code(&mut other, "SESSION_NOT_OWNED", "cancel").await;
    other.exec(r.clone()).await.unwrap();
    expect_code(&mut other, "SESSION_NOT_OWNED", "replaying the request id").await;
    // The refused cancel did nothing: the real process is still running.
    tokio::time::sleep(Duration::from_millis(150)).await;
    let mut o2 = execd.client().await.act_as("task:alpha");
    let mine = list_of(&mut o2).await;
    assert_eq!(mine.len(), 1);
    assert!(mine[0].running, "{:?}", mine[0]);
    assert_eq!(mine[0].owner, "task:alpha");
    // A task with no sessions of its own is not mistaken for the host.
    let mut third = execd.client().await.act_as("task:gamma");
    assert!(list_of(&mut third).await.is_empty());
    // The host sees every session and may read it.
    let mut host = execd.client().await;
    assert_eq!(list_of(&mut host).await.len(), 1);
    host.attach(&sid, 0).await.unwrap();
    assert!(matches!(
        host.next().await.unwrap().unwrap(),
        Event::Output(_)
    ));
    // Ownership is durable: after a hard restart the same refusals hold.
    drop((owner, other, o2, third, host));
    execd.child.kill().unwrap();
    let _ = execd.child.wait();
    let execd2 = Execd::spawn(dir.path());
    let mut other = execd2.client().await.act_as("task:beta");
    assert!(list_of(&mut other).await.is_empty());
    other.attach(&sid, 0).await.unwrap();
    expect_code(&mut other, "SESSION_NOT_OWNED", "attach after restart").await;
    let mut owner = execd2.client().await.act_as("task:alpha");
    owner.attach(&sid, 0).await.unwrap();
    assert!(matches!(
        owner.next().await.unwrap().unwrap(),
        Event::Output(_)
    ));
    // A session predating owners (no owner on disk) belongs to the host only.
    let meta = dir.path().join("sessions").join(format!("{sid}.json"));
    let mut json: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&meta).unwrap()).unwrap();
    assert_eq!(json["owner"], "task:alpha");
    json.as_object_mut().unwrap().remove("owner");
    drop((owner, other));
    drop(execd2);
    std::fs::write(&meta, serde_json::to_vec(&json).unwrap()).unwrap();
    let execd3 = Execd::spawn(dir.path());
    let mut alpha = execd3.client().await.act_as("task:alpha");
    assert!(list_of(&mut alpha).await.is_empty());
    let mut host = execd3.client().await;
    assert_eq!(list_of(&mut host).await[0].owner, "");
}

/// FIX-20: finished sessions are kept up to a count and dropped oldest
/// first, with their logs and metadata; running sessions are never dropped.
#[tokio::test]
async fn fix_20_finished_sessions_are_retained_by_count_and_their_logs_are_deleted() {
    let dir = tempfile::tempdir().unwrap();
    let execd = Execd::spawn_with(dir.path(), &["--retain-sessions", "3"]);
    let mut c = execd.client().await;
    let mut ids = Vec::new();
    for i in 0..6 {
        c.exec(req(&format!("short-{i}"), "echo-args", &["x"]))
            .await
            .unwrap();
        let (sid, _, _, _) = run_to_exit(&mut c).await;
        ids.push(sid);
        // Distinct end times, so "oldest first" is well defined.
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    let listed: Vec<String> = list_of(&mut c)
        .await
        .into_iter()
        .map(|s| s.session_id)
        .collect();
    assert_eq!(listed.len(), 3, "{listed:?}");
    for kept in &ids[3..] {
        assert!(listed.contains(kept), "{kept} is among the newest");
    }
    let leftovers: Vec<String> = std::fs::read_dir(dir.path().join("sessions"))
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| ids[..3].iter().any(|gone| n.starts_with(gone.as_str())))
        .collect();
    assert!(leftovers.is_empty(), "pruned sessions left {leftovers:?}");
    // A pruned session is unknown, and its request id starts a fresh one.
    c.attach(&ids[0], 0).await.unwrap();
    expect_code(&mut c, "UNKNOWN_SESSION", "attach to a pruned session").await;
    c.exec(req("short-0", "echo-args", &["x"])).await.unwrap();
    let Event::Started(s) = c.next().await.unwrap().unwrap() else {
        panic!()
    };
    assert!(!s.replayed);
}
