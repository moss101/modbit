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
        let mut child = Command::new(env!("CARGO_BIN_EXE_modbit-execd"))
            .arg("--data-dir")
            .arg(dir)
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
            Event::Sessions(_) => {}
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
            Event::Sessions(_) => {}
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

#[cfg(unix)]
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
    let (_, out, _, exited) = run_to_exit(&mut c).await;
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
