//! Schema for `core.db` events (docs/31 `events`). Versioned; the store
//! refuses write mode when it meets a newer schema than it supports.

/// Schema version this build writes.
pub const SCHEMA_VERSION: u32 = 1;

/// Inline payload ceiling in bytes; larger payloads go to the object store
/// (docs/33 "Backpressure": inline size ceiling, larger objects by ref).
pub const INLINE_PAYLOAD_CEILING: usize = 64 * 1024;

/// Version 1 DDL. `offset` is the store-wide monotonic cursor used for session
/// resume; `(aggregate_id, sequence)` is the per-aggregate order docs/13 orders on.
pub const V1: &str = r#"
CREATE TABLE IF NOT EXISTS schema_meta (
  key   TEXT PRIMARY KEY NOT NULL,
  value TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS events (
  offset              INTEGER PRIMARY KEY AUTOINCREMENT,
  event_id            BLOB    NOT NULL UNIQUE,
  tenant_id           BLOB    NOT NULL,
  session_id          BLOB    NOT NULL,
  task_id             BLOB,
  run_id              BLOB,
  turn_id             BLOB,
  step_id             BLOB,
  aggregate_type      TEXT    NOT NULL,
  aggregate_id        BLOB    NOT NULL,
  sequence            INTEGER NOT NULL CHECK (sequence >= 1),
  event_type          TEXT    NOT NULL,
  schema_version      INTEGER NOT NULL,
  occurred_at         INTEGER NOT NULL,
  actor_type          TEXT    NOT NULL,
  actor_id            TEXT    NOT NULL,
  causation_id        BLOB,
  correlation_id      BLOB,
  payload_inline      TEXT,
  payload_object_hash TEXT,
  payload_byte_length INTEGER,
  integrity_hash      TEXT    NOT NULL,
  CHECK ((payload_inline IS NOT NULL) <> (payload_object_hash IS NOT NULL)),
  UNIQUE (aggregate_id, sequence)
);
CREATE INDEX IF NOT EXISTS events_session_offset ON events (session_id, offset);
CREATE INDEX IF NOT EXISTS events_aggregate ON events (aggregate_type, aggregate_id, sequence);
"#;
