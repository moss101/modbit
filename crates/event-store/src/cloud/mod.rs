//! The cloud mirror of the canonical log (M8.1, docs/24 "Postgres", "Object
//! storage", docs/31 "Cloud schema", docs/33 "Cloud API implementation"):
//! a tenant-scoped, append-only event log in Postgres whose projections
//! (sessions, tasks, approvals) are updated in the same transaction as the
//! append, a command ledger that makes every mutating command idempotent
//! by its id, generation-fenced session leases the Cloud Core Worker claims
//! with `SKIP LOCKED`, principals and rotating refresh tokens, and an
//! S3-compatible object store keyed per tenant by content hash.
//!
//! Every query scopes by `tenant_id` (docs/24 "Multi-tenancy"); a resource
//! of another tenant is not found, never dereferenced. Events keep the same
//! envelope, sequence and integrity-hash chain as the local store, so a
//! session mirrored from a local Core verifies here exactly as it does there.

pub mod schema;

use std::collections::HashMap;

use deadpool_postgres::{Manager, ManagerConfig, Pool, RecyclingMethod};
use modbit_domain::approval::{Approval, ApprovalEvent};
use modbit_domain::event::{Actor, AggregateType, EventEnvelope, PayloadRef};
use modbit_domain::session::{Session, SessionEvent};
use modbit_domain::task::{Task, TaskEvent, TaskState};
use modbit_domain::{ApprovalId, EventId, SessionId, TaskId, TenantId, Timestamp};
use sha2::{Digest, Sha256};
use tokio_postgres::types::ToSql;
use tokio_postgres::{NoTls, Transaction};

use crate::schema::INLINE_PAYLOAD_CEILING;
use crate::{AppendRequest, NewEvent};

/// Errors from the cloud store.
#[derive(Debug, thiserror::Error)]
pub enum CloudError {
    /// Postgres failure.
    #[error("postgres: {0}")]
    Postgres(#[from] tokio_postgres::Error),
    /// Pool failure.
    #[error("pool: {0}")]
    Pool(String),
    /// Object store failure.
    #[error("object store: {0}")]
    Object(String),
    /// JSON failure.
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
    /// The caller's expected sequence does not match the aggregate's current sequence.
    #[error("sequence conflict on {aggregate}: expected {expected}, actual {actual}")]
    SequenceConflict {
        /// Aggregate id (hex).
        aggregate: String,
        /// Expected.
        expected: u64,
        /// Actual.
        actual: u64,
    },
    /// The event is not a valid transition for its aggregate's projection.
    #[error("invalid transition: {0}")]
    InvalidTransition(String),
    /// The chain hash of a mirrored event does not continue the aggregate's chain.
    #[error("integrity: {0}")]
    Integrity(String),
    /// Not found in this tenant.
    #[error("not found: {0}")]
    NotFound(String),
    /// The writer's lease is not the session's current one (docs/33: a
    /// stale owner cannot advance state).
    #[error("stale lease on session {session}: presented {presented}, current {current}")]
    StaleLease {
        /// Session.
        session: String,
        /// Presented generation.
        presented: u64,
        /// Current generation (0 when unheld).
        current: u64,
    },
    /// The database holds a newer schema than this build writes.
    #[error("schema {found} is newer than this build's {supported}; refusing write mode")]
    SchemaTooNew {
        /// Found.
        found: i32,
        /// Supported.
        supported: i32,
    },
}

impl From<deadpool_postgres::PoolError> for CloudError {
    fn from(e: deadpool_postgres::PoolError) -> Self {
        Self::Pool(e.to_string())
    }
}

impl From<object_store::Error> for CloudError {
    fn from(e: object_store::Error) -> Self {
        Self::Object(e.to_string())
    }
}

/// Result alias.
pub type Result<T> = std::result::Result<T, CloudError>;

/// S3-compatible object storage configuration (docs/24: encrypted bucket,
/// tenant-scoped content-hashed keys, signed short-lived URLs).
#[derive(Clone, Debug)]
pub struct S3Config {
    /// Endpoint URL (`https://s3.amazonaws.com`, or a MinIO endpoint).
    pub endpoint: String,
    /// Bucket.
    pub bucket: String,
    /// Region.
    pub region: String,
    /// Access key id.
    pub access_key_id: String,
    /// Secret access key (memory only; never logged).
    pub secret_access_key: String,
    /// Allow a plain-HTTP endpoint (local MinIO only).
    pub allow_http: bool,
}

/// Store configuration.
#[derive(Clone, Debug)]
pub struct CloudStoreConfig {
    /// Postgres connection string.
    pub database_url: String,
    /// Object storage; `None` keeps objects in Postgres (development only).
    pub s3: Option<S3Config>,
}

/// A principal the API authenticated (docs/24 "Identity"): a user of a
/// tenant, a worker or a service.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Principal {
    /// Id.
    pub principal_id: uuid::Uuid,
    /// Tenant.
    pub tenant_id: TenantId,
    /// `user` | `worker` | `service`.
    pub kind: String,
    /// Label.
    pub label: String,
}

/// What an append produced.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Appended {
    /// Store-wide offset of the first event.
    pub first_offset: u64,
    /// Store-wide offset of the last event.
    pub last_offset: u64,
    /// The session's cursor after the append.
    pub session_offset: u64,
}

/// A recorded command outcome (idempotency, docs/33).
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct CommandRecord {
    /// Command.
    pub command_id: uuid::Uuid,
    /// Kind.
    pub kind: String,
    /// `ACCEPTED` | `REJECTED` | `PENDING` (relayed to the session's execution owner, M8.2).
    pub status: String,
    /// Result code.
    pub code: String,
    /// Result body.
    pub result: serde_json::Value,
    /// First event offset when accepted.
    pub first_offset: Option<u64>,
    /// Last event offset when accepted.
    pub last_offset: Option<u64>,
}

/// One event as the API serves it.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct CloudEvent {
    /// Store-wide offset.
    pub offset: u64,
    /// The session's cursor.
    pub session_offset: u64,
    /// Envelope.
    pub envelope: EventEnvelope,
    /// Payload (inline, or fetched from the object store when small enough).
    pub payload: serde_json::Value,
}

/// A sandbox the gateway provisioned (M8.3).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SandboxRecord {
    /// Id.
    pub sandbox_id: uuid::Uuid,
    /// Tenant.
    pub tenant_id: TenantId,
    /// Session.
    pub session_id: SessionId,
    /// Task.
    pub task_id: TaskId,
    /// The worker that holds it.
    pub worker_id: String,
    /// The session lease generation it was issued under.
    pub lease_generation: u64,
    /// `microvm` | `reference`.
    pub backend: String,
    /// Whether the backend isolates.
    pub isolated: bool,
    /// `READY` | `DESTROYED` | `LOST`.
    pub state: String,
    /// Backend detail.
    pub detail: String,
}

/// A session lease a worker claimed (docs/33 "Cloud worker lifecycle").
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClaimedLease {
    /// Session.
    pub session_id: SessionId,
    /// Tenant.
    pub tenant_id: TenantId,
    /// Fencing generation (strictly greater than the previous holder's).
    pub generation: u64,
    /// Expiry (ms since the epoch).
    pub expires_at_ms: i64,
}

/// Object metadata.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ObjectMeta {
    /// Content hash (hex sha256).
    pub hash: String,
    /// Length.
    pub byte_length: u64,
    /// Media type.
    pub mime: String,
}

fn now_ms() -> i64 {
    Timestamp::now().millis()
}

fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

fn uuid_of(bytes: &[u8; 16]) -> uuid::Uuid {
    uuid::Uuid::from_bytes(*bytes)
}

fn tenant_uuid(t: TenantId) -> uuid::Uuid {
    uuid_of(t.as_bytes())
}

/// The cloud store.
pub struct CloudStore {
    pool: Pool,
    objects: Option<(std::sync::Arc<dyn object_store::ObjectStore>, S3Config)>,
}

impl std::fmt::Debug for CloudStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CloudStore")
            .field(
                "objects",
                &self.objects.as_ref().map(|(_, c)| c.bucket.clone()),
            )
            .finish_non_exhaustive()
    }
}

