//! QUAL-PX-000: the headless CLI drives a real Core through a task lifecycle
//! on a fixture repository against a scripted model: JSON event lines by
//! cursor, a typed question answered from the shell, a protected effect
//! approved from another shell, Core killed and the stream resumed by cursor
//! with no duplicate, documented exit codes, exactly one effect receipt, and a
//! static dependency check that the CLI carries no provider/fs/git/policy code.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

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
        "{} (build modbit-core first)",
        core.display()
    );
    core
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
}

/// A scripted OpenAI-compatible model over real HTTP (one reply per number of
/// tool results seen so far).
fn scripted_model(script: Vec<serde_json::Value>) -> String {
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

#[test]
fn qual_px_000_headless_cli_task_lifecycle() {
    let core = core_bin();
    let tmp = tempfile::tempdir().unwrap();
    let data_dir = tmp.path().join("profile");
    std::fs::create_dir_all(&data_dir).unwrap();
    let repo = tmp.path().join("repo");
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
    let wt = format!("{}-wt", repo_str.replace('\\', "/"));
    let script = vec![
        serde_json::json!({"calls": [{"name": "plan.update", "args": {"outcome": "o", "expected_files": ["out.txt"]}}]}),
        serde_json::json!({"calls": [{"name": "user.ask", "args": {"question": "Which greeting?", "options": [{"id": "hi", "label": "hi"}, {"id": "hello", "label": "hello"}], "reason": "change_set"}}]}),
        serde_json::json!({"calls": [{"name": "change.apply", "args": {"path": "out.txt", "op": "create", "content": "hi\n"}}]}),
        serde_json::json!({"calls": [{"name": "git.worktree.create", "args": {"branch": "t/px", "path": wt}}]}),
        serde_json::json!({"calls": [{"name": "git.worktree.close", "args": {"path": wt}}]}),
        serde_json::json!({"calls": [{"name": "task.complete", "args": {"summary": "done", "self_review": {"findings": []}}}]}),
    ];
    let base = scripted_model(script);
    let cli = Cli {
        data_dir: data_dir.clone(),
        core: core.clone(),
        env: vec![
            ("MODBIT_OPENAI_BASE_URL".into(), base.clone()),
            ("OPENAI_API_KEY".into(), String::new()),
            ("ANTHROPIC_API_KEY".into(), String::new()),
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
        "greet",
    ]);
    assert_eq!(code, 0, "{err}");
    let tid = out.trim().strip_prefix("task ").unwrap().to_owned();
    // 1. Run until the agent asks: exit 2 (NEEDS_INPUT), never a hang.
    let (code, out, err) = cli.run(&[
        "task",
        "run",
        "--session",
        &sid,
        "--task",
        &tid,
        "--endpoint",
        "openai",
        "--model",
        "gpt-5",
        "--wait",
    ]);
    assert_eq!(code, 2, "{out}{err}");
    assert!(out.contains("state=Waiting wait_reason=UserInput"), "{out}");
    let (code, out, _) = cli.run(&["task", "status", "--task", &tid]);
    assert_eq!(code, 2, "{out}");
    // 2. JSON lines by cursor.
    let (code, out, err) = cli.run(&[
        "events",
        "tail",
        "--session",
        &sid,
        "--after",
        "0",
        "--count",
        "500",
        "--json",
    ]);
    assert_eq!(code, 0, "{err}");
    let lines: Vec<serde_json::Value> = out
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    assert!(
        lines
            .iter()
            .any(|l| l["event_type"] == "UserQuestionAsked"
                && l["payload"]["options"][0]["id"] == "hi"),
        "{out}"
    );
    let offsets: Vec<u64> = lines
        .iter()
        .map(|l| l["offset"].as_u64().unwrap())
        .collect();
    assert!(
        offsets.windows(2).all(|w| w[0] < w[1]),
        "strictly increasing offsets"
    );
    let cursor = *offsets.last().unwrap();
    // 3. Answer the typed question from the shell; the run continues to the protected effect.
    let (code, out, _) = cli.run(&["question", "list", "--task", &tid]);
    assert_eq!(code, 0);
    let qid = out
        .lines()
        .find_map(|l| l.strip_prefix("question "))
        .unwrap()
        .split(' ')
        .next()
        .unwrap()
        .to_owned();
    // 4. Approve the protected effect from another shell while the run waits on it.
    let approver = {
        let cli2 = Cli {
            data_dir: data_dir.clone(),
            core: core.clone(),
            env: cli.env.clone(),
        };
        let sid2 = sid.clone();
        std::thread::spawn(move || {
            // Patient enough for a loaded machine: the run reaches the
            // protected effect only after a question, two model turns and a
            // change, and this test shares the machine with the others.
            for _ in 0..1_800 {
                let (_, out, _) = cli2.run(&["approval", "list", "--session", &sid2]);
                if let Some(id) = out
                    .lines()
                    .find(|l| l.contains("status=REQUESTED"))
                    .and_then(|l| l.strip_prefix("approval "))
                    .and_then(|l| l.split(' ').next())
                {
                    let (code, out, err) = cli2.run(&[
                        "approval",
                        "resolve",
                        "--session",
                        &sid2,
                        "--approval",
                        id,
                        "approve",
                        "ok",
                    ]);
                    assert!(code == 0 && out.contains("status=APPROVED"), "{out}{err}");
                    return true;
                }
                std::thread::sleep(Duration::from_millis(100));
            }
            false
        })
    };
    let (code, out, err) = cli.run(&[
        "question",
        "answer",
        "--session",
        &sid,
        "--task",
        &tid,
        "--question",
        &qid,
        "--option",
        "hi",
        "--endpoint",
        "openai",
        "--model",
        "gpt-5",
        "--wait",
    ]);
    assert_eq!(code, 0, "{out}{err}");
    assert!(out.contains("state=ReadyForReview"), "{out}");
    assert!(
        approver.join().unwrap(),
        "the approval was resolved from the other shell"
    );
    assert_eq!(
        std::fs::read_to_string(repo.join("out.txt")).unwrap(),
        "hi\n"
    );
    assert!(!Path::new(&wt).exists(), "the approved close happened");
    let (code, out, _) = cli.run(&["receipts", "--task", &tid]);
    assert!(
        code == 0 && out.contains("receipt_chain valid=true count=1"),
        "exactly one effect receipt: {out}"
    );
    // 5. Kill the Core and resume the stream by cursor: no duplicate, the same offsets continue.
    let (_, before, _) = cli.run(&[
        "events",
        "tail",
        "--session",
        &sid,
        "--after",
        &cursor.to_string(),
        "--count",
        "500",
        "--json",
    ]);
    // The CLI-spawned Core is tethered to the CLI's stdin: it ends with each
    // invocation, so every command already boots a fresh Core (a restart per
    // step). Kill any lingering one as well, then resume by cursor.
    if cfg!(unix) {
        let _ = Command::new("pkill")
            .args([
                "-9",
                "-f",
                &format!("modbit-core --data-dir {}", data_dir.display()),
            ])
            .status();
        std::thread::sleep(Duration::from_millis(300));
    }
    let (code, after, err) = cli.run(&[
        "events",
        "tail",
        "--session",
        &sid,
        "--after",
        &cursor.to_string(),
        "--count",
        "500",
        "--json",
    ]);
    assert_eq!(code, 0, "{err}");
    let parse = |s: &str| {
        s.lines()
            .map(|l| {
                serde_json::from_str::<serde_json::Value>(l).unwrap()["offset"]
                    .as_u64()
                    .unwrap()
            })
            .collect::<Vec<u64>>()
    };
    let (b, a) = (parse(&before), parse(&after));
    assert_eq!(a, b, "the resumed stream is identical after the restart");
    assert!(
        a.first().is_some_and(|f| *f > cursor) && a.windows(2).all(|w| w[0] < w[1]),
        "no duplicate after the cursor: {a:?}"
    );
    // 6. Documented exit codes on the other terminal states.
    let (code, out, _) = cli.run(&["task", "status", "--task", &tid]);
    assert_eq!(
        (code, out.contains("state=ReadyForReview")),
        (0, true),
        "{out}"
    );
    let (_, out, _) = cli.run(&[
        "task",
        "create",
        "--session",
        &sid,
        "--workspace",
        &repo_str,
        "second",
    ]);
    let tid2 = out.trim().strip_prefix("task ").unwrap().to_owned();
    let (code, _, _) = cli.run(&["task", "status", "--task", &tid2]);
    assert_eq!(code, 4, "Queued");
    let (code, out, _) = cli.run(&["task", "cancel", "--session", &sid, "--task", &tid2]);
    assert_eq!(code, 0, "{out}");
    let (code, out, _) = cli.run(&["task", "status", "--task", &tid2]);
    assert_eq!((code, out.contains("state=Cancelled")), (3, true), "{out}");
    let (code, _, err) = cli.run(&["task", "run", "--session", &sid, "--task", &tid2]);
    assert_eq!(code, 1, "a rejected command is 1: {err}");
    // 7. Thin client: no provider, filesystem-service, Git or policy crate in the CLI's dependencies.
    let manifest =
        std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml")).unwrap();
    for forbidden in [
        "modbit-providers",
        "modbit-workspace",
        "modbit-git",
        "modbit-policy",
        "modbit-tools",
        "reqwest",
    ] {
        assert!(
            !manifest.contains(forbidden),
            "thin client must not depend on {forbidden}"
        );
    }
}

/// REQ-EV-0111 / 0268 (and the reporting half of REQ-EV-0056 / 0092): the
/// product tells a user what compaction did — which epoch is installed, how
/// many transcript entries it summarised away, the manifest that holds them
/// and how often the run reused its cached prompt prefix instead of rebuilding
/// it. Run for real: a fixture repository, a scripted model and a token budget
/// small enough that the transcript is compacted while the task runs.
#[test]
fn qual_ev_0111_0268_context_show_reports_compaction_epochs_and_the_cached_prefix() {
    let core = core_bin();
    let tmp = tempfile::tempdir().unwrap();
    let data_dir = tmp.path().join("profile");
    std::fs::create_dir_all(&data_dir).unwrap();
    let repo = tmp.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    std::fs::write(
        repo.join("big.txt"),
        "filler line for the transcript\n".repeat(400),
    )
    .unwrap();
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
    let read = serde_json::json!({"calls": [{"name": "fs.read", "args": {"path": "big.txt"}}]});
    let script = vec![
        serde_json::json!({"calls": [{"name": "plan.update", "args": {"outcome": "read the big file", "expected_files": ["big.txt"]}}]}),
        read.clone(),
        read.clone(),
        read.clone(),
        read.clone(),
        serde_json::json!({"calls": [{"name": "task.complete", "args": {"summary": "done", "self_review": {"findings": []}}}]}),
    ];
    let base = scripted_model(script);
    let cli = Cli {
        data_dir: data_dir.clone(),
        core: core.clone(),
        env: vec![
            ("MODBIT_OPENAI_BASE_URL".into(), base.clone()),
            ("OPENAI_API_KEY".into(), String::new()),
            ("ANTHROPIC_API_KEY".into(), String::new()),
            ("MODBIT_COMPACTION_TOKEN_BUDGET".into(), "1500".into()),
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
        "read",
        "the",
        "big",
        "file",
    ]);
    assert_eq!(code, 0, "{err}");
    let tid = out.trim().strip_prefix("task ").unwrap().to_owned();
    let (code, out, err) = cli.run(&[
        "task",
        "run",
        "--session",
        &sid,
        "--task",
        &tid,
        "--endpoint",
        "openai",
        "--model",
        "gpt-5",
        "--wait",
    ]);
    // 0 = finished, 2 = the run stopped for input; either way it compacted.
    assert!(code == 0 || code == 2, "{code}: {out}{err}");
    let (code, out, err) = cli.run(&["context", "show", &tid]);
    assert_eq!(code, 0, "{err}");
    let line = out
        .lines()
        .find(|l| l.starts_with("compaction: "))
        .unwrap_or_else(|| panic!("no compaction report: {out}"));
    let field = |k: &str| -> String {
        line.split_whitespace()
            .find_map(|w| w.strip_prefix(k))
            .unwrap_or_else(|| panic!("{k} missing from {line}"))
            .to_owned()
    };
    assert!(field("epoch=").parse::<u32>().unwrap() >= 1, "{line}");
    assert!(field("epochs=").parse::<u32>().unwrap() >= 1, "{line}");
    assert!(
        field("compacted_entries=").parse::<u64>().unwrap() >= 2,
        "{line}"
    );
    assert_ne!(field("manifest="), "none", "{line}");
    // The prefix was rebuilt at least once (the first turn) and every epoch,
    // and reused on the turns in between.
    let hits: u32 = field("cache_hits=").parse().unwrap();
    let misses: u32 = field("cache_misses=").parse().unwrap();
    assert!(misses >= 2, "{line}");
    assert!(hits >= 1, "the prefix is reused between epochs: {line}");
}
