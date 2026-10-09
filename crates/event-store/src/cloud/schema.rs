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
pub const CLOUD_SCHEMA_VERSION: i32 = 8;

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
    (
        3,
        "cloud-v3: sandbox leases the Sandbox Gateway issues per tenant, session, task and worker (M8.3)",
        r"
CREATE TABLE IF NOT EXISTS sandboxes (
  sandbox_id UUID PRIMARY KEY,
  tenant_id UUID NOT NULL REFERENCES tenants(tenant_id),
  session_id UUID NOT NULL,
  task_id UUID NOT NULL,
  worker_id TEXT NOT NULL,
  lease_generation BIGINT NOT NULL,
  backend TEXT NOT NULL,
  isolated BOOLEAN NOT NULL,
  state TEXT NOT NULL,
  detail TEXT NOT NULL DEFAULT '',
  created_at_ms BIGINT NOT NULL,
  updated_at_ms BIGINT NOT NULL
);
CREATE INDEX IF NOT EXISTS sandboxes_by_session ON sandboxes(tenant_id, session_id);
",
    ),
    (
        4,
        "cloud-v4: the egress broker's audit — every admission and refusal per sandbox (M8.6)",
        r"
CREATE TABLE IF NOT EXISTS sandbox_egress (
  egress_id BIGSERIAL PRIMARY KEY,
  sandbox_id UUID NOT NULL,
  tenant_id UUID NOT NULL,
  kind TEXT NOT NULL,
  destination TEXT NOT NULL,
  allowed BOOLEAN NOT NULL,
  capability TEXT NOT NULL,
  detail TEXT NOT NULL,
  at_ms BIGINT NOT NULL
);
CREATE INDEX IF NOT EXISTS sandbox_egress_by_sandbox ON sandbox_egress(sandbox_id, egress_id);
",
    ),
    (
        5,
        "cloud-v5: capability negotiation — what each live worker serves, what each session requires (IMP-EV-0072)",
        r"
CREATE TABLE IF NOT EXISTS workers (
  worker_id TEXT PRIMARY KEY,
  capabilities TEXT[] NOT NULL DEFAULT '{}',
  protocol TEXT NOT NULL DEFAULT '',
  registered_at_ms BIGINT NOT NULL,
  seen_at_ms BIGINT NOT NULL
);
ALTER TABLE session_leases ADD COLUMN IF NOT EXISTS requirements TEXT[] NOT NULL DEFAULT '{}';
",
    ),
    (
        6,
        "cloud-v6: forge webhook intake — a repository's tenant and session, and every delivery once (PX-011)",
        r"
CREATE TABLE IF NOT EXISTS forge_repositories (
  provider TEXT NOT NULL,
  repository TEXT NOT NULL,
  tenant_id UUID NOT NULL REFERENCES tenants(tenant_id),
  session_id UUID NOT NULL,
  installation_id BIGINT NOT NULL DEFAULT 0,
  intake_label TEXT NOT NULL DEFAULT '',
  workspace_root TEXT NOT NULL DEFAULT '',
  execution_profile TEXT NOT NULL DEFAULT 'cloud_isolated',
  mapped_by UUID,
  created_at_ms BIGINT NOT NULL,
  updated_at_ms BIGINT NOT NULL,
  PRIMARY KEY (provider, repository)
);
CREATE INDEX IF NOT EXISTS forge_repositories_tenant ON forge_repositories(tenant_id, provider, repository);
CREATE TABLE IF NOT EXISTS webhook_deliveries (
  provider TEXT NOT NULL,
  delivery_id TEXT NOT NULL,
  event TEXT NOT NULL,
  tenant_id UUID,
  outcome TEXT NOT NULL,
  task_id UUID,
  received_at_ms BIGINT NOT NULL,
  PRIMARY KEY (provider, delivery_id)
);
",
    ),
    (
        7,
        "cloud-v7: identity and policy — roles, OIDC identities and logins, the provisioning audit, organisation keys, signed policy bundles (PX-129)",
        r"
ALTER TABLE principals ADD COLUMN IF NOT EXISTS role TEXT NOT NULL DEFAULT 'member';
CREATE TABLE IF NOT EXISTS principal_identities (
  issuer TEXT NOT NULL,
  subject TEXT NOT NULL,
  tenant_id UUID NOT NULL REFERENCES tenants(tenant_id),
  principal_id UUID NOT NULL REFERENCES principals(principal_id),
  created_at_ms BIGINT NOT NULL,
  PRIMARY KEY (issuer, subject)
);
CREATE INDEX IF NOT EXISTS principal_identities_principal ON principal_identities(principal_id);
CREATE TABLE IF NOT EXISTS oidc_logins (
  state_hash TEXT PRIMARY KEY,
  nonce TEXT NOT NULL,
  code_challenge TEXT NOT NULL,
  redirect_uri TEXT NOT NULL,
  created_at_ms BIGINT NOT NULL,
  expires_at_ms BIGINT NOT NULL,
  consumed_at_ms BIGINT
);
CREATE TABLE IF NOT EXISTS provisioning_audit (
  audit_id BIGSERIAL PRIMARY KEY,
  at_ms BIGINT NOT NULL,
  actor TEXT NOT NULL,
  tenant_id UUID,
  action TEXT NOT NULL,
  target TEXT NOT NULL,
  detail JSONB NOT NULL
);
CREATE INDEX IF NOT EXISTS provisioning_audit_tenant ON provisioning_audit(tenant_id, audit_id);
CREATE TABLE IF NOT EXISTS org_keys (
  tenant_id UUID NOT NULL REFERENCES tenants(tenant_id),
  key_id TEXT NOT NULL,
  public_key TEXT NOT NULL,
  created_at_ms BIGINT NOT NULL,
  revoked_at_ms BIGINT,
  PRIMARY KEY (tenant_id, key_id)
);
CREATE TABLE IF NOT EXISTS policy_bundles (
  tenant_id UUID NOT NULL REFERENCES tenants(tenant_id),
  generation BIGINT NOT NULL,
  key_id TEXT NOT NULL,
  signed JSONB NOT NULL,
  published_by UUID,
  published_at_ms BIGINT NOT NULL,
  PRIMARY KEY (tenant_id, generation)
);
",
    ),
    (
        8,
        "cloud-v8: automations — tenant-scoped definitions and versions, hash-bound enable approvals, pauses, per-endpoint webhook secrets by derivation, the replay nonce table, idempotent firings and run history, the refused-delivery audit, the schedule cursor and the clock offset (PX-085)",
        r"
CREATE TABLE IF NOT EXISTS automations (
  automation_id UUID PRIMARY KEY,
  tenant_id UUID NOT NULL REFERENCES tenants(tenant_id),
  name TEXT NOT NULL,
  workspace_root TEXT NOT NULL DEFAULT '',
  repository TEXT NOT NULL DEFAULT '',
  service_principal_id UUID NOT NULL,
  current_version INTEGER NOT NULL,
  paused BOOLEAN NOT NULL DEFAULT FALSE,
  enabled_version INTEGER,
  enabled_hash TEXT,
  enabled_by UUID,
  enabled_at_ms BIGINT,
  enabled_effects TEXT,
  enabled_capabilities TEXT[] NOT NULL DEFAULT '{}',
  enabled_paths TEXT[] NOT NULL DEFAULT '{}',
  enabled_hosts TEXT[] NOT NULL DEFAULT '{}',
  created_by UUID,
  created_at_ms BIGINT NOT NULL,
  updated_at_ms BIGINT NOT NULL,
  UNIQUE (tenant_id, name)
);
CREATE INDEX IF NOT EXISTS automations_tenant ON automations(tenant_id, created_at_ms);
CREATE TABLE IF NOT EXISTS automation_versions (
  automation_id UUID NOT NULL REFERENCES automations(automation_id),
  version INTEGER NOT NULL,
  tenant_id UUID NOT NULL,
  definition_json TEXT NOT NULL,
  definition_hash TEXT NOT NULL,
  controls JSONB NOT NULL,
  created_by UUID,
  created_at_ms BIGINT NOT NULL,
  PRIMARY KEY (automation_id, version)
);
CREATE TABLE IF NOT EXISTS automation_switches (
  scope TEXT PRIMARY KEY,
  paused BOOLEAN NOT NULL,
  note TEXT NOT NULL DEFAULT '',
  by_actor TEXT NOT NULL DEFAULT '',
  at_ms BIGINT NOT NULL
);
CREATE TABLE IF NOT EXISTS automation_endpoints (
  endpoint_id TEXT PRIMARY KEY,
  tenant_id UUID NOT NULL REFERENCES tenants(tenant_id),
  automation_id UUID NOT NULL REFERENCES automations(automation_id),
  trigger_id TEXT NOT NULL,
  rotation INTEGER NOT NULL DEFAULT 0,
  created_by UUID,
  created_at_ms BIGINT NOT NULL,
  revoked_at_ms BIGINT
);
CREATE INDEX IF NOT EXISTS automation_endpoints_tenant ON automation_endpoints(tenant_id, automation_id);
CREATE TABLE IF NOT EXISTS automation_nonces (
  endpoint_id TEXT NOT NULL,
  nonce TEXT NOT NULL,
  signed_at_s BIGINT NOT NULL,
  received_at_ms BIGINT NOT NULL,
  PRIMARY KEY (endpoint_id, nonce)
);
CREATE INDEX IF NOT EXISTS automation_nonces_age ON automation_nonces(received_at_ms);
CREATE TABLE IF NOT EXISTS automation_firings (
  dispatch_key TEXT PRIMARY KEY,
  tenant_id UUID NOT NULL REFERENCES tenants(tenant_id),
  automation_id UUID NOT NULL REFERENCES automations(automation_id),
  version INTEGER NOT NULL,
  trigger_id TEXT NOT NULL,
  trigger_kind TEXT NOT NULL,
  event_id TEXT NOT NULL,
  source TEXT NOT NULL,
  status TEXT NOT NULL,
  reason TEXT NOT NULL DEFAULT '',
  detail TEXT NOT NULL DEFAULT '',
  task_id UUID,
  session_id UUID,
  principal_id UUID,
  definition_hash TEXT NOT NULL,
  fired_ms BIGINT NOT NULL,
  admitted_ms BIGINT,
  dispatched_ms BIGINT,
  slot_ms BIGINT,
  catch_up BOOLEAN NOT NULL DEFAULT FALSE,
  missed BIGINT NOT NULL DEFAULT 0,
  payload TEXT NOT NULL DEFAULT '',
  payload_label TEXT NOT NULL DEFAULT '',
  budgets JSONB NOT NULL DEFAULT '{}'
);
CREATE INDEX IF NOT EXISTS automation_firings_automation ON automation_firings(tenant_id, automation_id, fired_ms);
CREATE INDEX IF NOT EXISTS automation_firings_task ON automation_firings(task_id);
CREATE INDEX IF NOT EXISTS automation_firings_open ON automation_firings(status) WHERE status IN ('PENDING', 'QUEUED');
CREATE TABLE IF NOT EXISTS automation_audit (
  audit_id BIGSERIAL PRIMARY KEY,
  at_ms BIGINT NOT NULL,
  tenant_id UUID,
  endpoint_id TEXT,
  automation_id UUID,
  code TEXT NOT NULL,
  delivery_id TEXT NOT NULL DEFAULT '',
  detail TEXT NOT NULL DEFAULT ''
);
CREATE INDEX IF NOT EXISTS automation_audit_tenant ON automation_audit(tenant_id, audit_id);
CREATE TABLE IF NOT EXISTS automation_schedule (
  automation_id UUID NOT NULL REFERENCES automations(automation_id),
  trigger_id TEXT NOT NULL,
  tenant_id UUID NOT NULL,
  version INTEGER NOT NULL,
  anchor_ms BIGINT NOT NULL,
  cursor_ms BIGINT NOT NULL,
  next_due_ms BIGINT,
  PRIMARY KEY (automation_id, trigger_id)
);
CREATE INDEX IF NOT EXISTS automation_schedule_due ON automation_schedule(next_due_ms) WHERE next_due_ms IS NOT NULL;
CREATE TABLE IF NOT EXISTS automation_clock (
  id INTEGER PRIMARY KEY CHECK (id = 1),
  offset_ms BIGINT NOT NULL DEFAULT 0
);
INSERT INTO automation_clock (id, offset_ms) VALUES (1, 0) ON CONFLICT (id) DO NOTHING;
",
    ),
];
