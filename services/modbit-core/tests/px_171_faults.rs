//! PX-171 (QUAL-PX-171) on the real Core: the `modbit-core` binary, its log
//! and its gateway against the shared fault-injecting provider server
//! (`tests/support/fault_provider.rs`), on both wire formats. A stream cut
//! in the middle of a tool call after the first token leaves a typed
//! failure on the log and no tool call at all; a rate limit with
//! `retry-after-ms` is waited and retried once, and the retry is recorded on
//! the route.

#[path = "../../../tests/support/fault_provider.rs"]
mod fault_provider;
mod px_common;

use fault_provider::{Answer, Fault, FaultServer, Wire, fault_provider};
use px_common::*;
use serde_json::{Value, json};

fn env_for(wire: Wire, server: &FaultServer) -> Vec<(String, String)> {
    match wire {
        Wire::OpenAi => model_env(&server.base_url),
        Wire::Anthropic => vec![
            ("MODBIT_ANTHROPIC_BASE_URL".into(), server.base_url.clone()),
            (
                "ANTHROPIC_API_KEY".into(),
                "anthropic-test-key-px171".into(),
            ),
            ("OPENAI_API_KEY".into(), String::new()),
        ],
    }
}

fn model_of(wire: Wire) -> &'static str {
    match wire {
        Wire::OpenAi => "gpt-5-mini",
        Wire::Anthropic => "claude-haiku-4-5-20251001",
    }
}

async fn run_task(wire: Wire, plan: Vec<Fault>) -> (Vec<Value>, FaultServer) {
    let (_repo, root) = plain_repo(&[("README.md", "hello\n")]);
    let server = fault_provider(wire, plan);
    let dir = tempfile::tempdir().unwrap();
    let env = env_for(wire, &server);
    let refs: Vec<(&str, &str)> = env.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
    let core = CoreProcess::spawn_with_env(dir.path(), &refs);
    let mut c = core.client().await;
    let (session, g) = session_with_lease(&mut c, 0x71).await;
    let task = create_task(
        &mut c,
        &session,
        g,
        &root,
        0x72,
        "local_trusted",
        "read the readme",
    )
    .await;
    start_task(&mut c, &task, g, 0x73, model_of(wire)).await;
    let st = wait_task(&mut c, &task, 120).await;
    assert!(!st.loop_alive, "{wire:?}: {st:?}");
    (replay(&core, &session).await, server)
}

fn of<'a>(ev: &'a [Value], t: &str) -> Vec<&'a Value> {
    ev.iter().filter(|e| e["event_type"] == t).collect()
}

fn tool() -> Answer {
    Answer::Tool {
        name: "fs.read".into(),
        args: json!({"path": "README.md"}),
    }
}

/// A cut after the first token, inside the tool call's arguments: the turn
/// fails `STREAM_INTERRUPTED` on the log, the request is not sent again and
/// no tool call is proposed, so nothing can run twice.
#[tokio::test(flavor = "multi_thread")]
async fn qual_px_171_a_cut_inside_a_tool_call_is_a_typed_failure_with_no_tool_call() {
    for wire in [Wire::OpenAi, Wire::Anthropic] {
        let keep = fault_provider::events(wire, &tool()).len() / 2;
        let (ev, server) = run_task(
            wire,
            vec![
                Fault::CutMidEvent {
                    answer: tool(),
                    keep,
                },
                Fault::Ok(tool()),
            ],
        )
        .await;
        let failed: Vec<&str> = of(&ev, "TurnFailed")
            .iter()
            .map(|e| {
                e["payload"]["payload"]["failure_code"]
                    .as_str()
                    .unwrap_or_default()
            })
            .collect();
        assert_eq!(failed, ["STREAM_INTERRUPTED"], "{wire:?}");
        assert!(
            of(&ev, "ToolCallProposed").is_empty(),
            "{wire:?}: no tool call from a cut stream"
        );
        assert_eq!(
            server.count(),
            1,
            "{wire:?}: not retried after the first token"
        );
    }
}

/// A 429 with `retry-after-ms` before any token: the Core's request waits,
/// is retried once, completes, and the route record says it retried.
#[tokio::test(flavor = "multi_thread")]
async fn qual_px_171_a_rate_limit_is_retried_once_and_the_route_records_it() {
    for wire in [Wire::OpenAi, Wire::Anthropic] {
        let limited = Fault::Status {
            status: 429,
            headers: vec![("retry-after-ms".into(), "300".into())],
            body: r#"{"error":{"type":"rate_limit_error","message":"slow down"}}"#.into(),
        };
        let (ev, server) = run_task(
            wire,
            vec![limited, Fault::Ok(Answer::Text("Nothing to do.".into()))],
        )
        .await;
        // The first invocation waited, retried once and completed; the
        // turns after it (the stand-in only ever answers in text) are
        // ordinary, unretried requests.
        let a = server.arrivals();
        assert!(a.len() >= 2, "{wire:?}");
        assert!(
            a[1] - a[0] >= std::time::Duration::from_millis(300),
            "{wire:?}"
        );
        let failed: Vec<&str> = of(&ev, "TurnFailed")
            .iter()
            .map(|e| {
                e["payload"]["payload"]["failure_code"]
                    .as_str()
                    .unwrap_or_default()
            })
            .collect();
        assert!(!failed.contains(&"RATE_LIMITED"), "{wire:?}: {failed:?}");
        let usage = of(&ev, "ModelUsageRecorded");
        assert_eq!(
            usage.len(),
            a.len() - 1,
            "{wire:?}: the 429 is a retry, not an invocation"
        );
        assert_eq!(
            usage[0]["payload"]["payload"]["route"]["retries"], 1,
            "{wire:?}: {}",
            usage[0]
        );
        assert!(
            usage[1..]
                .iter()
                .all(|u| u["payload"]["payload"]["route"]["retries"] == 0),
            "{wire:?}"
        );
    }
}
