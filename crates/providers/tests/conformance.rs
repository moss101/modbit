//! Provider conformance suite (docs/15 "Provider contract", "Health and
//! failover"): real HTTP over a real socket against wire-faithful local
//! OpenAI-compatible and Anthropic servers, with fault injection (rate limit,
//! stall, disconnect, malformed frames), plus the same scenarios against the
//! production endpoints when `MODBIT_LIVE_PROVIDERS=1` and credentials exist.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use modbit_providers::{
    ContentPart, Endpoint, Message, ModelCapability, ModelEvent, ModelPolicy, ModelRequest,
    ProviderGateway, ProviderKind, Requirements, Role, RouteError, SecretHandle, ToolProjection,
    stop,
};
use serde_json::{Value, json};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;

async fn write_frames(
    sock: &mut tokio::net::TcpStream,
    frames: Vec<(Option<&'static str>, String, u64)>,
) -> bool {
    let _ = sock
        .write_all(b"HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ntransfer-encoding: chunked\r\n\r\n")
        .await;
    for (event, data, delay) in frames {
        tokio::time::sleep(Duration::from_millis(delay)).await;
        let mut frame = String::new();
        if let Some(e) = event {
            frame.push_str(&format!("event: {e}\n"));
        }
        frame.push_str(&format!("data: {data}\n\n"));
        let chunk = format!("{:x}\r\n{}\r\n", frame.len(), frame);
        if sock.write_all(chunk.as_bytes()).await.is_err() {
            return false;
        }
    }
    true
}

/// One scripted response.
#[derive(Clone)]
enum Script {
    /// Status + JSON body.
    Status(u16, String),
    /// SSE frames `(event, data)` with a delay before each.
    Sse(Vec<(Option<&'static str>, String, u64)>),
    /// SSE frames, then close the socket without finishing.
    SseThenDrop(Vec<(Option<&'static str>, String, u64)>),
    /// Never answer.
    Stall,
}

#[derive(Clone, Debug)]
struct Seen {
    path: String,
    headers: Vec<(String, String)>,
    body: Value,
}

struct FakeProvider {
    base_url: String,
    seen: Arc<Mutex<Vec<Seen>>>,
    /// Set when a client dropped the connection while the server was still streaming.
    client_gone: Arc<Mutex<bool>>,
}

async fn fake(scripts: Vec<Script>) -> FakeProvider {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let seen = Arc::new(Mutex::new(Vec::new()));
    let client_gone = Arc::new(Mutex::new(false));
    let scripts = Arc::new(Mutex::new(scripts.into_iter()));
    let (s2, g2) = (Arc::clone(&seen), Arc::clone(&client_gone));
    tokio::spawn(async move {
        loop {
            let Ok((mut sock, _)) = listener.accept().await else {
                return;
            };
            let script = scripts.lock().unwrap().next();
            let seen = Arc::clone(&s2);
            let gone = Arc::clone(&g2);
            tokio::spawn(async move {
                // Read the request head and body.
                let mut buf = Vec::new();
                let mut tmp = [0u8; 4096];
                let (head_end, body_len) = loop {
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
                                (k.eq_ignore_ascii_case("content-length"))
                                    .then(|| v.trim().parse::<usize>().ok())
                                    .flatten()
                            })
                            .unwrap_or(0);
                        break (i + 4, len);
                    }
                };
                while buf.len() < head_end + body_len {
                    let n = sock.read(&mut tmp).await.unwrap_or(0);
                    if n == 0 {
                        break;
                    }
                    buf.extend_from_slice(&tmp[..n]);
                }
                let head = String::from_utf8_lossy(&buf[..head_end]).to_string();
                let mut lines = head.lines();
                let path = lines
                    .next()
                    .unwrap_or_default()
                    .split(' ')
                    .nth(1)
                    .unwrap_or_default()
                    .to_owned();
                let headers = lines
                    .filter_map(|l| l.split_once(':'))
                    .map(|(k, v)| (k.to_ascii_lowercase(), v.trim().to_owned()))
                    .collect();
                let body: Value = serde_json::from_slice(&buf[head_end..head_end + body_len])
                    .unwrap_or(Value::Null);
                seen.lock().unwrap().push(Seen {
                    path,
                    headers,
                    body,
                });
                match script {
                    None | Some(Script::Stall) => {
                        tokio::time::sleep(Duration::from_secs(600)).await;
                    }
                    Some(Script::Status(code, body)) => {
                        let _ = sock
                            .write_all(
                                format!(
                                    "HTTP/1.1 {code} X\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\r\n{body}",
                                    body.len()
                                )
                                .as_bytes(),
                            )
                            .await;
                    }
                    Some(Script::Sse(frames)) => {
                        let ok = write_frames(&mut sock, frames).await;
                        if !ok {
                            *gone.lock().unwrap() = true;
                        }
                        let _ = sock.write_all(b"0\r\n\r\n").await;
                    }
                    Some(Script::SseThenDrop(frames)) => {
                        let _ = write_frames(&mut sock, frames).await;
                        drop(sock);
                    }
                }
            });
        }
    });
    FakeProvider {
        base_url: format!("http://127.0.0.1:{port}"),
        seen,
        client_gone,
    }
}

fn model(name: &str, tools: bool) -> ModelCapability {
    ModelCapability {
        model: name.into(),
        context_tokens: 128_000,
        max_output_tokens: 8192,
        tools,
        parallel_tools: tools,
        vision: false,
        input_modalities: vec!["text".into()],
        reasoning: false,
        structured_output: true,
        agent_loop: tools,
        input_price_per_mtok: 1.0,
        output_price_per_mtok: 2.0,
    }
}

fn endpoint(
    name: &str,
    kind: ProviderKind,
    base: &str,
    cred: SecretHandle,
    retries: u32,
) -> Endpoint {
    Endpoint {
        name: name.into(),
        kind,
        base_url: base.into(),
        credential: cred,
        models: vec![model("m-tools", true), model("m-plain", false)],
        max_retries: retries,
    }
}

fn request(
    endpoint: &str,
    model: &str,
    messages: Vec<Message>,
    tools: bool,
    timeout_ms: u64,
) -> ModelRequest {
    ModelRequest {
        request_id: format!("req-{}", rand_id()),
        model_policy: ModelPolicy {
            endpoint: endpoint.into(),
            model: model.into(),
            reasoning_effort: None,
            service_tier: None,
        },
        messages,
        tool_projection: if tools {
            vec![ToolProjection {
                name: "fs.read".into(),
                description: "Read a file".into(),
                input_schema: json!({"type":"object","properties":{"path":{"type":"string"}},"required":["path"]}),
            }]
        } else {
            vec![]
        },
        response_format: None,
        cache_key: Some("stable-prefix-hash".into()),
        max_output_tokens: 256,
        timeout_ms,
        policy_tags: vec!["tenant:test".into()],
    }
}

fn rand_id() -> String {
    format!("{:08x}", rand::random::<u32>())
}

async fn collect(mut s: modbit_providers::ModelStream) -> Vec<ModelEvent> {
    let mut out = Vec::new();
    while let Some(e) = s.events.recv().await {
        out.push(e);
    }
    out
}

fn text_of(events: &[ModelEvent]) -> String {
    events
        .iter()
        .filter_map(|e| match e {
            ModelEvent::MessageDelta { text } => Some(text.as_str()),
            _ => None,
        })
        .collect()
}

fn openai_tool_stream() -> Vec<(Option<&'static str>, String, u64)> {
    let f = |d: Value| (None, d.to_string(), 1);
    vec![
        f(
            json!({"id":"chatcmpl-1","model":"m-tools-2026","service_tier":"default","choices":[{"index":0,"delta":{"role":"assistant","content":""},"finish_reason":null}]}),
        ),
        f(
            json!({"id":"chatcmpl-1","model":"m-tools-2026","choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"id":"call_1","type":"function","function":{"name":"fs.read","arguments":""}}]},"finish_reason":null}]}),
        ),
        f(
            json!({"id":"chatcmpl-1","model":"m-tools-2026","choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"function":{"arguments":"{\"pa"}}]},"finish_reason":null}]}),
        ),
        f(
            json!({"id":"chatcmpl-1","model":"m-tools-2026","choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"function":{"arguments":"th\":\"README.md\"}"}}]},"finish_reason":null}]}),
        ),
        f(
            json!({"id":"chatcmpl-1","model":"m-tools-2026","choices":[{"index":0,"delta":{},"finish_reason":"tool_calls"}]}),
        ),
        f(
            json!({"id":"chatcmpl-1","model":"m-tools-2026","choices":[],"usage":{"prompt_tokens":40,"completion_tokens":9,"prompt_tokens_details":{"cached_tokens":32}}}),
        ),
        (None, "[DONE]".into(), 1),
    ]
}

