//! PX-086: the `automation` verbs of the headless CLI over a real Core —
//! validate, create, show, the exact-hash enable approval, list, a run, the
//! history, and the kill switches. The CLI holds no definition, scheduler or
//! policy; what these assertions see is what the Core answered. No model is
//! configured, so a run ends with the Core's typed refusal to start it, which
//! is itself the unattended behaviour under test: nothing hangs, nothing is
//! auto-approved, and the outcome is recorded.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;

struct Cli {
    data_dir: PathBuf,
    core: PathBuf,
    /// Extra environment (a scripted model for the runs that need one).
    env: Vec<(String, String)>,
}

impl Cli {
    fn run(&self, args: &[&str]) -> (i32, String, String) {
        let out = Command::new(env!("CARGO_BIN_EXE_modbit-cli"))
            .env("MODBIT_CORE_BIN", &self.core)
            .env("OPENAI_API_KEY", "")
            .env("ANTHROPIC_API_KEY", "")
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
        assert_eq!(code, 0, "{args:?}: {out}{err}");
        out
    }

    fn fails(&self, args: &[&str]) -> String {
        let (code, out, err) = self.run(args);
        assert_ne!(code, 0, "{args:?} should fail: {out}{err}");
        format!("{out}{err}")
    }
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

fn word_after<'a>(text: &'a str, key: &str) -> &'a str {
    text.split_whitespace()
        .find_map(|w| w.strip_prefix(key))
        .unwrap_or_else(|| panic!("`{key}` in {text}"))
}

#[test]
fn automation_verbs_validate_create_approve_run_and_kill_through_the_core() {
    let cli_exe = PathBuf::from(env!("CARGO_BIN_EXE_modbit-cli"));
    let core = cli_exe.parent().unwrap().join(if cfg!(windows) {
        "modbit-core.exe"
    } else {
        "modbit-core"
    });
    assert!(
        core.exists(),
        "{} (build modbit-core first)",
        core.display()
    );
    let tmp = tempfile::tempdir().unwrap();
    let data_dir = tmp.path().join("profile");
    std::fs::create_dir_all(&data_dir).unwrap();
    let repo = tmp.path().join("repo");
    std::fs::create_dir_all(repo.join(".modbit")).unwrap();
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
    let root = repo
        .canonicalize()
        .unwrap()
        .to_string_lossy()
        .trim_start_matches(r"\\?\")
        .to_owned();
    let cli = Cli {
        data_dir,
        core,
        env: vec![],
    };
    let out = cli.ok(&["session", "create"]);
    let sid = out.trim().strip_prefix("session ").unwrap().to_owned();
    cli.ok(&["workspace", "trust", "--session", &sid, &root]);

    // Validation names every problem and exits non-zero.
    let bad = tmp.path().join("bad.json");
    std::fs::write(
        &bad,
        r#"{"schema":"modbit.automation/1","name":"bad","prompt":"p",
            "triggers":[{"kind":"schedule","id":"fast","cron":"*/2 * * * *"}],
            "profile":{"capabilities":["computer.control"]}}"#,
    )
    .unwrap();
    let out = cli.fails(&["automation", "validate", bad.to_str().unwrap()]);
    assert!(out.contains("BELOW_FLOOR"), "{out}");
    assert!(out.contains("UNKNOWN_CAPABILITY"), "{out}");

    let good = tmp.path().join("drift.json");
    std::fs::write(
        &good,
        r#"{"schema":"modbit.automation/1","name":"drift",
            "prompt":"Report whether the default branch moved ahead.",
            "triggers":[{"kind":"manual","id":"now"},
                        {"kind":"schedule","id":"nightly","cron":"0 3 * * *"}]}"#,
    )
    .unwrap();
    let out = cli.ok(&["automation", "validate", good.to_str().unwrap()]);
    assert!(out.starts_with("valid drift"), "{out}");
    assert!(out.contains("trigger nightly schedule"), "{out}");

    let out = cli.ok(&[
        "automation",
        "create",
        "--workspace",
        &root,
        good.to_str().unwrap(),
    ]);
    let id = out
        .lines()
        .next()
        .unwrap()
        .split_whitespace()
        .nth(1)
        .unwrap()
        .to_owned();
    assert!(out.contains("state=NEEDS_APPROVAL"), "{out}");
    let hash = out
        .lines()
        .find_map(|l| l.strip_prefix("hash "))
        .unwrap()
        .trim()
        .to_owned();

    // A run before approval is refused with the Core's typed code.
    let err = cli.fails(&["automation", "run", &id]);
    assert!(err.contains("NOT_ENABLED"), "{err}");
    // The approval names the exact hash.
    let err = cli.fails(&[
        "automation",
        "enable",
        &id,
        "--version",
        "1",
        "--hash",
        &"0".repeat(64),
    ]);
    assert!(err.contains("APPROVAL_MISMATCH"), "{err}");
    let out = cli.ok(&[
        "automation",
        "enable",
        &id,
        "--version",
        "1",
        "--hash",
        &hash,
    ]);
    assert!(out.contains("state=ENABLED"), "{out}");
    let shown = cli.ok(&["automation", "show", &id]);
    assert!(shown.contains(&format!("hash {hash}")), "{shown}");
    assert!(shown.contains("trigger nightly schedule"), "{shown}");
    let listed: serde_json::Value =
        serde_json::from_str(cli.ok(&["automation", "list", "--json"]).trim()).unwrap();
    assert_eq!(listed["automations"][0]["state"], "ENABLED");
    assert_eq!(listed["automations"][0]["definition_hash"], hash.as_str());

    // A run with no model configured ends typed, once, and is in the history.
    let out = cli.ok(&["automation", "run", &id, "--event-id", "by-hand-1"]);
    assert!(out.contains("status=failed"), "{out}");
    assert!(out.contains("START_REFUSED"), "{out}");
    let out = cli.ok(&["automation", "run", &id, "--event-id", "by-hand-1"]);
    assert!(out.contains("reason=DUPLICATE"), "{out}");
    let hist: serde_json::Value =
        serde_json::from_str(cli.ok(&["automation", "history", &id, "--json"]).trim()).unwrap();
    let rows = hist.as_array().unwrap();
    assert!(rows.iter().any(|r| r["reason"] == "DUPLICATE"));
    let failed = rows
        .iter()
        .find(|r| r["status"] == "failed")
        .expect("the failed run is recorded");
    assert!(
        failed["reason"]
            .as_str()
            .unwrap()
            .starts_with("START_REFUSED"),
        "{failed}"
    );
    assert_eq!(failed["event_id"], "by-hand-1");

    // The kill switches hold new firings.
    let out = cli.ok(&["automation", "pause", &id, "pausing"]);
    assert!(out.contains("paused"), "{out}");
    let out = cli.ok(&["automation", "run", &id, "--event-id", "by-hand-2"]);
    assert!(out.contains("reason=PAUSED"), "{out}");
    cli.ok(&["automation", "resume", &id]);
    let out = cli.ok(&["automation", "kill", "all", "stop everything"]);
    assert!(out.contains("killed"), "{out}");
    let out = cli.ok(&["automation", "list"]);
    assert!(out.contains("global_paused=true"), "{out}");
    let out = cli.ok(&["automation", "run", &id, "--event-id", "by-hand-3"]);
    assert!(out.contains("reason=PAUSED"), "{out}");
    cli.ok(&["automation", "resume", "all"]);
    let out = cli.ok(&["automation", "list"]);
    assert!(out.contains("global_paused=false"), "{out}");
    let _ = word_after(&out, "clock=");
}

