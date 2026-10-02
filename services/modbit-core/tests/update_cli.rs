//! M10.2 (docs/70 "Desktop update"): the offline subcommands the desktop
//! updater runs, against the real `modbit-core` binary and real SQLite files.
//! Each is read-only: it must work beside a running Core and change nothing.

use std::path::Path;
use std::process::{Command, Output};

fn core(args: &[&str], envs: &[(&str, &str)]) -> Output {
    let mut c = Command::new(env!("CARGO_BIN_EXE_modbit-core"));
    c.args(args);
    for (k, v) in envs {
        c.env(k, v);
    }
    c.output().expect("spawn modbit-core")
}

fn json(out: &Output) -> serde_json::Value {
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).expect("one JSON line")
}

fn make_profile(dir: &Path) {
    modbit_event_store::EventStore::open(dir).expect("create a profile at this build's schema");
}

#[test]
fn schema_info_reports_the_build_and_the_disk_without_creating_a_database() {
    let dir = tempfile::tempdir().unwrap();
    let d = dir.path().to_str().unwrap();
    let fresh = json(&core(&["schema-info", "--data-dir", d], &[]));
    assert!(fresh["build_schema_version"].as_u64().unwrap() >= 1);
    assert!(fresh["on_disk_schema_version"].is_null());
    assert!(!dir.path().join("core.db").exists());

    make_profile(dir.path());
    let seasoned = json(&core(&["schema-info", "--data-dir", d], &[]));
    assert_eq!(
        seasoned["on_disk_schema_version"],
        seasoned["build_schema_version"]
    );
}

#[test]
fn backup_writes_a_database_a_later_build_can_open_and_refuses_to_overwrite() {
    let dir = tempfile::tempdir().unwrap();
    make_profile(dir.path());
    let dest = dir.path().join("pre-update.db");
    let out = json(&core(
        &[
            "backup",
            "--data-dir",
            dir.path().to_str().unwrap(),
            "--to",
            dest.to_str().unwrap(),
        ],
        &[],
    ));
    assert_eq!(
        out["schema_version"],
        json(&core(
            &["schema-info", "--data-dir", dir.path().to_str().unwrap()],
            &[]
        ))["build_schema_version"]
    );
    assert!(dest.exists());

    let restored = tempfile::tempdir().unwrap();
    std::fs::copy(&dest, restored.path().join("core.db")).unwrap();
    modbit_event_store::EventStore::open(restored.path()).expect("the backup opens as a profile");

    let again = core(
        &[
            "backup",
            "--data-dir",
            dir.path().to_str().unwrap(),
            "--to",
            dest.to_str().unwrap(),
        ],
        &[],
    );
    assert!(
        !again.status.success(),
        "an existing backup is never overwritten"
    );
}

#[test]
fn device_policy_prints_the_constraints_the_updater_must_honor() {
    let dir = tempfile::tempdir().unwrap();
    let none = dir.path().join("absent.json");
    let unset = json(&core(
        &["device-policy"],
        &[("MODBIT_DEVICE_POLICY", none.to_str().unwrap())],
    ));
    assert!(unset["update_channel"].is_null() && unset["minimum_version"].is_null());

    let file = dir.path().join("device.json");
    std::fs::write(
        &file,
        r#"{"device":{"update_channel":"stable","minimum_version":"1.2.0"}}"#,
    )
    .unwrap();
    let set = json(&core(
        &["device-policy"],
        &[("MODBIT_DEVICE_POLICY", file.to_str().unwrap())],
    ));
    assert_eq!(set["update_channel"], "stable");
    assert_eq!(set["minimum_version"], "1.2.0");

    // A file that is not a configuration layer is no opinion, as it is for the Core's own reader.
    std::fs::write(&file, "not json").unwrap();
    let broken = json(&core(
        &["device-policy"],
        &[("MODBIT_DEVICE_POLICY", file.to_str().unwrap())],
    ));
    assert!(broken["minimum_version"].is_null());
}

#[test]
fn the_subcommands_refuse_missing_and_unknown_arguments() {
    assert_eq!(core(&["schema-info"], &[]).status.code(), Some(2));
    assert_eq!(
        core(&["backup", "--data-dir", "x"], &[]).status.code(),
        Some(2)
    );
    assert_eq!(
        core(&["schema-info", "--data-dir", "x", "--bogus"], &[])
            .status
            .code(),
        Some(2)
    );
}