impl CloudStore {
    /// Connect, and apply the migrations this build knows (forward-only;
    /// a newer schema refuses write mode).
    pub async fn connect(cfg: &CloudStoreConfig) -> Result<Self> {
        let pg: tokio_postgres::Config = cfg
            .database_url
            .parse()
            .map_err(|e: tokio_postgres::Error| CloudError::Pool(e.to_string()))?;
        let mgr = Manager::from_config(
            pg,
            NoTls,
            ManagerConfig {
                recycling_method: RecyclingMethod::Fast,
            },
        );
        let pool = Pool::builder(mgr)
            .max_size(16)
            .build()
            .map_err(|e| CloudError::Pool(e.to_string()))?;
        let objects = match &cfg.s3 {
            Some(s3) => {
                let store = object_store::aws::AmazonS3Builder::new()
                    .with_endpoint(&s3.endpoint)
                    .with_bucket_name(&s3.bucket)
                    .with_region(&s3.region)
                    .with_access_key_id(&s3.access_key_id)
                    .with_secret_access_key(&s3.secret_access_key)
                    .with_allow_http(s3.allow_http)
                    .with_virtual_hosted_style_request(false)
                    .build()?;
                let store: std::sync::Arc<dyn object_store::ObjectStore> =
                    std::sync::Arc::new(store);
                Some((store, s3.clone()))
            }
            None => None,
        };
        let this = Self { pool, objects };
        this.migrate().await?;
        Ok(this)
    }

    /// Apply pending migrations; returns the versions applied. One
    /// transaction under an advisory lock, so many instances starting at
    /// once (or many tests) apply the schema exactly once between them.
    pub async fn migrate(&self) -> Result<Vec<i32>> {
        let mut client = self.pool.get().await?;
        let tx = client.transaction().await?;
        tx.execute("SELECT pg_advisory_xact_lock(726482101)", &[])
            .await?;
        tx.batch_execute(
            "CREATE TABLE IF NOT EXISTS cloud_schema (version INTEGER PRIMARY KEY, name TEXT NOT NULL, checksum TEXT NOT NULL, applied_at_ms BIGINT NOT NULL)",
        )
        .await?;
        let found: i32 = tx
            .query_one("SELECT COALESCE(MAX(version), 0) FROM cloud_schema", &[])
            .await?
            .get(0);
        if found > schema::CLOUD_SCHEMA_VERSION {
            return Err(CloudError::SchemaTooNew {
                found,
                supported: schema::CLOUD_SCHEMA_VERSION,
            });
        }
        let mut applied = Vec::new();
        for (version, name, sql) in schema::MIGRATIONS {
            let checksum = sha256_hex(sql.as_bytes());
            let row = tx
                .query_opt(
                    "SELECT checksum FROM cloud_schema WHERE version = $1",
                    &[version],
                )
                .await?;
            if let Some(r) = row {
                let recorded: String = r.get(0);
                if recorded != checksum {
                    return Err(CloudError::Integrity(format!(
                        "migration {version} was applied with another checksum"
                    )));
                }
                continue;
            }
            tx.batch_execute(sql).await?;
            tx.execute(
                "INSERT INTO cloud_schema (version, name, checksum, applied_at_ms) VALUES ($1, $2, $3, $4)",
                &[version, name, &checksum, &now_ms()],
            )
            .await?;
            applied.push(*version);
        }
        tx.commit().await?;
        Ok(applied)
    }

    // ---- tenants and principals ----------------------------------------

    /// Create a tenant.
    pub async fn create_tenant(&self, name: &str) -> Result<TenantId> {
        let id = TenantId::new();
        let client = self.pool.get().await?;
        client
            .execute(
                "INSERT INTO tenants (tenant_id, name, created_at_ms) VALUES ($1, $2, $3)",
                &[&tenant_uuid(id), &name, &now_ms()],
            )
            .await?;
        Ok(id)
    }

    /// Create a principal; the secret is returned once and stored hashed.
    pub async fn create_principal(
        &self,
        tenant: TenantId,
        kind: &str,
        label: &str,
    ) -> Result<(Principal, String)> {
        let secret_bytes: [u8; 32] = rand::random();
        let secret = format!("mbs_{}", hex::encode(secret_bytes));
        let id = uuid::Uuid::now_v7();
        let client = self.pool.get().await?;
        client
            .execute(
                "INSERT INTO principals (principal_id, tenant_id, kind, label, secret_hash, created_at_ms) VALUES ($1, $2, $3, $4, $5, $6)",
                &[&id, &tenant_uuid(tenant), &kind, &label, &sha256_hex(secret.as_bytes()), &now_ms()],
            )
            .await?;
        Ok((
            Principal {
                principal_id: id,
                tenant_id: tenant,
                kind: kind.to_owned(),
                label: label.to_owned(),
            },
            secret,
        ))
    }

    /// The principal a secret names, unless revoked.
    pub async fn principal_by_secret(&self, secret: &str) -> Result<Option<Principal>> {
        let client = self.pool.get().await?;
        let row = client
            .query_opt(
                "SELECT principal_id, tenant_id, kind, label FROM principals WHERE secret_hash = $1 AND revoked_at_ms IS NULL",
                &[&sha256_hex(secret.as_bytes())],
            )
            .await?;
        Ok(row.map(|r| Principal {
            principal_id: r.get(0),
            tenant_id: TenantId::from_bytes(*r.get::<_, uuid::Uuid>(1).as_bytes()),
            kind: r.get(2),
            label: r.get(3),
        }))
    }

    /// The principal by id, within its tenant.
    pub async fn principal(&self, tenant: TenantId, id: uuid::Uuid) -> Result<Option<Principal>> {
        let client = self.pool.get().await?;
        let row = client
            .query_opt(
                "SELECT principal_id, tenant_id, kind, label FROM principals WHERE principal_id = $1 AND tenant_id = $2 AND revoked_at_ms IS NULL",
                &[&id, &tenant_uuid(tenant)],
            )
            .await?;
        Ok(row.map(|r| Principal {
            principal_id: r.get(0),
            tenant_id: TenantId::from_bytes(*r.get::<_, uuid::Uuid>(1).as_bytes()),
            kind: r.get(2),
            label: r.get(3),
        }))
    }

    /// Issue a refresh token (returned once; stored hashed).
    pub async fn issue_refresh_token(&self, p: &Principal, ttl_ms: i64) -> Result<String> {
        let bytes: [u8; 32] = rand::random();
        let token = format!("mbr_{}", hex::encode(bytes));
        let client = self.pool.get().await?;
        let now = now_ms();
        client
            .execute(
                "INSERT INTO refresh_tokens (token_hash, tenant_id, principal_id, issued_at_ms, expires_at_ms) VALUES ($1, $2, $3, $4, $5)",
                &[&sha256_hex(token.as_bytes()), &tenant_uuid(p.tenant_id), &p.principal_id, &now, &(now + ttl_ms)],
            )
            .await?;
        Ok(token)
    }

    /// Rotate a refresh token: the old one is spent (a second use is a
    /// replay and revokes the family), the new one is returned once.
    pub async fn rotate_refresh_token(
        &self,
        token: &str,
        ttl_ms: i64,
    ) -> Result<Option<(Principal, String)>> {
        let mut client = self.pool.get().await?;
        let tx = client.transaction().await?;
        let hash = sha256_hex(token.as_bytes());
        let row = tx
            .query_opt(
                "SELECT t.tenant_id, t.principal_id, t.expires_at_ms, t.rotated_to, t.revoked_at_ms, p.kind, p.label FROM refresh_tokens t JOIN principals p ON p.principal_id = t.principal_id WHERE t.token_hash = $1 FOR UPDATE",
                &[&hash],
            )
            .await?;
        let Some(r) = row else {
            tx.commit().await?;
            return Ok(None);
        };
        let expires: i64 = r.get(2);
        let rotated_to: Option<String> = r.get(3);
        let revoked: Option<i64> = r.get(4);
        let now = now_ms();
        if revoked.is_some() || expires < now {
            tx.commit().await?;
            return Ok(None);
        }
        let principal_id: uuid::Uuid = r.get(1);
        if let Some(next) = rotated_to {
            // Replay of a spent token: the whole family is revoked.
            tx.execute(
                "UPDATE refresh_tokens SET revoked_at_ms = $1 WHERE principal_id = $2 AND revoked_at_ms IS NULL",
                &[&now, &principal_id],
            )
            .await?;
            let _ = next;
            tx.commit().await?;
            return Ok(None);
        }
        let p = Principal {
            principal_id,
            tenant_id: TenantId::from_bytes(*r.get::<_, uuid::Uuid>(0).as_bytes()),
            kind: r.get(5),
            label: r.get(6),
        };
        let bytes: [u8; 32] = rand::random();
        let fresh = format!("mbr_{}", hex::encode(bytes));
        let fresh_hash = sha256_hex(fresh.as_bytes());
        tx.execute(
            "INSERT INTO refresh_tokens (token_hash, tenant_id, principal_id, issued_at_ms, expires_at_ms) VALUES ($1, $2, $3, $4, $5)",
            &[&fresh_hash, &tenant_uuid(p.tenant_id), &p.principal_id, &now, &(now + ttl_ms)],
        )
        .await?;
        tx.execute(
            "UPDATE refresh_tokens SET rotated_to = $1 WHERE token_hash = $2",
            &[&fresh_hash, &hash],
        )
        .await?;
        tx.commit().await?;
        Ok(Some((p, fresh)))
    }

