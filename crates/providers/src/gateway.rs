//! Provider Gateway (docs/15): endpoint registry with a capability catalog
//! (REQ-EV-0028, REQ-EV-0189), provider-neutral requested/resolved metadata
//! (REQ-EV-0112), bounded retry with jitter before the first token, stream
//! timeout, cancellation, and rolling health per endpoint. Raw credentials
//! are resolved per request and never appear in events, errors or logs.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::contract::{ModelEvent, ModelRequest, ProviderKind, SecretHandle, stop};
use crate::sse::SseParser;

/// What a model can do (docs/15 "Model Registry", docs/25 "Model capability metadata").
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ModelCapability {
    /// Model id.
    pub model: String,
    /// Context window (tokens).
    pub context_tokens: u32,
    /// Output cap (tokens).
    pub max_output_tokens: u32,
    /// Tool calling.
    pub tools: bool,
    /// Parallel tool calls.
    pub parallel_tools: bool,
    /// Vision / image input.
    pub vision: bool,
    /// Media modalities accepted as input (`text`, `image`, `pdf`, ...).
    pub input_modalities: Vec<String>,
    /// Exposes reasoning / accepts an effort setting.
    pub reasoning: bool,
    /// Structured output (JSON mode).
    pub structured_output: bool,
    /// Agent-loop support (multi-turn tool use).
    pub agent_loop: bool,
    /// USD per million input tokens (nominal; economics only).
    pub input_price_per_mtok: f64,
    /// USD per million output tokens.
    pub output_price_per_mtok: f64,
    /// The output budget one request to this model asks for, in tokens;
    /// 0 = derived ([`ModelCapability::output_budget`]). Never above
    /// `max_output_tokens`.
    #[serde(default)]
    pub output_budget_tokens: u32,
    /// The whole-request timeout for this model, in milliseconds; 0 =
    /// derived ([`ModelCapability::timeout_ms`]).
    #[serde(default)]
    pub request_timeout_ms: u64,
    /// The reasoning effort (`low`|`medium`|`high`) a request to this model
    /// carries unless the caller says otherwise; only honoured for a model
    /// that exposes reasoning. `None` = the provider's own default.
    #[serde(default)]
    pub default_reasoning_effort: Option<String>,
    /// The service tier a request to this model carries unless the caller
    /// says otherwise. `None` = the provider's own default.
    #[serde(default)]
    pub default_service_tier: Option<String>,
    /// How a request to this model projects the tool surface (PX-114):
    /// `direct` or `exec_only`. `None` = the Core's default. A small model
    /// that follows a few schemas better than thirty is the case for
    /// `exec_only`; the paired trial decides whether any model earns it.
    #[serde(default)]
    pub projection_mode: Option<String>,
    /// The most bytes of tool schemas one request to this model may carry
    /// (PX-114); 0 = the Core's default budget.
    #[serde(default)]
    pub max_projection_bytes: u32,
}

/// The output budget a request asks for when the catalog entry names none:
/// room for a large file write without asking for the model's whole ceiling.
pub const DEFAULT_OUTPUT_BUDGET_TOKENS: u32 = 16_384;
/// The request timeout for a small output budget; a larger budget earns
/// proportionally more time ([`ModelCapability::timeout_ms`]).
pub const BASE_REQUEST_TIMEOUT_MS: u64 = 120_000;
/// Milliseconds granted per output token beyond the first 4096 (a floor of
/// about 33 tokens per second; a model slower than that sets its own
/// `request_timeout_ms`).
pub const TIMEOUT_MS_PER_EXTRA_OUTPUT_TOKEN: u64 = 30;
/// Longest a provider's `Retry-After` is honoured; a provider asking for more
/// is reported as rate limited rather than waited on.
pub const MAX_RETRY_AFTER: Duration = Duration::from_secs(300);
/// Concurrent requests per endpoint when the endpoint names no limit.
pub const DEFAULT_ENDPOINT_CONCURRENCY: u32 = 16;

impl ModelCapability {
    /// The output budget one request asks for: the entry's own, else
    /// [`DEFAULT_OUTPUT_BUDGET_TOKENS`], and never more than the model's
    /// ceiling.
    #[must_use]
    pub fn output_budget(&self) -> u32 {
        let asked = if self.output_budget_tokens > 0 {
            self.output_budget_tokens
        } else {
            DEFAULT_OUTPUT_BUDGET_TOKENS
        };
        asked.min(self.max_output_tokens.max(1))
    }

    /// The whole-request timeout: the entry's own, else a base plus time in
    /// proportion to the output budget beyond 4096 tokens.
    #[must_use]
    pub fn timeout_ms(&self) -> u64 {
        if self.request_timeout_ms > 0 {
            return self.request_timeout_ms;
        }
        BASE_REQUEST_TIMEOUT_MS
            + u64::from(self.output_budget().saturating_sub(4096))
                * TIMEOUT_MS_PER_EXTRA_OUTPUT_TOKEN
    }

    /// The reasoning effort and service tier a request to this model carries
    /// by default. An effort is dropped for a model that exposes no
    /// reasoning, because the route would refuse it (`CapabilityMismatch`).
    #[must_use]
    pub fn execution_preference(&self) -> (Option<String>, Option<String>) {
        (
            self.default_reasoning_effort
                .clone()
                .filter(|_| self.reasoning),
            self.default_service_tier.clone(),
        )
    }
}

/// How the credential is presented on the wire.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthScheme {
    /// The family's own header: `Authorization: Bearer` for OpenAI,
    /// `x-api-key` for Anthropic.
    #[default]
    Native,
    /// `Authorization: Bearer` regardless of family, for Anthropic-protocol
    /// gateways that authenticate that way.
    Bearer,
}

/// One registered endpoint.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Endpoint {
    /// Name used by `ModelPolicy.endpoint`.
    pub name: String,
    /// Wire family.
    pub kind: ProviderKind,
    /// Base URL (no trailing slash), e.g. `https://api.openai.com`. A base
    /// that already names its API version (`https://api.z.ai/api/paas/v4`)
    /// is used as is; otherwise the family's `/v1` is appended ([`wire_url`]).
    pub base_url: String,
    /// Credential.
    pub credential: SecretHandle,
    /// Models served, with capabilities.
    pub models: Vec<ModelCapability>,
    /// Bounded retries before the first token (rate limit / transient errors).
    pub max_retries: u32,
    /// How the credential is sent.
    #[serde(default)]
    pub auth: AuthScheme,
    /// Provider-specific request fields a compatible gateway needs (for
    /// example `{"tool_stream": true}`), added to every request body of this
    /// endpoint where the canonical body has no such key. Never a secret.
    #[serde(default)]
    pub extra_body: serde_json::Map<String, serde_json::Value>,
    /// Requests this endpoint is asked to serve at once; the rest wait for a
    /// slot inside their own deadline. 0 = [`DEFAULT_ENDPOINT_CONCURRENCY`].
    #[serde(default)]
    pub max_concurrency: u32,
}

/// The request URL for a wire family on a base URL: the family's path under
/// the provider's `/v1`, unless the base's last segment already names an API
/// version (`v1`, `v4`, …), in which case the path is appended directly.
#[must_use]
pub fn wire_url(kind: ProviderKind, base_url: &str) -> String {
    let base = base_url.trim_end_matches('/');
    let versioned = base.rsplit('/').next().is_some_and(|seg| {
        seg.len() > 1 && seg.starts_with('v') && seg[1..].chars().all(|c| c.is_ascii_digit())
    });
    let path = match kind {
        ProviderKind::OpenAi => "chat/completions",
        ProviderKind::Anthropic => "messages",
    };
    if versioned {
        format!("{base}/{path}")
    } else {
        format!("{base}/v1/{path}")
    }
}

