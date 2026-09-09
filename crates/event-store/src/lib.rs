//! `modbit-event-store` — append-only canonical event store (durability layer 1,
//! docs/19) backed by SQLite in WAL mode with `synchronous=FULL` and
//! `foreign_keys=ON` (docs/31 "Local storage"), plus a content-addressed object
//! directory for payloads above the inline ceiling.
//!
//! Canonical owner: core-runtime (`docs/12`); this crate is THE implementation of
//! the doc 81 `event-state` system.
//!
//! Guarantees (docs/13 "Invariants", docs/31 `events`):
//! - every append is one SQLite transaction; sequences are contiguous per
//!   aggregate and unique `(aggregate_id, sequence)` is enforced by the schema;
//! - optimistic concurrency: a caller may require the aggregate's current
//!   sequence, and a stale expectation is rejected, recorded nowhere, and never
//!   applied silently;
//! - every event carries `integrity_hash = sha256(previous_hash || canonical
//!   content)` per aggregate, so tampering or a torn write is detectable by
//!   [`EventStore::verify_aggregate`];
//! - a store-wide monotonic `offset` lets clients resume a session stream from
//!   the last offset they saw (REQ-EV-0010).
//!
//! M1.2 adds, in this crate: a checksummed migration ledger (`migrations`),
//! projections of the five core aggregates updated in the same transaction as
//! the append and rebuildable from the log (`projections`), and a command
//! idempotency ledger so a retried command with the same `command_id` replays
//! its recorded outcome instead of appending again (docs/30, docs/33).

#![forbid(unsafe_code)]

pub mod migrations;
pub mod objects;
pub mod projections;
pub mod schema;
mod store;

pub use migrations::MigrationReport;
pub use objects::ObjectStore;
pub use store::{AppendRequest, CommandOutcome, CommandRecord, EventStore, NewEvent, StoredEvent};

/// Errors from the store.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// SQLite failure.
    #[error("sqlite: {0}")]
    Sqlite(#[from] rusqlite::Error),
    /// Filesystem failure.
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    /// JSON failure.
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
    /// The caller's expected sequence does not match the aggregate's current sequence.
    #[error("sequence conflict on {aggregate}: expected {expected}, actual {actual}")]
    SequenceConflict {
        /// Aggregate id (hex).
        aggregate: String,
        /// Expected current sequence.
        expected: u64,
        /// Actual current sequence.
        actual: u64,
    },
    /// Stored chain does not verify.
    #[error("integrity failure on {aggregate} at sequence {sequence}: {detail}")]
    Integrity {
        /// Aggregate id (hex).
        aggregate: String,
        /// Sequence where verification failed.
        sequence: u64,
        /// What was wrong.
        detail: String,
    },
    /// The database schema is newer than this build supports (docs/31 "Migration safety").
    #[error(
        "database schema version {found} is newer than supported {supported}; refusing write mode"
    )]
    SchemaTooNew {
        /// Found version.
        found: u32,
        /// Supported version.
        supported: u32,
    },
    /// A projection could not apply an event (illegal transition in the log).
    #[error("projection failure at offset {offset}: {detail}")]
    Projection {
        /// Store offset of the offending event.
        offset: u64,
        /// What was wrong.
        detail: String,
    },
    /// A command id was reused with a different request.
    #[error("command {command_id} was already accepted with a different request hash")]
    IdempotencyConflict {
        /// Command id (hex).
        command_id: String,
    },
    /// An object referenced by an event is missing or corrupt.
    #[error("object {hash} {detail}")]
    Object {
        /// Digest.
        hash: String,
        /// Problem.
        detail: String,
    },
}

/// Result alias.
pub type Result<T> = std::result::Result<T, Error>;
