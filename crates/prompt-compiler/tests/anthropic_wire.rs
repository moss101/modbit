//! The real prompt compiler into the real Anthropic request builder
//! (FIX-13): what the Messages API receives turn after turn. The compiler's
//! output is not inspected on its own; the assertions are on the wire body, so
//! a regression in either half (the layout that keeps volatile state out of
//! the prefix, or the marker placement) fails here. Live provider calls are
//! not available to this suite: that a real response reports cache-read tokens
//! on turn two is shown only by a live run (`.github/workflows/live-providers.yml`),
//! which this test deliberately does not fake.

use modbit_prompt_compiler::{ContextFragment, PromptInput, compile};
use modbit_providers::anthropic::request_body;
use modbit_providers::{ContentPart, Message, ModelPolicy, Role, ToolProjection};
use serde_json::{Value, json};

fn input(turn: usize) -> PromptInput {
    let mut transcript = Vec::new();
    for n in 0..turn {
        transcript.push(Message {
            role: Role::Assistant,
            parts: vec![ContentPart::ToolCall {
                call_id: format!("c{n}"),
                name: "fs.read".into(),
                arguments_json: format!("{{\"path\":\"f{n}.rs\"}}"),
            }],
        });
        transcript.push(Message {
            role: Role::Tool,
            parts: vec![ContentPart::ToolResult {
                call_id: format!("c{n}"),
                content: format!("body of f{n}"),
                is_error: false,
            }],
        });
    }
    PromptInput {
        goal: "make the parser reject empty input".into(),
        task_attachments: vec![],
        workspace_root: Some("/repo".into()),
        execution_profile: "local_trusted".into(),
        workspace_rules: vec!["Use cargo fmt.".into()],
        skills: vec![],
        compaction_summary: None,
        harness_state: json!({
            "turns": turn,
            "tool_calls": turn,
            "no_progress_turns": turn % 2,
            "baseline_checks": (0..turn * 5).map(|n| (format!("check_{n}"), "PASS")).collect::<Vec<_>>(),
        }),
        transcript,
        tools: vec![
            ToolProjection {
                name: "fs.read".into(),
                description: "Read a file".into(),
                input_schema: json!({"type": "object"}),
            },
            ToolProjection {
                name: "plan.update".into(),
                description: "Record a plan".into(),
                input_schema: json!({"type": "object"}),
            },
        ],
        context: vec![ContextFragment {
            source_ref: format!("workspace:src/p{turn}.rs"),
            path: format!("src/p{turn}.rs"),
            workspace_revision: 4,
            content_hash: "b".repeat(64),
            retrieval_reason: "exact_symbol".into(),
            lines: None,
            text: format!("fn p{turn}() {{}}"),
            ephemeral: false,
        }],
        model_policy: ModelPolicy {
            endpoint: "anthropic".into(),
            model: "claude-sonnet-4-5".into(),
            reasoning_effort: None,
            service_tier: None,
        },
        max_output_tokens: 4096,
        timeout_ms: 120_000,
    }
}

fn wire(turn: usize) -> Value {
    request_body(&compile(input(turn)).request)
}

/// Every `cache_control` marker in the body, as a path to the block that
/// carries it.
fn markers(body: &Value) -> Vec<String> {
    let mut out = Vec::new();
    if let Some(system) = body["system"].as_array() {
        for (i, b) in system.iter().enumerate() {
            if b.get("cache_control").is_some() {
                out.push(format!("system[{i}]"));
            }
        }
    }
    if let Some(messages) = body["messages"].as_array() {
        for (i, m) in messages.iter().enumerate() {
            for (j, b) in m["content"].as_array().into_iter().flatten().enumerate() {
                if b.get("cache_control").is_some() {
                    out.push(format!("messages[{i}].content[{j}]"));
                }
            }
        }
    }
    if let Some(tools) = body["tools"].as_array() {
        for (i, t) in tools.iter().enumerate() {
            if t.get("cache_control").is_some() {
                out.push(format!("tools[{i}]"));
            }
        }
    }
    out
}