/// Rolling health (docs/15 "Health and failover").
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct EndpointHealth {
    /// Requests started.
    pub requests: u64,
    /// Streams that completed normally.
    pub successes: u64,
    /// Streams that ended in error (after retries).
    pub failures: u64,
    /// Streams interrupted mid-way (disconnect/timeout after first token).
    pub interruptions: u64,
    /// Cancellations by the caller.
    pub cancellations: u64,
    /// Rate-limit responses seen.
    pub rate_limited: u64,
    /// Last first-token latency in ms.
    pub last_first_token_ms: Option<u64>,
    /// Tool calls whose arguments were not valid JSON.
    pub invalid_tool_calls: u64,
}

/// Why a request could not be routed (REQ-EV-0028/0189 "route away").
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum RouteError {
    /// Unknown endpoint.
    #[error("unknown endpoint `{0}`")]
    UnknownEndpoint(String),
    /// Unknown model on that endpoint.
    #[error("endpoint `{endpoint}` does not serve model `{model}`")]
    UnknownModel {
        /// Endpoint.
        endpoint: String,
        /// Model.
        model: String,
    },
    /// The active Model Registry does not allow this dispatch (REQ-EPR-002):
    /// the binding is revoked, absent, or does not satisfy the request.
    #[error("registry refuses `{endpoint}`/`{model}`: {detail} ({code})")]
    RegistryRefused {
        /// Endpoint.
        endpoint: String,
        /// Model.
        model: String,
        /// Stable code (`MODEL_REVOKED`, `MODEL_NOT_IN_REGISTRY`, `MODEL_NOT_ELIGIBLE`).
        code: &'static str,
        /// What the registry said.
        detail: String,
    },
    /// Organization policy blocks the endpoint/model (REQ-EV-0031); no request can widen it.
    #[error("endpoint `{endpoint}` model `{model}` is blocked by organization policy ({rule})")]
    PolicyBlocked {
        /// Endpoint.
        endpoint: String,
        /// Model.
        model: String,
        /// The rule that matched.
        rule: String,
    },
    /// The model lacks a required capability.
    #[error("model `{model}` lacks required capability `{capability}`")]
    CapabilityMismatch {
        /// Model.
        model: String,
        /// Capability name.
        capability: String,
    },
    /// No credential could be resolved.
    #[error("endpoint `{0}` has no usable credential")]
    MissingCredential(String),
}

/// What a request needs from the model (checked against the catalog before dispatch).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Requirements {
    /// Needs tool calling.
    pub tools: bool,
    /// Needs vision.
    pub vision: bool,
    /// Needs structured output.
    pub structured_output: bool,
    /// Input modalities present in the request.
    pub input_modalities: Vec<String>,
    /// The registry role the request is made in (`None` = `solver`, a run
    /// turn). A Core-originated call in another role — the compaction
    /// summarizer — is routed to a model the registry binds for that role.
    pub role: Option<String>,
}

/// Requested vs resolved route (REQ-EV-0112 routing record).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RouteRecord {
    /// Endpoint.
    pub endpoint: String,
    /// Provider family.
    pub kind: ProviderKind,
    /// Requested model.
    pub requested_model: String,
    /// Requested reasoning effort.
    pub requested_reasoning_effort: Option<String>,
    /// Requested service tier.
    pub requested_service_tier: Option<String>,
    /// Model the provider reported (from metadata), when known.
    pub resolved_model: Option<String>,
    /// Service tier the provider reported.
    pub resolved_service_tier: Option<String>,
    /// Policy reason text.
    pub reason: String,
    /// Retries performed before the first token.
    pub retries: u32,
    /// The provider's own request id, when it returned one: the handle its
    /// support and its logs use, which ours cannot substitute for.
    pub provider_request_id: Option<String>,
}

/// An endpoint's concurrency limit and the semaphore that enforces it.
type EndpointSlots = (u32, Arc<tokio::sync::Semaphore>);

/// The gateway.
#[derive(Clone)]
pub struct ProviderGateway {
    endpoints: Arc<Mutex<BTreeMap<String, Endpoint>>>,
    health: Arc<Mutex<BTreeMap<String, EndpointHealth>>>,
    client: reqwest::Client,
    /// The active Model Registry, when a signed configuration has been
    /// activated. Absent means the build's own defaults are in force.
    registry: Arc<Mutex<Option<crate::registry::ModelRegistry>>>,
    /// REQ-EPR-012: a promoted generation in its canary stage, beside the
    /// production one. Routing uses it only for requests whose policy in
    /// force allows canary routing; replacing production ends it.
    canary: Arc<Mutex<Option<crate::registry::ModelRegistry>>>,
    policy: Arc<OrgModelPolicy>,
    /// One semaphore per endpoint, sized by its `max_concurrency`: what the
    /// endpoint is asked to serve at once, whatever number of runs want it.
    slots: Arc<Mutex<BTreeMap<String, EndpointSlots>>>,
    /// Per endpoint, the instant before which no request is sent: set when a
    /// provider answers a rate limit with `Retry-After`, honoured by every
    /// request to that endpoint, not only the one that was told.
    cooldown: Arc<Mutex<BTreeMap<String, Instant>>>,
    /// The credential broker (REQ-PX-130): every endpoint's credential is
    /// registered with it, and the gateway obtains the key for each request
    /// through it — scoped to the task that makes the request, to the
    /// endpoint, to the purpose — so a revoked or rotated credential applies
    /// to the very next request.
    broker: Arc<modbit_secrets::CredentialBroker>,
}

/// The broker's identity of an endpoint's credential.
#[must_use]
pub fn credential_id(endpoint: &str) -> modbit_secrets::CredentialId {
    modbit_secrets::CredentialId::new(modbit_secrets::Kind::Provider, endpoint)
}

/// Register `ep`'s credential with `broker` (a rotation when it already is).
fn register_credential(broker: &modbit_secrets::CredentialBroker, ep: &Endpoint) {
    broker.register(modbit_secrets::Registration {
        id: credential_id(&ep.name),
        kind: modbit_secrets::Kind::Provider,
        source: ep.credential.clone(),
        audience: format!("provider:{}", ep.name),
    });
}

/// The principal a request speaks for: its task (the request id is
/// `<task id>:<ordinal>`), else the Core itself.
fn principal_of(request_id: &str) -> String {
    match request_id.split_once(':') {
        Some((task, rest)) if task.len() == 36 && rest.chars().all(|c| c.is_ascii_digit()) => {
            format!("task:{task}")
        }
        _ => "core".to_owned(),
    }
}

/// Organization model policy (REQ-EV-0031): block or require providers,
/// endpoints and models. Evaluated inside `route` before any capability or
/// credential check, so a task or profile request can never widen it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct OrgModelPolicy {
    /// Blocked selectors: `provider/model`, `provider/*`, `endpoint:<name>` or `*`.
    pub block: Vec<String>,
    /// When set, only these endpoint names may be routed to.
    pub require_endpoints: Vec<String>,
}

impl OrgModelPolicy {
    /// A version a plan can pin: a digest of the policy's exact content, so
    /// the same policy always names itself the same way and a changed one
    /// never passes for it.
    #[must_use]
    pub fn version(&self) -> String {
        use sha2::{Digest, Sha256};
        let mut h = Sha256::new();
        h.update(self.block.join(",").as_bytes());
        h.update([0]);
        h.update(self.require_endpoints.join(",").as_bytes());
        format!("policy-{}", &hex::encode(h.finalize())[..16])
    }

