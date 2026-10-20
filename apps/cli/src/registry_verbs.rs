//! `modbit-cli registry ...` (REQ-PX-134; docs/27 §7.2): the operator's verbs
//! for the signed Model Registry of REQ-EPR-002. The CLI holds no registry
//! logic of its own: the Core verifies, activates, rolls back and remembers
//! (`ActivateModelRegistry`, `RollbackModelRegistry`, `GetModelRegistry`);
//! the offline verbs below only produce and check the signed bytes an
//! operator carries to a Core.
//!
//! ```text
//! registry keygen   --out <key-file> [--key-id <id>]        DEV KEY ONLY: not for production
//! registry sign     --key <key-file> --key-id <id> --doc <document.json> [--out <signed.json>]
//! registry verify   <signed.json> (--trusted <id:hex> | --pubkey <hex>)
//! registry revoke   --key <key-file> --key-id <id> --doc <document.json>
//!                   --generation <new-generation> --binding <endpoint/model>... [--out <signed.json>]
//! registry activate <signed.json> [--expected-generation <g>]
//! registry rollback --expected-generation <g>
//! registry status
//! ```
//!
//! `keygen`, `sign`, `verify` and `revoke` need no Core. The key file holds
//! the Ed25519 secret as hex and is the only place it ever appears: it is
//! never printed, logged or put on a command line. A revocation is a new
//! signed generation in which the named bindings carry `revoked: true`; the
//! Core keeps a revoked binding revoked across every later generation and
//! across a rollback. Withdrawing a signing key's trust is a change to the
//! Core's `MODBIT_REGISTRY_KEYS`, not a verb: a Core that no longer lists the
//! key refuses everything it signed.

use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use modbit_protocol::client::Client;
use modbit_protocol::v1::{
    ActivateModelRegistry, GetModelRegistry, ModelRegistryView, RollbackModelRegistry,
};
use prost::Message;
use sha2::{Digest, Sha256};

use crate::{USAGE, envelope};

/// Verbs that need no Core.
#[must_use]
pub(crate) fn is_offline(words: &[&str]) -> bool {
    matches!(
        words,
        ["registry", "keygen" | "sign" | "verify" | "revoke", ..]
    )
}

fn flag<'a>(words: &[&'a str], name: &str) -> Option<&'a str> {
    words
        .iter()
        .position(|w| *w == name)
        .and_then(|i| words.get(i + 1).copied())
}

fn flags<'a>(words: &[&'a str], name: &str) -> Vec<&'a str> {
    words
        .iter()
        .enumerate()
        .filter(|(_, w)| **w == name)
        .filter_map(|(i, _)| words.get(i + 1).copied())
        .collect()
}

fn read_key(path: &str) -> Result<SigningKey, String> {
    // The path may be named in an error; the contents never are.
    let text =
        std::fs::read_to_string(path).map_err(|e| format!("reading key file {path}: {e}"))?;
    let bytes = hex::decode(text.trim())
        .ok()
        .and_then(|b| <[u8; 32]>::try_from(b.as_slice()).ok())
        .ok_or_else(|| format!("key file {path} does not hold a 64-hex-char Ed25519 secret"))?;
    Ok(SigningKey::from_bytes(&bytes))
}

fn signed_json(key: &SigningKey, key_id: &str, document: &str) -> String {
    let signature = key.sign(document.as_bytes());
    serde_json::json!({
        "key_id": key_id,
        "signature_hex": hex::encode(signature.to_bytes()),
        "document_json": document,
    })
    .to_string()
}

