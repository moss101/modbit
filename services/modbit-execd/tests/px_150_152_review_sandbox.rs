//! QUAL-PX-150 and QUAL-PX-152 against the real broker and the real operating
//! system confinement: the review sandbox keeps a confined process out of the
//! user's credential stores and out of its worktree's `.git` and `.modbit`, and a
//! process the sandbox refuses is told so with a typed `SANDBOX_DENIED`, while an
//! ordinary failing command never is.

#![cfg(unix)]

use std::io::{BufRead, BufReader};
use std::path::Path;
use std::process::{Child, Command, Stdio};

use modbit_protocol::local::{ReadyLine, decode_hex};
use modbit_protocol::v1::ExecRequest;
use modbit_terminal::{Event, ExecClient, ProcessExited};

struct Execd {
    child: Child,
    ready: ReadyLine,
}

impl Execd {
    /// The broker, started with `home` as its home directory (the confinement
    /// reads the credential stores relative to it).
    fn spawn(data: &Path, home: &Path) -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_modbit-execd"))
            .arg("--data-dir")
            .arg(data)
            .env("HOME", home)
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

    /// Whether this host confines review processes (macOS seatbelt, Linux seccomp).
    async fn sandbox_kind(&self) -> Option<String> {
        let mut c = self.client().await;
        c.probe_sandbox().await.unwrap();
        loop {
            if let Event::SandboxProbed(p) = c.next().await.unwrap().expect("broker closed") {
                return p.available.then_some(p.kind);
            }
        }
    }
}

