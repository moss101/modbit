//! FIX-17 (audit): a detached `task run` survives its client. The Core a CLI
//! spawns exits on its own once no client has been connected for the idle
//! window — but it must not do so while a run is alive. Real `modbit-core`
//! and `modbit-execd` processes, the real CLI, a model over real HTTP that
//! answers slowly: the client disconnects, the idle window (shortened through
//! `MODBIT_CORE_IDLE_EXIT_SECS`) passes several times over, and the same Core
//! process must still be serving, the run must finish on it with no restart,
//! and only then does the Core leave on its own.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

fn core_bin() -> PathBuf {
    let cli = PathBuf::from(env!("CARGO_BIN_EXE_modbit-cli"));
    let dir = cli.parent().unwrap();
    let core = dir.join(if cfg!(windows) {
        "modbit-core.exe"
    } else {
        "modbit-core"
    });
    assert!(
        core.exists(),
        "{} (build modbit-core and modbit-execd first)",
        core.display()
    );
    core
}

/// An OpenAI-compatible model over real HTTP whose every reply takes
/// `delay` and follows `script` (one entry per number of tool results seen).
fn slow_model(script: Vec<serde_json::Value>, delay: Duration) -> String {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut s) = stream else { break };
            let script = script.clone();
            std::thread::spawn(move || {
                let mut buf = Vec::new();
                let mut tmp = [0u8; 4096];
                let (head_end, len) = loop {
                    let n = s.read(&mut tmp).unwrap_or(0);
                    if n == 0 {
                        return;
                    }
                    buf.extend_from_slice(&tmp[..n]);
                    if let Some(pos) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
                        let head = String::from_utf8_lossy(&buf[..pos]).to_string();
                        let len = head
                            .lines()
                            .find_map(|l| {
                                let (k, v) = l.split_once(':')?;
                                k.eq_ignore_ascii_case("content-length")
                                    .then(|| v.trim().parse::<usize>().ok())
                                    .flatten()
                            })
                            .unwrap_or(0);
                        break (pos + 4, len);
                    }
                };
                while buf.len() < head_end + len {
                    let n = s.read(&mut tmp).unwrap_or(0);
                    if n == 0 {
                        break;
                    }
                    buf.extend_from_slice(&tmp[..n]);
                }
                let body: serde_json::Value =
                    serde_json::from_slice(&buf[head_end..head_end + len]).unwrap_or_default();
                let results = body["messages"]
                    .as_array()
                    .map(|m| m.iter().filter(|x| x["role"] == "tool").count())
                    .unwrap_or(0);
                std::thread::sleep(delay);
                let reply = script
                    .get(results)
                    .cloned()
                    .unwrap_or_else(|| serde_json::json!({"text": "nothing further"}));
                let mut frames: Vec<String> = Vec::new();
                if let Some(t) = reply["text"].as_str() {
                    frames.push(serde_json::json!({"id":"c","model":"scripted","choices":[{"index":0,"delta":{"content":t},"finish_reason":null}]}).to_string());
                }
                let calls = reply["calls"].as_array().cloned().unwrap_or_default();
                for (i, c) in calls.iter().enumerate() {
                    frames.push(serde_json::json!({"id":"c","model":"scripted","choices":[{"index":0,"delta":{"tool_calls":[{"index":i,"id":format!("call_{results}_{i}"),"type":"function","function":{"name":c["name"],"arguments":c["args"].to_string()}}]},"finish_reason":null}]}).to_string());
                }
                let finish = if calls.is_empty() {
                    "stop"
                } else {
                    "tool_calls"
                };
                frames.push(serde_json::json!({"id":"c","model":"scripted","choices":[{"index":0,"delta":{},"finish_reason":finish}],"usage":{"prompt_tokens":10,"completion_tokens":5}}).to_string());
                let mut payload = String::new();
                for f in frames {
                    payload.push_str(&format!("data: {f}\n\n"));
                }
                payload.push_str("data: [DONE]\n\n");
                let _ = s.write_all(format!("HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncontent-length: {}\r\n\r\n{payload}", payload.len()).as_bytes());
                let _ = s.flush();
            });
        }
    });
    format!("http://127.0.0.1:{port}")
}

