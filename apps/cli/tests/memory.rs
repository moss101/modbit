//! PX-113: the `memory` verbs of the headless CLI over a real Core — list,
//! show (with the item's history from the log), propose, promote, edit and
//! forget — and that a retried mutation does nothing twice. The CLI holds no
//! memory logic; what these assertions see is what the Core answered.

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
fn memory_verbs_list_show_propose_promote_edit_and_forget_through_the_core() {
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
    let out = cli.ok(&[
        "task",
        "create",
        "--session",
        &sid,
        "--workspace",
        &root,
        "memory",
    ]);
    let tid = out.trim().strip_prefix("task ").unwrap().to_owned();

    // Nothing yet.
    assert!(
        cli.ok(&["memory", "list", "--task", &tid])
            .contains("no memory items")
    );

    // Propose as a person: a candidate, with the id printed.
    let out = cli.ok(&[
        "memory",
        "propose",
        "--session",
        &sid,
        "--task",
        &tid,
        "--scope",
        "user",
        "--type",
        "convention",
        "--topic",
        "indentation",
        "use",
        "tabs",
        "for",
        "indentation",
    ]);
    let id = out
        .trim()
        .strip_prefix("proposed ")
        .and_then(|r| r.split_whitespace().next())
        .unwrap_or_else(|| panic!("{out}"))
        .to_owned();
    assert_eq!(id.len(), 64, "{out}");
    // A candidate is listed, not curated; `--status` filters.
    let out = cli.ok(&["memory", "list", "--task", &tid]);
    assert!(
        out.contains("proposed") && out.contains("topic=\"indentation\""),
        "{out}"
    );
    assert!(
        cli.ok(&["memory", "list", "--task", &tid, "--status", "curated"])
            .contains("no memory items")
    );

    // Promote by id prefix.
    let out = cli.ok(&[
        "memory",
        "promote",
        "--session",
        &sid,
        "--task",
        &tid,
        &id[..12],
    ]);
    assert!(out.starts_with(&format!("curated {id}")), "{out}");
    // Promoting it again is refused with a typed reason and a nonzero exit.
    let (code, out, _) = cli.run(&["memory", "promote", "--session", &sid, "--task", &tid, &id]);
    assert_eq!(code, 1, "{out}");
    assert!(
        out.contains("refused") && out.contains("NOT_PROPOSED"),
        "{out}"
    );

    // A transcript summary is refused whatever the person says.
    let out = cli.ok(&[
        "memory",
        "propose",
        "--session",
        &sid,
        "--task",
        &tid,
        "--scope",
        "session",
        "--type",
        "fact",
        "--topic",
        "flaky test",
        "--source",
        "transcript_summary",
        "login",
        "flakes",
    ]);
    let summary = out.split_whitespace().nth(1).unwrap().to_owned();
    let (code, out, _) = cli.run(&[
        "memory",
        "promote",
        "--session",
        &sid,
        "--task",
        &tid,
        &summary,
    ]);
    assert_eq!(code, 1);
    assert!(out.contains("TRANSCRIPT_NOT_VALIDATED"), "{out}");

    // Show: the item in full and its history from the log.
    let out = cli.ok(&["memory", "show", "--task", &tid, &id[..10]]);
    assert!(
        out.contains(&format!("id: {id}")) && out.contains("content: use tabs for indentation"),
        "{out}"
    );
    assert!(
        out.contains("MemoryProposed") && out.contains("MemoryPromoted"),
        "{out}"
    );
    assert!(out.matches("by user:").count() >= 2, "{out}");

    // Edit: a new item that supersedes the old.
    let out = cli.ok(&[
        "memory",
        "edit",
        "--session",
        &sid,
        "--task",
        &tid,
        &id,
        "--content",
        "use two spaces",
    ]);
    assert!(
        out.starts_with("edited ")
            && out.contains("status=curated")
            && out.contains(&format!("retired={id}")),
        "{out}"
    );
    let new_id = out
        .split("-> ")
        .nth(1)
        .unwrap()
        .split_whitespace()
        .next()
        .unwrap()
        .to_owned();
    let out = cli.ok(&["memory", "list", "--task", &tid, "--json"]);
    let v: serde_json::Value = serde_json::from_str(out.trim()).unwrap();
    let status_of = |i: &str| {
        v["items"]
            .as_array()
            .unwrap()
            .iter()
            .find(|x| x["id"] == i)
            .map(|x| x["status"].as_str().unwrap().to_owned())
    };
    assert_eq!(status_of(&id).as_deref(), Some("superseded"));
    assert_eq!(status_of(&new_id).as_deref(), Some("curated"));
    // A superseded item is not editable.
    let (code, out, _) = cli.run(&[
        "memory",
        "edit",
        "--session",
        &sid,
        "--task",
        &tid,
        &id,
        "--content",
        "x",
    ]);
    assert_eq!(code, 1);
    assert!(out.contains("NOT_EDITABLE"), "{out}");

    // Forget: delete leaves the store; supersede keeps it marked.
    let out = cli.ok(&[
        "memory",
        "forget",
        "--session",
        &sid,
        "--task",
        &tid,
        &new_id,
    ]);
    assert!(out.contains("status=deleted"), "{out}");
    let out = cli.ok(&["memory", "list", "--task", &tid, "--json"]);
    let v: serde_json::Value = serde_json::from_str(out.trim()).unwrap();
    assert!(
        v["items"]
            .as_array()
            .unwrap()
            .iter()
            .all(|x| x["id"] != new_id.as_str())
    );
    let out = cli.ok(&[
        "memory",
        "forget",
        "--session",
        &sid,
        "--task",
        &tid,
        "--supersede",
        &summary,
    ]);
    assert!(out.contains("status=superseded"), "{out}");
    let (code, out, _) = cli.run(&[
        "memory",
        "forget",
        "--session",
        &sid,
        "--task",
        &tid,
        &new_id,
    ]);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("unchanged"), "{out}");
    // A usage error is not a crash.
    let (code, _, err) = cli.run(&["memory", "promote", "--task", &tid]);
    assert_ne!(code, 0);
    assert!(err.contains("usage") || err.contains("--session"), "{err}");
}