/// A scripted OpenAI-compatible model over real HTTP (one reply per number of
/// tool results seen so far): the repository's standard stand-in for a model.
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

/// Poll `history <id> --json` until `want` holds of the rows.
fn history_until(
    cli: &Cli,
    id: &str,
    what: &str,
    want: impl Fn(&[serde_json::Value]) -> bool,
) -> Vec<serde_json::Value> {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(90);
    loop {
        let h: serde_json::Value =
            serde_json::from_str(cli.ok(&["automation", "history", id, "--json"]).trim()).unwrap();
        let rows = h.as_array().cloned().unwrap_or_default();
        if want(&rows) {
            return rows;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "timed out waiting for {what}: {rows:?}"
        );
        std::thread::sleep(std::time::Duration::from_millis(200));
    }
}

/// QUAL-PX-086: every operation the desktop performs is reproducible from the
/// CLI with the same result, on a real Core that runs the agent for real: the
/// shipped template, the exact approval, a real run, a dry run, an edit that
/// needs re-approval, a repository definition (data until approved), a
/// delivered event with its filter, acknowledging attention, and disabling.
#[test]
fn automation_verbs_reproduce_every_desktop_operation_on_a_real_run() {
    let cli_exe = PathBuf::from(env!("CARGO_BIN_EXE_modbit-cli"));
    let core = cli_exe.parent().unwrap().join(if cfg!(windows) {
        "modbit-core.exe"
    } else {
        "modbit-core"
    });
    assert!(
        core.exists(),
        "{} (build modbit-core first)",
        core.display()
    );
    let tmp = tempfile::tempdir().unwrap();
    let data_dir = tmp.path().join("profile");
    std::fs::create_dir_all(&data_dir).unwrap();
    let repo = tmp.path().join("repo");
    std::fs::create_dir_all(repo.join(".modbit/automations")).unwrap();
    std::fs::write(repo.join("a.txt"), "a\n").unwrap();
    // A completion needs a configured check (FIX-03).
    std::fs::write(
        repo.join(".modbit/verification.json"),
        r#"{"commands": [{"id": "fixture-noop", "argv": ["git", "--version"]}]}"#,
    )
    .unwrap();
    std::fs::write(
        repo.join(".modbit/automations/on-main.json"),
        r#"{"schema":"modbit.automation/1","name":"on-main","prompt":"Look at the change.",
            "triggers":[{"kind":"event","id":"pr","source":"forge","event":"pull_request",
                         "filters":{"branches":["main"]}}]}"#,
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
    let root = repo
        .canonicalize()
        .unwrap()
        .to_string_lossy()
        .trim_start_matches(r"\\?\")
        .to_owned();
    let script = vec![
        serde_json::json!({"calls": [{"name": "fs.read", "args": {"path": "a.txt"}}]}),
        serde_json::json!({"calls": [{"name": "plan.update", "args": {"outcome": "report", "expected_files": [], "verification": [], "protected_effects": []}}]}),
        serde_json::json!({"calls": [{"name": "task.complete", "args": {"summary": "up to date", "self_review": {"findings": [{"text": "read only", "resolved": true}], "verification": []}}}]}),
    ];
    let base = scripted_model(script);
    let cli = Cli {
        data_dir,
        core,
        env: vec![
            ("MODBIT_OPENAI_BASE_URL".into(), base),
            ("MODBIT_DEFAULT_MODEL".into(), "gpt-5-mini".into()),
        ],
    };
    let out = cli.ok(&["session", "create"]);
    let sid = out.trim().strip_prefix("session ").unwrap().to_owned();
    cli.ok(&["workspace", "trust", "--session", &sid, &root]);

    // The shipped template, created and approved exactly as shown.
    let templates = cli.ok(&["automation", "templates"]);
    assert!(templates.contains("default-branch-drift"), "{templates}");
    let out = cli.ok(&[
        "automation",
        "create",
        "--workspace",
        &root,
        "--template",
        "default-branch-drift",
    ]);
    let id = out
        .lines()
        .next()
        .unwrap()
        .split_whitespace()
        .nth(1)
        .unwrap()
        .to_owned();
    let hash = out
        .lines()
        .find_map(|l| l.strip_prefix("hash "))
        .unwrap()
        .trim()
        .to_owned();
    let out = cli.ok(&[
        "automation",
        "enable",
        &id,
        "--version",
        "1",
        "--hash",
        &hash,
        "--as-shown",
    ]);
    assert!(
        out.contains("state=ENABLED") && out.contains("effects=read_only"),
        "{out}"
    );

    // A real run: the agent runs, finishes, and the history says why.
    let out = cli.ok(&["automation", "run", &id, "--event-id", "real-1"]);
    assert!(out.contains("status=running"), "{out}");
    let rows = history_until(&cli, &id, "the real run", |r| {
        r.iter()
            .any(|x| x["event_id"] == "real-1" && x["status"] == "succeeded")
    });
    let real = rows.iter().find(|x| x["event_id"] == "real-1").unwrap();
    assert_eq!(real["reason"], "TASK_COMPLETED", "{real}");
    assert_eq!(real["test"], false);
    assert!(real["principal"].as_str().unwrap().starts_with("user:"));
    // The same history by task.
    let task = real["task_id"].as_str().unwrap().to_owned();
    let by_task = cli.ok(&["automation", "history", "--task", &task]);
    assert!(by_task.contains("status=succeeded"), "{by_task}");

    // A dry run: its row is marked, the reads-only posture is reported, and
    // the tree is unchanged.
    let out = cli.ok(&["automation", "run", &id, "--test", "--event-id", "dry-1"]);
    assert!(out.contains("status=running"), "{out}");
    let rows = history_until(&cli, &id, "the dry run", |r| {
        r.iter()
            .any(|x| x["event_id"] == "dry-1" && x["status"] == "succeeded")
    });
    let dry = rows.iter().find(|x| x["event_id"] == "dry-1").unwrap();
    assert_eq!(dry["test"], true);
    assert!(
        dry["outputs"]
            .as_str()
            .unwrap()
            .contains("\"dry_run\":true"),
        "{dry}"
    );
    let status = Command::new("git")
        .arg("-C")
        .arg(&repo)
        .args(["status", "--porcelain"])
        .output()
        .unwrap();
    assert!(
        String::from_utf8_lossy(&status.stdout).trim().is_empty(),
        "the tree is unchanged"
    );

    // An edit is a new version: disabled until approved again.
    let template_json = shipped_template(&cli);
    let v2 = tmp.path().join("v2.json");
    std::fs::write(
        &v2,
        template_json.replace("Report whether", "Please report whether"),
    )
    .unwrap();
    let out = cli.ok(&["automation", "update", &id, v2.to_str().unwrap()]);
    assert!(
        out.contains("state=NEEDS_APPROVAL") && out.contains("version=2"),
        "{out}"
    );
    let err = cli.fails(&["automation", "run", &id, "--event-id", "after-edit"]);
    assert!(err.contains("NOT_ENABLED"), "{err}");
    let hash2 = out
        .lines()
        .find_map(|l| l.strip_prefix("hash "))
        .unwrap()
        .trim()
        .to_owned();
    assert_ne!(hash2, hash);
    let err = cli.fails(&[
        "automation",
        "enable",
        &id,
        "--version",
        "2",
        "--hash",
        &hash,
        "--as-shown",
    ]);
    assert!(err.contains("APPROVAL_MISMATCH"), "{err}");
    let out = cli.ok(&[
        "automation",
        "enable",
        &id,
        "--version",
        "2",
        "--hash",
        &hash2,
        "--as-shown",
    ]);
    assert!(
        out.contains("state=ENABLED") && out.contains("version=2"),
        "{out}"
    );

    // A repository definition is data until its exact bytes are approved.
    let out = cli.ok(&["automation", "load", "--workspace", &root]);
    assert!(
        out.contains("on-main")
            && out.contains("state=NEEDS_APPROVAL")
            && out.contains("source=repository"),
        "{out}"
    );
    let repo_id = out
        .lines()
        .next()
        .unwrap()
        .split_whitespace()
        .nth(1)
        .unwrap()
        .to_owned();
    let repo_hash = out
        .lines()
        .find_map(|l| l.trim().strip_prefix("hash "))
        .unwrap()
        .trim()
        .to_owned();
    let payload = tmp.path().join("pr.json");
    std::fs::write(
        &payload,
        r#"{"action":"opened","branch":"main","title":"t","text":"hello"}"#,
    )
    .unwrap();
    let out = cli.ok(&[
        "automation",
        "fire",
        "--source",
        "forge",
        "--event",
        "pull_request",
        "--delivery",
        "d-1",
        payload.to_str().unwrap(),
    ]);
    assert!(
        out.contains("matched=0"),
        "an unapproved repository definition fires nothing: {out}"
    );
    cli.ok(&[
        "automation",
        "enable",
        &repo_id,
        "--version",
        "1",
        "--hash",
        &repo_hash,
        "--as-shown",
    ]);
    // A dry run honours the trigger's filters and says when they would skip.
    let dev = tmp.path().join("dev.json");
    std::fs::write(&dev, r#"{"action":"opened","branch":"dev"}"#).unwrap();
    let out = cli.ok(&[
        "automation",
        "run",
        &repo_id,
        "--test",
        "--trigger",
        "pr",
        "--source",
        "forge",
        "--event",
        "pull_request",
        "--payload-file",
        dev.to_str().unwrap(),
    ]);
    assert!(out.contains("filters would have skipped"), "{out}");
    // A delivered event runs it once; the same delivery again is a duplicate.
    let out = cli.ok(&[
        "automation",
        "fire",
        "--source",
        "forge",
        "--event",
        "pull_request",
        "--delivery",
        "d-2",
        payload.to_str().unwrap(),
    ]);
    assert!(out.contains("matched=1"), "{out}");
    let rows = history_until(&cli, &repo_id, "the event run", |r| {
        r.iter()
            .any(|x| x["event_id"] == "d-2" && x["status"] == "succeeded")
    });
    assert_eq!(rows.iter().filter(|x| x["event_id"] == "d-2").count(), 1);
    let out = cli.ok(&[
        "automation",
        "fire",
        "--source",
        "forge",
        "--event",
        "pull_request",
        "--delivery",
        "d-2",
        payload.to_str().unwrap(),
    ]);
    assert!(out.contains("reason=DUPLICATE"), "{out}");

    // Disable, and acknowledge a failure the attention list holds.
    let out = cli.ok(&["automation", "disable", &id, "retired"]);
    assert!(out.contains("state=DISABLED"), "{out}");
    let err = cli.fails(&["automation", "run", &id, "--event-id", "after-disable"]);
    assert!(err.contains("NOT_ENABLED"), "{err}");
    let ack = cli.ok(&["automation", "ack", real["dispatch_key"].as_str().unwrap()]);
    assert!(ack.contains("acknowledged"), "{ack}");
}

/// The template as the Core ships it, through the CLI.
fn shipped_template(cli: &Cli) -> String {
    let t: serde_json::Value =
        serde_json::from_str(cli.ok(&["automation", "templates", "--json"]).trim()).unwrap();
    t.as_array()
        .and_then(|a| {
            a.iter()
                .find(|x| x["template_id"] == "default-branch-drift")
        })
        .map(|x| x["definition"].to_string())
        .unwrap()
}

/// AUT-E01 ("failures are counted and never dropped"): a manual run refused
/// because the repository file no longer holds the approved bytes leaves a
/// run record, listed by `automation history` with the Core's typed reason,
/// and dispatches nothing: no task, no session, no cost.
#[test]
fn a_manual_run_refused_for_changed_repository_bytes_is_in_the_history_and_started_nothing() {
    let cli_exe = PathBuf::from(env!("CARGO_BIN_EXE_modbit-cli"));
    let core = cli_exe.parent().unwrap().join(if cfg!(windows) {
        "modbit-core.exe"
    } else {
        "modbit-core"
    });
    assert!(
        core.exists(),
        "{} (build modbit-core first)",
        core.display()
    );
    let tmp = tempfile::tempdir().unwrap();
    let data_dir = tmp.path().join("profile");
    std::fs::create_dir_all(&data_dir).unwrap();
    let repo = tmp.path().join("repo");
    std::fs::create_dir_all(repo.join(".modbit/automations")).unwrap();
    std::fs::write(repo.join("a.txt"), "a\n").unwrap();
    let definition = repo.join(".modbit/automations/by-hand.json");
    std::fs::write(
        &definition,
        r#"{"schema":"modbit.automation/1","name":"by-hand","prompt":"Look at the change.",
            "triggers":[{"kind":"manual","id":"now"}]}"#,
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
    let root = repo
        .canonicalize()
        .unwrap()
        .to_string_lossy()
        .trim_start_matches(r"\\?\")
        .to_owned();
    let cli = Cli {
        data_dir,
        core,
        env: vec![],
    };
    let out = cli.ok(&["session", "create"]);
    let sid = out.trim().strip_prefix("session ").unwrap().to_owned();
    cli.ok(&["workspace", "trust", "--session", &sid, &root]);
    let out = cli.ok(&["automation", "load", "--workspace", &root]);
    let id = out
        .lines()
        .next()
        .unwrap()
        .split_whitespace()
        .nth(1)
        .unwrap()
        .to_owned();
    let hash = out
        .lines()
        .find_map(|l| l.trim().strip_prefix("hash "))
        .unwrap()
        .trim()
        .to_owned();
    let out = cli.ok(&[
        "automation",
        "enable",
        &id,
        "--version",
        "1",
        "--hash",
        &hash,
        "--as-shown",
    ]);
    assert!(out.contains("state=ENABLED"), "{out}");
    let before: serde_json::Value =
        serde_json::from_str(cli.ok(&["automation", "history", &id, "--json"]).trim()).unwrap();
    assert!(before.as_array().unwrap().is_empty());

    // One byte changes on disk; the run is refused with the typed code ...
    let mut text = std::fs::read_to_string(&definition).unwrap();
    text.push('\n');
    std::fs::write(&definition, text).unwrap();
    let err = cli.fails(&["automation", "run", &id, "--event-id", "by-hand-1"]);
    assert!(err.contains("SOURCE_CHANGED"), "{err}");

    // ... and the refusal is a row in the history, not nothing.
    let hist: serde_json::Value =
        serde_json::from_str(cli.ok(&["automation", "history", &id, "--json"]).trim()).unwrap();
    let rows = hist.as_array().unwrap();
    assert_eq!(rows.len(), 1, "{hist}");
    assert_eq!(rows[0]["status"], "skipped", "{hist}");
    assert_eq!(rows[0]["reason"], "SOURCE_CHANGED", "{hist}");
    assert_eq!(rows[0]["task_id"], "", "nothing was dispatched: {hist}");
    assert_eq!(rows[0]["session_id"], "", "{hist}");
    assert_eq!(rows[0]["cost_minor"], 0, "{hist}");
    let text = cli.ok(&["automation", "history", &id]);
    assert!(text.contains("SOURCE_CHANGED"), "{text}");
}