    // ---- commands --------------------------------------------------------

    /// A recorded command outcome, within the tenant.
    pub async fn command(
        &self,
        tenant: TenantId,
        command_id: uuid::Uuid,
    ) -> Result<Option<CommandRecord>> {
        let client = self.pool.get().await?;
        let row = client
            .query_opt(
                "SELECT command_id, kind, status, code, result, first_offset, last_offset FROM commands WHERE command_id = $1 AND tenant_id = $2",
                &[&command_id, &tenant_uuid(tenant)],
            )
            .await?;
        Ok(row.map(|r| CommandRecord {
            command_id: r.get(0),
            kind: r.get(1),
            status: r.get(2),
            code: r.get(3),
            result: r.get(4),
            first_offset: r.get::<_, Option<i64>>(5).map(|v| v as u64),
            last_offset: r.get::<_, Option<i64>>(6).map(|v| v as u64),
        }))
    }

    /// Relay a command to the session's execution owner (M8.2): recorded
    /// `PENDING` with its body; the worker holding the lease executes it on
    /// its Core and completes it. Returns `false` when the id was recorded already.
    pub async fn enqueue_command(
        &self,
        tenant: TenantId,
        session: SessionId,
        command_id: uuid::Uuid,
        kind: &str,
        body: serde_json::Value,
    ) -> Result<bool> {
        let client = self.pool.get().await?;
        let pending =
            serde_json::json!({"status": "PENDING", "command_id": command_id.to_string()});
        let n = client
            .execute(
                "INSERT INTO commands (command_id, tenant_id, session_id, kind, status, code, result, body, recorded_at_ms) VALUES ($1, $2, $3, $4, 'PENDING', 'PENDING', $5, $6, $7) ON CONFLICT (command_id) DO NOTHING",
                &[&command_id, &tenant_uuid(tenant), &uuid_of(session.as_bytes()), &kind, &pending, &body, &now_ms()],
            )
            .await?;
        Ok(n == 1)
    }

    /// The pending commands of a session, oldest first: `(command_id, kind, body)`.
    pub async fn pending_commands(
        &self,
        tenant: TenantId,
        session: SessionId,
        limit: u32,
    ) -> Result<Vec<(uuid::Uuid, String, serde_json::Value)>> {
        let client = self.pool.get().await?;
        let rows = client
            .query(
                "SELECT command_id, kind, body FROM commands WHERE tenant_id = $1 AND session_id = $2 AND status = 'PENDING' ORDER BY recorded_at_ms ASC, command_id ASC LIMIT $3",
                &[&tenant_uuid(tenant), &uuid_of(session.as_bytes()), &i64::from(limit.clamp(1, 500))],
            )
            .await?;
        Ok(rows
            .into_iter()
            .map(|r| {
                (
                    r.get(0),
                    r.get(1),
                    r.get::<_, Option<serde_json::Value>>(2)
                        .unwrap_or(serde_json::Value::Null),
                )
            })
            .collect())
    }

    /// Complete a relayed command with the owner's outcome.
    pub async fn complete_command(
        &self,
        tenant: TenantId,
        command_id: uuid::Uuid,
        status: &str,
        code: &str,
        result: serde_json::Value,
    ) -> Result<bool> {
        let client = self.pool.get().await?;
        let n = client
            .execute(
                "UPDATE commands SET status = $1, code = $2, result = $3, completed_at_ms = $4 WHERE command_id = $5 AND tenant_id = $6 AND status = 'PENDING'",
                &[&status, &code, &result, &now_ms(), &command_id, &tenant_uuid(tenant)],
            )
            .await?;
        Ok(n == 1)
    }

    /// Whether a live lease holds the session: `(worker, generation)`.
    pub async fn held_by(
        &self,
        tenant: TenantId,
        session: SessionId,
    ) -> Result<Option<(String, u64)>> {
        Ok(self
            .lease(tenant, session)
            .await?
            .and_then(|(worker, generation, expires, _)| {
                if expires > now_ms() {
                    worker.map(|w| (w, generation))
                } else {
                    None
                }
            }))
    }

    /// Mirror events the session's execution owner recorded in its local
    /// Core (M8.2): verbatim envelopes, each continuing its aggregate's
    /// chain here (or the batch is refused), under the owner's lease
    /// generation checked in the same transaction — a fenced owner writes
    /// nothing. Already-present event ids are skipped (a re-sent batch is
    /// harmless). Projections and the session cursor move as for an append.
    pub async fn mirror(
        &self,
        tenant: TenantId,
        session: SessionId,
        worker_id: &str,
        generation: u64,
        events: Vec<(EventEnvelope, serde_json::Value)>,
    ) -> Result<Appended> {
        let mut client = self.pool.get().await?;
        let tx = client.transaction().await?;
        let tenant_u = tenant_uuid(tenant);
        let session_u = uuid_of(session.as_bytes());
        let lease = tx
            .query_opt(
                "SELECT worker_id, generation, expires_at_ms FROM session_leases WHERE session_id = $1 AND tenant_id = $2 FOR UPDATE",
                &[&session_u, &tenant_u],
            )
            .await?;
        let current = lease.as_ref().map_or(0, |r| r.get::<_, i64>(1) as u64);
        let live = lease.as_ref().is_some_and(|r| {
            r.get::<_, Option<String>>(0).as_deref() == Some(worker_id)
                && r.get::<_, i64>(1) as u64 == generation
                && r.get::<_, i64>(2) > now_ms()
        });
        if !live {
            return Err(CloudError::StaleLease {
                session: session.to_string(),
                presented: generation,
                current,
            });
        }
        let cursor = tx
            .query_opt(
                "SELECT last_session_offset FROM sessions WHERE session_id = $1 AND tenant_id = $2 FOR UPDATE",
                &[&session_u, &tenant_u],
            )
            .await?;
        let mut session_offset: i64 = cursor.map_or(0, |r| r.get(0));
        let mut first = None;
        let mut last = 0i64;
        let mut stored = Vec::new();
        for (env, payload) in events {
            if env.tenant_id != tenant || env.session_id != session {
                return Err(CloudError::Integrity(format!(
                    "event {} is not of this session",
                    env.event_id
                )));
            }
            let present = tx
                .query_opt(
                    "SELECT 1 FROM events WHERE (envelope->>'event_id') = $1",
                    &[&env.event_id.to_string()],
                )
                .await?;
            if present.is_some() {
                continue;
            }
            let (head, previous) = aggregate_head(&tx, &env.aggregate_id).await?;
            if env.sequence != head + 1 {
                return Err(CloudError::SequenceConflict {
                    aggregate: hex::encode(env.aggregate_id),
                    expected: head + 1,
                    actual: env.sequence,
                });
            }
            if crate::store::chain_hash(&previous, &env) != env.integrity_hash {
                return Err(CloudError::Integrity(format!(
                    "mirrored event {} (sequence {}) does not continue its aggregate's chain",
                    env.event_id, env.sequence
                )));
            }
            if let PayloadRef::Object { object_hash, .. } = &env.payload {
                let bytes = payload.to_string();
                let hash = self
                    .put_object_tx(&tx, tenant, bytes.as_bytes(), "application/json")
                    .await?;
                if &hash != object_hash {
                    return Err(CloudError::Integrity(format!(
                        "payload of {} does not match its object hash",
                        env.event_id
                    )));
                }
            }
            session_offset += 1;
            let offset = insert_event(&tx, &env, session_offset, &payload).await?;
            first.get_or_insert(offset);
            last = offset;
            stored.push((env, payload));
        }
        for (env, payload) in &stored {
            project(&tx, env, payload).await?;
        }
        tx.execute(
            "UPDATE sessions SET last_session_offset = $1, updated_at_ms = $2 WHERE session_id = $3",
            &[&session_offset, &now_ms(), &session_u],
        )
        .await?;
        if !stored.is_empty() {
            let note = serde_json::json!({"session_id": session.to_string(), "session_offset": session_offset}).to_string();
            tx.execute("SELECT pg_notify('modbit_events', $1)", &[&note])
                .await?;
        }
        tx.commit().await?;
        Ok(Appended {
            first_offset: first.unwrap_or(0) as u64,
            last_offset: last as u64,
            session_offset: session_offset as u64,
        })
    }

