//! The Model Registry (REQ-EPR-002, docs/27 and docs/38): what models exist,
//! what each can do, what it costs, how fast it is and who may use it —
//! activated from a signed, versioned configuration document rather than from
//! a build.
//!
//! Two boundaries are load-bearing.
//!
//! The registry holds current facts, never empirical workflow outcomes. A
//! success rate, a quality bound, an escalation rate or a sample count belongs
//! to the statistics dataset, which is separately versioned; a document that
//! carries one is refused at ingestion rather than quietly ignored. The
//! registry keeps only a `stats_version` reference, and produces no estimate
//! of its own.
//!
//! Configuration is not a release. A new registry generation is a signed
//! document the Core verifies and activates, so changing a binding, a price or
//! a revocation needs no new desktop build. A document whose signature does
//! not verify, whose freshness has expired, or which leaves a required role
//! with no binding, is refused whole: the previously active registry stays.

use std::collections::BTreeMap;

use ed25519_dalek::{Signature, VerifyingKey};
use serde::{Deserialize, Serialize};

/// The document shape this build understands.
pub const REGISTRY_SCHEMA_VERSION: u32 = 1;

/// Roles a registry must bind before it can be activated: the product cannot
/// run a conditional transaction without something to solve with and something
/// to review with.
pub const REQUIRED_ROLES: &[&str] = &["solver", "reviewer"];

/// Field names that belong to the statistics dataset and never to the
/// registry. The list is matched against every key of the document, at any
/// depth, so an empirical field cannot arrive nested inside an entry.
pub const EMPIRICAL_FIELDS: &[&str] = &[
    "success_rate",
    "quality_mean",
    "quality_lcb",
    "escalation_rate",
    "reviewer_accept_rate",
    "revision_rate",
    "sample_count",
    "samples",
    "outcome_statistics",
    "p_success",
    "confidence",
    "posterior",
    "mean_quality",
    "win_rate",
];

/// Why a configuration document was refused.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "refusal", rename_all = "snake_case")]
pub enum RegistryRefused {
    /// The signing key is not one this Core trusts.
    UnknownKey {
        /// The key id the document claimed.
        key_id: String,
    },
    /// The signature does not verify over the document bytes.
    BadSignature,
    /// The document does not parse, or carries a field this build does not
    /// know.
    Malformed {
        /// What the parser said.
        detail: String,
    },
    /// The document carries empirical workflow outcomes, which belong to the
    /// separately versioned statistics dataset.
    EmpiricalFields {
        /// The offending field names, in the order they appear.
        fields: Vec<String>,
    },
    /// The document is written for a different registry schema.
    IncompatibleSchema {
        /// What it claims.
        found: u32,
        /// What this build reads.
        supported: u32,
    },
    /// The document's freshness window has passed.
    Expired {
        /// When it expired.
        expires_at_ms: i64,
        /// When it was checked.
        now_ms: i64,
    },
    /// The document is not valid yet.
    NotYetValid {
        /// When it becomes valid.
        issued_at_ms: i64,
        /// When it was checked.
        now_ms: i64,
    },
    /// A role the product needs has no live binding.
    MissingRoleBinding {
        /// The role.
        role: String,
    },
    /// The mode floor is missing or outside its range.
    InvalidQualityFloor {
        /// What is wrong with it.
        detail: String,
    },
}

impl RegistryRefused {
    /// The stable code a client sees.
    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            Self::UnknownKey { .. } => "REGISTRY_UNKNOWN_KEY",
            Self::BadSignature => "REGISTRY_BAD_SIGNATURE",
            Self::Malformed { .. } => "REGISTRY_MALFORMED",
            Self::EmpiricalFields { .. } => "REGISTRY_EMPIRICAL_FIELDS",
            Self::IncompatibleSchema { .. } => "REGISTRY_INCOMPATIBLE_SCHEMA",
            Self::Expired { .. } => "REGISTRY_EXPIRED",
            Self::NotYetValid { .. } => "REGISTRY_NOT_YET_VALID",
            Self::MissingRoleBinding { .. } => "REGISTRY_MISSING_ROLE_BINDING",
            Self::InvalidQualityFloor { .. } => "REGISTRY_INVALID_QUALITY_FLOOR",
        }
    }
}

