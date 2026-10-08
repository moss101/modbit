//! PX-132 on the real broker: the listening sockets of a session's own
//! process tree are reported with the session that owns them, and no other
//! socket of the machine is. Real: the `modbit-execd` binary over its real
//! socket, real child processes holding real listening sockets (this
//! package's `execd_child` example in its `listen` role, started directly and
//! behind a shell), and a listener of the test process itself, which belongs
//! to nobody's session. Unix only: the process-table and socket queries have
//! one implementation per platform and the Windows one is not exercised here.
#![cfg(unix)]

use std::io::{BufRead, BufReader};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use modbit_protocol::local::{ReadyLine, decode_hex};
use modbit_protocol::v1::{ExecRequest, SessionListeners};
use modbit_terminal::{Event, ExecClient};

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
    let deps = std::env::current_exe().unwrap();
    let debug = deps.parent().unwrap().parent().unwrap();
    let p = debug.join("examples").join("execd_child");
    assert!(
        p.exists(),
        "{} missing; run `cargo build -p modbit-execd --examples`",
        p.display()
    );
    p.to_string_lossy().into_owned()
}

fn listen_request(id: &str, owner: &str, behind_a_shell: bool) -> ExecRequest {
    let argv = if behind_a_shell {
        // The server is a child of the shell, not the session's own process.
        vec![
            "sh".into(),
            "-c".into(),
            format!("'{}' & wait", child_bin()),
        ]
    } else {
        vec![child_bin()]
    };
    let mut env = std::collections::HashMap::new();
    env.insert("MODBIT_EXECD_TEST_ROLE".to_owned(), "listen".to_owned());
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
        owner: owner.into(),
        pty_rows: 0,
        pty_cols: 0,
    }
}

/// Start a request; returns its session id and the port its server printed.
async fn start_listener(c: &mut ExecClient, req: ExecRequest) -> (String, u16) {
    c.exec(req).await.unwrap();
    let mut sid = String::new();
    let deadline = Instant::now() + Duration::from_secs(20);
    let mut seen = String::new();
    loop {
        assert!(Instant::now() < deadline, "no port printed: {seen:?}");
        match tokio::time::timeout(Duration::from_secs(5), c.next()).await {
            Ok(Ok(Some(Event::Started(s)))) => {
                sid = s.session_id;
                c.attach(&sid, 0).await.unwrap();
            }
            Ok(Ok(Some(Event::Output(o)))) => {
                if o.session_id != sid {
                    continue;
                }
                seen.push_str(&String::from_utf8_lossy(&o.data));
                if let Some(port) = seen
                    .lines()
                    .find_map(|l| l.strip_prefix("listening ")?.trim().parse::<u16>().ok())
                {
                    return (sid, port);
                }
            }
            Ok(Ok(_)) => {}
            other => panic!("{other:?}"),
        }
    }
}

async fn services(c: &mut ExecClient) -> SessionListeners {
    c.list_services().await.unwrap();
    loop {
        if let Event::Listeners(l) = tokio::time::timeout(Duration::from_secs(20), c.next())
            .await
            .expect("the scan answered")
            .unwrap()
            .expect("broker open")
        {
            return l;
        }
    }
}

/// QUAL-PX-132 (broker side): a listener of a session's tree is reported with
/// its session, port, address and command; the same server behind a shell is
/// still the session's; a listener of the test process, which no session
/// started, is not; a task sees only the sessions it owns; a cancelled
/// session reports nothing.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn qual_px_132_the_broker_reports_the_listeners_of_a_sessions_own_tree_and_nothing_else() {
    let dir = tempfile::tempdir().unwrap();
    let execd = Execd::spawn(dir.path());
    // A listener nobody's session started: it is the test process's own.
    let stranger = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let stranger_port = stranger.local_addr().unwrap().port();

    let mut host = execd.client().await;
    let (direct, direct_port) =
        start_listener(&mut host, listen_request("a", "task:A", false)).await;
    let (shelled, shelled_port) =
        start_listener(&mut host, listen_request("b", "task:B", true)).await;

    let all = services(&mut host).await;
    assert_eq!(all.scan_error, "", "{all:?}");
    assert_eq!(all.scanned_sessions, 2);
    let find = |port: u16| all.listeners.iter().find(|l| l.port == u32::from(port));
    let a = find(direct_port).expect("the direct server is reported");
    assert_eq!(a.session_id, direct);
    assert_eq!(a.address, "127.0.0.1");
    assert!(a.command.contains("execd_child"), "{a:?}");
    assert!(a.pid > 0);
    let b = find(shelled_port).expect("the server behind the shell is the session's too");
    assert_eq!(b.session_id, shelled);
    assert!(
        find(stranger_port).is_none(),
        "a listener no session started is never attributed: {all:?}"
    );
    assert!(
        all.listeners.iter().all(|l| l.port != 0),
        "every entry is a real port"
    );

    // A task sees only what it owns.
    let mut task_a = execd.client().await.act_as("task:A");
    let mine = services(&mut task_a).await;
    assert_eq!(mine.listeners.len(), 1, "{mine:?}");
    assert_eq!(mine.listeners[0].port, u32::from(direct_port));

    // The session ends: its server is gone and nothing is reported for it.
    host.cancel(&direct).await.unwrap();
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        let now = services(&mut host).await;
        if now.listeners.iter().all(|l| l.session_id != direct) {
            assert!(
                now.listeners.iter().any(|l| l.session_id == shelled),
                "the other session's server is still there: {now:?}"
            );
            break;
        }
        assert!(Instant::now() < deadline, "still reported after cancel");
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    drop(stranger);
}
