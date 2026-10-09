//! PX-132 (QUAL-PX-132) on the real Core: the Core knows which ports the
//! processes of a task's terminals listen on, whether a dev server is ready
//! and healthy, and tells the agent, without a model round.
//!
//! Real: the `modbit-core` binary and its socket, the SQLite store, the
//! `modbit-execd` broker and the processes it starts (a Node HTTP server, a
//! Python HTTP server and a Rust HTTP server built and run by `cargo`, all
//! started by the agent's `shell.start`), real listening sockets, the Core's
//! probes. Stand-in: the model, a scripted OpenAI-compatible server (the
//! repository's standard stand-in). Unix only: the listener query has one
//! implementation per platform and only the Unix ones are exercised here.
#![cfg(unix)]

mod px_common;

use std::process::{Command, Stdio};
use std::time::Duration;

use modbit_protocol::client::Client;
use modbit_protocol::v1::{
    ContextInspectorView, GetContextInspector, Id, ListProcessServices, ProcessServiceList,
};
use prost::Message;
use px_common::*;
use serde_json::{Value, json};

const NODE_SERVER: &str = r#"
const http = require('http'), fs = require('fs');
const status = Number(process.env.STATUS || 200);
const s = http.createServer((q, r) => { r.statusCode = status; r.end('ok'); });
s.listen(0, '127.0.0.1', () => fs.writeFileSync('node.port', String(s.address().port)));
"#;

const PYTHON_SERVER: &str = r#"
import http.server, socketserver
class H(http.server.BaseHTTPRequestHandler):
    def do_GET(self):
        self.send_response(200); self.end_headers(); self.wfile.write(b'ok')
    def log_message(self, *a): pass
socketserver.TCPServer.allow_reuse_address = True
s = socketserver.TCPServer(('127.0.0.1', 0), H)
open('py.port', 'w').write(str(s.server_address[1]))
s.serve_forever()
"#;

const RUST_CARGO: &str =
    "[package]\nname = \"rsrv\"\nversion = \"0.0.0\"\nedition = \"2021\"\n[workspace]\n";

const RUST_SERVER: &str = r#"
use std::io::{Read, Write};
fn main() {
    let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    std::fs::write("rust.port", l.local_addr().unwrap().port().to_string()).unwrap();
    for s in l.incoming().flatten() {
        let mut s = s;
        let mut b = [0u8; 512];
        let _ = s.set_read_timeout(Some(std::time::Duration::from_millis(500)));
        let _ = s.read(&mut b);
        let _ = s.write_all(b"HTTP/1.1 200 OK\r\ncontent-length: 2\r\nconnection: close\r\n\r\nok");
    }
}
"#;