fn digest(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

fn emit(out: Option<&str>, text: &str, what: &str) -> Result<(), String> {
    match out {
        Some(path) => {
            std::fs::write(path, text).map_err(|e| format!("writing {path}: {e}"))?;
            println!("{what} written to {path}");
            Ok(())
        }
        None => {
            println!("{text}");
            Ok(())
        }
    }
}

/// The offline verbs.
pub(crate) fn offline(words: &[&str]) -> Result<(), String> {
    match words {
        ["registry", "keygen", rest @ ..] => keygen(rest),
        ["registry", "sign", rest @ ..] => sign(rest),
        ["registry", "verify", rest @ ..] => verify(rest),
        ["registry", "revoke", rest @ ..] => revoke(rest),
        _ => Err(USAGE.into()),
    }
}

fn keygen(words: &[&str]) -> Result<(), String> {
    let out = flag(words, "--out").ok_or(USAGE)?;
    let key_id = flag(words, "--key-id").unwrap_or("dev");
    let seed: [u8; 32] = rand::random();
    let key = SigningKey::from_bytes(&seed);
    // create_new: never overwrite an existing key.
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(out).map_err(|e| {
        format!("creating key file {out}: {e} (an existing file is never overwritten)")
    })?;
    std::io::Write::write_all(&mut file, format!("{}\n", hex::encode(seed)).as_bytes())
        .map_err(|e| format!("writing key file {out}: {e}"))?;
    println!(
        "DEV KEY - NOT FOR PRODUCTION: this key sits unprotected in {out}. Use an operator-held key for any real registry."
    );
    println!("trust it in a development Core with:");
    println!(
        "  MODBIT_REGISTRY_KEYS={key_id}:{}",
        hex::encode(key.verifying_key().to_bytes())
    );
    Ok(())
}

fn sign(words: &[&str]) -> Result<(), String> {
    let key = read_key(flag(words, "--key").ok_or(USAGE)?)?;
    let key_id = flag(words, "--key-id").ok_or(USAGE)?;
    let doc_path = flag(words, "--doc").ok_or(USAGE)?;
    let document =
        std::fs::read_to_string(doc_path).map_err(|e| format!("reading {doc_path}: {e}"))?;
    let value: serde_json::Value = serde_json::from_str(&document)
        .map_err(|e| format!("{doc_path} is not a JSON document: {e}"))?;
    let generation = value["registry_generation"].as_str().unwrap_or_default();
    if generation.is_empty() {
        return Err(format!("{doc_path} has no registry_generation"));
    }
    emit(
        flag(words, "--out"),
        &signed_json(&key, key_id, &document),
        &format!(
            "signed registry generation {generation} (document sha256 {})",
            digest(document.as_bytes())
        ),
    )
}

fn verify(words: &[&str]) -> Result<(), String> {
    let path = words
        .iter()
        .find(|w| !w.starts_with("--"))
        .copied()
        .ok_or(USAGE)?;
    // `--trusted <id:hex>` names the key id and key; `--pubkey <hex>` takes
    // the id from the file.
    let signed: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(path).map_err(|e| format!("reading {path}: {e}"))?,
    )
    .map_err(|e| format!("{path} is not a signed registry: {e}"))?;
    let key_id = signed["key_id"].as_str().unwrap_or_default();
    let document = signed["document_json"].as_str().unwrap_or_default();
    let signature = signed["signature_hex"].as_str().unwrap_or_default();
    let (trusted_id, hex_key) = match (flag(words, "--trusted"), flag(words, "--pubkey")) {
        (Some(t), _) => t
            .split_once(':')
            .map(|(i, k)| (i.to_owned(), k.to_owned()))
            .ok_or("--trusted takes <key id>:<64 hex chars>")?,
        (None, Some(k)) => (key_id.to_owned(), k.to_owned()),
        (None, None) => return Err(USAGE.into()),
    };
    if trusted_id != key_id {
        return Err(format!(
            "REGISTRY_UNKNOWN_KEY: the bundle is signed by `{key_id}`, not by the trusted key `{trusted_id}`"
        ));
    }
    let verifying = hex::decode(hex_key.trim())
        .ok()
        .and_then(|b| <[u8; 32]>::try_from(b.as_slice()).ok())
        .and_then(|b| VerifyingKey::from_bytes(&b).ok())
        .ok_or("the trusted key is not 64 hex chars of an Ed25519 public key")?;
    let sig = hex::decode(signature)
        .ok()
        .and_then(|b| <[u8; 64]>::try_from(b.as_slice()).ok())
        .map(|b| Signature::from_bytes(&b))
        .ok_or("REGISTRY_BAD_SIGNATURE: the signature is not 128 hex chars")?;
    verifying.verify(document.as_bytes(), &sig).map_err(
        |_| "REGISTRY_BAD_SIGNATURE: the signature does not verify over the document bytes",
    )?;
    let value: serde_json::Value = serde_json::from_str(document)
        .map_err(|e| format!("REGISTRY_MALFORMED: the signed document is not JSON: {e}"))?;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX));
    let (issued, expires) = (
        value["issued_at_ms"].as_i64().unwrap_or(0),
        value["expires_at_ms"].as_i64().unwrap_or(0),
    );
    if now < issued {
        return Err("REGISTRY_NOT_YET_VALID: the document is not valid yet".into());
    }
    if now >= expires {
        return Err("REGISTRY_EXPIRED: the document's freshness window has passed".into());
    }
    println!(
        "registry verify OK key={key_id} generation={} digest={} expires_at_ms={expires} (signature and freshness only; the Core also checks schema, roles and floors at activation)",
        value["registry_generation"].as_str().unwrap_or_default(),
        digest(document.as_bytes()),
    );
    Ok(())
}

