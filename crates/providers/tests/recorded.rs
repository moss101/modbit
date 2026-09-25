//! Recorded safe fixtures for the two provider wires (REQ-EV-0211,
//! QUAL-EV-0211: "staging integration uses real test credential and
//! recorded safe fixture; mock-only cannot pass").
//!
//! The live half (`live_recorded_…`, run by `.github/workflows/live-providers.yml`
//! with the repository's provider secrets) drives the gateway's tool round
//! trip against the real endpoint through a recording proxy and writes what
//! crossed the wire — request bodies and response chunks, never a
//! credential — to `MODBIT_LIVE_RECORD_DIR`. A recording from a green run
//! is committed under `tests/fixtures/recorded/` with its run id.
//!
//! The offline half runs everywhere: each committed recording is served
//! back by a server that answers only the recorded requests, and the real
//! adapter must reproduce the round trip from it. The fixture therefore
//! pins what the real provider accepted and answered — a change to the
//! request body the adapter builds, or to how it reads the stream, fails
//! here until a new live run re-records it.

mod support;

use modbit_providers::{
    AuthScheme, ContentPart, Endpoint, Message, ModelCapability, ModelEvent, ModelPolicy,
    ModelRequest, ProviderGateway, ProviderKind, Requirements, Role, SecretHandle, ToolProjection,
};
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;

use support::{
    RECORD_DIR, SCHEMA, fixture_name, fixtures_dir, host_and_path, integration_of, live_model,
    replay_server, safety_findings, sha256_hex, write_result,
};

/// The live recording test's name: its result record's file name and the
/// `recorded_by` of every fixture it writes.
const LIVE_TEST: &str = "live_recorded_tool_round_trip_on_each_configured_wire";

fn turn(endpoint: &str, model: &str, messages: Vec<Message>) -> ModelRequest {
    ModelRequest {
        request_id: format!("req-{:08x}", rand::random::<u32>()),
        model_policy: ModelPolicy {
            endpoint: endpoint.into(),
            model: model.into(),
            reasoning_effort: None,
            service_tier: None,
        },
        messages,
        tool_projection: vec![ToolProjection {
            name: "fs.read".into(),
            description: "Read a file".into(),
            input_schema: json!({"type":"object","properties":{"path":{"type":"string"}},"required":["path"]}),
        }],
        response_format: None,
        cache_key: Some("stable-prefix-hash".into()),
        // Room for a model that reasons before it acts.
        max_output_tokens: 1024,
        timeout_ms: 120_000,
        policy_tags: vec!["tenant:test".into()],
    }
}

fn opening() -> Vec<Message> {
    vec![
        Message::text(
            Role::System,
            "Use the fs.read tool to read README.md, then answer with one word.",
        ),
        Message::text(Role::User, "Go."),
    ]
}

/// What one tool round trip produced.
struct RoundTrip {
    /// The model's tool call: `(call_id, name, arguments_json)`.
    call: (String, String, String),
    /// Text of the answer after the tool result.
    answer: String,
    /// Ids the provider put in its streams (`ProviderMetadata`), in order.
    stream_ids: Vec<String>,
    /// Models the provider reported, in order.
    resolved_models: Vec<String>,
}

async fn collect(mut s: modbit_providers::ModelStream) -> Vec<ModelEvent> {
    let mut out = Vec::new();
    while let Some(e) = s.events.recv().await {
        out.push(e);
    }
    out
}

fn metadata(events: &[ModelEvent], key: &str) -> Vec<String> {
    events
        .iter()
        .filter_map(|e| match e {
            ModelEvent::ProviderMetadata { metadata } => metadata[key].as_str().map(str::to_owned),
            _ => None,
        })
        .collect()
}

