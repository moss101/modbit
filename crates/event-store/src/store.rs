//! The SQLite-backed store.

use std::path::{Path, PathBuf};

use modbit_domain::event::{Actor, AggregateType, EventEnvelope, PayloadRef};
use modbit_domain::{
    EventId, RunId, RunStepId, SessionId, TaskId, TenantId, Timestamp, ToolCallId, TurnId,
};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use sha2::{Digest, Sha256};

use crate::migrations::MigrationReport;
use crate::objects::ObjectStore;
use crate::schema::INLINE_PAYLOAD_CEILING;
use crate::{Error, Result};

/// A stored event with its store-wide offset.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StoredEvent {
    /// Store-wide monotonic offset (resume cursor).
    pub offset: u64,
    /// The envelope.
    pub envelope: EventEnvelope,
}

/// Scope of an evidence search (REQ-EV-0132); every field narrows.
#[derive(Clone, Debug, Default)]
pub struct EvidenceScope {
    /// Session.
    pub session_id: Option<SessionId>,
    /// Task.
    pub task_id: Option<TaskId>,
    /// Run.
    pub run_id: Option<RunId>,
    /// Step.
    pub step_id: Option<RunStepId>,
}

/// One evidence hit: an event, a tool call, a step or a check that matched.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct EvidenceHit {
    /// `message` | `tool` | `step` | `file` | `error` | `checkpoint` | `check` | `event`.
    pub kind: String,
    /// Store offset (events) or 0.
    pub offset: u64,
    /// Event type or row kind.
    pub event_type: String,
    /// Task.
    pub task_id: Option<String>,
    /// Run.
    pub run_id: Option<String>,
    /// Step.
    pub step_id: Option<String>,
    /// Tool call, when the hit is one.
    pub tool_call_id: Option<String>,
    /// Bounded excerpt around the match.
    pub snippet: String,
    /// When (ms), when known.
    pub occurred_at: Option<i64>,
}

/// One event to append. Lineage ids are copied onto the envelope.
#[derive(Clone, Debug)]
pub struct NewEvent {
    /// Canonical event type name.
    pub event_type: String,
    /// Payload JSON (stored inline or as an object depending on size).
    pub payload: serde_json::Value,
    /// Who caused it.
    pub actor: Actor,
    /// Causing event.
    pub causation_id: Option<EventId>,
    /// Correlation id.
    pub correlation_id: Option<EventId>,
    /// When it occurred (defaults to now when `None`).
    pub occurred_at: Option<Timestamp>,
}

impl NewEvent {
    /// Convenience constructor.
    pub fn new(event_type: impl Into<String>, payload: serde_json::Value, actor: Actor) -> Self {
        Self {
            event_type: event_type.into(),
            payload,
            actor,
            causation_id: None,
            correlation_id: None,
            occurred_at: None,
        }
    }
}

/// An append to one aggregate.
#[derive(Clone, Debug)]
pub struct AppendRequest {
    /// Tenant scope.
    pub tenant_id: TenantId,
    /// Session.
    pub session_id: SessionId,
    /// Task lineage.
    pub task_id: Option<TaskId>,
    /// Run lineage.
    pub run_id: Option<RunId>,
    /// Turn lineage.
    pub turn_id: Option<TurnId>,
    /// Step lineage.
    pub step_id: Option<RunStepId>,
    /// Aggregate kind.
    pub aggregate_type: AggregateType,
    /// Aggregate id bytes.
    pub aggregate_id: [u8; 16],
    /// Required current sequence of the aggregate (`0` for a new aggregate);
    /// `None` skips the optimistic check.
    pub expected_sequence: Option<u64>,
    /// Events in order.
    pub events: Vec<NewEvent>,
}

/// Fault injection for the kill-point suite (docs/54 faults 1 and 2; docs/19
/// "release-tested by process kill at every major state"; M4.6): the
/// process aborts right before or right after committing the n-th event of a
/// named type. `MODBIT_FAULT_KILL_BEFORE_EVENT="<EventType>:<n>"` and
/// `MODBIT_FAULT_KILL_AFTER_EVENT="<EventType>:<n>"`; unset in production.
#[derive(Debug, Default)]
struct FaultPlan {
    before: Option<(String, u32)>,
    after: Option<(String, u32)>,
    seen: std::collections::HashMap<String, u32>,
}

impl FaultPlan {
    fn from_env() -> Self {
        let parse = |var: &str| {
            let v = std::env::var(var).ok()?;
            let (ty, n) = v.split_once(':')?;
            Some((ty.to_owned(), n.parse::<u32>().ok()?))
        };
        Self {
            before: parse("MODBIT_FAULT_KILL_BEFORE_EVENT"),
            after: parse("MODBIT_FAULT_KILL_AFTER_EVENT"),
            seen: std::collections::HashMap::new(),
        }
    }

    fn armed(&self) -> bool {
        self.before.is_some() || self.after.is_some()
    }

    /// Count the events about to be committed; abort before the commit if
    /// the n-th event of the "before" type is among them.
    fn before_commit(&mut self, events: &[StoredEvent]) {
        if !self.armed() {
            return;
        }
        let mut hit = false;
        for e in events {
            let n = self.seen.entry(e.envelope.event_type.clone()).or_insert(0);
            *n += 1;
            if let Some((ty, at)) = &self.before
                && *ty == e.envelope.event_type
                && *n == *at
            {
                hit = true;
            }
        }
        if hit {
            eprintln!(
                "modbit-event-store: MODBIT_FAULT_KILL_BEFORE_EVENT hit; aborting before commit"
            );
            std::process::abort();
        }
    }

    /// Abort after the commit if the n-th event of the "after" type was in it.
    fn after_commit(&self, events: &[StoredEvent]) {
        let Some((ty, at)) = &self.after else {
            return;
        };
        // `seen` was counted in `before_commit` (the same batch).
        let count = self.seen.get(ty).copied().unwrap_or(0);
        let in_batch = events
            .iter()
            .filter(|e| e.envelope.event_type == *ty)
            .count() as u32;
        if in_batch > 0 && count >= *at && count - in_batch < *at {
            eprintln!(
                "modbit-event-store: MODBIT_FAULT_KILL_AFTER_EVENT hit; aborting after commit"
            );
            std::process::abort();
        }
    }
}

