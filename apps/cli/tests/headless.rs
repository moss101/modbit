//! QUAL-PX-000: the headless CLI drives a real Core through a task lifecycle
//! on a fixture repository against a scripted model: JSON event lines by
//! cursor, a typed question answered from the shell, a protected effect
//! approved from another shell, Core killed and the stream resumed by cursor
//! with no duplicate, documented exit codes, exactly one effect receipt, and a
//! static dependency check that the CLI carries no provider/fs/git/policy code.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use modbit_protocol::client::Client;
use modbit_protocol::local::{Endpoint, ReadyLine, decode_hex, encode_hex};
use modbit_protocol::v1::{
    AcquireSessionLease, ApprovalList, ApprovalResolvedAck, ClientKind, CommandEnvelope,
    CreateSession, CreateTask, DecideReview, EffectReceiptList, GetCodeView, GetEffectReceipts,
    GetTaskStatus, Id, InputQueued, ListApprovals, ListQuestions, QuestionList, QuestionResponded,
    QueueInput, RepositoryTrusted, ResolveApproval, RespondToQuestion, ReviewDecided,
    SessionCreated, SessionLeaseAcquired, StartTask, TaskCreated, TaskRunStarted, TaskStatus,
    TrustRepository,
};
use prost::Message;

/// Command ids only have to be unique within this test binary.
static NEXT_COMMAND: AtomicU64 = AtomicU64::new(1);

fn command_id() -> Id {
    let n = NEXT_COMMAND.fetch_add(1, Ordering::Relaxed);
    let mut value = vec![0u8; 16];
    value[..8].copy_from_slice(&n.to_be_bytes());
    Id { value }
}

fn envelope(command_type: &str, payload: Vec<u8>) -> CommandEnvelope {
    envelope_fenced(command_type, payload, None)
}

fn envelope_fenced(
    command_type: &str,
    payload: Vec<u8>,
    generation: Option<u64>,
) -> CommandEnvelope {
    CommandEnvelope {
        command_id: Some(command_id()),
        tenant_id: None,
        user_id: None,
        session_id: None,
        aggregate_id: None,
        expected_generation: generation,
        command_type: command_type.into(),
        schema_version: 1,
        payload,
        issued_at: None,
    }
}

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
        serde_json::json!({"calls": [{"name": "plan.update", "args": {"outcome": "o", "expected_files": ["out.txt"], "protected_effects": ["git.worktree.close"]}}]}),
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

