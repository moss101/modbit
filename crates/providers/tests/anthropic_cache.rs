//! Anthropic prompt-cache markers on the wire (audit C defect 7, audit G;
//! FIX-13): the exact request body the adapter builds when the caller
//! declares where its stable prefix ends. The assertions are on the JSON the
//! API receives, not on an intermediate structure: `system` is an array of
//! blocks (a flattened string cannot carry a marker), `cache_control` sits on
//! the last system block, on the last block of the message each breakpoint
//! names and on the last tool, never more than the API's four, never on an
//! empty block, and a request that declares no prefix is byte-for-byte what
//! it was before.

use modbit_providers::anthropic::{Decoder, request_body};
use modbit_providers::{
    ContentPart, MediaPayload, Message, ModelEvent, ModelPolicy, ModelRequest, Role, ToolProjection,
};
use serde_json::{Value, json};

fn request(messages: Vec<Message>, breakpoints: Vec<usize>, tools: usize) -> ModelRequest {
    ModelRequest {
        request_id: "r".into(),
        model_policy: ModelPolicy {
            endpoint: "anthropic".into(),
            model: "claude-x".into(),
            reasoning_effort: None,
            service_tier: None,
        },
        messages,
        tool_projection: (0..tools)
            .map(|i| ToolProjection {
                name: format!("t{i}"),
                description: format!("tool {i}"),
                input_schema: json!({"type": "object"}),
            })
            .collect(),
        response_format: None,
        cache_key: Some("k".into()),
        cache_breakpoints: breakpoints,
        max_output_tokens: 100,
        timeout_ms: 1000,
        policy_tags: vec![],
    }
}

fn marker() -> Value {
    json!({"type": "ephemeral"})
}

/// The shape the prompt compiler produces: three system segments, the task
/// turn, a transcript of one tool round trip, the volatile tail.
fn compiled_shape() -> Vec<Message> {
    vec![
        Message::text(Role::System, "S0 system"),
        Message::text(Role::System, "S1 rules"),
        Message::text(Role::System, "S2 epoch"),
        Message::text(Role::User, "goal"),
        Message {
            role: Role::Assistant,
            parts: vec![ContentPart::ToolCall {
                call_id: "c1".into(),
                name: "fs.read".into(),
                arguments_json: "{}".into(),
            }],
        },
        Message {
            role: Role::Tool,
            parts: vec![ContentPart::ToolResult {
                call_id: "c1".into(),
                content: "ok".into(),
                is_error: false,
            }],
        },
        Message::text(Role::User, "tail: harness_state"),
    ]
}

#[test]
fn breakpoints_become_cache_control_markers_at_exactly_the_declared_places() {
    let body = request_body(&request(compiled_shape(), vec![2, 3, 5], 2));
    assert_eq!(
        body,
        json!({
            "model": "claude-x",
            "stream": true,
            "max_tokens": 100,
            "system": [
                {"type": "text", "text": "S0 system"},
                {"type": "text", "text": "S1 rules"},
                {"type": "text", "text": "S2 epoch", "cache_control": marker()},
            ],
            "messages": [
                {"role": "user", "content": [
                    {"type": "text", "text": "goal", "cache_control": marker()},
                ]},
                {"role": "assistant", "content": [
                    {"type": "tool_use", "id": "c1", "name": "fs.read", "input": {}},
                ]},
                {"role": "user", "content": [
                    {"type": "tool_result", "tool_use_id": "c1", "is_error": false,
                     "content": [{"type": "text", "text": "ok"}],
                     "cache_control": marker()},
                ]},
                {"role": "user", "content": [
                    {"type": "text", "text": "tail: harness_state"},
                ]},
            ],
            "tools": [
                {"name": "t0", "description": "tool 0", "input_schema": {"type": "object"}},
                {"name": "t1", "description": "tool 1", "input_schema": {"type": "object"},
                 "cache_control": marker()},
            ],
        })
    );
}

#[test]
fn a_request_that_declares_no_prefix_is_unchanged_and_carries_no_marker() {
    let body = request_body(&request(compiled_shape(), vec![], 1));
    assert_eq!(
        body["system"],
        json!("S0 system\nS1 rules\nS2 epoch"),
        "system stays one string"
    );
    assert!(
        !body.to_string().contains("cache_control"),
        "no marker without a declared prefix: {body}"
    );
}

