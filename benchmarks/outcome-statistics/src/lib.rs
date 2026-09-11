//! Outcome statistics (REQ-EPR-015, docs/27 and docs/38): immutable,
//! versioned snapshots of what actually happened, keyed exactly, and joined by
//! a pinned `stats_version`.
//!
//! This is the dataset the Model Registry deliberately does not hold. The
//! registry says what exists and what it costs today; this says what was
//! observed, and it is a separate version so that changing one cannot rewrite
//! the other. A compiler pins both and joins them independently.
//!
//! Three rules keep a snapshot honest.
//!
//! Counts are derived, never declared. A caller supplies attributable samples
//! and the snapshot counts them; a record that claims a sample count its
//! samples do not support is refused.
//!
//! The same outcome counted twice is still one outcome. Samples are identified
//! by the outcome they came from, so replaying events cannot inflate a mean.
//!
//! A sparse key stays low-confidence. The interval is a Wilson score interval,
//! which stays wide when `n` is small, and a key below the sample threshold is
//! marked low-confidence so nothing can qualify a cheap plan on three
//! observations.

#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// The schema a snapshot is written at.
pub const STATS_SCHEMA_VERSION: u32 = 1;

/// Observations below this count are low-confidence whatever their mean.
pub const MIN_CONFIDENT_SAMPLES: u32 = 30;

/// What this dataset is, carried with it.
pub const NOTE: &str = "Observations, not predictions. A mean with a wide interval is a wide interval: a key below the sample threshold is low-confidence and must not qualify a cheaper plan. The Model Registry holds no part of this, and a new registry generation does not rewrite an observation.";

/// The exact keys observations are grouped under (docs/38: solver, joint
/// escalation, reviewer family and revision finding).
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum StatKey {
    /// What a solver did, with the Skill and harness it did it under.
    Solver {
        /// Model id.
        model: String,
        /// Skill that ran, or `none`.
        skill: String,
        /// Harness identity.
        harness: String,
    },
    /// What an escalation did: the joint key of where it came from, where it
    /// went, the gate that asked for it, the repository and the verification.
    Escalation {
        /// Model that was rejected.
        from_model: String,
        /// Model that was escalated to.
        to_model: String,
        /// Acceptance gate that rejected.
        gate: String,
        /// Repository the work was in.
        repository: String,
        /// Verification that judged it.
        verification: String,
    },
    /// What a reviewer family did.
    Reviewer {
        /// Model family.
        family: String,
    },
    /// What a revision did about one class of finding.
    Revision {
        /// Finding class.
        finding_class: String,
    },
}

impl StatKey {
    /// The stable id this key joins on.
    #[must_use]
    pub fn key_id(&self) -> String {
        match self {
            Self::Solver {
                model,
                skill,
                harness,
            } => format!("solver|{model}|{skill}|{harness}"),
            Self::Escalation {
                from_model,
                to_model,
                gate,
                repository,
                verification,
            } => format!("escalation|{from_model}|{to_model}|{gate}|{repository}|{verification}"),
            Self::Reviewer { family } => format!("reviewer|{family}"),
            Self::Revision { finding_class } => format!("revision|{finding_class}"),
        }
    }

    /// The components this key requires: a joint key with a missing gate or
    /// verification is not a key, it is a guess.
    fn missing_component(&self) -> Option<&'static str> {
        let empty = |s: &String| s.trim().is_empty();
        match self {
            Self::Solver {
                model,
                skill,
                harness,
            } => {
                if empty(model) {
                    Some("model")
                } else if empty(skill) {
                    Some("skill")
                } else if empty(harness) {
                    Some("harness")
                } else {
                    None
                }
            }
            Self::Escalation {
                from_model,
                to_model,
                gate,
                repository,
                verification,
            } => {
                if empty(from_model) {
                    Some("from_model")
                } else if empty(to_model) {
                    Some("to_model")
                } else if empty(gate) {
                    Some("gate")
                } else if empty(repository) {
                    Some("repository")
                } else if empty(verification) {
                    Some("verification")
                } else {
                    None
                }
            }
            Self::Reviewer { family } => empty(family).then_some("family"),
            Self::Revision { finding_class } => empty(finding_class).then_some("finding_class"),
        }
    }
}