    /// Parse `MODBIT_MODEL_POLICY`: `block=anthropic/*,openai/gpt-5-mini;require=openai`.
    #[must_use]
    pub fn parse(spec: &str) -> Self {
        let mut p = Self::default();
        for clause in spec.split(';') {
            let Some((k, v)) = clause.split_once('=') else {
                continue;
            };
            let items = v
                .split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_owned);
            match k.trim() {
                "block" => p.block.extend(items),
                "require" => p.require_endpoints.extend(items),
                _ => {}
            }
        }
        p
    }

    /// From the environment (`MODBIT_MODEL_POLICY`), empty when unset.
    #[must_use]
    pub fn from_env() -> Self {
        std::env::var("MODBIT_MODEL_POLICY")
            .ok()
            .map(|s| Self::parse(&s))
            .unwrap_or_default()
    }

    /// The rule blocking `endpoint`/`kind`/`model`, if any.
    #[must_use]
    pub fn blocking_rule(&self, endpoint: &str, kind: ProviderKind, model: &str) -> Option<String> {
        let provider = format!("{kind:?}").to_lowercase();
        for rule in &self.block {
            let hit = rule == "*"
                || rule.strip_prefix("endpoint:") == Some(endpoint)
                || rule == &format!("{provider}/*")
                || rule == &format!("{provider}/{model}");
            if hit {
                return Some(format!("block={rule}"));
            }
        }
        if !self.require_endpoints.is_empty()
            && !self.require_endpoints.iter().any(|e| e == endpoint)
        {
            return Some(format!("require={}", self.require_endpoints.join(",")));
        }
        None
    }
}

/// A running stream: events plus the route record filled as metadata arrives.
pub struct ModelStream {
    /// Events in order; ends with `Completed` or `Error`.
    pub events: mpsc::Receiver<ModelEvent>,
    /// Route record (requested values; resolved values update as metadata arrives).
    pub route: Arc<Mutex<RouteRecord>>,
}

impl ProviderGateway {
    /// Build over a set of endpoints.
    #[must_use]
    pub fn new(endpoints: Vec<Endpoint>) -> Self {
        let broker = Arc::new(modbit_secrets::CredentialBroker::new());
        for e in &endpoints {
            register_credential(&broker, e);
        }
        let map = endpoints
            .into_iter()
            .map(|e| (e.name.clone(), e))
            .collect::<BTreeMap<_, _>>();
        Self {
            broker,
            endpoints: Arc::new(Mutex::new(map)),
            health: Arc::new(Mutex::new(BTreeMap::new())),
            policy: Arc::new(OrgModelPolicy::default()),
            client: reqwest::Client::builder()
                .connect_timeout(Duration::from_secs(10))
                .build()
                .expect("reqwest client"),
            registry: Arc::new(Mutex::new(None)),
            canary: Arc::new(Mutex::new(None)),
            slots: Arc::new(Mutex::new(BTreeMap::new())),
            cooldown: Arc::new(Mutex::new(BTreeMap::new())),
        }
    }

    /// Use `broker` as the credential broker (the Core shares one across
    /// everything that holds a credential): every endpoint's credential is
    /// registered with it.
    #[must_use]
    pub fn with_broker(mut self, broker: Arc<modbit_secrets::CredentialBroker>) -> Self {
        for e in self.endpoints.lock().expect("endpoints").values() {
            register_credential(&broker, e);
        }
        self.broker = broker;
        self
    }

    /// The credential broker.
    #[must_use]
    pub fn broker(&self) -> &Arc<modbit_secrets::CredentialBroker> {
        &self.broker
    }

    /// Whether the endpoint has a credential it can use now (or needs none).
    #[must_use]
    pub fn credential_configured(&self, endpoint: &str) -> bool {
        let eps = self.endpoints.lock().expect("endpoints");
        match eps.get(endpoint) {
            Some(ep) if matches!(ep.credential, SecretHandle::None) => false,
            Some(_) => self.broker.configured(&credential_id(endpoint)),
            None => false,
        }
    }

    /// Wait until `ep` may be sent a request: its cooldown (a provider's
    /// `Retry-After`) has passed and one of its concurrency slots is free.
    /// The caller races this against its own deadline and cancellation, so a
    /// queue never outlives the request that joined it.
    async fn admit(&self, ep: &Endpoint) -> tokio::sync::OwnedSemaphorePermit {
        let size = if ep.max_concurrency == 0 {
            DEFAULT_ENDPOINT_CONCURRENCY
        } else {
            ep.max_concurrency
        };
        loop {
            let until = self
                .cooldown
                .lock()
                .expect("cooldown")
                .get(&ep.name)
                .copied();
            if let Some(until) = until
                && until > Instant::now()
            {
                tokio::time::sleep_until(until.into()).await;
                continue;
            }
            let semaphore = {
                let mut slots = self.slots.lock().expect("slots");
                let entry = slots.entry(ep.name.clone()).or_insert_with(|| {
                    (size, Arc::new(tokio::sync::Semaphore::new(size as usize)))
                });
                if entry.0 != size {
                    // The endpoint was reconfigured with another limit: a
                    // new semaphore, while requests already admitted finish
                    // on the old one.
                    *entry = (size, Arc::new(tokio::sync::Semaphore::new(size as usize)));
                }
                Arc::clone(&entry.1)
            };
            let permit = semaphore.acquire_owned().await.expect("never closed");
            // A rate limit may have arrived while this request queued.
            let cooling = self
                .cooldown
                .lock()
                .expect("cooldown")
                .get(&ep.name)
                .is_some_and(|until| *until > Instant::now());
            if !cooling {
                return permit;
            }
            drop(permit);
        }
    }

    /// Registered endpoints (credentials redacted by `Debug`).
    #[must_use]
    pub fn endpoints(&self) -> Vec<Endpoint> {
        self.endpoints
            .lock()
            .expect("endpoints")
            .values()
            .cloned()
            .collect()
    }

    /// Register or replace an endpoint at runtime (REQ-PX-022: provider
    /// setup hands the Core a credential it holds in memory only; nothing is
    /// written to the log, the object store or any file by this call).
    pub fn configure_endpoint(&self, endpoint: Endpoint) {
        // A rotation when the endpoint was configured before.
        register_credential(&self.broker, &endpoint);
        self.endpoints
            .lock()
            .expect("endpoints")
            .insert(endpoint.name.clone(), endpoint);
    }

    /// Forget an endpoint, credential included. What provider setup could not
    /// confirm is not left registered.
    pub fn remove_endpoint(&self, name: &str) -> bool {
        self.broker.forget(&credential_id(name));
        self.endpoints
            .lock()
            .expect("endpoints")
            .remove(name)
            .is_some()
    }

    /// Health snapshot.
    #[must_use]
    pub fn health(&self, endpoint: &str) -> EndpointHealth {
        self.health
            .lock()
            .expect("health")
            .get(endpoint)
            .cloned()
            .unwrap_or_default()
    }

    /// Attach the organization model policy (REQ-EV-0031).
    #[must_use]
    pub fn with_policy(mut self, policy: OrgModelPolicy) -> Self {
        self.policy = Arc::new(policy);
        self
    }

    /// The organization model policy in force.
    #[must_use]
    pub fn policy(&self) -> &OrgModelPolicy {
        &self.policy
    }

    /// Activate a signed configuration document (REQ-EPR-002). The previously
    /// active registry stays in force until a document verifies whole, so a
    /// bad generation never leaves the product without one.
    ///
    /// # Errors
    /// The document does not verify, is stale, carries empirical outcome
    /// fields, or leaves a required role unbound.
    pub fn activate_registry(
        &self,
        signed: &crate::registry::SignedRegistry,
        trusted: &std::collections::BTreeMap<String, [u8; 32]>,
        now_ms: i64,
    ) -> Result<crate::registry::ModelRegistry, crate::registry::RegistryRefused> {
        let registry = crate::registry::activate(signed, trusted, now_ms)?;
        *self.registry.lock().expect("registry") = Some(registry.clone());
        Ok(registry)
    }