/// What a model costs, in integer minor units per million tokens.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Economics {
    /// Input price per million tokens.
    pub input_per_mtok_minor: u64,
    /// Output price per million tokens.
    pub output_per_mtok_minor: u64,
    /// Currency.
    pub currency: String,
    /// Scale.
    pub scale: u8,
    /// Price of cached input per million tokens (REQ-EPR-009); absent =
    /// no cache discount.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cached_input_per_mtok_minor: Option<u64>,
    /// Price of writing a cache prefix per million tokens; absent = free.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_write_per_mtok_minor: Option<u64>,
    /// Cache lifetime in milliseconds; absent = the provider default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_ttl_ms: Option<u64>,
}

/// Current latency, as the provider or the operator measures it. These are
/// service facts, not outcome statistics.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Latency {
    /// Median time to last token for a reference request.
    pub p50_ms: u64,
    /// Tail time to last token for the same request.
    pub p95_ms: u64,
}

/// What governance allows for a model.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Governance {
    /// Where the provider processes the request.
    pub data_residency: String,
    /// Whether the provider retains prompts.
    pub retains_prompts: bool,
    /// Execution profiles this model may serve; empty means every profile.
    #[serde(default)]
    pub allowed_profiles: Vec<String>,
}

/// One model binding.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RegistryEntry {
    /// Gateway endpoint name.
    pub endpoint: String,
    /// Provider family (`openai`, `anthropic`).
    pub provider: String,
    /// Model family, for grouping bindings that share behaviour.
    pub family: String,
    /// Model id.
    pub model: String,
    /// Roles this binding may play (`solver`, `reviewer`, `reviser`).
    pub roles: Vec<String>,
    /// Input modalities it accepts.
    pub input_modalities: Vec<String>,
    /// Context window.
    pub context_tokens: u32,
    /// Output ceiling.
    pub max_output_tokens: u32,
    /// Whether it can call tools.
    pub tools: bool,
    /// Whether it accepts images.
    pub vision: bool,
    /// Whether it exposes reasoning effort.
    pub reasoning: bool,
    /// Whether it can be held to a schema.
    pub structured_output: bool,
    /// Current prices.
    pub economics: Economics,
    /// Current latency.
    pub latency: Latency,
    /// Governance.
    pub governance: Governance,
    /// Whether this binding is revoked: it stays in the document so a reader
    /// can see that it was withdrawn rather than never existed.
    #[serde(default)]
    pub revoked: bool,
}

/// The mode floor a plan must satisfy. It is a policy input, not an
/// observation: it says what the product demands, never what it achieved.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QualityFloor {
    /// Mode the floor applies to (`auto`, `quality`, `economy`).
    pub mode: String,
    /// Minimum quality a plan must claim, in `[0, 1]`.
    pub min_quality: f64,
    /// The ceiling a request may spend, in minor units.
    pub max_cost_minor: u64,
    /// Currency of that ceiling.
    pub currency: String,
    /// Scale of that ceiling.
    pub scale: u8,
}

/// The configuration document itself.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RegistryDocument {
    /// Schema version.
    pub schema_version: u32,
    /// The generation this document is, which every routing record pins.
    pub registry_generation: String,
    /// A reference to the statistics dataset version. The registry holds no
    /// statistics itself; this says which dataset a compiler should join.
    pub stats_version: String,
    /// When the document becomes valid.
    pub issued_at_ms: i64,
    /// When it stops being valid.
    pub expires_at_ms: i64,
    /// The mode floors.
    pub quality_floors: Vec<QualityFloor>,
    /// The bindings.
    pub entries: Vec<RegistryEntry>,
}

/// A document with the signature over its exact bytes.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SignedRegistry {
    /// Which trusted key signed it.
    pub key_id: String,
    /// Ed25519 signature over `document_json`, hex encoded.
    pub signature_hex: String,
    /// The document, as the exact bytes that were signed.
    pub document_json: String,
}