/// One attributable observation.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Sample {
    /// The outcome this came from, unique within the dataset. The same
    /// outcome observed again is the same sample.
    pub outcome_id: String,
    /// The key it is counted under.
    pub key: StatKey,
    /// Whether the outcome succeeded, by the verification's own verdict.
    pub success: bool,
    /// Cost in minor units, when the provider reported enough to know it.
    pub cost_minor: Option<u64>,
    /// Wall time, in milliseconds.
    pub wall_ms: u64,
    /// The digest of the baseline bundle this outcome was attributed from.
    pub attributed_to: String,
}

/// What a key's observations add up to.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Aggregate {
    /// Join id.
    pub key_id: String,
    /// The key.
    pub key: StatKey,
    /// Observations counted.
    pub samples: u32,
    /// Of those, the ones that succeeded.
    pub successes: u32,
    /// Success mean.
    pub mean: f64,
    /// Wilson score interval at 95%, which stays wide when `n` is small.
    pub interval: (f64, f64),
    /// Mean cost of the observations that had a known cost.
    pub mean_cost_minor: Option<u64>,
    /// Observations whose cost the provider never reported.
    pub unknown_cost_samples: u32,
    /// Mean wall time.
    pub mean_wall_ms: u64,
    /// Below the sample threshold: a prior, not evidence.
    pub low_confidence: bool,
}

impl Aggregate {
    /// Whether this observation may qualify a cheaper plan: enough samples,
    /// and a lower bound that clears the floor. A high mean over three
    /// observations qualifies nothing.
    #[must_use]
    pub fn qualifies(&self, floor: f64) -> bool {
        !self.low_confidence && self.interval.0 >= floor
    }
}

/// An immutable snapshot, addressed by its own digest.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Snapshot {
    /// Schema.
    pub schema_version: u32,
    /// The version a compiler pins.
    pub stats_version: String,
    /// The tenant these observations belong to.
    pub tenant_id: String,
    /// The versions that produced the observations, as `(name, value)` pairs:
    /// build, repository revision, environment, registry generation.
    pub source_versions: Vec<(String, String)>,
    /// The digests of the baseline bundles it was derived from.
    pub source_digests: Vec<String>,
    /// When it was materialized.
    pub created_at_ms: i64,
    /// Observations counted, after deduplication.
    pub samples: u32,
    /// One aggregate per key, in key order.
    pub aggregates: Vec<Aggregate>,
    /// What this dataset is and is not.
    pub note: String,
    /// sha256 over every field above.
    pub digest: String,
}

/// Why a snapshot was refused.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "refusal", rename_all = "snake_case")]
pub enum StatsRefused {
    /// A sample belongs to another tenant.
    WrongTenant {
        /// The tenant the snapshot is for.
        expected: String,
        /// The tenant a sample claimed.
        found: String,
    },
    /// A key is missing a component it requires.
    MissingKeyComponent {
        /// The key, as far as it goes.
        key_id: String,
        /// The component that is absent.
        component: String,
    },
    /// A caller declared a sample count its samples do not support.
    FabricatedSampleCount {
        /// What was claimed.
        claimed: u32,
        /// What the samples actually are.
        actual: u32,
    },
    /// A sample is not attributed to anything.
    Unattributed {
        /// The outcome that has no source.
        outcome_id: String,
    },
    /// The snapshot asked for is not the snapshot found.
    StaleStats {
        /// What the caller pinned.
        pinned: String,
        /// What it got.
        found: String,
    },
    /// The snapshot is written at a schema this build does not read.
    IncompatibleSchema {
        /// What it claims.
        found: u32,
        /// What this build reads.
        supported: u32,
    },
}