#[test]
fn the_api_limit_of_four_markers_is_kept_and_the_newest_prefixes_win() {
    // Five distinct anchors are declared; the API takes four in all, so the
    // oldest is dropped and the tools get none.
    let mut messages = compiled_shape();
    messages.insert(4, Message::text(Role::User, "steer"));
    let body = request_body(&request(messages, vec![0, 2, 3, 4, 6], 2));
    let wire = body.to_string();
    assert_eq!(wire.matches("cache_control").count(), 4, "{wire}");
    assert!(
        body["system"][0].get("cache_control").is_none(),
        "the oldest anchor was dropped: {wire}"
    );
    assert!(
        body["system"][2].get("cache_control").is_some(),
        "the system's last block keeps its marker: {wire}"
    );
    assert!(
        body["tools"][1].get("cache_control").is_none(),
        "no room left for a tool marker: {wire}"
    );
}

#[test]
fn a_marker_is_never_put_on_an_empty_block_and_a_bad_index_is_ignored() {
    let messages = vec![
        Message::text(Role::System, "S"),
        Message::text(Role::User, ""),
        Message::text(Role::User, "u"),
    ];
    // Index 1 names an empty text block, index 9 names nothing.
    let body = request_body(&request(messages, vec![1, 9], 0));
    assert!(!body.to_string().contains("cache_control"), "{body}");
}

#[test]
fn media_in_the_task_turn_rides_in_the_user_message_before_the_marker() {
    let mut task_turn = Message::text(Role::User, "goal");
    task_turn.parts.push(ContentPart::Media {
        source_ref: "e".repeat(64),
        mime: "image/png".into(),
        alt: "attachment".into(),
        call_id: None,
        data_base64: MediaPayload("AAAA".into()),
    });
    let messages = vec![
        Message::text(Role::System, "S"),
        task_turn,
        Message::text(Role::User, "tail"),
    ];
    let body = request_body(&request(messages, vec![0, 1], 0));
    assert_eq!(
        body["messages"][0],
        json!({"role": "user", "content": [
            {"type": "text", "text": "goal"},
            {"type": "image", "source": {"type": "base64", "media_type": "image/png", "data": "AAAA"},
             "cache_control": marker()},
        ]}),
        "the marker follows the image: it caches the bytes too"
    );
    assert_eq!(body["system"][0]["cache_control"], marker());
}

/// The usage the API reports on a first turn (a cache write) and a second
/// (a cache read) lands in the contract with reads and writes as subsets of
/// the total input, so accounting can price each.
#[test]
fn cache_write_then_cache_read_usage_is_reported_in_the_contract_shape() {
    let usage_of = |start: Value, delta: Value| {
        let mut d = Decoder::default();
        d.decode(Some("message_start"), &start.to_string());
        d.decode(Some("message_delta"), &delta.to_string())
            .into_iter()
            .find_map(|e| match e {
                ModelEvent::Usage { usage } => Some(usage),
                _ => None,
            })
            .expect("usage")
    };
    let first = usage_of(
        json!({"type": "message_start", "message": {"id": "m1", "model": "claude-x",
            "usage": {"input_tokens": 12, "cache_creation_input_tokens": 3000, "cache_read_input_tokens": 0, "output_tokens": 1}}}),
        json!({"type": "message_delta", "delta": {"stop_reason": "end_turn"}, "usage": {"output_tokens": 40}}),
    );
    assert_eq!(
        (
            first.input_tokens,
            first.cache_write_input_tokens,
            first.cached_input_tokens,
            first.output_tokens
        ),
        (3012, 3000, 0, 40)
    );
    let second = usage_of(
        json!({"type": "message_start", "message": {"id": "m2", "model": "claude-x",
            "usage": {"input_tokens": 80, "cache_creation_input_tokens": 120, "cache_read_input_tokens": 3000, "output_tokens": 1}}}),
        json!({"type": "message_delta", "delta": {"stop_reason": "end_turn"}, "usage": {"output_tokens": 55}}),
    );
    assert_eq!(
        (
            second.input_tokens,
            second.cache_write_input_tokens,
            second.cached_input_tokens
        ),
        (3200, 120, 3000)
    );
}
