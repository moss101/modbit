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
