//! PX-171 (QUAL-PX-171) at the gateway: the real `ProviderGateway` over
//! real HTTP against the shared fault-injecting server
//! (`tests/support/fault_provider.rs`), every case on both wire formats. A
//! rate limit with either retry header is waited and retried before the
//! first token; a body cut after the first token is a typed failure, never
//! retried, with no tool call; a cut before any event is retried; an idle
//! stall times out on schedule; a malformed event is refused.
//!
//! These prove the gateway's behaviour on the wire, not what a live
//! provider sends.

#[path = "../../../tests/support/fault_provider.rs"]
mod fault_provider;

use std::time::{Duration, Instant};

use fault_provider::{Answer, Fault, FaultServer, Wire, fault_provider};
use modbit_providers::{
    AuthScheme, Endpoint, Message, ModelCapability, ModelEvent, ModelPolicy, ModelRequest,
    ProviderGateway, ProviderKind, Requirements, Role, SecretHandle,
};
use serde_json::json;
use tokio_util::sync::CancellationToken;

const WIRES: [Wire; 2] = [Wire::OpenAi, Wire::Anthropic];

fn gateway(server: &FaultServer, wire: Wire) -> ProviderGateway {
    ProviderGateway::new(vec![Endpoint {
        name: "ep".into(),
        kind: match wire {
            Wire::OpenAi => ProviderKind::OpenAi,
            Wire::Anthropic => ProviderKind::Anthropic,
        },
        base_url: server.base_url.clone(),
        credential: SecretHandle::None,
        models: vec![ModelCapability {
            model: "m".into(),
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
            output_budget_tokens: 0,
            request_timeout_ms: 0,
            default_reasoning_effort: None,
            default_service_tier: None,
            projection_mode: None,
            max_projection_bytes: 0,
        }],
        max_retries: 2,
        auth: AuthScheme::Native,
        extra_body: Default::default(),
        max_concurrency: 0,
    }])
}

async fn run(gw: &ProviderGateway, timeout_ms: u64) -> (Vec<ModelEvent>, Duration) {
    let started = Instant::now();
    let mut stream = gw
        .stream(
            ModelRequest {
                request_id: format!("req-{}", rand_suffix()),
                model_policy: ModelPolicy {
                    endpoint: "ep".into(),
                    model: "m".into(),
                    reasoning_effort: None,
                    service_tier: None,
                },
                messages: vec![Message::text(Role::User, "go")],
                tool_projection: vec![],
                response_format: None,
                cache_key: None,
                cache_breakpoints: vec![],
                max_output_tokens: 64,
                timeout_ms,
                policy_tags: vec![],
            },
            &Requirements::default(),
            CancellationToken::new(),
        )
        .unwrap();
    let mut out = Vec::new();
    while let Some(e) = stream.events.recv().await {
        out.push(e);
    }
    (out, started.elapsed())
}

fn rand_suffix() -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static N: AtomicU64 = AtomicU64::new(0);
    N.fetch_add(1, Ordering::SeqCst)
}

fn completed(ev: &[ModelEvent]) -> bool {
    ev.iter().any(|e| matches!(e, ModelEvent::Completed { .. }))
}

fn error_codes(ev: &[ModelEvent]) -> Vec<String> {
    ev.iter()
        .filter_map(|e| match e {
            ModelEvent::Error { code, .. } => Some(code.clone()),
            _ => None,
        })
        .collect()
}

fn tool_calls(ev: &[ModelEvent]) -> usize {
    ev.iter()
        .filter(|e| matches!(e, ModelEvent::ToolCallComplete { .. }))
        .count()
}

fn tool() -> Answer {
    Answer::Tool {
        name: "fs.read".into(),
        args: json!({"path": "README.md"}),
    }
}

fn limited(header: &str, value: &str) -> Fault {
    Fault::Status {
        status: 429,
        headers: vec![(header.into(), value.into())],
        body: r#"{"error":{"type":"rate_limit_error","message":"slow down"}}"#.into(),
    }
}

/// A 429 with `retry-after` (seconds) or `retry-after-ms` is waited for at
/// least that long and retried, before any token; the retry completes.
#[tokio::test]
async fn qual_px_171_a_rate_limit_with_either_retry_header_is_waited_and_retried_before_the_first_token()
 {
    for wire in WIRES {
        for (header, value, at_least) in [
            ("retry-after", "1", Duration::from_millis(1000)),
            ("retry-after-ms", "350", Duration::from_millis(350)),
        ] {
            let server = fault_provider(wire, vec![limited(header, value), Fault::Ok(tool())]);
            let gw = gateway(&server, wire);
            let (ev, _) = run(&gw, 20_000).await;
            assert!(completed(&ev), "{wire:?} {header}: {ev:?}");
            assert_eq!(tool_calls(&ev), 1, "{wire:?} {header}: one tool call, once");
            let a = server.arrivals();
            assert_eq!(a.len(), 2, "{wire:?} {header}: one retry");
            assert!(
                a[1] - a[0] >= at_least,
                "{wire:?} {header}: waited {:?}",
                a[1] - a[0]
            );
        }
    }
}