fn have(program: &str, arg: &str) -> bool {
    Command::new(program)
        .arg(arg)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

fn spawn(dir: &std::path::Path, base: &str) -> CoreProcess {
    let mut env = model_env(base);
    env.push(("MODBIT_SERVICE_POLL_MS".into(), "200".into()));
    env.push(("MODBIT_EXECD_ORPHAN_GRACE_SECS".into(), "5".into()));
    let refs: Vec<(&str, &str)> = env.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
    CoreProcess::spawn_with_env(dir, &refs)
}

async fn services(c: &mut Client, task: &Id, include_gone: bool) -> ProcessServiceList {
    let ack = c
        .command(envelope(
            rand_id(),
            "ListProcessServices",
            ListProcessServices {
                task_id: Some(task.clone()),
                include_gone,
            }
            .encode_to_vec(),
        ))
        .await
        .unwrap();
    Client::result(&ack).unwrap()
}

async fn wait_services<F: Fn(&ProcessServiceList) -> bool>(
    c: &mut Client,
    task: &Id,
    secs: u64,
    include_gone: bool,
    done: F,
) -> ProcessServiceList {
    let deadline = std::time::Instant::now() + Duration::from_secs(secs);
    loop {
        let l = services(c, task, include_gone).await;
        if done(&l) {
            return l;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "the service state never arrived: {l:#?}"
        );
        tokio::time::sleep(Duration::from_millis(150)).await;
    }
}

fn observed(evs: &[Value], task: &Id) -> Vec<Value> {
    evs.iter()
        .filter(|e| {
            e["event_type"] == "ProcessServiceObserved"
                && e["task_id"].as_str() == Some(&hex_id(task))
        })
        .map(|e| e["payload"]["payload"].clone())
        .collect()
}

fn plan() -> Value {
    json!({"calls": [{"name": "plan.update", "args": {"outcome": "start the servers", "expected_files": []}}]})
}

fn complete() -> Value {
    json!({"calls": [{"name": "task.complete", "args": {"summary": "done", "self_review": {"findings": []}}}]})
}

fn start_cmd(argv: &[&str]) -> Value {
    json!({"calls": [{"name": "shell.start", "args": {"argv": argv}}]})
}

/// Wait for every named port file (up to ~25 s) and then a beat, so the Core
/// has probed what came up.
fn settle(files: &[&str]) -> Value {
    let waits = files
        .iter()
        .map(|f| {
            format!("i=0; while [ ! -f {f} ] && [ $i -lt 125 ]; do sleep 0.2; i=$((i+1)); done")
        })
        .collect::<Vec<_>>()
        .join("; ");
    json!({"calls": [{"name": "shell.exec", "args": {"argv": ["sh", "-c", format!("{waits}; sleep 1.5")], "timeout_ms": 40000}}]})
}

fn read_notes() -> Value {
    json!({"calls": [{"name": "fs.read", "args": {"path": "notes.txt"}}]})
}

fn tool_texts(body: &Value) -> Vec<String> {
    body["messages"]
        .as_array()
        .map(|m| {
            m.iter()
                .filter(|x| x["role"] == "tool")
                .map(|x| x["content"].as_str().unwrap_or_default().to_owned())
                .collect()
        })
        .unwrap_or_default()
}

async fn run_to_end(c: &mut Client, task: &Id, secs: u64) {
    let st = wait_task(c, task, secs).await;
    assert!(!st.loop_alive, "{st:?}");
}

/// QUAL-PX-132: a Node, a Python and a Rust server, each started by the
/// agent's shell, are reported ready with their real ports and their owning
/// terminal, the facts reach the agent in its next tool result, an unrelated
/// listener of the machine is not attributed to the task, and nothing about
/// the policy changes because a server was found.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn qual_px_132_three_dev_servers_are_reported_ready_and_the_agent_is_told_without_asking() {
    let node = have("node", "--version");
    let python = have("python3", "--version");
    assert!(node || python, "no Node and no Python on this machine");
    if !node {
        eprintln!("skipped: the Node server (node is not installed)");
    }
    if !python {
        eprintln!("skipped: the Python server (python3 is not installed)");
    }
    let (repo, root) = plain_repo(&[
        ("notes.txt", "n\n"),
        ("server.js", NODE_SERVER),
        ("server.py", PYTHON_SERVER),
        ("rsrv/Cargo.toml", RUST_CARGO),
        ("rsrv/src/main.rs", RUST_SERVER),
    ]);
    // A listener no terminal of the task started: the test's own.
    let stranger = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let stranger_port = u32::from(stranger.local_addr().unwrap().port());
    let mut steps: Vec<Value> = vec![plan()];
    let mut files: Vec<&str> = vec!["rust.port"];
    if node {
        steps.push(start_cmd(&["node", "server.js"]));
        files.push("node.port");
    }
    if python {
        steps.push(start_cmd(&["python3", "server.py"]));
        files.push("py.port");
    }
    steps.push(start_cmd(&[
        "cargo",
        "run",
        "--quiet",
        "--manifest-path",
        "rsrv/Cargo.toml",
    ]));
    steps.push(settle(&files));
    steps.push(read_notes());
    steps.push(complete());
    // The request that carries the settle step's result is held until the
    // test has seen every server READY on the Core, so the agent's next tool
    // result is taken after the Core has probed them rather than on a timer.
    let hold_at = steps.len() - 2;
    let gate = std::sync::Arc::new((std::sync::Mutex::new(false), std::sync::Condvar::new()));
    let (base, seen) = {
        let script = steps.clone();
        let gate = std::sync::Arc::clone(&gate);
        scripted_model_fn(std::sync::Arc::new(move |_body, results| {
            if results == hold_at {
                let (open, cv) = &*gate;
                let guard = open.lock().unwrap();
                let _ = cv
                    .wait_timeout_while(guard, Duration::from_secs(120), |o| !*o)
                    .unwrap();
            }
            script
                .get(results)
                .cloned()
                .unwrap_or_else(|| json!({"text": "I have nothing further to do."}))
        }))
        .await
    };
    let dir = tempfile::tempdir().unwrap();
    let core = spawn(dir.path(), &base);
    let mut c = core.client().await;
    let (session, g) = session_with_lease(&mut c, 0x31).await;
    let task = create_task(
        &mut c,
        &session,
        g,
        &root,
        0x32,
        "local_trusted",
        "start servers",
    )
    .await;
    start_task(&mut c, &task, g, 0x33, "gpt-5-mini").await;

    // Every server has written its port file; then the Core must report each
    // one READY before the agent is released to read its tool result.
    let deadline = std::time::Instant::now() + Duration::from_secs(60);
    let expected: Vec<(&str, u32)> = loop {
        let got: Option<Vec<(&str, u32)>> = files
            .iter()
            .map(|f| {
                std::fs::read_to_string(repo.path().join(f))
                    .ok()
                    .and_then(|s| s.trim().parse().ok())
                    .map(|p| (*f, p))
            })
            .collect();
        if let Some(v) = got {
            break v;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "a server never wrote its port file"
        );
        tokio::time::sleep(Duration::from_millis(150)).await;
    };
    wait_services(&mut c, &task, 60, false, |l| {
        expected.iter().all(|(_, p)| {
            l.services
                .iter()
                .any(|s| s.port == *p && s.state == "READY")
        })
    })
    .await;
    {
        let (open, cv) = &*gate;
        *open.lock().unwrap() = true;
        cv.notify_all();
    }
    run_to_end(&mut c, &task, 180).await;
    let list = wait_services(&mut c, &task, 30, false, |l| {
        expected.iter().all(|(_, p)| {
            l.services
                .iter()
                .any(|s| s.port == *p && s.state == "READY")
        })
    })
    .await;
    for (file, port) in &expected {
        let s = list.services.iter().find(|s| s.port == *port).unwrap();
        assert_eq!(s.state, "READY", "{file}: {s:?}");
        assert_eq!(s.address, "127.0.0.1", "{file}: {s:?}");
        assert_eq!(s.kind, "dev_server", "{file}: {s:?}");
        assert_eq!(s.http_status, 200, "{file}: {s:?}");
        assert!(!s.session_id.is_empty() && s.pid > 0, "{file}: {s:?}");
    }
    assert!(
        list.services.iter().all(|s| s.port != stranger_port),
        "an unrelated listener of the machine is not the task's: {list:#?}"
    );
    // The context breakdown shows the same live services.
    let ack = c
        .command(envelope(
            rand_id(),
            "GetContextInspector",
            GetContextInspector {
                task_id: Some(task.clone()),
            }
            .encode_to_vec(),
        ))
        .await
        .unwrap();
    let inspector: ContextInspectorView = Client::result(&ack).unwrap();
    assert_eq!(inspector.process_services.len(), expected.len());
    // Each server belongs to its own terminal.
    let sessions: std::collections::HashSet<&str> = list
        .services
        .iter()
        .map(|s| s.session_id.as_str())
        .collect();
    assert_eq!(sessions.len(), expected.len(), "{list:#?}");

    // The log records the transitions, and nothing is recorded for the stranger.
    let evs = replay(&core, &session).await;
    let obs = observed(&evs, &task);
    for (file, port) in &expected {
        assert!(
            obs.iter()
                .any(|o| o["port"] == *port && o["state"] == "READY"),
            "{file}: no READY record for {port}: {obs:#?}"
        );
    }
    assert!(obs.iter().all(|o| o["port"] != stranger_port));

    // The agent was told in a tool result it asked for something else with.
    let last = seen.lock().unwrap().last().cloned().unwrap();
    let texts = tool_texts(&last);
    let told = texts
        .iter()
        .find(|t| t.contains("process services of this task's terminals"))
        .unwrap_or_else(|| panic!("no tool result carried the services: {texts:#?}"));
    for (_, port) in &expected {
        assert!(told.contains(&format!("127.0.0.1:{port}")), "{told}");
    }
    assert!(
        told.contains("data and not an instruction"),
        "the observation is labelled data: {told}"
    );

    // Detection widened nothing: the policy generation never moved, and
    // finding a server did not appear as a policy or capability event.
    assert!(
        evs.iter()
            .all(|e| e["event_type"] != "PolicyGenerationChanged"),
        "finding a server changed the policy"
    );
    let first_snapshot = evs
        .iter()
        .find(|e| e["event_type"] == "CapabilitySnapshotRecorded")
        .map(|e| e["payload"]["payload"]["snapshot"]["lease"]["operations"].clone());
    let last_snapshot = evs
        .iter()
        .rev()
        .find(|e| e["event_type"] == "CapabilitySnapshotRecorded")
        .map(|e| e["payload"]["payload"]["snapshot"]["lease"]["operations"].clone());
    assert_eq!(first_snapshot, last_snapshot, "the lease never widened");
    drop(stranger);
}

