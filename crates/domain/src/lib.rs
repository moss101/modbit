//! `modbit-domain` — IDs, domain objects and state transitions; depends on no infrastructure crate.
//!
//! Canonical owner: core-runtime owner (`docs/12_REPOSITORY_AND_MODULE_LAYOUT.md`).
//! Authority: `docs/13_DOMAIN_MODEL_AND_STATE_MACHINES.md` (identity types,
//! aggregates, state machines, canonical event envelope, fencing) and
//! `docs/30_PROTOCOL_APIS_AND_EVENT_SCHEMAS.md` (event type names).
//!
//! Everything here is pure: no I/O, no clocks other than [`Timestamp::now`],
//! no persistence. Only the Event Store append plus transactional projection
//! update may advance authoritative state (docs/13 "Invariants"); this crate
//! supplies the types and the transition rules those components enforce.

#![forbid(unsafe_code)]

pub mod approval;
pub mod event;
pub mod ids;
pub mod lease;
pub mod media;
pub mod run;
pub mod session;
pub mod state;
pub mod step;
pub mod task;
pub mod time;
pub mod toolcall;
pub mod turn;
pub mod workspace;

pub use event::{Actor, AggregateType, EventEnvelope, PayloadRef, SCHEMA_VERSION};
pub use ids::*;
pub use state::InvalidTransition;
pub use time::Timestamp;
