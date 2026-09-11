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

/// Version 5 (M2.5): approvals, capability leases, the effect receipt chain
/// (docs/31 `approvals`, `capability_leases`, `effect_receipts`) and the
/// session emergency stop.
pub const V5_KERNEL: &str = r#"
ALTER TABLE sessions ADD COLUMN emergency_stopped_at INTEGER;
ALTER TABLE tool_calls ADD COLUMN approval_id BLOB;
CREATE TABLE IF NOT EXISTS approvals (
  approval_id      BLOB PRIMARY KEY NOT NULL,
  task_id          BLOB NOT NULL,
  tool_call_id     BLOB NOT NULL,
  tool_name        TEXT NOT NULL,
  effect_class     TEXT NOT NULL,
  intent_hash      TEXT NOT NULL,
  scope_json       TEXT NOT NULL,
  status           TEXT NOT NULL,
  generation       INTEGER NOT NULL,
  requested_at     INTEGER NOT NULL,
  resolved_at      INTEGER,
  resolver_user_id TEXT,
  expires_at       INTEGER
);
CREATE INDEX IF NOT EXISTS approvals_task ON approvals (task_id, status);
CREATE INDEX IF NOT EXISTS approvals_tool_call ON approvals (tool_call_id);
CREATE TABLE IF NOT EXISTS capability_leases (
  lease_id          BLOB PRIMARY KEY NOT NULL,
  tenant_id         BLOB NOT NULL,
  task_id           BLOB NOT NULL,
  agent_id          TEXT,
  resource_json     TEXT NOT NULL,
  operations_json   TEXT NOT NULL,
  effect_ceiling    TEXT NOT NULL,
  execution_profile TEXT NOT NULL,
  generation        INTEGER NOT NULL,
  status            TEXT NOT NULL,
  expires_at        INTEGER,
  revoked_at        INTEGER,
  revoke_reason     TEXT
);
CREATE INDEX IF NOT EXISTS capability_leases_task ON capability_leases (task_id, status);
CREATE TABLE IF NOT EXISTS effect_receipts (
  effect_id             BLOB PRIMARY KEY NOT NULL,
  seq                   INTEGER NOT NULL,
  previous_receipt_hash TEXT,
  task_id               BLOB NOT NULL,
  turn_id               BLOB,
  step_id               BLOB,
  tool_call_id          BLOB NOT NULL,
  capability_lease_id   BLOB,
  intent_hash           TEXT NOT NULL,
  policy_decision       TEXT NOT NULL,
  approval_id           BLOB,
  execution_target      TEXT NOT NULL,
  evidence_ref          TEXT,
  status                TEXT NOT NULL,
  occurred_at           INTEGER NOT NULL,
  receipt_hash          TEXT NOT NULL
);
CREATE UNIQUE INDEX IF NOT EXISTS effect_receipts_seq ON effect_receipts (seq);
CREATE INDEX IF NOT EXISTS effect_receipts_task ON effect_receipts (task_id, seq);
"#;

