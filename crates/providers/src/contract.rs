//! Provider contract (docs/15 "Provider contract"): one normalized streaming
//! inference boundary. Provider-specific semantics never leak past this file.

use std::fmt;

use serde::{Deserialize, Serialize};

/// Provider family an endpoint speaks.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderKind {
    /// OpenAI-compatible Chat Completions (`/v1/chat/completions`, SSE).
    OpenAi,
    /// Anthropic Messages API (`/v1/messages`, SSE).
    Anthropic,
}

impl ProviderKind {
    /// Whether this family's adapter implements structured (JSON) output:
    /// the OpenAI adapter sends `response_format: json_object`; the Anthropic
    /// Messages API has no such mode and its adapter implements none (forcing
    /// a tool would take the tool channel the agent loop needs). A capability
    /// the adapter does not implement is never claimed or silently dropped.
    #[must_use]
    pub fn implements_structured_output(self) -> bool {
        matches!(self, Self::OpenAi)
    }
}

/// A credential the gateway can present. The raw value is resolved only at
/// request time and is never printed (docs/15 "Credentials").
#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum SecretHandle {
    /// Read from the named environment variable of the Core process.
    Env(String),
    /// Held in memory by the Core (OS keychain integration hands it in).
    Inline(String),
    /// No credential (local, unauthenticated endpoints).
    None,
}

impl SecretHandle {
    /// Resolve the raw value.
    pub fn resolve(&self) -> Option<String> {
        match self {
            Self::Env(name) => std::env::var(name).ok().filter(|v| !v.is_empty()),
            Self::Inline(v) => Some(v.clone()),
            Self::None => None,
        }
    }
}

impl fmt::Debug for SecretHandle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Env(name) => write!(f, "SecretHandle::Env({name})"),
            Self::Inline(_) => write!(f, "SecretHandle::Inline(<redacted>)"),
            Self::None => write!(f, "SecretHandle::None"),
        }
    }
}

/// Message role.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    /// System / policy.
    System,
    /// User.
    User,
    /// Assistant.
    Assistant,
    /// Tool result (canonical; adapters may re-place it).
    Tool,
}

/// Media bytes on their way to a provider. The canonical model names media by
/// digest; this carries the one copy the dispatch needs and never prints or
/// serializes it (docs/25: bytes live in the object store, not in the log).
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MediaPayload(pub String);

impl fmt::Debug for MediaPayload {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "MediaPayload(<{} base64 chars redacted>)", self.0.len())
    }
}

/// One content part.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ContentPart {
    /// Text.
    Text {
        /// Text.
        text: String,
    },
    /// A tool call the assistant made earlier (replayed into context).
    ToolCall {
        /// Provider-visible call id.
        call_id: String,
        /// Tool name.
        name: String,
        /// Arguments JSON text.
        arguments_json: String,
    },
    /// A tool result for an earlier call.
    ToolResult {
        /// Call id it answers.
        call_id: String,
        /// Result text (bounded model view).
        content: String,
        /// Whether the tool failed.
        is_error: bool,
    },
    /// Media the model may look at (REQ-EV-0188). Provider-neutral: it names
    /// the egress copy by digest and says what it is; each adapter decides
    /// where its API accepts it, and a provider that refuses media inside a
    /// tool result gets the same media as a split follow-up message instead.
    Media {
        /// Object hash of the egress copy (metadata already stripped).
        source_ref: String,
        /// MIME type of those bytes, e.g. `image/png`.
        mime: String,
        /// What it is, in words. Survives when a provider cannot take bytes.
        alt: String,
        /// The tool call this media answers, when it came from a tool result.
        call_id: Option<String>,
        /// The bytes, base64, for this dispatch only.
        data_base64: MediaPayload,
    },
}

