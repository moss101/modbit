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

/// Where the activation history lives: every generation this Core
/// activated, its signed bytes by object reference, and every binding any of
/// them revoked (EPR-012: the rollback target and the revocations a rollback
/// must keep).
fn history_path(core: &Core) -> std::path::PathBuf {
    core.data_dir.join("registry").join("history.json")
}

/// The activation history.
#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub(crate) struct History {
    /// Activations, oldest first.
    pub entries: Vec<Activation>,
    /// Every binding (`endpoint/model`) any activated generation revoked.
    pub revoked: std::collections::BTreeSet<String>,
}

/// One activation.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub(crate) struct Activation {
    pub generation: String,
    /// sha256 of the SignedRegistry JSON in the object store.
    pub signed_ref: String,
    /// `ACTIVATED` | `PROMOTED` | `ROLLED_BACK` (production), `CANARY` |
    /// `CANARY_ENDED` (the canary stage beside it).
    pub kind: String,
    pub previous: String,
    pub at_ms: i64,
}

const CANARY: &str = "CANARY";
const CANARY_ENDED: &str = "CANARY_ENDED";

impl History {
    /// The production generation in force.
    fn production(&self) -> Option<&Activation> {
        self.entries
            .iter()
            .rev()
            .find(|a| a.kind != CANARY && a.kind != CANARY_ENDED)
    }

    /// The canary running: the last activation, when it is one. Anything
    /// recorded after a canary started (production replaced, the canary
    /// ended) ended it.
    fn canary(&self) -> Option<&Activation> {
        self.entries.last().filter(|a| a.kind == CANARY)
    }
}

/// The registry a task's requests route under: the canary, when one is
/// running and the task's policy in force allows canary routing
/// (`routing.canary` ALLOW, REQ-EPR-012), otherwise production.
pub(crate) fn for_task(core: &Core, task: &modbit_domain::task::Task) -> Option<ModelRegistry> {
    if let Some(canary) = core.gateway.canary()
        && canary_allowed(core, task)
    {
        return Some(canary);
    }
    core.gateway.registry()
}

/// The registry that prices a binding: production when it holds it,
/// otherwise the canary running when that does (a binding new in the
/// canary is priced by the generation that introduced it).
pub(crate) fn priced_by(core: &Core, endpoint: &str, model: &str) -> Option<ModelRegistry> {
    let production = core.gateway.registry();
    if production
        .as_ref()
        .is_some_and(|r| r.entry(endpoint, model).is_some())
    {
        return production;
    }
    core.gateway
        .canary()
        .filter(|c| c.entry(endpoint, model).is_some())
        .or(production)
}

/// Whether the policy in force for `task` allows canary routing.
pub(crate) fn canary_allowed(core: &Core, task: &modbit_domain::task::Task) -> bool {
    core.tools
        .configurations
        .for_task(task.task_id, &core.data_dir, task.workspace_root.as_deref())
        .permissions
        .get("routing.canary")
        .is_some_and(|p| p.value == modbit_policy::config::Permission::Allow)
}

fn load_history(core: &Core) -> History {
    std::fs::read(history_path(core))
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

fn save_history(core: &Core, h: &History) -> Result<(), String> {
    let path = history_path(core);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_vec_pretty(h).unwrap_or_default())
        .map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, &path).map_err(|e| e.to_string())
}

fn refusal(code: &str, detail: String) -> wire::ModelRegistryView {
    wire::ModelRegistryView {
        active: false,
        refusal_code: code.to_owned(),
        refusal_detail: detail,
        ..Default::default()
    }
}

