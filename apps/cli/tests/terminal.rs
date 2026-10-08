//! PX-043: the CLI reads the background-terminal registry and a terminal's
//! stream through the real Core and the real broker. The terminal outlives
//! each CLI invocation (the broker is durable), so `terminal list` and
//! `terminal attach` in later invocations see what an earlier one started.
//!
//! Unix only: the process is `sh` on a PTY.

#[cfg(not(unix))]
#[test]
fn terminal_cli_tests_are_unix_only() {
    eprintln!(
        "SKIPPED terminal CLI: the fixture process is `sh` on a PTY; the broker's own suite covers ConPTY"
    );
}

#[cfg(unix)]
mod unix {
    use std::io::{BufRead, BufReader};
    use std::path::{Path, PathBuf};
    use std::process::{Command, Stdio};
    use std::time::{Duration, Instant};

    fn core_bin() -> PathBuf {
        let cli = PathBuf::from(env!("CARGO_BIN_EXE_modbit-cli"));
        let core = cli.parent().unwrap().join("modbit-core");
        assert!(
            core.exists(),
            "{} (build modbit-core and modbit-execd first)",
            core.display()
        );
        core
    }

    fn cli_command(data_dir: &Path, args: &[&str]) -> Command {
        let mut c = Command::new(env!("CARGO_BIN_EXE_modbit-cli"));
        c.env("MODBIT_CORE_BIN", core_bin())
            .env("MODBIT_EXECD_ORPHAN_GRACE_SECS", "4")
            .arg("--data-dir")
            .arg(data_dir)
            .args(args);
        c
    }

    fn cli(data_dir: &Path, args: &[&str]) -> (bool, String, String) {
        let out = cli_command(data_dir, args).output().unwrap();
        (
            out.status.success(),
            String::from_utf8_lossy(&out.stdout).into_owned(),
            String::from_utf8_lossy(&out.stderr).into_owned(),
        )
    }

    /// A workspace the task may run commands in.
    fn workspace() -> (tempfile::TempDir, String) {
        let ws = tempfile::tempdir().unwrap();
        std::fs::write(ws.path().join("a.txt"), "a\n").unwrap();
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
                    .arg(ws.path())
                    .args(&args)
                    .status()
                    .unwrap()
                    .success()
            );
        }
        let root = ws
            .path()
            .canonicalize()
            .unwrap()
            .to_string_lossy()
            .into_owned();
        (ws, root)
    }

    #[test]
    fn px_043_the_cli_lists_and_attaches_to_a_terminal_that_outlives_each_invocation() {
        let (_ws, root) = workspace();
        let data = tempfile::tempdir().unwrap();
        let (ok, out, err) = cli(data.path(), &["session", "create"]);
        assert!(ok, "{err}");
        let sid = out.trim().strip_prefix("session ").unwrap().to_owned();
        let (ok, out, err) = cli(
            data.path(),
            &[
                "task",
                "create",
                "--session",
                &sid,
                "--workspace",
                &root,
                "watch",
                "a",
                "shell",
            ],
        );
        assert!(ok, "{err}");
        let task = out.split_whitespace().nth(1).unwrap().to_owned();
        let (ok, out, err) = cli(
            data.path(),
            &[
                "tool",
                "invoke",
                "--session",
                &sid,
                "--task",
                &task,
                "shell.start",
                r#"{"argv":["sh","-c","echo cli-visible-line; sleep 25"],"inherit_env":true,"pty":true}"#,
            ],
        );
        assert!(ok, "{out}{err}");

        // A later invocation sees it in the registry.
        let (ok, out, err) = cli(data.path(), &["terminal", "list", "--task", &task]);
        assert!(ok, "{err}");
        let line = out
            .lines()
            .find(|l| l.starts_with("terminal "))
            .unwrap_or_else(|| panic!("no terminal listed: {out}{err}"));
        assert!(
            line.contains("owner=agent") && line.contains("state=RUNNING"),
            "{line}"
        );
        assert!(line.contains(&format!("task={task}")), "{line}");
        let terminal = line.split_whitespace().nth(1).unwrap().to_owned();

        // Another reads its stream from a cursor, until it has seen the line.
        let mut child = cli_command(
            data.path(),
            &[
                "terminal",
                "attach",
                "--task",
                &task,
                "--terminal",
                &terminal,
            ],
        )
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
        let mut stdout = BufReader::new(child.stdout.take().unwrap());
        let deadline = Instant::now() + Duration::from_secs(60);
        let mut seen = String::new();
        let mut line = String::new();
        while !seen.contains("cli-visible-line") {
            assert!(
                Instant::now() < deadline,
                "never saw the terminal's output: {seen:?}"
            );
            line.clear();
            if stdout.read_line(&mut line).unwrap_or(0) == 0 {
                break;
            }
            seen.push_str(&line);
        }
        let _ = child.kill();
        let _ = child.wait();
        assert!(seen.contains("cli-visible-line"), "{seen:?}");

        // A cursor past the output is the typed error, not a hang.
        let (ok, _, err) = cli(
            data.path(),
            &[
                "terminal",
                "attach",
                "--task",
                &task,
                "--terminal",
                &terminal,
                "--from",
                "99999999",
            ],
        );
        assert!(!ok);
        assert!(err.contains("CURSOR_BEYOND_HEAD"), "{err}");
    }
}
