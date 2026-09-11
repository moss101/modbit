//! Activating the Model Registry (REQ-EPR-002, docs/27 and docs/38).
//!
//! Configuration is not a release: an operator signs a versioned document and
//! the Core activates it, so a new binding, a new price or a revocation needs
//! no new build. Only the Core reads the trusted public keys, the same way
//! only the Core reads provider credentials, and what a client sees of the
//! registry carries neither a credential nor an endpoint URL.

use std::collections::BTreeMap;

use modbit_protocol::v1 as wire;
use modbit_providers::registry::{ModelRegistry, SignedRegistry};

use crate::server::Core;

/// The public keys this Core trusts to sign configuration, from the
/// environment: `MODBIT_REGISTRY_KEYS="<key id>:<64 hex chars>,..."`.
///
/// An empty or malformed entry is skipped rather than trusted, and a Core with
/// no trusted key activates nothing.
#[must_use]
pub(crate) fn trusted_keys() -> BTreeMap<String, [u8; 32]> {
    let mut out = BTreeMap::new();
    let Ok(raw) = std::env::var("MODBIT_REGISTRY_KEYS") else {
        return out;
    };
    for pair in raw.split(',').filter(|s| !s.trim().is_empty()) {
        let Some((id, hex_key)) = pair.split_once(':') else {
            continue;
        };
        let Ok(bytes) = hex::decode(hex_key.trim()) else {
            continue;
        };
        if let Ok(key) = <[u8; 32]>::try_from(bytes.as_slice()) {
            out.insert(id.trim().to_owned(), key);
        }
    }
    out
}

/// Activate a signed document, or say exactly why it was refused.
pub(crate) fn activate(core: &Core, signed_json: &str) -> wire::ModelRegistryView {
    let refuse = |code: &str, detail: String| wire::ModelRegistryView {
        active: false,
        refusal_code: code.to_owned(),
        refusal_detail: detail,
        ..Default::default()
    };
    let signed: SignedRegistry = match serde_json::from_str(signed_json) {
        Ok(s) => s,
        Err(e) => return refuse("REGISTRY_MALFORMED", e.to_string()),
    };
    let trusted = trusted_keys();
    if trusted.is_empty() {
        return refuse(
            "REGISTRY_NO_TRUSTED_KEY",
            "this Core trusts no signing key (set MODBIT_REGISTRY_KEYS)".into(),
        );
    }
    let now = modbit_domain::Timestamp::now().0;
    match core.gateway.activate_registry(&signed, &trusted, now) {
        Ok(registry) => view(&registry),
        Err(r) => refuse(r.code(), format!("{r:?}")),
    }
}

/// The active registry as a client sees it, or an inactive view when no
/// document has been activated.
pub(crate) fn current(core: &Core) -> wire::ModelRegistryView {
    core.gateway
        .registry()
        .as_ref()
        .map_or_else(wire::ModelRegistryView::default, view)
}

fn view(registry: &ModelRegistry) -> wire::ModelRegistryView {
    wire::ModelRegistryView {
        active: true,
        registry_generation: registry.generation().to_owned(),
        stats_version: registry.stats_version().to_owned(),
        key_id: registry.key_id.clone(),
        document_digest: registry.document_digest.clone(),
        expires_at_ms: registry.document.expires_at_ms,
        bindings: registry
            .document
            .entries
            .iter()
            .map(|e| wire::RegistryBindingView {
                endpoint: e.endpoint.clone(),
                provider: e.provider.clone(),
                family: e.family.clone(),
                model: e.model.clone(),
                roles: e.roles.clone(),
                context_tokens: e.context_tokens,
                max_output_tokens: e.max_output_tokens,
                tools: e.tools,
                vision: e.vision,
                structured_output: e.structured_output,
                input_per_mtok_minor: e.economics.input_per_mtok_minor,
                output_per_mtok_minor: e.economics.output_per_mtok_minor,
                currency: e.economics.currency.clone(),
                scale: u32::from(e.economics.scale),
                latency_p50_ms: e.latency.p50_ms,
                latency_p95_ms: e.latency.p95_ms,
                data_residency: e.governance.data_residency.clone(),
                retains_prompts: e.governance.retains_prompts,
                revoked: e.revoked,
            })
            .collect(),
        refusal_code: String::new(),
        refusal_detail: String::new(),
    }
}
