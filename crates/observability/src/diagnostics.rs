//! The diagnostics package (IMP-EV-0142, REQ-EV-0142; docs/71 "Desktop
//! diagnostics package"): what a person or an operator exports to explain
//! a Core — build, health, integrity, the event ranges and chain heads that
//! pin the log, a metadata-only trace, recent error codes, provider health,
//! lease states and checksums. Source, prompts and payloads are left out
//! unless asked for; secrets never go in (the host redacts the whole package
//! before it is sealed).
//!
//! A package *replays*: [`verify`] checks its digest and every aggregate's
//! exported head against a log, so the same metadata proves the same
//! history on this machine later, after a restart, or on another copy of
//! the log.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// The schema this module writes.
pub const SCHEMA: &str = "modbit.diagnostics/1";

/// A diagnostics package.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Package {
    /// [`SCHEMA`].
    pub schema: String,
    /// When it was made (ms since the epoch).
    pub generated_at_ms: i64,
    /// What produced it.
    pub build: Build,
    /// The Core's health when it was made.
    pub health: Health,
    /// The store's and the chains' integrity when it was made.
    pub integrity: Integrity,
    /// What it covers.
    pub scope: Scope,
    /// Every aggregate in scope: its range and chain head.
    pub aggregates: Vec<AggregateRange>,
    /// What happened, as metadata: no payloads.
    pub trace: Vec<TraceLine>,
    /// Whether the trace was cut to its bound (the earliest lines dropped).
    pub trace_truncated: bool,
    /// Failures in scope, as class and code.
    pub recent_errors: Vec<ErrorCode>,
    /// The provider endpoints' health, without credentials.
    pub providers: Vec<ProviderHealth>,
    /// Sandbox, capacity and browser lease states.
    pub leases: Leases,
    /// Hashes of the objects the scope's events reference.
    pub object_refs: Vec<String>,
    /// Event payloads, only when the person asked for content.
    pub content: Option<Vec<ContentEvent>>,
    /// Secrets the host replaced before sealing.
    pub redactions: u64,
    /// sha256 over the package with this field empty.
    pub digest: String,
}

/// What produced a package.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Build {
    /// The Core's version.
    pub core_version: String,
    /// The surface protocol's version.
    pub protocol_version: String,
    /// Operating system.
    pub os: String,
    /// CPU architecture.
    pub arch: String,
    /// The store's boot generation.
    pub boot_generation: u64,
}

/// A Core's health.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Health {
    /// When the Core started (ms since the epoch).
    pub started_at_ms: i64,
    /// How long it has run.
    pub uptime_ms: u64,
    /// The store's last offset.
    pub last_offset: u64,
    /// What startup recovery verified and noted.
    pub recovery: Vec<String>,
}

/// Integrity when the package was made.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Integrity {
    /// SQLite's `integrity_check`: `ok` or what it found.
    pub database: String,
    /// Aggregates whose hash chains were recomputed and matched.
    pub chains_verified: u64,
    /// Aggregates whose chains did not verify, with why.
    pub chain_failures: Vec<String>,
    /// The protected-effect receipt chain: `valid` or why not.
    pub receipts: String,
}

/// What a package covers.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Scope {
    /// The session.
    pub session_id: String,
    /// The task, when the export was narrowed to one.
    pub task_id: Option<String>,
    /// The tasks whose events are in scope.
    pub task_ids: Vec<String>,
}

/// One aggregate's range in scope and its chain head.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AggregateRange {
    /// Aggregate type.
    pub aggregate_type: String,
    /// Aggregate id (hex).
    pub aggregate_id: String,
    /// First sequence in scope.
    pub first_sequence: u64,
    /// Last sequence in scope.
    pub last_sequence: u64,
    /// First store offset.
    pub first_offset: u64,
    /// Last store offset.
    pub last_offset: u64,
    /// Events in scope.
    pub count: u64,
    /// The chain hash of the event at `last_sequence`.
    pub head_hash: String,
}

/// One event as metadata.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TraceLine {
    /// Store offset.
    pub offset: u64,
    /// Aggregate type.
    pub aggregate_type: String,
    /// Sequence within the aggregate.
    pub sequence: u64,
    /// Event type.
    pub event_type: String,
    /// Task, when the event has one.
    pub task_id: Option<String>,
    /// Run, when the event has one.
    pub run_id: Option<String>,
    /// A failure's code, when the event is one.
    pub code: Option<String>,
}

/// One failure in scope.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ErrorCode {
    /// Store offset.
    pub offset: u64,
    /// Event type.
    pub event_type: String,
    /// Failure class, when classified.
    pub class: String,
    /// Code.
    pub code: String,
    /// Whether a retry can help, when classified.
    pub retryable: Option<bool>,
    /// Task.
    pub task_id: Option<String>,
}