    /// Install an already verified registry, as a compare-and-swap on the
    /// active generation when `expected` is given (REQ-EPR-012: two
    /// activations cannot both win).
    ///
    /// # Errors
    /// The active generation (empty for none) when it is not `expected`.
    pub fn install_registry(
        &self,
        registry: crate::registry::ModelRegistry,
        expected: Option<&str>,
    ) -> Result<(), String> {
        let mut active = self.registry.lock().expect("registry");
        let current = active
            .as_ref()
            .map(|r| r.generation().to_owned())
            .unwrap_or_default();
        if let Some(expected) = expected
            && expected != current
        {
            return Err(current);
        }
        *active = Some(registry);
        // A canary is measured against the generation it would replace; a
        // new production generation ends it.
        *self.canary.lock().expect("canary") = None;
        Ok(())
    }

    /// The active Model Registry, when one has been activated.
    #[must_use]
    pub fn registry(&self) -> Option<crate::registry::ModelRegistry> {
        self.registry.lock().expect("registry").clone()
    }

    /// The generation in its canary stage, when there is one.
    #[must_use]
    pub fn canary(&self) -> Option<crate::registry::ModelRegistry> {
        self.canary.lock().expect("canary").clone()
    }

    /// Install a verified generation as the canary beside production: a
    /// compare-and-swap on the production generation, refused while another
    /// canary is running.
    ///
    /// # Errors
    /// What is in the way: the production generation that is not
    /// `expected_production`, or the canary already running.
    pub fn install_canary(
        &self,
        registry: crate::registry::ModelRegistry,
        expected_production: &str,
    ) -> Result<(), String> {
        let active = self.registry.lock().expect("registry");
        let current = active
            .as_ref()
            .map(|r| r.generation().to_owned())
            .unwrap_or_default();
        if current.is_empty() || current != expected_production {
            return Err(format!(
                "production generation `{current}` is active, not `{expected_production}`"
            ));
        }
        let mut canary = self.canary.lock().expect("canary");
        if let Some(c) = canary.as_ref() {
            return Err(format!(
                "generation `{}` is already in its canary",
                c.generation()
            ));
        }
        *canary = Some(registry);
        Ok(())
    }

    /// End the canary `expected` without promoting it.
    ///
    /// # Errors
    /// The canary running (empty for none) when it is not `expected`.
    pub fn clear_canary(&self, expected: &str) -> Result<crate::registry::ModelRegistry, String> {
        let mut canary = self.canary.lock().expect("canary");
        match canary.as_ref() {
            Some(c) if c.generation() == expected => Ok(canary.take().expect("checked")),
            other => Err(other.map(|c| c.generation().to_owned()).unwrap_or_default()),
        }
    }

    /// Promote the canary `canary_generation` to production, in one step, as
    /// a compare-and-swap on both.
    ///
    /// # Errors
    /// The canary is not the one named, or production is not
    /// `expected_production`.
    pub fn promote_canary(
        &self,
        canary_generation: &str,
        expected_production: &str,
    ) -> Result<crate::registry::ModelRegistry, String> {
        let mut active = self.registry.lock().expect("registry");
        let mut canary = self.canary.lock().expect("canary");
        let current = active
            .as_ref()
            .map(|r| r.generation().to_owned())
            .unwrap_or_default();
        if current != expected_production {
            return Err(format!(
                "production generation `{current}` is active, not `{expected_production}`"
            ));
        }
        match canary.as_ref() {
            Some(c) if c.generation() == canary_generation => {}
            other => {
                return Err(format!(
                    "the canary running is `{}`, not `{canary_generation}`",
                    other.map(|c| c.generation().to_owned()).unwrap_or_default()
                ));
            }
        }
        let promoted = canary.take().expect("checked");
        *active = Some(promoted.clone());
        Ok(promoted)
    }

    /// What one endpoint's catalog says about a model, when it lists it.
    #[must_use]
    pub fn capability(&self, endpoint: &str, model: &str) -> Option<ModelCapability> {
        self.endpoints
            .lock()
            .expect("endpoints")
            .get(endpoint)?
            .models
            .iter()
            .find(|m| m.model == model)
            .cloned()
    }

