//! Postgres schema for the cloud control plane (docs/31 "Cloud schema",
//! docs/24 "Postgres"): the canonical event log mirrored per tenant, the
//! projections the Cloud API serves, the command ledger, session leases,
//! principals and object metadata. Every tenant resource carries
//! `tenant_id`; every query in this crate scopes by it (docs/24
//! "Multi-tenancy"). Migrations are versioned, applied in order inside one
//! transaction each, and recorded with their checksum (docs/31 "Migration
//! safety": forward-only, never destructive, a newer incompatible schema
//! refuses write mode).

/// The schema version this build writes.
pub const CLOUD_SCHEMA_VERSION: i32 = 2;

/// Ordered migrations `(version, name, sql)`.
pub const MIGRATIONS: &[(i32, &str, &str)] = &[
    (
        1,
        "cloud-v1: tenants, principals, refresh tokens, events, projections, commands, leases, objects, denials",
        r"
CREATE TABLE IF NOT EXISTS cloud_schema (
  version INTEGER PRIMARY KEY,
  name TEXT NOT NULL,
  checksum TEXT NOT NULL,
  applied_at_ms BIGINT NOT NULL
);
CREATE TABLE IF NOT EXISTS tenants (
  tenant_id UUID PRIMARY KEY,
  name TEXT NOT NULL,
  created_at_ms BIGINT NOT NULL
);
CREATE TABLE IF NOT EXISTS principals (
  principal_id UUID PRIMARY KEY,
  tenant_id UUID NOT NULL REFERENCES tenants(tenant_id),
  kind TEXT NOT NULL,
  label TEXT NOT NULL,
  secret_hash TEXT NOT NULL UNIQUE,
  created_at_ms BIGINT NOT NULL,
  revoked_at_ms BIGINT
);
CREATE INDEX IF NOT EXISTS principals_tenant ON principals(tenant_id);
CREATE TABLE IF NOT EXISTS refresh_tokens (
  token_hash TEXT PRIMARY KEY,
  tenant_id UUID NOT NULL REFERENCES tenants(tenant_id),
  principal_id UUID NOT NULL REFERENCES principals(principal_id),
  issued_at_ms BIGINT NOT NULL,
  expires_at_ms BIGINT NOT NULL,
  rotated_to TEXT,
  revoked_at_ms BIGINT
);
CREATE TABLE IF NOT EXISTS sessions (
  session_id UUID PRIMARY KEY,
  tenant_id UUID NOT NULL REFERENCES tenants(tenant_id),
  state TEXT NOT NULL,
  generation BIGINT NOT NULL,
  last_session_offset BIGINT NOT NULL DEFAULT 0,
  doc JSONB NOT NULL,
  created_at_ms BIGINT NOT NULL,
  updated_at_ms BIGINT NOT NULL
);
CREATE INDEX IF NOT EXISTS sessions_tenant ON sessions(tenant_id, created_at_ms);
CREATE TABLE IF NOT EXISTS events (
  event_offset BIGSERIAL PRIMARY KEY,
  tenant_id UUID NOT NULL REFERENCES tenants(tenant_id),
  session_id UUID NOT NULL,
  session_offset BIGINT NOT NULL,
  task_id UUID,
  run_id UUID,
  aggregate_type TEXT NOT NULL,
  aggregate_id BYTEA NOT NULL,
  sequence BIGINT NOT NULL,
  event_type TEXT NOT NULL,
  occurred_at_ms BIGINT NOT NULL,
  envelope JSONB NOT NULL,
  payload JSONB,
  payload_ref TEXT,
  UNIQUE (session_id, session_offset),
  UNIQUE (aggregate_id, sequence)
);
CREATE INDEX IF NOT EXISTS events_task ON events(task_id, event_offset);
CREATE TABLE IF NOT EXISTS tasks (
  task_id UUID PRIMARY KEY,
  tenant_id UUID NOT NULL REFERENCES tenants(tenant_id),
  session_id UUID NOT NULL REFERENCES sessions(session_id),
  state TEXT NOT NULL,
  wait_reason TEXT,
  generation BIGINT NOT NULL,
  doc JSONB NOT NULL,
  created_at_ms BIGINT NOT NULL,
  updated_at_ms BIGINT NOT NULL
);
CREATE INDEX IF NOT EXISTS tasks_session ON tasks(session_id, created_at_ms);
CREATE TABLE IF NOT EXISTS approvals (
  approval_id UUID PRIMARY KEY,
  tenant_id UUID NOT NULL REFERENCES tenants(tenant_id),
  session_id UUID NOT NULL,
  task_id UUID NOT NULL,
  state TEXT NOT NULL,
  intent_hash TEXT NOT NULL,
  generation BIGINT NOT NULL,
  doc JSONB NOT NULL,
  updated_at_ms BIGINT NOT NULL
);
CREATE INDEX IF NOT EXISTS approvals_task ON approvals(task_id);
CREATE TABLE IF NOT EXISTS commands (
  command_id UUID PRIMARY KEY,
  tenant_id UUID NOT NULL REFERENCES tenants(tenant_id),
  kind TEXT NOT NULL,
  status TEXT NOT NULL,
  code TEXT NOT NULL,
  result JSONB NOT NULL,
  first_offset BIGINT,
  last_offset BIGINT,
  recorded_at_ms BIGINT NOT NULL
);
CREATE TABLE IF NOT EXISTS session_leases (
  session_id UUID PRIMARY KEY REFERENCES sessions(session_id),
  tenant_id UUID NOT NULL REFERENCES tenants(tenant_id),
  worker_id TEXT,
  generation BIGINT NOT NULL DEFAULT 0,
  claimed_at_ms BIGINT,
  heartbeat_at_ms BIGINT,
  expires_at_ms BIGINT NOT NULL DEFAULT 0,
  ready BOOLEAN NOT NULL DEFAULT FALSE
);
CREATE INDEX IF NOT EXISTS session_leases_ready ON session_leases(ready, expires_at_ms);
CREATE TABLE IF NOT EXISTS objects (
  tenant_id UUID NOT NULL REFERENCES tenants(tenant_id),
  hash TEXT NOT NULL,
  byte_length BIGINT NOT NULL,
  mime TEXT NOT NULL,
  key TEXT NOT NULL,
  created_at_ms BIGINT NOT NULL,
  PRIMARY KEY (tenant_id, hash)
);
CREATE TABLE IF NOT EXISTS denials (
  denial_id BIGSERIAL PRIMARY KEY,
  tenant_id UUID,
  principal_id UUID,
  resource TEXT NOT NULL,
  reason TEXT NOT NULL,
  at_ms BIGINT NOT NULL
);
",
    ),
    (
        2,
        "cloud-v2: commands relayed to the session's execution owner (M8.2)",
        r"
ALTER TABLE commands ADD COLUMN IF NOT EXISTS session_id UUID;
ALTER TABLE commands ADD COLUMN IF NOT EXISTS body JSONB;
ALTER TABLE commands ADD COLUMN IF NOT EXISTS completed_at_ms BIGINT;
CREATE INDEX IF NOT EXISTS commands_pending ON commands(session_id, recorded_at_ms) WHERE status = 'PENDING';
",
    ),
];
