//! PX-105 (QUAL-PX-105): the owner's side of skills from the headless CLI over
//! a real Core, with a real user-authored skill on disk and a scripted
//! OpenAI-compatible server that records every request body. The CLI holds
//! no trust logic: `skill trust <name>@<hash>` sends `TrustSkill`, the Core
//! binds the decision to the content hash and decides what reaches a model.
//!
//! Before trust: the skill is listed with the exact command that would trust
//! it and is in no request. A wrong hash is refused. After
//! `skill trust name@hash`: its index line is in the next request and
//! `skill.load` returns the body. One edited byte: untrusted again and out of
//! the next request. `skill untrust` withdraws the decision.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};

use serde_json::{Value, json};

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

struct Cli {
    data_dir: PathBuf,
    core: PathBuf,
    base: String,
}

impl Cli {
    fn run(&self, args: &[&str]) -> (i32, String, String) {
        let out = Command::new(env!("CARGO_BIN_EXE_modbit-cli"))
            .env("MODBIT_CORE_BIN", &self.core)
            .env("MODBIT_OPENAI_BASE_URL", &self.base)
            .env("OPENAI_API_KEY", "")
            .env("ANTHROPIC_API_KEY", "")
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
}

/// What the scripted model answers (by the number of tool results already in
/// the conversation) and every request body it was sent.
#[derive(Clone, Default)]
struct Model {
    script: Arc<Mutex<Vec<Value>>>,
    seen: Arc<Mutex<Vec<Value>>>,
}

impl Model {
    fn serve(&self) -> String {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let this = self.clone();
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(s) = stream else { break };
                let this = this.clone();
                std::thread::spawn(move || this.answer(s));
            }
        });
        format!("http://127.0.0.1:{port}")
    }

    fn answer(&self, mut s: std::net::TcpStream) {
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
        let body: Value =
            serde_json::from_slice(&buf[head_end..head_end + len]).unwrap_or_default();
        let results = body["messages"]
            .as_array()
            .map(|m| m.iter().filter(|x| x["role"] == "tool").count())
            .unwrap_or(0);
        self.seen.lock().unwrap().push(body);
        let reply = self
            .script
            .lock()
            .unwrap()
            .get(results)
            .cloned()
            .unwrap_or_else(|| json!({"text": "nothing further"}));
        let mut frames: Vec<String> = Vec::new();
        if let Some(t) = reply["text"].as_str() {
            frames.push(json!({"id":"c","model":"scripted","choices":[{"index":0,"delta":{"content":t},"finish_reason":null}]}).to_string());
        }
        let calls = reply["calls"].as_array().cloned().unwrap_or_default();
        for (i, c) in calls.iter().enumerate() {
            frames.push(json!({"id":"c","model":"scripted","choices":[{"index":0,"delta":{"tool_calls":[{"index":i,"id":format!("call_{results}_{i}"),"type":"function","function":{"name":c["name"],"arguments":c["args"].to_string()}}]},"finish_reason":null}]}).to_string());
        }
        let finish = if calls.is_empty() {
            "stop"
        } else {
            "tool_calls"
        };
        frames.push(json!({"id":"c","model":"scripted","choices":[{"index":0,"delta":{},"finish_reason":finish}],"usage":{"prompt_tokens":10,"completion_tokens":5}}).to_string());
        let mut payload = String::new();
        for f in frames {
            payload.push_str(&format!("data: {f}\n\n"));
        }
        payload.push_str("data: [DONE]\n\n");
        let _ = s.write_all(
            format!(
                "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncontent-length: {}\r\n\r\n{payload}",
                payload.len()
            )
            .as_bytes(),
        );
        let _ = s.flush();
    }

    fn set_script(&self, script: Vec<Value>) {
        *self.script.lock().unwrap() = script;
    }

    fn mark(&self) -> usize {
        self.seen.lock().unwrap().len()
    }

    fn since(&self, mark: usize) -> Vec<Value> {
        self.seen.lock().unwrap()[mark..].to_vec()
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

fn write_skill(profile: &Path, body_marker: &str) {
    let d = profile.join("skills").join("lintfix-cli105");
    std::fs::create_dir_all(&d).unwrap();
    std::fs::write(
        d.join("SKILL.md"),
        format!(
            "---\nname: lintfix-cli105\nversion: 1.0.0\ndescription: fix lint warnings in a changed file\n---\n{body_marker}: run the linter, fix what it reports, rerun.\n"
        ),
    )
    .unwrap();
}

fn completes() -> Vec<Value> {
    vec![
        json!({"calls": [{"name": "plan.update", "args": {"outcome": "read", "expected_files": []}}]}),
        json!({"calls": [{"name": "task.complete", "args": {"summary": "done", "self_review": {"findings": []}}}]}),
    ]
}

fn loads_the_skill() -> Vec<Value> {
    vec![
        json!({"calls": [{"name": "plan.update", "args": {"outcome": "read", "expected_files": []}}]}),
        json!({"calls": [{"name": "skill.load", "args": {"name": "lintfix-cli105"}}]}),
        json!({"calls": [{"name": "task.complete", "args": {"summary": "done", "self_review": {"findings": []}}}]}),
    ]
}

/// The inventory line of the skill: `(trust, enabled, content hash, the rest)`.
fn inventory(cli: &Cli) -> (String, String, String, String) {
    let out = cli.ok(&["skill", "inventory"]);
    let line = out
        .lines()
        .find(|l| l.starts_with("skill lintfix-cli105 "))
        .unwrap_or_else(|| panic!("not in the inventory: {out}"))
        .to_owned();
    let field = |name: &str| {
        line.split_whitespace()
            .find_map(|w| w.strip_prefix(&format!("{name}=")))
            .unwrap_or_default()
            .to_owned()
    };
    (
        field("trust"),
        field("enabled"),
        field("content_hash"),
        line,
    )
}

struct Fx {
    cli: Cli,
    model: Model,
    session: String,
    repo: String,
    _tmp: tempfile::TempDir,
}

impl Fx {
    fn new() -> Self {
        let core = core_bin();
        let tmp = tempfile::tempdir().unwrap();
        let data_dir = tmp.path().join("profile");
        std::fs::create_dir_all(&data_dir).unwrap();
        let repo = tmp.path().join("repo");
        std::fs::create_dir_all(repo.join(".modbit")).unwrap();
        std::fs::write(repo.join("a.txt"), "a\n").unwrap();
        // A completion needs a mandatory check; this test's subject is skills.
        std::fs::write(
            repo.join(".modbit/verification.json"),
            r#"{"commands": [{"id": "fixture-noop", "argv": ["git", "--version"]}]}"#,
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
        let model = Model::default();
        model.set_script(completes());
        let base = model.serve();
        let cli = Cli {
            data_dir,
            core,
            base,
        };
        let out = cli.ok(&["session", "create"]);
        let session = out.trim().strip_prefix("session ").unwrap().to_owned();
        Fx {
            cli,
            model,
            session,
            repo: repo_str,
            _tmp: tmp,
        }
    }

    /// A new task, run to its end under `script`; the requests it sent.
    fn run_task(&self, script: Vec<Value>) -> Vec<Value> {
        self.model.set_script(script);
        let mark = self.model.mark();
        let out = self.cli.ok(&[
            "task",
            "create",
            "--session",
            &self.session,
            "--workspace",
            &self.repo,
            "read the notes",
        ]);
        let task = out.trim().strip_prefix("task ").unwrap().to_owned();
        let (code, out, err) = self.cli.run(&[
            "task",
            "run",
            "--session",
            &self.session,
            "--task",
            &task,
            "--endpoint",
            "openai",
            "--model",
            "gpt-5",
            "--wait",
        ]);
        assert_eq!(code, 0, "{out}{err}");
        self.model.since(mark)
    }
}

fn index_line_in(reqs: &[Value]) -> bool {
    reqs.iter()
        .any(|b| b.to_string().contains("lintfix-cli105: fix lint warnings"))
}

fn tool_names(body: &Value) -> Vec<String> {
    body["tools"]
        .as_array()
        .map(|t| {
            t.iter()
                .filter_map(|x| x["function"]["name"].as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default()
}

#[test]
fn skill_trust_by_hash_from_the_cli_gates_what_reaches_the_model_and_one_byte_revokes_it() {
    const MARKER: &str = "LINTFIX-CLI-BODY-MARKER";
    let fx = Fx::new();
    write_skill(&fx.cli.data_dir, MARKER);

    // 1. Listed, untrusted, with the command that would trust exactly these
    //    bytes; in no request; no loader offered.
    let (trust, enabled, hash, line) = inventory(&fx.cli);
    assert_eq!((trust.as_str(), enabled.as_str()), ("UNTRUSTED", "false"));
    assert!(
        line.contains(&format!("skill trust lintfix-cli105@{hash}")),
        "{line}"
    );
    let reqs = fx.run_task(completes());
    assert!(!reqs.is_empty());
    for b in &reqs {
        let t = b.to_string();
        assert!(!t.contains(MARKER) && !t.contains("lintfix-cli105"), "{t}");
        assert!(!tool_names(b).contains(&"skill.load".to_owned()));
    }

    // 2. Trust is bound to the hash named: a hash that is not these bytes, and
    //    a spec without one, are refused and change nothing.
    let wrong = format!("lintfix-cli105@{}", "0".repeat(64));
    let (code, out, err) = fx.cli.run(&["skill", "trust", &wrong]);
    assert_ne!(code, 0, "{out}{err}");
    assert!(
        format!("{out}{err}").contains("HASH_MISMATCH"),
        "{out}{err}"
    );
    let (code, out, err) = fx.cli.run(&["skill", "trust", "lintfix-cli105"]);
    assert_ne!(code, 0, "{out}{err}");
    assert_eq!(inventory(&fx.cli).0, "UNTRUSTED");

    // 3. The owner's command: the index line reaches the next request (no
    //    body), and `skill.load` returns the body, labelled with its source.
    let out = fx
        .cli
        .ok(&["skill", "trust", &format!("lintfix-cli105@{hash}")]);
    assert!(out.contains("trust=TRUSTED_BY_OWNER"), "{out}");
    assert_eq!(inventory(&fx.cli).0, "TRUSTED_BY_OWNER");
    let reqs = fx.run_task(loads_the_skill());
    assert!(index_line_in(&reqs), "the index line is in the request");
    assert!(
        !reqs[0].to_string().contains(MARKER),
        "the index carries no body"
    );
    assert!(tool_names(&reqs[0]).contains(&"skill.load".to_owned()));
    let last = reqs.last().unwrap();
    let tool_results: Vec<String> = last["messages"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|m| m["role"] == "tool")
        .map(|m| m["content"].to_string())
        .collect();
    let loaded = tool_results
        .iter()
        .find(|t| t.contains(MARKER))
        .unwrap_or_else(|| panic!("skill.load returned no body: {tool_results:#?}"));
    assert!(
        loaded.contains("grants no tool, no capability and no approval"),
        "the body is labelled data: {loaded}"
    );

    // 4. One edited byte: untrusted again, out of the next request, and
    //    `skill.load` refuses it.
    write_skill(&fx.cli.data_dir, "LINTFIX-CLI-BODY-MARKEr");
    let (trust, enabled, new_hash, _) = inventory(&fx.cli);
    assert_ne!(new_hash, hash);
    assert_eq!(
        (trust.as_str(), enabled.as_str()),
        ("CHANGED_SINCE_TRUSTED", "false")
    );
    let reqs = fx.run_task(completes());
    assert!(!index_line_in(&reqs), "an edited skill left the index");
    for b in &reqs {
        assert!(!b.to_string().contains("LINTFIX-CLI-BODY-MARKE"));
    }

    // 5. Trusting the new bytes works; untrust withdraws it again.
    fx.cli
        .ok(&["skill", "trust", &format!("lintfix-cli105@{new_hash}")]);
    assert_eq!(inventory(&fx.cli).0, "TRUSTED_BY_OWNER");
    assert!(index_line_in(&fx.run_task(completes())));
    let out = fx.cli.ok(&["skill", "untrust", "lintfix-cli105"]);
    assert!(out.contains("lintfix-cli105"), "{out}");
    assert_eq!(inventory(&fx.cli).0, "UNTRUSTED");
    assert!(!index_line_in(&fx.run_task(completes())));
}