/// Version 6 (M2.8): verification runs, check results and flaky-check
/// quarantines (docs/31 `verification_runs`, `check_results`, `flaky_checks`).
pub const V6_VERIFICATION: &str = r#"
CREATE TABLE IF NOT EXISTS verification_runs (
  verification_run_id TEXT PRIMARY KEY NOT NULL,
  run_id              BLOB NOT NULL,
  plan_ref            TEXT NOT NULL,
  stage               TEXT NOT NULL,
  candidate_revision  TEXT NOT NULL,
  environment_digest  TEXT NOT NULL,
  started_at          INTEGER NOT NULL,
  ended_at            INTEGER,
  status              TEXT NOT NULL,
  report_ref          TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS verification_runs_run ON verification_runs (run_id, started_at);
CREATE TABLE IF NOT EXISTS check_results (
  verification_run_id TEXT NOT NULL,
  check_id            TEXT NOT NULL,
  kind                TEXT NOT NULL,
  status              TEXT NOT NULL,
  duration_ms         INTEGER NOT NULL,
  location_ref        TEXT,
  error_class         TEXT,
  message_fingerprint TEXT,
  output_ref          TEXT,
  PRIMARY KEY (verification_run_id, check_id)
);
CREATE TABLE IF NOT EXISTS flaky_checks (
  flaky_id            TEXT PRIMARY KEY NOT NULL,
  run_id              BLOB NOT NULL,
  check_id            TEXT NOT NULL,
  first_run_id        TEXT NOT NULL,
  rerun_id            TEXT NOT NULL,
  quarantined_at      INTEGER NOT NULL,
  scope_revision_range TEXT NOT NULL,
  expires_reason      TEXT,
  resolved_at         INTEGER
);
CREATE INDEX IF NOT EXISTS flaky_checks_run ON flaky_checks (run_id);
"#;

/// Version 7 (EPR-001): the durable routing state — the compiled plan, its
/// slots and every attempt made against them. Derivable from the events, so a
/// rollback drops the tables and rebuilds.
pub const V7_ROUTING: &str = r#"
CREATE TABLE IF NOT EXISTS routing_plans (
  plan_id            TEXT PRIMARY KEY NOT NULL,
  tenant_id          BLOB NOT NULL,
  session_id         BLOB NOT NULL,
  task_id            BLOB NOT NULL,
  run_id             BLOB NOT NULL,
  schema_version     INTEGER NOT NULL,
  routing_epoch      INTEGER NOT NULL,
  lease_generation   INTEGER NOT NULL,
  created_at         INTEGER NOT NULL,
  input_digest       TEXT NOT NULL,
  content_digest     TEXT NOT NULL,
  plan_ref           TEXT NOT NULL,
  total_budget_minor INTEGER NOT NULL,
  total_budget_currency TEXT NOT NULL,
  total_budget_scale INTEGER NOT NULL,
  legacy_source      TEXT,
  UNIQUE (run_id, routing_epoch)
);
CREATE INDEX IF NOT EXISTS routing_plans_task ON routing_plans (task_id, created_at);
CREATE TABLE IF NOT EXISTS routing_slots (
  plan_id          TEXT NOT NULL REFERENCES routing_plans(plan_id),
  slot_id          TEXT NOT NULL,
  predecessor      TEXT,
  trigger          TEXT NOT NULL,
  max_activations  INTEGER NOT NULL,
  endpoint         TEXT NOT NULL,
  model            TEXT NOT NULL,
  role             TEXT NOT NULL,
  timeout_ms       INTEGER NOT NULL,
  max_output_tokens INTEGER NOT NULL,
  max_retries      INTEGER NOT NULL,
  reserved_minor   INTEGER NOT NULL,
  activations      INTEGER NOT NULL DEFAULT 0,
  PRIMARY KEY (plan_id, slot_id)
);
CREATE TABLE IF NOT EXISTS routing_attempts (
  plan_id        TEXT NOT NULL,
  slot_id        TEXT NOT NULL,
  attempt        INTEGER NOT NULL,
  started_at     INTEGER NOT NULL,
  ended_at       INTEGER,
  outcome        TEXT NOT NULL,
  usage_known    INTEGER NOT NULL,
  input_tokens   INTEGER,
  output_tokens  INTEGER,
  provider_request_id TEXT,
  PRIMARY KEY (plan_id, slot_id, attempt)
);
CREATE INDEX IF NOT EXISTS routing_attempts_plan ON routing_attempts (plan_id, started_at);
"#;

/// V8 (REQ-EPR-014): what admission decided. The plan a run was admitted with,
/// the digest of exactly what was validated and the money it reserved; and one
/// row per slot activation, which is what bounds a slot rather than the count
/// of attempts inside it.
pub const V8_ADMISSION: &str = r#"
CREATE TABLE IF NOT EXISTS routing_admissions (
  plan_id           TEXT PRIMARY KEY NOT NULL,
  validation_digest TEXT NOT NULL,
  reserved_minor    INTEGER NOT NULL,
  currency          TEXT NOT NULL,
  scale             INTEGER NOT NULL,
  lease_generation  INTEGER NOT NULL,
  admitted_at       INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS routing_activations (
  plan_id        TEXT NOT NULL,
  slot_id        TEXT NOT NULL,
  activation     INTEGER NOT NULL,
  reserved_minor INTEGER NOT NULL,
  activated_at   INTEGER NOT NULL,
  PRIMARY KEY (plan_id, slot_id, activation)
);
CREATE INDEX IF NOT EXISTS routing_activations_plan ON routing_activations (plan_id, activated_at);
"#;

/// V9 (REQ-EPR-016): what confidence-adjusted feasibility said about an
/// admitted plan, alongside the admission it belongs to.
pub const V9_FEASIBILITY: &str = r#"
ALTER TABLE routing_admissions ADD COLUMN feasibility TEXT NOT NULL DEFAULT '';
ALTER TABLE routing_admissions ADD COLUMN quality_lcb_bp INTEGER NOT NULL DEFAULT 0;
ALTER TABLE routing_admissions ADD COLUMN stats_version TEXT NOT NULL DEFAULT '';
ALTER TABLE routing_admissions ADD COLUMN thresholds_version TEXT NOT NULL DEFAULT '';
ALTER TABLE routing_admissions ADD COLUMN target_met INTEGER NOT NULL DEFAULT 0;
"#;

/// V10 (M4.1, REQ-EV-0055 protocol state): a tool call is bound to the run,
/// the turn and the model's call id that proposed it, and to the object that
/// holds its raw arguments, so a restarted Core re-enters the same call.
pub const V10_PROTOCOL_STATE: &str = r#"
ALTER TABLE tool_calls ADD COLUMN run_id BLOB;
ALTER TABLE tool_calls ADD COLUMN turn_id BLOB;
ALTER TABLE tool_calls ADD COLUMN call_id TEXT;
ALTER TABLE tool_calls ADD COLUMN arguments_ref TEXT;
CREATE INDEX IF NOT EXISTS tool_calls_run ON tool_calls (run_id, status);
CREATE TABLE IF NOT EXISTS protocol_state (
  session_id   BLOB    NOT NULL,
  protocol_key TEXT    NOT NULL,
  payload      TEXT    NOT NULL,
  generation   INTEGER NOT NULL,
  updated_at   INTEGER NOT NULL,
  PRIMARY KEY (session_id, protocol_key)
);
"#;

/// V11 (M4.2, docs/19 "Compaction epochs", docs/31 `compaction_epochs`):
/// the session branch generation a fork or revert moves, and the record of
/// every compaction request — pending, committed as an epoch, or rejected.
pub const V11_COMPACTION_EPOCHS: &str = r#"
ALTER TABLE sessions ADD COLUMN branch_generation INTEGER NOT NULL DEFAULT 0;
CREATE TABLE IF NOT EXISTS compaction_epochs (
  epoch_id           TEXT    PRIMARY KEY NOT NULL,
  session_id         BLOB    NOT NULL,
  task_id            BLOB    NOT NULL,
  branch_generation  INTEGER NOT NULL,
  epoch              INTEGER NOT NULL,
  previous_epoch_id  TEXT,
  source_event_start INTEGER NOT NULL,
  source_event_end   INTEGER NOT NULL,
  source_entries     INTEGER NOT NULL,
  compiler_version   TEXT    NOT NULL,
  target_tokens      INTEGER NOT NULL,
  status             TEXT    NOT NULL,
  mode               TEXT    NOT NULL,
  result_object_hash TEXT,
  rejection          TEXT,
  created_at         INTEGER NOT NULL,
  committed_at       INTEGER
);
CREATE INDEX IF NOT EXISTS compaction_epochs_task ON compaction_epochs (task_id, created_at);
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
    Migration {
        version: 5,
        name: "kernel_approvals_leases_receipts",
        up: V5_KERNEL,
        rollback: "Additive: nullable columns and derivable tables. Rollback = drop `approvals`, `capability_leases`, `effect_receipts` and rebuild projections; no event is touched.",
    },
    Migration {
        version: 6,
        name: "verification_runs_checks_flaky",
        up: V6_VERIFICATION,
        rollback: "Additive derivable tables. Rollback = drop `verification_runs`, `check_results`, `flaky_checks` and rebuild projections; no event is touched.",
    },
    Migration {
        version: 7,
        name: "routing_plans_slots_attempts",
        up: V7_ROUTING,
        rollback: "Additive derivable tables. Rollback = drop `routing_plans`, `routing_slots`, `routing_attempts` and rebuild projections; no event is touched.",
    },
    Migration {
        version: 8,
        name: "routing_admissions_activations",
        up: V8_ADMISSION,
        rollback: "Additive derivable tables. Rollback = drop `routing_admissions` and `routing_activations` and rebuild projections; no event is touched.",
    },
    Migration {
        version: 9,
        name: "routing_admission_feasibility",
        up: V9_FEASIBILITY,
        rollback: "Additive derivable columns with defaults. Rollback = older builds ignore the columns; a rebuild derives them from the log; no event is touched.",
    },
    Migration {
        version: 10,
        name: "protocol_state",
        up: V10_PROTOCOL_STATE,
        rollback: "Additive nullable derivable columns, an index and the derivable `protocol_state` table (docs/31). Rollback = drop the table; older builds ignore the columns; a rebuild derives everything from the log; no event is touched.",
    },
    Migration {
        version: 11,
        name: "compaction_epochs",
        up: V11_COMPACTION_EPOCHS,
        rollback: "Additive: a defaulted column on `sessions` and the derivable `compaction_epochs` table (docs/31). Rollback = drop the table; older builds ignore the column; a rebuild derives the rows from the log; no event is touched.",
    },
];

/// Schema version this build writes.
pub const SCHEMA_VERSION: u32 = MIGRATIONS[MIGRATIONS.len() - 1].version;