    /// Import a handoff's log into this tenant (M8.7, docs/21 "Handoff
    /// local → cloud"): the envelopes verbatim — their ids, sequences,
    /// integrity hashes and the origin tenant inside them — each continuing
    /// its aggregate's chain here, stored and projected under `tenant`. The
    /// session must be new to this tenant or already the same log (an event
    /// already present is skipped; a fork is refused). No lease is needed:
    /// nobody holds the session yet.
    pub async fn import_handoff(
        &self,
        tenant: TenantId,
        session: SessionId,
        events: Vec<(EventEnvelope, serde_json::Value)>,
    ) -> Result<Appended> {
        let mut client = self.pool.get().await?;
        let tx = client.transaction().await?;
        let tenant_u = tenant_uuid(tenant);
        let session_u = uuid_of(session.as_bytes());
        let other = tx
            .query_opt(
                "SELECT tenant_id FROM sessions WHERE session_id = $1 AND tenant_id <> $2",
                &[&session_u, &tenant_u],
            )
            .await?;
        if other.is_some() {
            return Err(CloudError::NotFound(format!("session {session}")));
        }
        let cursor = tx
            .query_opt(
                "SELECT last_session_offset FROM sessions WHERE session_id = $1 AND tenant_id = $2 FOR UPDATE",
                &[&session_u, &tenant_u],
            )
            .await?;
        let mut session_offset: i64 = cursor.map_or(0, |r| r.get(0));
        let mut first = None;
        let mut last = 0i64;
        let mut stored = Vec::new();
        for (env, payload) in events {
            if env.session_id != session {
                return Err(CloudError::Integrity(format!(
                    "event {} is not of this session",
                    env.event_id
                )));
            }
            let present = tx
                .query_opt(
                    "SELECT 1 FROM events WHERE (envelope->>'event_id') = $1",
                    &[&env.event_id.to_string()],
                )
                .await?;
            if present.is_some() {
                continue;
            }
            let (head, previous) = aggregate_head(&tx, &env.aggregate_id).await?;
            if env.sequence != head + 1 {
                return Err(CloudError::SequenceConflict {
                    aggregate: hex::encode(env.aggregate_id),
                    expected: head + 1,
                    actual: env.sequence,
                });
            }
            if crate::store::chain_hash(&previous, &env) != env.integrity_hash {
                return Err(CloudError::Integrity(format!(
                    "handoff event {} (sequence {}) does not continue its aggregate's chain",
                    env.event_id, env.sequence
                )));
            }
            if let PayloadRef::Object { object_hash, .. } = &env.payload {
                let bytes = payload.to_string();
                let hash = self
                    .put_object_tx(&tx, tenant, bytes.as_bytes(), "application/json")
                    .await?;
                if &hash != object_hash {
                    return Err(CloudError::Integrity(format!(
                        "payload of {} does not match its object hash",
                        env.event_id
                    )));
                }
            }
            session_offset += 1;
            let offset = insert_event_as(&tx, tenant, &env, session_offset, &payload).await?;
            first.get_or_insert(offset);
            last = offset;
            stored.push((env, payload));
        }
        for (env, payload) in &stored {
            project_as(&tx, tenant, env, payload).await?;
        }
        tx.execute(
            "UPDATE sessions SET last_session_offset = $1, updated_at_ms = $2 WHERE session_id = $3",
            &[&session_offset, &now_ms(), &session_u],
        )
        .await?;
        if !stored.is_empty() {
            let note = serde_json::json!({"session_id": session.to_string(), "session_offset": session_offset}).to_string();
            tx.execute("SELECT pg_notify('modbit_events', $1)", &[&note])
                .await?;
        }
        tx.commit().await?;
        Ok(Appended {
            first_offset: first.unwrap_or(0) as u64,
            last_offset: last as u64,
            session_offset: session_offset as u64,
        })
    }

    /// Record a rejected command (so a retry answers the same way).
    pub async fn record_rejection(
        &self,
        tenant: TenantId,
        command_id: uuid::Uuid,
        kind: &str,
        code: &str,
        result: serde_json::Value,
    ) -> Result<()> {
        let client = self.pool.get().await?;
        client
            .execute(
                "INSERT INTO commands (command_id, tenant_id, kind, status, code, result, recorded_at_ms) VALUES ($1, $2, $3, 'REJECTED', $4, $5, $6) ON CONFLICT (command_id) DO NOTHING",
                &[&command_id, &tenant_uuid(tenant), &kind, &code, &result, &now_ms()],
            )
            .await?;
        Ok(())
    }

    // ---- the log ---------------------------------------------------------

    /// Append events to the tenant's log in one transaction with their
    /// projections and, when a command is named, its ledger row. The
    /// envelopes are built here (sequence, hash chain); `mirror` appends
    /// envelopes a local Core already built.
    pub async fn append(
        &self,
        req: AppendRequest,
        command: Option<(uuid::Uuid, &str, serde_json::Value)>,
    ) -> Result<Appended> {
        let mut client = self.pool.get().await?;
        let tx = client.transaction().await?;
        let tenant = tenant_uuid(req.tenant_id);
        let session = uuid_of(req.session_id.as_bytes());
        // The session row is the append's anchor: locked for the cursor.
        let cursor_row = tx
            .query_opt(
                "SELECT last_session_offset FROM sessions WHERE session_id = $1 AND tenant_id = $2 FOR UPDATE",
                &[&session, &tenant],
            )
            .await?;
        let mut session_offset: i64 = match cursor_row {
            Some(r) => r.get(0),
            None => {
                let creates = req.aggregate_type == AggregateType::Session
                    && req
                        .events
                        .first()
                        .is_some_and(|e| e.event_type == "SessionCreated");
                if !creates {
                    return Err(CloudError::NotFound(format!("session {}", req.session_id)));
                }
                0
            }
        };
        // The aggregate's current sequence and hash.
        let (mut sequence, mut previous) = aggregate_head(&tx, &req.aggregate_id).await?;
        if let Some(expected) = req.expected_sequence
            && expected != sequence
        {
            return Err(CloudError::SequenceConflict {
                aggregate: hex::encode(req.aggregate_id),
                expected,
                actual: sequence,
            });
        }
        let mut first = None;
        let mut last = 0i64;
        let mut stored = Vec::new();
        for ev in req.events {
            sequence += 1;
            session_offset += 1;
            let payload_text = ev.payload.to_string();
            let payload_ref = if payload_text.len() > INLINE_PAYLOAD_CEILING {
                let hash = self
                    .put_object_tx(
                        &tx,
                        req.tenant_id,
                        payload_text.as_bytes(),
                        "application/json",
                    )
                    .await?;
                PayloadRef::Object {
                    object_hash: hash,
                    byte_length: payload_text.len() as u64,
                }
            } else {
                PayloadRef::Inline {
                    payload: ev.payload.clone(),
                }
            };
            let mut env = EventEnvelope {
                event_id: EventId::new(),
                tenant_id: req.tenant_id,
                session_id: req.session_id,
                task_id: req.task_id,
                run_id: req.run_id,
                turn_id: req.turn_id,
                step_id: req.step_id,
                aggregate_type: req.aggregate_type,
                aggregate_id: req.aggregate_id,
                sequence,
                event_type: ev.event_type.clone(),
                schema_version: 1,
                occurred_at: ev.occurred_at.unwrap_or_else(Timestamp::now),
                actor: ev.actor.clone(),
                causation_id: ev.causation_id,
                correlation_id: ev.correlation_id,
                payload: payload_ref,
                integrity_hash: String::new(),
            };
            env.integrity_hash = crate::store::chain_hash(&previous, &env);
            previous = env.integrity_hash.clone();
            let offset = insert_event(&tx, &env, session_offset, &ev.payload).await?;
            first.get_or_insert(offset);
            last = offset;
            stored.push((env, ev.payload));
        }
        // Projections in the same transaction.
        for (env, payload) in &stored {
            project(&tx, env, payload).await?;
        }
        tx.execute(
            "INSERT INTO sessions (session_id, tenant_id, state, generation, last_session_offset, doc, created_at_ms, updated_at_ms) VALUES ($1, $2, 'ACTIVE', 0, $3, '{}'::jsonb, $4, $4) ON CONFLICT (session_id) DO UPDATE SET last_session_offset = EXCLUDED.last_session_offset, updated_at_ms = EXCLUDED.updated_at_ms",
            &[&session, &tenant, &session_offset, &now_ms()],
        )
        .await?;
        if let Some((command_id, kind, result)) = command {
            tx.execute(
                "INSERT INTO commands (command_id, tenant_id, kind, status, code, result, first_offset, last_offset, recorded_at_ms) VALUES ($1, $2, $3, 'ACCEPTED', 'OK', $4, $5, $6, $7)",
                &[&command_id, &tenant, &kind, &result, &first.unwrap_or(0), &last, &now_ms()],
            )
            .await?;
        }
        // Wake the streams (docs/24: the API streams committed events).
        let note = serde_json::json!({"session_id": req.session_id.to_string(), "session_offset": session_offset}).to_string();
        tx.execute("SELECT pg_notify('modbit_events', $1)", &[&note])
            .await?;
        tx.commit().await?;
        Ok(Appended {
            first_offset: first.unwrap_or(0) as u64,
            last_offset: last as u64,
            session_offset: session_offset as u64,
        })
    }