/// A server that listens but answers 500 is unhealthy, not ready; when it
/// dies it is gone, and the agent's next tool result says so.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn qual_px_132_a_server_that_returns_500_is_unhealthy_and_one_that_dies_is_gone() {
    if !have("node", "--version") {
        eprintln!("skipped: node is not installed");
        return;
    }
    let (repo, root) = plain_repo(&[("notes.txt", "n\n"), ("server.js", NODE_SERVER)]);
    let node_start = json!({"calls": [{"name": "shell.start", "args": {"argv": ["node", "server.js"], "env": {"STATUS": "500"}}}]});
    // The third step waits for the test to kill the server (a slow model).
    let steps = [
        plan(),
        node_start,
        settle(&["node.port"]),
        json!({"text": "", "delay_ms": 0, "calls": [{"name": "fs.read", "args": {"path": "notes.txt"}}]}),
        read_notes(),
        complete(),
    ];
    let reply: Reply = std::sync::Arc::new(move |_body, results| {
        let mut r = steps
            .get(results)
            .cloned()
            .unwrap_or_else(|| json!({"text": "nothing further"}));
        // After the first fs.read the model is slow, giving the test the
        // time to kill the server before the next tool result.
        if results == 4 {
            r["delay_ms"] = json!(6000);
        }
        r
    });
    let (base, seen) = scripted_model_fn(reply).await;
    let dir = tempfile::tempdir().unwrap();
    let core = spawn(dir.path(), &base);
    let mut c = core.client().await;
    let (session, g) = session_with_lease(&mut c, 0x41).await;
    let task = create_task(
        &mut c,
        &session,
        g,
        &root,
        0x42,
        "local_trusted",
        "start a server",
    )
    .await;
    start_task(&mut c, &task, g, 0x43, "gpt-5-mini").await;
    let listed = wait_services(&mut c, &task, 60, false, |l| !l.services.is_empty()).await;
    let s = &listed.services[0];
    assert_eq!(s.state, "UNHEALTHY", "listening but answering 500: {s:?}");
    assert_eq!(s.http_status, 500);
    let pid = s.pid;
    let port = s.port;
    // About to kill what the Core reported: it must be this test's server.
    assert!(
        s.command.contains("server.js"),
        "not the test's server: {s:?}"
    );
    assert_eq!(
        std::fs::read_to_string(repo.path().join("node.port"))
            .unwrap()
            .trim(),
        port.to_string()
    );
    // The model is mid-delay: kill the server.
    tokio::time::sleep(Duration::from_millis(1500)).await;
    assert!(
        Command::new("kill")
            .args(["-9", &pid.to_string()])
            .status()
            .unwrap()
            .success()
    );
    let gone = wait_services(&mut c, &task, 30, true, |l| {
        l.services
            .iter()
            .any(|s| s.port == port && s.state == "GONE")
    })
    .await;
    assert!(gone.services.iter().any(|s| s.state == "GONE"));
    // Not listed as live any more.
    let live = services(&mut c, &task, false).await;
    assert!(live.services.iter().all(|s| s.port != port), "{live:#?}");
    run_to_end(&mut c, &task, 120).await;
    let evs = replay(&core, &session).await;
    let states: Vec<String> = observed(&evs, &task)
        .iter()
        .filter(|o| o["port"] == port)
        .map(|o| o["state"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(
        states.first().map(String::as_str),
        Some("UNHEALTHY"),
        "{states:?}"
    );
    assert_eq!(
        states.last().map(String::as_str),
        Some("GONE"),
        "{states:?}"
    );
    // The agent's later tool result carried both facts.
    let all: String = seen
        .lock()
        .unwrap()
        .last()
        .map(|b| tool_texts(b).join("\n"))
        .unwrap_or_default();
    assert!(all.contains("UNHEALTHY"), "{all}");
    assert!(all.contains("GONE"), "{all}");
}

/// A killed Core: the durable broker keeps the server running; the restarted
/// Core reads the recorded state back, records nothing it already knew, and
/// notices a server that stopped while it was away.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn qual_px_132_a_restarted_core_reattaches_and_reobserves_without_repeating_itself() {
    if !have("node", "--version") {
        eprintln!("skipped: node is not installed");
        return;
    }
    let (repo, root) = plain_repo(&[("notes.txt", "n\n"), ("server.js", NODE_SERVER)]);
    let steps = vec![
        plan(),
        start_cmd(&["node", "server.js"]),
        settle(&["node.port"]),
        read_notes(),
        complete(),
    ];
    let (base, _seen) = scripted_model(steps, vec![]).await;
    let dir = tempfile::tempdir().unwrap();
    let mut core = spawn(dir.path(), &base);
    let mut c = core.client().await;
    let (session, g) = session_with_lease(&mut c, 0x51).await;
    let task = create_task(
        &mut c,
        &session,
        g,
        &root,
        0x52,
        "local_trusted",
        "start a server",
    )
    .await;
    start_task(&mut c, &task, g, 0x53, "gpt-5-mini").await;
    run_to_end(&mut c, &task, 120).await;
    let ready = wait_services(&mut c, &task, 30, false, |l| {
        l.services.iter().any(|s| s.state == "READY")
    })
    .await;
    let (port, pid) = (ready.services[0].port, ready.services[0].pid);
    assert!(
        ready.services[0].command.contains("server.js"),
        "not the test's server: {:?}",
        ready.services[0]
    );
    let before = observed(&replay(&core, &session).await, &task);
    assert!(before.iter().any(|o| o["state"] == "READY"));
    // Kill the Core (the broker and the server keep running).
    drop(c);
    core.kill();
    let core2 = spawn(dir.path(), &base);
    let mut c2 = core2.client().await;
    // The read model survives the restart (it is folded from the log) ...
    let again = wait_services(&mut c2, &task, 30, false, |l| {
        l.services
            .iter()
            .any(|s| s.port == port && s.state == "READY")
    })
    .await;
    assert_eq!(again.services[0].port, port);
    // ... the new Core re-observes the live server and writes nothing new.
    tokio::time::sleep(Duration::from_millis(1500)).await;
    let after = observed(&replay(&core2, &session).await, &task);
    assert_eq!(
        after.len(),
        before.len(),
        "a re-observed service is not recorded again: {after:#?}"
    );
    // The server stops while the new Core watches: it is recorded gone.
    assert!(
        Command::new("kill")
            .args(["-9", &pid.to_string()])
            .status()
            .unwrap()
            .success()
    );
    wait_services(&mut c2, &task, 30, true, |l| {
        l.services
            .iter()
            .any(|s| s.port == port && s.state == "GONE")
    })
    .await;
    drop(repo);
}