/// A provider endpoint's health, without its credential.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderHealth {
    /// Endpoint name.
    pub name: String,
    /// Wire family.
    pub kind: String,
    /// The endpoint's host (no path, query or user info).
    pub host: String,
    /// Whether a credential is configured (never the value).
    pub credential_configured: bool,
    /// Models served.
    pub models: Vec<String>,
    /// Requests started.
    pub requests: u64,
    /// Completed normally.
    pub successes: u64,
    /// Ended in error.
    pub failures: u64,
    /// Interrupted mid-stream.
    pub interruptions: u64,
    /// Rate-limit responses.
    pub rate_limited: u64,
    /// Last first-token latency.
    pub last_first_token_ms: Option<u64>,
}

/// Lease states.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Leases {
    /// Capacity tickets held: holder and what it holds.
    pub capacity: Vec<String>,
    /// Tasks holding a sandbox.
    pub sandboxes: Vec<String>,
}

/// An event with its payload (content opt-in only).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ContentEvent {
    /// Store offset.
    pub offset: u64,
    /// Event type.
    pub event_type: String,
    /// The payload, redacted.
    pub payload: serde_json::Value,
}

impl Package {
    /// The digest this package should carry.
    #[must_use]
    pub fn computed_digest(&self) -> String {
        let mut unsealed = self.clone();
        unsealed.digest = String::new();
        let bytes = serde_json::to_vec(&unsealed).unwrap_or_default();
        hex::encode(Sha256::digest(&bytes))
    }

    /// Set the digest.
    #[must_use]
    pub fn sealed(mut self) -> Self {
        self.digest = self.computed_digest();
        self
    }
}

/// What a replay of a package against a log found.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Verification {
    /// The package is what was sealed.
    pub digest_ok: bool,
    /// Aggregates checked against the log.
    pub aggregates_checked: u64,
    /// What did not match.
    pub mismatches: Vec<String>,
}

impl Verification {
    /// Everything matched.
    #[must_use]
    pub fn verified(&self) -> bool {
        self.digest_ok && self.mismatches.is_empty()
    }
}

/// Replay a package's evidence metadata against a log. `head_of(aggregate,
/// sequence)` is the log's chain hash of that aggregate's event at that
/// sequence (`None` when it has none); `chain_ok(aggregate)` recomputes the
/// aggregate's whole chain in the log.
pub fn verify(
    p: &Package,
    head_of: impl Fn(&str, u64) -> Option<String>,
    chain_ok: impl Fn(&str) -> Result<(), String>,
) -> Verification {
    let mut v = Verification {
        digest_ok: p.schema == SCHEMA && p.digest == p.computed_digest(),
        ..Verification::default()
    };
    if p.schema != SCHEMA {
        v.mismatches
            .push(format!("schema `{}` is not {SCHEMA}", p.schema));
    }
    for a in &p.aggregates {
        v.aggregates_checked += 1;
        if let Err(e) = chain_ok(&a.aggregate_id) {
            v.mismatches.push(format!(
                "{} {}: the log's chain does not verify: {e}",
                a.aggregate_type, a.aggregate_id
            ));
            continue;
        }
        match head_of(&a.aggregate_id, a.last_sequence) {
            Some(h) if h == a.head_hash => {}
            Some(h) => v.mismatches.push(format!(
                "{} {} at {}: the log's head is {h}, the package says {}",
                a.aggregate_type, a.aggregate_id, a.last_sequence, a.head_hash
            )),
            None => v.mismatches.push(format!(
                "{} {} at {}: not in the log",
                a.aggregate_type, a.aggregate_id, a.last_sequence
            )),
        }
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    fn package() -> Package {
        Package {
            schema: SCHEMA.into(),
            aggregates: vec![AggregateRange {
                aggregate_type: "Task".into(),
                aggregate_id: "aa".into(),
                first_sequence: 1,
                last_sequence: 3,
                head_hash: "h3".into(),
                ..AggregateRange::default()
            }],
            ..Package::default()
        }
        .sealed()
    }

    #[test]
    fn a_sealed_package_replays_against_its_log_and_a_changed_one_does_not() {
        let p = package();
        let log = |_: &str, s: u64| (s == 3).then(|| "h3".to_owned());
        let ok = |_: &str| Ok(());
        assert!(verify(&p, log, ok).verified());
        // Tampered after sealing.
        let mut t = p.clone();
        t.aggregates[0].head_hash = "forged".into();
        let v = verify(&t, log, ok);
        assert!(!v.digest_ok && !v.verified(), "{v:?}");
        // Resealed to hide it: the log still disagrees.
        let v = verify(&t.clone().sealed(), log, ok);
        assert!(v.digest_ok && v.mismatches.len() == 1, "{v:?}");
        // A log whose chain no longer verifies.
        let v = verify(&p, log, |_: &str| Err("hash mismatch at 2".into()));
        assert!(
            !v.verified() && v.mismatches[0].contains("does not verify"),
            "{v:?}"
        );
        // A round trip through JSON keeps the digest.
        let back: Package = serde_json::from_str(&serde_json::to_string(&p).unwrap()).unwrap();
        assert!(verify(&back, log, ok).verified());
    }
}
