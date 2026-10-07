//! Shared harness for the PX-126 / PX-128 / PX-129 tests on the real cloud
//! API, a real Postgres (`MODBIT_CLOUD_TEST_DATABASE_URL`), a real worker
//! hosting a real `modbit-core` per session, and a scripted
//! OpenAI-compatible model (the repository's standard model stand-in).
//! Kept apart from `cloud_worker.rs` so the two never conflict.
#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use modbit_cloud_api::{Config as ApiConfig, Extras, Served, serve_with};
use modbit_cloud_worker::{Config, ProviderConfig};
use modbit_event_store::cloud::{CloudStoreConfig, S3Config};
use serde_json::{Value, json};

/// A database of this test's own on the configured server.
pub async fn fresh_database(url: &str) -> String {
    let cfg: tokio_postgres::Config = url.parse().expect("database url");
    let (client, conn) = cfg.connect(tokio_postgres::NoTls).await.expect("connect");
    tokio::spawn(async move {
        let _ = conn.await;
    });
    let name = format!("modbit_px_{}", uuid::Uuid::now_v7().simple());
    client
        .execute(&format!("CREATE DATABASE {name}"), &[])
        .await
        .expect("create database");
    let (head, tail) = url.rsplit_once('/').expect("url path");
    let query = tail
        .split_once('?')
        .map(|(_, q)| format!("?{q}"))
        .unwrap_or_default();
    format!("{head}/{name}{query}")
}

pub async fn store_config() -> Option<CloudStoreConfig> {
    let database_url = std::env::var("MODBIT_CLOUD_TEST_DATABASE_URL").ok()?;
    let database_url = fresh_database(&database_url).await;
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
    Some(CloudStoreConfig { database_url, s3 })
}

pub fn core_bin() -> PathBuf {
    if let Ok(p) = std::env::var("MODBIT_CORE_BIN") {
        return PathBuf::from(p);
    }
    let exe = std::env::current_exe().expect("test exe");
    let dir = exe.parent().and_then(|p| p.parent()).expect("target/debug");
    dir.join(if cfg!(windows) {
        "modbit-core.exe"
    } else {
        "modbit-core"
    })
}

