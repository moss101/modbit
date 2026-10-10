//! A small chat client for the live evaluations that talk to the model
//! themselves (PX-137's tool loop, PX-138's recall questions). It is the
//! product's own provider gateway run in-process (`modbit_providers`): the
//! endpoint, its priced catalog, its auth scheme and its extra request body
//! are read from `OPENAI_API_KEY`, `MODBIT_OPENAI_BASE_URL`,
//! `MODBIT_OPENAI_MODELS`, `MODBIT_OPENAI_AUTH` and `MODBIT_OPENAI_EXTRA_BODY`
//! by the same parser the Core uses, and the request goes through the same
//! OpenAI adapter, so what is measured is the wire the product speaks. The
//! one addition is `temperature: 0`, sent as an endpoint extra-body field
//! unless the operator's own extra body already sets one.
//!
//! The reply carries the usage the gateway returned, which is the only thing
//! the [`crate::spend::SpendMeter`] is charged from. The key is held by the
//! gateway's credential broker and never formatted: not in `Debug`, not in an
//! error.

use std::time::Instant;

use modbit_providers::{
    ContentPart, Message, ModelEvent, ModelPolicy, ModelRequest, ProviderGateway, Requirements,
    Role, ToolProjection, endpoints_from,
};
use modbit_secrets::SecretHandle;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio_util::sync::CancellationToken;

use crate::spend::{Price, SpendMeter};

/// A tool call the model asked for.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ToolCall {
    /// The provider's id for the call.
    pub id: String,
    /// The tool.
    pub name: String,
    /// The arguments, as the model wrote them.
    pub arguments: String,
}

/// One completion.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Reply {
    /// The assistant's text.
    pub text: String,
    /// Tool calls, in order.
    pub tool_calls: Vec<ToolCall>,
    /// Input tokens the gateway reported (0 with `usage_known == false`).
    pub input_tokens: u64,
    /// Output tokens the gateway reported.
    pub output_tokens: u64,
    /// Of the input, the cached part.
    pub cached_input_tokens: u64,
    /// Whether the reply carried a usage block at all.
    pub usage_known: bool,
    /// Why the model stopped.
    pub finish_reason: String,
    /// Wall time of the call, milliseconds.
    pub latency_ms: u64,
    /// The model id the gateway says answered.
    pub model_answered: String,
    /// The gateway's request id, when it sent one.
    pub request_id: String,
}

/// A configured endpoint.
#[derive(Clone)]
pub struct Chat {
    gateway: ProviderGateway,
    /// The model every request names.
    pub model: String,
    /// Its list price.
    pub price: Price,
    /// The base URL (no credential in it), for the retained record.
    pub base_url: String,
}

impl std::fmt::Debug for Chat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Chat")
            .field("base_url", &self.base_url)
            .field("model", &self.model)
            .finish_non_exhaustive()
    }
}

/// Why a call produced no reply.
#[derive(Clone, Debug, PartialEq)]
pub enum ChatError {
    /// The spend cap would be exceeded.
    Cap(crate::spend::CapReached),
    /// The call failed (after the gateway's own retries).
    Failed(String),
}

