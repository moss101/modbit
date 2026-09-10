//! `modbit-observability` — outcome and usage projections over the canonical
//! log, and the fixed-revision baseline they are published as.
//!
//! Canonical owner: observability (`docs/12_REPOSITORY_AND_MODULE_LAYOUT.md`,
//! `docs/81_ARCHITECTURE_GUARDRAILS_AND_FORBIDDEN_DUPLICATION.md`).
//! Dependency direction is enforced by `tools/architecture-lint`.
//!
//! The baseline (REQ-EPR-000, docs/27 §22) is what any later routing change is
//! measured against, so it is pinned rather than remembered: the repository
//! revision, the build and the environment it was produced on, and, per task,
//! what the work cost and whether it was verified. Unknown is carried as
//! unknown — a stream that dropped before its usage frame leaves the cost
//! unknown, and the bundle says so instead of writing a zero.

pub mod baseline;

pub use baseline::{
    BaselineBundle, Interventions, TaskOutcome, Usage, bundle_digest, digest_of, publish,
};