impl Drop for Execd {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn request(id: &str, profile: &str, cwd: &Path, argv: &[&str]) -> ExecRequest {
    ExecRequest {
        request_id: id.into(),
        argv: argv.iter().map(|s| (*s).to_owned()).collect(),
        cwd: cwd.to_string_lossy().into_owned(),
        env: [("PATH".to_owned(), "/usr/bin:/bin".to_owned())]
            .into_iter()
            .collect(),
        inherit_env: false,
        timeout_ms: 20_000,
        pty: false,
        stdin_mode: "closed".into(),
        output_budget_bytes: 4096,
        execution_profile: profile.into(),
        capability_lease_id: None,
        terminal_session_id: None,
        owner: String::new(),
        pty_rows: 0,
        pty_cols: 0,
    }
}

async fn run(c: &mut ExecClient, r: ExecRequest) -> (String, ProcessExited) {
    c.exec(r).await.unwrap();
    let mut out = String::new();
    loop {
        match c.next().await.unwrap().expect("broker closed") {
            Event::Output(o) => out.push_str(&String::from_utf8_lossy(&o.data)),
            Event::Exited(e) => return (out, e),
            _ => {}
        }
    }
}

fn sh(c_id: &str, cwd: &Path, script: &str) -> ExecRequest {
    request(c_id, "review_isolated", cwd, &["sh", "-c", script])
}

struct Rig {
    _data: tempfile::TempDir,
    home: tempfile::TempDir,
    wt: tempfile::TempDir,
    execd: Execd,
}

fn rig() -> Rig {
    let data = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    let wt = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(home.path().join(".ssh")).unwrap();
    std::fs::write(home.path().join(".ssh/id_test"), "TOPSECRETVALUE\n").unwrap();
    std::fs::write(home.path().join("notes.txt"), "plain home file\n").unwrap();
    std::fs::create_dir_all(wt.path().join(".git")).unwrap();
    std::fs::create_dir_all(wt.path().join(".modbit")).unwrap();
    let execd = Execd::spawn(data.path(), home.path());
    Rig {
        _data: data,
        home,
        wt,
        execd,
    }
}

#[tokio::test]
async fn qual_px_152_an_ordinary_failure_is_never_a_sandbox_denial() {
    let rig = rig();
    if rig.execd.sandbox_kind().await.is_none() {
        eprintln!("skipped: this host has no review sandbox");
        return;
    }
    let mut c = rig.execd.client().await;
    // A failing test inside the sandbox: its own reasons, no refusal.
    let (out, e) = run(
        &mut c,
        sh("t1", rig.wt.path(), "echo '3 tests failed'; exit 1"),
    )
    .await;
    assert_eq!(e.exit_code, Some(1), "{out}");
    assert!(e.sandbox_denied.is_none(), "{e:?}");
    // A plain permission problem is not the sandbox either.
    std::fs::write(rig.wt.path().join("locked"), "x").unwrap();
    let (out, e) = run(
        &mut c,
        sh(
            "t2",
            rig.wt.path(),
            "chmod 000 locked; cat locked; echo done",
        ),
    )
    .await;
    assert!(out.contains("ermission denied"), "{out}");
    assert!(e.sandbox_denied.is_none(), "{e:?}");
    // The same words from a process that is not confined are not a denial,
    // and neither are they when the process succeeded.
    let (_, e) = run(
        &mut c,
        request(
            "t3",
            "local_trusted",
            rig.wt.path(),
            &["sh", "-c", "echo 'x: Operation not permitted' >&2; exit 1"],
        ),
    )
    .await;
    assert!(e.sandbox_denied.is_none(), "{e:?}");
    let (_, e) = run(
        &mut c,
        sh("t4", rig.wt.path(), "echo 'x: Operation not permitted' >&2"),
    )
    .await;
    assert_eq!(e.exit_code, Some(0));
    assert!(e.sandbox_denied.is_none(), "{e:?}");
}

#[tokio::test]
async fn qual_px_152_a_refused_network_operation_returns_the_typed_denial() {
    let rig = rig();
    if rig.execd.sandbox_kind().await.is_none() {
        eprintln!("skipped: this host has no review sandbox");
        return;
    }
    let mut c = rig.execd.client().await;
    let (_, e) = run(
        &mut c,
        request(
            "n1",
            "review_isolated",
            rig.wt.path(),
            &[
                "/usr/bin/python3",
                "-c",
                "import socket; socket.create_connection(('127.0.0.1', 9), timeout=2)",
            ],
        ),
    )
    .await;
    let d = e.sandbox_denied.as_ref().unwrap_or_else(|| panic!("{e:?}"));
    assert_eq!(d.code, "SANDBOX_DENIED");
    assert_eq!(Some(d.exit_code), e.exit_code);
    assert!(d.excerpt.to_lowercase().contains("operation not permitted"));
    assert!(
        d.excerpt.len() <= 3 * 200 + 2,
        "bounded: {}",
        d.excerpt.len()
    );
}

#[cfg(target_os = "macos")]
#[tokio::test]
async fn qual_px_150_the_macos_profile_keeps_credential_stores_git_and_modbit_out_of_reach() {
    let rig = rig();
    if rig.execd.sandbox_kind().await.as_deref() != Some("seatbelt") {
        eprintln!("skipped: no seatbelt on this host");
        return;
    }
    let mut c = rig.execd.client().await;
    let home = rig.home.path().to_string_lossy().into_owned();

    // The worktree is writable, and an ordinary home file is readable.
    let (out, e) = run(
        &mut c,
        sh(
            "w1",
            rig.wt.path(),
            &format!("echo built > out.txt && cat {home}/notes.txt"),
        ),
    )
    .await;
    assert_eq!(e.exit_code, Some(0), "{out}");
    assert!(out.contains("plain home file"), "{out}");
    assert!(e.sandbox_denied.is_none(), "{e:?}");
    assert_eq!(
        std::fs::read_to_string(rig.wt.path().join("out.txt")).unwrap(),
        "built\n"
    );

    // A denied write outside the worktree and a denied read of a credential
    // store each return the typed code, the exit code and an excerpt.
    let (_, w) = run(
        &mut c,
        sh("w2", rig.wt.path(), "echo x > /etc/modbit-px150-probe"),
    )
    .await;
    let d = w.sandbox_denied.as_ref().unwrap_or_else(|| panic!("{w:?}"));
    assert_eq!(d.code, "SANDBOX_DENIED");
    assert_eq!(Some(d.exit_code), w.exit_code);
    assert!(
        d.excerpt.contains("/etc/modbit-px150-probe"),
        "{}",
        d.excerpt
    );
    assert!(!std::path::Path::new("/etc/modbit-px150-probe").exists());

    let (out, r) = run(
        &mut c,
        sh("r1", rig.wt.path(), &format!("cat {home}/.ssh/id_test")),
    )
    .await;
    let d = r.sandbox_denied.as_ref().unwrap_or_else(|| panic!("{r:?}"));
    assert_eq!(d.code, "SANDBOX_DENIED");
    assert!(
        !out.contains("TOPSECRETVALUE") && !d.excerpt.contains("TOPSECRETVALUE"),
        "the secret reached neither the output nor the excerpt"
    );

    // `.git` and `.modbit` inside the writable root are read-only.
    for (id, target) in [("g1", ".git/HEAD"), ("g2", ".modbit/state")] {
        let (_, e) = run(&mut c, sh(id, rig.wt.path(), &format!("echo x > {target}"))).await;
        assert!(e.sandbox_denied.is_some(), "{target}: {e:?}");
        assert!(!rig.wt.path().join(target).exists(), "{target}");
    }

    // The temp directory is writable, /dev is writable only for the nodes a
    // build needs.
    let (out, e) = run(
        &mut c,
        sh(
            "d1",
            rig.wt.path(),
            "echo ok > /tmp/modbit-px150-tmp && rm /tmp/modbit-px150-tmp && echo hidden > /dev/null && echo fine",
        ),
    )
    .await;
    assert_eq!(e.exit_code, Some(0), "{out}");
    let (_, e) = run(
        &mut c,
        sh("d2", rig.wt.path(), "echo x > /dev/modbit-px150-node"),
    )
    .await;
    assert!(e.sandbox_denied.is_some(), "{e:?}");
}

/// The profile takes its paths as parameters: a working directory whose name
/// carries a quote, a parenthesis and a backslash cannot rewrite the policy.
#[cfg(target_os = "macos")]
#[tokio::test]
async fn qual_px_150_a_hostile_working_directory_name_cannot_change_the_profile() {
    let rig = rig();
    if rig.execd.sandbox_kind().await.as_deref() != Some("seatbelt") {
        eprintln!("skipped: no seatbelt on this host");
        return;
    }
    let base = tempfile::tempdir().unwrap();
    let hostile = base
        .path()
        .join("wt\") (allow file-write* (subpath \"/\")) (\"x\\y");
    std::fs::create_dir_all(&hostile).unwrap();
    let mut c = rig.execd.client().await;
    let (out, e) = run(
        &mut c,
        sh(
            "h1",
            &hostile,
            "echo ok > inside.txt && echo x > /etc/modbit-px150-hostile",
        ),
    )
    .await;
    assert!(
        hostile.join("inside.txt").exists(),
        "the real worktree is writable: {out}"
    );
    assert!(e.sandbox_denied.is_some(), "and /etc still is not: {e:?}");
    assert!(!std::path::Path::new("/etc/modbit-px150-hostile").exists());
}