impl std::fmt::Display for ChatError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Cap(c) => write!(f, "{c}"),
            Self::Failed(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for ChatError {}

fn text_of(m: &Value) -> String {
    match &m["content"] {
        Value::String(s) => s.clone(),
        Value::Array(parts) => parts
            .iter()
            .filter_map(|p| p["text"].as_str())
            .collect::<Vec<_>>()
            .join(""),
        _ => String::new(),
    }
}

/// OpenAI-style chat messages as the gateway's normalized messages.
fn to_messages(messages: &[Value]) -> Vec<Message> {
    messages
        .iter()
        .map(|m| match m["role"].as_str().unwrap_or("user") {
            "system" => Message::text(Role::System, text_of(m)),
            "assistant" => {
                let mut parts = Vec::new();
                let text = text_of(m);
                if !text.is_empty() {
                    parts.push(ContentPart::Text { text });
                }
                for c in m["tool_calls"].as_array().into_iter().flatten() {
                    parts.push(ContentPart::ToolCall {
                        call_id: c["id"].as_str().unwrap_or_default().to_owned(),
                        name: c["function"]["name"]
                            .as_str()
                            .unwrap_or_default()
                            .to_owned(),
                        arguments_json: c["function"]["arguments"]
                            .as_str()
                            .unwrap_or("{}")
                            .to_owned(),
                    });
                }
                Message {
                    role: Role::Assistant,
                    parts,
                }
            }
            "tool" => Message {
                role: Role::Tool,
                parts: vec![ContentPart::ToolResult {
                    call_id: m["tool_call_id"].as_str().unwrap_or_default().to_owned(),
                    content: text_of(m),
                    is_error: false,
                }],
            },
            _ => Message::text(Role::User, text_of(m)),
        })
        .collect()
}

fn to_tools(tools: &[Value]) -> Vec<ToolProjection> {
    tools
        .iter()
        .map(|t| {
            let f = if t["function"].is_object() {
                &t["function"]
            } else {
                t
            };
            ToolProjection {
                name: f["name"].as_str().unwrap_or_default().to_owned(),
                description: f["description"].as_str().unwrap_or_default().to_owned(),
                input_schema: if f["parameters"].is_object() {
                    f["parameters"].clone()
                } else {
                    serde_json::json!({"type": "object", "properties": {}})
                },
            }
        })
        .collect()
}

impl Chat {
    /// Configure from a lookup (the process environment in production).
    ///
    /// # Errors
    /// No key, no catalog entry for the model, or a malformed variable.
    pub fn from_lookup(
        lookup: &dyn Fn(&str) -> Option<String>,
        model: &str,
    ) -> Result<Self, String> {
        let present = |n: &str| {
            lookup(n)
                .map(|v| v.trim().to_owned())
                .filter(|v| !v.is_empty())
        };
        let key = present("OPENAI_API_KEY")
            .ok_or("OPENAI_API_KEY is not set: a live run needs a real credential")?;
        present("MODBIT_OPENAI_MODELS").ok_or(
            "MODBIT_OPENAI_MODELS is not set: the harness prices a call from the catalog and an unknown price is not free",
        )?;
        // The product's own parser, over this lookup; the key is handed to the
        // gateway's broker directly rather than read from the process
        // environment.
        let mut endpoints = endpoints_from(|n| lookup(n));
        let mut ep = endpoints
            .drain(..)
            .find(|e| e.name == "openai")
            .ok_or("the openai endpoint is not configured (see the message above)")?;
        let cap = ep
            .models
            .iter()
            .find(|m| m.model == model)
            .cloned()
            .ok_or_else(|| {
                format!(
                    "the catalog does not list {model}: an unknown price is not free, so no cost can be charged"
                )
            })?;
        ep.credential = SecretHandle::Inline(key);
        ep.extra_body
            .entry("temperature".to_owned())
            .or_insert_with(|| serde_json::json!(0));
        // The gateway's own retries cover a rate limit or a transient error.
        ep.max_retries = ep.max_retries.max(3);
        let base_url = ep.base_url.clone();
        Ok(Self {
            gateway: ProviderGateway::new(vec![ep]),
            model: model.to_owned(),
            price: Price {
                input_per_mtok_usd: cap.input_price_per_mtok,
                output_per_mtok_usd: cap.output_price_per_mtok,
            },
            base_url,
        })
    }

    /// Configure from the process environment.
    ///
    /// # Errors
    /// See [`Chat::from_lookup`].
    pub fn from_env(model: &str) -> Result<Self, String> {
        Self::from_lookup(&|k| std::env::var(k).ok(), model)
    }

    /// One completion. `reserve_usd` is what the call may cost at most (the
    /// meter refuses the call when the cap could not cover it); the reply's
    /// own usage is charged to the meter.
    ///
    /// # Errors
    /// The cap, or a failure after the gateway's retries.
    pub async fn complete(
        &self,
        meter: &SpendMeter,
        reserve_usd: f64,
        messages: &[Value],
        tools: &[Value],
        max_tokens: u32,
    ) -> Result<Reply, ChatError> {
        meter.admit(reserve_usd).map_err(ChatError::Cap)?;
        let request = ModelRequest {
            request_id: format!("live-eval:{}", rand::random::<u64>()),
            model_policy: ModelPolicy {
                endpoint: "openai".into(),
                model: self.model.clone(),
                reasoning_effort: None,
                service_tier: None,
            },
            messages: to_messages(messages),
            tool_projection: to_tools(tools),
            response_format: None,
            cache_key: None,
            cache_breakpoints: vec![],
            max_output_tokens: max_tokens,
            timeout_ms: 240_000,
            policy_tags: vec![],
        };
        let needs = Requirements {
            tools: !tools.is_empty(),
            ..Requirements::default()
        };
        let started = Instant::now();
        let mut stream = self
            .gateway
            .stream(request, &needs, CancellationToken::new())
            .map_err(|e| ChatError::Failed(format!("{e:?}")))?;
        let mut text = String::new();
        let mut calls: Vec<ToolCall> = Vec::new();
        let (mut usage, mut finish) = (None, String::new());
        let mut failure: Option<String> = None;
        while let Some(ev) = stream.events.recv().await {
            match ev {
                ModelEvent::MessageDelta { text: t } => text.push_str(&t),
                ModelEvent::ToolCallComplete {
                    call_id,
                    name,
                    arguments_json,
                } => calls.push(ToolCall {
                    id: call_id,
                    name,
                    arguments: arguments_json,
                }),
                ModelEvent::Usage { usage: u } => usage = Some(u),
                ModelEvent::Completed { stop_reason } => finish = stop_reason,
                ModelEvent::Error {
                    code,
                    message,
                    retryable: _,
                } => failure = Some(format!("{code}: {message}")),
                _ => {}
            }
        }
        let route = stream.route.lock().map(|r| r.clone()).ok();
        // Whatever usage the gateway did report is charged, even for a call
        // that then failed: the provider billed it.
        if let Some(u) = &usage {
            meter.charge(self.price, u.input_tokens, u.output_tokens);
        }
        if let Some(why) = failure {
            // A call the gateway failed carries no usage unless it reported
            // some; nothing else is invented for it.
            return Err(ChatError::Failed(why));
        }
        if usage.is_none() {
            meter.charge_unknown();
        }
        let u = usage.clone().unwrap_or_default();
        Ok(Reply {
            text,
            tool_calls: calls,
            input_tokens: u.input_tokens,
            output_tokens: u.output_tokens,
            cached_input_tokens: u.cached_input_tokens,
            usage_known: usage.is_some(),
            finish_reason: finish,
            latency_ms: u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
            model_answered: route
                .as_ref()
                .and_then(|r| r.resolved_model.clone())
                .unwrap_or_default(),
            request_id: route
                .and_then(|r| r.provider_request_id)
                .unwrap_or_default(),
        })
    }
}

/// A scripted OpenAI-compatible chat server for the offline tests of the
/// live harnesses: one reply per request, computed from the request body,
/// streamed the way the wire does (server-sent events, the usage in the last
/// chunk). It is a stand-in for the model and says so: a harness that runs
/// against it has not been run live.
pub mod scripted {
    use std::sync::{Arc, Mutex};

    use serde_json::{Value, json};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    /// What the server answers with.
    #[derive(Clone, Debug, Default)]
    pub struct Scripted {
        /// Assistant text.
        pub text: String,
        /// Tool calls `(name, arguments JSON)`.
        pub calls: Vec<(String, Value)>,
        /// Usage; `None` leaves the usage block out.
        pub usage: Option<(u64, u64)>,
        /// An HTTP status other than 200, with its body.
        pub status: Option<(u16, String)>,
    }

    /// The request bodies the server has seen.
    pub type Seen = Arc<Mutex<Vec<Value>>>;

    /// Serve `reply` on a loopback port; returns the base URL and the bodies seen.
    ///
    /// # Panics
    /// The loopback socket cannot be bound.
    pub async fn serve(reply: Arc<dyn Fn(&Value) -> Scripted + Send + Sync>) -> (String, Seen) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let seen: Seen = Arc::new(Mutex::new(Vec::new()));
        let seen2 = Arc::clone(&seen);
        tokio::spawn(async move {
            loop {
                let Ok((mut sock, _)) = listener.accept().await else {
                    return;
                };
                let (seen, reply) = (Arc::clone(&seen2), Arc::clone(&reply));
                tokio::spawn(async move {
                    let mut buf = Vec::new();
                    let mut tmp = [0u8; 8192];
                    let (head_end, len) = loop {
                        let n = sock.read(&mut tmp).await.unwrap_or(0);
                        if n == 0 {
                            return;
                        }
                        buf.extend_from_slice(&tmp[..n]);
                        if let Some(i) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
                            let head = String::from_utf8_lossy(&buf[..i]).to_string();
                            let len = head
                                .lines()
                                .find_map(|l| {
                                    let (k, v) = l.split_once(':')?;
                                    k.eq_ignore_ascii_case("content-length")
                                        .then(|| v.trim().parse::<usize>().ok())
                                        .flatten()
                                })
                                .unwrap_or(0);
                            break (i + 4, len);
                        }
                    };
                    while buf.len() < head_end + len {
                        let n = sock.read(&mut tmp).await.unwrap_or(0);
                        if n == 0 {
                            break;
                        }
                        buf.extend_from_slice(&tmp[..n]);
                    }
                    let body: Value =
                        serde_json::from_slice(&buf[head_end..head_end + len]).unwrap_or_default();
                    let r = reply(&body);
                    seen.lock().unwrap().push(body);
                    if let Some((code, text)) = r.status {
                        let head = format!(
                            "HTTP/1.1 {code} X\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
                            text.len()
                        );
                        let _ = sock.write_all(head.as_bytes()).await;
                        let _ = sock.write_all(text.as_bytes()).await;
                        let _ = sock.shutdown().await;
                        return;
                    }
                    let mut frames: Vec<String> = Vec::new();
                    if !r.text.is_empty() {
                        frames.push(
                            json!({"id":"c","model":"scripted-model","choices":[{"index":0,
                                "delta":{"content": r.text},"finish_reason":null}]})
                            .to_string(),
                        );
                    }
                    for (i, (name, args)) in r.calls.iter().enumerate() {
                        frames.push(
                            json!({"id":"c","model":"scripted-model","choices":[{"index":0,
                                "delta":{"tool_calls":[{"index":i,"id":format!("call_{i}"),"type":"function",
                                    "function":{"name":name,"arguments":args.to_string()}}]},
                                "finish_reason":null}]})
                            .to_string(),
                        );
                    }
                    let finish = if r.calls.is_empty() {
                        "stop"
                    } else {
                        "tool_calls"
                    };
                    let mut last = json!({"id":"c","model":"scripted-model","choices":[{"index":0,
                        "delta":{},"finish_reason":finish}]});
                    if let Some((i, o)) = r.usage {
                        last["usage"] = json!({"prompt_tokens": i, "completion_tokens": o,
                            "prompt_tokens_details": {"cached_tokens": 0}});
                    }
                    frames.push(last.to_string());
                    let mut payload = String::new();
                    for f in frames {
                        payload.push_str(&format!("data: {f}\n\n"));
                    }
                    payload.push_str("data: [DONE]\n\n");
                    let head = "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\nx-request-id: req_scripted\r\nconnection: close\r\n\r\n";
                    let _ = sock.write_all(head.as_bytes()).await;
                    let _ = sock.write_all(payload.as_bytes()).await;
                    let _ = sock.shutdown().await;
                });
            }
        });
        (format!("http://127.0.0.1:{port}/v4"), seen)
    }
}
