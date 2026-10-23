//! PX-134 (QUAL-PX-134) against the real Core process: a registry bundle that
//! is older than the active one is refused unless it is reached by the
//! explicit rollback, and a `SIGKILL` at any point of an activation leaves
//! exactly one whole active bundle after the restart — the old one or the new
//! one, never a mixture and never none.

mod px_common;

use std::time::Duration;

use ed25519_dalek::{Signer, SigningKey};
use modbit_protocol::client::Client;
use modbit_protocol::v1::{
    ActivateModelRegistry, GetModelRegistry, ModelRegistryView, RollbackModelRegistry,
};
use modbit_providers::registry::{
    Economics, Governance, Latency, QualityFloor, REGISTRY_SCHEMA_VERSION, RegistryDocument,
    RegistryEntry, SignedRegistry,
};
use prost::Message;
use px_common::{CoreProcess, envelope, rand_id};

fn entry(model: &str, roles: &[&str]) -> RegistryEntry {
    RegistryEntry {
        endpoint: "openai".into(),
        provider: "openai".into(),
        family: "gpt-5".into(),
        model: model.into(),
        roles: roles.iter().map(|r| (*r).to_owned()).collect(),
        input_modalities: vec!["text".into()],
        context_tokens: 400_000,
        max_output_tokens: 64_000,
        tools: true,
        vision: false,
        reasoning: true,
        structured_output: true,
        economics: Economics {
            input_per_mtok_minor: 125,
            output_per_mtok_minor: 1_000,
            currency: "USD".into(),
            scale: 2,
            cached_input_per_mtok_minor: None,
            cache_write_per_mtok_minor: None,
            cache_ttl_ms: None,
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
        fallbacks: vec![],
    }
}

/// A signed bundle issued `age_ms` ago.
fn bundle(key: &SigningKey, generation: &str, age_ms: i64) -> String {
    let now = modbit_domain::Timestamp::now().0;
    let doc = RegistryDocument {
        promotion: None,
        schema_version: REGISTRY_SCHEMA_VERSION,
        registry_generation: generation.into(),
        stats_version: "stats-2026-09-05".into(),
        issued_at_ms: now - age_ms,
        expires_at_ms: now + 86_400_000,
        quality_floors: vec![QualityFloor {
            mode: "auto".into(),
            min_quality: 0.72,
            max_cost_minor: 5_000,
            currency: "USD".into(),
            scale: 2,
        }],
        entries: vec![
            entry("gpt-5-mini", &["solver"]),
            entry("gpt-5", &["solver", "reviewer"]),
        ],
    };
    let json = serde_json::to_string(&doc).unwrap();
    serde_json::to_string(&SignedRegistry {
        key_id: "ops".into(),
        signature_hex: hex::encode(key.sign(json.as_bytes()).to_bytes()),
        document_json: json,
    })
    .unwrap()
}

async fn status(c: &mut Client) -> ModelRegistryView {
    let ack = c
        .command(envelope(
            rand_id(),
            "GetModelRegistry",
            GetModelRegistry {}.encode_to_vec(),
        ))
        .await
        .unwrap();
    Client::result(&ack).unwrap()
}

async fn activate(c: &mut Client, signed: &str) -> ModelRegistryView {
    let ack = c
        .command(envelope(
            rand_id(),
            "ActivateModelRegistry",
            ActivateModelRegistry {
                signed_json: signed.to_owned(),
                expected_generation: String::new(),
            }
            .encode_to_vec(),
        ))
        .await
        .unwrap();
    Client::result(&ack).unwrap()
}

#[tokio::test]
async fn an_older_bundle_is_refused_until_it_is_rolled_back_to() {
    let key = SigningKey::from_bytes(&[21u8; 32]);
    let keys = format!("ops:{}", hex::encode(key.verifying_key().to_bytes()));
    let dir = tempfile::tempdir().unwrap();
    let env = [
        ("OPENAI_API_KEY", "sk-test-registry"),
        ("ANTHROPIC_API_KEY", ""),
        ("MODBIT_REGISTRY_KEYS", keys.as_str()),
    ];
    let core = CoreProcess::spawn_with_env(dir.path(), &env);
    let mut c = core.client().await;
    let one = activate(&mut c, &bundle(&key, "reg-1", 120_000)).await;
    assert_eq!(one.registry_generation, "reg-1", "{one:?}");
    let two = activate(&mut c, &bundle(&key, "reg-2", 60_000)).await;
    assert_eq!(two.registry_generation, "reg-2", "{two:?}");
    assert_eq!(two.previous_good_generation, "reg-1");

    // An older bundle, validly signed and unexpired, is not an activation.
    let old = activate(&mut c, &bundle(&key, "reg-0", 300_000)).await;
    assert!(!old.active, "{old:?}");
    assert_eq!(old.refusal_code, "REGISTRY_OLDER_THAN_ACTIVE", "{old:?}");
    let now = status(&mut c).await;
    assert_eq!(now.registry_generation, "reg-2");
    assert_eq!(now.document_digest, two.document_digest);

    // The explicit rollback is the way back, and it restores reg-1's digest.
    let ack = c
        .command(envelope(
            rand_id(),
            "RollbackModelRegistry",
            RollbackModelRegistry {
                expected_generation: "reg-2".into(),
            }
            .encode_to_vec(),
        ))
        .await
        .unwrap();
    let back: ModelRegistryView = Client::result(&ack).unwrap();
    assert_eq!(back.registry_generation, "reg-1", "{back:?}");
    assert_eq!(back.document_digest, one.document_digest);
}

#[tokio::test]
async fn sigkill_during_activation_leaves_exactly_one_whole_active_bundle() {
    let key = SigningKey::from_bytes(&[22u8; 32]);
    let keys = format!("ops:{}", hex::encode(key.verifying_key().to_bytes()));
    let dir = tempfile::tempdir().unwrap();
    let env = [
        ("OPENAI_API_KEY", "sk-test-registry"),
        ("ANTHROPIC_API_KEY", ""),
        ("MODBIT_REGISTRY_KEYS", keys.as_str()),
    ];
    let first = bundle(&key, "reg-1", 120_000);
    let mut digests = std::collections::BTreeMap::new();
    {
        let core = CoreProcess::spawn_with_env(dir.path(), &env);
        let mut c = core.client().await;
        let v = activate(&mut c, &first).await;
        digests.insert("reg-1".to_owned(), v.document_digest);
    }
    let mut saw = std::collections::BTreeSet::new();
    for round in 0..12u64 {
        let generation = format!("reg-{}", round + 2);
        let next = bundle(&key, &generation, 60_000 - round as i64 * 1_000);
        let mut core = CoreProcess::spawn_with_env(dir.path(), &env);
        let mut c = core.client().await;
        // Kill at a different point of the activation each round.
        let _ = tokio::time::timeout(
            Duration::from_micros(200 * round * round),
            activate(&mut c, &next),
        )
        .await;
        core.kill();
        drop(c);
        let core = CoreProcess::spawn_with_env(dir.path(), &env);
        let mut c = core.client().await;
        let v = status(&mut c).await;
        assert!(
            v.active,
            "round {round}: no active bundle after the kill: {v:?}"
        );
        // Whole: its digest is the digest of the one signed document it names.
        let want = if v.registry_generation == generation {
            let expected = activate_digest(&next);
            digests.insert(generation.clone(), expected.clone());
            expected
        } else {
            digests
                .get(&v.registry_generation)
                .unwrap_or_else(|| panic!("round {round}: unknown generation {v:?}"))
                .clone()
        };
        assert_eq!(v.document_digest, want, "round {round}: {v:?}");
        assert_eq!(v.bindings.len(), 2, "round {round}: {v:?}");
        saw.insert(v.registry_generation.clone());
        // Whichever survived, the Core keeps serving on it: the next bundle
        // still activates (the kill left no half-written state to trip on).
        let again = activate(&mut c, &next).await;
        assert!(again.active, "round {round}: {again:?}");
        digests.insert(again.registry_generation.clone(), again.document_digest);
    }
    assert!(!saw.is_empty());
}

/// The digest the Core reports for a signed bundle: SHA-256 of the exact
/// document bytes that were verified.
fn activate_digest(signed: &str) -> String {
    use sha2::{Digest, Sha256};
    let s: SignedRegistry = serde_json::from_str(signed).unwrap();
    hex::encode(Sha256::digest(s.document_json.as_bytes()))
}
