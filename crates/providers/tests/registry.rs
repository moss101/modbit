//! REQ-EPR-002 / EPR-FI-002: what a signed configuration document must be
//! before it becomes the active Model Registry, and what it may never carry.

use std::collections::BTreeMap;

use ed25519_dalek::{Signer, SigningKey};
use modbit_providers::registry::{
    Economics, Governance, Latency, ModelRegistry, Needs, QualityFloor, REGISTRY_SCHEMA_VERSION,
    RegistryDocument, RegistryEntry, RegistryRefused, SignedRegistry, activate,
};

const NOW: i64 = 1_800_000_000_000;

fn key() -> SigningKey {
    SigningKey::from_bytes(&[7u8; 32])
}

fn trusted() -> BTreeMap<String, [u8; 32]> {
    let mut m = BTreeMap::new();
    m.insert("ops-2026".to_owned(), key().verifying_key().to_bytes());
    m
}

fn entry(model: &str, roles: &[&str], tools: bool) -> RegistryEntry {
    RegistryEntry {
        endpoint: "openai".into(),
        provider: "openai".into(),
        family: "gpt-5".into(),
        model: model.into(),
        roles: roles.iter().map(|r| (*r).to_owned()).collect(),
        input_modalities: vec!["text".into()],
        context_tokens: 400_000,
        max_output_tokens: 64_000,
        tools,
        vision: false,
        reasoning: true,
        structured_output: true,
        economics: Economics {
            input_per_mtok_minor: 25,
            output_per_mtok_minor: 200,
            currency: "USD".into(),
            scale: 2,
        },
        latency: Latency {
            p50_ms: 900,
            p95_ms: 4_200,
        },
        governance: Governance {
            data_residency: "us".into(),
            retains_prompts: false,
            allowed_profiles: vec![],
        },
        revoked: false,
    }
}

fn document() -> RegistryDocument {
    RegistryDocument {
        schema_version: REGISTRY_SCHEMA_VERSION,
        registry_generation: "registry-2026-09-11".into(),
        stats_version: "stats-2026-09-05".into(),
        issued_at_ms: NOW - 1_000,
        expires_at_ms: NOW + 86_400_000,
        quality_floors: vec![QualityFloor {
            mode: "auto".into(),
            min_quality: 0.72,
            max_cost_minor: 5_000,
            currency: "USD".into(),
            scale: 2,
        }],
        entries: vec![
            entry("gpt-5-mini", &["solver"], true),
            entry("gpt-5", &["solver", "reviewer"], true),
        ],
    }
}

fn sign(doc: &RegistryDocument) -> SignedRegistry {
    sign_json(&serde_json::to_string(doc).unwrap())
}

fn sign_json(json: &str) -> SignedRegistry {
    SignedRegistry {
        key_id: "ops-2026".to_owned(),
        signature_hex: hex::encode(key().sign(json.as_bytes()).to_bytes()),
        document_json: json.to_owned(),
    }
}

fn active() -> ModelRegistry {
    activate(&sign(&document()), &trusted(), NOW).expect("activated")
}

#[test]
fn a_signed_document_activates_without_a_new_build() {
    let r = active();
    assert_eq!(r.generation(), "registry-2026-09-11");
    assert_eq!(r.key_id, "ops-2026");
    assert_eq!(r.document_digest.len(), 64);
    // The registry references the statistics dataset and holds none of it.
    assert_eq!(r.stats_version(), "stats-2026-09-05");
    let json = serde_json::to_string(&r.document).unwrap();
    for field in modbit_providers::registry::EMPIRICAL_FIELDS {
        assert!(
            !json.contains(field),
            "the registry shape can express {field}"
        );
    }
    // Capabilities and governance filter the bindings.
    let needs = Needs {
        tools: true,
        min_context_tokens: 200_000,
        ..Needs::default()
    };
    let solvers: Vec<&str> = r
        .bindings_for("solver", &needs)
        .iter()
        .map(|e| e.model.as_str())
        .collect();
    assert_eq!(solvers, vec!["gpt-5-mini", "gpt-5"]);
    assert_eq!(
        r.bindings_for("reviewer", &needs)
            .iter()
            .map(|e| e.model.as_str())
            .collect::<Vec<_>>(),
        vec!["gpt-5"]
    );
    // A need the binding cannot meet removes it rather than bending it.
    assert!(
        r.bindings_for(
            "solver",
            &Needs {
                vision: true,
                ..Needs::default()
            }
        )
        .is_empty()
    );
    assert!(
        r.bindings_for(
            "solver",
            &Needs {
                min_context_tokens: 2_000_000,
                ..Needs::default()
            }
        )
        .is_empty()
    );
    assert!(
        r.check_dispatch("openai", "gpt-5", "solver", &needs)
            .is_ok()
    );
}

#[test]
fn ingestion_refuses_empirical_workflow_outcomes() {
    // A document that carries a success rate on an entry, however nested.
    let mut value = serde_json::to_value(document()).unwrap();
    value["entries"][0]["success_rate"] = serde_json::json!(0.81);
    value["entries"][1]["outcome_statistics"] = serde_json::json!({"samples": 12});
    let err = activate(&sign_json(&value.to_string()), &trusted(), NOW).unwrap_err();
    assert_eq!(err.code(), "REGISTRY_EMPIRICAL_FIELDS");
    assert!(
        matches!(err, RegistryRefused::EmpiricalFields { ref fields }
            if fields == &vec!["success_rate".to_owned(), "outcome_statistics".to_owned(), "samples".to_owned()]),
        "{err:?}"
    );
    // And a field this build simply does not know is refused too, rather than
    // ignored into a silent misconfiguration.
    let mut value = serde_json::to_value(document()).unwrap();
    value["entries"][0]["preferred"] = serde_json::json!(true);
    let err = activate(&sign_json(&value.to_string()), &trusted(), NOW).unwrap_err();
    assert_eq!(err.code(), "REGISTRY_MALFORMED");
}

