//! M8.1 qualification (docs/24 "Cloud API", "Postgres", "Object storage",
//! "Multi-tenancy"; docs/30 "Cloud HTTP control API"; docs/33 "Cloud API
//! implementation", "Idempotency"; docs/31 "Cloud schema"): the real API over
//! a real Postgres and a real S3-compatible store (MinIO in CI, the same in
//! the local loop). Runs only where `MODBIT_CLOUD_TEST_DATABASE_URL` names a
//! database — the hosted `cloud` job provides one; anywhere else the test
//! says why it did not run instead of pretending.

use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use modbit_cloud_api::{Config, Served, serve};
use modbit_domain::approval::ApprovalEvent;
use modbit_domain::event::{Actor, AggregateType};
use modbit_domain::toolcall::EffectClass;
use modbit_domain::{ApprovalId, SessionId, TaskId, TenantId, ToolCallId};
use modbit_event_store::AppendRequest;
use modbit_event_store::cloud::{CloudStoreConfig, S3Config, new_event};
use serde_json::{Value, json};

fn env_config(rate_capacity: u32) -> Option<Config> {
    let database_url = std::env::var("MODBIT_CLOUD_TEST_DATABASE_URL").ok()?;
    let s3 = std::env::var("MODBIT_CLOUD_TEST_S3_ENDPOINT")
        .ok()
        .map(|endpoint| S3Config {
            endpoint,
            bucket: std::env::var("MODBIT_CLOUD_TEST_S3_BUCKET")
                .unwrap_or_else(|_| "modbit-test".into()),
            region: "us-east-1".into(),
            access_key_id: std::env::var("MODBIT_CLOUD_TEST_S3_ACCESS_KEY_ID")
                .unwrap_or_else(|_| "modbit".into()),
            secret_access_key: std::env::var("MODBIT_CLOUD_TEST_S3_SECRET_ACCESS_KEY")
                .unwrap_or_else(|_| "modbitsecret".into()),
            allow_http: true,
        });
    Some(Config {
        store: CloudStoreConfig { database_url, s3 },
        token_key: None,
        bind: "127.0.0.1:0".into(),
        rate_capacity,
        rate_per_second: 0.5,
        worker_key: None,
    })
}

struct Api {
    served: Served,
    http: reqwest::Client,
    base: String,
}

impl Api {
    async fn start(rate_capacity: u32) -> Option<Api> {
        let Some(cfg) = env_config(rate_capacity) else {
            eprintln!(
                "SKIPPED: MODBIT_CLOUD_TEST_DATABASE_URL unset (the hosted cloud job runs this against Postgres and MinIO)"
            );
            return None;
        };
        let served = serve(cfg).await.expect("serve");
        let base = format!("http://{}", served.addr);
        Some(Api {
            served,
            http: reqwest::Client::new(),
            base,
        })
    }

    async fn tenant(&self, name: &str) -> (TenantId, String) {
        let t = self.served.state.store.create_tenant(name).await.unwrap();
        let (_p, secret) = self
            .served
            .state
            .store
            .create_principal(t, "user", name)
            .await
            .unwrap();
        (t, secret)
    }

    async fn token(&self, secret: &str) -> Value {
        self.http
            .post(format!("{}/v1/auth/token", self.base))
            .json(&json!({"secret": secret}))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap()
    }

    async fn post(
        &self,
        token: &str,
        path: &str,
        body: Value,
    ) -> (reqwest::StatusCode, Value, Option<String>) {
        let r = self
            .http
            .post(format!("{}{path}", self.base))
            .bearer_auth(token)
            .json(&body)
            .send()
            .await
            .unwrap();
        let status = r.status();
        let replayed = r
            .headers()
            .get("x-modbit-replayed")
            .map(|v| v.to_str().unwrap().to_owned());
        (status, r.json().await.unwrap_or(Value::Null), replayed)
    }

    async fn get(&self, token: &str, path: &str) -> (reqwest::StatusCode, Value) {
        let r = self
            .http
            .get(format!("{}{path}", self.base))
            .bearer_auth(token)
            .send()
            .await
            .unwrap();
        let status = r.status();
        (status, r.json().await.unwrap_or(Value::Null))
    }
}

