//! Migration runner (docs/31 "Migration safety"): forward-only, checksummed,
//! recorded in `schema_migrations`; never drops unknown tables or columns;
//! refuses write mode on a newer schema; detects drift of applied SQL.

use rusqlite::{Connection, OptionalExtension, params};

use crate::objects::sha256_hex;
use crate::schema::{MIGRATIONS, SCHEMA_VERSION};
use crate::{Error, Result};

const LEDGER: &str = r#"
CREATE TABLE IF NOT EXISTS schema_migrations (
  version    INTEGER PRIMARY KEY NOT NULL,
  name       TEXT NOT NULL,
  checksum   TEXT NOT NULL,
  applied_at INTEGER NOT NULL
);
"#;

/// Outcome of running migrations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MigrationReport {
    /// Version found before running (0 for a new database).
    pub from_version: u32,
    /// Version after running.
    pub to_version: u32,
    /// Versions applied by this run.
    pub applied: Vec<u32>,
}

/// Current recorded schema version (0 when the ledger is absent or empty).
pub fn current_version(conn: &Connection) -> Result<u32> {
    let has_ledger: Option<String> = conn
        .query_row(
            "SELECT name FROM sqlite_master WHERE type='table' AND name='schema_migrations'",
            [],
            |r| r.get(0),
        )
        .optional()?;
    if has_ledger.is_none() {
        // A version-1 database written before the ledger existed records itself in schema_meta.
        let has_meta: Option<String> = conn
            .query_row(
                "SELECT name FROM sqlite_master WHERE type='table' AND name='schema_meta'",
                [],
                |r| r.get(0),
            )
            .optional()?;
        if has_meta.is_none() {
            return Ok(0);
        }
        let v: Option<String> = conn
            .query_row(
                "SELECT value FROM schema_meta WHERE key='schema_version'",
                [],
                |r| r.get(0),
            )
            .optional()?;
        return Ok(v.and_then(|s| s.parse().ok()).unwrap_or(0));
    }
    let v: Option<i64> = conn.query_row("SELECT MAX(version) FROM schema_migrations", [], |r| {
        r.get(0)
    })?;
    Ok(v.unwrap_or(0) as u32)
}

/// Apply every pending migration atomically under one IMMEDIATE transaction,
/// so concurrent openers of a fresh database serialize instead of racing.
pub fn migrate(conn: &mut Connection) -> Result<MigrationReport> {
    let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    // Read the version before creating the ledger so a pre-ledger version-1
    // database (schema_meta only) is recognised as version 1, not 0.
    let from_version = current_version(&tx)?;
    tx.execute_batch(LEDGER)?;
    if from_version > SCHEMA_VERSION {
        return Err(Error::SchemaTooNew {
            found: from_version,
            supported: SCHEMA_VERSION,
        });
    }
    let now = modbit_domain::Timestamp::now().millis();
    // Drift check: an applied migration's SQL must still hash the same.
    for m in MIGRATIONS.iter().filter(|m| m.version <= from_version) {
        let recorded: Option<String> = tx
            .query_row(
                "SELECT checksum FROM schema_migrations WHERE version = ?1",
                params![m.version],
                |r| r.get(0),
            )
            .optional()?;
        match recorded {
            Some(c) if c != sha256_hex(m.up.as_bytes()) => {
                return Err(Error::Integrity {
                    aggregate: "<schema>".into(),
                    sequence: u64::from(m.version),
                    detail: format!(
                        "migration {} ({}) SQL differs from the applied checksum",
                        m.version, m.name
                    ),
                });
            }
            Some(_) => {}
            None => {
                // Pre-ledger version-1 database: adopt it into the ledger without re-running.
                tx.execute(
                    "INSERT INTO schema_migrations (version, name, checksum, applied_at) VALUES (?1, ?2, ?3, ?4)",
                    params![m.version, m.name, sha256_hex(m.up.as_bytes()), now],
                )?;
            }
        }
    }
    let mut applied = Vec::new();
    for m in MIGRATIONS.iter().filter(|m| m.version > from_version) {
        tx.execute_batch(m.up)?;
        tx.execute(
            "INSERT INTO schema_migrations (version, name, checksum, applied_at) VALUES (?1, ?2, ?3, ?4)",
            params![m.version, m.name, sha256_hex(m.up.as_bytes()), now],
        )?;
        tx.execute(
            "INSERT OR REPLACE INTO schema_meta (key, value) VALUES ('schema_version', ?1)",
            params![m.version.to_string()],
        )?;
        applied.push(m.version);
    }
    tx.commit()?;
    Ok(MigrationReport {
        from_version,
        to_version: SCHEMA_VERSION,
        applied,
    })
}

/// Rollback plans, for operators (docs/31 requires one per migration).
#[must_use]
pub fn rollback_plans() -> Vec<(u32, &'static str, &'static str)> {
    MIGRATIONS
        .iter()
        .map(|m| (m.version, m.name, m.rollback))
        .collect()
}

/// What an updater needs to know before it installs a build (docs/70 "Desktop
/// update"): the schema this build writes and the one on disk, read without
/// migrating anything.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SchemaInfo {
    /// `SCHEMA_VERSION` of this build.
    pub build_schema_version: u32,
    /// Schema recorded in `core.db`; `None` when the profile has no database yet.
    pub on_disk_schema_version: Option<u32>,
}

/// Read the schema versions for the profile rooted at `dir`. Opens `core.db`
/// read-only: no migration, no WAL checkpoint, no file created, so it is safe
/// beside a running Core and cannot change what a later install must open.
pub fn schema_info(dir: &std::path::Path) -> Result<SchemaInfo> {
    let db = dir.join("core.db");
    let on_disk = if db.exists() {
        let conn = Connection::open_with_flags(
            &db,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )?;
        conn.busy_timeout(std::time::Duration::from_secs(10))?;
        Some(current_version(&conn)?)
    } else {
        None
    };
    Ok(SchemaInfo {
        build_schema_version: SCHEMA_VERSION,
        on_disk_schema_version: on_disk,
    })
}

/// A consistent single-file copy of `core.db` (docs/70: a critical migration
/// takes a backup first). `VACUUM INTO` reads one transaction's snapshot, so a
/// Core writing in WAL mode cannot tear the copy; the destination must not exist.
pub fn backup_database(dir: &std::path::Path, dest: &std::path::Path) -> Result<u32> {
    if dest.exists() {
        return Err(Error::Io(std::io::Error::new(
            std::io::ErrorKind::AlreadyExists,
            format!("backup destination {} exists", dest.display()),
        )));
    }
    let conn = Connection::open_with_flags(
        dir.join("core.db"),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    conn.busy_timeout(std::time::Duration::from_secs(10))?;
    let version = current_version(&conn)?;
    let dest_str = dest.to_string_lossy().into_owned();
    conn.execute("VACUUM INTO ?1", [dest_str])?;
    Ok(version)
}