    /// Events of a session after a cursor (the session's own offsets), oldest first.
    pub async fn events_after(
        &self,
        tenant: TenantId,
        session: SessionId,
        after_session_offset: u64,
        limit: u32,
    ) -> Result<Vec<CloudEvent>> {
        let client = self.pool.get().await?;
        let rows = client
            .query(
                "SELECT event_offset, session_offset, envelope, payload, payload_ref FROM events WHERE tenant_id = $1 AND session_id = $2 AND session_offset > $3 ORDER BY session_offset ASC LIMIT $4",
                &[&tenant_uuid(tenant), &uuid_of(session.as_bytes()), &(after_session_offset as i64), &(i64::from(limit.clamp(1, 1000)))],
            )
            .await?;
        let mut out = Vec::with_capacity(rows.len());
        for r in rows {
            let envelope: EventEnvelope = serde_json::from_value(r.get::<_, serde_json::Value>(2))?;
            let payload: Option<serde_json::Value> = r.get(3);
            let payload_ref: Option<String> = r.get(4);
            let payload = match (payload, payload_ref) {
                (Some(p), _) => p,
                (None, Some(hash)) => {
                    let bytes = self.get_object(tenant, &hash).await?;
                    serde_json::from_slice(&bytes)?
                }
                (None, None) => serde_json::Value::Null,
            };
            out.push(CloudEvent {
                offset: r.get::<_, i64>(0) as u64,
                session_offset: r.get::<_, i64>(1) as u64,
                envelope,
                payload,
            });
        }
        Ok(out)
    }

    // ---- projections -----------------------------------------------------

    /// The session, within the tenant.
    pub async fn session(&self, tenant: TenantId, id: SessionId) -> Result<Option<(Session, u64)>> {
        let client = self.pool.get().await?;
        let row = client
            .query_opt(
                "SELECT doc, last_session_offset FROM sessions WHERE session_id = $1 AND tenant_id = $2 AND doc <> '{}'::jsonb",
                &[&uuid_of(id.as_bytes()), &tenant_uuid(tenant)],
            )
            .await?;
        match row {
            Some(r) => Ok(Some((
                serde_json::from_value(r.get::<_, serde_json::Value>(0))?,
                r.get::<_, i64>(1) as u64,
            ))),
            None => Ok(None),
        }
    }

    /// The task, within the tenant.
    pub async fn task(&self, tenant: TenantId, id: TaskId) -> Result<Option<Task>> {
        let client = self.pool.get().await?;
        let row = client
            .query_opt(
                "SELECT doc FROM tasks WHERE task_id = $1 AND tenant_id = $2",
                &[&uuid_of(id.as_bytes()), &tenant_uuid(tenant)],
            )
            .await?;
        match row {
            Some(r) => Ok(Some(serde_json::from_value(
                r.get::<_, serde_json::Value>(0),
            )?)),
            None => Ok(None),
        }
    }

    /// The tasks of a session, oldest first.
    pub async fn tasks_of(&self, tenant: TenantId, session: SessionId) -> Result<Vec<Task>> {
        let client = self.pool.get().await?;
        let rows = client
            .query(
                "SELECT doc FROM tasks WHERE session_id = $1 AND tenant_id = $2 ORDER BY created_at_ms ASC",
                &[&uuid_of(session.as_bytes()), &tenant_uuid(tenant)],
            )
            .await?;
        rows.into_iter()
            .map(|r| Ok(serde_json::from_value(r.get::<_, serde_json::Value>(0))?))
            .collect()
    }

    /// The approval, within the tenant, with its session.
    pub async fn approval(
        &self,
        tenant: TenantId,
        id: ApprovalId,
    ) -> Result<Option<(Approval, SessionId)>> {
        let client = self.pool.get().await?;
        let row = client
            .query_opt(
                "SELECT doc, session_id FROM approvals WHERE approval_id = $1 AND tenant_id = $2",
                &[&uuid_of(id.as_bytes()), &tenant_uuid(tenant)],
            )
            .await?;
        match row {
            Some(r) => Ok(Some((
                serde_json::from_value(r.get::<_, serde_json::Value>(0))?,
                SessionId::from_bytes(*r.get::<_, uuid::Uuid>(1).as_bytes()),
            ))),
            None => Ok(None),
        }
    }

    // ---- leases (docs/33 "Cloud worker lifecycle") -----------------------

    /// Mark a session ready for a worker (work is waiting on it).
    pub async fn mark_ready(&self, tenant: TenantId, session: SessionId) -> Result<()> {
        let client = self.pool.get().await?;
        client
            .execute(
                "INSERT INTO session_leases (session_id, tenant_id, ready) VALUES ($1, $2, TRUE) ON CONFLICT (session_id) DO UPDATE SET ready = TRUE",
                &[&uuid_of(session.as_bytes()), &tenant_uuid(tenant)],
            )
            .await?;
        Ok(())
    }

    /// Claim one ready session whose lease is free or expired: the
    /// generation strictly increases (the previous holder is fenced).
    /// `SKIP LOCKED` lets many workers claim concurrently without a queue;
    /// `except` names sessions this worker will not take (ones it is still
    /// winding down).
    pub async fn claim_ready_session(
        &self,
        worker_id: &str,
        ttl_ms: i64,
        except: &[SessionId],
    ) -> Result<Option<ClaimedLease>> {
        let mut client = self.pool.get().await?;
        let tx = client.transaction().await?;
        let now = now_ms();
        let except: Vec<uuid::Uuid> = except.iter().map(|s| uuid_of(s.as_bytes())).collect();
        let row = tx
            .query_opt(
                "SELECT session_id, tenant_id, generation FROM session_leases WHERE ready = TRUE AND expires_at_ms < $1 AND NOT (session_id = ANY($2)) ORDER BY expires_at_ms ASC LIMIT 1 FOR UPDATE SKIP LOCKED",
                &[&now, &except],
            )
            .await?;
        let Some(r) = row else {
            tx.commit().await?;
            return Ok(None);
        };
        let session: uuid::Uuid = r.get(0);
        let tenant: uuid::Uuid = r.get(1);
        let generation: i64 = r.get::<_, i64>(2) + 1;
        tx.execute(
            "UPDATE session_leases SET worker_id = $1, generation = $2, claimed_at_ms = $3, heartbeat_at_ms = $3, expires_at_ms = $4 WHERE session_id = $5",
            &[&worker_id, &generation, &now, &(now + ttl_ms), &session],
        )
        .await?;
        tx.commit().await?;
        Ok(Some(ClaimedLease {
            session_id: SessionId::from_bytes(*session.as_bytes()),
            tenant_id: TenantId::from_bytes(*tenant.as_bytes()),
            generation: generation as u64,
            expires_at_ms: now + ttl_ms,
        }))
    }