/// docs/24 "Identity": a secret buys a token pair; access tokens verify
/// without the store; a refresh token rotates once and a replay revokes the
/// family; an unknown secret is refused.
#[tokio::test]
async fn qual_m8_1_tokens_are_issued_from_a_secret_and_refresh_rotates_once() {
    let Some(api) = Api::start(120).await else {
        return;
    };
    let (_t, secret) = api.tenant("tokens").await;
    let first = api.token(&secret).await;
    assert!(
        first["access_token"].as_str().unwrap().starts_with("mba_"),
        "{first}"
    );
    let refresh = first["refresh_token"].as_str().unwrap().to_owned();
    // Rotation: the new token works, the old one is spent.
    let (s, second, _) = api
        .post("", "/v1/auth/refresh", json!({"refresh_token": refresh}))
        .await;
    assert_eq!(s, 200, "{second}");
    let refresh2 = second["refresh_token"].as_str().unwrap().to_owned();
    assert_ne!(refresh2, refresh);
    let (s, replay, _) = api
        .post("", "/v1/auth/refresh", json!({"refresh_token": refresh}))
        .await;
    assert_eq!(s, 401, "a spent token is a replay: {replay}");
    let (s, revoked, _) = api
        .post("", "/v1/auth/refresh", json!({"refresh_token": refresh2}))
        .await;
    assert_eq!(s, 401, "the replay revoked the family: {revoked}");
    let (s, unknown, _) = api
        .post("", "/v1/auth/token", json!({"secret": "mbs_nope"}))
        .await;
    assert_eq!(s, 401, "{unknown}");
    // An unauthenticated control request is refused before any handler.
    let (s, body) = api
        .get(
            "mba_not.a.token",
            "/v1/sessions/00000000-0000-0000-0000-000000000000",
        )
        .await;
    assert_eq!(
        (s.as_u16(), body["code"].as_str()),
        (401, Some("UNAUTHENTICATED"))
    );
    api.served.stop();
}

