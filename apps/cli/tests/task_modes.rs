//! QUAL-PX-100 (CLI half): the headless CLI sets a task's mode and execution
//! preference through the same typed commands as any client, and shows the
//! Core's answer. The Core is the only decider: a task created with
//! `--mode ask` is refused a write by the Capability Kernel, a name the CLI
//! does not know is refused by the Core with its typed code, and the control
//! (a task created without the flag) writes. Each CLI invocation is its own
//! Core process over the same data directory, so every read after the first
//! command is also a restart.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};

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

    fn ok(&self, args: &[&str]) -> String {
        let (code, out, err) = self.run(args);
        assert_eq!(code, 0, "{args:?}\n{out}{err}");
        out
    }
}

fn core_bin() -> PathBuf {
    let cli = PathBuf::from(env!("CARGO_BIN_EXE_modbit-cli"));
    let core = cli.parent().unwrap().join(if cfg!(windows) {
        "modbit-core.exe"
    } else {
        "modbit-core"
    });
    assert!(
        core.exists(),
        "{} (build modbit-core first)",
        core.display()
    );
    core
}

/// A scripted OpenAI-compatible model (one reply per number of tool results
/// seen so far) that keeps every request body it was sent.
fn scripted_model(script: Vec<serde_json::Value>) -> (String, Arc<Mutex<Vec<serde_json::Value>>>) {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let seen = Arc::new(Mutex::new(Vec::new()));
    let seen2 = Arc::clone(&seen);
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut s) = stream else { break };
            let script = script.clone();
            let seen = Arc::clone(&seen2);
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
                seen.lock().unwrap().push(body);
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
    (format!("http://127.0.0.1:{port}"), seen)
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

/// A committed repository with a configured no-op check (FIX-03).
fn repo(parent: &Path) -> String {
    let repo = parent.join("repo");
    std::fs::create_dir_all(repo.join(".modbit")).unwrap();
    std::fs::write(repo.join("a.txt"), "a\n").unwrap();
    std::fs::write(
        repo.join(".modbit/verification.json"),
        r#"{"commands": [{"id": "fixture-noop", "argv": ["git", "--version"]}]}"#,
    )
    .unwrap();
    git(&repo, &["init", "-q", "-b", "main"]);
    git(&repo, &["config", "core.autocrlf", "false"]);
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
    repo.canonicalize()
        .unwrap()
        .to_string_lossy()
        .trim_start_matches(r"\\?\")
        .to_owned()
}

fn field<'a>(out: &'a str, line: &str, key: &str) -> &'a str {
    let l = out
        .lines()
        .find(|l| l.starts_with(line))
        .unwrap_or_else(|| panic!("no `{line}` line in:\n{out}"));
    l.split(' ')
        .find_map(|w| w.strip_prefix(&format!("{key}=")))
        .unwrap_or_else(|| panic!("no `{key}` in `{l}`"))
}

