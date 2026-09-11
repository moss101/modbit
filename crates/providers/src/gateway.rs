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
}

/// One registered endpoint.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Endpoint {
    /// Name used by `ModelPolicy.endpoint`.
    pub name: String,
    /// Wire family.
    pub kind: ProviderKind,
    /// Base URL (no trailing slash), e.g. `https://api.openai.com`.
    pub base_url: String,
    /// Credential.
    pub credential: SecretHandle,
    /// Models served, with capabilities.
    pub models: Vec<ModelCapability>,
    /// Bounded retries before the first token (rate limit / transient errors).
    pub max_retries: u32,
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

/// The gateway.
#[derive(Clone)]
pub struct ProviderGateway {
    endpoints: Arc<BTreeMap<String, Endpoint>>,
    health: Arc<Mutex<BTreeMap<String, EndpointHealth>>>,
    client: reqwest::Client,
    policy: Arc<OrgModelPolicy>,
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
        let map = endpoints
            .into_iter()
            .map(|e| (e.name.clone(), e))
            .collect::<BTreeMap<_, _>>();
        Self {
            endpoints: Arc::new(map),
            health: Arc::new(Mutex::new(BTreeMap::new())),
            policy: Arc::new(OrgModelPolicy::default()),
            client: reqwest::Client::builder()
                .connect_timeout(Duration::from_secs(10))
                .build()
                .expect("reqwest client"),
        }
    }

    /// Registered endpoints (credentials redacted by `Debug`).
    #[must_use]
    pub fn endpoints(&self) -> Vec<&Endpoint> {
        self.endpoints.values().collect()
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

    /// What one endpoint's catalog says about a model, when it lists it.
    #[must_use]
    pub fn capability(&self, endpoint: &str, model: &str) -> Option<ModelCapability> {
        self.endpoints
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
        if (needs.structured_output || req.response_format.as_deref() == Some("json_object"))
            && !cap.structured_output
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
        if !matches!(ep.credential, SecretHandle::None) && ep.credential.resolve().is_none() {
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
                r = self.attempt(&ep, &req, &tx, &cancel, &route, deadline) => r,
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
                Attempt::Retryable { code, message } => {
                    if code == "RATE_LIMITED" {
                        self.bump(&ep.name, |h| h.rate_limited += 1);
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
                    let wait = Duration::from_millis(base + jitter);
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
        let (url, body, mut rb) = match ep.kind {
            ProviderKind::OpenAi => {
                let url = format!("{}/v1/chat/completions", ep.base_url);
                let body = crate::openai::request_body(req);
                let mut rb = self.client.post(&url);
                if let Some(key) = ep.credential.resolve() {
                    rb = rb.bearer_auth(key);
                }
                (url, body, rb)
            }
            ProviderKind::Anthropic => {
                let url = format!("{}/v1/messages", ep.base_url);
                let body = crate::anthropic::request_body(req);
                let mut rb = self
                    .client
                    .post(&url)
                    .header("anthropic-version", "2023-06-01");
                if let Some(key) = ep.credential.resolve() {
                    rb = rb.header("x-api-key", key);
                }
                (url, body, rb)
            }
        };
        let _ = url;
        rb = rb
            .header("accept", "text/event-stream")
            .header("x-modbit-request-id", &req.request_id)
            .timeout(deadline.saturating_duration_since(Instant::now()))
            .json(&body);
        let started = Instant::now();
        let resp = match rb.send().await {
            Ok(r) => r,
            Err(e) => {
                let msg = redact(&e.to_string());
                return if e.is_timeout() {
                    Attempt::Timeout
                } else if e.is_connect() {
                    Attempt::Retryable {
                        code: "CONNECT_FAILED".into(),
                        message: msg,
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
            let text = resp.text().await.unwrap_or_default();
            let message = redact(&format!(
                "HTTP {}: {}",
                status.as_u16(),
                text.chars().take(300).collect::<String>()
            ));
            return match status.as_u16() {
                429 => Attempt::Retryable {
                    code: "RATE_LIMITED".into(),
                    message,
                },
                500..=599 => Attempt::Retryable {
                    code: "PROVIDER_UNAVAILABLE".into(),
                    message,
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
                    let msg = redact(&e.to_string());
                    return if e.is_timeout() {
                        Attempt::Timeout
                    } else if first_token {
                        Attempt::Interrupted(msg)
                    } else {
                        Attempt::Retryable {
                            code: "CONNECT_FAILED".into(),
                            message: msg,
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
                            if *retryable && !first_token {
                                return Attempt::Retryable {
                                    code: code.clone(),
                                    message: message.clone(),
                                };
                            }
                            let _ = tx.send(e.clone()).await;
                            return Attempt::Failed {
                                code: code.clone(),
                                message: message.clone(),
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

/// Strip anything that looks like a bearer token or API key from messages.
fn redact(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for word in s.split(' ') {
        if word.len() > 20
            && (word.starts_with("sk-") || word.starts_with("Bearer") || word.contains("key="))
        {
            out.push_str("<redacted>");
        } else {
            out.push_str(word);
        }
        out.push(' ');
    }
    out.trim_end().to_owned()
}

enum Attempt {
    Done,
    Cancelled,
    Timeout,
    Interrupted(String),
    Failed { code: String, message: String },
    Retryable { code: String, message: String },
}

/// Endpoints from the Core's environment (docs/15 "Credentials": only the
/// Core reads them). `OPENAI_API_KEY` / `ANTHROPIC_API_KEY`, optional
/// `MODBIT_OPENAI_BASE_URL` / `MODBIT_ANTHROPIC_BASE_URL`.
#[must_use]
pub fn endpoints_from_env() -> Vec<Endpoint> {
    let mut out = Vec::new();
    let openai_base = std::env::var("MODBIT_OPENAI_BASE_URL")
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "https://api.openai.com".into());
    let anthropic_base = std::env::var("MODBIT_ANTHROPIC_BASE_URL")
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "https://api.anthropic.com".into());
    let openai_cred = if std::env::var("OPENAI_API_KEY").is_ok_and(|v| !v.is_empty()) {
        SecretHandle::Env("OPENAI_API_KEY".into())
    } else if std::env::var("MODBIT_OPENAI_BASE_URL").is_ok() {
        SecretHandle::None
    } else {
        return out;
    };
    out.push(Endpoint {
        name: "openai".into(),
        kind: ProviderKind::OpenAi,
        base_url: openai_base.trim_end_matches('/').to_owned(),
        credential: openai_cred,
        models: default_openai_models(),
        max_retries: 3,
    });
    let anthropic_cred = if std::env::var("ANTHROPIC_API_KEY").is_ok_and(|v| !v.is_empty()) {
        SecretHandle::Env("ANTHROPIC_API_KEY".into())
    } else if std::env::var("MODBIT_ANTHROPIC_BASE_URL").is_ok() {
        SecretHandle::None
    } else {
        return out;
    };
    out.push(Endpoint {
        name: "anthropic".into(),
        kind: ProviderKind::Anthropic,
        base_url: anthropic_base.trim_end_matches('/').to_owned(),
        credential: anthropic_cred,
        models: default_anthropic_models(),
        max_retries: 3,
    });
    out
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
    ]
}

/// Default Anthropic catalog entries.
#[must_use]
pub fn default_anthropic_models() -> Vec<ModelCapability> {
    vec![
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
    ]
}