/// The canonical Event Store.
pub struct EventStore {
    conn: Connection,
    objects: ObjectStore,
    db_path: PathBuf,
    fault: FaultPlan,
}

impl std::fmt::Debug for EventStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EventStore")
            .field("db_path", &self.db_path)
            .finish_non_exhaustive()
    }
}

fn canonical_content(e: &EventEnvelope) -> Vec<u8> {
    // Deterministic byte string over every field except integrity_hash.
    let mut v = Vec::new();
    v.extend_from_slice(e.event_id.as_bytes());
    v.extend_from_slice(e.tenant_id.as_bytes());
    v.extend_from_slice(e.session_id.as_bytes());
    for id in [
        e.task_id.map(|x| *x.as_bytes()),
        e.run_id.map(|x| *x.as_bytes()),
        e.turn_id.map(|x| *x.as_bytes()),
        e.step_id.map(|x| *x.as_bytes()),
    ] {
        v.push(u8::from(id.is_some()));
        v.extend_from_slice(&id.unwrap_or([0; 16]));
    }
    v.extend_from_slice(e.aggregate_type.as_str().as_bytes());
    v.push(0);
    v.extend_from_slice(&e.aggregate_id);
    v.extend_from_slice(&e.sequence.to_be_bytes());
    v.extend_from_slice(e.event_type.as_bytes());
    v.push(0);
    v.extend_from_slice(&e.schema_version.to_be_bytes());
    v.extend_from_slice(&e.occurred_at.millis().to_be_bytes());
    v.extend_from_slice(
        serde_json::to_string(&e.actor)
            .expect("actor json")
            .as_bytes(),
    );
    v.push(0);
    for id in [e.causation_id, e.correlation_id] {
        v.push(u8::from(id.is_some()));
        v.extend_from_slice(id.map(|x| *x.as_bytes()).as_ref().unwrap_or(&[0; 16]));
    }
    match &e.payload {
        PayloadRef::Inline { payload } => {
            v.push(1);
            v.extend_from_slice(payload.to_string().as_bytes());
        }
        PayloadRef::Object {
            object_hash,
            byte_length,
        } => {
            v.push(2);
            v.extend_from_slice(object_hash.as_bytes());
            v.extend_from_slice(&byte_length.to_be_bytes());
        }
    }
    v
}

/// `sha256(previous_hash || canonical content)`.
fn chain_hash(previous: &str, e: &EventEnvelope) -> String {
    let mut h = Sha256::new();
    h.update(previous.as_bytes());
    h.update(canonical_content(e));
    hex::encode(h.finalize())
}

fn is_busy(e: &rusqlite::Error) -> bool {
    matches!(
        e,
        rusqlite::Error::SqliteFailure(f, _)
            if f.code == rusqlite::ErrorCode::DatabaseBusy || f.code == rusqlite::ErrorCode::DatabaseLocked
    )
}

fn opt_blob(id: Option<[u8; 16]>) -> Option<Vec<u8>> {
    id.map(|b| b.to_vec())
}

fn blob16(bytes: Vec<u8>) -> rusqlite::Result<[u8; 16]> {
    bytes.try_into().map_err(|_| rusqlite::Error::InvalidQuery)
}

impl EventStore {
    /// Open (creating) the store rooted at `dir`: `core.db` plus `objects/`.
    /// Applies pending migrations; refuses a newer schema.
    pub fn open(dir: &Path) -> Result<Self> {
        Self::open_with_report(dir).map(|(s, _)| s)
    }