impl ContentPart {
    /// The input modality this part needs from the model (`image`, `pdf`, …).
    #[must_use]
    pub fn modality(&self) -> Option<&'static str> {
        let Self::Media { mime, .. } = self else {
            return None;
        };
        Some(match mime.split('/').next().unwrap_or_default() {
            "image" => "image",
            "audio" => "audio",
            "video" => "video",
            _ if mime == "application/pdf" => "pdf",
            _ => "file",
        })
    }
}

/// One message.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Message {
    /// Role.
    pub role: Role,
    /// Parts.
    pub parts: Vec<ContentPart>,
}

impl Message {
    /// A text message.
    #[must_use]
    pub fn text(role: Role, text: impl Into<String>) -> Self {
        Self {
            role,
            parts: vec![ContentPart::Text { text: text.into() }],
        }
    }
}

/// The messages without any tool result (or media answering one) whose tool
/// call no earlier assistant message announced (FIX-07, audit C).
///
/// A strict endpoint answers such an orphan with a 400 — a `tool` message
/// that is not a response to a preceding `tool_calls`, a `tool_result` block
/// with no `tool_use` — and it can reach the adapter when history was cut
/// through a turn (a compaction replayed from a log written before the cut was
/// aligned). The orphan is dropped, not rewritten into a user message: a
/// tool's text must never be promoted to a role it did not have. A message
/// left with no parts is dropped too. The borrowed slice is returned when
/// nothing needs repair, which is the normal case.
#[must_use]
pub fn without_orphan_tool_results(messages: &[Message]) -> std::borrow::Cow<'_, [Message]> {
    without_orphan_tool_results_mapped(messages).0
}

/// [`without_orphan_tool_results`] plus, when messages were dropped, the index
/// in `messages` of each message kept (`None` means the identity mapping).
/// Cache breakpoints index the original messages, so an adapter that places
/// them needs the mapping.
#[must_use]
pub fn without_orphan_tool_results_mapped(
    messages: &[Message],
) -> (std::borrow::Cow<'_, [Message]>, Option<Vec<usize>>) {
    use std::collections::HashSet;
    let announces = |m: &Message, ids: &mut HashSet<String>| {
        if m.role == Role::Assistant {
            for p in &m.parts {
                if let ContentPart::ToolCall { call_id, .. } = p {
                    ids.insert(call_id.clone());
                }
            }
        }
    };
    let orphan = |p: &ContentPart, ids: &HashSet<String>| match p {
        ContentPart::ToolResult { call_id, .. }
        | ContentPart::Media {
            call_id: Some(call_id),
            ..
        } => !ids.contains(call_id),
        _ => false,
    };
    let mut ids = HashSet::new();
    let mut dirty = false;
    for m in messages {
        if m.parts.iter().any(|p| orphan(p, &ids)) {
            dirty = true;
            break;
        }
        announces(m, &mut ids);
    }
    if !dirty {
        return (std::borrow::Cow::Borrowed(messages), None);
    }
    let mut ids = HashSet::new();
    let mut out = Vec::with_capacity(messages.len());
    let mut kept = Vec::with_capacity(messages.len());
    for (at, m) in messages.iter().enumerate() {
        let parts: Vec<ContentPart> = m
            .parts
            .iter()
            .filter(|p| !orphan(p, &ids))
            .cloned()
            .collect();
        announces(m, &mut ids);
        if !parts.is_empty() || m.parts.is_empty() {
            out.push(Message {
                role: m.role,
                parts,
            });
            kept.push(at);
        }
    }
    (std::borrow::Cow::Owned(out), Some(kept))
}

/// A tool as projected to the model (docs/16 "Dynamic task-scoped projection").
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolProjection {
    /// `namespace.name`.
    pub name: String,
    /// Description.
    pub description: String,
    /// JSON Schema.
    pub input_schema: serde_json::Value,
}

/// Provider-neutral model policy for one request (REQ-EV-0112).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelPolicy {
    /// Endpoint name in the gateway registry.
    pub endpoint: String,
    /// Requested model id.
    pub model: String,
    /// Reasoning effort label (`low|medium|high`), when the model supports it.
    pub reasoning_effort: Option<String>,
    /// Service tier label, when the provider exposes one.
    pub service_tier: Option<String>,
}

