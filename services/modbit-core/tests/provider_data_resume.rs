//! Opaque per-call provider data on the real Core (Gemini 3's thought
//! signature): the `modbit-core` binary persists what the provider attached
//! to a tool call with the assistant message, sends it back on the next
//! request, and still sends it back after the process is killed and the run
//! resumes from the log. The model is the repository's scripted
//! OpenAI-compatible server; the Core, its store, its log and its gateway are
//! real.

mod px_common;

use modbit_protocol::client::Client;
use modbit_protocol::v1::*;
use prost::Message as _;
use px_common::*;
use serde_json::{Value, json};
use std::time::Duration;

fn assistant_calls(body: &Value) -> Vec<Value> {
    body["messages"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|m| m["role"] == "assistant")
        .flat_map(|m| m["tool_calls"].as_array().cloned().unwrap_or_default())
        .collect()
}

#[tokio::test(flavor = "multi_thread")]
async fn a_tool_calls_provider_data_is_replayed_after_a_core_restart() {
    let sig = json!({"google": {"thought_signature": "c2lnLXJlc3VtZQ=="}});
    let (_repo, root) = plain_repo(&[("README.md", "hello\n")]);
    let gate = tempfile::tempdir().unwrap();
    let release = gate.path().join("release");
    let held = release.to_string_lossy().into_owned();
    let call = json!({"calls": [{"name": "fs.read", "args": {"path": "README.md"}, "extra_content": sig}]});
    // The answer to the tool result is held until the test releases it, so
    // the Core can be killed while it waits.
    let (base, seen) = scripted_model_fn(std::sync::Arc::new(move |_, results| {
        if results == 0 {
            call.clone()
        } else {
            json!({"wait_file": held, "wait_file_secs": 120, "text": "The readme says hello."})
        }
    }))
    .await;
    let env = model_env(&base);
    let refs: Vec<(&str, &str)> = env.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
    let dir = tempfile::tempdir().unwrap();
    let mut core = CoreProcess::spawn_with_env(dir.path(), &refs);
    let mut c = core.client().await;
    let (session, g) = session_with_lease(&mut c, 0x41).await;
    let task = create_task(
        &mut c,
        &session,
        g,
        &root,
        0x42,
        "local_trusted",
        "read the readme",
    )
    .await;
    start_task(&mut c, &task, g, 0x43, "gpt-5-mini").await;
    let deadline = std::time::Instant::now() + Duration::from_secs(60);
    while seen.lock().unwrap().len() < 2 {
        assert!(
            std::time::Instant::now() < deadline,
            "the follow-up never arrived"
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let follow_up = seen.lock().unwrap()[1].clone();
    let calls = assistant_calls(&follow_up);
    assert_eq!(calls.len(), 1, "{follow_up}");
    assert_eq!(calls[0]["function"]["name"], "fs.read");
    assert_eq!(
        calls[0]["extra_content"], sig,
        "the next request carries what the provider attached, unchanged"
    );
    let call_id = calls[0]["id"].clone();

    // Kill the Core while the follow-up is in flight: what survives is the log.
    drop(c);
    core.kill();
    std::fs::write(&release, b"go").unwrap();
    let before = seen.lock().unwrap().len();
    let core2 = CoreProcess::spawn_with_env(dir.path(), &refs);
    let mut c2 = core2.client().await;
    let ack = c2
        .command(envelope(
            id16(0x44),
            "AcquireSessionLease",
            AcquireSessionLease {
                session_id: Some(session.clone()),
                owner: "resumer".into(),
            }
            .encode_to_vec(),
        ))
        .await
        .unwrap();
    let lease: SessionLeaseAcquired = Client::result(&ack).unwrap();
    let ack = c2
        .command(envelope_fenced(
            id16(0x45),
            "StartTask",
            StartTask {
                task_id: Some(task.clone()),
                endpoint: String::new(),
                model: "gpt-5-mini".into(),
                max_turns: 14,
                max_tool_calls: 0,
                max_no_progress_turns: 6,
                skills: vec![],
                ..Default::default()
            }
            .encode_to_vec(),
            Some(lease.lease_generation),
        ))
        .await
        .unwrap();
    let started: TaskRunStarted = Client::result(&ack).unwrap();
    assert!(started.resumed, "{started:?}");
    let st = wait_task(&mut c2, &task, 60).await;
    // How the run ends is the harness's business (a read-only answer that
    // never finishes is suspended as no progress); what matters here is
    // every request the resumed run made.
    assert!(!st.loop_alive, "{st:?}");
    let resumed = seen.lock().unwrap()[before..].to_vec();
    assert!(!resumed.is_empty(), "the resumed run asked the model again");
    for body in &resumed {
        let calls = assistant_calls(body);
        assert_eq!(calls.len(), 1, "{body}");
        assert_eq!(
            calls[0]["id"], call_id,
            "the same call, rebuilt from the log"
        );
        assert_eq!(
            calls[0]["extra_content"], sig,
            "the rebuilt call carries the signature the provider attached"
        );
    }
}