/// An activated registry: a verified document plus the bytes it was verified
/// from, so a later reader can check the signature again.
#[derive(Clone, Debug, PartialEq)]
pub struct ModelRegistry {
    /// The document.
    pub document: RegistryDocument,
    /// The key that signed it.
    pub key_id: String,
    /// Digest of the exact bytes that were verified.
    pub document_digest: String,
}

/// What a caller needs from a binding.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Needs {
    /// It must be able to call tools.
    pub tools: bool,
    /// It must accept images.
    pub vision: bool,
    /// It must be holdable to a schema.
    pub structured_output: bool,
    /// It must have at least this much context.
    pub min_context_tokens: u32,
    /// The execution profile the task runs under, when governance restricts it.
    pub execution_profile: Option<String>,
}

fn digest(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(bytes);
    hex::encode(h.finalize())
}

/// Every empirical field name that appears anywhere in the document, in order
/// of first appearance.
fn empirical_fields(value: &serde_json::Value, found: &mut Vec<String>) {
    match value {
        serde_json::Value::Object(map) => {
            for (k, v) in map {
                if EMPIRICAL_FIELDS.contains(&k.as_str()) && !found.contains(k) {
                    found.push(k.clone());
                }
                empirical_fields(v, found);
            }
        }
        serde_json::Value::Array(items) => {
            for v in items {
                empirical_fields(v, found);
            }
        }
        _ => {}
    }
}

/// Verify and activate a signed configuration document.
///
/// `trusted` maps a key id to its Ed25519 public key. Nothing outside that map
/// can activate a registry, and the signature is checked over the document's
/// exact bytes before anything in it is read as configuration.
///
/// # Errors
/// The key is unknown, the signature does not verify, the document is
/// malformed or written for another schema, it carries empirical outcome
/// fields, its freshness window does not contain `now_ms`, its floors are out
/// of range, or a required role has no live binding.
pub fn activate(
    signed: &SignedRegistry,
    trusted: &BTreeMap<String, [u8; 32]>,
    now_ms: i64,
) -> Result<ModelRegistry, RegistryRefused> {
    let Some(key) = trusted.get(&signed.key_id) else {
        return Err(RegistryRefused::UnknownKey {
            key_id: signed.key_id.clone(),
        });
    };
    let verifying = VerifyingKey::from_bytes(key).map_err(|_| RegistryRefused::BadSignature)?;
    let raw = hex::decode(&signed.signature_hex).map_err(|_| RegistryRefused::BadSignature)?;
    let bytes: [u8; 64] = raw.try_into().map_err(|_| RegistryRefused::BadSignature)?;
    verifying
        .verify_strict(
            signed.document_json.as_bytes(),
            &Signature::from_bytes(&bytes),
        )
        .map_err(|_| RegistryRefused::BadSignature)?;
    // Ingestion refuses empirical fields before the document is parsed as
    // configuration, so a field that a stricter parser would merely reject as
    // unknown is named for what it is.
    let value: serde_json::Value =
        serde_json::from_str(&signed.document_json).map_err(|e| RegistryRefused::Malformed {
            detail: e.to_string(),
        })?;
    let mut fields = Vec::new();
    empirical_fields(&value, &mut fields);
    if !fields.is_empty() {
        return Err(RegistryRefused::EmpiricalFields { fields });
    }
    let document: RegistryDocument =
        serde_json::from_value(value).map_err(|e| RegistryRefused::Malformed {
            detail: e.to_string(),
        })?;
    if document.schema_version != REGISTRY_SCHEMA_VERSION {
        return Err(RegistryRefused::IncompatibleSchema {
            found: document.schema_version,
            supported: REGISTRY_SCHEMA_VERSION,
        });
    }
    if now_ms < document.issued_at_ms {
        return Err(RegistryRefused::NotYetValid {
            issued_at_ms: document.issued_at_ms,
            now_ms,
        });
    }
    if now_ms >= document.expires_at_ms {
        return Err(RegistryRefused::Expired {
            expires_at_ms: document.expires_at_ms,
            now_ms,
        });
    }
    for floor in &document.quality_floors {
        if !(0.0..=1.0).contains(&floor.min_quality) || !floor.min_quality.is_finite() {
            return Err(RegistryRefused::InvalidQualityFloor {
                detail: format!("{} floor min_quality {}", floor.mode, floor.min_quality),
            });
        }
        if floor.currency.is_empty() || floor.max_cost_minor == 0 {
            return Err(RegistryRefused::InvalidQualityFloor {
                detail: format!("{} floor has no spendable ceiling", floor.mode),
            });
        }
    }
    if document.quality_floors.is_empty() {
        return Err(RegistryRefused::InvalidQualityFloor {
            detail: "no mode floor is defined".into(),
        });
    }
    for role in REQUIRED_ROLES {
        if !document
            .entries
            .iter()
            .any(|e| !e.revoked && e.roles.iter().any(|r| r == role))
        {
            return Err(RegistryRefused::MissingRoleBinding {
                role: (*role).to_owned(),
            });
        }
    }
    Ok(ModelRegistry {
        document_digest: digest(signed.document_json.as_bytes()),
        key_id: signed.key_id.clone(),
        document,
    })
}