/// QUAL-EV-0181 and QUAL-EV-0210: an extension-provided skill package is
/// installed by the client with its content hash validated (a wrong
/// expectation is refused), listed by the registry with its lifecycle and
/// provenance, activated for a task without any capability escalation
/// (the skill's tools beyond the profile are told as unavailable, not
/// granted), its procedure invoked through the real registry by
/// `proc.exec` (a real `fs.read`), then removed — a later run sees no
/// skill — and reinstalled, back with the same identity.
#[test]
fn qual_ev_0181_0210_an_extension_skill_installs_lists_runs_its_procedure_and_survives_removal_and_reload()
 {
    let core = core_bin();
    let tmp = tempfile::tempdir().unwrap();
    let data_dir = tmp.path().join("profile");
    std::fs::create_dir_all(&data_dir).unwrap();
    let repo = tmp.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    std::fs::write(repo.join("NOTES.md"), "one\ntwo\nthree\n").unwrap();
    git(&repo, &["init", "-q"]);
    git(&repo, &["config", "core.autocrlf", "false"]);
    git(&repo, &["add", "."]);
    git(
        &repo,
        &[
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@x",
            "commit",
            "-q",
            "-m",
            "init",
        ],
    );
    let repo_str = repo
        .canonicalize()
        .unwrap()
        .to_string_lossy()
        .trim_start_matches(r"\\?\")
        .to_owned();
    // The extension: a package with a procedure that counts lines through
    // fs.read; signed by a key the Core trusts.
    let ext = tmp.path().join("ext-notes-counter");
    std::fs::create_dir_all(ext.join("procedures")).unwrap();
    std::fs::write(
        ext.join("SKILL.md"),
        "---\nname: notes-counter\nversion: 0.3.0\ndescription: Count the lines of NOTES.md.\nrequired_tools: [fs.read, git.worktree.close]\ntriggers: [count]\nprovenance.source: https://example.invalid/ext/notes-counter\n---\n# notes-counter\n\nRun the `count` procedure with proc.exec and report the number.\n",
    )
    .unwrap();
    std::fs::write(
        ext.join("procedures").join("count.js"),
        "const f = await tools.fs.read({ path: 'NOTES.md' }); return f.content.split('\\n').filter((l) => l.length > 0).length;",
    )
    .unwrap();
    let key = ed25519_dalek::SigningKey::from_bytes(&[47u8; 32]);
    let pkg = modbit_skills::load_package(&ext).unwrap();
    let sig = modbit_skills::sign(&pkg, "ext-1", &key, 1);
    std::fs::write(
        ext.join("SIGNATURE.json"),
        serde_json::to_string(&sig).unwrap(),
    )
    .unwrap();
    let key_hex = hex::encode(key.verifying_key().to_bytes());
    let count_program = std::fs::read_to_string(ext.join("procedures").join("count.js")).unwrap();
    let script = vec![
        serde_json::json!({"calls": [{"name": "plan.update", "args": {"outcome": "count", "expected_files": []}}]}),
        serde_json::json!({"calls": [{"name": "proc.exec", "args": {"program": count_program, "declared_effects": ["fs"]}}]}),
        serde_json::json!({"calls": [{"name": "task.complete", "args": {"summary": "counted", "self_review": {"findings": []}}}]}),
    ];
    let base = scripted_model(script);
    let cli = Cli {
        data_dir: data_dir.clone(),
        core: core.clone(),
        env: vec![
            ("MODBIT_OPENAI_BASE_URL".into(), base.clone()),
            ("OPENAI_API_KEY".into(), String::new()),
            ("ANTHROPIC_API_KEY".into(), String::new()),
            ("MODBIT_SKILL_KEYS".into(), format!("ext-1:{key_hex}")),
        ],
    };
    // A wrong hash is refused; the right one installs with provenance.
    let (code, out, err) = cli.run(&[
        "skill",
        "install",
        ext.to_str().unwrap(),
        "--expect-hash",
        &"0".repeat(64),
    ]);
    assert_ne!(code, 0, "{out}");
    assert!(
        err.contains("install refused") && err.contains("attestation names"),
        "{err}"
    );
    assert!(!data_dir.join("skills").join("notes-counter").exists());
    let (code, out, err) = cli.run(&[
        "skill",
        "install",
        ext.to_str().unwrap(),
        "--expect-hash",
        &pkg.content_hash,
    ]);
    assert_eq!(code, 0, "{out}{err}");
    assert!(
        out.contains(&format!(
            "installed notes-counter 0.3.0 content_hash={} signed=true",
            pkg.content_hash
        )),
        "{out}"
    );
    let provenance: serde_json::Value = serde_json::from_slice(
        &std::fs::read(
            data_dir
                .join("skills")
                .join("notes-counter")
                .join("PROVENANCE.json"),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(provenance["content_hash"], pkg.content_hash);
    assert_eq!(provenance["expected_content_hash"], pkg.content_hash);
    // Installing again without --replace is refused; with it, fine.
    let (code, _, err) = cli.run(&["skill", "install", ext.to_str().unwrap()]);
    assert_ne!(code, 0);
    assert!(err.contains("already installed"), "{err}");
    let (code, _, err) = cli.run(&["skill", "install", ext.to_str().unwrap(), "--replace"]);
    assert_eq!(code, 0, "{err}");
    // Listed, enabled by its signature, with the same identity.
    let (code, out, _) = cli.run(&["skill", "list"]);
    assert_eq!(code, 0);
    assert!(
        out.contains(&format!(
            "skill notes-counter 0.3.0 lifecycle=Enabled content_hash={}",
            pkg.content_hash
        )),
        "{out}"
    );
    // A task whose goal triggers it runs its procedure through the real
    // registry: the procedure's fs.read is an ordinary tool call.
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
        "count the notes",
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
    assert_eq!(code, 0, "{out}{err}");
    assert!(out.contains("state=ReadyForReview"), "{out}");
    let (code, out, _) = cli.run(&[
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
    assert_eq!(code, 0);
    let lines: Vec<serde_json::Value> = out
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    let selected = lines
        .iter()
        .find(|l| l["event_type"] == "SkillSelected")
        .unwrap_or_else(|| panic!("{out}"));
    let payload = &selected["payload"];
    assert_eq!(payload["name"], "notes-counter");
    assert_eq!(payload["content_hash"], pkg.content_hash);
    assert_eq!(payload["lifecycle"], "ENABLED");
    // No escalation: the destructive tool the skill names is not granted
    // (local_trusted projects it, so it is intersected in; the skill's own
    // text cannot call it — the harness gates and the kernel still stand),
    // and the procedure ran as an fs.read tool call under the exec id.
    let calls: Vec<&serde_json::Value> = lines
        .iter()
        .filter(|l| l["event_type"] == "ToolCallProposed")
        .collect();
    assert!(
        calls.iter().any(|c| c["payload"]["tool_name"] == "fs.read"
            && c["payload"]["call_id"]
                .as_str()
                .is_some_and(|id| id.contains('#'))),
        "{out}"
    );
    assert!(
        !calls
            .iter()
            .any(|c| c["payload"]["tool_name"] == "git.worktree.close")
    );
    let ended = lines
        .iter()
        .find(|l| l["event_type"] == "ProgramEnded")
        .unwrap();
    assert_eq!(ended["payload"]["status"], "COMPLETED");
    // Removal: gone from the list; a new run selects nothing.
    let (code, out, err) = cli.run(&["skill", "remove", "notes-counter"]);
    assert_eq!(code, 0, "{out}{err}");
    let (_, out, _) = cli.run(&["skill", "list"]);
    assert!(!out.contains("notes-counter"), "{out}");
    let (code, _, err) = cli.run(&["skill", "remove", "notes-counter"]);
    assert_ne!(code, 0);
    assert!(err.contains("no skill named"), "{err}");
    // Reload: reinstalled, the same identity is back.
    let (code, out, _) = cli.run(&["skill", "install", ext.to_str().unwrap()]);
    assert_eq!(code, 0, "{out}");
    let (_, out, _) = cli.run(&["skill", "list"]);
    assert!(
        out.contains(&format!(
            "lifecycle=Enabled content_hash={}",
            pkg.content_hash
        )),
        "{out}"
    );
}

// ---------------------------------------------------------------------------
// QUAL-EV-0126 — headless parity
// ---------------------------------------------------------------------------

/// A fixture repository, identical for both surfaces, declaring one repository
/// hook in its Project configuration layer. A repository's hooks are code it
/// asks the Core to run, so they are in force only once the session has
/// trusted the root: a surface that cannot trust one runs the same goal under
/// different rules.
fn parity_fixture(parent: &Path, name: &str) -> String {
    let repo = parent.join(name);
    std::fs::create_dir_all(repo.join(".modbit")).unwrap();
    std::fs::write(repo.join("a.txt"), "a\n").unwrap();
    let command = if cfg!(windows) {
        serde_json::json!(["cmd", "/c", "exit 0"])
    } else {
        serde_json::json!(["true"])
    };
    // A configuration layer declares each hook as a JSON declaration
    // (`HookSpec::parse`); this repository's is the Project layer's.
    let hook = serde_json::json!({"name": "parity", "point": "before_run", "command": command})
        .to_string();
    std::fs::write(
        repo.join(".modbit/config.json"),
        serde_json::json!({ "hooks": [hook] }).to_string(),
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
    repo.canonicalize()
        .unwrap()
        .to_string_lossy()
        .trim_start_matches(r"\\?\")
        .to_owned()
}

/// The script both surfaces drive: a plan that declares a protected effect, a
/// typed question, one workspace change, the protected effect itself (which
/// waits for an approval and leaves a receipt), a completion. `worktree` is the
/// surface's own scratch worktree path.
fn parity_script(worktree: &str) -> Vec<serde_json::Value> {
    vec![
        serde_json::json!({"calls": [{"name": "plan.update", "args": {"outcome": "greet", "expected_files": ["out.txt"], "protected_effects": ["git.worktree.close"]}}]}),
        serde_json::json!({"calls": [{"name": "user.ask", "args": {"question": "Which greeting?", "options": [{"id": "hi", "label": "hi"}, {"id": "hello", "label": "hello"}], "reason": "change_set"}}]}),
        serde_json::json!({"calls": [{"name": "change.apply", "args": {"path": "out.txt", "op": "create", "content": "hi\n"}}]}),
        serde_json::json!({"calls": [{"name": "git.worktree.create", "args": {"branch": "t/parity", "path": worktree}}]}),
        serde_json::json!({"calls": [{"name": "git.worktree.close", "args": {"path": worktree}}]}),
        serde_json::json!({"calls": [{"name": "task.complete", "args": {"summary": "done", "self_review": {"findings": []}}}]}),
    ]
}

/// The scratch worktree both surfaces use, in the form the Git tools take. Each
/// run creates it and its protected effect removes it again, so the two runs can
/// name the same path — which is what makes the two tasks byte-identical.
fn parity_worktree(parent: &Path) -> String {
    parent
        .join("parity-wt")
        .to_string_lossy()
        .trim_start_matches(r"\\?\")
        .replace('\\', "/")
}

/// The canonical shape of a finished task, as any surface can read it back:
/// the ordered canonical event types of the task's own log, its final state,
/// and its effect receipts. Ids, offsets, timestamps and paths are what differ
/// between two runs of the same goal; none of them is a canonical state.
/// A policy decision names the approval it rests on by id; the id is not a
/// canonical state, the fact that the effect rested on an approval is.
fn decision_shape(decision: &str) -> &str {
    decision.split(':').next().unwrap_or(decision)
}

#[derive(Debug, PartialEq, Eq)]
struct Canonical {
    events: Vec<String>,
    state: String,
    wait_reason: String,
    receipts: Vec<String>,
    review_state: String,
}

fn canonical_from_json(lines: &[serde_json::Value], task_hex: &str) -> Vec<String> {
    lines
        .iter()
        .filter(|l| l["task_id"].as_str() == Some(task_hex))
        .map(|l| l["event_type"].as_str().unwrap_or_default().to_owned())
        .collect()
}

#[test]
fn qual_ev_0126_an_identical_task_runs_the_same_through_the_desktop_client_and_the_headless_cli() {
    let core = core_bin();
    let tmp = tempfile::tempdir().unwrap();
    let data_dir = tmp.path().join("profile");
    std::fs::create_dir_all(&data_dir).unwrap();
    let repo_cli = parity_fixture(tmp.path(), "repo-cli");
    let repo_desktop = parity_fixture(tmp.path(), "repo-desktop");
    let base = scripted_model(parity_script(&parity_worktree(tmp.path())));
    let cli = Cli {
        data_dir: data_dir.clone(),
        core: core.clone(),
        env: vec![
            ("MODBIT_OPENAI_BASE_URL".into(), base.clone()),
            ("OPENAI_API_KEY".into(), String::new()),
            ("ANTHROPIC_API_KEY".into(), String::new()),
        ],
    };

    // ---- the headless surface, driven by the real CLI binary ----
    let (code, out, err) = cli.run(&["session", "create"]);
    assert_eq!(code, 0, "{err}");
    let sid_cli = out.trim().strip_prefix("session ").unwrap().to_owned();
    // The verb this task adds: without it a headless task never gets the
    // repository's hooks, so the same goal ran under different rules.
    let (code, out, err) = cli.run(&["workspace", "trust", "--session", &sid_cli, &repo_cli]);
    assert_eq!(code, 0, "{out}{err}");
    assert!(out.contains("scope=repository"), "{out}");
    let (code, out, err) = cli.run(&[
        "task",
        "create",
        "--session",
        &sid_cli,
        "--workspace",
        &repo_cli,
        "greet",
    ]);
    assert_eq!(code, 0, "{err}");
    let tid_cli = out.trim().strip_prefix("task ").unwrap().to_owned();
    let (code, out, err) = cli.run(&[
        "task",
        "run",
        "--session",
        &sid_cli,
        "--task",
        &tid_cli,
        "--endpoint",
        "openai",
        "--model",
        "gpt-5",
        "--wait",
    ]);
    assert_eq!(code, 2, "{out}{err}");
    assert!(out.contains("state=Waiting wait_reason=UserInput"), "{out}");
    let (code, out, _) = cli.run(&["question", "list", "--task", &tid_cli]);
    assert_eq!(code, 0, "{out}");
    let qid_cli = out
        .lines()
        .find_map(|l| l.strip_prefix("question "))
        .unwrap()
        .split(' ')
        .next()
        .unwrap()
        .to_owned();
    // Steering is a Core contract for every client kind (docs/11), so it is
    // part of what the two surfaces must do identically.
    let (code, out, err) = cli.run(&[
        "task",
        "steer",
        "--session",
        &sid_cli,
        "--task",
        &tid_cli,
        "--mode",
        "FOLLOW_UP",
        "and now the other greeting",
    ]);
    assert_eq!(code, 0, "{out}{err}");
    assert!(
        out.contains("mode=FOLLOW_UP") && out.contains("sequence="),
        "{out}"
    );
    // The protected effect the plan declared waits for a decision, and a
    // headless operator makes it from another shell while the run holds.
    let approver = {
        let cli2 = Cli {
            data_dir: data_dir.clone(),
            core: core.clone(),
            env: cli.env.clone(),
        };
        let sid2 = sid_cli.clone();
        std::thread::spawn(move || {
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
        &sid_cli,
        "--task",
        &tid_cli,
        "--question",
        &qid_cli,
        "--option",
        "hi",
        "--endpoint",
        "openai",
        "--model",
        "gpt-5",
        "--wait",
    ]);
    assert_eq!(code, 0, "{out}{err}");
    assert!(
        approver.join().unwrap(),
        "the protected effect was approved from another shell"
    );
    let (code, out, err) = cli.run(&[
        "review",
        "decide",
        "--session",
        &sid_cli,
        "--task",
        &tid_cli,
        "accept",
        "ok",
    ]);
    assert_eq!(code, 0, "{out}{err}");
    assert!(out.contains("review decided state=Completed"), "{out}");

    // ---- the desktop surface, driven by a Desktop-kind protocol client on
    // ---- the same Core the CLI spawned
    let ready = ReadyLine::parse(
        std::fs::read_to_string(data_dir.join("core.ready"))
            .expect("the CLI's Core left a ready line")
            .trim(),
    )
    .expect("ready line");
    let secret = decode_hex(&ready.boot_secret_hex).expect("boot secret");
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    let desktop = rt.block_on(drive_desktop(&ready.endpoint, &secret, &repo_desktop));

    // ---- the same canonical states ----
    let (code, out, err) = cli.run(&[
        "events",
        "tail",
        "--session",
        &sid_cli,
        "--after",
        "0",
        "--count",
        "2000",
        "--json",
    ]);
    assert_eq!(code, 0, "{err}");
    let cli_lines: Vec<serde_json::Value> = out
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    let headless = Canonical {
        events: canonical_from_json(&cli_lines, &tid_cli),
        state: "Completed".into(),
        wait_reason: String::new(),
        receipts: {
            let (_, out, _) = cli.run(&["receipts", "--task", &tid_cli]);
            let mut kinds: Vec<String> = out
                .lines()
                .filter(|l| l.starts_with("receipt "))
                .map(|l| {
                    let field = |k: &str| {
                        l.split_whitespace()
                            .find_map(|w| w.strip_prefix(k))
                            .unwrap_or_default()
                    };
                    format!(
                        "{}/{}",
                        field("status="),
                        decision_shape(field("decision="))
                    )
                })
                .collect();
            kinds.sort();
            kinds
        },
        review_state: "Completed".into(),
    };
    assert_eq!(
        headless, desktop,
        "the same task must reach the same canonical states through either surface\nheadless: {headless:#?}\ndesktop: {desktop:#?}"
    );
    // The hook the repository declared ran on both surfaces: trust is what put
    // it in force, and the headless surface can now grant it.
    assert!(
        headless.events.iter().any(|e| e == "HookInvoked"),
        "the repository's hook ran headlessly: {:?}",
        headless.events
    );

    // ---- the declared differences ----
    // The task's provenance records which surface authored it, and nothing else.
    let origins: Vec<&str> = cli_lines
        .iter()
        .filter(|l| l["event_type"] == "TaskCreated")
        .filter_map(|l| l["payload"]["origin"].as_str())
        .collect();
    assert!(origins.contains(&"cli"), "{origins:?}");
    // A UI-only surface is refused to a headless connection at the transport,
    // and the task it named stays valid (REQ-EV-0043, docs/30).
    let refusal = rt.block_on(code_view_as_cli(
        &ready.endpoint,
        &secret,
        parse_test_id(&tid_cli),
    ));
    assert_eq!(
        refusal, "CLIENT_CAPABILITY",
        "a headless client holds no ui.code_view"
    );
    let (code, out, _) = cli.run(&["task", "status", "--task", &tid_cli]);
    assert_eq!(code, 0, "the refused UI request left the task valid: {out}");
    assert!(out.contains("state=Completed"), "{out}");

    // ---- the rest of the contracts this task made reachable headlessly ----
    // Recovery evidence: the same report the desktop reads after a restart.
    let (code, out, err) = cli.run(&["recovery", "show"]);
    assert_eq!(code, 0, "{err}");
    assert!(
        out.contains("recovery boot_generation=") && out.contains("tasks="),
        "{out}"
    );
    // Onboarding: the stacks the repository was detected as.
    let (code, out, err) = cli.run(&["starter", "list", "--workspace", &repo_cli]);
    assert_eq!(code, 0, "{err}");
    assert!(out.starts_with("stacks "), "{out}");
    // A pull request is bound to the revision the review accepted, and the
    // binding is checked before anything reaches a forge.
    let (code, out, _) = cli.run(&[
        "pr",
        "open",
        "--session",
        &sid_cli,
        "--task",
        &tid_cli,
        "--revision",
        "99",
    ]);
    assert_ne!(code, 0, "a stale revision opens no pull request: {out}");

    // ---- the provider credential never leaves the Core's custody ----
    let key = "sk-parity-0126-secret";
    let with_key = Cli {
        data_dir: data_dir.clone(),
        core: core.clone(),
        env: {
            let mut e = cli.env.clone();
            e.push(("MODBIT_PROVIDER_API_KEY".into(), key.into()));
            e
        },
    };
    let (code, out, err) = with_key.run(&[
        "provider",
        "configure",
        "--provider",
        "openai",
        "--base-url",
        &base,
    ]);
    assert_eq!(code, 0, "{err}");
    assert!(out.contains("credential=available"), "{out}");
    assert!(
        !out.contains(key) && !err.contains(key),
        "the CLI never echoes a credential"
    );
    // Nor does it accept one on the command line, where a process list would
    // read it.
    let (code, _, err) = cli.run(&[
        "provider",
        "configure",
        "--provider",
        "openai",
        "--api-key",
        key,
    ]);
    assert_eq!(code, 1, "{err}");
    assert!(err.contains("never goes on the command line"), "{err}");
    // And nothing under the data directory carries the value.
    let mut found = Vec::new();
    scan_for(&data_dir, key.as_bytes(), &mut found);
    assert!(found.is_empty(), "a credential reached storage: {found:?}");
}

fn parse_test_id(hex: &str) -> Id {
    Id {
        value: decode_hex(hex).expect("hex id"),
    }
}

/// Every file under `dir` whose bytes contain `needle`.
fn scan_for(dir: &Path, needle: &[u8], found: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for e in entries.flatten() {
        let p = e.path();
        if p.is_dir() {
            scan_for(&p, needle, found);
        } else if let Ok(bytes) = std::fs::read(&p)
            && bytes.windows(needle.len()).any(|w| w == needle)
        {
            found.push(p);
        }
    }
}

/// `GetCodeView` from a headless connection: the code the transport refuses it
/// with.
async fn code_view_as_cli(endpoint: &Endpoint, secret: &[u8], task_id: Id) -> String {
    let mut c = Client::connect(endpoint, secret, ClientKind::Cli, "test")
        .await
        .expect("connect");
    match c
        .command(envelope(
            "GetCodeView",
            GetCodeView {
                task_id: Some(task_id),
                path: "a.txt".into(),
                expected_file_revision: String::new(),
            }
            .encode_to_vec(),
        ))
        .await
    {
        Err(modbit_protocol::client::ClientError::Rejected { code, .. }) => code,
        Err(e) => panic!("unexpected transport error: {e}"),
        Ok(_) => panic!("a headless client must not be served a UI-only surface"),
    }
}

/// The same task, driven start to finish by a Desktop-kind client.
async fn drive_desktop(endpoint: &Endpoint, secret: &[u8], repo: &str) -> Canonical {
    let mut c = Client::connect(endpoint, secret, ClientKind::Desktop, "test")
        .await
        .expect("connect");
    let ack = c
        .command(envelope(
            "CreateSession",
            CreateSession { space_id: None }.encode_to_vec(),
        ))
        .await
        .expect("session");
    let created: SessionCreated = Client::result(&ack).expect("session");
    let sid = created.session_id.expect("session id");
    let ack = c
        .command(envelope(
            "AcquireSessionLease",
            AcquireSessionLease {
                session_id: Some(sid.clone()),
                owner: "desktop test".into(),
            }
            .encode_to_vec(),
        ))
        .await
        .expect("lease");
    let lease: SessionLeaseAcquired = Client::result(&ack).expect("lease");
    let g = Some(lease.lease_generation);
    let ack = c
        .command(envelope_fenced(
            "TrustRepository",
            TrustRepository {
                session_id: Some(sid.clone()),
                workspace_root: repo.into(),
                scope: "repository".into(),
            }
            .encode_to_vec(),
            g,
        ))
        .await
        .expect("trust");
    let _: RepositoryTrusted = Client::result(&ack).expect("trust");
    let ack = c
        .command(envelope_fenced(
            "CreateTask",
            CreateTask {
                session_id: Some(sid.clone()),
                goal_text: "greet".into(),
                workspace_id: None,
                execution_profile: String::new(),
                origin: "desktop".into(),
                workspace_root: repo.into(),
                issue_url: String::new(),
                issue_json: String::new(),
            }
            .encode_to_vec(),
            g,
        ))
        .await
        .expect("task");
    let task: TaskCreated = Client::result(&ack).expect("task");
    let tid = task.task_id.expect("task id");
    start_desktop_run(&mut c, &tid, g).await;
    let status = wait_for_desktop(&mut c, &sid, &tid, g, &["Waiting"]).await;
    assert_eq!(status.wait_reason, "UserInput", "{status:?}");
    let ack = c
        .command(envelope(
            "ListQuestions",
            ListQuestions {
                task_id: Some(tid.clone()),
            }
            .encode_to_vec(),
        ))
        .await
        .expect("questions");
    let questions: QuestionList = Client::result(&ack).expect("questions");
    let q = questions
        .questions
        .into_iter()
        .find(|q| !q.answered)
        .expect("an open question");
    let ack = c
        .command(envelope_fenced(
            "QueueInput",
            QueueInput {
                task_id: Some(tid.clone()),
                input_id: "parity-follow-up".into(),
                mode: "FOLLOW_UP".into(),
                text: "and now the other greeting".into(),
            }
            .encode_to_vec(),
            g,
        ))
        .await
        .expect("steer");
    let _: InputQueued = Client::result(&ack).expect("steer");
    let ack = c
        .command(envelope_fenced(
            "RespondToQuestion",
            RespondToQuestion {
                task_id: Some(tid.clone()),
                question_id: q.question_id,
                option_id: "hi".into(),
                text: String::new(),
            }
            .encode_to_vec(),
            g,
        ))
        .await
        .expect("answer");
    let _: QuestionResponded = Client::result(&ack).expect("answer");
    start_desktop_run(&mut c, &tid, g).await;
    wait_for_desktop(&mut c, &sid, &tid, g, &["ReadyForReview"]).await;
    let ack = c
        .command(envelope_fenced(
            "DecideReview",
            DecideReview {
                task_id: Some(tid.clone()),
                decision: "ACCEPT".into(),
                rejected: vec![],
                note: "ok".into(),
                expected_workspace_revision: 0,
            }
            .encode_to_vec(),
            g,
        ))
        .await
        .expect("review");
    let decided: ReviewDecided = Client::result(&ack).expect("review");
    let status = wait_for_desktop(&mut c, &sid, &tid, g, &["Completed"]).await;
    let ack = c
        .command(envelope(
            "GetEffectReceipts",
            GetEffectReceipts {
                task_id: Some(tid.clone()),
            }
            .encode_to_vec(),
        ))
        .await
        .expect("receipts");
    let receipts: EffectReceiptList = Client::result(&ack).expect("receipts");
    let mut kinds: Vec<String> = receipts
        .receipts
        .into_iter()
        .map(|r| format!("{}/{}", r.status, decision_shape(&r.policy_decision)))
        .collect();
    kinds.sort();
    let task_hex = encode_hex(&tid.value);
    c.subscribe(sid, 0).await.expect("subscribe");
    let mut events = Vec::new();
    while let Ok(Ok(Some(e))) =
        tokio::time::timeout(std::time::Duration::from_millis(500), c.next_event()).await
    {
        let ev = e.event.unwrap_or_default();
        if ev.task_id.as_ref().map(|t| encode_hex(&t.value)).as_deref() == Some(task_hex.as_str()) {
            events.push(ev.event_type);
        }
    }
    Canonical {
        events,
        state: status.state,
        wait_reason: status.wait_reason,
        receipts: kinds,
        review_state: decided.task_state,
    }
}

async fn start_desktop_run(c: &mut Client, task_id: &Id, generation: Option<u64>) {
    let ack = c
        .command(envelope_fenced(
            "StartTask",
            StartTask {
                task_id: Some(task_id.clone()),
                endpoint: "openai".into(),
                model: "gpt-5".into(),
                max_turns: 0,
                max_tool_calls: 0,
                max_no_progress_turns: 0,
                skills: vec![],
            }
            .encode_to_vec(),
            generation,
        ))
        .await
        .expect("start");
    let _: TaskRunStarted = Client::result(&ack).expect("start");
}

/// Poll the task's status until it reaches one of `states`, approving the
/// protected effect the run waits on — the decision the CLI's operator makes
/// from another shell.
async fn wait_for_desktop(
    c: &mut Client,
    session_id: &Id,
    task_id: &Id,
    generation: Option<u64>,
    states: &[&str],
) -> TaskStatus {
    for _ in 0..1_800 {
        resolve_desktop_approvals(c, session_id, generation).await;
        let ack = c
            .command(envelope(
                "GetTaskStatus",
                GetTaskStatus {
                    task_id: Some(task_id.clone()),
                }
                .encode_to_vec(),
            ))
            .await
            .expect("status");
        let status: TaskStatus = Client::result(&ack).expect("status");
        if states.contains(&status.state.as_str()) && !status.loop_alive {
            return status;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    panic!("the task never reached {states:?}");
}

/// Approve every approval the task's run is waiting on.
async fn resolve_desktop_approvals(c: &mut Client, session_id: &Id, generation: Option<u64>) {
    let Ok(ack) = c
        .command(envelope(
            "ListApprovals",
            ListApprovals {
                session_id: Some(session_id.clone()),
            }
            .encode_to_vec(),
        ))
        .await
    else {
        return;
    };
    let Ok(list) = Client::result::<ApprovalList>(&ack) else {
        return;
    };
    for a in list
        .approvals
        .into_iter()
        .filter(|a| a.status == "REQUESTED")
    {
        let ack = c
            .command(envelope_fenced(
                "ResolveApproval",
                ResolveApproval {
                    approval_id: a.approval_id,
                    approve: true,
                    reason: "ok".into(),
                    intent_hash: a.intent_hash,
                }
                .encode_to_vec(),
                generation,
            ))
            .await
            .expect("approve");
        let r: ApprovalResolvedAck = Client::result(&ack).expect("approve");
        assert_eq!(r.status, "APPROVED", "{r:?}");
    }
}

/// IMP-EV-0142 through the CLI (docs/71): `doctor` reports a clean store and
/// chains and the provider without its key; `trace` lists a task's events by
/// type with no payloads; `export diagnostics` writes a sealed package with
/// no credential in it; `diagnostics verify` replays it (and refuses it once
/// changed); `export handoff` writes M8.7's bundle from a headless caller.
#[test]
fn imp_ev_0142_doctor_trace_export_verify_and_handoff_through_the_cli() {
    const KEY: &str = "sk-modbit-cli-0142-provider-key-5e3b1f";
    let core = core_bin();
    let tmp = tempfile::tempdir().unwrap();
    let data_dir = tmp.path().join("profile");
    std::fs::create_dir_all(&data_dir).unwrap();
    let repo = tmp.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    std::fs::write(repo.join("a.txt"), "a\n").unwrap();
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
    let repo_str = repo
        .canonicalize()
        .unwrap()
        .to_string_lossy()
        .trim_start_matches(r"\\?\")
        .to_owned();
    let script = vec![
        serde_json::json!({"calls": [{"name": "plan.update", "args": {"outcome": "read a", "expected_files": []}}]}),
        serde_json::json!({"calls": [{"name": "fs.read", "args": {"path": "a.txt"}}]}),
        serde_json::json!({"calls": [{"name": "task.complete", "args": {"summary": "read", "self_review": {"findings": []}}}]}),
    ];
    let base = scripted_model(script);
    let cli = Cli {
        data_dir: data_dir.clone(),
        core: core.clone(),
        env: vec![
            ("MODBIT_OPENAI_BASE_URL".into(), base.clone()),
            ("OPENAI_API_KEY".into(), KEY.into()),
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
        "read a",
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
        "--model",
        "gpt-5",
        "--wait",
    ]);
    assert_eq!(code, 0, "{out}{err}");

    let (code, out, err) = cli.run(&["doctor", "--session", &sid]);
    assert_eq!(code, 0, "{out}{err}");
    assert!(out.contains("integrity database=ok"), "{out}");
    assert!(out.contains("receipts=valid"), "{out}");
    assert!(
        out.contains("provider openai kind=OpenAi host=127.0.0.1:")
            && out.contains("credential=configured"),
        "{out}"
    );
    assert!(!out.contains(KEY), "{out}");

    let (code, out, err) = cli.run(&["trace", "--session", &sid, "--task", &tid]);
    assert_eq!(code, 0, "{err}");
    for t in [
        "TaskCreated",
        "RunCreated",
        "ToolCallSucceeded",
        "TaskReadyForReview",
    ] {
        assert!(out.contains(t), "{t}: {out}");
    }
    assert!(
        !out.contains("hello") && !out.contains(KEY),
        "metadata only: {out}"
    );

    let file = tmp.path().join("diagnostics.json");
    let file_str = file.to_string_lossy().to_string();
    let (code, out, err) = cli.run(&[
        "export",
        "diagnostics",
        "--session",
        &sid,
        "--include-content",
        "--out",
        &file_str,
    ]);
    assert_eq!(code, 0, "{out}{err}");
    assert!(out.starts_with("exported digest="), "{out}");
    let written = std::fs::read_to_string(&file).unwrap();
    assert!(!written.contains(KEY), "no credential in the package");
    let (code, out, err) = cli.run(&["diagnostics", "verify", &file_str]);
    assert_eq!(code, 0, "{out}{err}");
    assert!(out.starts_with("verified=true digest_ok=true"), "{out}");
    // Changed after export: it no longer replays, and the CLI says so.
    let mut pkg: serde_json::Value = serde_json::from_str(&written).unwrap();
    pkg["aggregates"][0]["head_hash"] = serde_json::json!("0".repeat(64));
    std::fs::write(&file, pkg.to_string()).unwrap();
    let (code, out, _) = cli.run(&["diagnostics", "verify", &file_str]);
    assert_ne!(code, 0, "{out}");
    assert!(out.starts_with("verified=false digest_ok=false"), "{out}");

    let bundle = tmp.path().join("handoff");
    let (code, out, err) = cli.run(&[
        "export",
        "handoff",
        "--session",
        &sid,
        "--task",
        &tid,
        "--out",
        &bundle.to_string_lossy(),
    ]);
    assert_eq!(code, 0, "{out}{err}");
    assert!(out.starts_with("handoff bundle="), "{out}");
    assert!(bundle.join("manifest.json").exists(), "{out}");
}