    /// Input modalities the request itself carries, deduplicated.
    fn media_modalities(req: &ModelRequest) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for m in &req.messages {
            for p in &m.parts {
                if let Some(modality) = p.modality()
                    && !out.iter().any(|x| x == modality)
                {
                    out.push(modality.to_owned());
                }
            }
        }
        out
    }

    /// Resolve the route: organization policy, endpoint, model and capability checks (no network).
    pub fn route(
        &self,
        req: &ModelRequest,
        needs: &Requirements,
    ) -> Result<(Endpoint, ModelCapability, RouteRecord), RouteError> {
        let ep = self
            .endpoints
            .lock()
            .expect("endpoints")
            .get(&req.model_policy.endpoint)
            .cloned()
            .ok_or_else(|| RouteError::UnknownEndpoint(req.model_policy.endpoint.clone()))?;
        if let Some(rule) = self
            .policy
            .blocking_rule(&ep.name, ep.kind, &req.model_policy.model)
        {
            return Err(RouteError::PolicyBlocked {
                endpoint: ep.name.clone(),
                model: req.model_policy.model.clone(),
                rule,
            });
        }
        // A configuration generation can withdraw a binding between one
        // dispatch and the next, so the registry is consulted per request
        // rather than at startup (REQ-EPR-002).
        let canary = self.canary.lock().expect("canary").clone();
        if let Some(registry) = self.registry.lock().expect("registry").as_ref() {
            let wanted = crate::registry::Needs {
                tools: needs.tools || !req.tool_projection.is_empty(),
                vision: needs.vision,
                structured_output: needs.structured_output,
                min_context_tokens: 0,
                execution_profile: None,
            };
            // A binding the canary holds passes too: routing gives it only
            // to requests whose policy allows canary routing, and the canary
            // keeps every revocation production has (REQ-EPR-012).
            let role = needs.role.as_deref().unwrap_or("solver");
            let checked = registry.check_dispatch(&ep.name, &req.model_policy.model, role, &wanted);
            let checked = match (checked, canary.as_ref()) {
                (Err(e), Some(c)) => c
                    .check_dispatch(&ep.name, &req.model_policy.model, role, &wanted)
                    .map_err(|_| e),
                (r, _) => r,
            };
            if let Err((code, detail)) = checked {
                return Err(RouteError::RegistryRefused {
                    endpoint: ep.name.clone(),
                    model: req.model_policy.model.clone(),
                    code,
                    detail,
                });
            }
        }
        let cap = ep
            .models
            .iter()
            .find(|m| m.model == req.model_policy.model)
            .cloned()
            .ok_or_else(|| RouteError::UnknownModel {
                endpoint: ep.name.clone(),
                model: req.model_policy.model.clone(),
            })?;
        let mismatch = |capability: &str| RouteError::CapabilityMismatch {
            model: cap.model.clone(),
            capability: capability.into(),
        };
        // The request decides what it needs: media in the messages requires the
        // matching input modality (and vision for images) whether or not the
        // caller asked for it (REQ-EV-0188 / 0189).
        let carried = Self::media_modalities(req);
        if (needs.tools || !req.tool_projection.is_empty()) && !cap.tools {
            return Err(mismatch("tools"));
        }
        if (needs.vision || carried.iter().any(|m| m == "image")) && !cap.vision {
            return Err(mismatch("vision"));
        }
        // A catalog entry can claim structured output; the adapter has to
        // implement it. A wire that does not is refused here, whatever the
        // entry says, so a request for JSON never silently degrades to text.
        if (needs.structured_output || req.response_format.as_deref() == Some("json_object"))
            && (!cap.structured_output || !ep.kind.implements_structured_output())
        {
            return Err(mismatch("structured_output"));
        }
        for m in needs.input_modalities.iter().chain(carried.iter()) {
            if !cap.input_modalities.iter().any(|x| x == m) {
                return Err(mismatch(&format!("input_modality:{m}")));
            }
        }
        if req.model_policy.reasoning_effort.is_some() && !cap.reasoning {
            return Err(mismatch("reasoning"));
        }
        if !matches!(ep.credential, SecretHandle::None)
            && !self.broker.usable(&credential_id(&ep.name))
        {
            return Err(RouteError::MissingCredential(ep.name.clone()));
        }
        let record = RouteRecord {
            endpoint: ep.name.clone(),
            kind: ep.kind,
            requested_model: req.model_policy.model.clone(),
            requested_reasoning_effort: req.model_policy.reasoning_effort.clone(),
            requested_service_tier: req.model_policy.service_tier.clone(),
            resolved_model: None,
            resolved_service_tier: None,
            provider_request_id: None,
            reason: format!(
                "policy endpoint `{}` serves `{}`; capabilities satisfied",
                ep.name, cap.model
            ),
            retries: 0,
        };
        Ok((ep, cap, record))
    }

    /// Start a streaming request. Returns immediately; events arrive on the
    /// channel. Cancel via `cancel`; the stream ends with `Completed{cancelled}`.
    pub fn stream(
        &self,
        req: ModelRequest,
        needs: &Requirements,
        cancel: CancellationToken,
    ) -> Result<ModelStream, RouteError> {
        let (ep, _cap, record) = self.route(&req, needs)?;
        let route = Arc::new(Mutex::new(record));
        let (tx, rx) = mpsc::channel(256);
        let gw = self.clone();
        let route_task = Arc::clone(&route);
        tokio::spawn(async move {
            gw.run(ep, req, tx, cancel, route_task).await;
        });
        Ok(ModelStream { events: rx, route })
    }

    fn bump(&self, endpoint: &str, f: impl FnOnce(&mut EndpointHealth)) {
        let mut h = self.health.lock().expect("health");
        f(h.entry(endpoint.to_owned()).or_default());
    }

    async fn run(
        &self,
        ep: Endpoint,
        req: ModelRequest,
        tx: mpsc::Sender<ModelEvent>,
        cancel: CancellationToken,
        route: Arc<Mutex<RouteRecord>>,
    ) {
        self.bump(&ep.name, |h| h.requests += 1);
        let deadline = Instant::now() + Duration::from_millis(req.timeout_ms.max(1));
        let mut attempt = 0u32;
        loop {
            let outcome = tokio::select! {
                _ = cancel.cancelled() => Attempt::Cancelled,
                _ = tokio::time::sleep_until(deadline.into()) => Attempt::Timeout,
                r = async {
                    // Queue behind the endpoint's cooldown and concurrency
                    // limit; the slot is held until this attempt's stream
                    // ends and is free again during a retry's backoff.
                    let _slot = self.admit(&ep).await;
                    self.attempt(&ep, &req, &tx, &cancel, &route, deadline).await
                } => r,
            };
            match outcome {
                Attempt::Done => {
                    self.bump(&ep.name, |h| h.successes += 1);
                    return;
                }
                Attempt::Cancelled => {
                    self.bump(&ep.name, |h| h.cancellations += 1);
                    let _ = tx
                        .send(ModelEvent::Completed {
                            stop_reason: stop::CANCELLED.into(),
                        })
                        .await;
                    return;
                }
                Attempt::Timeout => {
                    self.bump(&ep.name, |h| h.interruptions += 1);
                    let _ = tx
                        .send(ModelEvent::Error {
                            code: "TIMEOUT".into(),
                            message: format!("no completion within {} ms", req.timeout_ms),
                            retryable: false,
                        })
                        .await;
                    return;
                }
                Attempt::Interrupted(msg) => {
                    self.bump(&ep.name, |h| h.interruptions += 1);
                    let _ = tx
                        .send(ModelEvent::Error {
                            code: "STREAM_INTERRUPTED".into(),
                            message: msg,
                            retryable: false,
                        })
                        .await;
                    return;
                }
                Attempt::Failed { code, message } => {
                    self.bump(&ep.name, |h| h.failures += 1);
                    let _ = tx
                        .send(ModelEvent::Error {
                            code,
                            message,
                            retryable: false,
                        })
                        .await;
                    return;
                }
                Attempt::Retryable {
                    code,
                    message,
                    retry_after,
                } => {
                    if code == "RATE_LIMITED" {
                        self.bump(&ep.name, |h| h.rate_limited += 1);
                    }
                    // The provider said how long to wait: every request to
                    // this endpoint waits, and this one does not come back
                    // sooner than that.
                    let told = retry_after.map(|d| d.min(MAX_RETRY_AFTER));
                    if let Some(wait) = told {
                        self.cooldown
                            .lock()
                            .expect("cooldown")
                            .insert(ep.name.clone(), Instant::now() + wait);
                    }
                    if let Some(asked) = retry_after {
                        let left = deadline.saturating_duration_since(Instant::now());
                        if asked > MAX_RETRY_AFTER || told.is_some_and(|w| w >= left) {
                            // Waiting would outlive the request: say so now
                            // rather than time out later.
                            self.bump(&ep.name, |h| h.failures += 1);
                            let _ = tx
                                .send(ModelEvent::Error {
                                    code,
                                    message: format!(
                                        "{message} (the provider asked for {} s before the next request; the request has {} s left)",
                                        asked.as_secs(),
                                        left.as_secs()
                                    ),
                                    retryable: true,
                                })
                                .await;
                            return;
                        }
                    }
                    attempt += 1;
                    if attempt > ep.max_retries {
                        self.bump(&ep.name, |h| h.failures += 1);
                        let _ = tx
                            .send(ModelEvent::Error {
                                code,
                                message: format!("{message} (after {} retries)", ep.max_retries),
                                retryable: true,
                            })
                            .await;
                        return;
                    }
                    route.lock().expect("route").retries = attempt;
                    // Bounded backoff with jitter, charged to the same request deadline.
                    let base = 100u64 * (1u64 << attempt.min(6));
                    let jitter: u64 = rand::random::<u64>() % (base / 2 + 1);
                    let wait = Duration::from_millis(base + jitter).max(told.unwrap_or_default());
                    tokio::select! {
                        _ = cancel.cancelled() => {
                            self.bump(&ep.name, |h| h.cancellations += 1);
                            let _ = tx.send(ModelEvent::Completed { stop_reason: stop::CANCELLED.into() }).await;
                            return;
                        }
                        _ = tokio::time::sleep(wait) => {}
                    }
                }
            }
        }
    }

    /// One HTTP attempt. Retryable failures are only possible before the
    /// first token; after that an interruption is reported, never replayed.
    async fn attempt(
        &self,
        ep: &Endpoint,
        req: &ModelRequest,
        tx: &mpsc::Sender<ModelEvent>,
        cancel: &CancellationToken,
        route: &Arc<Mutex<RouteRecord>>,
        deadline: Instant,
    ) -> Attempt {
        let url = wire_url(ep.kind, &ep.base_url);
        let (mut body, mut rb) = match ep.kind {
            ProviderKind::OpenAi => (crate::openai::request_body(req), self.client.post(&url)),
            ProviderKind::Anthropic => (
                crate::anthropic::request_body(req),
                self.client
                    .post(&url)
                    .header("anthropic-version", "2023-06-01"),
            ),
        };
        // REQ-EV-0017: everything this attempt reports is error text, and
        // this endpoint's own key is the value most likely to come back in it.
        let redactor = self.broker.redactor();
        if !matches!(ep.credential, SecretHandle::None) {
            // The key is obtained for this request, for this task, for this
            // endpoint: a revoked, expired or rotated credential is decided
            // here, on the very next request.
            let key = match self.broker.acquire(
                &credential_id(&ep.name),
                &modbit_secrets::Use {
                    principal: principal_of(&req.request_id),
                    audience: format!("provider:{}", ep.name),
                    purpose: "model.request".into(),
                    nonce: None,
                },
            ) {
                Ok(k) => k,
                Err(refusal) => {
                    return Attempt::Failed {
                        code: refusal.code().into(),
                        message: format!(
                            "the credential for endpoint `{}` was refused by the credential broker",
                            ep.name
                        ),
                    };
                }
            };
            rb = match (ep.kind, ep.auth) {
                (ProviderKind::Anthropic, AuthScheme::Native) => {
                    rb.header("x-api-key", key.expose())
                }
                _ => rb.bearer_auth(key.expose()),
            };
        }
        // Gateway-specific fields fill in beside the canonical body; a
        // canonical key is never overridden by configuration.
        if let Some(obj) = body.as_object_mut() {
            for (k, v) in &ep.extra_body {
                obj.entry(k.clone()).or_insert_with(|| v.clone());
            }
        }
        rb = rb
            .header("accept", "text/event-stream")
            .header("x-modbit-request-id", &req.request_id)
            .timeout(deadline.saturating_duration_since(Instant::now()))
            .json(&body);
        let started = Instant::now();
        let resp = match rb.send().await {
            Ok(r) => r,
            Err(e) => {
                let msg = redactor.error_text(&e.to_string());
                // Nothing of a response was seen: a connection that could
                // not be made, or one from the pool the server had already
                // closed (a keep-alive idle cut lands exactly this way after
                // a long approval wait), is retried under the same bounded
                // budget — the request carries its own id for the provider
                // to deduplicate. A failure once the request is in flight
                // toward a response is not replayed (below).
                return if e.is_timeout() {
                    Attempt::Timeout
                } else if e.is_connect() {
                    Attempt::Retryable {
                        code: "CONNECT_FAILED".into(),
                        message: msg,
                        retry_after: None,
                    }
                } else if e.is_request() && !e.is_body() && !e.is_decode() {
                    Attempt::Retryable {
                        code: "TRANSPORT".into(),
                        message: msg,
                        retry_after: None,
                    }
                } else {
                    Attempt::Failed {
                        code: "TRANSPORT".into(),
                        message: msg,
                    }
                };
            }
        };
        let status = resp.status();
        // Both families answer with their own request id, under different
        // header names; a response without one leaves the field unknown.
        if let Some(id) = ["x-request-id", "request-id", "cf-ray"]
            .into_iter()
            .find_map(|h| resp.headers().get(h))
            .and_then(|v| v.to_str().ok())
            .filter(|v| !v.is_empty())
        {
            route.lock().expect("route").provider_request_id = Some(id.to_owned());
        }
        if !status.is_success() {
            // How long the provider asked for, from the response head, before
            // the body is consumed.
            let told = retry_after_of(resp.headers(), std::time::SystemTime::now());
            let text = resp.text().await.unwrap_or_default();
            // Redacted whole, then bounded: a cut through a key must not
            // leave its tail behind.
            let message = format!(
                "HTTP {}: {}",
                status.as_u16(),
                redactor
                    .error_text(&text)
                    .chars()
                    .take(300)
                    .collect::<String>()
            );
            return match status.as_u16() {
                429 => Attempt::Retryable {
                    code: "RATE_LIMITED".into(),
                    message,
                    retry_after: told,
                },
                500..=599 => Attempt::Retryable {
                    code: "PROVIDER_UNAVAILABLE".into(),
                    message,
                    retry_after: told,
                },
                401 | 403 => Attempt::Failed {
                    code: "AUTH_REJECTED".into(),
                    message,
                },
                _ => Attempt::Failed {
                    code: "PROVIDER_REJECTED".into(),
                    message,
                },
            };
        }
        let mut body = resp.bytes_stream();
        let mut parser = SseParser::default();
        let mut openai = crate::openai::Decoder::default();
        let mut anthropic = crate::anthropic::Decoder::default();
        let mut first_token = false;
        loop {
            let chunk = tokio::select! {
                _ = cancel.cancelled() => return Attempt::Cancelled,
                c = body.next() => c,
            };
            let bytes = match chunk {
                Some(Ok(b)) => b,
                Some(Err(e)) => {
                    let msg = redactor.error_text(&e.to_string());
                    return if e.is_timeout() {
                        Attempt::Timeout
                    } else if first_token {
                        Attempt::Interrupted(msg)
                    } else {
                        Attempt::Retryable {
                            code: "CONNECT_FAILED".into(),
                            message: msg,
                            retry_after: None,
                        }
                    };
                }
                None => {
                    let finished = match ep.kind {
                        ProviderKind::OpenAi => openai.finished(),
                        ProviderKind::Anthropic => anthropic.finished(),
                    };
                    return if finished {
                        Attempt::Done
                    } else if first_token {
                        Attempt::Interrupted("stream ended before completion".into())
                    } else {
                        Attempt::Retryable {
                            code: "EMPTY_STREAM".into(),
                            message: "stream ended before the first event".into(),
                            retry_after: None,
                        }
                    };
                }
            };
            for ev in parser.feed(&bytes) {
                let events = match ep.kind {
                    ProviderKind::OpenAi => openai.decode(&ev.data),
                    ProviderKind::Anthropic => anthropic.decode(ev.event.as_deref(), &ev.data),
                };
                for e in events {
                    if !first_token {
                        first_token = true;
                        let ms = started.elapsed().as_millis() as u64;
                        self.bump(&ep.name, |h| h.last_first_token_ms = Some(ms));
                    }
                    match &e {
                        ModelEvent::ProviderMetadata { metadata } => {
                            let mut r = route.lock().expect("route");
                            if let Some(m) = metadata["resolved_model"].as_str() {
                                r.resolved_model = Some(m.to_owned());
                            }
                            if let Some(t) = metadata["service_tier"].as_str() {
                                r.resolved_service_tier = Some(t.to_owned());
                            }
                        }
                        ModelEvent::ToolCallComplete { arguments_json, .. } => {
                            if serde_json::from_str::<serde_json::Value>(arguments_json).is_err() {
                                self.bump(&ep.name, |h| h.invalid_tool_calls += 1);
                            }
                        }
                        ModelEvent::Error {
                            code,
                            message,
                            retryable,
                        } => {
                            // What the provider said in its stream is error
                            // text too.
                            let message = redactor.error_text(message);
                            if *retryable && !first_token {
                                return Attempt::Retryable {
                                    code: code.clone(),
                                    message,
                                    retry_after: None,
                                };
                            }
                            let _ = tx
                                .send(ModelEvent::Error {
                                    code: code.clone(),
                                    message: message.clone(),
                                    retryable: *retryable,
                                })
                                .await;
                            return Attempt::Failed {
                                code: code.clone(),
                                message,
                            };
                        }
                        _ => {}
                    }
                    let done = matches!(e, ModelEvent::Completed { .. });
                    if tx.send(e).await.is_err() {
                        return Attempt::Cancelled;
                    }
                    if done {
                        return Attempt::Done;
                    }
                }
            }
        }
    }
}