fn git(dir: &Path, args: &[&str]) {
    assert!(
        Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(args)
            .status()
            .unwrap()
            .success()
    );
}

struct Cli {
    data_dir: PathBuf,
    core: PathBuf,
    env: Vec<(String, String)>,
}

impl Cli {
    fn run(&self, args: &[&str]) -> (i32, String, String) {
        let out = Command::new(env!("CARGO_BIN_EXE_modbit-cli"))
            .env("MODBIT_CORE_BIN", &self.core)
            .envs(self.env.iter().map(|(k, v)| (k.as_str(), v.as_str())))
            .arg("--data-dir")
            .arg(&self.data_dir)
            .args(args)
            .output()
            .unwrap();
        (
            out.status.code().unwrap_or(-1),
            String::from_utf8_lossy(&out.stdout).to_string(),
            String::from_utf8_lossy(&out.stderr).to_string(),
        )
    }

    /// The ready line of the Core serving this profile, if one is.
    fn ready(&self) -> Option<String> {
        std::fs::read_to_string(self.data_dir.join("core.ready")).ok()
    }
}

/// A profile, a fixture repository and a created task whose model replies
/// after `delay` following `script`; the Core's idle window is 2 s.
fn fixture(
    tmp: &Path,
    script: Vec<serde_json::Value>,
    delay: Duration,
) -> (Cli, String, String, PathBuf) {
    let core = core_bin();
    let data_dir = tmp.join("profile");
    std::fs::create_dir_all(&data_dir).unwrap();
    let repo = tmp.join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    std::fs::write(repo.join("a.txt"), "a\n").unwrap();
    git(&repo, &["init", "-q", "-b", "main"]);
    git(&repo, &["add", "-A"]);
    git(
        &repo,
        &[
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@e",
            "commit",
            "-q",
            "-m",
            "base",
        ],
    );
    let repo_str = repo
        .canonicalize()
        .unwrap()
        .to_string_lossy()
        .trim_start_matches(r"\\?\")
        .to_owned();
    let base = slow_model(script, delay);
    let cli = Cli {
        data_dir,
        core,
        env: vec![
            ("MODBIT_OPENAI_BASE_URL".into(), base),
            ("OPENAI_API_KEY".into(), String::new()),
            ("ANTHROPIC_API_KEY".into(), String::new()),
            ("MODBIT_CORE_IDLE_EXIT_SECS".into(), "2".into()),
        ],
    };
    let (code, out, err) = cli.run(&["session", "create"]);
    assert_eq!(code, 0, "{err}");
    let sid = out.trim().strip_prefix("session ").unwrap().to_owned();
    let (code, out, err) = cli.run(&[
        "task",
        "create",
        "--session",
        &sid,
        "--workspace",
        &repo_str,
        "a long job",
    ]);
    assert_eq!(code, 0, "{err}");
    let tid = out.trim().strip_prefix("task ").unwrap().to_owned();
    (cli, sid, tid, repo)
}

/// Start the run detached (no `--wait`): the CLI goes away once it started.
fn run_detached(cli: &Cli, sid: &str, tid: &str) -> String {
    let (code, out, err) = cli.run(&[
        "task",
        "run",
        "--session",
        sid,
        "--task",
        tid,
        "--endpoint",
        "openai",
        "--model",
        "gpt-5",
    ]);
    assert_eq!(code, 0, "{out}{err}");
    cli.ready().expect("the Core the CLI spawned is serving")
}

fn wait_for_review(cli: &Cli, tid: &str, serving: &str) -> String {
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        let (_, out, _) = cli.run(&["task", "status", "--task", tid]);
        if out.contains("loop_alive=false") && out.contains("state=ReadyForReview") {
            assert_eq!(
                cli.ready().as_deref(),
                Some(serving),
                "the run completed on the Core that started it, with no restart in between"
            );
            return out;
        }
        assert!(
            Instant::now() < deadline,
            "the task never reached review: {out}"
        );
        assert!(
            !out.contains("state=Waiting"),
            "the run was suspended by a restart instead of continuing: {out}"
        );
        std::thread::sleep(Duration::from_millis(500));
    }
}