#[test]
fn qual_px_100_the_cli_sets_mode_and_preference_through_the_core_and_the_core_decides() {
    let core = core_bin();
    let tmp = tempfile::tempdir().unwrap();
    let data_dir = tmp.path().join("profile");
    std::fs::create_dir_all(&data_dir).unwrap();
    let root = repo(tmp.path());
    let script = vec![
        serde_json::json!({"calls": [{"name": "fs.read", "args": {"path": "a.txt"}}]}),
        serde_json::json!({"calls": [{"name": "plan.update", "args": {"outcome": "look", "expected_files": ["a.txt"]}}]}),
        serde_json::json!({"calls": [{"name": "task.complete", "args": {"summary": "planned", "self_review": {"findings": []}}}]}),
    ];
    let (base, seen) = scripted_model(script);
    let cli = Cli {
        data_dir,
        core,
        env: vec![
            ("MODBIT_OPENAI_BASE_URL".into(), base),
            ("OPENAI_API_KEY".into(), String::new()),
            ("ANTHROPIC_API_KEY".into(), String::new()),
            (
                "MODBIT_OPENAI_MODELS".into(),
                "gpt-fx=1/2;ctx=200000;out=65536;reasoning=true;effort=low;tier=flex".into(),
            ),
        ],
    };
    let out = cli.ok(&["session", "create"]);
    let sid = out.trim().strip_prefix("session ").unwrap().to_owned();
    let create = |extra: &[&str]| -> String {
        let mut args = vec!["task", "create", "--session", &sid, "--workspace", &root];
        args.extend_from_slice(extra);
        args.push("look at a.txt");
        let out = cli.ok(&args);
        out.trim().strip_prefix("task ").unwrap().to_owned()
    };
    let write = r#"{"path":"new.txt","op":"replace","content":"x\n"}"#;
    let invoke = |tid: &str| {
        cli.run(&[
            "tool",
            "invoke",
            "--session",
            &sid,
            "--task",
            tid,
            "change.apply",
            write,
        ])
    };

    // The control: a task created without `--mode` writes.
    let agent = create(&[]);
    let (code, out, err) = invoke(&agent);
    assert_eq!(code, 0, "{out}{err}");
    assert!(out.contains("status=SUCCESS"), "{out}");
    let new_txt = Path::new(&root).join("new.txt");
    assert!(new_txt.exists());
    std::fs::remove_file(&new_txt).unwrap();

    // `--mode ask`: the Kernel refuses the write, the file is untouched.
    let ask = create(&[
        "--mode",
        "ask",
        "--profile",
        "local_trusted",
        "--objective",
        "cost",
        "--effort",
        "high",
        "--tier",
        "flex",
    ]);
    let (code, out, err) = invoke(&ask);
    assert_eq!(code, 0, "{out}{err}");
    assert!(out.contains("status=POLICY_DENIED"), "{out}");
    assert!(out.contains("MODE_POSTURE"), "{out}");
    assert!(!new_txt.exists(), "the write must not have happened");
    // The Core's answer, shown: mode, the posture it derived, the preference,
    // and DIRECT with a typed reason (no signed registry here).
    let posture = cli.ok(&["task", "posture", "--task", &ask]);
    assert_eq!(field(&posture, "posture", "mode"), "ASK", "{posture}");
    assert_eq!(field(&posture, "posture", "writes"), "false", "{posture}");
    assert_eq!(
        field(&posture, "preference", "objective"),
        "COST",
        "{posture}"
    );
    assert_eq!(field(&posture, "preference", "effort"), "high", "{posture}");
    assert_eq!(field(&posture, "preference", "tier"), "flex", "{posture}");
    assert_eq!(field(&posture, "routing", "outcome"), "DIRECT", "{posture}");
    assert_eq!(
        field(&posture, "routing", "reason"),
        "NO_ACTIVE_REGISTRY",
        "{posture}"
    );
    // The session view shows the mode the Core recorded.
    let shown = cli.ok(&["session", "show", "--session", &sid]);
    assert!(shown.contains(&ask), "{shown}");

    // A change through the typed command, by name; the Core enforces it.
    let out = cli.ok(&["task", "mode", "--session", &sid, "--task", &ask, "agent"]);
    assert!(
        out.contains("mode AGENT previous=ASK effective=IMMEDIATE"),
        "{out}"
    );
    let (_, out, _) = invoke(&ask);
    assert!(out.contains("status=SUCCESS"), "{out}");
    std::fs::remove_file(&new_txt).unwrap();
    cli.ok(&["task", "mode", "--session", &sid, "--task", &ask, "ask"]);
    let (_, out, _) = invoke(&ask);
    assert!(out.contains("MODE_POSTURE"), "{out}");

    // Names the Core does not know are refused by the Core, with its codes,
    // and change nothing (the CLI computes no refusal of its own).
    for (args, code) in [
        (
            vec!["task", "mode", "--session", &sid, "--task", &ask, "godmode"],
            "UNKNOWN_MODE",
        ),
        (
            vec![
                "task",
                "preference",
                "--session",
                &sid,
                "--task",
                &ask,
                "--effort",
                "extreme",
            ],
            "INVALID_EFFORT",
        ),
        (
            vec![
                "task",
                "preference",
                "--session",
                &sid,
                "--task",
                &ask,
                "--objective",
                "cheapest",
            ],
            "UNKNOWN_OBJECTIVE",
        ),
        (
            vec![
                "task",
                "preference",
                "--session",
                &sid,
                "--task",
                &ask,
                "--pin",
                "openai/not-a-model",
            ],
            "UNKNOWN_MODEL",
        ),
        (
            vec![
                "task",
                "create",
                "--session",
                &sid,
                "--mode",
                "godmode",
                "x",
            ],
            "UNKNOWN_MODE",
        ),
        (
            vec![
                "task",
                "create",
                "--session",
                &sid,
                "--profile",
                "godmode",
                "x",
            ],
            "UNKNOWN_PROFILE",
        ),
    ] {
        let (exit, out, err) = cli.run(&args);
        assert_ne!(exit, 0, "{args:?}\n{out}{err}");
        assert!(format!("{out}{err}").contains(code), "{args:?}: {out}{err}");
    }
    let posture = cli.ok(&["task", "posture", "--task", &ask]);
    assert_eq!(field(&posture, "posture", "mode"), "ASK", "{posture}");

    // SetExecutionPreference by flags: a patch, recorded, DIRECT and why.
    let out = cli.ok(&[
        "task",
        "preference",
        "--session",
        &sid,
        "--task",
        &ask,
        "--effort",
        "medium",
    ]);
    assert_eq!(field(&out, "preference", "effort"), "medium", "{out}");
    assert_eq!(field(&out, "preference", "objective"), "COST", "{out}");
    assert_eq!(field(&out, "preference", "routing"), "DIRECT", "{out}");
    assert_eq!(
        field(&out, "preference", "reason"),
        "NO_ACTIVE_REGISTRY",
        "{out}"
    );

    // `task run --mode plan --objective intelligence --effort low`: the mode
    // is the typed command sent first, the rest rides on the start; the
    // provider's wire carries the effort, and the plan task completes with
    // nothing written.
    let plan = create(&[]);
    let out = cli.ok(&[
        "task",
        "run",
        "--session",
        &sid,
        "--task",
        &plan,
        "--model",
        "gpt-fx",
        "--mode",
        "plan",
        "--objective",
        "intelligence",
        "--effort",
        "low",
        "--tier",
        "priority",
        "--wait",
    ]);
    assert!(out.contains("mode PLAN previous=AGENT"), "{out}");
    assert!(out.contains("state=ReadyForReview"), "{out}");
    {
        let seen = seen.lock().unwrap();
        let last = seen.last().unwrap();
        assert_eq!(last["reasoning_effort"], "low", "{last:.300}");
        assert_eq!(last["service_tier"], "priority", "{last:.300}");
        let tools: Vec<&str> = last["tools"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|t| t["function"]["name"].as_str())
            .collect();
        assert!(!tools.contains(&"change.apply"), "{tools:?}");
    }
    let posture = cli.ok(&["task", "posture", "--task", &plan]);
    assert_eq!(field(&posture, "posture", "mode"), "PLAN", "{posture}");
    assert_eq!(
        field(&posture, "preference", "objective"),
        "INTELLIGENCE",
        "{posture}"
    );
    assert_eq!(
        field(&posture, "preference", "effort_applied"),
        "low",
        "{posture}"
    );
    assert_ne!(
        field(&posture, "preference", "applied_offset"),
        "0",
        "{posture}"
    );
    assert!(!new_txt.exists());
    // The events the Core wrote are the record any other client reads.
    let tail = cli.ok(&[
        "events",
        "tail",
        "--session",
        &sid,
        "--after",
        "0",
        "--count",
        "2000",
        "--json",
    ]);
    let types: Vec<String> = tail
        .lines()
        .map(|l| serde_json::from_str::<serde_json::Value>(l).unwrap())
        .filter(|l| l["task_id"] == plan.as_str())
        .map(|l| l["event_type"].as_str().unwrap().to_owned())
        .collect();
    for t in [
        "TaskModeSet",
        "ExecutionPreferenceSet",
        "TaskPostureApplied",
        "ExecutionPreferenceApplied",
    ] {
        assert!(types.iter().any(|x| x == t), "{t} in {types:?}");
    }
}