/// Activate a signed document, or say exactly why it was refused.
///
/// A document is verified (key, signature, schema, freshness, roles), then
/// every binding an earlier generation revoked stays revoked in it. A
/// promotion (a document with `promotion`, REQ-EPR-012) must also pass the
/// promotion gate, and then starts as the canary beside production — only
/// `PromoteCanary`, on the canary's own observed requests, makes it
/// production. Every activation installs as a compare-and-swap on
/// `expected_generation` when one is given — a promotion must give one.
pub(crate) async fn activate(
    core: &Core,
    signed_json: &str,
    expected_generation: &str,
) -> wire::ModelRegistryView {
    let signed: SignedRegistry = match serde_json::from_str(signed_json) {
        Ok(s) => s,
        Err(e) => return refusal("REGISTRY_MALFORMED", e.to_string()),
    };
    let trusted = trusted_keys();
    if trusted.is_empty() {
        return refusal(
            "REGISTRY_NO_TRUSTED_KEY",
            "this Core trusts no signing key (set MODBIT_REGISTRY_KEYS)".into(),
        );
    }
    let now = modbit_domain::Timestamp::now().0;
    let mut registry = match modbit_providers::registry::activate(&signed, &trusted, now) {
        Ok(r) => r,
        Err(r) => return refusal(r.code(), format!("{r:?}")),
    };
    let mut history = load_history(core);
    if let Err((code, detail)) = keep_revocations(&mut registry, &history) {
        return refusal(code, detail);
    }
    let current = core.gateway.registry();
    if let Some(promotion) = registry.document.promotion.clone() {
        if let Err((code, detail)) = crate::promotion::gate(
            core,
            &registry,
            &promotion,
            current.as_ref(),
            expected_generation,
        )
        .await
        {
            return refusal(code, detail);
        }
        return start_canary(core, registry, &signed, expected_generation, &mut history).await;
    }
    install(
        core,
        registry,
        &signed,
        expected_generation,
        "ACTIVATED",
        &mut history,
    )
    .await
}

async fn put_signed(
    core: &Core,
    signed: &SignedRegistry,
) -> Result<String, wire::ModelRegistryView> {
    let store = core.store.lock().await;
    store
        .objects()
        .put(serde_json::to_string(signed).unwrap_or_default().as_bytes())
        .map_err(|e| refusal("STORE", e.to_string()))
}

/// A promotion that passed the gate starts as the canary beside production,
/// as a compare-and-swap on production.
async fn start_canary(
    core: &Core,
    registry: ModelRegistry,
    signed: &SignedRegistry,
    expected_generation: &str,
    history: &mut History,
) -> wire::ModelRegistryView {
    let signed_ref = match put_signed(core, signed).await {
        Ok(r) => r,
        Err(v) => return v,
    };
    if let Err(detail) = core
        .gateway
        .install_canary(registry.clone(), expected_generation)
    {
        return refusal("REGISTRY_ACTIVATION_CONFLICT", detail);
    }
    history.revoked.extend(registry.revoked());
    history.entries.push(Activation {
        generation: registry.generation().to_owned(),
        signed_ref,
        kind: CANARY.to_owned(),
        previous: expected_generation.to_owned(),
        at_ms: modbit_domain::Timestamp::now().0,
    });
    if let Err(e) = save_history(core, history) {
        eprintln!("modbit-core: recording the canary failed: {e}");
    }
    let mut v = view(&registry);
    v.previous_good_generation = expected_generation.to_owned();
    v.canary_generation = registry.generation().to_owned();
    v.activation = CANARY.to_owned();
    v
}

/// Promote the running canary to production (REQ-EPR-012): only on its own
/// observed requests (`<session id>:<task id>`, routed under it, reviewed,
/// at least as many as its promotion asks), and as a compare-and-swap on
/// both the canary and the production generation it replaces.
pub(crate) async fn promote_canary(
    core: &Core,
    canary_generation: &str,
    expected_generation: &str,
    canary_requests: &[String],
) -> wire::ModelRegistryView {
    let mut history = load_history(core);
    let Some(canary) = core.gateway.canary() else {
        return refusal(
            "REGISTRY_ACTIVATION_CONFLICT",
            "no canary is running".into(),
        );
    };
    if canary.generation() != canary_generation {
        return refusal(
            "REGISTRY_ACTIVATION_CONFLICT",
            format!(
                "the canary running is `{}`, not `{canary_generation}`",
                canary.generation()
            ),
        );
    }
    let Some(started) = history.canary().cloned() else {
        return refusal(
            "REGISTRY_NO_HISTORY",
            "the canary's start is not recorded".into(),
        );
    };
    let Some(promotion) = canary.document.promotion.clone() else {
        return refusal(
            "POLICY_EVIDENCE_MISSING",
            "the canary carries no promotion".into(),
        );
    };
    if let Err((code, detail)) =
        crate::promotion::canary_holds(core, &canary, &promotion, canary_requests).await
    {
        return refusal(code, detail);
    }
    let promoted = match core
        .gateway
        .promote_canary(canary_generation, expected_generation)
    {
        Ok(r) => r,
        Err(detail) => return refusal("REGISTRY_ACTIVATION_CONFLICT", detail),
    };
    history.revoked.extend(promoted.revoked());
    history.entries.push(Activation {
        generation: promoted.generation().to_owned(),
        signed_ref: started.signed_ref,
        kind: "PROMOTED".to_owned(),
        previous: expected_generation.to_owned(),
        at_ms: modbit_domain::Timestamp::now().0,
    });
    if let Err(e) = save_history(core, &history) {
        eprintln!("modbit-core: recording the promotion failed: {e}");
    }
    let mut v = view(&promoted);
    v.previous_good_generation = expected_generation.to_owned();
    v.activation = "PROMOTED".to_owned();
    v
}

