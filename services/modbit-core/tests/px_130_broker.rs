//! PX-130 (QUAL-PX-130) on the real Core: every credential is used through
//! the one broker. A planted provider key reaches the (scripted) provider in
//! the Authorization header of each request and appears nowhere else; the
//! counter at the broker equals the number of uses; a revoked credential is
//! refused on the very next request; a rotation applies to the next request.
//!
//! Real: the `modbit-core` binary, its socket and store, the gateway's real
//! HTTP requests to a local server that records each request's headers.
//! Stand-in: that server's scripted replies.
#![cfg(unix)]

mod px_common;

use std::sync::{Arc, Mutex};
use std::time::Duration;

use modbit_protocol::client::Client;
use modbit_protocol::v1::{
    ConfigureProvider, CredentialBrokerView, CredentialRevoked, GetCredentialBroker,
    RevokeCredential,
};
use prost::Message;
use px_common::*;
use serde_json::json;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

const KEY1: &str = "sk-planted-broker-key-1111222233334444";
const KEY2: &str = "sk-planted-broker-key-5555666677778888";

/// A scripted OpenAI-compatible server that records the Authorization header
/// of every chat request.
async fn server() -> (String, Arc<Mutex<Vec<String>>>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let auth: Arc<Mutex<Vec<String>>> = Arc::default();
    let a2 = Arc::clone(&auth);
    tokio::spawn(async move {
        loop {
            let Ok((mut sock, _)) = listener.accept().await else {
                return;
            };
            let auth = Arc::clone(&a2);
            tokio::spawn(async move {
                let mut buf = Vec::new();
                let mut tmp = [0u8; 8192];
                let (end, len, head) = loop {
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
                        break (i + 4, len, head);
                    }
                };
                while buf.len() < end + len {
                    let n = sock.read(&mut tmp).await.unwrap_or(0);
                    if n == 0 {
                        break;
                    }
                    buf.extend_from_slice(&tmp[..n]);
                }
                let body: serde_json::Value =
                    serde_json::from_slice(&buf[end..(end + len).min(buf.len())])
                        .unwrap_or_default();
                let results = body["messages"]
                    .as_array()
                    .map_or(0, |m| m.iter().filter(|x| x["role"] == "tool").count());
                if head.starts_with("POST") {
                    auth.lock().unwrap().push(
                        head.lines()
                            .find_map(|l| {
                                let (k, v) = l.split_once(':')?;
                                k.eq_ignore_ascii_case("authorization")
                                    .then(|| v.trim().to_owned())
                            })
                            .unwrap_or_default(),
                    );
                }
                let call = if results == 0 {
                    json!({"name": "plan.update", "args": {"outcome": "x", "expected_files": []}})
                } else {
                    json!({"name": "task.complete", "args": {"summary": "done", "self_review": {"findings": []}}})
                };
                let frames = [
                    json!({"id":"c","model":"m","choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"id":format!("call_{results}"),"type":"function","function":{"name":call["name"],"arguments":call["args"].to_string()}}]},"finish_reason":null}]}).to_string(),
                    json!({"id":"c","model":"m","choices":[{"index":0,"delta":{},"finish_reason":"tool_calls"}],"usage":{"prompt_tokens":100,"completion_tokens":10,"prompt_tokens_details":{"cached_tokens":0}}}).to_string(),
                    "[DONE]".to_owned(),
                ];
                let _ = sock
                    .write_all(b"HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\nx-request-id: r\r\nconnection: close\r\ntransfer-encoding: chunked\r\n\r\n")
                    .await;
                for f in frames {
                    let frame = format!("data: {f}\n\n");
                    let _ = sock
                        .write_all(format!("{:x}\r\n{}\r\n", frame.len(), frame).as_bytes())
                        .await;
                }
                let _ = sock.write_all(b"0\r\n\r\n").await;
                let _ = sock.shutdown().await;
            });
        }
    });
    (format!("http://127.0.0.1:{port}"), auth)
}

async fn broker_view(c: &mut Client) -> (CredentialBrokerView, Vec<u8>) {
    let ack = c
        .command(envelope(
            rand_id(),
            "GetCredentialBroker",
            GetCredentialBroker { audit_limit: 500 }.encode_to_vec(),
        ))
        .await
        .unwrap();
    let raw = ack.result.clone();
    (Client::result(&ack).unwrap(), raw)
}

fn provider_row(v: &CredentialBrokerView) -> &modbit_protocol::v1::CredentialView {
    v.credentials
        .iter()
        .find(|c| c.id == "provider:openai")
        .expect("the provider credential is in the broker")
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn qual_px_130_a_provider_key_is_used_through_the_broker_counted_revoked_and_rotated() {
    let (repo, root) = plain_repo(&[("a.txt", "a\n")]);
    let (base, auth) = server().await;
    let dir = tempfile::tempdir().unwrap();
    let core = CoreProcess::spawn_with_env(
        dir.path(),
        &[
            ("MODBIT_OPENAI_BASE_URL", base.as_str()),
            ("OPENAI_API_KEY", KEY1),
            ("ANTHROPIC_API_KEY", ""),
        ],
    );
    let mut c = core.client().await;
    let (session, g) = session_with_lease(&mut c, 0x31).await;
    let task = create_task(&mut c, &session, g, &root, 0x32, "local_trusted", "goal").await;
    start_task(&mut c, &task, g, 0x33, "gpt-5-mini").await;
    let st = wait_task(&mut c, &task, 120).await;
    assert!(!st.loop_alive, "{st:?}");

    // The key reached the provider, on every request.
    let seen = auth.lock().unwrap().clone();
    assert!(seen.len() >= 2, "{seen:?}");
    assert!(
        seen.iter().all(|h| h == &format!("Bearer {KEY1}")),
        "{seen:?}"
    );
    // A counter at the interface equals the number of uses.
    let (view, raw) = broker_view(&mut c).await;
    let row = provider_row(&view);
    assert_eq!(row.uses, seen.len() as u64, "{row:?}");
    assert_eq!(row.kind, "provider");
    assert_eq!(
        (row.generation, row.revoked, row.configured),
        (1, false, true)
    );
    assert!(
        view.audit
            .iter()
            .any(|a| a.action == "USE" && a.principal.starts_with("task:"))
    );
    // The planted key is in no artifact the Core produced: the broker view,
    // the events, the object store.
    assert!(!String::from_utf8_lossy(&raw).contains(KEY1));
    let events = replay(&core, &session).await;
    assert!(
        !serde_json::to_string(&events)
            .unwrap()
            .contains("sk-planted-broker-key")
    );
    drop(c);
    let mut c = core.client().await;

    // Rotation: the next request carries the new key.
    let before = auth.lock().unwrap().len();
    let ack = c
        .command(envelope(
            rand_id(),
            "ConfigureProvider",
            ConfigureProvider {
                provider: "openai".into(),
                api_key: KEY2.into(),
                base_url: base.clone(),
            }
            .encode_to_vec(),
        ))
        .await;
    assert!(ack.is_ok(), "{ack:?}");
    let t2 = create_task(&mut c, &session, g, &root, 0x34, "local_trusted", "again").await;
    start_task(&mut c, &t2, g, 0x35, "gpt-5-mini").await;
    let st = wait_task(&mut c, &t2, 120).await;
    assert!(!st.loop_alive, "{st:?}");
    let after = auth.lock().unwrap().clone();
    assert!(after.len() > before);
    assert!(
        after[before..]
            .iter()
            .all(|h| h == &format!("Bearer {KEY2}")),
        "rotation applies to the next request: {:?}",
        &after[before..]
    );
    let (view, _) = broker_view(&mut c).await;
    assert!(provider_row(&view).generation >= 2);

    // Revocation: the very next request is refused, and nothing is sent.
    let ack = c
        .command(envelope(
            rand_id(),
            "RevokeCredential",
            RevokeCredential {
                credential_id: "provider:openai".into(),
                reason: "operator".into(),
                ..Default::default()
            }
            .encode_to_vec(),
        ))
        .await
        .unwrap();
    assert_eq!(
        Client::result::<CredentialRevoked>(&ack).unwrap().revoked,
        1
    );
    let sent = auth.lock().unwrap().len();
    let t3 = create_task(&mut c, &session, g, &root, 0x36, "local_trusted", "third").await;
    start_task(&mut c, &t3, g, 0x37, "gpt-5-mini").await;
    let st = wait_task(&mut c, &t3, 60).await;
    assert!(!st.loop_alive, "{st:?}");
    assert_ne!(st.state, "ReadyForReview", "{st:?}");
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(
        auth.lock().unwrap().len(),
        sent,
        "a revoked key is never sent"
    );
    let (view, _) = broker_view(&mut c).await;
    assert!(provider_row(&view).revoked);
    drop(repo);
}