/// docs/33 "Idempotency", docs/24 "Multi-tenancy", docs/30: a session and a
/// task are created with command ids — a retry replays the recorded
/// outcome and appends nothing; events replay by cursor; another tenant's
/// principal finds nothing and the attempt is audited; control commands
/// are durable events; a queued task with no owner cancels durably.
#[tokio::test]
async fn qual_m8_1_commands_are_idempotent_events_replay_by_cursor_and_tenants_never_cross() {
    let Some(api) = Api::start(120).await else {
        return;
    };
    let (ta, secret_a) = api.tenant("a").await;
    let (tb, secret_b) = api.tenant("b").await;
    let a = api.token(&secret_a).await["access_token"]
        .as_str()
        .unwrap()
        .to_owned();
    let b = api.token(&secret_b).await["access_token"]
        .as_str()
        .unwrap()
        .to_owned();
    let c1 = uuid::Uuid::now_v7().to_string();
    let (s, created, replayed) = api
        .post(&a, "/v1/sessions", json!({"command_id": c1}))
        .await;
    assert_eq!((s.as_u16(), replayed), (201, None), "{created}");
    let sid = created["session_id"].as_str().unwrap().to_owned();
    let (s, again, replayed) = api
        .post(&a, "/v1/sessions", json!({"command_id": c1}))
        .await;
    assert_eq!(
        (s.as_u16(), replayed.as_deref()),
        (200, Some("true")),
        "{again}"
    );
    assert_eq!(
        again["session_id"], created["session_id"],
        "the same session, no second one"
    );
    let c2 = uuid::Uuid::now_v7().to_string();
    let (s, task, _) = api.post(&a, &format!("/v1/sessions/{sid}/tasks"), json!({"command_id": c2, "goal_text": "add a test", "execution_profile": "cloud_isolated"})).await;
    assert_eq!(s, 201, "{task}");
    let tid = task["task_id"].as_str().unwrap().to_owned();
    let (s, task2, replayed) = api
        .post(
            &a,
            &format!("/v1/sessions/{sid}/tasks"),
            json!({"command_id": c2, "goal_text": "add a test"}),
        )
        .await;
    assert_eq!(
        (s.as_u16(), replayed.as_deref()),
        (200, Some("true")),
        "{task2}"
    );
    assert_eq!(task2["task_id"], task["task_id"]);
    // The projection: one task, queued, the session ready for a worker.
    let (s, view) = api.get(&a, &format!("/v1/sessions/{sid}")).await;
    assert_eq!(s, 200, "{view}");
    assert_eq!(view["tasks"].as_array().unwrap().len(), 1, "{view}");
    assert_eq!(view["tasks"][0]["state"], "QUEUED");
    assert_eq!(view["lease"]["ready"], true);
    assert_eq!(view["last_session_offset"], 3);
    // Cursor replay: three events, then nothing after the cursor.
    let (s, evs) = api
        .get(&a, &format!("/v1/events?session_id={sid}&after=0"))
        .await;
    assert_eq!(s, 200);
    let types: Vec<&str> = evs["events"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["envelope"]["event_type"].as_str().unwrap())
        .collect();
    assert_eq!(types, vec!["SessionCreated", "TaskCreated", "TaskQueued"]);
    assert_eq!(evs["next_after"], 3);
    let (_, none) = api
        .get(&a, &format!("/v1/events?session_id={sid}&after=3"))
        .await;
    assert_eq!(none["events"].as_array().unwrap().len(), 0);
    // The integrity chain holds per aggregate, like the local store's.
    let e = &evs["events"][2]["envelope"];
    assert_eq!(e["sequence"], 2);
    assert_eq!(e["integrity_hash"].as_str().unwrap().len(), 64);
    // Tenant B: the session and the task do not exist for it, and the
    // attempts are on B's audit.
    let (s, body) = api.get(&b, &format!("/v1/sessions/{sid}")).await;
    assert_eq!(
        (s.as_u16(), body["code"].as_str()),
        (404, Some("NOT_FOUND")),
        "{body}"
    );
    let (s, body, _) = api
        .post(
            &b,
            &format!("/v1/tasks/{tid}:steer"),
            json!({"command_id": uuid::Uuid::now_v7().to_string(), "text": "x"}),
        )
        .await;
    assert_eq!(s, 404, "{body}");
    let (s, body) = api.get(&b, &format!("/v1/events?session_id={sid}")).await;
    assert_eq!(s, 404, "{body}");
    let denials = api.served.state.store.denials(tb).await.unwrap();
    assert!(
        denials.iter().any(|(r, _)| r == &format!("session:{sid}"))
            && denials.iter().any(|(r, _)| r == &format!("task:{tid}")),
        "{denials:?}"
    );
    assert!(api.served.state.store.denials(ta).await.unwrap().is_empty());
    // Control: steer is a durable input; pause and resume are requests; the
    // queued task with no owner cancels durably.
    let (s, steered, _) = api
        .post(
            &a,
            &format!("/v1/tasks/{tid}:steer"),
            json!({"command_id": uuid::Uuid::now_v7().to_string(), "text": "also update the docs"}),
        )
        .await;
    assert_eq!(
        (s.as_u16(), steered["event_type"].as_str()),
        (200, Some("TaskInputQueued")),
        "{steered}"
    );
    let (s, paused, _) = api
        .post(
            &a,
            &format!("/v1/tasks/{tid}:pause"),
            json!({"command_id": uuid::Uuid::now_v7().to_string()}),
        )
        .await;
    assert_eq!(
        (s.as_u16(), paused["event_type"].as_str()),
        (200, Some("TaskPauseRequested")),
        "{paused}"
    );
    let (s, cancelled, _) = api
        .post(
            &a,
            &format!("/v1/tasks/{tid}:cancel"),
            json!({"command_id": uuid::Uuid::now_v7().to_string(), "reason": "changed my mind"}),
        )
        .await;
    assert_eq!(
        (s.as_u16(), cancelled["event_type"].as_str()),
        (200, Some("TaskCancelled")),
        "{cancelled}"
    );
    let (_, view) = api.get(&a, &format!("/v1/sessions/{sid}")).await;
    assert_eq!(view["tasks"][0]["state"], "CANCELLED");
    let (s, terminal, _) = api
        .post(
            &a,
            &format!("/v1/tasks/{tid}:cancel"),
            json!({"command_id": uuid::Uuid::now_v7().to_string()}),
        )
        .await;
    assert_eq!(
        (s.as_u16(), terminal["code"].as_str()),
        (409, Some("TASK_TERMINAL")),
        "{terminal}"
    );
    // A bad payload is refused before anything is recorded.
    let (s, bad, _) = api
        .post(
            &a,
            &format!("/v1/sessions/{sid}/tasks"),
            json!({"goal_text": "no command id"}),
        )
        .await;
    assert_eq!(
        (s.as_u16(), bad["code"].as_str()),
        (400, Some("BAD_PAYLOAD"))
    );
    api.served.stop();
}