/// REQ-PX-055 (CLI half): the proposals of a task are listed from the Core and
/// answered through the same typed command as the desktop's card. A task with
/// none lists none; an answer naming a proposal the Core does not know is
/// refused with the Core's typed code and changes nothing (each invocation is
/// its own Core over the same data directory, so this is also a restart).
#[test]
fn px_055_the_cli_lists_and_answers_mode_proposals_through_the_core() {
    let core = core_bin();
    let tmp = tempfile::tempdir().unwrap();
    let data_dir = tmp.path().join("profile");
    std::fs::create_dir_all(&data_dir).unwrap();
    let root = repo(tmp.path());
    let cli = Cli {
        data_dir,
        core,
        env: vec![
            ("OPENAI_API_KEY".into(), String::new()),
            ("ANTHROPIC_API_KEY".into(), String::new()),
        ],
    };
    let out = cli.ok(&["session", "create"]);
    let sid = out.trim().strip_prefix("session ").unwrap().to_owned();
    let out = cli.ok(&[
        "task",
        "create",
        "--session",
        &sid,
        "--workspace",
        &root,
        "--mode",
        "plan",
        "look at a.txt",
    ]);
    let tid = out.trim().strip_prefix("task ").unwrap().to_owned();
    let out = cli.ok(&["task", "proposals", "--task", &tid]);
    assert_eq!(out.trim(), "no proposals", "{out}");
    let (code, out, err) = cli.run(&[
        "task",
        "proposal",
        "--session",
        &sid,
        "--task",
        &tid,
        "--proposal",
        "mp-nothing",
        "accept",
    ]);
    assert_ne!(code, 0, "{out}{err}");
    assert!(
        format!("{out}{err}").contains("UNKNOWN_PROPOSAL"),
        "{out}{err}"
    );
    // The mode is what the person set; nothing moved it.
    let out = cli.ok(&["task", "posture", "--task", &tid]);
    assert!(out.contains("PLAN"), "{out}");
}
