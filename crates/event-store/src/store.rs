//! The SQLite-backed store.

use std::path::{Path, PathBuf};

use modbit_domain::event::{Actor, AggregateType, EventEnvelope, PayloadRef};
use modbit_domain::{EventId, RunId, RunStepId, SessionId, TaskId, TenantId, Timestamp, TurnId};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use sha2::{Digest, Sha256};

use crate::objects::ObjectStore;
use crate::schema::{INLINE_PAYLOAD_CEILING, SCHEMA_VERSION, V1};
use crate::{Error, Result};

/// A stored event with its store-wide offset.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StoredEvent {
    /// Store-wide monotonic offset (resume cursor).
    pub offset: u64,
    /// The envelope.
    pub envelope: EventEnvelope,
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

/// The canonical Event Store.
pub struct EventStore {
    conn: Connection,
    objects: ObjectStore,
    db_path: PathBuf,
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

fn opt_blob(id: Option<[u8; 16]>) -> Option<Vec<u8>> {
    id.map(|b| b.to_vec())
}

fn blob16(bytes: Vec<u8>) -> rusqlite::Result<[u8; 16]> {
    bytes.try_into().map_err(|_| rusqlite::Error::InvalidQuery)
}

impl EventStore {
    /// Open (creating) the store rooted at `dir`: `core.db` plus `objects/`.
    pub fn open(dir: &Path) -> Result<Self> {
        std::fs::create_dir_all(dir)?;
        let db_path = dir.join("core.db");
        let conn = Connection::open(&db_path)?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "FULL")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        conn.execute_batch(V1)?;
        let found: Option<String> = conn
            .query_row(
                "SELECT value FROM schema_meta WHERE key = 'schema_version'",
                [],
                |r| r.get(0),
            )
            .optional()?;
        match found {
            None => {
                conn.execute(
                    "INSERT INTO schema_meta (key, value) VALUES ('schema_version', ?1)",
                    params![SCHEMA_VERSION.to_string()],
                )?;
            }
            Some(v) => {
                let found: u32 = v.parse().unwrap_or(u32::MAX);
                if found > SCHEMA_VERSION {
                    return Err(Error::SchemaTooNew {
                        found,
                        supported: SCHEMA_VERSION,
                    });
                }
            }
        }
        let objects = ObjectStore::open(dir.join("objects"))?;
        Ok(Self {
            conn,
            objects,
            db_path,
        })
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

    /// Append events to one aggregate in a single transaction.
    pub fn append(&mut self, req: AppendRequest) -> Result<Vec<StoredEvent>> {
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
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
                let hash = self.objects.put(payload_text.as_bytes())?;
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
            out.push(StoredEvent {
                offset,
                envelope: env,
            });
        }
        tx.commit()?;
        Ok(out)
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
