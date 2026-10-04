//! Anthropic Messages API adapter (streaming SSE with `tool_use` blocks).

use serde_json::{Value, json};

use crate::contract::{ContentPart, ModelEvent, ModelRequest, Role, Usage, stop};

/// One media block in this transport's shape. Images are native; anything
/// else is described in words rather than guessed into a format the API does
/// not document (the description keeps the digest, so the model can ask for
/// the bytes through `artifact.range`).
fn media_block(mime: &str, alt: &str, data: &crate::contract::MediaPayload) -> Value {
    if mime.starts_with("image/") {
        json!({"type": "image", "source": {"type": "base64", "media_type": mime, "data": data.0}})
    } else {
        json!({"type": "text", "text": format!("[attachment {mime}: {alt}] (not sent as bytes: this transport takes images)")})
    }
}

/// Where a cache breakpoint lands once the messages are laid out in this
/// transport's shape.
#[derive(Clone, Copy)]
enum Anchor {
    /// The last block of the top-level `system` array.
    System(usize),
    /// The last content block of the message at this index.
    Message(usize),
}

/// The marker this transport takes for "cache everything up to here".
fn ephemeral() -> Value {
    json!({"type": "ephemeral"})
}

/// Most cache markers the Messages API accepts on one request.
const MAX_CACHE_MARKERS: usize = 4;

