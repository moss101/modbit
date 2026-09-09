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

/// Apply every pending migration in its own transaction.
pub fn migrate(conn: &mut Connection) -> Result<MigrationReport> {
    // Read the version before creating the ledger so a pre-ledger version-1
    // database (schema_meta only) is recognised as version 1, not 0.
    let from_version = current_version(conn)?;
    conn.execute_batch(LEDGER)?;
    if from_version > SCHEMA_VERSION {
        return Err(Error::SchemaTooNew {
            found: from_version,
            supported: SCHEMA_VERSION,
        });
    }
    // Drift check: an applied migration's SQL must still hash the same.
    for m in MIGRATIONS.iter().filter(|m| m.version <= from_version) {
        let recorded: Option<String> = conn
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
                let tx = conn.transaction()?;
                tx.execute(
                    "INSERT INTO schema_migrations (version, name, checksum, applied_at) VALUES (?1, ?2, ?3, ?4)",
                    params![m.version, m.name, sha256_hex(m.up.as_bytes()), modbit_domain::Timestamp::now().millis()],
                )?;
                tx.commit()?;
            }
        }
    }
    let mut applied = Vec::new();
    for m in MIGRATIONS.iter().filter(|m| m.version > from_version) {
        let tx = conn.transaction()?;
        tx.execute_batch(m.up)?;
        tx.execute(
            "INSERT INTO schema_migrations (version, name, checksum, applied_at) VALUES (?1, ?2, ?3, ?4)",
            params![m.version, m.name, sha256_hex(m.up.as_bytes()), modbit_domain::Timestamp::now().millis()],
        )?;
        tx.execute(
            "INSERT OR REPLACE INTO schema_meta (key, value) VALUES ('schema_version', ?1)",
            params![m.version.to_string()],
        )?;
        tx.commit()?;
        applied.push(m.version);
    }
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