    /// Open and return the migration report.
    pub fn open_with_report(dir: &Path) -> Result<(Self, MigrationReport)> {
        std::fs::create_dir_all(dir)?;
        let db_path = dir.join("core.db");
        // Opening sets pragmas and runs migrations, both of which take write
        // locks. The busy handler does not cover every lock-upgrade path
        // (SQLite returns BUSY immediately when waiting could deadlock), so a
        // concurrent opener retries with backoff for up to ten seconds.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        let (conn, report) = loop {
            match Self::connect_and_migrate(&db_path) {
                Ok(v) => break v,
                Err(Error::Sqlite(e)) if is_busy(&e) && std::time::Instant::now() < deadline => {
                    std::thread::sleep(std::time::Duration::from_millis(25));
                }
                Err(e) => return Err(e),
            }
        };
        let objects = ObjectStore::open(dir.join("objects"))?;
        let mut store = Self {
            conn,
            objects,
            db_path,
            fault: FaultPlan::from_env(),
        };
        if report.applied.iter().any(|v| *v >= 2) {
            // Projections were introduced after events may already exist: derive them.
            store.rebuild_projections()?;
        }
        Ok((store, report))
    }

    fn connect_and_migrate(db_path: &Path) -> Result<(Connection, MigrationReport)> {
        let mut conn = Connection::open(db_path)?;
        conn.busy_timeout(std::time::Duration::from_secs(10))?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "FULL")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        let report = crate::migrations::migrate(&mut conn)?;
        Ok((conn, report))
    }

    /// The object store.
    #[must_use]
    pub fn objects(&self) -> &ObjectStore {
        &self.objects
    }

    /// Path of `core.db`.
    #[must_use]
    pub fn db_path(&self) -> &Path {
        &self.db_path
    }

    /// Current sequence and integrity hash of an aggregate (`0`, empty for none).
    pub fn head(&self, aggregate_id: &[u8; 16]) -> Result<(u64, String)> {
        let row: Option<(i64, String)> = self
            .conn
            .query_row(
                "SELECT sequence, integrity_hash FROM events WHERE aggregate_id = ?1 ORDER BY sequence DESC LIMIT 1",
                params![aggregate_id.as_slice()],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        Ok(row
            .map(|(s, h)| (s as u64, h))
            .unwrap_or((0, String::new())))
    }

    /// Append events to one aggregate in a single transaction, updating the
    /// projections in that same transaction.
    pub fn append(&mut self, req: AppendRequest) -> Result<Vec<StoredEvent>> {
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let out = append_in(&tx, &self.objects, req)?;
        self.fault.before_commit(&out);
        tx.commit()?;
        self.fault.after_commit(&out);
        Ok(out)
    }

    /// Append several requests — different aggregates — in one transaction
    /// (docs/19: "Core commits event + critical projection changes in one
    /// transaction"; M4.6): a tool call's outcome and the file changes it
    /// made land together or not at all, so a kill between them cannot leave
    /// an effect without its record. Requests are applied in order.
    pub fn append_all(
        &mut self,
        reqs: Vec<AppendRequest>,
        lease_generation: Option<u64>,
    ) -> Result<Vec<StoredEvent>> {
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        if let (Some(g), Some(first)) = (lease_generation, reqs.first()) {
            let current: Option<i64> = tx
                .query_row(
                    "SELECT lease_generation FROM sessions WHERE session_id = ?1",
                    params![first.session_id.as_bytes().as_slice()],
                    |r| r.get(0),
                )
                .optional()?;
            let current = current.map_or(0, |c| c as u64);
            if current != g {
                return Err(Error::StaleLease {
                    session: first.session_id.to_string(),
                    presented: g,
                    current,
                });
            }
        }
        let mut out = Vec::new();
        for req in reqs {
            out.extend(append_in(&tx, &self.objects, req)?);
        }
        self.fault.before_commit(&out);
        tx.commit()?;
        self.fault.after_commit(&out);
        Ok(out)
    }

    /// Append under the session kernel lease (docs/13 "Fencing and epochs",
    /// docs/33 "Session kernel lease", M4.4): the events land only if the
    /// session's lease generation is exactly `lease_generation` at commit
    /// time, checked inside the same transaction. A writer whose lease was
    /// superseded gets [`Error::StaleLease`] and nothing is written — it may
    /// still record audit events through [`Self::append`], but it cannot
    /// advance state.
    pub fn append_fenced(
        &mut self,
        req: AppendRequest,
        lease_generation: u64,
    ) -> Result<Vec<StoredEvent>> {
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let current: Option<i64> = tx
            .query_row(
                "SELECT lease_generation FROM sessions WHERE session_id = ?1",
                params![req.session_id.as_bytes().as_slice()],
                |r| r.get(0),
            )
            .optional()?;
        let current = current.map_or(0, |g| g as u64);
        if current != lease_generation {
            return Err(Error::StaleLease {
                session: req.session_id.to_string(),
                presented: lease_generation,
                current,
            });
        }
        let out = append_in(&tx, &self.objects, req)?;
        self.fault.before_commit(&out);
        tx.commit()?;
        self.fault.after_commit(&out);
        Ok(out)
    }

    /// Execute a mutating command idempotently (docs/30: "Mutating commands are
    /// idempotent by `command_id`"). A retry with the same `command_id` and
    /// `request_hash` returns the recorded outcome without appending; the same
    /// id with a different hash is a conflict.
    pub fn execute_command(
        &mut self,
        cmd: CommandRecord,
        req: AppendRequest,
    ) -> Result<CommandOutcome> {
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let prior: Option<(String, Option<i64>, Option<i64>)> = tx
            .query_row(
                "SELECT request_hash, first_event_offset, last_event_offset FROM commands WHERE command_id = ?1",
                params![cmd.command_id.as_bytes().as_slice()],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()?;
        if let Some((hash, first, last)) = prior {
            if hash != cmd.request_hash {
                return Err(Error::IdempotencyConflict {
                    command_id: hex::encode(cmd.command_id.as_bytes()),
                });
            }
            let events = match (first, last) {
                (Some(f), Some(l)) => read_range(&tx, f as u64, l as u64)?,
                _ => Vec::new(),
            };
            return Ok(CommandOutcome::Replayed(events));
        }
        let events = append_in(&tx, &self.objects, req)?;
        tx.execute(
            "INSERT INTO commands (command_id, tenant_id, command_type, request_hash, first_event_offset, last_event_offset, accepted_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                cmd.command_id.as_bytes().as_slice(),
                cmd.tenant_id.as_bytes().as_slice(),
                &cmd.command_type,
                &cmd.request_hash,
                events.first().map(|e| e.offset as i64),
                events.last().map(|e| e.offset as i64),
                Timestamp::now().millis(),
            ],
        )?;
        self.fault.before_commit(&events);
        tx.commit()?;
        self.fault.after_commit(&events);
        Ok(CommandOutcome::Applied(events))
    }

    /// Rebuild every projection row from the event log (idempotent).
    pub fn rebuild_projections(&mut self) -> Result<u64> {
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let n = crate::projections::rebuild(&tx, &self.objects)?;
        tx.commit()?;
        Ok(n)
    }

    /// Startup recovery (docs/19 resume steps 2 and 3 for M1, docs/33 step 11):
    /// SQLite integrity check, hash-chain verification of every aggregate,
    /// projection catch-up from the log when the cursor lags, and a persisted,
    /// monotonically increasing boot generation. Returns what was done.
    pub fn recover_on_start(&mut self) -> Result<RecoveryOutcome> {
        let started = std::time::Instant::now();
        self.integrity_check()?;
        let mut notes = Vec::new();
        let aggregate_ids: Vec<Vec<u8>> = {
            let mut stmt = self
                .conn
                .prepare("SELECT DISTINCT aggregate_id FROM events")?;
            let rows = stmt.query_map([], |r| r.get::<_, Vec<u8>>(0))?;
            rows.collect::<std::result::Result<_, _>>()?
        };
        let mut events_verified = 0u64;
        for id in &aggregate_ids {
            let arr: [u8; 16] = id.as_slice().try_into().map_err(|_| Error::Integrity {
                aggregate: hex::encode(id),
                sequence: 0,
                detail: "aggregate id is not 16 bytes".into(),
            })?;
            events_verified += self.verify_aggregate(&arr)?;
        }
        let last_offset = self.last_offset()?;
        let projection_offset = self.projection_offset()?;
        let mut projections_rebuilt = false;
        if projection_offset != last_offset {
            notes.push(format!("projection cursor {projection_offset} lagged the log at {last_offset}; rebuilt from the log"));
            self.rebuild_projections()?;
            projections_rebuilt = true;
        }
        let boot_generation: u64 = {
            let prev: Option<String> = self
                .conn
                .query_row(
                    "SELECT value FROM schema_meta WHERE key = 'boot_generation'",
                    [],
                    |r| r.get(0),
                )
                .optional()?;
            let next = prev.and_then(|v| v.parse::<u64>().ok()).unwrap_or(0) + 1;
            self.conn.execute(
                "INSERT OR REPLACE INTO schema_meta (key, value) VALUES ('boot_generation', ?1)",
                params![next.to_string()],
            )?;
            next
        };
        let sessions: i64 = self
            .conn
            .query_row("SELECT count(*) FROM sessions", [], |r| r.get(0))?;
        let tasks: i64 = self
            .conn
            .query_row("SELECT count(*) FROM tasks", [], |r| r.get(0))?;
        Ok(RecoveryOutcome {
            boot_generation,
            last_offset,
            events_verified,
            aggregates_verified: aggregate_ids.len() as u64,
            projections_rebuilt,
            sessions: sessions as u64,
            tasks: tasks as u64,
            notes,
            recovery_ms: started.elapsed().as_millis() as u64,
        })
    }

    /// Offset the projections are caught up to.
    pub fn projection_offset(&self) -> Result<u64> {
        let v: Option<i64> = self
            .conn
            .query_row(
                "SELECT last_offset FROM projection_state WHERE name = ?1",
                params![crate::projections::PROJECTION_NAME],
                |r| r.get(0),
            )
            .optional()?;
        Ok(v.unwrap_or(0) as u64)
    }

    /// Load a task projection.
    pub fn task(&self, id: &TaskId) -> Result<Option<modbit_domain::task::Task>> {
        crate::projections::load_task(&self.conn, id)
    }

    /// Load a session projection.
    pub fn session(&self, id: &SessionId) -> Result<Option<modbit_domain::session::Session>> {
        crate::projections::load_session(&self.conn, id)
    }

    /// Load a run projection.
    pub fn run(&self, id: &RunId) -> Result<Option<modbit_domain::run::Run>> {
        crate::projections::load_run(&self.conn, id)
    }

    /// Load a turn projection.
    pub fn turn(&self, id: &TurnId) -> Result<Option<modbit_domain::turn::Turn>> {
        crate::projections::load_turn(&self.conn, id)
    }

    /// Load a tool-call projection.
    pub fn tool_call(
        &self,
        id: &modbit_domain::ToolCallId,
    ) -> Result<Option<modbit_domain::toolcall::ToolCall>> {
        crate::projections::load_tool_call(&self.conn, id)
    }

    /// The task's outstanding tool calls (not succeeded, failed or cancelled:
    /// proposed, validated, awaiting approval, dispatched, streaming or of
    /// unknown outcome), oldest first.
    pub fn open_tool_calls(&self, task: &TaskId) -> Result<Vec<modbit_domain::toolcall::ToolCall>> {
        crate::projections::load_open_tool_calls(&self.conn, task)
    }

    /// Every tool call of a task, oldest first.
    pub fn tool_calls_for_task(
        &self,
        task: &TaskId,
    ) -> Result<Vec<modbit_domain::toolcall::ToolCall>> {
        crate::projections::load_tool_calls_for_task(&self.conn, task)
    }

    /// Load an approval projection.
    pub fn approval(
        &self,
        id: &modbit_domain::ApprovalId,
    ) -> Result<Option<modbit_domain::approval::Approval>> {
        crate::projections::load_approval(&self.conn, id)
    }

    /// The latest approval bound to a tool call.
    pub fn approval_for_call(
        &self,
        id: &modbit_domain::ToolCallId,
    ) -> Result<Option<modbit_domain::approval::Approval>> {
        crate::projections::load_approval_for_call(&self.conn, id)
    }

    /// Approvals across a session's tasks (pending first).
    pub fn approvals_for_session(
        &self,
        id: &SessionId,
    ) -> Result<Vec<modbit_domain::approval::Approval>> {
        crate::projections::load_approvals_for_session(&self.conn, id)
    }

    /// The checkpoints of a task (docs/31 `checkpoints`), by epoch.
    pub fn checkpoints(&self, task: &TaskId) -> Result<Vec<crate::projections::CheckpointRow>> {
        crate::projections::load_checkpoints(&self.conn, task)
    }

    /// The compaction requests of a task (docs/31 `compaction_epochs`),
    /// oldest first: pending, committed as an epoch, or rejected.
    pub fn compaction_epochs(
        &self,
        task: &TaskId,
    ) -> Result<Vec<crate::projections::CompactionEpochRow>> {
        crate::projections::load_compaction_epochs(&self.conn, task)
    }

    /// The materialized protocol state of a task (docs/31 `protocol_state`,
    /// docs/19 layer 2), written in the transaction of every event that
    /// touched its calls, approvals, leases, question or reconciliations.
    pub fn protocol_state(
        &self,
        session: &SessionId,
        task: &TaskId,
    ) -> Result<Option<modbit_protocol_state::ProtocolState>> {
        crate::projections::load_protocol_state(&self.conn, session, task)
    }

    /// The approvals of a task, oldest first.
    pub fn approvals_for_task(
        &self,
        task: &TaskId,
    ) -> Result<Vec<modbit_domain::approval::Approval>> {
        crate::projections::load_approvals_for_task(&self.conn, task)
    }

    /// Load a capability lease projection.
    pub fn lease(
        &self,
        id: &modbit_domain::CapabilityLeaseId,
    ) -> Result<Option<modbit_domain::lease::CapabilityLease>> {
        crate::projections::load_lease(&self.conn, id)
    }

    /// Leases of a task, newest first.
    pub fn leases_for_task(
        &self,
        id: &TaskId,
    ) -> Result<Vec<modbit_domain::lease::CapabilityLease>> {
        crate::projections::load_leases_for_task(&self.conn, id)
    }

    /// Active leases across a session's tasks.
    pub fn active_leases_for_session(
        &self,
        id: &SessionId,
    ) -> Result<Vec<modbit_domain::lease::CapabilityLease>> {
        crate::projections::load_active_leases_for_session(&self.conn, id)
    }

    /// Newest receipt hash in the protected-effect chain.
    pub fn last_receipt_hash(&self) -> Result<Option<String>> {
        crate::projections::last_receipt_hash(&self.conn)
    }

    /// The receipt chain (all, or one task's).
    pub fn receipts(
        &self,
        task: Option<&TaskId>,
    ) -> Result<Vec<modbit_domain::toolcall::EffectReceipt>> {
        crate::projections::load_receipts(&self.conn, task)
    }

    /// Verification runs of an agent run.
    pub fn verification_runs(
        &self,
        run: &RunId,
    ) -> Result<Vec<crate::projections::VerificationRunRow>> {
        crate::projections::load_verification_runs(&self.conn, run)
    }

    /// Routing plans compiled for an agent run, with their slots (REQ-EPR-001).
    pub fn routing_plans(&self, run: &RunId) -> Result<Vec<crate::projections::RoutingPlanRow>> {
        crate::projections::load_routing_plans(&self.conn, run)
    }

    /// Attempts recorded against a routing plan, including the ones that
    /// failed and the ones whose cost the provider never reported.
    pub fn routing_attempts(
        &self,
        run: &RunId,
        plan_id: &str,
    ) -> Result<Vec<crate::projections::RoutingAttemptRow>> {
        crate::projections::load_routing_attempts(&self.conn, run, plan_id)
    }

    /// What admission decided about a run's plan (REQ-EPR-014).
    pub fn routing_admission(
        &self,
        run: &RunId,
        plan_id: &str,
    ) -> Result<Option<crate::projections::RoutingAdmissionRow>> {
        crate::projections::load_routing_admission(&self.conn, run, plan_id)
    }

    /// The activations recorded against a run's plan: what bounds a slot,
    /// and what a restart must recover rather than repeat.
    pub fn routing_activations(
        &self,
        run: &RunId,
        plan_id: &str,
    ) -> Result<Vec<crate::projections::RoutingActivationRow>> {
        crate::projections::load_routing_activations(&self.conn, run, plan_id)
    }

    /// Quarantined checks of an agent run: (check_id, first_run_id, rerun_id).
    pub fn flaky_checks(&self, run: &RunId) -> Result<Vec<(String, String, String)>> {
        crate::projections::load_flaky_checks(&self.conn, run)
    }

    /// Tasks in `Running` state.
    pub fn running_tasks(&self) -> Result<Vec<modbit_domain::task::Task>> {
        crate::projections::load_running_tasks(&self.conn)
    }

    /// Tasks that are running or waiting, oldest first.
    pub fn live_tasks(&self) -> Result<Vec<modbit_domain::task::Task>> {
        crate::projections::load_live_tasks(&self.conn)
    }

    /// Runs of a task, newest attempt first.
    pub fn runs_for_task(&self, id: &TaskId) -> Result<Vec<modbit_domain::run::Run>> {
        crate::projections::load_runs_for_task(&self.conn, id)
    }

    /// Load a run-step projection.
    pub fn step(&self, id: &RunStepId) -> Result<Option<modbit_domain::step::RunStep>> {
        crate::projections::load_step(&self.conn, id)
    }

    /// Events of one aggregate with `sequence > after`, ascending.
    pub fn read_aggregate(
        &self,
        aggregate_id: &[u8; 16],
        after: u64,
        limit: usize,
    ) -> Result<Vec<StoredEvent>> {
        let mut stmt = self.conn.prepare_cached(&format!(
            "SELECT {COLUMNS} FROM events WHERE aggregate_id = ?1 AND sequence > ?2 ORDER BY sequence ASC LIMIT ?3"
        ))?;
        let rows = stmt.query_map(
            params![aggregate_id.as_slice(), after as i64, limit as i64],
            row_to_event,
        )?;
        rows.map(|r| r.map_err(Error::from)).collect()
    }

    /// Events of one session with `offset > after`, ascending by offset
    /// (the resume cursor of REQ-EV-0010).
    pub fn read_session(
        &self,
        session_id: &SessionId,
        after_offset: u64,
        limit: usize,
    ) -> Result<Vec<StoredEvent>> {
        let mut stmt = self.conn.prepare_cached(&format!(
            "SELECT {COLUMNS} FROM events WHERE session_id = ?1 AND offset > ?2 ORDER BY offset ASC LIMIT ?3"
        ))?;
        let rows = stmt.query_map(
            params![
                session_id.as_bytes().as_slice(),
                after_offset as i64,
                limit as i64
            ],
            row_to_event,
        )?;
        rows.map(|r| r.map_err(Error::from)).collect()
    }

    /// Search the tenant's evidence — event types and payloads (inline or
    /// small objects), tool calls, run steps and verification checks — for
    /// the query's words within `scope` (REQ-EV-0132). Never crosses the
    /// tenant: the scope ids are only honoured for events the tenant owns.
    pub fn search_evidence(
        &self,
        tenant_id: &TenantId,
        scope: &EvidenceScope,
        query: &str,
        kinds: &[String],
        limit: usize,
    ) -> Result<Vec<EvidenceHit>> {
        let words: Vec<String> = query
            .split_whitespace()
            .map(str::to_ascii_lowercase)
            .filter(|w| !w.is_empty())
            .collect();
        if words.is_empty() {
            return Ok(vec![]);
        }
        let limit = limit.clamp(1, 500);
        let wants = |k: &str| kinds.is_empty() || kinds.iter().any(|x| x == k);
        let mut sql = format!("SELECT {COLUMNS} FROM events WHERE tenant_id = ?1");
        let mut args: Vec<rusqlite::types::Value> =
            vec![rusqlite::types::Value::Blob(tenant_id.as_bytes().to_vec())];
        if let Some(sid) = &scope.session_id {
            args.push(rusqlite::types::Value::Blob(sid.as_bytes().to_vec()));
            sql.push_str(&format!(" AND session_id = ?{}", args.len()));
        }
        if let Some(t) = &scope.task_id {
            args.push(rusqlite::types::Value::Blob(t.as_bytes().to_vec()));
            sql.push_str(&format!(" AND task_id = ?{}", args.len()));
        }
        if let Some(r) = &scope.run_id {
            args.push(rusqlite::types::Value::Blob(r.as_bytes().to_vec()));
            sql.push_str(&format!(" AND run_id = ?{}", args.len()));
        }
        if let Some(st) = &scope.step_id {
            args.push(rusqlite::types::Value::Blob(st.as_bytes().to_vec()));
            sql.push_str(&format!(" AND step_id = ?{}", args.len()));
        }
        sql.push_str(" ORDER BY offset DESC LIMIT 4000");
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(rusqlite::params_from_iter(args.iter()), row_to_event)?;
        let matches = |text: &str| -> Option<usize> {
            let lower = text.to_ascii_lowercase();
            words.iter().filter_map(|w| lower.find(w.as_str())).min()
        };
        let snippet = |text: &str, at: usize| -> String {
            let start = text[..at]
                .char_indices()
                .rev()
                .nth(80)
                .map_or(0, |(i, _)| i);
            let end = text[at..]
                .char_indices()
                .nth(160)
                .map_or(text.len(), |(i, _)| at + i);
            text[start..end].replace('\n', " ")
        };
        let mut out: Vec<EvidenceHit> = Vec::new();
        let mut task_ids: Vec<TaskId> = Vec::new();
        for ev in rows {
            let ev = ev?;
            let env = &ev.envelope;
            if let Some(t) = env.task_id
                && !task_ids.contains(&t)
            {
                task_ids.push(t);
            }
            let kind = match env.event_type.as_str() {
                t if t.contains("Failed")
                    || t.contains("Regression")
                    || t.contains("Invariant")
                    || t.contains("Exhausted") =>
                {
                    "error"
                }
                t if t.starts_with("ToolCall") => "tool",
                t if t.starts_with("Step") => "step",
                t if t.contains("Checkpoint") => "checkpoint",
                t if t.starts_with("Model") || t.contains("Input") || t.contains("Question") => {
                    "message"
                }
                "FileChanged" => "file",
                _ => "event",
            };
            if !wants(kind) {
                continue;
            }
            let payload = self.payload(&ev.envelope).unwrap_or_default();
            // Referenced objects (tool results, step outputs, transcript
            // messages) are part of the evidence: small ones are searched too.
            let mut text = format!("{} {}", env.event_type, payload);
            if let Some(obj) = payload.as_object() {
                for (k, v) in obj.iter().filter(|(k, _)| k.ends_with("_ref")).take(3) {
                    if let Some(h) = v.as_str()
                        && h.len() == 64
                        && let Ok(bytes) = self.objects().get(h)
                        && bytes.len() <= 64 * 1024
                    {
                        text.push_str(&format!(" [{k}] {}", String::from_utf8_lossy(&bytes)));
                    }
                }
            }
            if let Some(at) = matches(&text) {
                out.push(EvidenceHit {
                    kind: kind.into(),
                    offset: ev.offset,
                    event_type: env.event_type.clone(),
                    task_id: env.task_id.map(|t| t.to_string()),
                    run_id: env.run_id.map(|r| r.to_string()),
                    step_id: env.step_id.map(|s| s.to_string()),
                    tool_call_id: (env.aggregate_type == AggregateType::ToolCall)
                        .then(|| ToolCallId::from_bytes(env.aggregate_id).to_string()),
                    snippet: snippet(&text, at),
                    occurred_at: Some(env.occurred_at.millis()),
                });
                if out.len() >= limit {
                    return Ok(out);
                }
            }
        }
        // Tool-call and check rows of the tenant's tasks in scope.
        if wants("tool") {
            for t in &task_ids {
                let mut st = self.conn.prepare_cached(
                    "SELECT tool_call_id, tool_name, status, policy_decision, step_id, completed_at FROM tool_calls WHERE task_id = ?1 ORDER BY completed_at DESC LIMIT 500",
                )?;
                let rows = st.query_map(params![t.as_bytes().as_slice()], |r| {
                    Ok((
                        r.get::<_, Vec<u8>>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, String>(2)?,
                        r.get::<_, Option<String>>(3)?,
                        r.get::<_, Option<Vec<u8>>>(4)?,
                        r.get::<_, Option<i64>>(5)?,
                    ))
                })?;
                for row in rows {
                    let (id, name, status, decision, step, at) = row?;
                    let text =
                        format!("tool_call {name} {status} {}", decision.unwrap_or_default());
                    if let Some(atm) = matches(&text) {
                        let arr = |b: &[u8]| <[u8; 16]>::try_from(b).ok();
                        out.push(EvidenceHit {
                            kind: "tool".into(),
                            offset: 0,
                            event_type: "tool_call".into(),
                            task_id: Some(t.to_string()),
                            run_id: None,
                            step_id: step
                                .as_deref()
                                .and_then(arr)
                                .map(|a| RunStepId::from_bytes(a).to_string()),
                            tool_call_id: arr(&id).map(|a| ToolCallId::from_bytes(a).to_string()),
                            snippet: snippet(&text, atm),
                            occurred_at: at,
                        });
                        if out.len() >= limit {
                            return Ok(out);
                        }
                    }
                }
            }
        }
        if wants("check") {
            for t in &task_ids {
                for run in self.runs_for_task(t).unwrap_or_default() {
                    if scope.run_id.is_some_and(|r| r != run.run_id) {
                        continue;
                    }
                    for vr in self.verification_runs(&run.run_id).unwrap_or_default() {
                        for (check_id, status) in &vr.checks {
                            let text = format!("check {check_id} {status} stage {}", vr.stage);
                            if let Some(atm) = matches(&text) {
                                out.push(EvidenceHit {
                                    kind: "check".into(),
                                    offset: 0,
                                    event_type: "check_result".into(),
                                    task_id: Some(t.to_string()),
                                    run_id: Some(run.run_id.to_string()),
                                    step_id: None,
                                    tool_call_id: None,
                                    snippet: snippet(&text, atm),
                                    occurred_at: None,
                                });
                                if out.len() >= limit {
                                    return Ok(out);
                                }
                            }
                        }
                    }
                }
            }
        }
        Ok(out)
    }

    /// Highest offset in the store (`0` when empty).
    pub fn last_offset(&self) -> Result<u64> {
        let v: Option<i64> = self
            .conn
            .query_row("SELECT MAX(offset) FROM events", [], |r| r.get(0))?;
        Ok(v.unwrap_or(0) as u64)
    }

    /// Resolve a payload, fetching and verifying the object when stored by reference.
    pub fn payload(&self, e: &EventEnvelope) -> Result<serde_json::Value> {
        match &e.payload {
            PayloadRef::Inline { payload } => Ok(payload.clone()),
            PayloadRef::Object { object_hash, .. } => {
                Ok(serde_json::from_slice(&self.objects.get(object_hash)?)?)
            }
        }
    }

    /// Recompute the hash chain of one aggregate and check sequences are
    /// contiguous from 1; returns the number of verified events.
    pub fn verify_aggregate(&self, aggregate_id: &[u8; 16]) -> Result<u64> {
        let events = self.read_aggregate(aggregate_id, 0, usize::MAX)?;
        let mut previous = String::new();
        for (expected_seq, e) in (1u64..).zip(&events) {
            let env = &e.envelope;
            let agg = hex::encode(aggregate_id);
            if env.sequence != expected_seq {
                return Err(Error::Integrity {
                    aggregate: agg,
                    sequence: env.sequence,
                    detail: format!("expected sequence {expected_seq}"),
                });
            }
            let recomputed = chain_hash(&previous, env);
            if recomputed != env.integrity_hash {
                return Err(Error::Integrity {
                    aggregate: agg,
                    sequence: env.sequence,
                    detail: "integrity hash mismatch".into(),
                });
            }
            if let PayloadRef::Object { object_hash, .. } = &env.payload {
                self.objects.get(object_hash)?;
            }
            previous = env.integrity_hash.clone();
        }
        Ok(events.len() as u64)
    }

    /// Run SQLite's integrity check (docs/31 "periodic integrity check").
    pub fn integrity_check(&self) -> Result<()> {
        let v: String = self
            .conn
            .query_row("PRAGMA integrity_check", [], |r| r.get(0))?;
        if v == "ok" {
            Ok(())
        } else {
            Err(Error::Integrity {
                aggregate: "<database>".into(),
                sequence: 0,
                detail: v,
            })
        }
    }
}