#[test]
fn a_document_that_is_not_authentic_or_not_fresh_never_activates() {
    // Tampered after signing.
    let mut signed = sign(&document());
    signed.document_json = signed
        .document_json
        .replace("\"registry-2026-09-11\"", "\"registry-tampered\"");
    assert_eq!(
        activate(&signed, &trusted(), NOW).unwrap_err().code(),
        "REGISTRY_BAD_SIGNATURE"
    );
    // Signed by a key this Core does not trust.
    let other = SigningKey::from_bytes(&[9u8; 32]);
    let json = serde_json::to_string(&document()).unwrap();
    let forged = SignedRegistry {
        key_id: "ops-2026".to_owned(),
        signature_hex: hex::encode(other.sign(json.as_bytes()).to_bytes()),
        document_json: json.clone(),
    };
    assert_eq!(
        activate(&forged, &trusted(), NOW).unwrap_err().code(),
        "REGISTRY_BAD_SIGNATURE"
    );
    let unknown = SignedRegistry {
        key_id: "nobody".to_owned(),
        ..sign(&document())
    };
    assert_eq!(
        activate(&unknown, &trusted(), NOW).unwrap_err().code(),
        "REGISTRY_UNKNOWN_KEY"
    );
    // Freshness: the same document, checked outside its window.
    let doc = document();
    let err = activate(&sign(&doc), &trusted(), doc.expires_at_ms).unwrap_err();
    assert_eq!(err.code(), "REGISTRY_EXPIRED");
    let err = activate(&sign(&doc), &trusted(), doc.issued_at_ms - 1).unwrap_err();
    assert_eq!(err.code(), "REGISTRY_NOT_YET_VALID");
    // A document for another schema.
    let mut future = document();
    future.schema_version = REGISTRY_SCHEMA_VERSION + 1;
    assert_eq!(
        activate(&sign(&future), &trusted(), NOW)
            .unwrap_err()
            .code(),
        "REGISTRY_INCOMPATIBLE_SCHEMA"
    );
}

#[test]
fn a_registry_without_a_floor_or_a_reviewer_is_refused_whole() {
    let mut no_floor = document();
    no_floor.quality_floors.clear();
    assert_eq!(
        activate(&sign(&no_floor), &trusted(), NOW)
            .unwrap_err()
            .code(),
        "REGISTRY_INVALID_QUALITY_FLOOR"
    );
    let mut bad_floor = document();
    bad_floor.quality_floors[0].min_quality = 1.4;
    assert_eq!(
        activate(&sign(&bad_floor), &trusted(), NOW)
            .unwrap_err()
            .code(),
        "REGISTRY_INVALID_QUALITY_FLOOR"
    );
    let mut no_reviewer = document();
    no_reviewer.entries[1].roles = vec!["solver".into()];
    let err = activate(&sign(&no_reviewer), &trusted(), NOW).unwrap_err();
    assert_eq!(err.code(), "REGISTRY_MISSING_ROLE_BINDING");
    assert!(
        matches!(err, RegistryRefused::MissingRoleBinding { ref role } if role == "reviewer"),
        "{err:?}"
    );
    // Revoking the only reviewer is the same thing: the generation that would
    // leave the product unable to review is refused, not activated.
    let mut revoked = document();
    revoked.entries[1].revoked = true;
    assert_eq!(
        activate(&sign(&revoked), &trusted(), NOW)
            .unwrap_err()
            .code(),
        "REGISTRY_MISSING_ROLE_BINDING"
    );
}

#[test]
fn a_revoked_model_is_refused_at_dispatch_and_says_so() {
    let mut doc = document();
    // A generation that withdraws the cheap solver but keeps a reviewer.
    doc.entries[0].revoked = true;
    doc.registry_generation = "registry-2026-09-12".into();
    let r = activate(&sign(&doc), &trusted(), NOW).expect("activated");
    let needs = Needs {
        tools: true,
        ..Needs::default()
    };
    let (code, detail) = r
        .check_dispatch("openai", "gpt-5-mini", "solver", &needs)
        .unwrap_err();
    assert_eq!(code, "MODEL_REVOKED");
    assert!(detail.contains("registry-2026-09-12"), "{detail}");
    // The withdrawn binding is still visible as withdrawn, and the remaining
    // eligible solver is what a caller can fall back to.
    assert!(r.entry("openai", "gpt-5-mini").unwrap().revoked);
    assert_eq!(
        r.bindings_for("solver", &needs)
            .iter()
            .map(|e| e.model.as_str())
            .collect::<Vec<_>>(),
        vec!["gpt-5"]
    );
    // A model that was never bound is a different answer from a revoked one.
    let (code, _) = r
        .check_dispatch("openai", "gpt-4o", "solver", &needs)
        .unwrap_err();
    assert_eq!(code, "MODEL_NOT_IN_REGISTRY");
    // And a live binding that cannot do what the request needs is neither.
    let (code, _) = r
        .check_dispatch(
            "openai",
            "gpt-5",
            "solver",
            &Needs {
                vision: true,
                ..Needs::default()
            },
        )
        .unwrap_err();
    assert_eq!(code, "MODEL_NOT_ELIGIBLE");
}
