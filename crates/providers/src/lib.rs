//! `modbit-providers` — model adapters and the Provider Gateway (docs/15).
//! THE doc 81 model/provider gateway.
//!
//! One normalized streaming boundary: [`ModelRequest`] in, [`ModelEvent`]s
//! out, for OpenAI-compatible Chat Completions and the Anthropic Messages
//! API. The gateway owns the endpoint registry and capability catalog
//! (REQ-EV-0028/0189), records requested vs resolved route metadata
//! (REQ-EV-0112), retries transient failures only before the first token,
//! enforces the request timeout, honours cancellation and keeps rolling
//! health. Raw credentials are resolved per request through a
//! [`SecretHandle`] and never reach events, errors or logs.
//!
//! Registry/profiler/plan-compiler pieces of docs/15 and docs/27 arrive with
//! their own scheduled tasks; nothing here routes by learned statistics.

#![forbid(unsafe_code)]

pub mod anthropic;
pub mod contract;
pub mod gateway;
pub mod openai;
pub mod profiler;
pub mod registry;
pub mod sse;

pub use contract::{
    ContentPart, MediaPayload, Message, ModelEvent, ModelPolicy, ModelRequest, ProviderKind, Role,
    SecretHandle, ToolProjection, Usage, stop,
};
pub use gateway::{
    Endpoint, EndpointHealth, ModelCapability, ModelStream, OrgModelPolicy, ProviderGateway,
    Requirements, RouteError, RouteRecord, default_anthropic_models, default_openai_models,
    endpoints_from_env,
};