/// What startup recovery did (M1.5).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecoveryOutcome {
    /// Monotonic per-store Core start counter.
    pub boot_generation: u64,
    /// Highest offset in the log.
    pub last_offset: u64,
    /// Events whose hash chain was re-verified.
    pub events_verified: u64,
    /// Aggregates verified.
    pub aggregates_verified: u64,
    /// Whether projections had to be rebuilt from the log.
    pub projections_rebuilt: bool,
    /// Session rows.
    pub sessions: u64,
    /// Task rows.
    pub tasks: u64,
    /// Human-readable notes about what recovery had to repair.
    pub notes: Vec<String>,
    /// Wall time spent.
    pub recovery_ms: u64,
}

/// A command's identity for the idempotency ledger.
#[derive(Clone, Debug)]
pub struct CommandRecord {
    /// Stable command id chosen by the client.
    pub command_id: EventId,
    /// Tenant scope.
    pub tenant_id: TenantId,
    /// Command type name.
    pub command_type: String,
    /// Hash of the full request (payload + target), so a reused id with a
    /// different request is detected.
    pub request_hash: String,
}

/// Result of an idempotent command execution.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CommandOutcome {
    /// Executed now; these events were appended.
    Applied(Vec<StoredEvent>),
    /// Already executed earlier; these are the events it appended then.
    Replayed(Vec<StoredEvent>),
}