/// How long a provider asked for before the next request, from a rate-limit
/// or overload response: `retry-after-ms` (milliseconds, as OpenAI sends it)
/// or `retry-after` as a number of seconds or an HTTP date (RFC 9110
/// §10.2.3). `None` when the response asks for nothing, or asks for a time
/// already past or a value that is not one of those forms.
#[must_use]
pub fn retry_after_of(
    headers: &reqwest::header::HeaderMap,
    now: std::time::SystemTime,
) -> Option<Duration> {
    let text = |name: &str| {
        headers
            .get(name)
            .and_then(|v| v.to_str().ok())
            .map(str::trim)
    };
    if let Some(ms) = text("retry-after-ms").and_then(|v| v.parse::<u64>().ok()) {
        return Some(Duration::from_millis(ms));
    }
    let value = text("retry-after")?;
    if let Ok(secs) = value.parse::<f64>() {
        return (secs.is_finite() && secs >= 0.0).then(|| Duration::from_secs_f64(secs.min(1e9)));
    }
    let at = http_date_to_unix(value)?;
    let now = now.duration_since(std::time::UNIX_EPOCH).ok()?.as_secs();
    at.checked_sub(now).map(Duration::from_secs)
}

/// Seconds since the epoch of an IMF-fixdate (`Sun, 06 Nov 1994 08:49:37
/// GMT`), the one HTTP-date form a sender must produce.
fn http_date_to_unix(s: &str) -> Option<u64> {
    const MONTHS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    let mut it = s.split_once(", ")?.1.split_whitespace();
    let day: i64 = it.next()?.parse().ok()?;
    let name = it.next()?;
    let month = i64::try_from(MONTHS.iter().position(|m| *m == name)?).ok()? + 1;
    let year: i64 = it.next()?.parse().ok()?;
    let mut hms = it.next()?.split(':').map(|p| p.parse::<i64>().ok());
    let (h, m, sec) = (hms.next()??, hms.next()??, hms.next()??);
    if it.next()? != "GMT" || !(1..=31).contains(&day) || h > 23 || m > 59 || sec > 60 {
        return None;
    }
    // Days from the civil date (proleptic Gregorian), after Howard Hinnant.
    let y = if month <= 2 { year - 1 } else { year };
    let era = y.div_euclid(400);
    let yoe = y.rem_euclid(400);
    let doy = (153 * (if month > 2 { month - 3 } else { month + 9 }) + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    u64::try_from(days * 86_400 + h * 3600 + m * 60 + sec).ok()
}

enum Attempt {
    Done,
    Cancelled,
    Timeout,
    Interrupted(String),
    Failed {
        code: String,
        message: String,
    },
    Retryable {
        code: String,
        message: String,
        /// How long the provider asked for before the next request, when it
        /// said (`Retry-After`).
        retry_after: Option<Duration>,
    },
}

/// Endpoints from the Core's environment (docs/15 "Credentials": only the
/// Core reads them). Per provider: `OPENAI_API_KEY` / `ANTHROPIC_API_KEY`;
/// optional `MODBIT_<P>_BASE_URL` (a compatible gateway), `MODBIT_<P>_MODELS`
/// (its catalog, see [`parse_models_spec`]), `MODBIT_<P>_AUTH` (`native` or
/// `bearer`) and `MODBIT_<P>_EXTRA_BODY` (a JSON object of request fields).
#[must_use]
pub fn endpoints_from_env() -> Vec<Endpoint> {
    endpoints_from(|name| std::env::var(name).ok())
}

/// [`endpoints_from_env`] over an explicit lookup, so a test can configure
/// endpoints without touching the process environment. Each provider is
/// registered on its own: a missing OpenAI key does not hide an Anthropic
/// one. A malformed catalog, auth or extra-body value leaves that provider
/// unregistered with the reason on stderr; nothing is guessed in its place.
#[must_use]
pub fn endpoints_from(lookup: impl Fn(&str) -> Option<String>) -> Vec<Endpoint> {
    let present = |name: &str| lookup(name).filter(|v| !v.trim().is_empty());
    let mut out = Vec::new();
    for (name, kind, key_var, default_base, defaults) in [
        (
            "openai",
            ProviderKind::OpenAi,
            "OPENAI_API_KEY",
            "https://api.openai.com",
            default_openai_models as fn() -> Vec<ModelCapability>,
        ),
        (
            "anthropic",
            ProviderKind::Anthropic,
            "ANTHROPIC_API_KEY",
            "https://api.anthropic.com",
            default_anthropic_models as fn() -> Vec<ModelCapability>,
        ),
    ] {
        let prefix = format!("MODBIT_{}", name.to_ascii_uppercase());
        let base_var = format!("{prefix}_BASE_URL");
        // An environment variable set to nothing is nothing: it registers no
        // endpoint, the same as an absent one.
        let credential = if present(key_var).is_some() {
            SecretHandle::Env(key_var.into())
        } else if present(&base_var).is_some() {
            SecretHandle::None
        } else {
            continue;
        };
        let configured = (|| -> Result<Endpoint, String> {
            let mut models = match present(&format!("{prefix}_MODELS")) {
                Some(spec) => {
                    parse_models_spec(&spec).map_err(|e| format!("{prefix}_MODELS: {e}"))?
                }
                None => defaults(),
            };
            // A catalog entry claims what the wire's adapter implements, not
            // more (a configured entry cannot add structured output to a
            // family whose adapter has none).
            if !kind.implements_structured_output() {
                for m in &mut models {
                    m.structured_output = false;
                }
            }
            let max_concurrency = match present(&format!("{prefix}_MAX_CONCURRENCY")) {
                None => 0,
                Some(v) => v
                    .trim()
                    .parse::<u32>()
                    .ok()
                    .filter(|n| *n > 0)
                    .ok_or_else(|| {
                        format!("{prefix}_MAX_CONCURRENCY: expected a positive number, got {v:?}")
                    })?,
            };
            let auth = match present(&format!("{prefix}_AUTH")).as_deref() {
                None | Some("native") => AuthScheme::Native,
                Some("bearer") => AuthScheme::Bearer,
                Some(other) => {
                    return Err(format!(
                        "{prefix}_AUTH: expected `native` or `bearer`, got {other:?}"
                    ));
                }
            };
            let extra_body = match present(&format!("{prefix}_EXTRA_BODY")) {
                Some(text) => match serde_json::from_str::<serde_json::Value>(&text) {
                    Ok(serde_json::Value::Object(map)) => map,
                    Ok(_) => return Err(format!("{prefix}_EXTRA_BODY: expected a JSON object")),
                    Err(e) => return Err(format!("{prefix}_EXTRA_BODY: {e}")),
                },
                None => serde_json::Map::new(),
            };
            Ok(Endpoint {
                name: name.into(),
                kind,
                base_url: present(&base_var)
                    .unwrap_or_else(|| default_base.into())
                    .trim_end_matches('/')
                    .to_owned(),
                credential,
                models,
                max_retries: 3,
                auth,
                extra_body,
                max_concurrency,
            })
        })();
        match configured {
            Ok(ep) => out.push(ep),
            Err(reason) => eprintln!("provider endpoint `{name}` not registered: {reason}"),
        }
    }
    out
}

/// Parse a configured catalog: comma-separated entries
/// `model=input/output[;key=value…]`, prices in USD per million tokens.
/// Prices are mandatory because an unknown provider cost is not free
/// (docs/73). Optional keys: `ctx` (context tokens, default 128000), `out`
/// (max output tokens, default 16384), `vision` and `reasoning` (`true` /
/// `false`, default `false`), `budget` (the output tokens one request asks
/// for, default [`DEFAULT_OUTPUT_BUDGET_TOKENS`], never above `out`),
/// `timeout` (whole-request milliseconds, default derived from the budget),
/// `effort` (`low`|`medium`|`high`, only with `reasoning=true`) and `tier`
/// (the service tier a request carries), `projection` (`direct` |
/// `exec_only` | `typed`, how the model's tool surface is projected, PX-114) and
/// `schema_bytes` (the most tool-schema bytes one request may carry).
/// Example:
/// `glm-5.3-flash=0.15/0.50;ctx=200000;out=131072;budget=32768`.
pub fn parse_models_spec(spec: &str) -> Result<Vec<ModelCapability>, String> {
    let mut out = Vec::new();
    for entry in spec.split(',').map(str::trim).filter(|e| !e.is_empty()) {
        let mut fields = entry.split(';').map(str::trim);
        let head = fields.next().unwrap_or_default();
        let (model, prices) = head
            .split_once('=')
            .ok_or_else(|| format!("{entry:?}: expected `model=input_price/output_price`"))?;
        let model = model.trim();
        if model.is_empty() {
            return Err(format!("{entry:?}: empty model id"));
        }
        let (inp, outp) = prices.split_once('/').ok_or_else(|| {
            format!("{entry:?}: expected `input_price/output_price` in USD per million tokens")
        })?;
        let price = |v: &str| {
            v.trim()
                .parse::<f64>()
                .ok()
                .filter(|p| p.is_finite() && *p >= 0.0)
                .ok_or_else(|| format!("{entry:?}: price {v:?} is not a non-negative number"))
        };
        let (input_price, output_price) = (price(inp)?, price(outp)?);
        let (mut ctx, mut max_out, mut vision, mut reasoning) =
            (128_000u32, 16_384u32, false, false);
        let (mut budget, mut timeout) = (0u32, 0u64);
        let (mut effort, mut tier): (Option<String>, Option<String>) = (None, None);
        let (mut projection, mut schema_bytes): (Option<String>, u32) = (None, 0);
        for field in fields.filter(|f| !f.is_empty()) {
            let (k, v) = field
                .split_once('=')
                .ok_or_else(|| format!("{entry:?}: expected `key=value`, got {field:?}"))?;
            let v = v.trim();
            match k.trim() {
                "ctx" => {
                    ctx = v
                        .parse()
                        .map_err(|_| format!("{entry:?}: ctx {v:?} is not a token count"))?
                }
                "out" => {
                    max_out = v
                        .parse()
                        .map_err(|_| format!("{entry:?}: out {v:?} is not a token count"))?
                }
                "vision" => {
                    vision = v
                        .parse()
                        .map_err(|_| format!("{entry:?}: vision {v:?} is not true/false"))?
                }
                "reasoning" => {
                    reasoning = v
                        .parse()
                        .map_err(|_| format!("{entry:?}: reasoning {v:?} is not true/false"))?
                }
                "budget" => {
                    budget = v
                        .parse()
                        .map_err(|_| format!("{entry:?}: budget {v:?} is not a token count"))?
                }
                "timeout" => {
                    timeout = v
                        .parse()
                        .map_err(|_| format!("{entry:?}: timeout {v:?} is not milliseconds"))?
                }
                "effort" => {
                    if !matches!(v, "low" | "medium" | "high") {
                        return Err(format!("{entry:?}: effort {v:?} is not low/medium/high"));
                    }
                    effort = Some(v.to_owned());
                }
                "tier" => {
                    if v.is_empty() || !v.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
                        return Err(format!("{entry:?}: tier {v:?} is not a tier name"));
                    }
                    tier = Some(v.to_owned());
                }
                "projection" => {
                    if !matches!(v, "direct" | "exec_only" | "typed") {
                        return Err(format!(
                            "{entry:?}: projection {v:?} is not direct/exec_only/typed"
                        ));
                    }
                    projection = Some(v.to_owned());
                }
                "schema_bytes" => {
                    schema_bytes = v
                        .parse()
                        .map_err(|_| format!("{entry:?}: schema_bytes {v:?} is not a byte count"))?
                }
                other => {
                    return Err(format!(
                        "{entry:?}: unknown key {other:?} (ctx, out, vision, reasoning, budget, timeout, effort, tier, projection, schema_bytes)"
                    ));
                }
            }
        }
        if effort.is_some() && !reasoning {
            return Err(format!(
                "{entry:?}: effort needs reasoning=true (the model exposes no reasoning to set)"
            ));
        }
        if ctx == 0 || max_out == 0 {
            return Err(format!("{entry:?}: ctx and out must be positive"));
        }
        let mut entry_cap = cap(
            model,
            ctx,
            max_out,
            reasoning,
            vision,
            input_price,
            output_price,
        );
        entry_cap.output_budget_tokens = budget;
        entry_cap.request_timeout_ms = timeout;
        entry_cap.default_reasoning_effort = effort;
        entry_cap.default_service_tier = tier;
        entry_cap.projection_mode = projection;
        entry_cap.max_projection_bytes = schema_bytes;
        out.push(entry_cap);
    }
    if out.is_empty() {
        return Err("no model entries".into());
    }
    Ok(out)
}

