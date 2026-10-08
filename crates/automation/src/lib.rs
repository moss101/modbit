//! Modbit automation (DR-PX-2026-10-03-010, docs/68): definitions of work
//! that starts by itself, as data.
//!
//! This crate holds the document model and every pure rule about it: the
//! definition and its validation and hash, cron and interval schedules and
//! the missed-run arithmetic, trigger filters, the signed-webhook check, and
//! the ledger events with the fold that turns them into state. It runs
//! nothing. The Scheduler, the policy kernel, the approval aggregate and the
//! effect ledger keep their single owners (docs/81); a trigger only ever
//! results in an ordinary `CreateTask` and `StartTask` issued by the Core
//! for a principal under a ceiling.

pub mod cron;
pub mod definition;
pub mod filter;
pub mod registry;
pub mod schedule;
pub mod webhook;

pub use definition::{
    Definition, Issue, SCHEMA, parse, parse_and_validate, resolve_inputs, sha256_hex, validate,
};
pub use registry::{AutomationEvent, Registry, RunStatus, Source, dispatch_key};