/// Events with `offset` in `[first, last]`.
fn read_range(conn: &Connection, first: u64, last: u64) -> Result<Vec<StoredEvent>> {
    let mut stmt = conn.prepare_cached(&format!(
        "SELECT {COLUMNS} FROM events WHERE offset >= ?1 AND offset <= ?2 ORDER BY offset ASC"
    ))?;
    let rows = stmt.query_map(params![first as i64, last as i64], row_to_event)?;
    rows.map(|r| r.map_err(Error::from)).collect()
}

/// Every event with `offset > after`, ascending.
pub(crate) fn read_all_from(conn: &Connection, after: u64) -> Result<Vec<StoredEvent>> {
    let mut stmt = conn.prepare_cached(&format!(
        "SELECT {COLUMNS} FROM events WHERE offset > ?1 ORDER BY offset ASC"
    ))?;
    let rows = stmt.query_map(params![after as i64], row_to_event)?;
    rows.map(|r| r.map_err(Error::from)).collect()
}

fn append_in(
    tx: &rusqlite::Transaction<'_>,
    objects: &ObjectStore,
    req: AppendRequest,
) -> Result<Vec<StoredEvent>> {
    let (current, mut previous_hash) = {
        let row: Option<(i64, String)> = tx
                .query_row(
                    "SELECT sequence, integrity_hash FROM events WHERE aggregate_id = ?1 ORDER BY sequence DESC LIMIT 1",
                    params![req.aggregate_id.as_slice()],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .optional()?;
        row.map(|(s, h)| (s as u64, h))
            .unwrap_or((0, String::new()))
    };
    if let Some(expected) = req.expected_sequence
        && expected != current
    {
        return Err(Error::SequenceConflict {
            aggregate: hex::encode(req.aggregate_id),
            expected,
            actual: current,
        });
    }
    let mut out = Vec::with_capacity(req.events.len());
    for (sequence, ev) in (current + 1..).zip(req.events) {
        let payload_text = ev.payload.to_string();
        let payload = if payload_text.len() > INLINE_PAYLOAD_CEILING {
            let hash = objects.put(payload_text.as_bytes())?;
            PayloadRef::Object {
                object_hash: hash,
                byte_length: payload_text.len() as u64,
            }
        } else {
            PayloadRef::Inline {
                payload: ev.payload,
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
            event_type: ev.event_type,
            schema_version: modbit_domain::SCHEMA_VERSION,
            occurred_at: ev.occurred_at.unwrap_or_else(Timestamp::now),
            actor: ev.actor,
            causation_id: ev.causation_id,
            correlation_id: ev.correlation_id,
            payload,
            integrity_hash: String::new(),
        };
        env.integrity_hash = chain_hash(&previous_hash, &env);
        previous_hash = env.integrity_hash.clone();
        let (actor_type, actor_id) = actor_columns(&env.actor);
        let (inline, object_hash, byte_length) = match &env.payload {
            PayloadRef::Inline { payload } => (Some(payload.to_string()), None, None),
            PayloadRef::Object {
                object_hash,
                byte_length,
            } => (None, Some(object_hash.clone()), Some(*byte_length as i64)),
        };
        tx.execute(
                "INSERT INTO events (event_id, tenant_id, session_id, task_id, run_id, turn_id, step_id, aggregate_type, aggregate_id, sequence, event_type, schema_version, occurred_at, actor_type, actor_id, causation_id, correlation_id, payload_inline, payload_object_hash, payload_byte_length, integrity_hash)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21)",
                params![
                    env.event_id.as_bytes().as_slice(),
                    env.tenant_id.as_bytes().as_slice(),
                    env.session_id.as_bytes().as_slice(),
                    opt_blob(env.task_id.map(|x| *x.as_bytes())),
                    opt_blob(env.run_id.map(|x| *x.as_bytes())),
                    opt_blob(env.turn_id.map(|x| *x.as_bytes())),
                    opt_blob(env.step_id.map(|x| *x.as_bytes())),
                    env.aggregate_type.as_str(),
                    env.aggregate_id.as_slice(),
                    env.sequence as i64,
                    &env.event_type,
                    env.schema_version,
                    env.occurred_at.millis(),
                    actor_type,
                    actor_id,
                    opt_blob(env.causation_id.map(|x| *x.as_bytes())),
                    opt_blob(env.correlation_id.map(|x| *x.as_bytes())),
                    inline,
                    object_hash,
                    byte_length,
                    &env.integrity_hash,
                ],
            )?;
        let offset = tx.last_insert_rowid() as u64;
        let stored = StoredEvent {
            offset,
            envelope: env,
        };
        crate::projections::apply(tx, &stored, objects)?;
        out.push(stored);
    }
    Ok(out)
}

const COLUMNS: &str = "offset, event_id, tenant_id, session_id, task_id, run_id, turn_id, step_id, aggregate_type, aggregate_id, sequence, event_type, schema_version, occurred_at, actor_type, actor_id, causation_id, correlation_id, payload_inline, payload_object_hash, payload_byte_length, integrity_hash";

fn actor_columns(a: &Actor) -> (&'static str, String) {
    match a {
        Actor::User(id) => ("user", id.to_string()),
        Actor::Core(s) => ("core", s.clone()),
        Actor::Agent(s) => ("agent", s.clone()),
        Actor::External(s) => ("external", s.clone()),
    }
}

fn actor_from(kind: &str, id: String) -> rusqlite::Result<Actor> {
    Ok(match kind {
        "user" => Actor::User(
            modbit_domain::UserId::parse(&id).map_err(|_| rusqlite::Error::InvalidQuery)?,
        ),
        "core" => Actor::Core(id),
        "agent" => Actor::Agent(id),
        "external" => Actor::External(id),
        _ => return Err(rusqlite::Error::InvalidQuery),
    })
}

fn opt_id<T: From<[u8; 16]>>(v: Option<Vec<u8>>) -> rusqlite::Result<Option<T>> {
    v.map(|b| blob16(b).map(T::from)).transpose()
}

macro_rules! id_from_bytes {
    ($($t:ty),*) => {$(
        impl From<[u8; 16]> for IdWrap<$t> {
            fn from(b: [u8; 16]) -> Self { IdWrap(<$t>::from_bytes(b)) }
        }
    )*};
}
struct IdWrap<T>(T);
id_from_bytes!(TaskId, RunId, TurnId, RunStepId, EventId);

fn row_to_event(r: &rusqlite::Row<'_>) -> rusqlite::Result<StoredEvent> {
    let offset: i64 = r.get(0)?;
    let aggregate_type =
        AggregateType::parse(&r.get::<_, String>(8)?).ok_or(rusqlite::Error::InvalidQuery)?;
    let inline: Option<String> = r.get(18)?;
    let object_hash: Option<String> = r.get(19)?;
    let byte_length: Option<i64> = r.get(20)?;
    let payload = match (inline, object_hash) {
        (Some(text), None) => PayloadRef::Inline {
            payload: serde_json::from_str(&text).map_err(|_| rusqlite::Error::InvalidQuery)?,
        },
        (None, Some(hash)) => PayloadRef::Object {
            object_hash: hash,
            byte_length: byte_length.unwrap_or(0) as u64,
        },
        _ => return Err(rusqlite::Error::InvalidQuery),
    };
    Ok(StoredEvent {
        offset: offset as u64,
        envelope: EventEnvelope {
            event_id: EventId::from_bytes(blob16(r.get(1)?)?),
            tenant_id: TenantId::from_bytes(blob16(r.get(2)?)?),
            session_id: SessionId::from_bytes(blob16(r.get(3)?)?),
            task_id: opt_id::<IdWrap<TaskId>>(r.get(4)?)?.map(|w| w.0),
            run_id: opt_id::<IdWrap<RunId>>(r.get(5)?)?.map(|w| w.0),
            turn_id: opt_id::<IdWrap<TurnId>>(r.get(6)?)?.map(|w| w.0),
            step_id: opt_id::<IdWrap<RunStepId>>(r.get(7)?)?.map(|w| w.0),
            aggregate_type,
            aggregate_id: blob16(r.get(9)?)?,
            sequence: r.get::<_, i64>(10)? as u64,
            event_type: r.get(11)?,
            schema_version: r.get(12)?,
            occurred_at: Timestamp(r.get(13)?),
            actor: actor_from(&r.get::<_, String>(14)?, r.get(15)?)?,
            causation_id: opt_id::<IdWrap<EventId>>(r.get(16)?)?.map(|w| w.0),
            correlation_id: opt_id::<IdWrap<EventId>>(r.get(17)?)?.map(|w| w.0),
            payload,
            integrity_hash: r.get(21)?,
        },
    })
}

#[cfg(test)]
mod tests {
    #[test]
    fn sha_helper_is_stable() {
        assert_eq!(
            crate::objects::sha256_hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }
}