    /// Claim one specific ready session when its lease is free or expired
    /// (a worker resuming a session it knows); `None` when held or not ready.
    pub async fn claim_session(
        &self,
        session: SessionId,
        worker_id: &str,
        ttl_ms: i64,
    ) -> Result<Option<ClaimedLease>> {
        let mut client = self.pool.get().await?;
        let tx = client.transaction().await?;
        let now = now_ms();
        let row = tx
            .query_opt(
                "SELECT session_id, tenant_id, generation FROM session_leases WHERE session_id = $1 AND ready = TRUE AND expires_at_ms < $2 FOR UPDATE SKIP LOCKED",
                &[&uuid_of(session.as_bytes()), &now],
            )
            .await?;
        let Some(r) = row else {
            tx.commit().await?;
            return Ok(None);
        };
        let tenant: uuid::Uuid = r.get(1);
        let generation: i64 = r.get::<_, i64>(2) + 1;
        tx.execute(
            "UPDATE session_leases SET worker_id = $1, generation = $2, claimed_at_ms = $3, heartbeat_at_ms = $3, expires_at_ms = $4 WHERE session_id = $5",
            &[&worker_id, &generation, &now, &(now + ttl_ms), &uuid_of(session.as_bytes())],
        )
        .await?;
        tx.commit().await?;
        Ok(Some(ClaimedLease {
            session_id: session,
            tenant_id: TenantId::from_bytes(*tenant.as_bytes()),
            generation: generation as u64,
            expires_at_ms: now + ttl_ms,
        }))
    }

    /// Renew a held lease; `false` when the holder was fenced (another
    /// generation holds it) or the lease expired and was taken.
    pub async fn renew_lease(
        &self,
        session: SessionId,
        worker_id: &str,
        generation: u64,
        ttl_ms: i64,
    ) -> Result<bool> {
        let client = self.pool.get().await?;
        let now = now_ms();
        let n = client
            .execute(
                "UPDATE session_leases SET heartbeat_at_ms = $1, expires_at_ms = $2 WHERE session_id = $3 AND worker_id = $4 AND generation = $5 AND expires_at_ms >= $1",
                &[&now, &(now + ttl_ms), &uuid_of(session.as_bytes()), &worker_id, &(generation as i64)],
            )
            .await?;
        Ok(n == 1)
    }

    /// Release a held lease (the session stays ready when `ready`).
    pub async fn release_lease(
        &self,
        session: SessionId,
        worker_id: &str,
        generation: u64,
        ready: bool,
    ) -> Result<bool> {
        let client = self.pool.get().await?;
        let n = client
            .execute(
                "UPDATE session_leases SET expires_at_ms = 0, ready = $1 WHERE session_id = $2 AND worker_id = $3 AND generation = $4",
                &[&ready, &uuid_of(session.as_bytes()), &worker_id, &(generation as i64)],
            )
            .await?;
        Ok(n == 1)
    }

    /// The lease as recorded: `(worker, generation, expires_at_ms, ready)`.
    pub async fn lease(
        &self,
        tenant: TenantId,
        session: SessionId,
    ) -> Result<Option<(Option<String>, u64, i64, bool)>> {
        let client = self.pool.get().await?;
        let row = client
            .query_opt(
                "SELECT worker_id, generation, expires_at_ms, ready FROM session_leases WHERE session_id = $1 AND tenant_id = $2",
                &[&uuid_of(session.as_bytes()), &tenant_uuid(tenant)],
            )
            .await?;
        Ok(row.map(|r| (r.get(0), r.get::<_, i64>(1) as u64, r.get(2), r.get(3))))
    }

    // ---- objects ----------------------------------------------------------

    fn object_key(tenant: TenantId, hash: &str) -> object_store::path::Path {
        object_store::path::Path::from(format!("tenants/{tenant}/objects/{hash}"))
    }

    async fn put_object_tx(
        &self,
        tx: &Transaction<'_>,
        tenant: TenantId,
        bytes: &[u8],
        mime: &str,
    ) -> Result<String> {
        let hash = sha256_hex(bytes);
        let key = match &self.objects {
            Some((store, _)) => {
                let path = Self::object_key(tenant, &hash);
                if store.head(&path).await.is_err() {
                    store
                        .put(&path, bytes::Bytes::copy_from_slice(bytes).into())
                        .await?;
                }
                path.to_string()
            }
            None => format!("pg:{}", hex::encode(bytes)),
        };
        tx.execute(
            "INSERT INTO objects (tenant_id, hash, byte_length, mime, key, created_at_ms) VALUES ($1, $2, $3, $4, $5, $6) ON CONFLICT (tenant_id, hash) DO NOTHING",
            &[&tenant_uuid(tenant), &hash, &(bytes.len() as i64), &mime, &key, &now_ms()],
        )
        .await?;
        Ok(hash)
    }

    /// Store bytes for the tenant; the content hash is the id.
    pub async fn put_object(&self, tenant: TenantId, bytes: &[u8], mime: &str) -> Result<String> {
        let mut client = self.pool.get().await?;
        let tx = client.transaction().await?;
        let hash = self.put_object_tx(&tx, tenant, bytes, mime).await?;
        tx.commit().await?;
        Ok(hash)
    }

    /// Object metadata, within the tenant.
    pub async fn object_meta(&self, tenant: TenantId, hash: &str) -> Result<Option<ObjectMeta>> {
        let client = self.pool.get().await?;
        let row = client
            .query_opt(
                "SELECT byte_length, mime FROM objects WHERE tenant_id = $1 AND hash = $2",
                &[&tenant_uuid(tenant), &hash],
            )
            .await?;
        Ok(row.map(|r| ObjectMeta {
            hash: hash.to_owned(),
            byte_length: r.get::<_, i64>(0) as u64,
            mime: r.get(1),
        }))
    }

    /// The whole object, within the tenant.
    pub async fn get_object(&self, tenant: TenantId, hash: &str) -> Result<Vec<u8>> {
        let client = self.pool.get().await?;
        let row = client
            .query_opt(
                "SELECT key FROM objects WHERE tenant_id = $1 AND hash = $2",
                &[&tenant_uuid(tenant), &hash],
            )
            .await?;
        let Some(r) = row else {
            return Err(CloudError::NotFound(format!("object {hash}")));
        };
        let key: String = r.get(0);
        if let Some(inline) = key.strip_prefix("pg:") {
            return hex::decode(inline).map_err(|e| CloudError::Object(e.to_string()));
        }
        let Some((store, _)) = &self.objects else {
            return Err(CloudError::Object("no object store configured".into()));
        };
        let got = store.get(&object_store::path::Path::from(key)).await?;
        let bytes = got.bytes().await?;
        if sha256_hex(&bytes) != hash {
            return Err(CloudError::Integrity(format!(
                "object {hash} content does not match its hash"
            )));
        }
        Ok(bytes.to_vec())
    }

    /// A byte range of the object (`start`, up to `len` bytes), within the tenant.
    pub async fn get_object_range(
        &self,
        tenant: TenantId,
        hash: &str,
        start: u64,
        len: u64,
    ) -> Result<(Vec<u8>, ObjectMeta)> {
        let Some(meta) = self.object_meta(tenant, hash).await? else {
            return Err(CloudError::NotFound(format!("object {hash}")));
        };
        let start = start.min(meta.byte_length);
        let end = start.saturating_add(len).min(meta.byte_length);
        let bytes = match &self.objects {
            Some((store, _)) => {
                let key = Self::object_key(tenant, hash);
                if start == end {
                    Vec::new()
                } else {
                    store.get_range(&key, start..end).await?.to_vec()
                }
            }
            None => {
                let all = self.get_object(tenant, hash).await?;
                all[start as usize..end as usize].to_vec()
            }
        };
        Ok((bytes, meta))
    }