/// docs/30: the WSS stream replays after the cursor, says when it caught
/// up, then delivers what commits live; a client that drops and returns
/// with its last cursor sees nothing twice.
#[tokio::test]
async fn qual_m8_1_the_stream_replays_after_the_cursor_then_delivers_live_commits() {
    let Some(api) = Api::start(120).await else {
        return;
    };
    let (_t, secret) = api.tenant("stream").await;
    let a = api.token(&secret).await["access_token"]
        .as_str()
        .unwrap()
        .to_owned();
    let (_, created, _) = api
        .post(
            &a,
            "/v1/sessions",
            json!({"command_id": uuid::Uuid::now_v7().to_string()}),
        )
        .await;
    let sid = created["session_id"].as_str().unwrap().to_owned();
    let (_, task, _) = api
        .post(
            &a,
            &format!("/v1/sessions/{sid}/tasks"),
            json!({"command_id": uuid::Uuid::now_v7().to_string(), "goal_text": "stream me"}),
        )
        .await;
    let tid = task["task_id"].as_str().unwrap().to_owned();
    let url = format!(
        "ws://{}/v1/stream?session_id={sid}&after=1",
        api.served.addr
    );
    let mut req = tokio_tungstenite::tungstenite::client::IntoClientRequest::into_client_request(
        url.as_str(),
    )
    .unwrap();
    req.headers_mut()
        .insert("authorization", format!("Bearer {a}").parse().unwrap());
    let (mut ws, _) = tokio_tungstenite::connect_async(req).await.expect("ws");
    let mut got: Vec<Value> = Vec::new();
    async fn next<S>(ws: &mut tokio_tungstenite::WebSocketStream<S>) -> Value
    where
        S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
    {
        loop {
            let msg = tokio::time::timeout(Duration::from_secs(15), ws.next())
                .await
                .expect("frame in time")
                .unwrap()
                .unwrap();
            if let tokio_tungstenite::tungstenite::Message::Text(t) = msg {
                return serde_json::from_str::<Value>(&t).unwrap();
            }
        }
    }
    // After cursor 1: the two task events, then caught_up at 3.
    got.push(next(&mut ws).await);
    got.push(next(&mut ws).await);
    let caught = next(&mut ws).await;
    let types: Vec<&str> = got
        .iter()
        .map(|v| v["event"]["envelope"]["event_type"].as_str().unwrap())
        .collect();
    assert_eq!(types, vec!["TaskCreated", "TaskQueued"]);
    assert_eq!(caught["caught_up"], 3, "{caught}");
    // Live: a steer over HTTP lands on the stream.
    let (s, _, _) = api
        .post(
            &a,
            &format!("/v1/tasks/{tid}:steer"),
            json!({"command_id": uuid::Uuid::now_v7().to_string(), "text": "live"}),
        )
        .await;
    assert_eq!(s, 200);
    let live = next(&mut ws).await;
    assert_eq!(
        live["event"]["envelope"]["event_type"], "TaskInputQueued",
        "{live}"
    );
    assert_eq!(live["event"]["session_offset"], 4);
    ws.close(None).await.ok();
    // Reconnect with the last cursor: nothing twice.
    let url = format!(
        "ws://{}/v1/stream?session_id={sid}&after=4",
        api.served.addr
    );
    let mut req = tokio_tungstenite::tungstenite::client::IntoClientRequest::into_client_request(
        url.as_str(),
    )
    .unwrap();
    req.headers_mut()
        .insert("authorization", format!("Bearer {a}").parse().unwrap());
    let (mut ws2, _) = tokio_tungstenite::connect_async(req).await.expect("ws");
    let first = next(&mut ws2).await;
    assert_eq!(first["caught_up"], 4, "{first}");
    ws2.send(tokio_tungstenite::tungstenite::Message::Close(None))
        .await
        .ok();
    api.served.stop();
}

