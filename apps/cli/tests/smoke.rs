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

fn cli(data_dir: &std::path::Path, core: &std::path::Path, args: &[&str]) -> (bool, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_modbit-cli"))
        .env("MODBIT_CORE_BIN", core)
        .arg("--data-dir")
        .arg(data_dir)
        .args(args)
        .output()
        .unwrap();
    (
        out.status.success(),
        format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        ),
    )
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

    let (ok, out) = cli(&data_dir, &core, &["session", "create"]);
    assert!(ok, "{out}");
    let sid = out.trim().strip_prefix("session ").unwrap().to_owned();
    assert_eq!(sid.len(), 32, "{out}");

    let (ok, out) = cli(
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
    assert!(ok, "{out}");
    assert!(out.starts_with("task "), "{out}");

    let (ok, out) = cli(&data_dir, &core, &["session", "show", "--session", &sid]);
    assert!(ok, "{out}");
    assert!(out.contains("state=Active"), "{out}");
    assert!(
        out.contains("state=Queued") && out.contains("goal=\"fix the flaky build\""),
        "{out}"
    );

    let (ok, out) = cli(
        &data_dir,
        &core,
        &["events", "tail", "--session", &sid, "--after", "0"],
    );
    assert!(ok, "{out}");
    let lines: Vec<_> = out.lines().collect();
    assert_eq!(lines.len(), 3, "{out}");
    assert!(
        lines[0].ends_with("SessionCreated") && lines[2].ends_with("TaskQueued"),
        "{out}"
    );

    let (ok, out) = cli(
        &data_dir,
        &core,
        &["events", "tail", "--session", &sid, "--after", "2"],
    );
    assert!(
        ok && out.lines().count() == 1 && out.contains("TaskQueued"),
        "{out}"
    );

    let (ok, out) = cli(&data_dir, &core, &["session", "show", "--session", "00"]);
    assert!(!ok && out.contains("not a 32-hex-char id"), "{out}");
    // No socket files leak into the data directory.
    assert!(
        !std::fs::read_dir(&data_dir).unwrap().any(|e| e
            .unwrap()
            .path()
            .extension()
            .is_some_and(|x| x == "sock"))
    );
}