/// The gateway's tool round trip: the model calls `fs.read`, gets the file,
/// answers. Every check is on canonical events, so the live run and the
/// replay are judged by the same rule.
async fn tool_round_trip(gw: &ProviderGateway, ep: &str, model: &str) -> Result<RoundTrip, String> {
    let needs = Requirements {
        tools: true,
        ..Default::default()
    };
    let first = collect(
        gw.stream(turn(ep, model, opening()), &needs, CancellationToken::new())
            .map_err(|e| format!("route: {e:?}"))?,
    )
    .await;
    let call = first
        .iter()
        .find_map(|e| match e {
            ModelEvent::ToolCallComplete {
                call_id,
                name,
                arguments_json,
            } => Some((call_id.clone(), name.clone(), arguments_json.clone())),
            _ => None,
        })
        .ok_or_else(|| format!("first turn: no tool call: {first:?}"))?;
    if call.1 != "fs.read" {
        return Err(format!("first turn: called {:?}, not fs.read", call.1));
    }
    serde_json::from_str::<Value>(&call.2)
        .map_err(|e| format!("first turn: arguments are not JSON ({e}): {}", call.2))?;
    if !matches!(first.last(), Some(ModelEvent::Completed { .. })) {
        return Err(format!("first turn did not complete: {first:?}"));
    }
    let mut next = opening();
    next.push(Message {
        role: Role::Assistant,
        parts: vec![ContentPart::ToolCall {
            call_id: call.0.clone(),
            name: call.1.clone(),
            arguments_json: call.2.clone(),
        }],
    });
    next.push(Message {
        role: Role::Tool,
        parts: vec![ContentPart::ToolResult {
            call_id: call.0.clone(),
            content: "# demo".into(),
            is_error: false,
        }],
    });
    let second = collect(
        gw.stream(turn(ep, model, next), &needs, CancellationToken::new())
            .map_err(|e| format!("route: {e:?}"))?,
    )
    .await;
    let answer: String = second
        .iter()
        .filter_map(|e| match e {
            ModelEvent::MessageDelta { text } => Some(text.as_str()),
            _ => None,
        })
        .collect();
    if answer.trim().is_empty() || !matches!(second.last(), Some(ModelEvent::Completed { .. })) {
        return Err(format!("second turn: no completed answer: {second:?}"));
    }
    let both: Vec<ModelEvent> = first.into_iter().chain(second).collect();
    Ok(RoundTrip {
        call,
        answer,
        stream_ids: metadata(&both, "provider_request_id"),
        resolved_models: metadata(&both, "resolved_model"),
    })
}

/// Live half: the round trip against each configured real endpoint, through
/// the recording proxy. Leaves a `passed` result record naming what it
/// proved and the digest of every recording, or `skipped` when the live
/// switch is off — never a silent pass.
#[tokio::test]
async fn live_recorded_tool_round_trip_on_each_configured_wire() {
    if std::env::var("MODBIT_LIVE_PROVIDERS").as_deref() != Ok("1") {
        eprintln!("skipped: MODBIT_LIVE_PROVIDERS=1 not set");
        write_result(
            LIVE_TEST,
            "skipped",
            json!({"reason": "MODBIT_LIVE_PROVIDERS=1 not set"}),
        );
        return;
    }
    let endpoints = modbit_providers::endpoints_from_env();
    assert!(
        !endpoints.is_empty(),
        "MODBIT_LIVE_PROVIDERS=1 but no endpoint is configured (OPENAI_API_KEY / ANTHROPIC_API_KEY)"
    );
    let record_dir = std::env::var_os(RECORD_DIR)
        .filter(|d| !d.is_empty())
        .map(std::path::PathBuf::from);
    let mut proven = Vec::new();
    let mut recordings = Vec::new();
    for ep in endpoints {
        if matches!(ep.credential, SecretHandle::None) {
            eprintln!("live: endpoint `{}` has no credential; skipped", ep.name);
            continue;
        }
        let model = live_model(&ep);
        let proxy = support::recording_proxy(&ep.base_url).await;
        let mut via = ep.clone();
        via.base_url = proxy.base_url.clone();
        let gw = ProviderGateway::new(vec![via]);
        let rt = tool_round_trip(&gw, &ep.name, &model)
            .await
            .unwrap_or_else(|e| panic!("{}: {e}", ep.name));
        let exchanges = proxy.exchanges.lock().expect("exchanges").clone();
        assert_eq!(
            exchanges.len(),
            2,
            "{}: two exchanges crossed the proxy",
            ep.name
        );
        for ex in &exchanges {
            assert_eq!(ex["response"]["status"], 200, "{}: {ex}", ep.name);
        }
        let (host, base_path) = host_and_path(&ep.base_url);
        let wire = match ep.kind {
            ProviderKind::OpenAi => "openai",
            ProviderKind::Anthropic => "anthropic",
        };
        let fixture = json!({
            "schema": SCHEMA,
            "integration": integration_of(ep.kind),
            "wire": wire,
            "recorded_by": format!("crates/providers/tests/recorded.rs::{LIVE_TEST}"),
            "provenance": {
                "run": support::run_id(),
                "endpoint_host": host,
                "base_path": base_path,
                "model": model,
                "auth": match ep.auth { AuthScheme::Native => "native", AuthScheme::Bearer => "bearer" },
                "extra_body": Value::Object(ep.extra_body.clone()),
                "recorded_unix": std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs())
                    .unwrap_or_default(),
                "stream_ids": rt.stream_ids,
                "resolved_models": rt.resolved_models,
            },
            "exchanges": exchanges,
        });
        let held: Vec<String> = ep.credential.resolve().into_iter().collect();
        let findings = safety_findings(&fixture, &held);
        assert!(
            findings.is_empty(),
            "{}: the recording is not safe to keep: {findings:?}",
            ep.name
        );
        let mut bytes = serde_json::to_vec_pretty(&fixture).expect("json");
        bytes.push(b'\n');
        let digest = sha256_hex(&bytes);
        if let Some(dir) = &record_dir {
            std::fs::create_dir_all(dir).expect("record dir");
            std::fs::write(dir.join(fixture_name(ep.kind)), &bytes).expect("write recording");
        }
        eprintln!(
            "live: `{}` ({wire} wire) at {host}{base_path} model {model}: {}({}) then {:?}; recording sha256 {digest}",
            ep.name,
            rt.call.1,
            rt.call.2,
            rt.answer.trim()
        );
        proven.push(json!({
            "integration": integration_of(ep.kind),
            "endpoint": ep.name,
            "endpoint_host": host,
            "model": model,
            "stream_ids": fixture["provenance"]["stream_ids"],
        }));
        recordings.push(json!({
            "integration": integration_of(ep.kind),
            "file": fixture_name(ep.kind),
            "sha256": digest,
        }));
    }
    assert!(
        !proven.is_empty(),
        "MODBIT_LIVE_PROVIDERS=1 but no configured endpoint carries a credential"
    );
    write_result(
        LIVE_TEST,
        "passed",
        json!({"proven": proven, "recordings": recordings}),
    );
}