/// The body with the markers removed: what the API hashes as content.
fn without_markers(v: &Value) -> Value {
    match v {
        Value::Object(m) => Value::Object(
            m.iter()
                .filter(|(k, _)| k.as_str() != "cache_control")
                .map(|(k, v)| (k.clone(), without_markers(v)))
                .collect(),
        ),
        Value::Array(a) => Value::Array(a.iter().map(without_markers).collect()),
        other => other.clone(),
    }
}

#[test]
fn the_compiled_prompt_carries_markers_on_the_system_the_task_turn_the_transcript_end_and_the_tools()
 {
    // Turn 1: no transcript yet, so the task turn closes the prefix.
    let first = wire(0);
    assert_eq!(
        markers(&first),
        ["system[2]", "messages[0].content[0]", "tools[1]"]
    );
    assert_eq!(first["system"].as_array().unwrap().len(), 3);
    assert!(
        first["system"][0]["text"]
            .as_str()
            .unwrap()
            .starts_with("You are Modbit"),
        "the system segments are separate blocks, not one flattened string"
    );
    assert!(
        first["system"][1]["text"]
            .as_str()
            .unwrap()
            .contains("Use cargo fmt.")
    );
    // The task turn is stable text only; the harness state is the last
    // message and carries no marker.
    let n = first["messages"].as_array().unwrap().len();
    assert_eq!(n, 2);
    assert!(
        first["messages"][0]["content"][0]["text"]
            .as_str()
            .unwrap()
            .starts_with("Task goal: make the parser reject empty input")
    );
    assert!(
        !first["messages"][0]["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("harness_state")
    );
    let tail = first["messages"][n - 1]["content"][0]["text"]
        .as_str()
        .unwrap();
    assert!(tail.contains("harness_state:") && tail.contains("Retrieved context"));
    assert!(
        first["messages"][n - 1]["content"][0]
            .get("cache_control")
            .is_none()
    );

    // Turn 4: the marker moves to the end of the transcript, the last tool
    // result before the volatile tail; the tail never carries one.
    let later = wire(3);
    let n = later["messages"].as_array().unwrap().len();
    assert_eq!(n, 1 + 6 + 1, "task turn, three round trips, the tail");
    assert_eq!(
        markers(&later),
        [
            "system[2]".to_owned(),
            "messages[0].content[0]".to_owned(),
            format!("messages[{}].content[0]", n - 2),
            "tools[1]".to_owned(),
        ]
    );
    assert_eq!(
        later["messages"][n - 2]["content"][0]["type"],
        "tool_result",
        "the marker is on the last block of the stable transcript"
    );
}

/// The point of the layout: the content the API caches on turn N is the
/// content that opens turn N+1, so turn N+1 reads it instead of writing it
/// again. Compared as the API compares: the request up to the marker, markers
/// removed.
#[test]
fn each_turns_cached_prefix_is_the_opening_of_the_next_turns_request() {
    let mut previous = wire(0);
    for turn in 1..=20 {
        let next = wire(turn);
        // Everything the previous request cached: tools, system, and the
        // messages up to its last marker.
        let last_marked = previous["messages"]
            .as_array()
            .unwrap()
            .iter()
            .rposition(|m| {
                m["content"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|b| b.get("cache_control").is_some())
            })
            .unwrap();
        let cached = json!({
            "tools": without_markers(&previous["tools"]),
            "system": without_markers(&previous["system"]),
            "messages": without_markers(&Value::Array(
                previous["messages"].as_array().unwrap()[..=last_marked].to_vec()
            )),
        });
        let reopened = json!({
            "tools": without_markers(&next["tools"]),
            "system": without_markers(&next["system"]),
            "messages": without_markers(&Value::Array(
                next["messages"].as_array().unwrap()[..=last_marked].to_vec()
            )),
        });
        assert_eq!(
            cached, reopened,
            "turn {turn}: the prefix the previous turn cached changed"
        );
        previous = next;
    }
}

/// Without cache markers (a caller that declares no prefix) the same prompt
/// compiles to the old wire: one system string, no marker. A guard that the
/// marker path is opt-in per request.
#[test]
fn a_request_with_its_breakpoints_removed_is_the_unmarked_wire() {
    let mut req = compile(input(2)).request;
    req.cache_breakpoints.clear();
    let body = request_body(&req);
    assert!(body["system"].is_string(), "{}", body["system"]);
    assert!(!body.to_string().contains("cache_control"));
}