    /// A short-lived signed URL for direct download (docs/24 "Multi-tenancy":
    /// per-tenant prefixes plus signed short-lived URLs); `None` without an
    /// object store.
    pub async fn signed_get_url(
        &self,
        tenant: TenantId,
        hash: &str,
        ttl: std::time::Duration,
    ) -> Result<Option<String>> {
        if self.object_meta(tenant, hash).await?.is_none() {
            return Err(CloudError::NotFound(format!("object {hash}")));
        }
        let Some((_, cfg)) = &self.objects else {
            return Ok(None);
        };
        use object_store::signer::Signer;
        let s3 = object_store::aws::AmazonS3Builder::new()
            .with_endpoint(&cfg.endpoint)
            .with_bucket_name(&cfg.bucket)
            .with_region(&cfg.region)
            .with_access_key_id(&cfg.access_key_id)
            .with_secret_access_key(&cfg.secret_access_key)
            .with_allow_http(cfg.allow_http)
            .with_virtual_hosted_style_request(false)
            .build()?;
        let url = s3
            .signed_url(reqwest::Method::GET, &Self::object_key(tenant, hash), ttl)
            .await?;
        Ok(Some(url.to_string()))
    }

    // ---- audit -----------------------------------------------------------

    /// Record a denied dereference (docs/24: cross-tenant use is denied and audited).
    pub async fn record_denial(
        &self,
        tenant: Option<TenantId>,
        principal: Option<uuid::Uuid>,
        resource: &str,
        reason: &str,
    ) -> Result<()> {
        let client = self.pool.get().await?;
        client
            .execute(
                "INSERT INTO denials (tenant_id, principal_id, resource, reason, at_ms) VALUES ($1, $2, $3, $4, $5)",
                &[&tenant.map(tenant_uuid), &principal, &resource, &reason, &now_ms()],
            )
            .await?;
        Ok(())
    }

    /// Record a sandbox the gateway provisioned (M8.3, docs/24 "Sandbox
    /// Gateway": the mapping from tenant/session/task to a substrate lease).
    #[allow(clippy::too_many_arguments)]
    pub async fn record_sandbox(
        &self,
        sandbox: uuid::Uuid,
        tenant: TenantId,
        session: SessionId,
        task: TaskId,
        worker_id: &str,
        lease_generation: u64,
        backend: &str,
        isolated: bool,
        state: &str,
        detail: &str,
    ) -> Result<()> {
        let client = self.pool.get().await?;
        let now = now_ms();
        client
            .execute(
                "INSERT INTO sandboxes (sandbox_id, tenant_id, session_id, task_id, worker_id, lease_generation, backend, isolated, state, detail, created_at_ms, updated_at_ms) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $11)",
                &[
                    &sandbox,
                    &tenant_uuid(tenant),
                    &uuid_of(session.as_bytes()),
                    &uuid_of(task.as_bytes()),
                    &worker_id,
                    &(lease_generation as i64),
                    &backend,
                    &isolated,
                    &state,
                    &detail,
                    &now,
                ],
            )
            .await?;
        Ok(())
    }

    /// A sandbox within its tenant: `(session, task, worker, generation,
    /// backend, isolated, state)`; `None` for another tenant's or unknown.
    pub async fn sandbox(
        &self,
        tenant: TenantId,
        sandbox: uuid::Uuid,
    ) -> Result<Option<SandboxRecord>> {
        let client = self.pool.get().await?;
        let row = client
            .query_opt(
                "SELECT session_id, task_id, worker_id, lease_generation, backend, isolated, state, detail FROM sandboxes WHERE tenant_id = $1 AND sandbox_id = $2",
                &[&tenant_uuid(tenant), &sandbox],
            )
            .await?;
        Ok(row.map(|r| SandboxRecord {
            sandbox_id: sandbox,
            tenant_id: tenant,
            session_id: SessionId::from_bytes(*r.get::<_, uuid::Uuid>(0).as_bytes()),
            task_id: TaskId::from_bytes(*r.get::<_, uuid::Uuid>(1).as_bytes()),
            worker_id: r.get(2),
            lease_generation: r.get::<_, i64>(3) as u64,
            backend: r.get(4),
            isolated: r.get(5),
            state: r.get(6),
            detail: r.get(7),
        }))
    }

    /// Move a sandbox to `state`.
    pub async fn set_sandbox_state(
        &self,
        tenant: TenantId,
        sandbox: uuid::Uuid,
        state: &str,
        detail: &str,
    ) -> Result<bool> {
        let client = self.pool.get().await?;
        let n = client
            .execute(
                "UPDATE sandboxes SET state = $1, detail = $2, updated_at_ms = $3 WHERE tenant_id = $4 AND sandbox_id = $5",
                &[&state, &detail, &now_ms(), &tenant_uuid(tenant), &sandbox],
            )
            .await?;
        Ok(n == 1)
    }

    /// Record one egress decision of the broker (M8.6).
    #[allow(clippy::too_many_arguments)]
    pub async fn record_egress(
        &self,
        sandbox: uuid::Uuid,
        tenant: TenantId,
        kind: &str,
        destination: &str,
        allowed: bool,
        capability: &str,
        detail: &str,
        at_ms: i64,
    ) -> Result<()> {
        let client = self.pool.get().await?;
        client
            .execute(
                "INSERT INTO sandbox_egress (sandbox_id, tenant_id, kind, destination, allowed, capability, detail, at_ms) VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
                &[&sandbox, &tenant_uuid(tenant), &kind, &destination, &allowed, &capability, &detail, &at_ms],
            )
            .await?;
        Ok(())
    }

    /// The egress audit of a sandbox within its tenant:
    /// `(kind, destination, allowed, capability, detail, at_ms)`.
    pub async fn egress_audit(
        &self,
        tenant: TenantId,
        sandbox: uuid::Uuid,
    ) -> Result<Vec<(String, String, bool, String, String, i64)>> {
        let client = self.pool.get().await?;
        let rows = client
            .query(
                "SELECT kind, destination, allowed, capability, detail, at_ms FROM sandbox_egress WHERE tenant_id = $1 AND sandbox_id = $2 ORDER BY egress_id ASC",
                &[&tenant_uuid(tenant), &sandbox],
            )
            .await?;
        Ok(rows
            .iter()
            .map(|r| (r.get(0), r.get(1), r.get(2), r.get(3), r.get(4), r.get(5)))
            .collect())
    }

    /// Denials recorded for a tenant (audit read).
    pub async fn denials(&self, tenant: TenantId) -> Result<Vec<(String, String)>> {
        let client = self.pool.get().await?;
        let rows = client
            .query(
                "SELECT resource, reason FROM denials WHERE tenant_id = $1 ORDER BY denial_id ASC",
                &[&tenant_uuid(tenant)],
            )
            .await?;
        Ok(rows.into_iter().map(|r| (r.get(0), r.get(1))).collect())
    }

    /// A dedicated connection listening for committed events (`pg_notify`),
    /// delivering `(session_id, session_offset)` per append.
    pub async fn listen(
        &self,
        database_url: &str,
    ) -> Result<tokio::sync::mpsc::UnboundedReceiver<(SessionId, u64)>> {
        use futures_util::StreamExt;
        let (client, mut connection) = tokio_postgres::connect(database_url, NoTls).await?;
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        tokio::spawn(async move {
            let mut stream = futures_util::stream::poll_fn(move |cx| connection.poll_message(cx));
            while let Some(msg) = stream.next().await {
                match msg {
                    Ok(tokio_postgres::AsyncMessage::Notification(n)) => {
                        if let Ok(v) = serde_json::from_str::<serde_json::Value>(n.payload())
                            && let Some(sid) = v["session_id"]
                                .as_str()
                                .and_then(|s| SessionId::parse(s).ok())
                        {
                            let _ = tx.send((sid, v["session_offset"].as_u64().unwrap_or(0)));
                        }
                    }
                    Ok(_) => {}
                    Err(_) => break,
                }
            }
        });
        client.batch_execute("LISTEN modbit_events").await?;
        // The client must live as long as the listener.
        tokio::spawn(async move {
            let _keep = client;
            std::future::pending::<()>().await;
        });
        Ok(rx)
    }
}

async fn aggregate_head(tx: &Transaction<'_>, aggregate_id: &[u8; 16]) -> Result<(u64, String)> {
    let row = tx
        .query_opt(
            "SELECT sequence, envelope->>'integrity_hash' FROM events WHERE aggregate_id = $1 ORDER BY sequence DESC LIMIT 1",
            &[&aggregate_id.as_slice()],
        )
        .await?;
    Ok(match row {
        Some(r) => (r.get::<_, i64>(0) as u64, r.get::<_, String>(1)),
        None => (0, String::new()),
    })
}

