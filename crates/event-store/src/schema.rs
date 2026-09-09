//! Schema for `core.db` (docs/31). Versioned migrations with forward SQL and a
//! rollback/read-compatibility note each; the store refuses write mode when it
//! meets a newer schema than it supports and detects drift of an applied
//! migration's SQL through its checksum.

/// One migration.
#[derive(Debug, Clone, Copy)]
pub struct Migration {
    /// Monotonic version, starting at 1.
    pub version: u32,
    /// Short name.
    pub name: &'static str,
    /// Forward DDL (idempotent `IF NOT EXISTS` forms are still recorded once).
    pub up: &'static str,
    /// Rollback / read-compatibility plan (docs/31 "Migration safety").
    pub rollback: &'static str,
}

/// Inline payload ceiling in bytes; larger payloads go to the object store
/// (docs/33 "Backpressure": inline size ceiling, larger objects by ref).
pub const INLINE_PAYLOAD_CEILING: usize = 64 * 1024;

/// Version 1 (M1.1): the append-only events table. `offset` is the store-wide
/// monotonic cursor used for session resume; `(aggregate_id, sequence)` is the
/// per-aggregate order docs/13 orders on.
pub const V1_EVENTS: &str = r#"
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

/// Version 2 (M1.2): projections of the five core aggregates (docs/31
/// `sessions`, `tasks`, `runs`, `turns`, `run_steps`), the projection cursor,
/// and the command idempotency ledger (docs/30, docs/33 "Idempotency").
pub const V2_PROJECTIONS_AND_COMMANDS: &str = r#"
CREATE TABLE IF NOT EXISTS sessions (
  session_id          BLOB PRIMARY KEY NOT NULL,
  tenant_id           BLOB NOT NULL,
  user_id             BLOB NOT NULL,
  space_id            BLOB NOT NULL,
  state               TEXT NOT NULL,
  generation          INTEGER NOT NULL,
  created_at          INTEGER NOT NULL,
  updated_at          INTEGER NOT NULL,
  current_task_id     BLOB,
  last_event_sequence INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS tasks (
  task_id           BLOB PRIMARY KEY NOT NULL,
  session_id        BLOB NOT NULL REFERENCES sessions(session_id),
  goal_text         TEXT NOT NULL,
  workspace_id      BLOB NOT NULL,
  base_revision     TEXT,
  execution_profile TEXT NOT NULL,
  policy_profile_id TEXT,
  origin            TEXT NOT NULL,
  state             TEXT NOT NULL,
  wait_reason       TEXT,
  generation        INTEGER NOT NULL,
  created_at        INTEGER NOT NULL,
  started_at        INTEGER,
  completed_at      INTEGER,
  failure_code      TEXT
);
CREATE INDEX IF NOT EXISTS tasks_session ON tasks (session_id, created_at);
CREATE TABLE IF NOT EXISTS runs (
  run_id                  BLOB PRIMARY KEY NOT NULL,
  task_id                 BLOB NOT NULL REFERENCES tasks(task_id),
  attempt                 INTEGER NOT NULL,
  owner_location          TEXT NOT NULL,
  kernel_lease_generation INTEGER NOT NULL,
  state                   TEXT NOT NULL,
  generation              INTEGER NOT NULL,
  started_at              INTEGER,
  ended_at                INTEGER,
  UNIQUE (task_id, attempt)
);
CREATE TABLE IF NOT EXISTS turns (
  turn_id              BLOB PRIMARY KEY NOT NULL,
  run_id               BLOB NOT NULL REFERENCES runs(run_id),
  ordinal              INTEGER NOT NULL,
  state                TEXT NOT NULL,
  model_route_json     TEXT,
  tool_projection_hash TEXT,
  context_pack_id      TEXT,
  generation           INTEGER NOT NULL,
  started_at           INTEGER NOT NULL,
  ended_at             INTEGER,
  UNIQUE (run_id, ordinal)
);
CREATE TABLE IF NOT EXISTS run_steps (
  step_id        BLOB PRIMARY KEY NOT NULL,
  turn_id        BLOB NOT NULL REFERENCES turns(turn_id),
  step_type_json TEXT NOT NULL,
  state          TEXT NOT NULL,
  ordinal        INTEGER NOT NULL,
  generation     INTEGER NOT NULL,
  started_at     INTEGER,
  ended_at       INTEGER,
  input_ref      TEXT,
  output_ref     TEXT,
  failure_code   TEXT,
  UNIQUE (turn_id, ordinal)
);
CREATE TABLE IF NOT EXISTS projection_state (
  name        TEXT PRIMARY KEY NOT NULL,
  last_offset INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS commands (
  command_id         BLOB PRIMARY KEY NOT NULL,
  tenant_id          BLOB NOT NULL,
  command_type       TEXT NOT NULL,
  request_hash       TEXT NOT NULL,
  first_event_offset INTEGER,
  last_event_offset  INTEGER,
  accepted_at        INTEGER NOT NULL
);
"#;

/// Version 3 (M1 leases): session lease generation and owner (REQ-EV-0054/0273).
pub const V3_SESSION_LEASES: &str = r#"
ALTER TABLE sessions ADD COLUMN lease_generation INTEGER NOT NULL DEFAULT 0;
ALTER TABLE sessions ADD COLUMN lease_owner TEXT;
"#;

/// Version 4 (M2.4): task workspace root and the tool_calls projection (docs/31 `tool_calls`).
pub const V4_WORKSPACE_ROOT_AND_TOOL_CALLS: &str = r#"
ALTER TABLE tasks ADD COLUMN workspace_root TEXT;
CREATE TABLE IF NOT EXISTS tool_calls (
  tool_call_id           BLOB PRIMARY KEY NOT NULL,
  task_id                BLOB NOT NULL,
  step_id                BLOB,
  tool_name              TEXT NOT NULL,
  tool_version           TEXT NOT NULL,
  effect_class           TEXT NOT NULL,
  capability_lease_id    BLOB,
  status                 TEXT NOT NULL,
  arguments_hash         TEXT NOT NULL,
  generation             INTEGER NOT NULL,
  dispatched_at          INTEGER,
  completed_at           INTEGER,
  result_ref             TEXT,
  unknown_outcome_reason TEXT,
  policy_decision        TEXT
);
CREATE INDEX IF NOT EXISTS tool_calls_task ON tool_calls (task_id, dispatched_at);
"#;

/// All migrations in order. Never edit an entry once shipped; append a new one.
pub const MIGRATIONS: &[Migration] = &[
    Migration {
        version: 1,
        name: "events",
        up: V1_EVENTS,
        rollback: "No rollback: version 1 is the base. Older builds cannot read this database at all.",
    },
    Migration {
        version: 2,
        name: "projections_and_commands",
        up: V2_PROJECTIONS_AND_COMMANDS,
        rollback: "Read-compatible with version 1 readers of `events`: the new tables are derivable (`rebuild_projections`) and `commands` only adds idempotency. Rollback = drop the seven added tables; no event is touched.",
    },
    Migration {
        version: 3,
        name: "session_leases",
        up: V3_SESSION_LEASES,
        rollback: "Additive columns with defaults; version-2 readers ignore them. Rollback = rebuild projections after dropping the columns; no event is touched.",
    },
    Migration {
        version: 4,
        name: "workspace_root_and_tool_calls",
        up: V4_WORKSPACE_ROOT_AND_TOOL_CALLS,
        rollback: "Additive: a nullable column and a derivable table. Rollback = drop `tool_calls` and rebuild projections; no event is touched.",
    },
];

/// Schema version this build writes.
pub const SCHEMA_VERSION: u32 = MIGRATIONS[MIGRATIONS.len() - 1].version;
