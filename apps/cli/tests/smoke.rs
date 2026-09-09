//! End-to-end smoke: the real `modbit-cli` binary spawns the real `modbit-core`
//! binary, creates a session and task, shows the snapshot and tails events.
//! Uses a deliberately long data directory to exercise the socket-path rule.

use std::path::PathBuf;
use std::process::Command;

fn core_bin() -> PathBuf {
    let cli = PathBuf::from(env!("CARGO_BIN_EXE_modbit-cli"));
    let dir = cli.parent().unwrap();
    let name = if cfg!(windows) {
        "modbit-core.exe"
    } else {
        "modbit-core"
    };
    let core = dir.join(name);
    if !core.exists() {
        let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
        let status = Command::new(cargo)
            .args(["build", "-p", "modbit-core", "--locked"])
            .status()
            .unwrap();
        assert!(status.success(), "building modbit-core for the smoke test");
    }
    assert!(core.exists(), "{}", core.display());
    core
}

/// Runs the CLI; returns (success, stdout, stdout+stderr). Assertions on
/// values use stdout only: the Core's recovery log line arrives on stderr.
fn cli(
    data_dir: &std::path::Path,
    core: &std::path::Path,
    args: &[&str],
) -> (bool, String, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_modbit-cli"))
        .env("MODBIT_CORE_BIN", core)
        .arg("--data-dir")
        .arg(data_dir)
        .args(args)
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    let all = format!("{stdout}{}", String::from_utf8_lossy(&out.stderr));
    (out.status.success(), stdout, all)
}