fn openai_text_stream(text: &[&str]) -> Vec<(Option<&'static str>, String, u64)> {
    let mut v: Vec<(Option<&'static str>, String, u64)> = text
        .iter()
        .map(|t| (None, json!({"id":"chatcmpl-2","model":"m-tools-2026","choices":[{"index":0,"delta":{"content":t},"finish_reason":null}]}).to_string(), 1))
        .collect();
    v.push((None, json!({"id":"chatcmpl-2","model":"m-tools-2026","choices":[{"index":0,"delta":{},"finish_reason":"stop"}],"usage":{"prompt_tokens":60,"completion_tokens":4}}).to_string(), 1));
    v.push((None, "[DONE]".into(), 1));
    v
}

fn anthropic_tool_stream() -> Vec<(Option<&'static str>, String, u64)> {
    vec![
        (Some("message_start"), json!({"type":"message_start","message":{"id":"msg_1","model":"m-tools-2026","usage":{"input_tokens":50,"cache_read_input_tokens":40,"output_tokens":1}}}).to_string(), 1),
        (Some("content_block_start"), json!({"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}).to_string(), 1),
        (Some("content_block_delta"), json!({"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Reading."}}).to_string(), 1),
        (Some("content_block_stop"), json!({"type":"content_block_stop","index":0}).to_string(), 1),
        (Some("content_block_start"), json!({"type":"content_block_start","index":1,"content_block":{"type":"tool_use","id":"toolu_1","name":"fs.read","input":{}}}).to_string(), 1),
        (Some("content_block_delta"), json!({"type":"content_block_delta","index":1,"delta":{"type":"input_json_delta","partial_json":"{\"path\": "}}).to_string(), 1),
        (Some("content_block_delta"), json!({"type":"content_block_delta","index":1,"delta":{"type":"input_json_delta","partial_json":"\"README.md\"}"}}).to_string(), 1),
        (Some("content_block_stop"), json!({"type":"content_block_stop","index":1}).to_string(), 1),
        (Some("message_delta"), json!({"type":"message_delta","delta":{"stop_reason":"tool_use"},"usage":{"output_tokens":12}}).to_string(), 1),
        (Some("message_stop"), json!({"type":"message_stop"}).to_string(), 1),
    ]
}

fn anthropic_text_stream(text: &str) -> Vec<(Option<&'static str>, String, u64)> {
    vec![
        (Some("message_start"), json!({"type":"message_start","message":{"id":"msg_2","model":"m-tools-2026","usage":{"input_tokens":70,"output_tokens":1}}}).to_string(), 1),
        (Some("ping"), json!({"type":"ping"}).to_string(), 1),
        (Some("content_block_start"), json!({"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}).to_string(), 1),
        (Some("content_block_delta"), json!({"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":text}}).to_string(), 1),
        (Some("content_block_stop"), json!({"type":"content_block_stop","index":0}).to_string(), 1),
        (Some("message_delta"), json!({"type":"message_delta","delta":{"stop_reason":"end_turn"},"usage":{"output_tokens":5}}).to_string(), 1),
        (Some("message_stop"), json!({"type":"message_stop"}).to_string(), 1),
    ]
}

/// Drives a full tool round trip on either wire and checks the request bodies.
async fn tool_round_trip(kind: ProviderKind, scripts: Vec<Script>) {
    let server = fake(scripts).await;
    let gw = ProviderGateway::new(vec![endpoint(
        "ep",
        kind,
        &server.base_url,
        SecretHandle::Inline("sk-test-secret-value-1234567890".into()),
        2,
    )]);
    let messages = vec![
        Message::text(Role::System, "You are a coding agent."),
        Message::text(Role::User, "What does README say?"),
    ];
    let s = gw
        .stream(
            request("ep", "m-tools", messages.clone(), true, 5_000),
            &Requirements {
                tools: true,
                ..Default::default()
            },
            CancellationToken::new(),
        )
        .unwrap();
    let route = Arc::clone(&s.route);
    let events = collect(s).await;
    let complete = events.iter().find_map(|e| match e {
        ModelEvent::ToolCallComplete {
            call_id,
            name,
            arguments_json,
        } => Some((call_id.clone(), name.clone(), arguments_json.clone())),
        _ => None,
    });
    let (call_id, name, args) = complete.expect("tool call completed");
    assert_eq!(name, "fs.read");
    assert_eq!(
        serde_json::from_str::<Value>(&args).unwrap()["path"],
        "README.md"
    );
    assert!(
        events
            .iter()
            .any(|e| matches!(e, ModelEvent::ToolCallStart { name, .. } if name == "fs.read"))
    );
    assert!(
        events
            .iter()
            .any(|e| matches!(e, ModelEvent::ToolCallDelta { .. }))
    );
    assert!(
        matches!(events.last(), Some(ModelEvent::Completed { stop_reason }) if stop_reason == stop::TOOL_USE),
        "{events:?}"
    );
    let usage = events
        .iter()
        .rev()
        .find_map(|e| match e {
            ModelEvent::Usage { usage } => Some(usage.clone()),
            _ => None,
        })
        .unwrap();
    assert!(
        usage.input_tokens > 0 && usage.output_tokens > 0 && usage.cached_input_tokens > 0,
        "{usage:?}"
    );
    let r = route.lock().unwrap().clone();
    assert_eq!(
        (r.requested_model.as_str(), r.resolved_model.as_deref()),
        ("m-tools", Some("m-tools-2026")),
        "REQ-EV-0112: requested vs resolved"
    );
    assert_eq!(r.retries, 0);
    // Second leg: the tool result goes back with the same call identity.
    let mut next = messages;
    next.push(Message {
        role: Role::Assistant,
        parts: vec![ContentPart::ToolCall {
            call_id: call_id.clone(),
            name: "fs.read".into(),
            arguments_json: args,
        }],
    });
    next.push(Message {
        role: Role::Tool,
        parts: vec![ContentPart::ToolResult {
            call_id: call_id.clone(),
            content: "# demo".into(),
            is_error: false,
        }],
    });
    let s = gw
        .stream(
            request("ep", "m-tools", next, true, 5_000),
            &Requirements {
                tools: true,
                ..Default::default()
            },
            CancellationToken::new(),
        )
        .unwrap();
    let events = collect(s).await;
    assert_eq!(text_of(&events), "It says demo.");
    assert!(
        matches!(events.last(), Some(ModelEvent::Completed { stop_reason }) if stop_reason == stop::END_TURN)
    );
    // Wire checks.
    let seen = server.seen.lock().unwrap().clone();
    assert_eq!(seen.len(), 2);
    let auth = |s: &Seen, k: &str| {
        s.headers
            .iter()
            .find(|(h, _)| h == k)
            .map(|(_, v)| v.clone())
    };
    match kind {
        ProviderKind::OpenAi => {
            assert_eq!(seen[0].path, "/v1/chat/completions");
            assert_eq!(
                auth(&seen[0], "authorization").as_deref(),
                Some("Bearer sk-test-secret-value-1234567890")
            );
            assert_eq!(seen[0].body["stream"], true);
            assert_eq!(seen[0].body["tools"][0]["function"]["name"], "fs.read");
            assert_eq!(seen[0].body["messages"][0]["role"], "system");
            let m = &seen[1].body["messages"];
            assert_eq!(m[2]["tool_calls"][0]["id"], call_id);
            assert_eq!(
                (m[3]["role"].as_str(), m[3]["tool_call_id"].as_str()),
                (Some("tool"), Some(call_id.as_str()))
            );
        }
        ProviderKind::Anthropic => {
            assert_eq!(seen[0].path, "/v1/messages");
            assert_eq!(
                auth(&seen[0], "x-api-key").as_deref(),
                Some("sk-test-secret-value-1234567890")
            );
            assert!(auth(&seen[0], "anthropic-version").is_some());
            assert_eq!(seen[0].body["system"], "You are a coding agent.");
            assert_eq!(seen[0].body["tools"][0]["name"], "fs.read");
            let m = &seen[1].body["messages"];
            assert_eq!(m[1]["content"][0]["id"], call_id);
            assert_eq!(
                (
                    m[2]["role"].as_str(),
                    m[2]["content"][0]["type"].as_str(),
                    m[2]["content"][0]["tool_use_id"].as_str()
                ),
                (Some("user"), Some("tool_result"), Some(call_id.as_str()))
            );
        }
    }
    // The credential never appears in events or route records.
    let dump = format!("{events:?}{r:?}{:?}", gw.endpoints());
    assert!(
        !dump.contains("sk-test-secret"),
        "credential leaked: {dump}"
    );
    assert!(dump.contains("SecretHandle::Inline(<redacted>)"));
    let h = gw.health("ep");
    assert_eq!((h.requests, h.successes, h.failures), (2, 2, 0));
    assert!(h.last_first_token_ms.is_some());
}

#[tokio::test]
async fn openai_streaming_tool_round_trip_over_real_http() {
    tool_round_trip(
        ProviderKind::OpenAi,
        vec![
            Script::Sse(openai_tool_stream()),
            Script::Sse(openai_text_stream(&["It says", " demo."])),
        ],
    )
    .await;
}

#[tokio::test]
async fn anthropic_streaming_tool_round_trip_over_real_http() {
    tool_round_trip(
        ProviderKind::Anthropic,
        vec![
            Script::Sse(anthropic_tool_stream()),
            Script::Sse(anthropic_text_stream("It says demo.")),
        ],
    )
    .await;
}

/// docs/15 "Health and failover": bounded retry with jitter, charged to the
/// same request; exhaustion is a typed retryable error.
#[tokio::test]
async fn rate_limit_is_retried_within_bounds_and_then_reported() {
    let server = fake(vec![
        Script::Status(429, json!({"error":{"message":"slow down"}}).to_string()),
        Script::Status(503, "unavailable".into()),
        Script::Sse(openai_text_stream(&["ok"])),
        Script::Status(429, "{}".into()),
        Script::Status(429, "{}".into()),
    ])
    .await;
    let gw = ProviderGateway::new(vec![endpoint(
        "ep",
        ProviderKind::OpenAi,
        &server.base_url,
        SecretHandle::None,
        2,
    )]);
    let s = gw
        .stream(
            request(
                "ep",
                "m-plain",
                vec![Message::text(Role::User, "hi")],
                false,
                10_000,
            ),
            &Requirements::default(),
            CancellationToken::new(),
        )
        .unwrap();
    let route = Arc::clone(&s.route);
    let events = collect(s).await;
    assert_eq!(text_of(&events), "ok");
    assert_eq!(route.lock().unwrap().retries, 2);
    // Exhaustion: one attempt + one retry, then a retryable error.
    let gw1 = ProviderGateway::new(vec![endpoint(
        "ep",
        ProviderKind::OpenAi,
        &server.base_url,
        SecretHandle::None,
        1,
    )]);
    let s = gw1
        .stream(
            request(
                "ep",
                "m-plain",
                vec![Message::text(Role::User, "hi")],
                false,
                10_000,
            ),
            &Requirements::default(),
            CancellationToken::new(),
        )
        .unwrap();
    let events = collect(s).await;
    assert!(
        matches!(events.last(), Some(ModelEvent::Error { code, retryable: true, .. }) if code == "RATE_LIMITED"),
        "{events:?}"
    );
    assert_eq!(server.seen.lock().unwrap().len(), 5);
    assert_eq!(gw.health("ep").rate_limited, 1);
    assert_eq!(gw1.health("ep").rate_limited, 2);
}

#[tokio::test]
async fn timeout_and_cancellation_end_the_stream_and_release_the_connection() {
    // Timeout: the provider never answers.
    let server = fake(vec![Script::Stall]).await;
    let gw = ProviderGateway::new(vec![endpoint(
        "ep",
        ProviderKind::Anthropic,
        &server.base_url,
        SecretHandle::None,
        0,
    )]);
    let t0 = Instant::now();
    let s = gw
        .stream(
            request(
                "ep",
                "m-plain",
                vec![Message::text(Role::User, "hi")],
                false,
                400,
            ),
            &Requirements::default(),
            CancellationToken::new(),
        )
        .unwrap();
    let events = collect(s).await;
    assert!(
        matches!(events.last(), Some(ModelEvent::Error { code, .. }) if code == "TIMEOUT"),
        "{events:?}"
    );
    assert!(t0.elapsed() < Duration::from_secs(5));
    assert_eq!(gw.health("ep").interruptions, 1);
    // Cancellation mid-stream: first delta arrives, then the caller cancels; the
    // server sees the connection go away.
    let mut frames = anthropic_text_stream("partial");
    frames.push((
        Some("content_block_delta"),
        json!({"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"late"}})
            .to_string(),
        3_000,
    ));
    // Reorder: keep message_stop last but make the late frame stall before it.
    let stop_frame = frames.remove(6);
    frames.push(stop_frame);
    let server = fake(vec![Script::Sse(frames)]).await;
    let gw = ProviderGateway::new(vec![endpoint(
        "ep",
        ProviderKind::Anthropic,
        &server.base_url,
        SecretHandle::None,
        0,
    )]);
    let cancel = CancellationToken::new();
    let mut s = gw
        .stream(
            request(
                "ep",
                "m-plain",
                vec![Message::text(Role::User, "hi")],
                false,
                30_000,
            ),
            &Requirements::default(),
            cancel.clone(),
        )
        .unwrap();
    let mut got_text = false;
    let mut last = None;
    while let Some(e) = s.events.recv().await {
        if matches!(e, ModelEvent::MessageDelta { .. }) && !got_text {
            got_text = true;
            cancel.cancel();
        }
        last = Some(e);
    }
    assert!(got_text);
    assert!(
        matches!(last, Some(ModelEvent::Completed { ref stop_reason }) if stop_reason == stop::CANCELLED),
        "{last:?}"
    );
    tokio::time::sleep(Duration::from_millis(3_500)).await;
    assert!(
        *server.client_gone.lock().unwrap(),
        "server should observe the dropped connection"
    );
    assert_eq!(gw.health("ep").cancellations, 1);
}

/// After the first token nothing is replayed: an interruption is typed, not retried.
#[tokio::test]
async fn interruption_after_first_token_is_reported_not_retried() {
    let server = fake(vec![
        Script::SseThenDrop(
            openai_text_stream(&["par", "tial"])
                .into_iter()
                .take(2)
                .collect(),
        ),
        Script::Sse(openai_text_stream(&["never"])),
    ])
    .await;
    let gw = ProviderGateway::new(vec![endpoint(
        "ep",
        ProviderKind::OpenAi,
        &server.base_url,
        SecretHandle::None,
        3,
    )]);
    let s = gw
        .stream(
            request(
                "ep",
                "m-plain",
                vec![Message::text(Role::User, "hi")],
                false,
                5_000,
            ),
            &Requirements::default(),
            CancellationToken::new(),
        )
        .unwrap();
    let events = collect(s).await;
    assert_eq!(text_of(&events), "partial");
    assert!(
        matches!(events.last(), Some(ModelEvent::Error { code, retryable: false, .. }) if code == "STREAM_INTERRUPTED"),
        "{events:?}"
    );
    assert_eq!(
        server.seen.lock().unwrap().len(),
        1,
        "no replay after the first token"
    );
    // Malformed frames are typed too.
    let server = fake(vec![Script::Sse(vec![(None, "{not json".into(), 1)])]).await;
    let gw = ProviderGateway::new(vec![endpoint(
        "ep",
        ProviderKind::OpenAi,
        &server.base_url,
        SecretHandle::None,
        0,
    )]);
    let s = gw
        .stream(
            request(
                "ep",
                "m-plain",
                vec![Message::text(Role::User, "hi")],
                false,
                5_000,
            ),
            &Requirements::default(),
            CancellationToken::new(),
        )
        .unwrap();
    let events = collect(s).await;
    assert!(
        matches!(events.last(), Some(ModelEvent::Error { code, .. }) if code == "MALFORMED_STREAM"),
        "{events:?}"
    );
    // Authentication rejection is final.
    let server = fake(vec![Script::Status(
        401,
        json!({"error":{"message":"bad key"}}).to_string(),
    )])
    .await;
    let gw = ProviderGateway::new(vec![endpoint(
        "ep",
        ProviderKind::Anthropic,
        &server.base_url,
        SecretHandle::None,
        3,
    )]);
    let s = gw
        .stream(
            request(
                "ep",
                "m-plain",
                vec![Message::text(Role::User, "hi")],
                false,
                5_000,
            ),
            &Requirements::default(),
            CancellationToken::new(),
        )
        .unwrap();
    let events = collect(s).await;
    assert!(
        matches!(events.last(), Some(ModelEvent::Error { code, retryable: false, .. }) if code == "AUTH_REJECTED"),
        "{events:?}"
    );
    assert_eq!(server.seen.lock().unwrap().len(), 1);
}

/// QUAL-EV-0028 / QUAL-EV-0189: capability mismatch routes away before any
/// network call; missing credentials are typed.
#[tokio::test]
async fn qual_ev_0028_0189_capability_catalog_refuses_mismatched_models_before_dispatch() {
    let server = fake(vec![]).await;
    let gw = ProviderGateway::new(vec![
        endpoint(
            "ep",
            ProviderKind::OpenAi,
            &server.base_url,
            SecretHandle::None,
            0,
        ),
        endpoint(
            "locked",
            ProviderKind::OpenAi,
            &server.base_url,
            SecretHandle::Env("MODBIT_DEFINITELY_UNSET_KEY".into()),
            0,
        ),
    ]);
    let tools_req = request(
        "ep",
        "m-plain",
        vec![Message::text(Role::User, "hi")],
        true,
        1_000,
    );
    let err = gw
        .route(
            &tools_req,
            &Requirements {
                tools: true,
                ..Default::default()
            },
        )
        .unwrap_err();
    assert_eq!(
        err,
        RouteError::CapabilityMismatch {
            model: "m-plain".into(),
            capability: "tools".into()
        }
    );
    let vision = gw
        .route(
            &request("ep", "m-tools", vec![], false, 1_000),
            &Requirements {
                vision: true,
                input_modalities: vec!["image".into()],
                ..Default::default()
            },
        )
        .unwrap_err();
    assert_eq!(
        vision,
        RouteError::CapabilityMismatch {
            model: "m-tools".into(),
            capability: "vision".into()
        }
    );
    let mut reasoning = request("ep", "m-tools", vec![], false, 1_000);
    reasoning.model_policy.reasoning_effort = Some("high".into());
    assert!(
        matches!(gw.route(&reasoning, &Requirements::default()), Err(RouteError::CapabilityMismatch { capability, .. }) if capability == "reasoning")
    );
    assert_eq!(
        gw.route(
            &request("nope", "m-tools", vec![], false, 1_000),
            &Requirements::default()
        )
        .unwrap_err(),
        RouteError::UnknownEndpoint("nope".into())
    );
    assert_eq!(
        gw.route(
            &request("ep", "m-x", vec![], false, 1_000),
            &Requirements::default()
        )
        .unwrap_err(),
        RouteError::UnknownModel {
            endpoint: "ep".into(),
            model: "m-x".into()
        }
    );
    assert_eq!(
        gw.route(
            &request("locked", "m-tools", vec![], false, 1_000),
            &Requirements::default()
        )
        .unwrap_err(),
        RouteError::MissingCredential("locked".into())
    );
    // The eligible endpoint routes.
    let (ep, cap, rec) = gw
        .route(
            &request("ep", "m-tools", vec![], true, 1_000),
            &Requirements {
                tools: true,
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(
        (
            ep.name.as_str(),
            cap.model.as_str(),
            rec.requested_model.as_str()
        ),
        ("ep", "m-tools", "m-tools")
    );
    assert!(
        server.seen.lock().unwrap().is_empty(),
        "routing makes no network calls"
    );
}

/// Live proof (docs/15 "Live provider proof"): runs only with
/// `MODBIT_LIVE_PROVIDERS=1` and real credentials; otherwise it is skipped
/// and the deferral is tracked by DR-M2-001.
#[tokio::test]
async fn live_streaming_tool_round_trip_and_cancellation_against_production_endpoints() {
    if std::env::var("MODBIT_LIVE_PROVIDERS").as_deref() != Ok("1") {
        eprintln!("skipped: MODBIT_LIVE_PROVIDERS=1 not set");
        return;
    }
    let gw = ProviderGateway::new(modbit_providers::endpoints_from_env());
    for ep in gw.endpoints() {
        let model = ep
            .models
            .iter()
            .find(|m| m.model.contains("mini") || m.model.contains("haiku"))
            .map(|m| m.model.clone())
            .unwrap();
        let msgs = vec![
            Message::text(
                Role::System,
                "Use the fs.read tool to read README.md, then answer with one word.",
            ),
            Message::text(Role::User, "Go."),
        ];
        let s = gw
            .stream(
                request(&ep.name, &model, msgs.clone(), true, 60_000),
                &Requirements {
                    tools: true,
                    ..Default::default()
                },
                CancellationToken::new(),
            )
            .unwrap();
        let events = collect(s).await;
        let call = events
            .iter()
            .find_map(|e| match e {
                ModelEvent::ToolCallComplete {
                    call_id,
                    name,
                    arguments_json,
                } => Some((call_id.clone(), name.clone(), arguments_json.clone())),
                _ => None,
            })
            .unwrap_or_else(|| panic!("{}: no tool call: {events:?}", ep.name));
        assert_eq!(call.1, "fs.read");
        let mut next = msgs;
        next.push(Message {
            role: Role::Assistant,
            parts: vec![ContentPart::ToolCall {
                call_id: call.0.clone(),
                name: call.1,
                arguments_json: call.2,
            }],
        });
        next.push(Message {
            role: Role::Tool,
            parts: vec![ContentPart::ToolResult {
                call_id: call.0,
                content: "# demo".into(),
                is_error: false,
            }],
        });
        let s = gw
            .stream(
                request(&ep.name, &model, next, true, 60_000),
                &Requirements {
                    tools: true,
                    ..Default::default()
                },
                CancellationToken::new(),
            )
            .unwrap();
        let events = collect(s).await;
        assert!(!text_of(&events).is_empty(), "{}: {events:?}", ep.name);
        assert!(matches!(events.last(), Some(ModelEvent::Completed { .. })));
        let cancel = CancellationToken::new();
        let mut s = gw
            .stream(
                request(
                    &ep.name,
                    &model,
                    vec![Message::text(
                        Role::User,
                        "Count slowly from 1 to 200, one number per line.",
                    )],
                    false,
                    60_000,
                ),
                &Requirements::default(),
                cancel.clone(),
            )
            .unwrap();
        let mut last = None;
        while let Some(e) = s.events.recv().await {
            if matches!(e, ModelEvent::MessageDelta { .. }) {
                cancel.cancel();
            }
            last = Some(e);
        }
        assert!(
            matches!(last, Some(ModelEvent::Completed { ref stop_reason }) if stop_reason == stop::CANCELLED)
        );
    }
}

/// QUAL-EV-0112: the routing record shows requested vs resolved model, effort
/// and tier plus the policy reason, and it is provider-neutral.
#[tokio::test]
async fn qual_ev_0112_route_record_shows_requested_vs_resolved_values_and_policy_reason() {
    let server = fake(vec![Script::Sse(openai_text_stream(&["ok"]))]).await;
    let mut ep = endpoint(
        "ep",
        ProviderKind::OpenAi,
        &server.base_url,
        SecretHandle::None,
        0,
    );
    ep.models[0].reasoning = true;
    let gw = ProviderGateway::new(vec![ep]);
    let mut req = request(
        "ep",
        "m-tools",
        vec![Message::text(Role::User, "hi")],
        false,
        5_000,
    );
    req.model_policy.reasoning_effort = Some("high".into());
    req.model_policy.service_tier = Some("priority".into());
    let s = gw
        .stream(req, &Requirements::default(), CancellationToken::new())
        .unwrap();
    let route = Arc::clone(&s.route);
    let _ = collect(s).await;
    let r = route.lock().unwrap().clone();
    assert_eq!(r.requested_model, "m-tools");
    assert_eq!(
        r.resolved_model.as_deref(),
        Some("m-tools-2026"),
        "resolved from provider metadata"
    );
    assert_eq!(r.requested_reasoning_effort.as_deref(), Some("high"));
    assert_eq!(r.requested_service_tier.as_deref(), Some("priority"));
    // The fake stream reports no tier: the record says "unknown" rather than echoing the request.
    assert_eq!(r.resolved_service_tier, None);
    assert!(
        r.reason.contains("policy endpoint `ep` serves `m-tools`"),
        "{}",
        r.reason
    );
    assert_eq!(r.kind, ProviderKind::OpenAi);
    // The wire carried the requested envelope; the record is what the runtime logs (ModelUsageRecorded.route).
    let seen = server.seen.lock().unwrap().clone();
    assert_eq!(seen[0].body["reasoning_effort"], "high");
    assert_eq!(seen[0].body["service_tier"], "priority");
    let json = serde_json::to_value(&r).unwrap();
    assert!(json.get("requested_model").is_some() && json.get("resolved_model").is_some());
}