/// A body cut in the middle of an event after the first token ends with a
/// typed failure; nothing is retried and no tool call is reported.
#[tokio::test]
async fn qual_px_171_a_cut_after_the_first_token_is_typed_and_never_retried() {
    for wire in WIRES {
        let keep = fault_provider::events(wire, &tool()).len() / 2;
        let server = fault_provider(
            wire,
            vec![
                Fault::CutMidEvent {
                    answer: tool(),
                    keep,
                },
                Fault::Ok(tool()),
            ],
        );
        let gw = gateway(&server, wire);
        let (ev, _) = run(&gw, 20_000).await;
        assert!(!completed(&ev), "{wire:?}: {ev:?}");
        assert_eq!(error_codes(&ev), ["STREAM_INTERRUPTED"], "{wire:?}: {ev:?}");
        assert_eq!(tool_calls(&ev), 0, "{wire:?}: no half tool call");
        assert_eq!(
            server.count(),
            1,
            "{wire:?}: not retried after the first token"
        );
    }
}

/// A body cut before its first event is retried, and the retry's answer is
/// the only one reported.
#[tokio::test]
async fn qual_px_171_a_cut_before_any_event_is_retried() {
    for wire in WIRES {
        let server = fault_provider(
            wire,
            vec![
                Fault::CutMidEvent {
                    answer: tool(),
                    keep: 0,
                },
                Fault::Ok(tool()),
            ],
        );
        let gw = gateway(&server, wire);
        let (ev, _) = run(&gw, 20_000).await;
        assert!(completed(&ev), "{wire:?}: {ev:?}");
        assert_eq!(tool_calls(&ev), 1, "{wire:?}: {ev:?}");
        assert_eq!(server.count(), 2, "{wire:?}");
    }
}

/// A body that ends cleanly part-way through the answer: after the first
/// token it is a typed failure and not retried, with no tool call; before
/// any event it is retried.
#[tokio::test]
async fn qual_px_171_a_stream_that_stops_short_is_typed_after_the_first_token_and_retried_before_it()
 {
    for wire in WIRES {
        let keep = fault_provider::events(wire, &tool()).len() / 2;
        let server = fault_provider(
            wire,
            vec![
                Fault::EndEarly {
                    answer: tool(),
                    keep,
                },
                Fault::Ok(tool()),
            ],
        );
        let gw = gateway(&server, wire);
        let (ev, _) = run(&gw, 20_000).await;
        assert!(!completed(&ev), "{wire:?}: {ev:?}");
        assert_eq!(error_codes(&ev), ["STREAM_INTERRUPTED"], "{wire:?}: {ev:?}");
        assert_eq!(tool_calls(&ev), 0, "{wire:?}");
        assert_eq!(
            server.count(),
            1,
            "{wire:?}: not retried after the first token"
        );

        let server = fault_provider(
            wire,
            vec![
                Fault::EndEarly {
                    answer: tool(),
                    keep: 0,
                },
                Fault::Ok(tool()),
            ],
        );
        let gw = gateway(&server, wire);
        let (ev, _) = run(&gw, 20_000).await;
        assert!(completed(&ev), "{wire:?}: {ev:?}");
        assert_eq!(tool_calls(&ev), 1, "{wire:?}");
        assert_eq!(
            server.count(),
            2,
            "{wire:?}: retried before the first token"
        );
    }
}

/// An idle stall past the request's timeout ends with TIMEOUT on schedule.
#[tokio::test]
async fn qual_px_171_an_idle_stall_times_out_on_schedule() {
    for wire in WIRES {
        let server = fault_provider(
            wire,
            vec![Fault::Stall {
                answer: Answer::Text("hello".into()),
                keep: 2,
                ms: 10_000,
            }],
        );
        let gw = gateway(&server, wire);
        let (ev, took) = run(&gw, 800).await;
        assert_eq!(error_codes(&ev), ["TIMEOUT"], "{wire:?}: {ev:?}");
        assert!(
            took >= Duration::from_millis(700) && took < Duration::from_secs(4),
            "{wire:?}: {took:?}"
        );
        assert_eq!(server.count(), 1, "{wire:?}");
    }
}

/// An event whose data is not JSON is refused with a typed code and the
/// stream does not complete.
#[tokio::test]
async fn qual_px_171_a_malformed_event_is_refused() {
    for wire in WIRES {
        let server = fault_provider(
            wire,
            vec![Fault::Malformed {
                answer: Answer::Text("hello".into()),
                keep: 2,
            }],
        );
        let gw = gateway(&server, wire);
        let (ev, _) = run(&gw, 20_000).await;
        assert!(!completed(&ev), "{wire:?}: {ev:?}");
        let codes = error_codes(&ev);
        assert!(
            !codes.is_empty() && codes.iter().all(|c| c == "MALFORMED_STREAM"),
            "{wire:?}: {ev:?}"
        );
        assert_eq!(server.count(), 1, "{wire:?}");
    }
}
