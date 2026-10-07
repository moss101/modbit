//! `modbit-effects` — receipts, hash chain and evidence references.
//!
//! Canonical owner: effects-security (`docs/12_REPOSITORY_AND_MODULE_LAYOUT.md`,
//! `docs/81_ARCHITECTURE_GUARDRAILS_AND_FORBIDDEN_DUPLICATION.md`).
//! Dependency direction is enforced by `tools/architecture-lint`.
//!
//! Where the protected-effect receipt chain actually lives today (FIX-08):
//! `modbit_policy::ledger` owns the receipt hash, seal and chain verification
//! (including the authorization/result pair and `in_doubt`), and
//! `modbit_event_store::EventStore::append_all_chained` links each receipt to
//! the chain's tail inside the append transaction. This stub is not a second
//! implementation of either and must not become one.
//!
//! This crate is created by milestone task M0.1 and carries no behavior yet.
//! Behavior arrives only through the graph-scheduled tasks that name this
//! crate as owner; nothing here may be read as an implemented feature.
