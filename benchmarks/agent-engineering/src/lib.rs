//! Modbit Eval Harness (docs/12 `benchmarks/agent-engineering`, docs/63).
//!
//! The fixed competence baseline of `PX-020`: the internal competence suite
//! and the public benchmark slice run under a frozen protocol on the real
//! product — real Core, real tools, real verification, real repair loop —
//! through the headless CLI, and the immutable baseline bundle is recorded
//! and referenced by digest. Nothing here relaxes a product contract for
//! the benchmark: no benchmark prompt, tool or policy exists on the Core's
//! side, and DI-3 test integrity is scored again by the harness on top of
//! the engine's own enforcement.
//!
//! The library is pure (suite loading and digests, event scoring, metrics
//! with intervals, bundle validation); the `competence-baseline` binary
//! drives the product.

pub mod bundle;
pub mod events;
pub mod profile;
pub mod suite;
pub mod workspace;

pub use bundle::{
    Bundle, BundleRefused, Economics, Environment, EventLog, Metrics, Protocol, Rate, TargetRecord,
    TrialOutcome, digest_of, metrics,
};
pub use events::{Counts, Event, count, parse_events};
pub use profile::{
    HarnessProfile, KNOWN_GOOD, MAX_PROFILE_REPAIRS, ProfileRefused, generate, repair,
};
pub use suite::{Acceptance, FileOp, HiddenFile, Suite, SuiteProtocol, TaskSpec, protected_intact};
pub use workspace::copy_fixture;

/// Harness version recorded in every bundle (docs/63 "harness version").
pub const HARNESS_VERSION: &str = "competence-baseline/1";

/// SHA-256 of bytes as lowercase hex.
#[must_use]
pub fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::Digest;
    hex::encode(sha2::Sha256::digest(bytes))
}