fn git(dir: &Path, args: &[&str]) {
    let out = std::process::Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// A committed repository with the no-op mandatory check (FIX-03).
pub fn repo(dir: &Path) -> String {
    std::fs::create_dir_all(dir).unwrap();
    std::fs::write(dir.join("NOTES.md"), "# notes\n").unwrap();
    std::fs::create_dir_all(dir.join(".modbit")).unwrap();
    std::fs::write(
        dir.join(".modbit/verification.json"),
        r#"{"commands": [{"id": "fixture-noop", "argv": ["git", "--version"]}]}"#,
    )
    .unwrap();
    git(dir, &["init", "-q", "-b", "main"]);
    git(dir, &["config", "core.autocrlf", "false"]);
    git(dir, &["add", "-A"]);
    git(
        dir,
        &[
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@e",
            "commit",
            "-q",
            "-m",
            "base",
        ],
    );
    dir.canonicalize()
        .unwrap()
        .to_string_lossy()
        .trim_start_matches(r"\\?\")
        .to_owned()
}

/// A branch at HEAD (the one Modbit pushes for a task's pull request).
pub fn branch(dir: &str, name: &str) -> String {
    git(Path::new(dir), &["branch", name]);
    let out = std::process::Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["rev-parse", name])
        .output()
        .unwrap();
    String::from_utf8_lossy(&out.stdout).trim().to_owned()
}

/// A scripted OpenAI-compatible provider: the n-th request that follows a
/// tool-calling turn answers with the n-th step.
pub async fn scripted_model(script: Vec<Value>) -> (String, Arc<Mutex<Vec<Value>>>) {
    use axum::{Router, routing::post};
    let seen: Arc<Mutex<Vec<Value>>> = Default::default();
    let seen2 = seen.clone();
    let script = Arc::new(script);
    let app = Router::new().route(
        "/v1/chat/completions",
        post(move |axum::Json(body): axum::Json<Value>| {
            let script = Arc::clone(&script);
            let seen = seen2.clone();
            async move {
                let results = body["messages"].as_array().map_or(0, |m| {
                    m.iter()
                        .filter(|x| x["role"] == "assistant" && !x["tool_calls"].is_null())
                        .count()
                });
                seen.lock().unwrap().push(body.clone());
                let step = script.get(results).cloned().unwrap_or(json!({}));
                let mut out = String::new();
                let calls = step["calls"].as_array().cloned().unwrap_or_default();
                for (i, c) in calls.iter().enumerate() {
                    let frame = json!({"id": "c", "model": "scripted", "choices": [{"index": 0, "delta": {"tool_calls": [{"index": i, "id": format!("call_{results}_{i}"), "type": "function", "function": {"name": c["name"], "arguments": c["args"].to_string()}}]}, "finish_reason": null}]});
                    out.push_str(&format!("data: {frame}\n\n"));
                }
                if calls.is_empty() {
                    out.push_str(&format!("data: {}\n\n", json!({"id": "c", "model": "scripted", "choices": [{"index": 0, "delta": {"content": "Nothing further."}, "finish_reason": null}]})));
                }
                out.push_str(&format!("data: {}\n\n", json!({"id": "c", "model": "scripted", "choices": [{"index": 0, "delta": {}, "finish_reason": if calls.is_empty() { "stop" } else { "tool_calls" }}], "usage": {"prompt_tokens": 10, "completion_tokens": 5}})));
                out.push_str("data: [DONE]\n\n");
                ([(axum::http::header::CONTENT_TYPE, "text/event-stream")], out)
            }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    (format!("http://{addr}"), seen)
}

/// The cloud API over the store, with a tenant, a principal and its token.
pub struct Api {
    pub base: String,
    pub http: reqwest::Client,
    pub served: Served,
}

impl Api {
    pub async fn start(store: &CloudStoreConfig, webhook_secret: Option<&[u8]>) -> Api {
        Self::start_with(store, webhook_secret, Extras::default()).await
    }

    pub async fn start_with(
        store: &CloudStoreConfig,
        webhook_secret: Option<&[u8]>,
        extras: Extras,
    ) -> Api {
        let served = serve_with(
            ApiConfig {
                store: store.clone(),
                token_key: Some(vec![9u8; 32]),
                bind: "127.0.0.1:0".into(),
                rate_capacity: 1000,
                rate_per_second: 200.0,
                worker_key: None,
                github_webhook_secret: webhook_secret.map(<[u8]>::to_vec),
            },
            extras,
        )
        .await
        .expect("api");
        Api {
            base: format!("http://{}", served.addr),
            http: reqwest::Client::new(),
            served,
        }
    }

    /// A tenant with one user principal; returns its access token.
    pub async fn tenant(&self, name: &str) -> (modbit_domain::TenantId, String) {
        let store = &self.served.state.store;
        let tenant = store.create_tenant(name).await.unwrap();
        let (_p, secret) = store.create_principal(tenant, "user", name).await.unwrap();
        let (_, tok) = self
            .post("", "/v1/auth/token", json!({"secret": secret}))
            .await;
        (tenant, tok["access_token"].as_str().unwrap().to_owned())
    }

    pub async fn post(&self, token: &str, path: &str, body: Value) -> (u16, Value) {
        let r = self
            .http
            .post(format!("{}{path}", self.base))
            .bearer_auth(token)
            .json(&body)
            .send()
            .await
            .unwrap();
        (r.status().as_u16(), r.json().await.unwrap_or(Value::Null))
    }

    pub async fn get(&self, token: &str, path: &str) -> (u16, Value) {
        let r = self
            .http
            .get(format!("{}{path}", self.base))
            .bearer_auth(token)
            .send()
            .await
            .unwrap();
        (r.status().as_u16(), r.json().await.unwrap_or(Value::Null))
    }

    /// All events of a session as `(type, payload)`.
    pub async fn events(&self, token: &str, session: &str) -> Vec<(String, Value)> {
        let (_, evs) = self
            .get(
                token,
                &format!("/v1/events?session_id={session}&after=0&limit=5000"),
            )
            .await;
        evs["events"]
            .as_array()
            .map(|a| {
                a.iter()
                    .map(|e| {
                        (
                            e["envelope"]["event_type"]
                                .as_str()
                                .unwrap_or_default()
                                .to_owned(),
                            e["payload"].clone(),
                        )
                    })
                    .collect()
            })
            .unwrap_or_default()
    }
}

/// Record, on the task's cloud log, that its Core opened pull request
/// `number` of `repo` on `head` — the `ForgePullRequestOpened` a Core
/// writes (the pull-request path itself is PX-007's, proven locally).
#[allow(clippy::too_many_arguments)]
pub async fn seed_pull_request(
    api: &Api,
    tenant: modbit_domain::TenantId,
    session: &str,
    task: &str,
    repo: &str,
    number: u64,
    head: &str,
    head_sha: &str,
) {
    use modbit_domain::event::{Actor, AggregateType};
    use modbit_domain::task::TaskEvent;
    let task = modbit_domain::TaskId::parse(task).unwrap();
    let (owner, name) = repo.split_once('/').unwrap();
    let ev = modbit_event_store::cloud::new_event(
        "ForgePullRequestOpened",
        &TaskEvent::ForgePullRequestOpened {
            idempotency_key: format!("seed-{number}-{repo}"),
            owner: owner.into(),
            repo: name.into(),
            number,
            url: format!("https://github.test/{repo}/pull/{number}"),
            head: head.into(),
            head_sha: head_sha.into(),
            base: "main".into(),
            result: json!({}),
        },
        Actor::External("test".into()),
    )
    .unwrap();
    api.served
        .state
        .store
        .append(
            modbit_event_store::AppendRequest {
                tenant_id: tenant,
                session_id: modbit_domain::SessionId::parse(session).unwrap(),
                task_id: Some(task),
                run_id: None,
                turn_id: None,
                step_id: None,
                aggregate_type: AggregateType::Task,
                aggregate_id: *task.as_bytes(),
                expected_sequence: None,
                events: vec![ev],
            },
            None,
        )
        .await
        .unwrap();
}

/// `sha256=<hex hmac>` of a webhook body, as GitHub signs it.
pub fn sign(secret: &[u8], body: &[u8]) -> String {
    use hmac::{Hmac, KeyInit, Mac};
    let mut mac = Hmac::<sha2::Sha256>::new_from_slice(secret).unwrap();
    mac.update(body);
    format!("sha256={}", hex::encode(mac.finalize().into_bytes()))
}

/// An ISO-8601 time `secs_ago` seconds before now (GitHub's form).
pub fn iso(secs_ago: i64) -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    let secs = now - secs_ago;
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z",
        rem / 3600,
        (rem % 3600) / 60,
        rem % 60
    )
}

pub async fn until<T>(what: &str, secs: u64, mut probe: impl AsyncFnMut() -> Option<T>) -> T {
    let deadline = Instant::now() + Duration::from_secs(secs);
    loop {
        if let Some(v) = probe().await {
            return v;
        }
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
}

/// A worker config with no sandbox gateway (tasks the test makes run in the
/// profile it names on the host, as a self-hosted worker's `local_trusted`
/// tasks do) and the forge the tests point at.
pub fn worker_config(
    store: &CloudStoreConfig,
    id: &str,
    data_dir: &Path,
    model_base: &str,
    forge: Option<(&str, &str)>,
) -> Config {
    Config {
        store: store.clone(),
        worker_id: id.into(),
        data_dir: data_dir.to_path_buf(),
        core_bin: core_bin(),
        lease_ttl: Duration::from_secs(10),
        poll: Duration::from_millis(300),
        endpoint: Some("openai".into()),
        model: Some("gpt-5".into()),
        capacity: 2,
        provider: Some(ProviderConfig {
            provider: "openai".into(),
            api_key: String::new(),
            base_url: model_base.to_owned(),
        }),
        sandbox_gateway: None,
        forge: forge.map(|(base, token)| modbit_cloud_worker::ForgeConfig {
            api_base_url: base.to_owned(),
            token: token.to_owned(),
        }),
        api: None,
        capabilities: None,
    }
}
