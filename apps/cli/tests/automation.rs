//! PX-086: the `automation` verbs of the headless CLI over a real Core —
//! validate, create, show, the exact-hash enable approval, list, a run, the
//! history, and the kill switches. The CLI holds no definition, scheduler or
//! policy; what these assertions see is what the Core answered. No model is
//! configured, so a run ends with the Core's typed refusal to start it, which
//! is itself the unattended behaviour under test: nothing hangs, nothing is
//! auto-approved, and the outcome is recorded.

use std::path::{Path, PathBuf};
use std::process::Command;

struct Cli {
    data_dir: PathBuf,
    core: PathBuf,
}

impl Cli {
    fn run(&self, args: &[&str]) -> (i32, String, String) {
        let out = Command::new(env!("CARGO_BIN_EXE_modbit-cli"))
            .env("MODBIT_CORE_BIN", &self.core)
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
    let cli = Cli { data_dir, core };
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