impl StatsRefused {
    /// The stable code a client sees.
    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            Self::WrongTenant { .. } => "STATS_WRONG_TENANT",
            Self::MissingKeyComponent { .. } => "STATS_MISSING_KEY_COMPONENT",
            Self::FabricatedSampleCount { .. } => "STATS_FABRICATED_SAMPLE_COUNT",
            Self::Unattributed { .. } => "STATS_UNATTRIBUTED_SAMPLE",
            Self::StaleStats { .. } => "STATS_STALE",
            Self::IncompatibleSchema { .. } => "STATS_INCOMPATIBLE_SCHEMA",
        }
    }
}

/// What a caller says it is materializing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Materialization<'a> {
    /// The version to publish under.
    pub stats_version: &'a str,
    /// The tenant.
    pub tenant_id: &'a str,
    /// The versions that produced the observations.
    pub source_versions: Vec<(String, String)>,
    /// When.
    pub created_at_ms: i64,
    /// The count the caller believes it is publishing, when it wants that
    /// checked. `None` leaves the count to the samples, which is the usual
    /// case; `Some` is refused when it disagrees with them.
    pub declared_samples: Option<u32>,
}

/// The 95% Wilson score interval for `successes` out of `n`.
#[must_use]
pub fn wilson(successes: u32, n: u32) -> (f64, f64) {
    if n == 0 {
        return (0.0, 1.0);
    }
    let z = 1.959_963_984_540_054_f64;
    let n_f = f64::from(n);
    let p = f64::from(successes) / n_f;
    let denom = 1.0 + z * z / n_f;
    let centre = p + z * z / (2.0 * n_f);
    let spread = z * ((p * (1.0 - p) + z * z / (4.0 * n_f)) / n_f).sqrt();
    (
        ((centre - spread) / denom).clamp(0.0, 1.0),
        ((centre + spread) / denom).clamp(0.0, 1.0),
    )
}

fn digest_of(parts: &[&str]) -> String {
    let mut h = Sha256::new();
    for p in parts {
        h.update(p.as_bytes());
        h.update([0]);
    }
    hex::encode(h.finalize())
}

/// Materialize a snapshot from attributable samples.
///
/// Samples are deduplicated by `outcome_id`, so the same outcome arriving
/// twice is counted once. Counts and intervals are derived from what is left.
///
/// # Errors
/// A sample is unattributed, a key is missing a required component, or a
/// declared sample count disagrees with the samples.
pub fn materialize(m: &Materialization<'_>, samples: &[Sample]) -> Result<Snapshot, StatsRefused> {
    let mut seen: Vec<&Sample> = Vec::new();
    for s in samples {
        if s.attributed_to.trim().is_empty() {
            return Err(StatsRefused::Unattributed {
                outcome_id: s.outcome_id.clone(),
            });
        }
        if let Some(component) = s.key.missing_component() {
            return Err(StatsRefused::MissingKeyComponent {
                key_id: s.key.key_id(),
                component: component.to_owned(),
            });
        }
        // The same outcome observed twice is one outcome.
        if !seen.iter().any(|k| k.outcome_id == s.outcome_id) {
            seen.push(s);
        }
    }
    let counted = u32::try_from(seen.len()).unwrap_or(u32::MAX);
    if let Some(claimed) = m.declared_samples
        && claimed != counted
    {
        return Err(StatsRefused::FabricatedSampleCount {
            claimed,
            actual: counted,
        });
    }
    let mut key_ids: Vec<String> = seen.iter().map(|s| s.key.key_id()).collect();
    key_ids.sort();
    key_ids.dedup();
    let mut aggregates = Vec::new();
    for key_id in key_ids {
        let group: Vec<&&Sample> = seen.iter().filter(|s| s.key.key_id() == key_id).collect();
        let n = u32::try_from(group.len()).unwrap_or(u32::MAX);
        let successes =
            u32::try_from(group.iter().filter(|s| s.success).count()).unwrap_or(u32::MAX);
        let known: Vec<u64> = group.iter().filter_map(|s| s.cost_minor).collect();
        let mean_cost_minor = if known.is_empty() {
            None
        } else {
            Some(known.iter().sum::<u64>() / known.len() as u64)
        };
        let wall: u64 = group.iter().map(|s| s.wall_ms).sum();
        aggregates.push(Aggregate {
            key: group[0].key.clone(),
            key_id,
            samples: n,
            successes,
            mean: if n == 0 {
                0.0
            } else {
                f64::from(successes) / f64::from(n)
            },
            interval: wilson(successes, n),
            mean_cost_minor,
            unknown_cost_samples: n - u32::try_from(known.len()).unwrap_or(0),
            mean_wall_ms: if n == 0 { 0 } else { wall / u64::from(n) },
            low_confidence: n < MIN_CONFIDENT_SAMPLES,
        });
    }
    let mut source_digests: Vec<String> = seen.iter().map(|s| s.attributed_to.clone()).collect();
    source_digests.sort();
    source_digests.dedup();
    let mut snapshot = Snapshot {
        schema_version: STATS_SCHEMA_VERSION,
        stats_version: m.stats_version.to_owned(),
        tenant_id: m.tenant_id.to_owned(),
        source_versions: m.source_versions.clone(),
        source_digests,
        created_at_ms: m.created_at_ms,
        samples: counted,
        aggregates,
        note: NOTE.to_owned(),
        digest: String::new(),
    };
    snapshot.digest = digest_of(&[
        &snapshot.schema_version.to_string(),
        &snapshot.stats_version,
        &snapshot.tenant_id,
        &serde_json::to_string(&snapshot.source_versions).unwrap_or_default(),
        &snapshot.source_digests.join(","),
        &snapshot.samples.to_string(),
        &serde_json::to_string(&snapshot.aggregates).unwrap_or_default(),
    ]);
    Ok(snapshot)
}