/// Build the request body. System messages become the top-level `system`.
///
/// When the caller declares stable-prefix breakpoints
/// (`ModelRequest::cache_breakpoints`), `system` becomes an array of blocks
/// (one per system message, never flattened into a string that cannot carry a
/// marker), and an ephemeral `cache_control` marker is placed at each
/// breakpoint: on the last system block for a system message, on the last
/// content block of a conversation message otherwise, and on the last tool
/// definition so the tools stay cacheable on their own while room remains
/// under the API's four markers. The prefix order the API caches is tools,
/// system, messages.
#[must_use]
pub fn request_body(req: &ModelRequest) -> Value {
    let mut system_texts: Vec<String> = Vec::new();
    let mut messages: Vec<Value> = Vec::new();
    // For each input message, where a breakpoint after it lands.
    let mut anchors: Vec<Option<Anchor>> = Vec::with_capacity(req.messages.len());
    for m in &req.messages {
        let before = messages.len();
        match m.role {
            Role::System => {
                for p in &m.parts {
                    if let ContentPart::Text { text } = p {
                        system_texts.push(text.clone());
                    }
                }
            }
            Role::User | Role::Tool => {
                // Tool results are user-role content blocks on this transport
                // (docs/25 "Provider media normalization": placement may change,
                // call identity never does).
                // This transport takes media inside the tool result itself, so
                // nothing is split here (REQ-EV-0188: placement is the
                // adapter's business, semantics are not).
                let media_for = |id: Option<&str>| -> Vec<Value> {
                    m.parts
                        .iter()
                        .filter_map(|p| match p {
                            ContentPart::Media {
                                mime,
                                alt,
                                call_id,
                                data_base64,
                                ..
                            } if call_id.as_deref() == id => {
                                Some(media_block(mime, alt, data_base64))
                            }
                            _ => None,
                        })
                        .collect()
                };
                let blocks: Vec<Value> = m
                    .parts
                    .iter()
                    .flat_map(|p| match p {
                        ContentPart::Text { text } => {
                            vec![json!({"type": "text", "text": text})]
                        }
                        ContentPart::ToolResult {
                            call_id,
                            content,
                            is_error,
                        } => {
                            let mut content_blocks =
                                vec![json!({"type": "text", "text": content})];
                            content_blocks.extend(media_for(Some(call_id.as_str())));
                            vec![json!({"type": "tool_result", "tool_use_id": call_id, "content": content_blocks, "is_error": is_error})]
                        }
                        // Media that belongs to no tool call rides in the
                        // message itself.
                        ContentPart::Media {
                            mime,
                            alt,
                            call_id: None,
                            data_base64,
                            ..
                        } => vec![media_block(mime, alt, data_base64)],
                        ContentPart::Media { .. } | ContentPart::ToolCall { .. } => vec![],
                    })
                    .collect();
                messages.push(json!({"role": "user", "content": blocks}));
            }
            Role::Assistant => {
                let blocks: Vec<Value> = m
                    .parts
                    .iter()
                    .map(|p| match p {
                        ContentPart::Text { text } => json!({"type": "text", "text": text}),
                        ContentPart::ToolCall {
                            call_id,
                            name,
                            arguments_json,
                        } => json!({"type": "tool_use", "id": call_id, "name": name, "input": serde_json::from_str::<Value>(arguments_json).unwrap_or(json!({}))}),
                        ContentPart::ToolResult { .. } | ContentPart::Media { .. } => Value::Null,
                    })
                    .filter(|v| !v.is_null())
                    .collect();
                messages.push(json!({"role": "assistant", "content": blocks}));
            }
        }
        anchors.push(match m.role {
            Role::System => system_texts.len().checked_sub(1).map(Anchor::System),
            _ if messages.len() > before => Some(Anchor::Message(messages.len() - 1)),
            _ => None,
        });
    }
    let mut system_blocks: Vec<Value> = system_texts
        .iter()
        .map(|t| json!({"type": "text", "text": t}))
        .collect();
    let mut tools: Vec<Value> = req
        .tool_projection
        .iter()
        .map(|t| json!({"name": t.name, "description": t.description, "input_schema": t.input_schema}))
        .collect();
    if !req.cache_breakpoints.is_empty() {
        // Distinct anchors, earliest first, at most the API's limit.
        let mut placed: Vec<(usize, bool)> = Vec::new();
        let mut order: Vec<usize> = req.cache_breakpoints.clone();
        order.sort_unstable();
        order.dedup();
        for at in order {
            let Some(Some(anchor)) = anchors.get(at).copied() else {
                continue;
            };
            let key = match anchor {
                Anchor::System(i) => (i, true),
                Anchor::Message(i) => (i, false),
            };
            if !placed.contains(&key) {
                placed.push(key);
            }
        }
        // The newest prefixes matter most when more were listed than fit.
        let keep_from = placed.len().saturating_sub(MAX_CACHE_MARKERS);
        let placed = &placed[keep_from..];
        let spare_for_tools = placed.len() < MAX_CACHE_MARKERS;
        for &(i, is_system) in placed {
            let block = if is_system {
                system_blocks.get_mut(i)
            } else {
                messages
                    .get_mut(i)
                    .and_then(|m| m["content"].as_array_mut())
                    .and_then(|c| c.last_mut())
            };
            // The API refuses a marker on an empty text block.
            if let Some(block) = block
                && !(block["type"] == "text" && block["text"] == "")
            {
                block["cache_control"] = ephemeral();
            }
        }
        if spare_for_tools && let Some(last) = tools.last_mut() {
            last["cache_control"] = ephemeral();
        }
    }
    let mut body = json!({
        "model": req.model_policy.model,
        "messages": messages,
        "stream": true,
        "max_tokens": req.max_output_tokens,
    });
    if req.cache_breakpoints.is_empty() {
        let system = system_texts.join("\n");
        if !system.is_empty() {
            body["system"] = Value::String(system);
        }
    } else if !system_blocks.is_empty() {
        body["system"] = Value::Array(system_blocks);
    }
    if !tools.is_empty() {
        body["tools"] = Value::Array(tools);
    }
    if let Some(effort) = &req.model_policy.reasoning_effort {
        let wanted: u32 = match effort.as_str() {
            "low" => 1024,
            "high" => 16384,
            _ => 4096,
        };
        // Thinking is drawn from `max_tokens` and the API requires the
        // budget to be smaller than it (and at least 1024). It is also held
        // to half of `max_tokens`, so reasoning can never leave the visible
        // answer no room. A request whose output cap leaves less than the
        // minimum goes without a thinking block rather than as a 400.
        let budget = wanted.min(req.max_output_tokens / 2);
        if budget >= 1024 {
            body["thinking"] = json!({"type": "enabled", "budget_tokens": budget});
        }
    }
    if let Some(tier) = &req.model_policy.service_tier {
        body["service_tier"] = Value::String(tier.clone());
    }
    body
}

/// Streaming state: content blocks by index.
#[derive(Debug, Default)]
pub struct Decoder {
    blocks: Vec<Option<(String, String, String)>>, // tool_use: (id, name, partial json)
    stop_reason: Option<String>,
    usage: Usage,
    /// The Messages API reports plain input, cache reads and cache writes
    /// as three disjoint counts, each cumulative for the message (a later
    /// report replaces an earlier one, it never adds to it).
    plain_input: u64,
    cache_read: u64,
    cache_write: u64,
    done: bool,
}

impl Decoder {
    /// Take in a `usage` object from `message_start` or `message_delta`:
    /// each count it carries replaces the one held, and the contract's total
    /// input is their sum, with reads and writes as its subsets.
    fn absorb_usage(&mut self, u: &Value) {
        if let Some(n) = u["input_tokens"].as_u64() {
            self.plain_input = n;
        }
        if let Some(n) = u["cache_read_input_tokens"].as_u64() {
            self.cache_read = n;
        }
        if let Some(n) = u["cache_creation_input_tokens"].as_u64() {
            self.cache_write = n;
        }
        if let Some(n) = u["output_tokens"].as_u64() {
            self.usage.output_tokens = n;
        }
        self.usage.input_tokens = self
            .plain_input
            .saturating_add(self.cache_read)
            .saturating_add(self.cache_write);
        self.usage.cached_input_tokens = self.cache_read;
        self.usage.cache_write_input_tokens = self.cache_write;
    }