impl ModelRegistry {
    /// The generation every routing record pins.
    #[must_use]
    pub fn generation(&self) -> &str {
        &self.document.registry_generation
    }

    /// The statistics dataset a compiler should join. The registry itself
    /// holds no statistics; this is a reference and nothing more.
    #[must_use]
    pub fn stats_version(&self) -> &str {
        &self.document.stats_version
    }

    /// The live bindings for a role that satisfy what the caller needs, in
    /// document order. Capability and governance both filter: a model that
    /// cannot call tools is not a solver for a task that needs them, and a
    /// model the profile does not allow is not a binding at all.
    #[must_use]
    pub fn bindings_for(&self, role: &str, needs: &Needs) -> Vec<&RegistryEntry> {
        self.document
            .entries
            .iter()
            .filter(|e| !e.revoked && e.roles.iter().any(|r| r == role))
            .filter(|e| !needs.tools || e.tools)
            .filter(|e| !needs.vision || e.vision)
            .filter(|e| !needs.structured_output || e.structured_output)
            .filter(|e| e.context_tokens >= needs.min_context_tokens)
            .filter(|e| {
                needs.execution_profile.as_ref().is_none_or(|p| {
                    e.governance.allowed_profiles.is_empty()
                        || e.governance.allowed_profiles.contains(p)
                })
            })
            .collect()
    }

    /// The binding for one endpoint and model, live or revoked.
    #[must_use]
    pub fn entry(&self, endpoint: &str, model: &str) -> Option<&RegistryEntry> {
        self.document
            .entries
            .iter()
            .find(|e| e.endpoint == endpoint && e.model == model)
    }

    /// Whether a dispatch to this endpoint and model is allowed right now, and
    /// why not when it is not.
    ///
    /// # Errors
    /// The model is not in the registry, has been revoked, or does not satisfy
    /// what the caller needs.
    pub fn check_dispatch(
        &self,
        endpoint: &str,
        model: &str,
        role: &str,
        needs: &Needs,
    ) -> Result<&RegistryEntry, (&'static str, String)> {
        let Some(entry) = self.entry(endpoint, model) else {
            return Err((
                "MODEL_NOT_IN_REGISTRY",
                format!(
                    "{endpoint}/{model} is not bound in registry generation {}",
                    self.generation()
                ),
            ));
        };
        if entry.revoked {
            return Err((
                "MODEL_REVOKED",
                format!(
                    "{endpoint}/{model} is revoked in registry generation {}",
                    self.generation()
                ),
            ));
        }
        if !self
            .bindings_for(role, needs)
            .iter()
            .any(|e| e.endpoint == endpoint && e.model == model)
        {
            return Err((
                "MODEL_NOT_ELIGIBLE",
                format!("{endpoint}/{model} does not satisfy the {role} bindings of this request"),
            ));
        }
        Ok(entry)
    }
}