/// docs/30 approvals and outputs, docs/24 object storage: an approval opened
/// by the execution owner is resolved only on its exact intent (a mismatch
/// is refused and its command recorded), a second resolution is refused;
/// an output is read by range inside the tenant, with a short-lived signed
/// URL as the artifact grant; another tenant finds neither.
#[tokio::test]
async fn qual_m8_1_approvals_bind_to_intent_and_outputs_read_by_range_within_the_tenant() {
    let Some(api) = Api::start(120).await else {
        return;
    };
    let (ta, secret_a) = api.tenant("outputs-a").await;
    let (_tb, secret_b) = api.tenant("outputs-b").await;
    let a = api.token(&secret_a).await["access_token"]
        .as_str()
        .unwrap()
        .to_owned();
    let b = api.token(&secret_b).await["access_token"]
        .as_str()
        .unwrap()
        .to_owned();
    let (_, created, _) = api
        .post(
            &a,
            "/v1/sessions",
            json!({"command_id": uuid::Uuid::now_v7().to_string()}),
        )
        .await;
    let sid = SessionId::parse(created["session_id"].as_str().unwrap()).unwrap();
    let (_, task, _) = api
        .post(
            &a,
            &format!("/v1/sessions/{sid}/tasks"),
            json!({"command_id": uuid::Uuid::now_v7().to_string(), "goal_text": "approve me"}),
        )
        .await;
    let tid = TaskId::parse(task["task_id"].as_str().unwrap()).unwrap();
    // The execution owner opens an approval (as a worker will, M8.2).
    let aid = ApprovalId::new();
    let store = &api.served.state.store;
    store
        .append(
            AppendRequest {
                tenant_id: ta,
                session_id: sid,
                task_id: Some(tid),
                run_id: None,
                turn_id: None,
                step_id: None,
                aggregate_type: AggregateType::Approval,
                aggregate_id: *aid.as_bytes(),
                expected_sequence: Some(0),
                events: vec![
                    new_event(
                        "ApprovalRequested",
                        &ApprovalEvent::ApprovalRequested {
                            task_id: tid,
                            tool_call_id: ToolCallId::new(),
                            tool_name: "shell.exec".into(),
                            effect_class: EffectClass::Destructive,
                            intent_hash: "abc123".into(),
                            scope_json: "{}".into(),
                            expires_at: None,
                        },
                        Actor::Core("worker".into()),
                    )
                    .unwrap(),
                ],
            },
            None,
        )
        .await
        .unwrap();
    let wrong = uuid::Uuid::now_v7().to_string();
    let (s, body, _) = api
        .post(
            &a,
            &format!("/v1/approvals/{aid}:approve"),
            json!({"command_id": wrong, "intent_hash": "other"}),
        )
        .await;
    assert_eq!(
        (s.as_u16(), body["code"].as_str()),
        (409, Some("INTENT_MISMATCH")),
        "{body}"
    );
    let (s, body, replayed) = api
        .post(
            &a,
            &format!("/v1/approvals/{aid}:approve"),
            json!({"command_id": wrong, "intent_hash": "abc123"}),
        )
        .await;
    assert_eq!(
        (s.as_u16(), replayed.as_deref()),
        (409, Some("true")),
        "the same command id answers the same way: {body}"
    );
    let (s, body, _) = api
        .post(
            &b,
            &format!("/v1/approvals/{aid}:approve"),
            json!({"command_id": uuid::Uuid::now_v7().to_string(), "intent_hash": "abc123"}),
        )
        .await;
    assert_eq!(s, 404, "another tenant: {body}");
    let (s, body, _) = api.post(&a, &format!("/v1/approvals/{aid}:approve"), json!({"command_id": uuid::Uuid::now_v7().to_string(), "intent_hash": "abc123", "reason": "go"})).await;
    assert_eq!(
        (s.as_u16(), body["status"].as_str()),
        (200, Some("APPROVED")),
        "{body}"
    );
    let (s, body, _) = api
        .post(
            &a,
            &format!("/v1/approvals/{aid}:deny"),
            json!({"command_id": uuid::Uuid::now_v7().to_string(), "intent_hash": "abc123"}),
        )
        .await;
    assert_eq!(
        (s.as_u16(), body["code"].as_str()),
        (409, Some("APPROVAL_NOT_OPEN")),
        "{body}"
    );
    let (approval, _) = store.approval(ta, aid).await.unwrap().unwrap();
    assert_eq!(format!("{:?}", approval.state), "Approved");
    assert_eq!(approval.resolver.as_deref(), Some("user:outputs-a"));
    // Outputs: content-hashed, tenant-scoped, read by range.
    let hash = store
        .put_object(ta, b"0123456789abcdef", "text/plain")
        .await
        .unwrap();
    assert_eq!(hash.len(), 64);
    let r = api
        .http
        .get(format!("{}/v1/outputs/{hash}?start=2&len=3", api.base))
        .bearer_auth(&a)
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 206);
    assert_eq!(
        r.headers().get("content-range").unwrap().to_str().unwrap(),
        "bytes 2-4/16"
    );
    assert_eq!(r.bytes().await.unwrap().as_ref(), b"234");
    let r = api
        .http
        .get(format!("{}/v1/outputs/{hash}", api.base))
        .bearer_auth(&a)
        .header("range", "bytes=10-15")
        .send()
        .await
        .unwrap();
    assert_eq!(r.bytes().await.unwrap().as_ref(), b"abcdef");
    let (s, body) = api.get(&b, &format!("/v1/outputs/{hash}")).await;
    assert_eq!(s, 404, "another tenant: {body}");
    let (s, art) = api.get(&a, &format!("/v1/artifacts/{hash}")).await;
    assert_eq!(
        (s.as_u16(), art["byte_length"].as_u64()),
        (200, Some(16)),
        "{art}"
    );
    if let Some(url) = art["url"].as_str() {
        // The grant: a signed URL any HTTP client fetches, for five minutes, for this tenant's key.
        assert!(
            url.contains(&format!("tenants/{ta}/objects/{hash}")),
            "{url}"
        );
        let direct = api.http.get(url).send().await.unwrap();
        assert_eq!(direct.status(), 200, "signed URL");
        assert_eq!(direct.bytes().await.unwrap().as_ref(), b"0123456789abcdef");
    } else {
        eprintln!(
            "no object store configured (MODBIT_CLOUD_TEST_S3_ENDPOINT unset): the grant has no URL here"
        );
    }
    api.served.stop();
}

