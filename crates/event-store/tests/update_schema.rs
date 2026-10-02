//! M10.2 (docs/70 "Desktop update"): what an updater reads before it installs.
//! Real SQLite files; the version probe must never migrate, create or lock
//! anything, and the backup must be a database a build can open on its own.

use modbit_domain::event::{Actor, AggregateType};
use modbit_domain::{SessionId, TaskId, TenantId};
use modbit_event_store::{
    AppendRequest, Error, EventStore, NewEvent, backup_database, schema_info,
};
use rusqlite::Connection;
use serde_json::json;

fn write_one_event(dir: &std::path::Path) {
    let mut store = EventStore::open(dir).unwrap();
    store
        .append(AppendRequest {
            tenant_id: TenantId::from_bytes([7; 16]),
            session_id: SessionId::new(),
            task_id: None,
            run_id: None,
            turn_id: None,
            step_id: None,
            aggregate_type: AggregateType::Checkpoint,
            aggregate_id: *TaskId::new().as_bytes(),
            expected_sequence: Some(0),
            events: vec![NewEvent::new(
                "TaskCreated",
                json!({"goal": "x"}),
                Actor::Core("test".into()),
            )],
        })
        .unwrap();
}

fn event_count(db: &std::path::Path) -> i64 {
    Connection::open(db)
        .unwrap()
        .query_row("SELECT COUNT(*) FROM events", [], |r| r.get(0))
        .unwrap()
}

#[test]
fn a_profile_without_a_database_reports_none_and_the_probe_creates_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let info = schema_info(dir.path()).unwrap();
    assert_eq!(info.on_disk_schema_version, None);
    assert!(info.build_schema_version >= 1);
    assert!(
        !dir.path().join("core.db").exists(),
        "a read-only probe must not create the database"
    );
}

#[test]
fn the_probe_reports_the_on_disk_version_without_migrating_or_modifying_the_file() {
    let dir = tempfile::tempdir().unwrap();
    write_one_event(dir.path());
    let info = schema_info(dir.path()).unwrap();
    assert_eq!(info.on_disk_schema_version, Some(info.build_schema_version));

    // A database written by a newer build is reported as it is, not refused and not touched.
    let db = dir.path().join("core.db");
    {
        let c = Connection::open(&db).unwrap();
        c.pragma_update(None, "journal_mode", "DELETE").unwrap();
        c.execute(
            "INSERT INTO schema_migrations(version, name, checksum, applied_at) VALUES (?1, 'from-a-newer-build', 'x', 0)",
            [i64::from(info.build_schema_version) + 5],
        )
        .unwrap();
    }
    let before = std::fs::read(&db).unwrap();
    let newer = schema_info(dir.path()).unwrap();
    assert_eq!(
        newer.on_disk_schema_version,
        Some(info.build_schema_version + 5)
    );
    assert_eq!(
        std::fs::read(&db).unwrap(),
        before,
        "the probe left the database byte-identical"
    );
}

#[test]
fn a_backup_is_a_complete_openable_database_at_the_version_it_was_taken() {
    let dir = tempfile::tempdir().unwrap();
    write_one_event(dir.path());
    let dest = dir.path().join("backups").join("pre-update.db");
    std::fs::create_dir_all(dest.parent().unwrap()).unwrap();
    let version = backup_database(dir.path(), &dest).unwrap();
    assert_eq!(
        version,
        schema_info(dir.path()).unwrap().build_schema_version
    );
    assert_eq!(event_count(&dest), event_count(&dir.path().join("core.db")));
    // The backup stands alone: a store opens it as its own profile.
    let restored = tempfile::tempdir().unwrap();
    std::fs::copy(&dest, restored.path().join("core.db")).unwrap();
    EventStore::open(restored.path()).unwrap();
}

#[test]
fn a_backup_never_overwrites_an_earlier_one_and_a_missing_database_is_an_error() {
    let dir = tempfile::tempdir().unwrap();
    write_one_event(dir.path());
    let dest = dir.path().join("b.db");
    backup_database(dir.path(), &dest).unwrap();
    let again = backup_database(dir.path(), &dest);
    assert!(
        matches!(again, Err(Error::Io(ref e)) if e.kind() == std::io::ErrorKind::AlreadyExists),
        "{again:?}"
    );

    let empty = tempfile::tempdir().unwrap();
    assert!(backup_database(empty.path(), &empty.path().join("x.db")).is_err());
    assert!(!empty.path().join("core.db").exists());
}