/// docs/15 `ModelRequest`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelRequest {
    /// Request id (idempotency + trace).
    pub request_id: String,
    /// Policy.
    pub model_policy: ModelPolicy,
    /// Messages (already compiled segments).
    pub messages: Vec<Message>,
    /// Tools projected for this turn.
    pub tool_projection: Vec<ToolProjection>,
    /// `json_object` / `text`.
    pub response_format: Option<String>,
    /// Stable-prefix cache key metadata (hash of stable segments).
    pub cache_key: Option<String>,
    /// Indices into `messages` after which the prompt prefix is stable
    /// across turns (docs/15 "Prompt cache economics"): everything up to and
    /// including each listed message is a cacheable prefix. A transport with
    /// explicit cache markers (Anthropic `cache_control`) places one at each;
    /// a transport that caches automatically (OpenAI prefix caching) ignores
    /// them. Empty = the caller declares no stable prefix and no marker is
    /// sent.
    #[serde(default)]
    pub cache_breakpoints: Vec<usize>,
    /// Output cap.
    pub max_output_tokens: u32,
    /// Timeout for the whole stream.
    pub timeout_ms: u64,
    /// Tenant/user policy tags (opaque).
    pub policy_tags: Vec<String>,
}

/// Token usage, in one meaning on every wire (docs/38
/// "CompleteAccountingAndAttribution": the total input and its cached
/// subset, never double charged). Adapters translate their provider's
/// report into this shape; a provider that reports cache reads beside the
/// input rather than inside it is summed here, not downstream.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Usage {
    /// Every input token the request was billed for, cached, cache-written
    /// and plain alike.
    pub input_tokens: u64,
    /// Output tokens.
    pub output_tokens: u64,
    /// The subset of `input_tokens` served from the provider cache.
    pub cached_input_tokens: u64,
    /// The subset of `input_tokens` written to the provider cache (a price
    /// of its own on providers that charge for cache writes).
    #[serde(default)]
    pub cache_write_input_tokens: u64,
}

/// docs/15 `ModelEvent`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum ModelEvent {
    /// Text delta.
    MessageDelta {
        /// Text.
        text: String,
    },
    /// Reasoning delta (when the provider exposes it).
    ReasoningDelta {
        /// Text.
        text: String,
    },
    /// A tool call began.
    ToolCallStart {
        /// Call id.
        call_id: String,
        /// Tool name.
        name: String,
    },
    /// Arguments delta.
    ToolCallDelta {
        /// Call id.
        call_id: String,
        /// JSON text fragment.
        arguments_delta: String,
    },
    /// A tool call is complete.
    ToolCallComplete {
        /// Call id.
        call_id: String,
        /// Tool name.
        name: String,
        /// Complete arguments JSON text.
        arguments_json: String,
    },
    /// Usage (may arrive more than once; the last one wins).
    Usage {
        /// Usage.
        usage: Usage,
    },
    /// Provider metadata (resolved model, service tier, ids), redacted.
    ProviderMetadata {
        /// JSON.
        metadata: serde_json::Value,
    },
    /// Stream finished.
    Completed {
        /// `end_turn | tool_use | max_tokens | stop` (normalized).
        stop_reason: String,
    },
    /// Stream failed.
    Error {
        /// Stable code.
        code: String,
        /// Message (never contains credentials).
        message: String,
        /// Whether a bounded retry may help.
        retryable: bool,
    },
}

/// Normalized stop reasons.
pub mod stop {
    /// Natural end.
    pub const END_TURN: &str = "end_turn";
    /// Model wants tools run.
    pub const TOOL_USE: &str = "tool_use";
    /// Output cap.
    pub const MAX_TOKENS: &str = "max_tokens";
    /// Cancelled by the caller.
    pub const CANCELLED: &str = "cancelled";
}