#[test]
fn cli_drives_a_real_core_end_to_end() {
    let core = core_bin();
    let tmp = tempfile::tempdir().unwrap();
    let data_dir = tmp
        .path()
        .join("a-deliberately-long-data-directory-name-to-exceed-socket-limits")
        .join("modbit-profile-directory");
    std::fs::create_dir_all(&data_dir).unwrap();

    let (ok, out, all) = cli(&data_dir, &core, &["session", "create"]);
    assert!(ok, "{all}");
    let sid = out.trim().strip_prefix("session ").unwrap().to_owned();
    assert_eq!(sid.len(), 32, "{all}");

    let (ok, out, all) = cli(
        &data_dir,
        &core,
        &[
            "task",
            "create",
            "--session",
            &sid,
            "fix",
            "the",
            "flaky",
            "build",
        ],
    );
    assert!(ok, "{all}");
    assert!(out.starts_with("task "), "{all}");

    let (ok, out, all) = cli(&data_dir, &core, &["session", "show", "--session", &sid]);
    assert!(ok, "{all}");
    assert!(out.contains("state=Active"), "{all}");
    assert!(
        out.contains("state=Queued") && out.contains("goal=\"fix the flaky build\""),
        "{all}"
    );

    let (ok, out, all) = cli(
        &data_dir,
        &core,
        &["events", "tail", "--session", &sid, "--after", "0"],
    );
    assert!(ok, "{all}");
    let lines: Vec<_> = out.lines().collect();
    assert_eq!(lines.len(), 5, "{all}");
    assert!(
        lines[0].ends_with("SessionCreated")
            && lines[1].ends_with("SessionLeaseAcquired")
            && lines[2].ends_with("TaskCreated")
            && lines[3].ends_with("TaskQueued")
            && lines[4].ends_with("CapabilityLeaseGranted"),
        "{all}"
    );

    let (ok, out, all) = cli(
        &data_dir,
        &core,
        &["events", "tail", "--session", &sid, "--after", "3"],
    );
    assert!(
        ok && out.lines().count() == 2 && out.contains("TaskQueued"),
        "{all}"
    );

    // M2.4: tools through the registry, policy and event loop, from the CLI.
    let (ok, out, all) = cli(&data_dir, &core, &["tool", "list"]);
    assert!(
        ok && out.contains("tool fs.read ") && out.contains("tool shell.exec "),
        "{all}"
    );
    let repo = tmp.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    std::fs::write(repo.join("hello.txt"), "hi\n").unwrap();
    for args in [
        vec!["init", "-q", "-b", "main"],
        vec!["add", "-A"],
        vec![
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@e",
            "commit",
            "-q",
            "-m",
            "base",
        ],
    ] {
        assert!(
            Command::new("git")
                .arg("-C")
                .arg(&repo)
                .args(&args)
                .status()
                .unwrap()
                .success()
        );
    }
    let repo_str = repo
        .canonicalize()
        .unwrap()
        .to_string_lossy()
        .trim_start_matches(r"\\?\")
        .to_owned();
    let (ok, out, all) = cli(
        &data_dir,
        &core,
        &[
            "task",
            "create",
            "--session",
            &sid,
            "--workspace",
            &repo_str,
            "read",
            "a",
            "file",
        ],
    );
    assert!(ok, "{all}");
    let tid = out.trim().strip_prefix("task ").unwrap().to_owned();
    let (ok, out, all) = cli(
        &data_dir,
        &core,
        &[
            "tool",
            "invoke",
            "--session",
            &sid,
            "--task",
            &tid,
            "fs.read",
            r#"{"path":"hello.txt"}"#,
        ],
    );
    assert!(
        ok && out.contains("status=SUCCESS") && out.contains(r#""content":"hi\n""#),
        "{all}"
    );
    let (ok, out, all) = cli(
        &data_dir,
        &core,
        &[
            "tool",
            "invoke",
            "--session",
            &sid,
            "--task",
            &tid,
            "fs.read",
            r#"{"path":"../etc/passwd"}"#,
        ],
    );
    assert!(
        ok && out.contains("status=APPLICATION_FAILURE") && out.contains("PATH_OUTSIDE_ROOT"),
        "{all}"
    );
    let (ok, out, all) = cli(
        &data_dir,
        &core,
        &[
            "events",
            "tail",
            "--session",
            &sid,
            "--after",
            "4",
            "--count",
            "50",
        ],
    );
    assert!(
        ok && out.contains("ToolCallSucceeded") && out.contains("ToolCallFailed"),
        "{all}"
    );

    // REQ-EV-0064/0065 (M2 backlog): a typed undo plan for one call, applied through the CLI.
    let create_call = "fedcba9876543210fedcba9876543210";
    let (ok, out, all) = cli(
        &data_dir,
        &core,
        &[
            "tool",
            "invoke",
            "--session",
            &sid,
            "--task",
            &tid,
            "--call",
            create_call,
            "change.apply",
            r#"{"path":"made.txt","op":"create","content":"made\n"}"#,
        ],
    );
    assert!(ok && out.contains("status=SUCCESS"), "{all}");
    assert!(repo.join("made.txt").exists());
    let (ok, out, all) = cli(
        &data_dir,
        &core,
        &[
            "change",
            "undo",
            "--session",
            &sid,
            "--task",
            &tid,
            "--call",
            create_call,
        ],
    );
    assert!(
        ok && out.contains("undo applied=false steps=1") && out.contains("step delete made.txt"),
        "{all}"
    );
    assert!(
        repo.join("made.txt").exists(),
        "a plan alone changes nothing"
    );
    let (ok, out, all) = cli(
        &data_dir,
        &core,
        &[
            "change",
            "undo",
            "--session",
            &sid,
            "--task",
            &tid,
            "--call",
            create_call,
            "--apply",
        ],
    );
    assert!(ok && out.contains("undo applied=true steps=1"), "{all}");
    assert!(!repo.join("made.txt").exists());

    // M2.5: the task lease, an approval-gated destructive tool, the receipt chain.
    let (ok, out, all) = cli(&data_dir, &core, &["lease", "list", "--task", &tid]);
    assert!(
        ok && out.contains("status=ACTIVE") && out.contains("profile=local_trusted"),
        "{all}"
    );
    let wt = repo_str.clone() + "-wt";
    let wt_json = wt.replace('\\', "/");
    let (ok, out, all) = cli(
        &data_dir,
        &core,
        &[
            "tool",
            "invoke",
            "--session",
            &sid,
            "--task",
            &tid,
            "git.worktree.create",
            &format!(r#"{{"branch":"task/cli","path":"{wt_json}"}}"#),
        ],
    );
    assert!(ok && out.contains("status=SUCCESS"), "{all}");
    let call = "0123456789abcdef0123456789abcdef";
    let close = format!(r#"{{"path":"{wt_json}"}}"#);
    let (ok, out, all) = cli(
        &data_dir,
        &core,
        &[
            "tool",
            "invoke",
            "--session",
            &sid,
            "--task",
            &tid,
            "--call",
            call,
            "git.worktree.close",
            &close,
        ],
    );
    assert!(ok && out.contains("status=APPROVAL_PENDING"), "{all}");
    assert!(
        std::path::Path::new(&wt).exists(),
        "no effect before approval"
    );
    let (ok, out, all) = cli(&data_dir, &core, &["approval", "list", "--session", &sid]);
    assert!(ok && out.contains("status=REQUESTED"), "{all}");
    let approval = out
        .lines()
        .find_map(|l| l.strip_prefix("approval "))
        .and_then(|l| l.split(' ').next())
        .unwrap()
        .to_owned();
    let (ok, out, all) = cli(
        &data_dir,
        &core,
        &[
            "approval",
            "resolve",
            "--session",
            &sid,
            "--approval",
            &approval,
            "approve",
            "looks",
            "fine",
        ],
    );
    assert!(ok && out.contains("status=APPROVED"), "{all}");
    let (ok, out, all) = cli(
        &data_dir,
        &core,
        &[
            "tool",
            "invoke",
            "--session",
            &sid,
            "--task",
            &tid,
            "--call",
            call,
            "git.worktree.close",
            &close,
        ],
    );
    assert!(
        ok && out.contains("status=SUCCESS") && out.contains(&format!("approval={approval}")),
        "{all}"
    );
    assert!(!std::path::Path::new(&wt).exists());
    let (ok, out, all) = cli(&data_dir, &core, &["receipts", "--task", &tid]);
    assert!(
        ok && out.contains("receipt_chain valid=true count=1") && out.contains("status=SUCCESS"),
        "{all}"
    );
    let (ok, out, all) = cli(&data_dir, &core, &["stop", "--session", &sid, "halt"]);
    assert!(ok && out.contains("leases_revoked=2"), "{all}");
    let (ok, out, all) = cli(&data_dir, &core, &["lease", "list", "--task", &tid]);
    assert!(ok && out.contains("status=REVOKED"), "{all}");

    // M2.7: task status is served for a task that has not started (exit 4:
    // still Queued, per apps/cli/README.md).
    let (ok, out, all) = cli(&data_dir, &core, &["task", "status", "--task", &tid]);
    assert!(
        !ok && out.contains("state=Queued") && out.contains("loop_alive=false"),
        "{all}"
    );

    // M2.6: the gateway catalog is served by the Core from its own environment.
    let out = Command::new(env!("CARGO_BIN_EXE_modbit-cli"))
        .env("MODBIT_CORE_BIN", &core)
        .env("MODBIT_OPENAI_BASE_URL", "http://127.0.0.1:9")
        .env("OPENAI_API_KEY", "")
        .arg("--data-dir")
        .arg(&data_dir)
        .args(["model", "list"])
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&out.stdout).to_string();
    assert!(
        out.status.success()
            && text.contains("model openai/gpt-5-mini ")
            && text.contains("health openai requests=0"),
        "{text}{}",
        String::from_utf8_lossy(&out.stderr)
    );

    let (ok, _, all) = cli(&data_dir, &core, &["session", "show", "--session", "00"]);
    assert!(!ok && all.contains("not a 32-hex-char id"), "{all}");
    // No socket files leak into the data directory.
    assert!(
        !std::fs::read_dir(&data_dir).unwrap().any(|e| e
            .unwrap()
            .path()
            .extension()
            .is_some_and(|x| x == "sock"))
    );
}
