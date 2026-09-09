//! OpenAI-compatible Chat Completions adapter (streaming SSE with tool calls).

use serde_json::{Value, json};

use crate::contract::{ContentPart, ModelEvent, ModelRequest, Role, Usage, stop};

/// Build the request body.
#[must_use]
pub fn request_body(req: &ModelRequest) -> Value {
    let mut messages = Vec::new();
    for m in &req.messages {
        match m.role {
            Role::Tool => {
                for p in &m.parts {
                    if let ContentPart::ToolResult {
                        call_id, content, ..
                    } = p
                    {
                        messages.push(
                            json!({"role": "tool", "tool_call_id": call_id, "content": content}),
                        );
                    }
                }
            }
            Role::Assistant => {
                let text: String = m
                    .parts
                    .iter()
                    .filter_map(|p| match p {
                        ContentPart::Text { text } => Some(text.as_str()),
                        _ => None,
                    })
                    .collect();
                let calls: Vec<Value> = m
                    .parts
                    .iter()
                    .filter_map(|p| match p {
                        ContentPart::ToolCall {
                            call_id,
                            name,
                            arguments_json,
                        } => Some(json!({"id": call_id, "type": "function", "function": {"name": name, "arguments": arguments_json}})),
                        _ => None,
                    })
                    .collect();
                let mut msg = json!({"role": "assistant"});
                if !text.is_empty() || calls.is_empty() {
                    msg["content"] = Value::String(text);
                }
                if !calls.is_empty() {
                    msg["tool_calls"] = Value::Array(calls);
                }
                messages.push(msg);
            }
            Role::System | Role::User => {
                let text: String = m
                    .parts
                    .iter()
                    .filter_map(|p| match p {
                        ContentPart::Text { text } => Some(text.as_str()),
                        _ => None,
                    })
                    .collect();
                let role = if m.role == Role::System {
                    "system"
                } else {
                    "user"
                };
                messages.push(json!({"role": role, "content": text}));
            }
        }
    }
    let mut body = json!({
        "model": req.model_policy.model,
        "messages": messages,
        "stream": true,
        "stream_options": {"include_usage": true},
        "max_completion_tokens": req.max_output_tokens,
    });
    if !req.tool_projection.is_empty() {
        body["tools"] = Value::Array(
            req.tool_projection
                .iter()
                .map(|t| json!({"type": "function", "function": {"name": t.name, "description": t.description, "parameters": t.input_schema}}))
                .collect(),
        );
    }
    if let Some(effort) = &req.model_policy.reasoning_effort {
        body["reasoning_effort"] = Value::String(effort.clone());
    }
    if let Some(tier) = &req.model_policy.service_tier {
        body["service_tier"] = Value::String(tier.clone());
    }
    if req.response_format.as_deref() == Some("json_object") {
        body["response_format"] = json!({"type": "json_object"});
    }
    if let Some(k) = &req.cache_key {
        body["prompt_cache_key"] = Value::String(k.clone());
    }
    body
}

/// Streaming state: accumulates tool-call deltas by index.
#[derive(Debug, Default)]
pub struct Decoder {
    calls: Vec<(String, String, String)>, // (id, name, args)
    finish: Option<String>,
    done: bool,
}

impl Decoder {
    /// Decode one SSE data payload into events.
    pub fn decode(&mut self, data: &str) -> Vec<ModelEvent> {
        let mut out = Vec::new();
        if data.trim() == "[DONE]" {
            if !self.done {
                self.done = true;
                out.extend(self.flush_calls());
                out.push(ModelEvent::Completed {
                    stop_reason: normalize_finish(self.finish.as_deref()),
                });
            }
            return out;
        }
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
        if let Some(err) = v.get("error") {
            return vec![ModelEvent::Error {
                code: "PROVIDER_ERROR".into(),
                message: err
                    .get("message")
                    .and_then(Value::as_str)
                    .unwrap_or("provider error")
                    .to_owned(),
                retryable: false,
            }];
        }
        if let Some(u) = v.get("usage").filter(|u| !u.is_null()) {
            out.push(ModelEvent::Usage {
                usage: Usage {
                    input_tokens: u["prompt_tokens"].as_u64().unwrap_or(0),
                    output_tokens: u["completion_tokens"].as_u64().unwrap_or(0),
                    cached_input_tokens: u["prompt_tokens_details"]["cached_tokens"]
                        .as_u64()
                        .unwrap_or(0),
                },
            });
        }
        if let Some(model) = v.get("model").and_then(Value::as_str) {
            out.push(ModelEvent::ProviderMetadata {
                metadata: json!({"resolved_model": model, "service_tier": v.get("service_tier").cloned().unwrap_or(Value::Null), "provider_request_id": v.get("id").cloned().unwrap_or(Value::Null)}),
            });
        }
        for choice in v["choices"].as_array().into_iter().flatten() {
            let delta = &choice["delta"];
            if let Some(t) = delta["content"].as_str()
                && !t.is_empty()
            {
                out.push(ModelEvent::MessageDelta { text: t.to_owned() });
            }
            if let Some(t) = delta["reasoning_content"].as_str()
                && !t.is_empty()
            {
                out.push(ModelEvent::ReasoningDelta { text: t.to_owned() });
            }
            for tc in delta["tool_calls"].as_array().into_iter().flatten() {
                let idx = tc["index"].as_u64().unwrap_or(0) as usize;
                while self.calls.len() <= idx {
                    self.calls
                        .push((String::new(), String::new(), String::new()));
                }
                let entry = &mut self.calls[idx];
                if let Some(id) = tc["id"].as_str() {
                    entry.0 = id.to_owned();
                }
                if let Some(name) = tc["function"]["name"].as_str() {
                    entry.1 = name.to_owned();
                    out.push(ModelEvent::ToolCallStart {
                        call_id: entry.0.clone(),
                        name: name.to_owned(),
                    });
                }
                if let Some(a) = tc["function"]["arguments"].as_str()
                    && !a.is_empty()
                {
                    entry.2.push_str(a);
                    out.push(ModelEvent::ToolCallDelta {
                        call_id: entry.0.clone(),
                        arguments_delta: a.to_owned(),
                    });
                }
            }
            if let Some(f) = choice["finish_reason"].as_str() {
                self.finish = Some(f.to_owned());
            }
        }
        out
    }

    fn flush_calls(&mut self) -> Vec<ModelEvent> {
        std::mem::take(&mut self.calls)
            .into_iter()
            .filter(|(_, name, _)| !name.is_empty())
            .map(|(id, name, args)| ModelEvent::ToolCallComplete {
                call_id: id,
                name,
                arguments_json: if args.is_empty() { "{}".into() } else { args },
            })
            .collect()
    }

    /// Whether `[DONE]` was seen.
    #[must_use]
    pub fn finished(&self) -> bool {
        self.done
    }
}

fn normalize_finish(f: Option<&str>) -> String {
    match f {
        Some("tool_calls") | Some("function_call") => stop::TOOL_USE.into(),
        Some("length") => stop::MAX_TOKENS.into(),
        _ => stop::END_TURN.into(),
    }
}