/// Keep every revocation any activated generation made; refuse when that
/// leaves a required role without a live binding.
fn keep_revocations(
    registry: &mut ModelRegistry,
    history: &History,
) -> Result<(), (&'static str, String)> {
    let changed = registry.keep_revoked(&history.revoked);
    if let Some(role) = registry.unbound_role() {
        return Err((
            "REGISTRY_INCOMPATIBLE",
            format!(
                "no live {role} binding once earlier revocations are kept ({}); a revoked model is not brought back",
                changed.join(", ")
            ),
        ));
    }
    Ok(())
}

/// Install with compare-and-swap, store the signed bytes, record the
/// activation.
async fn install(
    core: &Core,
    registry: ModelRegistry,
    signed: &SignedRegistry,
    expected_generation: &str,
    kind: &str,
    history: &mut History,
) -> wire::ModelRegistryView {
    let signed_ref = match put_signed(core, signed).await {
        Ok(r) => r,
        Err(v) => return v,
    };
    let previous = core
        .gateway
        .registry()
        .map(|r| r.generation().to_owned())
        .unwrap_or_default();
    let expected = (!expected_generation.is_empty()).then_some(expected_generation);
    if let Err(active) = core.gateway.install_registry(registry.clone(), expected) {
        return refusal(
            "REGISTRY_ACTIVATION_CONFLICT",
            format!("expected generation `{expected_generation}` but `{active}` is active"),
        );
    }
    history.revoked.extend(registry.revoked());
    history.entries.push(Activation {
        generation: registry.generation().to_owned(),
        signed_ref,
        kind: kind.to_owned(),
        previous,
        at_ms: modbit_domain::Timestamp::now().0,
    });
    if let Err(e) = save_history(core, history) {
        eprintln!("modbit-core: recording the registry activation failed: {e}");
    }
    let mut v = view(&registry);
    v.previous_good_generation = history
        .entries
        .last()
        .map(|a| a.previous.clone())
        .unwrap_or_default();
    v.activation = kind.to_owned();
    v
}

