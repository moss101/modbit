//! A small non-streaming client for an OpenAI-compatible chat endpoint, for
//! the live evaluations that talk to the model themselves (PX-137's tool
//! loop, PX-138's recall questions). It is configured exactly as the Core is:
//! `OPENAI_API_KEY`, `MODBIT_OPENAI_BASE_URL`, `MODBIT_OPENAI_MODELS` (the
//! priced catalog), `MODBIT_OPENAI_AUTH` and `MODBIT_OPENAI_EXTRA_BODY`, and
//! it builds the request URL the way the product's gateway does.
//!
//! The reply carries the usage the gateway returned, which is the only thing
//! the [`crate::spend::SpendMeter`] is charged from. The key is held in a
//! field that is never formatted: not in `Debug`, not in an error.

use std::time::{Duration, Instant};

use modbit_providers::{ProviderKind, wire_url};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::spend::{Price, SpendMeter, price_from_catalog};

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
    /// Wall time of the successful attempt, milliseconds.
    pub latency_ms: u64,
    /// The model id the gateway says answered.
    pub model_answered: String,
    /// The gateway's request id header, when it sent one.
    pub request_id: String,
}

/// A configured endpoint.
#[derive(Clone)]
pub struct Chat {
    http: reqwest::Client,
    url: String,
    key: String,
    bearer: bool,
    extra_body: serde_json::Map<String, Value>,
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
    /// The call failed (after its retries).
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
        let base_url = present("MODBIT_OPENAI_BASE_URL")
            .unwrap_or_else(|| "https://api.openai.com".to_owned());
        let models = present("MODBIT_OPENAI_MODELS").ok_or(
            "MODBIT_OPENAI_MODELS is not set: the harness prices a call from the catalog and an unknown price is not free",
        )?;
        let price = price_from_catalog(&models, model)?;
        let bearer = match present("MODBIT_OPENAI_AUTH").as_deref() {
            None | Some("native" | "bearer") => true,
            Some(other) => {
                return Err(format!(
                    "MODBIT_OPENAI_AUTH: `{other}` is not native or bearer"
                ));
            }
        };
        let extra_body = match present("MODBIT_OPENAI_EXTRA_BODY") {
            None => serde_json::Map::new(),
            Some(text) => match serde_json::from_str::<Value>(&text) {
                Ok(Value::Object(m)) => m,
                _ => return Err("MODBIT_OPENAI_EXTRA_BODY: expected a JSON object".into()),
            },
        };
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(180))
            .build()
            .map_err(|e| e.to_string())?;
        Ok(Self {
            http,
            url: wire_url(ProviderKind::OpenAi, &base_url),
            key,
            bearer,
            extra_body,
            model: model.to_owned(),
            price,
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
    /// The cap, or a failure after three attempts.
    pub async fn complete(
        &self,
        meter: &SpendMeter,
        reserve_usd: f64,
        messages: &[Value],
        tools: &[Value],
        max_tokens: u32,
    ) -> Result<Reply, ChatError> {
        meter.admit(reserve_usd).map_err(ChatError::Cap)?;
        let mut body = json!({
            "model": self.model,
            "messages": messages,
            "max_tokens": max_tokens,
            "stream": false,
            "temperature": 0.0,
        });
        if !tools.is_empty() {
            body["tools"] = Value::Array(tools.to_vec());
        }
        for (k, v) in &self.extra_body {
            if body.get(k).is_none() {
                body[k] = v.clone();
            }
        }
        let mut last = String::new();
        for attempt in 0..3u32 {
            if attempt > 0 {
                tokio::time::sleep(Duration::from_millis(1_500 * u64::from(attempt))).await;
            }
            let started = Instant::now();
            let mut req = self.http.post(&self.url).json(&body);
            req = if self.bearer {
                req.bearer_auth(&self.key)
            } else {
                req.header("x-api-key", &self.key)
            };
            let resp = match req.send().await {
                Ok(r) => r,
                Err(e) => {
                    last = format!("transport: {}", e.without_url());
                    continue;
                }
            };
            let status = resp.status();
            let request_id = resp
                .headers()
                .get("x-request-id")
                .and_then(|v| v.to_str().ok())
                .unwrap_or_default()
                .to_owned();
            let text = resp.text().await.unwrap_or_default();
            if status.as_u16() == 429 || status.is_server_error() {
                last = format!("HTTP {status}: {}", snippet(&text));
                continue;
            }
            if !status.is_success() {
                // Charged nothing: the gateway rejected the request.
                return Err(ChatError::Failed(format!(
                    "HTTP {status}: {}",
                    snippet(&text)
                )));
            }
            let v: Value = serde_json::from_str(&text)
                .map_err(|e| ChatError::Failed(format!("reply is not JSON: {e}")))?;
            let reply = parse_reply(
                &v,
                request_id,
                u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
            );
            if reply.usage_known {
                meter.charge(self.price, reply.input_tokens, reply.output_tokens);
            } else {
                meter.charge_unknown();
            }
            return Ok(reply);
        }
        Err(ChatError::Failed(last))
    }
}

fn snippet(text: &str) -> String {
    text.chars().take(300).collect()
}

/// A chat-completions reply, parsed.
#[must_use]
pub fn parse_reply(v: &Value, request_id: String, latency_ms: u64) -> Reply {
    let choice = &v["choices"][0];
    let message = &choice["message"];
    let usage = &v["usage"];
    let tool_calls = message["tool_calls"]
        .as_array()
        .map(|calls| {
            calls
                .iter()
                .map(|c| ToolCall {
                    id: c["id"].as_str().unwrap_or_default().to_owned(),
                    name: c["function"]["name"]
                        .as_str()
                        .unwrap_or_default()
                        .to_owned(),
                    arguments: c["function"]["arguments"]
                        .as_str()
                        .unwrap_or("{}")
                        .to_owned(),
                })
                .collect()
        })
        .unwrap_or_default();
    Reply {
        text: message["content"].as_str().unwrap_or_default().to_owned(),
        tool_calls,
        input_tokens: usage["prompt_tokens"].as_u64().unwrap_or(0),
        output_tokens: usage["completion_tokens"].as_u64().unwrap_or(0),
        cached_input_tokens: usage["prompt_tokens_details"]["cached_tokens"]
            .as_u64()
            .unwrap_or(0),
        usage_known: usage["prompt_tokens"].is_u64() && usage["completion_tokens"].is_u64(),
        finish_reason: choice["finish_reason"]
            .as_str()
            .unwrap_or_default()
            .to_owned(),
        latency_ms,
        model_answered: v["model"].as_str().unwrap_or_default().to_owned(),
        request_id,
    }
}

/// A scripted OpenAI-compatible chat server for the offline tests of the
/// live harnesses: one reply per request, computed from the request body,
/// with a usage block. It is a stand-in for the model and says so: a harness
/// that runs against it has not been run live.
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
                    let (status, payload) = if let Some((code, text)) = r.status {
                        (code, text)
                    } else {
                        let calls: Vec<Value> = r
                            .calls
                            .iter()
                            .enumerate()
                            .map(|(i, (name, args))| {
                                json!({"id": format!("call_{i}"), "type": "function",
                                    "function": {"name": name, "arguments": args.to_string()}})
                            })
                            .collect();
                        let mut message = json!({"role": "assistant", "content": r.text});
                        if !calls.is_empty() {
                            message["tool_calls"] = Value::Array(calls);
                        }
                        let mut v = json!({"id": "scripted", "model": "scripted-model",
                            "choices": [{"index": 0, "message": message,
                                "finish_reason": if r.calls.is_empty() { "stop" } else { "tool_calls" }}]});
                        if let Some((i, o)) = r.usage {
                            v["usage"] = json!({"prompt_tokens": i, "completion_tokens": o});
                        }
                        (200, v.to_string())
                    };
                    let head = format!(
                        "HTTP/1.1 {status} X\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
                        payload.len()
                    );
                    let _ = sock.write_all(head.as_bytes()).await;
                    let _ = sock.write_all(payload.as_bytes()).await;
                    let _ = sock.shutdown().await;
                });
            }
        });
        (format!("http://127.0.0.1:{port}/v4"), seen)
    }
}