fn load(kind: ProviderKind) -> Value {
    let path = fixtures_dir().join(fixture_name(kind));
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!(
            "{}: {e} — record it with the live-providers workflow (MODBIT_LIVE_RECORD_DIR) and commit it",
            path.display()
        )
    });
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// An endpoint on the replay server shaped like the recorded one: same
/// family, auth scheme, extra body fields and model.
fn replay_endpoint(kind: ProviderKind, fixture: &Value, base_url: &str) -> Endpoint {
    let p = &fixture["provenance"];
    Endpoint {
        name: "replay".into(),
        kind,
        base_url: base_url.into(),
        // A value for the header, sent only to the local replay server.
        credential: SecretHandle::Inline("replay-without-a-credential".into()),
        models: vec![ModelCapability {
            model: p["model"].as_str().unwrap_or_default().into(),
            context_tokens: 128_000,
            max_output_tokens: 8192,
            tools: true,
            parallel_tools: true,
            vision: false,
            input_modalities: vec!["text".into()],
            reasoning: false,
            structured_output: true,
            agent_loop: true,
            input_price_per_mtok: 1.0,
            output_price_per_mtok: 2.0,
        }],
        max_retries: 0,
        auth: if p["auth"] == "bearer" {
            AuthScheme::Bearer
        } else {
            AuthScheme::Native
        },
        extra_body: p["extra_body"].as_object().cloned().unwrap_or_default(),
    }
}

async fn replay(kind: ProviderKind) {
    let fixture = load(kind);
    assert_eq!(fixture["schema"], SCHEMA);
    assert_eq!(fixture["integration"], integration_of(kind));
    let findings = safety_findings(&fixture, &[]);
    assert!(findings.is_empty(), "unsafe fixture: {findings:?}");
    let model = fixture["provenance"]["model"]
        .as_str()
        .unwrap_or_default()
        .to_owned();
    let server = replay_server(&fixture).await;
    let gw = ProviderGateway::new(vec![replay_endpoint(kind, &fixture, &server.base_url)]);
    let rt = tool_round_trip(&gw, "replay", &model)
        .await
        .unwrap_or_else(|e| {
            panic!(
                "the adapter no longer reproduces the recorded round trip: {e}; refused: {:?}",
                server.refused.lock().expect("refused")
            )
        });
    assert!(
        server.refused.lock().expect("refused").is_empty(),
        "every request the adapter sent is one the provider accepted: {:?}",
        server.refused.lock().expect("refused")
    );
    let recorded = fixture["exchanges"].as_array().map_or(0, Vec::len);
    assert_eq!(*server.served.lock().expect("served"), recorded);
    // The adapter reads the recorded stream the way it read the live one.
    let ids: Vec<String> = fixture["provenance"]["stream_ids"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|v| v.as_str().map(str::to_owned))
        .collect();
    assert_eq!(rt.stream_ids, ids, "provider ids read from the stream");
    let models: Vec<String> = fixture["provenance"]["resolved_models"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|v| v.as_str().map(str::to_owned))
        .collect();
    assert_eq!(rt.resolved_models, models, "models the provider reported");
    assert_eq!(rt.call.1, "fs.read");
    assert!(!rt.answer.trim().is_empty());
}

