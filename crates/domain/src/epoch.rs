//! `AuthorizationEpoch` and `CapabilitySnapshot` (REQ-PX-131, docs/23
//! "Policy generations"): the capability view of one model round, frozen when
//! the round begins, with the monotonic epoch that every kernel decision and
//! receipt of that round carries.
//!
//! The types are plain data; the Core computes the snapshot at the round
//! boundary and the policy crate hashes and verifies it. Nothing here reads a
//! clock or a store.

use serde::{Deserialize, Serialize};

/// What a decision or a receipt says about the authority it ran under: the
/// round's epoch and the digest of the snapshot that epoch froze.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthorizationStamp {
    /// The monotonic epoch of the round (per task, from 1).
    pub epoch: u64,
    /// SHA-256 (hex) of the canonical snapshot the epoch froze.
    pub snapshot_hash: String,
}

/// The capability lease as the round saw it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LeaseSnapshot {
    /// The lease presented.
    pub lease_id: String,
    /// Its generation.
    pub generation: u64,
    /// The highest effect class it can authorize.
    pub effect_ceiling: String,
    /// Its operations, sorted.
    pub operations: Vec<String>,
    /// SHA-256 (hex) of its sorted resource selectors.
    pub resources_digest: String,
}

/// The capability view frozen for one model round.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapabilitySnapshot {
    /// The round's epoch: monotonic per task, from 1, never reused. A
    /// restarted Core continues the sequence from the log.
    pub epoch: u64,
    /// The run the round belongs to.
    pub run_id: String,
    /// The round's turn.
    pub turn_id: String,
    /// The turn's ordinal in the run (1-based).
    pub round: u32,
    /// The configuration generation the round is decided under
    /// (`modbit_policy::config::generation`).
    pub config_generation: String,
    /// Object hash of the resolved configuration, so a restarted Core decides
    /// the rest of an interrupted round under the configuration it began
    /// with, not whatever the files say now.
    pub config_ref: String,
    /// The mode posture in force (`AGENT` | `PLAN` | `READ_ONLY` ...).
    pub mode: String,
    /// The execution profile.
    pub execution_profile: String,
    /// The lease the task held at the boundary; absent when it holds none.
    pub lease: Option<LeaseSnapshot>,
    /// The tools projected to the model this round, sorted.
    pub projected_tools: Vec<String>,
    /// The tool-projection digest the request carried.
    pub projection_hash: String,
    /// Skills selected for the round (ids), sorted.
    pub skills: Vec<String>,
}
