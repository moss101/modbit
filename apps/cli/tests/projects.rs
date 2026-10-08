//! PX-063: the `project` verbs of the real `modbit-cli` against the real
//! `modbit-core` binary. The CLI holds no project logic: what it prints is
//! what the Core answered, and a refusal is the Core's typed code.

use std::path::{Path, PathBuf};
use std::process::Command;

fn core_bin() -> PathBuf {
    let cli = PathBuf::from(env!("CARGO_BIN_EXE_modbit-cli"));
    let core = cli.parent().unwrap().join(if cfg!(windows) {
        "modbit-core.exe"
    } else {
        "modbit-core"
    });
    assert!(core.exists(), "{} (build modbit-core first)", core.display());
    core
}

/// Runs the CLI; returns (success, stdout, stdout+stderr).
fn cli(data_dir: &Path, core: &Path, args: &[&str]) -> (bool, String, String) {
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

fn json(out: &str) -> serde_json::Value {
    let line = out.lines().find(|l| l.starts_with('{')).unwrap_or("null");
    serde_json::from_str(line).unwrap_or_else(|e| panic!("{e}: {out}"))
}

#[test]
fn qual_px_063_the_project_verbs_drive_the_core_and_show_its_typed_refusals() {
    let core = core_bin();
    let tmp = tempfile::tempdir().unwrap();
    let data_dir = tmp.path().join("profile");
    std::fs::create_dir_all(&data_dir).unwrap();
    let repo = tmp.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    let ws = repo
        .canonicalize()
        .unwrap()
        .to_string_lossy()
        .trim_start_matches(r"\\?\")
        .to_owned();

    let (ok, out, all) = cli(&data_dir, &core, &["session", "create"]);
    assert!(ok, "{all}");
    let sid = out.trim().strip_prefix("session ").unwrap().to_owned();
    let (ok, out, all) = cli(
        &data_dir,
        &core,
        &["task", "create", "--session", &sid, "--workspace", &ws, "a goal"],
    );
    assert!(ok, "{all}");
    let tid = out.trim().strip_prefix("task ").unwrap().to_owned();

    // Create, then list and show it as JSON.
    let (ok, out, all) = cli(
        &data_dir,
        &core,
        &["project", "create", "--session", &sid, "--workspace", &ws, "--color", "warn", "--icon", "rocket", "--json", "Release", "2"],
    );
    assert!(ok, "{all}");
    let created = json(&out);
    assert_eq!(created["name"], "Release 2");
    assert_eq!(created["color"], "warn");
    assert_eq!(created["icon"], "rocket");
    let pid = created["project_id"].as_str().unwrap().to_owned();
    let (ok, out, all) = cli(&data_dir, &core, &["project", "list", "--json"]);
    assert!(ok, "{all}");
    assert_eq!(json(&out)["projects"][0]["project_id"], pid.as_str());

    // A draft task is refused with the Core's typed reason; nothing changes.
    let (ok, _, all) = cli(
        &data_dir,
        &core,
        &["project", "add", "--session", &sid, "--project", &pid, "--task", &tid],
    );
    assert!(!ok, "{all}");
    assert!(all.contains("TASK_IS_DRAFT"), "{all}");
    let (_, out, _) = cli(&data_dir, &core, &["project", "show", "--json", &pid]);
    assert_eq!(json(&out)["members"].as_array().unwrap().len(), 0);

    // Stopped, the task is no draft and joins; the rollup counts it.
    let (ok, _, all) = cli(&data_dir, &core, &["task", "cancel", "--session", &sid, "--task", &tid]);
    assert!(ok, "{all}");
    let (ok, out, all) = cli(
        &data_dir,
        &core,
        &["project", "add", "--session", &sid, "--project", &pid, "--task", &tid, "--json"],
    );
    assert!(ok, "{all}");
    let v = json(&out);
    assert_eq!(v["rollup"]["members"], 1);
    assert_eq!(v["members"][0]["task_id"], tid.as_str());

    // Archive leaves the task alone and hides the project from the default list.
    let (ok, _, all) = cli(&data_dir, &core, &["project", "archive", "--session", &sid, "--project", &pid]);
    assert!(ok, "{all}");
    let (_, out, _) = cli(&data_dir, &core, &["project", "list", "--json"]);
    assert_eq!(json(&out)["projects"].as_array().unwrap().len(), 0);
    let (_, out, _) = cli(&data_dir, &core, &["project", "list", "--archived", "--json"]);
    let listed = json(&out);
    assert_eq!(listed["projects"][0]["archived"], true);
    assert_eq!(listed["projects"][0]["members"].as_array().unwrap().len(), 1);
    let (ok, _, all) = cli(&data_dir, &core, &["project", "rename", "--session", &sid, "--project", &pid, "Nope"]);
    assert!(!ok && all.contains("PROJECT_ARCHIVED"), "{all}");
    let (ok, _, all) = cli(&data_dir, &core, &["project", "archive", "--session", &sid, "--project", &pid, "--undo"]);
    assert!(ok, "{all}");
    let (ok, out, all) = cli(
        &data_dir,
        &core,
        &["project", "remove", "--session", &sid, "--project", &pid, "--task", &tid, "--json"],
    );
    assert!(ok, "{all}");
    assert_eq!(json(&out)["rollup"]["members"], 0);
}