/// docs/33 "Cloud worker lifecycle" (the lease half M8.2 builds on) and
/// docs/31 (append and projection in one transaction): a ready session is
/// claimed by one worker with a strictly greater generation, a second
/// claim gets nothing, the fenced holder cannot renew, a release lets the
/// next claim take the next generation; an event that is not a valid
/// transition is rejected with nothing written; a stale expected sequence
/// conflicts.
#[tokio::test]
async fn qual_m8_1_leases_fence_by_generation_and_an_append_is_all_or_nothing() {
    let Some(api) = Api::start(120).await else {
        return;
    };
    let (ta, secret) = api.tenant("leases").await;
    let a = api.token(&secret).await["access_token"]
        .as_str()
        .unwrap()
        .to_owned();
    let (_, created, _) = api
        .post(
            &a,
            "/v1/sessions",
            json!({"command_id": uuid::Uuid::now_v7().to_string()}),
        )
        .await;
    let sid = SessionId::parse(created["session_id"].as_str().unwrap()).unwrap();
    let (_, task, _) = api
        .post(
            &a,
            &format!("/v1/sessions/{sid}/tasks"),
            json!({"command_id": uuid::Uuid::now_v7().to_string(), "goal_text": "lease me"}),
        )
        .await;
    let tid = TaskId::parse(task["task_id"].as_str().unwrap()).unwrap();
    let store = &api.served.state.store;
    let claimed = store
        .claim_session(sid, "w1", 5_000)
        .await
        .unwrap()
        .expect("a ready session");
    assert_eq!(
        (claimed.session_id, claimed.tenant_id, claimed.generation),
        (sid, ta, 1)
    );
    assert!(
        store
            .claim_session(sid, "w2", 5_000)
            .await
            .unwrap()
            .is_none(),
        "held"
    );
    assert!(store.renew_lease(sid, "w1", 1, 5_000).await.unwrap());
    assert!(
        !store.renew_lease(sid, "w2", 1, 5_000).await.unwrap(),
        "not the holder"
    );
    assert!(store.release_lease(sid, "w1", 1, true).await.unwrap());
    let again = store
        .claim_session(sid, "w2", 5_000)
        .await
        .unwrap()
        .expect("released and ready");
    assert_eq!(
        again.generation, 2,
        "strictly greater: the previous holder is fenced"
    );
    assert!(
        !store.renew_lease(sid, "w1", 1, 5_000).await.unwrap(),
        "fenced"
    );
    // A cancel while a worker holds the session is relayed to that worker
    // (the only writer of the session's log): 202, pending until the owner
    // completes it; a retry answers the same.
    let relayed = uuid::Uuid::now_v7().to_string();
    let (s, body, _) = api
        .post(
            &a,
            &format!("/v1/tasks/{tid}:cancel"),
            json!({"command_id": relayed}),
        )
        .await;
    assert_eq!(
        (
            s.as_u16(),
            body["status"].as_str(),
            body["relayed_to"]["worker_id"].as_str()
        ),
        (202, Some("PENDING"), Some("w2")),
        "{body}"
    );
    let (s, body, replayed) = api
        .post(
            &a,
            &format!("/v1/tasks/{tid}:cancel"),
            json!({"command_id": relayed}),
        )
        .await;
    assert_eq!(
        (s.as_u16(), replayed.as_deref()),
        (202, Some("true")),
        "{body}"
    );
    let (s, rec) = api.get(&a, &format!("/v1/commands/{relayed}")).await;
    assert_eq!(
        (s.as_u16(), rec["status"].as_str(), rec["kind"].as_str()),
        (200, Some("PENDING"), Some("Task:cancel")),
        "{rec}"
    );
    let pending = store.pending_commands(ta, sid, 10).await.unwrap();
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].2["task_id"], json!(tid.to_string()));
    // The owner completes it; the record answers with the owner's outcome.
    assert!(
        store
            .complete_command(
                ta,
                pending[0].0,
                "ACCEPTED",
                "OK",
                json!({"task_id": tid.to_string(), "outcome": "cancelled"})
            )
            .await
            .unwrap()
    );
    let (_, rec) = api.get(&a, &format!("/v1/commands/{relayed}")).await;
    assert_eq!(rec["status"], "ACCEPTED");
    assert!(
        store
            .pending_commands(ta, sid, 10)
            .await
            .unwrap()
            .is_empty()
    );
    // Mirroring under the lease: a fenced owner writes nothing; a live owner's
    // event must continue its aggregate's chain.
    let local_events = {
        let cloud = store.events_after(ta, sid, 0, 100).await.unwrap();
        let last_task = cloud
            .iter()
            .rfind(|e| e.envelope.aggregate_id == *tid.as_bytes())
            .unwrap();
        let mut env = last_task.envelope.clone();
        env.event_id = modbit_domain::EventId::new();
        env.sequence += 1;
        env.event_type = "TaskStarted".into();
        env.payload = modbit_domain::event::PayloadRef::Inline {
            payload: json!({"__type": "TaskStarted"}),
        };
        env.integrity_hash = "0".repeat(64);
        vec![(env, json!({"__type": "TaskStarted"}))]
    };
    let fenced = store.mirror(ta, sid, "w1", 1, local_events.clone()).await;
    assert!(
        matches!(
            fenced,
            Err(modbit_event_store::cloud::CloudError::StaleLease { .. })
        ),
        "{fenced:?}"
    );
    let forged = store.mirror(ta, sid, "w2", 2, local_events).await;
    assert!(
        matches!(
            forged,
            Err(modbit_event_store::cloud::CloudError::Integrity(_))
        ),
        "a hash that does not continue the chain: {forged:?}"
    );
    // All or nothing: an invalid transition writes no event; a stale
    // expectation conflicts; the cursor is unchanged by either.
    let before = store.session(ta, sid).await.unwrap().unwrap().1;
    let bad = store
        .append(
            AppendRequest {
                tenant_id: ta,
                session_id: sid,
                task_id: Some(tid),
                run_id: None,
                turn_id: None,
                step_id: None,
                aggregate_type: AggregateType::Task,
                aggregate_id: *tid.as_bytes(),
                expected_sequence: None,
                events: vec![
                    new_event(
                        "TaskCompleted",
                        &modbit_domain::task::TaskEvent::TaskCompleted,
                        Actor::Core("t".into()),
                    )
                    .unwrap(),
                ],
            },
            None,
        )
        .await;
    assert!(
        matches!(
            bad,
            Err(modbit_event_store::cloud::CloudError::InvalidTransition(_))
        ),
        "{bad:?}"
    );
    let stale = store
        .append(
            AppendRequest {
                tenant_id: ta,
                session_id: sid,
                task_id: Some(tid),
                run_id: None,
                turn_id: None,
                step_id: None,
                aggregate_type: AggregateType::Task,
                aggregate_id: *tid.as_bytes(),
                expected_sequence: Some(1),
                events: vec![
                    new_event(
                        "TaskStarted",
                        &modbit_domain::task::TaskEvent::TaskStarted,
                        Actor::Core("t".into()),
                    )
                    .unwrap(),
                ],
            },
            None,
        )
        .await;
    assert!(
        matches!(
            stale,
            Err(modbit_event_store::cloud::CloudError::SequenceConflict { .. })
        ),
        "{stale:?}"
    );
    assert_eq!(
        store.session(ta, sid).await.unwrap().unwrap().1,
        before,
        "nothing appended"
    );
    let evs = store.events_after(ta, sid, 0, 100).await.unwrap();
    assert!(evs.iter().all(
        |e| e.envelope.event_type != "TaskCompleted" && e.envelope.event_type != "TaskStarted"
    ));
    api.served.stop();
}

/// docs/33: rate limits per principal, before any handler.
#[tokio::test]
async fn qual_m8_1_a_principal_over_its_budget_is_refused_rate_limited() {
    let Some(api) = Api::start(4).await else {
        return;
    };
    let (_t, secret) = api.tenant("rate").await;
    let a = api.token(&secret).await["access_token"]
        .as_str()
        .unwrap()
        .to_owned();
    let (_, created, _) = api
        .post(
            &a,
            "/v1/sessions",
            json!({"command_id": uuid::Uuid::now_v7().to_string()}),
        )
        .await;
    let sid = created["session_id"].as_str().unwrap().to_owned();
    let mut statuses = Vec::new();
    for _ in 0..6 {
        let (s, _) = api.get(&a, &format!("/v1/sessions/{sid}")).await;
        statuses.push(s.as_u16());
    }
    assert!(statuses.contains(&429), "{statuses:?}");
    assert_eq!(statuses[0], 200);
    api.served.stop();
}