fn wait_for_core_exit(cli: &Cli, within: Duration) {
    let deadline = Instant::now() + within;
    while cli.ready().is_some() {
        assert!(
            Instant::now() < deadline,
            "the idle Core never left on its own"
        );
        std::thread::sleep(Duration::from_millis(250));
    }
}

#[test]
fn fix_17_a_detached_run_survives_the_clients_disconnect_past_the_idle_window() {
    let tmp = tempfile::tempdir().unwrap();
    // Three model turns of 3 s each: the run lasts ~9 s, with the client gone
    // for all of it, against an idle window of 2 s.
    let (cli, sid, tid, repo) = fixture(
        tmp.path(),
        vec![
            serde_json::json!({"calls": [{"name": "plan.update", "args": {"outcome": "o", "expected_files": ["out.txt"], "protected_effects": []}}]}),
            serde_json::json!({"calls": [{"name": "change.apply", "args": {"path": "out.txt", "op": "create", "content": "hi\n"}}]}),
            serde_json::json!({"calls": [{"name": "task.complete", "args": {"summary": "done", "self_review": {"findings": []}}}]}),
        ],
        Duration::from_secs(3),
    );
    let serving = run_detached(&cli, &sid, &tid);
    let started = Instant::now();
    // Several idle windows pass with no client connected, mid-run.
    std::thread::sleep(Duration::from_secs(6));
    assert!(
        started.elapsed() < Duration::from_secs(20),
        "the machine is too loaded to prove anything"
    );
    assert_eq!(
        cli.ready().as_deref(),
        Some(serving.as_str()),
        "the same Core is still serving mid-run: it did not idle-exit under a live run"
    );
    // The run finishes on that Core; polling the status is the first client
    // since the detach.
    let last = wait_for_review(&cli, &tid, &serving);
    assert!(last.contains("state=ReadyForReview"), "{last}");
    assert_eq!(
        std::fs::read_to_string(repo.join("out.txt")).unwrap_or_default(),
        "hi\n",
        "the work of the detached run reached the workspace"
    );
    // Idle exit still works once nothing is alive: the Core leaves by itself.
    wait_for_core_exit(&cli, Duration::from_secs(30));
}

#[test]
fn fix_17_a_background_command_keeps_the_core_up_after_its_run_ended() {
    let tmp = tempfile::tempdir().unwrap();
    // The run starts a 9 s background command and completes at once. The
    // broker stops its processes when no Core returns, so a Core that left
    // after the run would kill it: the Core stays until the command ends.
    let marker = tmp.path().join("bg-done.txt");
    let marker_str = marker.to_string_lossy().replace('\\', "/");
    let (cli, sid, tid, _repo) = fixture(
        tmp.path(),
        vec![
            serde_json::json!({"calls": [{"name": "plan.update", "args": {"outcome": "o", "expected_files": [], "protected_effects": []}}]}),
            serde_json::json!({"calls": [{"name": "shell.start", "args": {"argv": ["sh", "-c", format!("sleep 9; echo done > {marker_str}")]}}]}),
            serde_json::json!({"calls": [{"name": "task.complete", "args": {"summary": "done", "self_review": {"findings": []}}}]}),
        ],
        Duration::from_millis(300),
    );
    let serving = run_detached(&cli, &sid, &tid);
    // The run is over within a couple of seconds; the command is not.
    std::thread::sleep(Duration::from_secs(6));
    assert_eq!(
        cli.ready().as_deref(),
        Some(serving.as_str()),
        "the Core stayed up for the running background command"
    );
    assert!(!marker.exists(), "the command is still running");
    let _ = wait_for_review(&cli, &tid, &serving);
    let deadline = Instant::now() + Duration::from_secs(30);
    while !marker.exists() {
        assert!(
            Instant::now() < deadline,
            "the background command never finished"
        );
        std::thread::sleep(Duration::from_millis(250));
    }
    assert_eq!(
        cli.ready().as_deref(),
        Some(serving.as_str()),
        "the command finished on the Core that started it"
    );
    wait_for_core_exit(&cli, Duration::from_secs(30));
}