/// Return to the generation the active one replaced, as a compare-and-swap
/// on the active generation. The earlier document is verified again (keys,
/// freshness) and every revocation since stays in force, so a rollback
/// cannot bring a revoked binding back. Naming the running canary ends the
/// canary instead; production is untouched.
pub(crate) async fn rollback(core: &Core, expected_generation: &str) -> wire::ModelRegistryView {
    let mut history = load_history(core);
    if let Some(canary) = history.canary().cloned()
        && canary.generation == expected_generation
    {
        if let Err(running) = core.gateway.clear_canary(expected_generation) {
            return refusal(
                "REGISTRY_ACTIVATION_CONFLICT",
                format!("the canary running is `{running}`, not `{expected_generation}`"),
            );
        }
        history.entries.push(Activation {
            generation: canary.generation.clone(),
            signed_ref: canary.signed_ref.clone(),
            kind: CANARY_ENDED.to_owned(),
            previous: canary.previous.clone(),
            at_ms: modbit_domain::Timestamp::now().0,
        });
        if let Err(e) = save_history(core, &history) {
            eprintln!("modbit-core: recording the canary's end failed: {e}");
        }
        let mut v = current(core);
        v.activation = CANARY_ENDED.to_owned();
        return v;
    }
    let Some(current) = history.production().cloned() else {
        return refusal(
            "REGISTRY_NO_HISTORY",
            "nothing has been activated to roll back".into(),
        );
    };
    if expected_generation.is_empty() || expected_generation != current.generation {
        return refusal(
            "REGISTRY_ACTIVATION_CONFLICT",
            format!(
                "expected generation `{expected_generation}` but `{}` is active",
                current.generation
            ),
        );
    }
    let Some(target) = history
        .entries
        .iter()
        .rev()
        .find(|a| a.generation == current.previous && a.kind != CANARY_ENDED)
        .cloned()
    else {
        return refusal(
            "REGISTRY_NO_PREVIOUS_GOOD",
            format!(
                "`{}` replaced nothing this Core recorded",
                current.generation
            ),
        );
    };
    let bytes = {
        let store = core.store.lock().await;
        store.objects().get(&target.signed_ref)
    };
    let Ok(signed) = bytes
        .map_err(|e| e.to_string())
        .and_then(|b| serde_json::from_slice::<SignedRegistry>(&b).map_err(|e| e.to_string()))
    else {
        return refusal(
            "REGISTRY_NO_PREVIOUS_GOOD",
            "the previous generation's signed document is unreadable".into(),
        );
    };
    let now = modbit_domain::Timestamp::now().0;
    let mut registry = match modbit_providers::registry::activate(&signed, &trusted_keys(), now) {
        Ok(r) => r,
        Err(r) => {
            return refusal(
                r.code(),
                format!("the previous generation no longer verifies: {r:?}"),
            );
        }
    };
    if let Err((code, detail)) = keep_revocations(&mut registry, &history) {
        return refusal(code, detail);
    }
    install(
        core,
        registry,
        &signed,
        expected_generation,
        "ROLLED_BACK",
        &mut history,
    )
    .await
}

/// At boot: the production generation in force and the canary running,
/// each verified again with every revocation kept; nothing of either when
/// it no longer verifies.
pub(crate) async fn restore(core: &Core) {
    let history = load_history(core);
    let Some(production) = history.production().cloned() else {
        return;
    };
    let Some(registry) = reverify(core, &production, &history).await else {
        return;
    };
    let _ = core.gateway.install_registry(registry, None);
    eprintln!(
        "modbit-core: registry generation {} restored",
        production.generation
    );
    if let Some(canary) = history.canary().cloned()
        && let Some(registry) = reverify(core, &canary, &history).await
        && core
            .gateway
            .install_canary(registry, &canary.previous)
            .is_ok()
    {
        eprintln!(
            "modbit-core: canary generation {} restored",
            canary.generation
        );
    }
}

async fn reverify(core: &Core, a: &Activation, history: &History) -> Option<ModelRegistry> {
    let bytes = {
        let store = core.store.lock().await;
        store.objects().get(&a.signed_ref)
    };
    let Ok(signed) = bytes
        .map_err(|e| e.to_string())
        .and_then(|b| serde_json::from_slice::<SignedRegistry>(&b).map_err(|e| e.to_string()))
    else {
        eprintln!(
            "modbit-core: registry generation {}'s document is unreadable; not restored",
            a.generation
        );
        return None;
    };
    let now = modbit_domain::Timestamp::now().0;
    match modbit_providers::registry::activate(&signed, &trusted_keys(), now) {
        Ok(mut registry) => keep_revocations(&mut registry, history)
            .is_ok()
            .then_some(registry),
        Err(r) => {
            eprintln!(
                "modbit-core: registry generation {} no longer verifies ({}); not restored",
                a.generation,
                r.code()
            );
            None
        }
    }
}

/// The active registry as a client sees it, or an inactive view when no
/// document has been activated.
pub(crate) fn current(core: &Core) -> wire::ModelRegistryView {
    let mut v = core
        .gateway
        .registry()
        .as_ref()
        .map_or_else(wire::ModelRegistryView::default, view);
    v.canary_generation = core
        .gateway
        .canary()
        .map(|c| c.generation().to_owned())
        .unwrap_or_default();
    v
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
        ..Default::default()
    }
}