fn cap(
    model: &str,
    context: u32,
    output: u32,
    reasoning: bool,
    vision: bool,
    inp: f64,
    outp: f64,
) -> ModelCapability {
    ModelCapability {
        model: model.into(),
        context_tokens: context,
        max_output_tokens: output,
        tools: true,
        parallel_tools: true,
        vision,
        input_modalities: if vision {
            vec!["text".into(), "image".into()]
        } else {
            vec!["text".into()]
        },
        reasoning,
        structured_output: true,
        agent_loop: true,
        input_price_per_mtok: inp,
        output_price_per_mtok: outp,
        output_budget_tokens: 0,
        request_timeout_ms: 0,
        default_reasoning_effort: None,
        default_service_tier: None,
        projection_mode: None,
        max_projection_bytes: 0,
    }
}

/// Default OpenAI catalog entries (nominal economics; conformance probes refine).
#[must_use]
pub fn default_openai_models() -> Vec<ModelCapability> {
    vec![
        cap("gpt-5", 400_000, 128_000, true, true, 1.25, 10.0),
        cap("gpt-5-mini", 400_000, 128_000, true, true, 0.25, 2.0),
        cap("gpt-4.1", 1_000_000, 32_768, false, true, 2.0, 8.0),
        cap("gpt-4.1-mini", 1_000_000, 32_768, false, true, 0.4, 1.6),
        // Text only: no image input, so media reaches it through the vision
        // bridge or as an explicit unsupported-modality note (REQ-EV-0184).
        cap("o3-mini", 200_000, 100_000, true, false, 1.1, 4.4),
    ]
}

/// Default Anthropic catalog entries.
#[must_use]
pub fn default_anthropic_models() -> Vec<ModelCapability> {
    let mut models = vec![
        cap("claude-opus-5", 200_000, 64_000, true, true, 15.0, 75.0),
        cap("claude-sonnet-5", 200_000, 64_000, true, true, 3.0, 15.0),
        cap(
            "claude-haiku-4-5-20251001",
            200_000,
            64_000,
            true,
            true,
            1.0,
            5.0,
        ),
    ];
    // The Messages API has no JSON mode and the adapter implements none, so
    // the catalog does not claim one (`route` refuses a request for it).
    for m in &mut models {
        m.structured_output = false;
    }
    models
}