/// The OpenAI-protocol recording reproduces through the real adapter.
#[tokio::test]
async fn replay_recorded_openai_wire_tool_round_trip() {
    replay(ProviderKind::OpenAi).await;
}

/// The Anthropic-protocol recording reproduces through the real adapter.
#[tokio::test]
async fn replay_recorded_anthropic_wire_tool_round_trip() {
    replay(ProviderKind::Anthropic).await;
}

/// A fixture answers only what the real provider was asked: a request body
/// the recording does not hold is refused with the first difference, and
/// the adapter reports the refusal as the provider's.
#[tokio::test]
async fn replay_refuses_a_request_the_provider_never_accepted() {
    let fixture = load(ProviderKind::OpenAi);
    let model = fixture["provenance"]["model"]
        .as_str()
        .unwrap_or_default()
        .to_owned();
    let server = replay_server(&fixture).await;
    let gw = ProviderGateway::new(vec![replay_endpoint(
        ProviderKind::OpenAi,
        &fixture,
        &server.base_url,
    )]);
    let mut changed = opening();
    changed[0] = Message::text(Role::System, "Answer in one word. Do not use tools.");
    let events = collect(
        gw.stream(
            turn("replay", &model, changed),
            &Requirements {
                tools: true,
                ..Default::default()
            },
            CancellationToken::new(),
        )
        .expect("route"),
    )
    .await;
    let refused = server.refused.lock().expect("refused").clone();
    assert_eq!(refused.len(), 1, "{refused:?}");
    assert!(
        refused[0].contains("differs from the recording at /body/messages/0"),
        "{refused:?}"
    );
    assert!(
        events.iter().any(|e| matches!(
            e,
            ModelEvent::Error { code, message, .. }
                if code == "PROVIDER_REJECTED" && message.contains("differs from the recording")
        )),
        "{events:?}"
    );
    assert_eq!(*server.served.lock().expect("served"), 0);
}

/// Every committed recording names the run that made it, carries no
/// credential header and nothing the product's redactor would replace; and
/// the check has teeth — a leaked header, a key-shaped token or a held
/// value is found.
#[test]
fn recorded_fixtures_carry_provenance_and_no_credential() {
    let mut seen = Vec::new();
    for entry in std::fs::read_dir(fixtures_dir()).expect("fixtures dir") {
        let path = entry.expect("entry").path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let fixture: Value =
            serde_json::from_str(&std::fs::read_to_string(&path).expect("read")).expect("json");
        let name = path.display().to_string();
        assert_eq!(fixture["schema"], SCHEMA, "{name}");
        let p = &fixture["provenance"];
        let run = p["run"].as_str().unwrap_or_default();
        assert!(
            !run.is_empty() && run.chars().all(|c| c.is_ascii_digit()),
            "{name}: recorded by CI run {run:?}, not a local or hand-made file"
        );
        for key in ["endpoint_host", "model", "auth"] {
            assert!(
                p[key].as_str().is_some_and(|v| !v.is_empty()),
                "{name}: provenance.{key}"
            );
        }
        let exchanges = fixture["exchanges"].as_array().expect("exchanges");
        assert!(!exchanges.is_empty(), "{name}");
        for ex in exchanges {
            assert!(ex["request"]["body"].is_object(), "{name}: a request body");
            assert!(
                ex["response"]["chunks"]
                    .as_array()
                    .is_some_and(|c| !c.is_empty()),
                "{name}: response chunks"
            );
        }
        let findings = safety_findings(&fixture, &[]);
        assert!(findings.is_empty(), "{name}: {findings:?}");

        let mut leaked = fixture.clone();
        leaked["exchanges"][0]["request"]["headers"]["authorization"] =
            json!("Bearer sk-live0123456789abcdef0123456789");
        let found = safety_findings(&leaked, &[]);
        assert!(
            found.iter().any(|f| f.contains("credential header"))
                && found.iter().any(|f| f.contains("redactor")),
            "{name}: {found:?}"
        );
        let held = "held-provider-key-7f3a9c".to_owned();
        let mut echoed = fixture.clone();
        echoed["exchanges"][0]["response"]["chunks"][0]["text"] =
            json!(format!("echo {held} back"));
        assert!(!safety_findings(&echoed, &[held]).is_empty(), "{name}");
        seen.push(
            path.file_name()
                .and_then(|n| n.to_str())
                .unwrap_or_default()
                .to_owned(),
        );
    }
    seen.sort();
    assert_eq!(
        seen,
        [
            fixture_name(ProviderKind::Anthropic),
            fixture_name(ProviderKind::OpenAi)
        ],
        "one recording per wire"
    );
}