/// The Core stopped, the server died while it was down: the restarted Core
/// records the service gone (marked as re-observed), never ready.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn qual_px_132_a_server_that_died_while_the_core_was_down_is_recorded_gone_on_restart() {
    if !have("node", "--version") {
        eprintln!("skipped: node is not installed");
        return;
    }
    let (_repo, root) = plain_repo(&[("notes.txt", "n\n"), ("server.js", NODE_SERVER)]);
    let steps = vec![
        plan(),
        start_cmd(&["node", "server.js"]),
        settle(&["node.port"]),
        read_notes(),
        complete(),
    ];
    let (base, _seen) = scripted_model(steps, vec![]).await;
    let dir = tempfile::tempdir().unwrap();
    let mut core = spawn(dir.path(), &base);
    let mut c = core.client().await;
    let (session, g) = session_with_lease(&mut c, 0x61).await;
    let task = create_task(
        &mut c,
        &session,
        g,
        &root,
        0x62,
        "local_trusted",
        "start a server",
    )
    .await;
    start_task(&mut c, &task, g, 0x63, "gpt-5-mini").await;
    run_to_end(&mut c, &task, 120).await;
    let ready = wait_services(&mut c, &task, 30, false, |l| {
        l.services.iter().any(|s| s.state == "READY")
    })
    .await;
    let (port, pid) = (ready.services[0].port, ready.services[0].pid);
    assert!(
        ready.services[0].command.contains("server.js"),
        "not the test's server: {:?}",
        ready.services[0]
    );
    drop(c);
    core.kill();
    assert!(
        Command::new("kill")
            .args(["-9", &pid.to_string()])
            .status()
            .unwrap()
            .success()
    );
    let core2 = spawn(dir.path(), &base);
    let mut c2 = core2.client().await;
    wait_services(&mut c2, &task, 30, true, |l| {
        l.services
            .iter()
            .any(|s| s.port == port && s.state == "GONE")
    })
    .await;
    let obs = observed(&replay(&core2, &session).await, &task);
    let gone = obs
        .iter()
        .find(|o| o["state"] == "GONE")
        .expect("the loss is on the log");
    assert_eq!(gone["previous"], "READY");
    assert_eq!(gone["reobserved"], true, "{gone}");
}