fn revoke(words: &[&str]) -> Result<(), String> {
    let key = read_key(flag(words, "--key").ok_or(USAGE)?)?;
    let key_id = flag(words, "--key-id").ok_or(USAGE)?;
    let doc_path = flag(words, "--doc").ok_or(USAGE)?;
    let generation = flag(words, "--generation").ok_or(USAGE)?;
    let bindings = flags(words, "--binding");
    if bindings.is_empty() {
        return Err("name at least one --binding <endpoint/model> to revoke".into());
    }
    let mut value: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(doc_path).map_err(|e| format!("reading {doc_path}: {e}"))?,
    )
    .map_err(|e| format!("{doc_path} is not a JSON document: {e}"))?;
    if value["registry_generation"].as_str() == Some(generation) {
        return Err("a revocation is a new generation: --generation must differ".into());
    }
    let entries = value["entries"]
        .as_array_mut()
        .ok_or("the document has no entries")?;
    for b in &bindings {
        let entry = entries.iter_mut().find(|e| {
            format!(
                "{}/{}",
                e["endpoint"].as_str().unwrap_or_default(),
                e["model"].as_str().unwrap_or_default()
            ) == *b
        });
        match entry {
            Some(e) => e["revoked"] = serde_json::Value::Bool(true),
            None => return Err(format!("the document holds no binding `{b}`")),
        }
    }
    value["registry_generation"] = serde_json::Value::String(generation.to_owned());
    // The edited document is re-serialized, so it is a new document with a
    // new signature: it never reuses the old signature or bytes.
    let document = serde_json::to_string(&value).map_err(|e| e.to_string())?;
    emit(
        flag(words, "--out"),
        &signed_json(&key, key_id, &document),
        &format!(
            "revoked {} in generation {generation} (activate it; a revoked binding stays revoked in every later generation)",
            bindings.join(", ")
        ),
    )
}

fn print_view(v: &ModelRegistryView) {
    if !v.active {
        println!(
            "registry not active refusal={} detail={}",
            v.refusal_code, v.refusal_detail
        );
        return;
    }
    println!(
        "registry active generation={} activation={} key={} digest={} stats_version={} expires_at_ms={} previous_good={} canary={}",
        v.registry_generation,
        if v.activation.is_empty() {
            "-"
        } else {
            &v.activation
        },
        v.key_id,
        v.document_digest,
        v.stats_version,
        v.expires_at_ms,
        if v.previous_good_generation.is_empty() {
            "-"
        } else {
            &v.previous_good_generation
        },
        if v.canary_generation.is_empty() {
            "-"
        } else {
            &v.canary_generation
        },
    );
    for b in &v.bindings {
        println!(
            "binding {}/{} family={} roles={} revoked={} price_in={} price_out={} {}",
            b.endpoint,
            b.model,
            b.family,
            b.roles.join(","),
            b.revoked,
            b.input_per_mtok_minor,
            b.output_per_mtok_minor,
            b.currency,
        );
    }
}

/// The verbs that talk to the Core.
pub(crate) async fn run(client: &mut Client, words: &[&str]) -> Result<(), String> {
    match words {
        ["registry", "activate", rest @ ..] => {
            let path = rest
                .iter()
                .find(|w| !w.starts_with("--"))
                .copied()
                .ok_or(USAGE)?;
            let signed =
                std::fs::read_to_string(path).map_err(|e| format!("reading {path}: {e}"))?;
            let ack = client
                .command(envelope(
                    "ActivateModelRegistry",
                    ActivateModelRegistry {
                        signed_json: signed,
                        expected_generation: flag(rest, "--expected-generation")
                            .unwrap_or_default()
                            .to_owned(),
                    }
                    .encode_to_vec(),
                ))
                .await
                .map_err(|e| e.to_string())?;
            let v: ModelRegistryView = Client::result(&ack).map_err(|e| e.to_string())?;
            print_view(&v);
            if v.active {
                Ok(())
            } else {
                Err(format!("{}: {}", v.refusal_code, v.refusal_detail))
            }
        }
        ["registry", "rollback", rest @ ..] => {
            let ack = client
                .command(envelope(
                    "RollbackModelRegistry",
                    RollbackModelRegistry {
                        expected_generation: flag(rest, "--expected-generation")
                            .ok_or(USAGE)?
                            .to_owned(),
                    }
                    .encode_to_vec(),
                ))
                .await
                .map_err(|e| e.to_string())?;
            let v: ModelRegistryView = Client::result(&ack).map_err(|e| e.to_string())?;
            print_view(&v);
            if v.active {
                Ok(())
            } else {
                Err(format!("{}: {}", v.refusal_code, v.refusal_detail))
            }
        }
        ["registry", "status"] => {
            let ack = client
                .command(envelope(
                    "GetModelRegistry",
                    GetModelRegistry {}.encode_to_vec(),
                ))
                .await
                .map_err(|e| e.to_string())?;
            let v: ModelRegistryView = Client::result(&ack).map_err(|e| e.to_string())?;
            print_view(&v);
            Ok(())
        }
        _ => Err(USAGE.into()),
    }
}