impl Snapshot {
    /// The observation for a key, when there is one.
    #[must_use]
    pub fn get(&self, key: &StatKey) -> Option<&Aggregate> {
        let id = key.key_id();
        self.aggregates.iter().find(|a| a.key_id == id)
    }

    /// Read a snapshot for a pinned version: the compiler joins the registry
    /// and the statistics independently, so the registry generation is not
    /// part of this lookup and cannot change what was observed.
    ///
    /// # Errors
    /// The snapshot is a different version from the one pinned, or is written
    /// at a schema this build does not read.
    pub fn joined(&self, pinned_stats_version: &str) -> Result<&Self, StatsRefused> {
        if self.schema_version != STATS_SCHEMA_VERSION {
            return Err(StatsRefused::IncompatibleSchema {
                found: self.schema_version,
                supported: STATS_SCHEMA_VERSION,
            });
        }
        if self.stats_version != pinned_stats_version {
            return Err(StatsRefused::StaleStats {
                pinned: pinned_stats_version.to_owned(),
                found: self.stats_version.clone(),
            });
        }
        Ok(self)
    }

    /// Whether these observations belong to a tenant.
    ///
    /// # Errors
    /// The snapshot belongs to another tenant.
    pub fn for_tenant(&self, tenant_id: &str) -> Result<&Self, StatsRefused> {
        if self.tenant_id != tenant_id {
            return Err(StatsRefused::WrongTenant {
                expected: tenant_id.to_owned(),
                found: self.tenant_id.clone(),
            });
        }
        Ok(self)
    }
}

/// Derive samples from a published baseline bundle (REQ-EPR-000 → EPR-015).
///
/// Every task in the bundle is one attributable observation of the solver that
/// ran it. The direct path runs no Skill and its harness identity is the build
/// that ran it, so the key says exactly that rather than leaving a component
/// blank or inventing one.
#[must_use]
pub fn samples_from_baseline(
    bundle: &modbit_observability::baseline::BaselineBundle,
) -> Vec<Sample> {
    bundle
        .tasks
        .iter()
        .map(|t| Sample {
            outcome_id: format!("{}:{}", bundle.bundle_digest, t.task_id),
            key: StatKey::Solver {
                model: t.model.clone(),
                skill: "none".into(),
                harness: bundle.build_digest.clone(),
            },
            success: t.verified,
            // Cost stays unknown unless the provider reported the usage it was
            // computed from: an unknown cost is never averaged as zero.
            cost_minor: None,
            wall_ms: t.wall_ms,
            attributed_to: bundle.bundle_digest.clone(),
        })
        .collect()
}