    /// Decode one SSE event (`event` name + data).
    pub fn decode(&mut self, event: Option<&str>, data: &str) -> Vec<ModelEvent> {
        let v: Value = match serde_json::from_str(data) {
            Ok(v) => v,
            Err(e) => {
                return vec![ModelEvent::Error {
                    code: "MALFORMED_STREAM".into(),
                    message: format!("invalid JSON in stream: {e}"),
                    retryable: false,
                }];
            }
        };
        let kind = event.or_else(|| v["type"].as_str()).unwrap_or("");
        let mut out = Vec::new();
        match kind {
            "message_start" => {
                let m = &v["message"];
                self.absorb_usage(&m["usage"]);
                out.push(ModelEvent::ProviderMetadata {
                    metadata: json!({"resolved_model": m["model"].clone(), "provider_request_id": m["id"].clone()}),
                });
            }
            "content_block_start" => {
                let idx = v["index"].as_u64().unwrap_or(0) as usize;
                while self.blocks.len() <= idx {
                    self.blocks.push(None);
                }
                let cb = &v["content_block"];
                if cb["type"].as_str() == Some("tool_use") {
                    let id = cb["id"].as_str().unwrap_or_default().to_owned();
                    let name = cb["name"].as_str().unwrap_or_default().to_owned();
                    out.push(ModelEvent::ToolCallStart {
                        call_id: id.clone(),
                        name: name.clone(),
                    });
                    self.blocks[idx] = Some((id, name, String::new()));
                } else if let Some(t) = cb["text"].as_str()
                    && !t.is_empty()
                {
                    out.push(ModelEvent::MessageDelta { text: t.to_owned() });
                }
            }
            "content_block_delta" => {
                let idx = v["index"].as_u64().unwrap_or(0) as usize;
                let d = &v["delta"];
                match d["type"].as_str() {
                    Some("text_delta") => {
                        if let Some(t) = d["text"].as_str() {
                            out.push(ModelEvent::MessageDelta { text: t.to_owned() });
                        }
                    }
                    Some("thinking_delta") => {
                        if let Some(t) = d["thinking"].as_str() {
                            out.push(ModelEvent::ReasoningDelta { text: t.to_owned() });
                        }
                    }
                    Some("input_json_delta") => {
                        if let Some(pj) = d["partial_json"].as_str()
                            && let Some(Some(b)) = self.blocks.get_mut(idx)
                        {
                            b.2.push_str(pj);
                            out.push(ModelEvent::ToolCallDelta {
                                call_id: b.0.clone(),
                                arguments_delta: pj.to_owned(),
                            });
                        }
                    }
                    _ => {}
                }
            }
            "content_block_stop" => {
                let idx = v["index"].as_u64().unwrap_or(0) as usize;
                if let Some(slot) = self.blocks.get_mut(idx)
                    && let Some((id, name, args)) = slot.take()
                {
                    out.push(ModelEvent::ToolCallComplete {
                        call_id: id,
                        name,
                        arguments_json: if args.is_empty() { "{}".into() } else { args },
                    });
                }
            }
            "message_delta" => {
                if let Some(s) = v["delta"]["stop_reason"].as_str() {
                    self.stop_reason = Some(s.to_owned());
                }
                self.absorb_usage(&v["usage"]);
                out.push(ModelEvent::Usage {
                    usage: self.usage.clone(),
                });
            }
            "message_stop" => {
                if !self.done {
                    self.done = true;
                    out.push(ModelEvent::Completed {
                        stop_reason: match self.stop_reason.as_deref() {
                            Some("tool_use") => stop::TOOL_USE.into(),
                            Some("max_tokens") => stop::MAX_TOKENS.into(),
                            _ => stop::END_TURN.into(),
                        },
                    });
                }
            }
            "error" => {
                let e = &v["error"];
                let t = e["type"].as_str().unwrap_or("api_error");
                out.push(ModelEvent::Error {
                    code: format!("PROVIDER_{}", t.to_uppercase()),
                    message: e["message"].as_str().unwrap_or("provider error").to_owned(),
                    retryable: matches!(t, "overloaded_error" | "rate_limit_error"),
                });
            }
            _ => {} // ping and unknown event types are ignored
        }
        out
    }

    /// Whether `message_stop` was seen.
    #[must_use]
    pub fn finished(&self) -> bool {
        self.done
    }
}