async fn insert_event(
    tx: &Transaction<'_>,
    env: &EventEnvelope,
    session_offset: i64,
    payload: &serde_json::Value,
) -> Result<i64> {
    insert_event_as(tx, env.tenant_id, env, session_offset, payload).await
}

/// `insert_event` scoped to `tenant` (a handoff's envelopes keep their
/// origin tenant inside; the row belongs to the admitting tenant).
async fn insert_event_as(
    tx: &Transaction<'_>,
    tenant: TenantId,
    env: &EventEnvelope,
    session_offset: i64,
    payload: &serde_json::Value,
) -> Result<i64> {
    let (inline, payload_ref): (Option<serde_json::Value>, Option<String>) = match &env.payload {
        PayloadRef::Inline { payload } => (Some(payload.clone()), None),
        PayloadRef::Object { object_hash, .. } => (None, Some(object_hash.clone())),
    };
    let _ = payload;
    let params: [&(dyn ToSql + Sync); 13] = [
        &uuid_of(tenant.as_bytes()),
        &uuid_of(env.session_id.as_bytes()),
        &session_offset,
        &env.task_id.map(|t| uuid_of(t.as_bytes())),
        &env.run_id.map(|t| uuid_of(t.as_bytes())),
        &env.aggregate_type.as_str(),
        &env.aggregate_id.as_slice(),
        &(env.sequence as i64),
        &env.event_type,
        &env.occurred_at.millis(),
        &serde_json::to_value(env)?,
        &inline,
        &payload_ref,
    ];
    let row = tx
        .query_one(
            "INSERT INTO events (tenant_id, session_id, session_offset, task_id, run_id, aggregate_type, aggregate_id, sequence, event_type, occurred_at_ms, envelope, payload, payload_ref) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13) RETURNING event_offset",
            &params,
        )
        .await?;
    Ok(row.get(0))
}

/// Update the projection of the event's aggregate (sessions, tasks,
/// approvals) with the domain state machines — the same transitions the
/// local store enforces (docs/13).
async fn project(
    tx: &Transaction<'_>,
    env: &EventEnvelope,
    payload: &serde_json::Value,
) -> Result<()> {
    project_as(tx, env.tenant_id, env, payload).await
}

/// `project` scoped to `tenant`.
async fn project_as(
    tx: &Transaction<'_>,
    tenant_id: TenantId,
    env: &EventEnvelope,
    payload: &serde_json::Value,
) -> Result<()> {
    let at = env.occurred_at;
    let tenant = uuid_of(tenant_id.as_bytes());
    let invalid = |e: modbit_domain::InvalidTransition| {
        CloudError::InvalidTransition(format!("{e} (sequence {})", env.sequence))
    };
    match env.aggregate_type {
        AggregateType::Session => {
            let event: SessionEvent = serde_json::from_value(payload.clone())?;
            let sid = SessionId::from_bytes(env.aggregate_id);
            let existing = tx
                .query_opt(
                    "SELECT doc FROM sessions WHERE session_id = $1 AND doc <> '{}'::jsonb",
                    &[&uuid_of(sid.as_bytes())],
                )
                .await?;
            let mut s: Session = match existing {
                Some(r) => serde_json::from_value(r.get::<_, serde_json::Value>(0))?,
                None => Session::create(sid, &event, at).map_err(invalid)?,
            };
            if env.sequence > 1 {
                s.apply(&event, at).map_err(invalid)?;
            }
            let state = serde_json::to_string(&s.state)?
                .trim_matches('"')
                .to_owned();
            tx.execute(
                "INSERT INTO sessions (session_id, tenant_id, state, generation, last_session_offset, doc, created_at_ms, updated_at_ms) VALUES ($1, $2, $3, $4, 0, $5, $6, $7) ON CONFLICT (session_id) DO UPDATE SET state = EXCLUDED.state, generation = EXCLUDED.generation, doc = EXCLUDED.doc, updated_at_ms = EXCLUDED.updated_at_ms",
                &[&uuid_of(sid.as_bytes()), &tenant, &state, &(s.generation as i64), &serde_json::to_value(&s)?, &s.created_at.millis(), &at.millis()],
            )
            .await?;
        }
        AggregateType::Task => {
            let event: TaskEvent = serde_json::from_value(payload.clone())?;
            let tid = TaskId::from_bytes(env.aggregate_id);
            let existing = tx
                .query_opt(
                    "SELECT doc FROM tasks WHERE task_id = $1",
                    &[&uuid_of(tid.as_bytes())],
                )
                .await?;
            let mut t: Task = match existing {
                Some(r) => serde_json::from_value(r.get::<_, serde_json::Value>(0))?,
                None => Task::create(tid, &event, at).map_err(invalid)?,
            };
            if env.sequence > 1 {
                t.apply(&event, at).map_err(invalid)?;
            }
            let (state, wait_reason) = match t.state {
                TaskState::Waiting(r) => (
                    "WAITING".to_owned(),
                    Some(serde_json::to_string(&r)?.trim_matches('"').to_owned()),
                ),
                other => (
                    serde_json::to_string(&other)?.trim_matches('"').to_owned(),
                    None,
                ),
            };
            tx.execute(
                "INSERT INTO tasks (task_id, tenant_id, session_id, state, wait_reason, generation, doc, created_at_ms, updated_at_ms) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9) ON CONFLICT (task_id) DO UPDATE SET state = EXCLUDED.state, wait_reason = EXCLUDED.wait_reason, generation = EXCLUDED.generation, doc = EXCLUDED.doc, updated_at_ms = EXCLUDED.updated_at_ms",
                &[&uuid_of(tid.as_bytes()), &tenant, &uuid_of(t.session_id.as_bytes()), &state, &wait_reason, &(t.generation as i64), &serde_json::to_value(&t)?, &t.created_at.millis(), &at.millis()],
            )
            .await?;
        }
        AggregateType::Approval => {
            let event: ApprovalEvent = serde_json::from_value(payload.clone())?;
            let aid = ApprovalId::from_bytes(env.aggregate_id);
            let existing = tx
                .query_opt(
                    "SELECT doc FROM approvals WHERE approval_id = $1",
                    &[&uuid_of(aid.as_bytes())],
                )
                .await?;
            let mut a: Approval = match existing {
                Some(r) => serde_json::from_value(r.get::<_, serde_json::Value>(0))?,
                None => Approval::create(aid, &event, at).map_err(invalid)?,
            };
            if env.sequence > 1 {
                a.apply(&event, at).map_err(invalid)?;
            }
            let state = serde_json::to_string(&a.state)?
                .trim_matches('"')
                .to_owned();
            tx.execute(
                "INSERT INTO approvals (approval_id, tenant_id, session_id, task_id, state, intent_hash, generation, doc, updated_at_ms) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9) ON CONFLICT (approval_id) DO UPDATE SET state = EXCLUDED.state, generation = EXCLUDED.generation, doc = EXCLUDED.doc, updated_at_ms = EXCLUDED.updated_at_ms",
                &[&uuid_of(aid.as_bytes()), &tenant, &uuid_of(env.session_id.as_bytes()), &uuid_of(a.task_id.as_bytes()), &state, &a.intent_hash, &(a.generation as i64), &serde_json::to_value(&a)?, &at.millis()],
            )
            .await?;
        }
        _ => {}
    }
    Ok(())
}

/// Build an actor for the API's own writes.
#[must_use]
pub fn api_actor(principal: &Principal) -> Actor {
    if principal.kind == "user" {
        Actor::User(modbit_domain::UserId::from_bytes(
            *principal.principal_id.as_bytes(),
        ))
    } else {
        Actor::External(format!("{}:{}", principal.kind, principal.label))
    }
}

/// One `NewEvent` from a typed payload.
pub fn new_event<T: serde::Serialize>(
    event_type: &str,
    payload: &T,
    actor: Actor,
) -> Result<NewEvent> {
    Ok(NewEvent {
        event_type: event_type.to_owned(),
        payload: serde_json::to_value(payload)?,
        actor,
        causation_id: None,
        correlation_id: None,
        occurred_at: None,
    })
}

// A map type used by callers building projections in bulk.
#[allow(dead_code)]
type Map<K, V> = HashMap<K, V>;
