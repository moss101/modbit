//! M8.2 qualification (docs/24 "Cloud Core Worker", docs/33 "Session kernel
//! lease", "Cloud worker lifecycle"): a real worker over a real Postgres and
//! an S3-compatible store claims the session's fenced lease, spawns the real `modbit-core`
//! in the session's tenant, materializes the cloud log there, runs the
//! queued task with a real (scripted) provider, executes the commands the
//! API relayed, mirrors every local event verbatim to the cloud log; a
//! successor claims the released session at the next generation and
//! resumes from the same log without duplicating anything; an owner whose
//! lease was taken is fenced and stops.
//!
//! Runs only where `MODBIT_CLOUD_TEST_DATABASE_URL` names a database (the
//! hosted `cloud` job); the built `modbit-core` is found through
//! `MODBIT_CORE_BIN` or beside the test binary.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use modbit_cloud_api::{Config as ApiConfig, serve};
use modbit_cloud_worker::{Config, Hosting, ProviderConfig, SandboxGatewayConfig, start};
use modbit_event_store::cloud::{CloudStoreConfig, S3Config};
use prost::Message;
use serde_json::{Value, json};

/// A database of this test's own on the configured server (the API's
/// qualification leaves ready sessions of other tenants behind on the
/// shared one; a worker would host those first).
async fn fresh_database(url: &str) -> String {
    let cfg: tokio_postgres::Config = url.parse().expect("database url");
    let (client, conn) = cfg.connect(tokio_postgres::NoTls).await.expect("connect");
    tokio::spawn(async move {
        let _ = conn.await;
    });
    let name = format!("modbit_worker_{}", uuid::Uuid::now_v7().simple());
    client
        .execute(&format!("CREATE DATABASE {name}"), &[])
        .await
        .expect("create database");
    let base = cfg.get_dbname().unwrap_or("modbit");
    // The URL's path is the database name: swap it.
    let (head, _) = url.rsplit_once('/').expect("url path");
    let tail = url.rsplit_once('/').map(|(_, t)| t).unwrap_or_default();
    let query = tail
        .split_once('?')
        .map(|(_, q)| format!("?{q}"))
        .unwrap_or_default();
    assert!(tail.starts_with(base));
    format!("{head}/{name}{query}")
}

async fn store_config() -> Option<CloudStoreConfig> {
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

/// A committed repository for the task's workspace (the cloud's own
/// sandboxed workspace arrives with M8.3; here the worker's host holds it).
fn repo(dir: &std::path::Path) -> String {
    std::fs::create_dir_all(dir).unwrap();
    std::fs::write(dir.join("NOTES.md"), "# notes\n").unwrap();
    for args in [
        vec!["init", "-q", "-b", "main"],
        vec!["config", "core.autocrlf", "false"],
        vec!["add", "-A"],
        vec![
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@e",
            "commit",
            "-q",
            "-m",
            "base",
        ],
    ] {
        assert!(
            std::process::Command::new("git")
                .arg("-C")
                .arg(dir)
                .args(&args)
                .status()
                .unwrap()
                .success()
        );
    }
    dir.canonicalize()
        .unwrap()
        .to_string_lossy()
        .trim_start_matches(r"\\?\")
        .to_owned()
}

fn core_bin() -> PathBuf {
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

/// A scripted OpenAI-compatible provider: the n-th request (by the number
/// of tool results it carries) answers with the n-th step's tool calls.
async fn scripted_model(script: Vec<Value>) -> (String, Arc<Mutex<Vec<Value>>>) {
    let (base, seen, _gate) = scripted_model_gated(script).await;
    (base, seen)
}

/// A scripted model whose steps marked `{"gate": true}` are held until the
/// test opens the gate (a run parked mid-model-call, for the handoff).
async fn scripted_model_gated(
    script: Vec<Value>,
) -> (String, Arc<Mutex<Vec<Value>>>, Arc<tokio::sync::Notify>) {
    use axum::{Router, routing::post};
    let seen: Arc<Mutex<Vec<Value>>> = Default::default();
    let seen2 = seen.clone();
    let script = Arc::new(script);
    let gate = Arc::new(tokio::sync::Notify::new());
    let gate2 = Arc::clone(&gate);
    // A conversation that arrives with history (a handoff's continuation)
    // starts the script at its first request.
    let base: Arc<Mutex<Option<usize>>> = Default::default();
    let app = Router::new().route(
        "/v1/chat/completions",
        post(move |axum::Json(body): axum::Json<Value>| {
            let script = Arc::clone(&script);
            let seen = seen2.clone();
            let gate = Arc::clone(&gate2);
            let base = Arc::clone(&base);
            async move {
                // The n-th step answers the n-th request that follows a tool-calling
                // turn (a step may carry several calls).
                let results = body["messages"].as_array().map_or(0, |m| {
                    m.iter()
                        .filter(|x| x["role"] == "assistant" && !x["tool_calls"].is_null())
                        .count()
                });
                seen.lock().unwrap().push(body.clone());
                let offset = *base.lock().unwrap().get_or_insert(results);
                let results = results.saturating_sub(offset);
                let step = script.get(results).cloned().unwrap_or(json!({}));
                if step["gate"].as_bool().unwrap_or(false) {
                    gate.notified().await;
                }
                let mut out = String::new();
                let calls = step["calls"].as_array().cloned().unwrap_or_default();
                for (i, c) in calls.iter().enumerate() {
                    // M8.8: `{"$ref": {"role", "name"}}` in the arguments is
                    // the ref of that entity in the latest compiled page the
                    // model saw (what a real model reads off the snapshot).
                    let args = resolve_refs(c["args"].clone(), &body["messages"]);
                    let frame = json!({"id": "c", "model": "scripted", "choices": [{"index": 0, "delta": {"tool_calls": [{"index": i, "id": format!("call_{results}_{i}"), "type": "function", "function": {"name": c["name"], "arguments": args.to_string()}}]}, "finish_reason": null}]});
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
    (format!("http://{addr}"), seen, gate)
}

/// The entities of the latest compiled page in the conversation's tool
/// results (newest first), as `(role, name, ref, bounds)`.
fn latest_entities(messages: &Value) -> Vec<Value> {
    let Some(msgs) = messages.as_array() else {
        return vec![];
    };
    for m in msgs.iter().rev() {
        if m["role"] != "tool" {
            continue;
        }
        let text = m["content"].as_str().unwrap_or_default();
        let Some(json_text) = text.split("output:\n").nth(1) else {
            continue;
        };
        let Ok(v) = serde_json::from_str::<Value>(json_text.trim()) else {
            continue;
        };
        for key in ["page", "after"] {
            if let Some(es) = v[key]["entities"].as_array() {
                return es.clone();
            }
        }
        if let Some(es) = v["entities"].as_array() {
            return es.clone();
        }
    }
    vec![]
}

fn resolve_refs(args: Value, messages: &Value) -> Value {
    match args {
        Value::Object(map) => {
            if let Some(want) = map.get("$ref") {
                let entities = latest_entities(messages);
                let found = entities.iter().find(|e| {
                    e["role"] == want["role"]
                        && e["name"]
                            .as_str()
                            .is_some_and(|n| n.contains(want["name"].as_str().unwrap_or_default()))
                });
                return match found {
                    Some(e) => e["ref"].clone(),
                    None => json!("000000000000"),
                };
            }
            Value::Object(
                map.into_iter()
                    .map(|(k, v)| (k, resolve_refs(v, messages)))
                    .collect(),
            )
        }
        Value::Array(a) => Value::Array(a.into_iter().map(|v| resolve_refs(v, messages)).collect()),
        other => other,
    }
}

struct Api {
    base: String,
    http: reqwest::Client,
    served: modbit_cloud_api::Served,
}

impl Api {
    async fn post(&self, token: &str, path: &str, body: Value) -> (u16, Value) {
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
    async fn get(&self, token: &str, path: &str) -> (u16, Value) {
        let r = self
            .http
            .get(format!("{}{path}", self.base))
            .bearer_auth(token)
            .send()
            .await
            .unwrap();
        (r.status().as_u16(), r.json().await.unwrap_or(Value::Null))
    }
}

async fn until<T>(what: &str, secs: u64, mut probe: impl AsyncFnMut() -> Option<T>) -> T {
    let deadline = Instant::now() + Duration::from_secs(secs);
    loop {
        if let Some(v) = probe().await {
            return v;
        }
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        tokio::time::sleep(Duration::from_millis(300)).await;
    }
}

fn worker_config(
    store: &CloudStoreConfig,
    id: &str,
    data_dir: &std::path::Path,
    model_base: &str,
    lease_ttl: Duration,
    gateway: &Gateway,
) -> Config {
    worker_config_with(store, id, data_dir, model_base, lease_ttl, gateway, None)
}

/// A fake forge on loopback (M8.6): answers `/user` with `authorized: true`
/// only when the `Authorization` header carries the real token, and keeps
/// what it saw.
struct FakeForge {
    base_url: String,
    token: String,
    seen: Arc<Mutex<Vec<String>>>,
}

async fn fake_forge() -> FakeForge {
    use axum::{Router, routing::get};
    let token = format!("ghp-real-{}", uuid::Uuid::now_v7().simple());
    let expected = format!("Bearer {token}");
    let seen: Arc<Mutex<Vec<String>>> = Default::default();
    let seen2 = Arc::clone(&seen);
    // M8.8: a page on the admitted host for the cloud browser — a form
    // whose link greets whoever was typed, and the greeting.
    let form_html = "<!doctype html><html><head><title>Greeter</title></head><body><main><h1>Greeter</h1><label>Your name <input id=who name=who></label> <a id=go href=\"/greet?who=\">Greet</a><script>document.getElementById('who').addEventListener('input', e => { document.getElementById('go').href = '/greet?who=' + encodeURIComponent(e.target.value); });</script></main></body></html>";
    let app = Router::new()
        .route("/form", get(move || async move { axum::response::Html(form_html) }))
        .route(
            "/greet",
            get(|q: axum::extract::Query<std::collections::HashMap<String, String>>| async move {
                let who = q.get("who").cloned().unwrap_or_default();
                let who: String = who.chars().filter(|c| c.is_alphanumeric() || *c == ' ').collect();
                axum::response::Html(format!("<!doctype html><html><head><title>Greeted</title></head><body><main><h1>hello, {who}</h1><a id=again href=\"/form\">Again</a></main></body></html>"))
            }),
        )
        .route(
        "/user",
        get(move |headers: axum::http::HeaderMap| {
            let expected = expected.clone();
            let seen = Arc::clone(&seen2);
            async move {
                let got = headers
                    .get("authorization")
                    .and_then(|v| v.to_str().ok())
                    .unwrap_or_default()
                    .to_owned();
                seen.lock().unwrap().push(got.clone());
                if got == expected {
                    (
                        axum::http::StatusCode::OK,
                        "{\"login\":\"ada\",\"authorized\":true}",
                    )
                } else {
                    (
                        axum::http::StatusCode::UNAUTHORIZED,
                        "{\"authorized\":false}",
                    )
                }
            }
        }),
    );
    let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = l.local_addr().unwrap();
    tokio::spawn(async move {
        let _ = axum::serve(l, app).await;
    });
    FakeForge {
        base_url: format!("http://{addr}"),
        token,
        seen,
    }
}

fn worker_config_with(
    store: &CloudStoreConfig,
    id: &str,
    data_dir: &std::path::Path,
    model_base: &str,
    lease_ttl: Duration,
    gateway: &Gateway,
    forge: Option<&FakeForge>,
) -> Config {
    Config {
        store: store.clone(),
        worker_id: id.into(),
        data_dir: data_dir.to_path_buf(),
        core_bin: core_bin(),
        lease_ttl,
        poll: Duration::from_millis(300),
        endpoint: Some("openai".into()),
        model: Some("gpt-5".into()),
        capacity: 2,
        provider: Some(ProviderConfig {
            provider: "openai".into(),
            api_key: String::new(),
            base_url: model_base.to_owned(),
        }),
        sandbox_gateway: Some(SandboxGatewayConfig {
            base_url: gateway.base_url.clone(),
            worker_token: gateway.token_for(id),
        }),
        forge: forge.map(|f| modbit_cloud_worker::ForgeConfig {
            api_base_url: f.base_url.clone(),
            token: f.token.clone(),
        }),
        api: None,
        capabilities: None,
    }
}

fn guest_bin() -> PathBuf {
    if let Ok(p) = std::env::var("MODBIT_GUEST_BIN") {
        return PathBuf::from(p);
    }
    let exe = std::env::current_exe().expect("test exe");
    let dir = exe.parent().and_then(|p| p.parent()).expect("target/debug");
    dir.join(if cfg!(windows) {
        "modbit-guest.exe"
    } else {
        "modbit-guest"
    })
}

async fn gw_get(gateway: &Gateway, worker: &str, path: &str) -> (u16, Value) {
    let r = reqwest::Client::new()
        .get(format!("{}{path}", gateway.base_url))
        .bearer_auth(gateway.token_for(worker))
        .send()
        .await
        .unwrap();
    (r.status().as_u16(), r.json().await.unwrap_or(Value::Null))
}

/// The Sandbox Gateway the workers' Cores provision from (M8.5): the
/// MicroVM backend where the job provides Firecracker, the kernel and the
/// signed image; the reference backend (its guest signed by a key of the
/// test's own) elsewhere.
struct Gateway {
    base_url: String,
    served: modbit_sandbox_gateway::Served,
    /// `microvm` | `reference`.
    backend: &'static str,
    /// The worker key bytes the gateway was given.
    key: Vec<u8>,
}

impl Gateway {
    async fn start(store: &CloudStoreConfig, dir: &std::path::Path) -> Self {
        std::fs::create_dir_all(dir).unwrap();
        #[cfg(unix)]
        let microvm = modbit_sandbox::backend::microvm::MicrovmConfig::from_env()
            .filter(|c| c.unavailable_reason().is_none())
            .map(|c| modbit_sandbox::backend::microvm::MicrovmConfig {
                work_dir: dir.join("vms"),
                ..c
            });
        #[cfg(not(unix))]
        let microvm: Option<()> = None;
        let (backend, kind) = match microvm {
            #[cfg(unix)]
            Some(c) => (modbit_sandbox_gateway::BackendChoice::Microvm(c), "microvm"),
            _ => {
                let bin = guest_bin();
                assert!(bin.is_file(), "modbit-guest at {}", bin.display());
                let key = modbit_sandbox::image::fresh_signing_key();
                let (sha256, size) = modbit_sandbox::image::sha256_file(&bin).unwrap();
                let manifest = modbit_sandbox::image::ImageManifest {
                    kind: "reference-guest".into(),
                    sha256,
                    size,
                    guest_version: env!("CARGO_PKG_VERSION").into(),
                    guest_protocol: format!(
                        "{}.{}",
                        modbit_sandbox::GUEST_PROTOCOL_MAJOR,
                        modbit_sandbox::GUEST_PROTOCOL_MINOR
                    ),
                    kernel_sha256: String::new(),
                    built_from: "cloud_worker.rs".into(),
                    built_at_ms: 1,
                };
                let signed = modbit_sandbox::image::sign(&manifest, "test-publisher", &key);
                let path = dir.join("guest.manifest.json");
                std::fs::write(&path, serde_json::to_string(&signed).unwrap()).unwrap();
                (
                    modbit_sandbox_gateway::BackendChoice::Reference {
                        guest_bin: bin,
                        work_dir: dir.join("sandboxes"),
                        manifest: Some(path),
                        trusted_keys: vec![(
                            "test-publisher".to_owned(),
                            key.verifying_key().to_bytes(),
                        )],
                        chromium: modbit_sandbox::backend::reference::detect_chromium(),
                    },
                    "reference",
                )
            }
        };
        let key: Vec<u8> = uuid::Uuid::now_v7()
            .as_bytes()
            .iter()
            .chain(uuid::Uuid::new_v4().as_bytes())
            .copied()
            .collect();
        let served = modbit_sandbox_gateway::serve(modbit_sandbox_gateway::Config {
            store: store.clone(),
            worker_key: Some(modbit_sandbox::auth::WorkerKey::new(key.clone())),
            bind: "127.0.0.1:0".into(),
            backend,
        })
        .await
        .expect("gateway");
        eprintln!("test gateway: {kind} backend at {}", served.addr);
        Self {
            base_url: format!("http://{}", served.addr),
            served,
            backend: kind,
            key,
        }
    }

    /// The worker key's bytes (the API verifies worker tokens under the same key).
    fn key_bytes(&self) -> Vec<u8> {
        self.key.clone()
    }

    fn token_for(&self, worker_id: &str) -> String {
        self.served
            .state
            .worker_key
            .issue(&modbit_sandbox::auth::WorkerClaims {
                worker_id: worker_id.into(),
                exp_ms: i64::MAX,
            })
    }
}

#[tokio::test]
async fn qual_m8_2_a_worker_claims_the_lease_runs_the_task_relays_commands_mirrors_the_log_and_a_successor_resumes_fenced()
 {
    let Some(store_cfg) = store_config().await else {
        eprintln!(
            "SKIPPED: MODBIT_CLOUD_TEST_DATABASE_URL unset (the hosted cloud job runs this against Postgres and an S3-compatible store)"
        );
        return;
    };
    assert!(
        core_bin().exists(),
        "modbit-core at {} (build it, or set MODBIT_CORE_BIN)",
        core_bin().display()
    );
    let script = vec![
        json!({"calls": [{"name": "plan.update", "args": {"outcome": "the notes are read", "expected_files": []}}]}),
        // A process under cloud_isolated runs inside the task's sandbox (M8.5):
        // nothing of the worker's environment is there.
        json!({"calls": [{"name": "shell.exec", "args": {"argv": ["/bin/sh", "-c", "env; echo sandboxed=$(hostname 2>/dev/null || echo unknown)"], "inherit_env": true}}]}),
        json!({"calls": [{"name": "task.complete", "args": {"summary": "read the notes", "self_review": {"findings": []}}}]}),
    ];
    let (model_base, seen) = scripted_model(script).await;
    let served = serve(ApiConfig {
        store: store_cfg.clone(),
        token_key: None,
        bind: "127.0.0.1:0".into(),
        rate_capacity: 500,
        rate_per_second: 100.0,
        worker_key: None,
        github_webhook_secret: None,
    })
    .await
    .expect("api");
    let api = Api {
        base: format!("http://{}", served.addr),
        http: reqwest::Client::new(),
        served,
    };
    let tenant = api
        .served
        .state
        .store
        .create_tenant("worker-test")
        .await
        .unwrap();
    let (_p, secret) = api
        .served
        .state
        .store
        .create_principal(tenant, "user", "ada")
        .await
        .unwrap();
    let (_, tok) = api
        .post("", "/v1/auth/token", json!({"secret": secret}))
        .await;
    let a = tok["access_token"].as_str().unwrap().to_owned();
    let keep = tempfile::tempdir().unwrap();
    let data = match std::env::var("MODBIT_CLOUD_WORKER_TEST_KEEP_DIR") {
        Ok(d) => std::path::PathBuf::from(d),
        Err(_) => keep.path().to_path_buf(),
    };
    let root = repo(&data.join("repo"));
    let gateway = Gateway::start(&store_cfg, &data.join("gateway")).await;
    let (_, created) = api
        .post(
            &a,
            "/v1/sessions",
            json!({"command_id": uuid::Uuid::now_v7().to_string()}),
        )
        .await;
    let sid = created["session_id"].as_str().unwrap().to_owned();
    let c_task = uuid::Uuid::now_v7().to_string();
    let (s, task) = api.post(&a, &format!("/v1/sessions/{sid}/tasks"), json!({"command_id": c_task, "goal_text": "read the notes", "execution_profile": "cloud_isolated", "workspace_root": root})).await;
    assert_eq!(s, 201, "{task}");
    let tid = task["task_id"].as_str().unwrap().to_owned();
    assert_eq!(
        tid.replace('-', ""),
        c_task.replace('-', ""),
        "the task is named by its creating command"
    );
    // Worker A claims the session and runs the task.
    let worker_a = start(worker_config(
        &store_cfg,
        "worker-a",
        &data.join("a"),
        &model_base,
        Duration::from_secs(10),
        &gateway,
    ))
    .await
    .expect("worker a");
    let view = until("worker a to hold the session", 60, async || {
        let (_, v) = api.get(&a, &format!("/v1/sessions/{sid}")).await;
        (v["lease"]["worker_id"] == "worker-a").then_some(v)
    })
    .await;
    assert_eq!(view["lease"]["generation"], 1);
    let view = until("the task to reach review on the worker", 120, async || {
        let (_, v) = api.get(&a, &format!("/v1/sessions/{sid}")).await;
        (v["tasks"][0]["state"] == "READY_FOR_REVIEW").then_some(v)
    })
    .await;
    assert_eq!(
        view["tasks"][0]["task_id"]
            .as_str()
            .unwrap()
            .replace('-', ""),
        tid.replace('-', "")
    );
    // The cloud log is the worker's Core's log, mirrored: the run's events
    // are there, in order, chain-verified on the way in.
    let (_, evs) = api
        .get(
            &a,
            &format!("/v1/events?session_id={sid}&after=0&limit=1000"),
        )
        .await;
    let types: Vec<String> = evs["events"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["envelope"]["event_type"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(
        &types[..3],
        &["SessionCreated", "TaskCreated", "TaskQueued"],
        "{types:?}"
    );
    for needed in [
        "SessionLeaseAcquired",
        "TaskStarted",
        "RunCreated",
        "PlanRecorded",
        "TaskReadyForReview",
    ] {
        assert!(
            types.iter().any(|t| t == needed),
            "{needed} mirrored: {types:?}"
        );
    }
    let bodies = seen.lock().unwrap().clone();
    assert!(
        bodies.len() >= 3,
        "the worker's Core called the provider: {}",
        bodies.len()
    );
    let last = bodies.last().unwrap();
    let tool_texts: Vec<String> = last["messages"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|m| m["role"] == "tool")
        .map(|m| m["content"].as_str().unwrap_or_default().to_owned())
        .collect();
    assert!(
        tool_texts[1].contains("status: SUCCESS")
            && !tool_texts[1].contains("MODBIT_")
            && !tool_texts[1].contains("OPENAI_"),
        "the process ran inside the sandbox with nothing of the worker's environment ({} backend): {}",
        gateway.backend,
        tool_texts[1]
    );
    assert!(
        types.iter().any(|t| t == "SandboxLeaseAcquired"),
        "the task's sandbox is on the cloud log: {types:?}"
    );
    // Relayed commands: while worker A holds the session the API records
    // them pending; the worker executes them on its Core; the log shows it.
    let c_steer = uuid::Uuid::now_v7().to_string();
    let (s, relayed) = api
        .post(
            &a,
            &format!("/v1/tasks/{tid}:steer"),
            json!({"command_id": c_steer, "text": "also the changelog"}),
        )
        .await;
    assert_eq!(
        (
            s,
            relayed["status"].as_str(),
            relayed["relayed_to"]["worker_id"].as_str()
        ),
        (202, Some("PENDING"), Some("worker-a")),
        "{relayed}"
    );
    let done = until("the steer to complete", 30, async || {
        let (_, r) = api.get(&a, &format!("/v1/commands/{c_steer}")).await;
        (r["status"] != "PENDING").then_some(r)
    })
    .await;
    assert_eq!(
        (
            done["status"].as_str(),
            done["result"]["event_type"].as_str()
        ),
        (Some("ACCEPTED"), Some("TaskInputQueued")),
        "{done}"
    );
    let (_, evs) = api
        .get(
            &a,
            &format!("/v1/events?session_id={sid}&after=0&limit=1000"),
        )
        .await;
    assert!(
        evs["events"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["envelope"]["event_type"] == "TaskInputQueued"),
        "the relayed steer is on the cloud log"
    );
    let c_pause = uuid::Uuid::now_v7().to_string();
    let (s, _) = api
        .post(
            &a,
            &format!("/v1/tasks/{tid}:pause"),
            json!({"command_id": c_pause}),
        )
        .await;
    assert_eq!(s, 202);
    let paused = until("the pause to be answered", 30, async || {
        let (_, r) = api.get(&a, &format!("/v1/commands/{c_pause}")).await;
        (r["status"] != "PENDING").then_some(r)
    })
    .await;
    assert_eq!(
        (paused["status"].as_str(), paused["code"].as_str()),
        (Some("REJECTED"), Some("PAUSE_UNSUPPORTED")),
        "{paused}"
    );
    // A second task created while held is relayed: the worker creates and runs it.
    let c_task2 = uuid::Uuid::now_v7().to_string();
    let (s, relayed) = api.post(&a, &format!("/v1/sessions/{sid}/tasks"), json!({"command_id": c_task2, "goal_text": "read them again", "execution_profile": "cloud_isolated", "workspace_root": root})).await;
    assert_eq!(
        (s, relayed["status"].as_str()),
        (202, Some("PENDING")),
        "{relayed}"
    );
    let created2 = until(
        "the second task to be created by the worker",
        30,
        async || {
            let (_, r) = api.get(&a, &format!("/v1/commands/{c_task2}")).await;
            (r["status"] != "PENDING").then_some(r)
        },
    )
    .await;
    assert_eq!(created2["status"], "ACCEPTED", "{created2}");
    assert_eq!(
        created2["result"]["task_id"].as_str().unwrap(),
        c_task2.replace('-', ""),
        "named by its command"
    );
    until("the second task to reach review", 120, async || {
        let (_, v) = api.get(&a, &format!("/v1/sessions/{sid}")).await;
        (v["tasks"].as_array().unwrap().len() == 2 && v["tasks"][1]["state"] == "READY_FOR_REVIEW")
            .then_some(())
    })
    .await;
    // Fencing: the lease is taken from under worker A (its expiry cut, a
    // successor claims); A's next renewal fails and it stops its Core; B
    // resumes the same session at generation 2 from the same log, imports
    // it into a fresh Core and adds only its own lease event.
    let store = &api.served.state.store;
    let (_, before) = api
        .get(
            &a,
            &format!("/v1/events?session_id={sid}&after=0&limit=1000"),
        )
        .await;
    let n_before = before["events"].as_array().unwrap().len();
    let sid_typed = modbit_domain::SessionId::parse(&sid).unwrap();
    // (Cut the expiry as an expired heartbeat would; the successor claims.)
    {
        let cfg: tokio_postgres::Config = store_cfg.database_url.parse().unwrap();
        let (client, conn) = cfg.connect(tokio_postgres::NoTls).await.unwrap();
        tokio::spawn(async move {
            let _ = conn.await;
        });
        client
            .execute(
                "UPDATE session_leases SET expires_at_ms = 0 WHERE session_id = $1",
                &[&uuid::Uuid::from_bytes(*sid_typed.as_bytes())],
            )
            .await
            .unwrap();
    }
    let worker_b = start(worker_config(
        &store_cfg,
        "worker-b",
        &data.join("b"),
        &model_base,
        Duration::from_secs(10),
        &gateway,
    ))
    .await
    .expect("worker b");
    let view = until("worker b to hold the session", 60, async || {
        let (_, v) = api.get(&a, &format!("/v1/sessions/{sid}")).await;
        (v["lease"]["worker_id"] == "worker-b").then_some(v)
    })
    .await;
    assert_eq!(view["lease"]["generation"], 2);
    // B's Core materialized the whole log and took its own local lease: one
    // new event, nothing duplicated, every aggregate's chain intact.
    let after = until("worker b's lease event to mirror", 60, async || {
        let (_, v) = api
            .get(
                &a,
                &format!("/v1/events?session_id={sid}&after=0&limit=1000"),
            )
            .await;
        let n = v["events"].as_array().unwrap().len();
        (n > n_before).then_some(v)
    })
    .await;
    let events = after["events"].as_array().unwrap();
    assert_eq!(
        events.len(),
        n_before + 1,
        "exactly B's SessionLeaseAcquired joined the log"
    );
    assert_eq!(
        events.last().unwrap()["envelope"]["event_type"],
        "SessionLeaseAcquired"
    );
    let mut per_aggregate: std::collections::HashMap<String, Vec<u64>> = Default::default();
    for e in events {
        per_aggregate
            .entry(e["envelope"]["aggregate_id"].to_string())
            .or_default()
            .push(e["envelope"]["sequence"].as_u64().unwrap());
    }
    for (agg, seqs) in per_aggregate {
        let expected: Vec<u64> = (1..=seqs.len() as u64).collect();
        assert_eq!(seqs, expected, "aggregate {agg} contiguous");
    }
    // A is fenced: its next heartbeat is refused, its Core stops, and
    // stopping it releases nothing (B holds the lease).
    let fenced = until(
        "worker a to notice it is fenced",
        30,
        async || match worker_a.hosting(sid_typed) {
            Some(Hosting::Fenced { generation }) => Some(generation),
            _ => None,
        },
    )
    .await;
    assert_eq!(fenced, 1);
    worker_a.stop().await;
    let (_, view) = api.get(&a, &format!("/v1/sessions/{sid}")).await;
    assert_eq!(view["lease"]["worker_id"], "worker-b", "{view}");
    assert_eq!(view["lease"]["generation"], 2);
    // B runs a third task on the resumed session.
    let c_task3 = uuid::Uuid::now_v7().to_string();
    let (s, _) = api.post(&a, &format!("/v1/sessions/{sid}/tasks"), json!({"command_id": c_task3, "goal_text": "and once more", "execution_profile": "cloud_isolated", "workspace_root": root})).await;
    assert_eq!(s, 202);
    until(
        "the third task to reach review under worker b",
        120,
        async || {
            let (_, v) = api.get(&a, &format!("/v1/sessions/{sid}")).await;
            (v["tasks"].as_array().unwrap().len() == 3
                && v["tasks"][2]["state"] == "READY_FOR_REVIEW")
                .then_some(())
        },
    )
    .await;
    worker_b.stop().await;
    let (_, view) = api.get(&a, &format!("/v1/sessions/{sid}")).await;
    assert_eq!(
        view["lease"]["ready"], true,
        "released, ready for the next owner: {view}"
    );
    assert!(
        view["lease"]["expires_at_ms"].as_i64().unwrap()
            <= modbit_domain::Timestamp::now().millis()
    );
    // Every mirrored event verified its chain on the way in; the local
    // Cores' logs and the cloud's agree on every aggregate's sequence.
    let (_, evs) = api
        .get(
            &a,
            &format!("/v1/events?session_id={sid}&after=0&limit=1000"),
        )
        .await;
    let all = evs["events"].as_array().unwrap();
    assert!(
        all.iter()
            .all(|e| e["envelope"]["integrity_hash"].as_str().unwrap().len() == 64)
    );
    let _ = store;
}

/// M8.5 (docs/21 "`modbit-guest`", "Sandbox substrate boundary"; docs/24
/// "Cloud Core Worker"; REQ-EV-0289): a `cloud_isolated` task's tools act
/// inside its sandbox — a process runs in the guest (on the MicroVM
/// backend, under the MicroVM's kernel), a file it writes there is what
/// `fs.read` reads back, `fs.list` and `fs.stat` see the guest's workspace
/// — the sandbox's identity is on the cloud log, and a cancel releases it.
#[tokio::test]
async fn qual_m8_5_a_cloud_isolated_tasks_tools_act_inside_its_sandbox_and_the_sandbox_is_released_when_the_task_ends()
 {
    let Some(store_cfg) = store_config().await else {
        eprintln!(
            "SKIPPED: MODBIT_CLOUD_TEST_DATABASE_URL unset (the hosted cloud job runs this against Postgres and a sandbox)"
        );
        return;
    };
    assert!(
        core_bin().exists(),
        "modbit-core at {}",
        core_bin().display()
    );
    let script = vec![
        json!({"calls": [{"name": "plan.update", "args": {"outcome": "the notes are summarized in the sandbox", "expected_files": ["summary.txt"]}}]}),
        // One turn of work in the guest: a process writes a file, the file
        // tools read it back; a path outside the workspace and a write to a
        // protected path are refused by the guest's policy.
        json!({"calls": [
            {"name": "shell.exec", "args": {"argv": ["/bin/sh", "-c", "uname -r; cat NOTES.md > summary.txt; echo written-in-guest >> summary.txt; pwd"]}},
            {"name": "fs.read", "args": {"path": "summary.txt"}},
            {"name": "fs.list", "args": {"path": "."}},
            {"name": "fs.stat", "args": {"path": "summary.txt"}},
            {"name": "fs.read", "args": {"path": "../../etc/passwd"}},
            {"name": "shell.exec", "args": {"argv": ["/bin/sh", "-c", "echo x > .git/hooks/pre-commit && echo wrote-hook || echo hook-refused"]}},
            // M8.6: the forge through the broker with the token it holds; a
            // destination the lease does not grant is refused.
            {"name": "shell.exec", "args": {"argv": ["/bin/sh", "-c", "FETCH=$(command -v wget >/dev/null 2>&1 && echo 'wget -qO-' || echo 'curl -s'); $FETCH http://forge.modbit.internal/user; echo; env | grep -i proxy | sort"]}},
            {"name": "shell.exec", "args": {"argv": ["/bin/sh", "-c", "FETCH=$(command -v wget >/dev/null 2>&1 && echo 'wget -qO-' || echo 'curl -s'); $FETCH http://127.0.0.1:9/hello || echo egress-refused"]}}
        ]}),
        json!({"calls": [{"name": "task.complete", "args": {"summary": "summarized", "self_review": {"findings": []}}}]}),
    ];
    let (model_base, seen) = scripted_model(script).await;
    let served = serve(ApiConfig {
        store: store_cfg.clone(),
        token_key: None,
        bind: "127.0.0.1:0".into(),
        rate_capacity: 500,
        rate_per_second: 100.0,
        worker_key: None,
        github_webhook_secret: None,
    })
    .await
    .expect("api");
    let api = Api {
        base: format!("http://{}", served.addr),
        http: reqwest::Client::new(),
        served,
    };
    let store = &api.served.state.store;
    let tenant = store.create_tenant("sandbox-test").await.unwrap();
    let (_p, secret) = store.create_principal(tenant, "user", "ada").await.unwrap();
    let (_, tok) = api
        .post("", "/v1/auth/token", json!({"secret": secret}))
        .await;
    let a = tok["access_token"].as_str().unwrap().to_owned();
    let keep = tempfile::tempdir().unwrap();
    let data = match std::env::var("MODBIT_CLOUD_WORKER_TEST_KEEP_DIR") {
        Ok(d) => std::path::PathBuf::from(d).join("m85"),
        Err(_) => keep.path().to_path_buf(),
    };
    let root = repo(&data.join("repo"));
    let gateway = Gateway::start(&store_cfg, &data.join("gateway")).await;
    let forge = fake_forge().await;
    let (_, created) = api
        .post(
            &a,
            "/v1/sessions",
            json!({"command_id": uuid::Uuid::now_v7().to_string()}),
        )
        .await;
    let sid = created["session_id"].as_str().unwrap().to_owned();
    let (s, task) = api.post(&a, &format!("/v1/sessions/{sid}/tasks"), json!({"command_id": uuid::Uuid::now_v7().to_string(), "goal_text": "summarize the notes", "execution_profile": "cloud_isolated", "workspace_root": root})).await;
    assert_eq!(s, 201, "{task}");
    let tid = task["task_id"].as_str().unwrap().to_owned();
    let worker = start(worker_config_with(
        &store_cfg,
        "worker-s",
        &data.join("w"),
        &model_base,
        Duration::from_secs(10),
        &gateway,
        Some(&forge),
    ))
    .await
    .expect("worker");
    until("the task to reach review", 180, async || {
        let (_, v) = api.get(&a, &format!("/v1/sessions/{sid}")).await;
        (v["tasks"][0]["state"] == "READY_FOR_REVIEW").then_some(())
    })
    .await;
    // The sandbox's identity is on the cloud log, no credential with it.
    let (_, evs) = api
        .get(
            &a,
            &format!("/v1/events?session_id={sid}&after=0&limit=1000"),
        )
        .await;
    let events = evs["events"].as_array().unwrap();
    let lease = events
        .iter()
        .find(|e| e["envelope"]["event_type"] == "SandboxLeaseAcquired")
        .expect("SandboxLeaseAcquired on the cloud log");
    let payload = &lease["payload"];
    assert_eq!(
        payload["backend"].as_str(),
        Some(gateway.backend),
        "{payload}"
    );
    assert_eq!(
        payload["isolated"].as_bool(),
        Some(gateway.backend == "microvm")
    );
    assert_eq!(payload["workspace_root"], "/workspace");
    assert!(
        !payload["image_version"]
            .as_str()
            .unwrap_or_default()
            .is_empty(),
        "the verified image is named: {payload}"
    );
    let sandbox_id = payload["sandbox_id"].as_str().unwrap().to_owned();
    assert!(
        !payload.to_string().contains(&forge.token) && !payload.to_string().contains("secret"),
        "no secret on the log: {payload}"
    );
    // What the model saw: every tool result came from inside the guest.
    let bodies = seen.lock().unwrap().clone();
    let last = bodies.last().unwrap();
    let tool_texts: Vec<String> = last["messages"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|m| m["role"] == "tool")
        .map(|m| m["content"].as_str().unwrap_or_default().to_owned())
        .collect();
    eprintln!("tool results:\n{}", tool_texts.join("\n----\n"));
    let exec = &tool_texts[1];
    assert!(
        exec.contains("status: SUCCESS") && exec.contains("/workspace"),
        "the process ran in the guest's workspace: {exec}"
    );
    if gateway.backend == "microvm" {
        assert!(
            exec.contains("6.1.155"),
            "the process ran under the MicroVM's kernel: {exec}"
        );
    }
    assert!(
        exec.contains(&format!("\"sandbox\": \"{sandbox_id}\"")) || exec.contains(&sandbox_id),
        "the result names the sandbox: {exec}"
    );
    let read = &tool_texts[2];
    assert!(
        read.contains("# notes") && read.contains("written-in-guest"),
        "fs.read reads what the guest process wrote: {read}"
    );
    let list = &tool_texts[3];
    assert!(
        list.contains("summary.txt") && list.contains("NOTES.md"),
        "fs.list sees the guest's workspace: {list}"
    );
    let stat = &tool_texts[4];
    assert!(
        stat.contains("\"exists\":true") && stat.contains("\"kind\":\"file\""),
        "{stat}"
    );
    let outside = &tool_texts[5];
    assert!(
        outside.contains("PATH_OUTSIDE_ROOT") || outside.contains("OUTSIDE_WORKSPACE"),
        "a path outside the workspace is refused by the guest: {outside}"
    );
    let hook = &tool_texts[6];
    if gateway.backend == "microvm" {
        assert!(
            hook.contains("hook-refused"),
            "a protected path cannot be written even by a process in the MicroVM: {hook}"
        );
    } else {
        assert!(
            hook.contains("wrote-hook") || hook.contains("hook-refused"),
            "{hook}"
        );
    }
    // M8.6: the forge answered through the broker with the token the guest
    // never held (its environment shows the proxy, nothing more); an
    // ungranted destination was refused; the lease's grants are on the log
    // and the broker's audit has both decisions.
    let forge_text = &tool_texts[7];
    assert!(
        (forge_text.contains("\"authorized\":true") || forge_text.contains("authorized\\\":true"))
            && forge_text.contains("http_proxy=http://127.0.0.1:")
            && !forge_text.contains(&forge.token),
        "the forge through the broker, the token never in the guest: {forge_text}"
    );
    let denied_text = &tool_texts[8];
    assert!(
        denied_text.contains("egress-refused") || !denied_text.contains("hello"),
        "{denied_text}"
    );
    let seen = forge.seen.lock().unwrap().clone();
    assert!(
        seen.iter().any(|h| h == &format!("Bearer {}", forge.token)),
        "the fake forge saw the real token from the broker: {seen:?}"
    );
    assert!(
        payload["credentials"]
            .as_array()
            .is_some_and(|c| c.iter().any(|v| v == "forge.modbit.internal")),
        "{payload}"
    );
    assert!(
        payload["egress"].as_array().is_some_and(|e| !e.is_empty()),
        "{payload}"
    );
    let (s, audit) = gw_get(
        &gateway,
        "worker-s",
        &format!("/v1/sandboxes/{sandbox_id}/egress?tenant_id={tenant}"),
    )
    .await;
    assert_eq!(s, 200, "{audit}");
    let records = audit["records"].as_array().unwrap();
    assert!(
        records
            .iter()
            .any(|r| r["kind"] == "credentialed" && r["allowed"] == true),
        "{audit}"
    );
    assert!(records.iter().any(|r| r["allowed"] == false), "{audit}");
    // The sandbox lives while the task does; a cancel ends the task and
    // releases it, on the cloud log.
    let c_cancel = uuid::Uuid::now_v7().to_string();
    let (s, _) = api
        .post(
            &a,
            &format!("/v1/tasks/{tid}:cancel"),
            json!({"command_id": c_cancel}),
        )
        .await;
    assert_eq!(s, 202);
    until("the sandbox to be released", 60, async || {
        let (_, v) = api
            .get(
                &a,
                &format!("/v1/events?session_id={sid}&after=0&limit=1000"),
            )
            .await;
        v["events"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| {
                e["envelope"]["event_type"] == "SandboxReleased"
                    && e["payload"]["sandbox_id"] == sandbox_id.as_str()
            })
            .then_some(())
    })
    .await;
    let (s, rec) = api
        .http
        .get(format!(
            "{}/v1/sandboxes/{sandbox_id}?tenant_id={tenant}",
            gateway.base_url
        ))
        .bearer_auth(gateway.token_for("worker-s"))
        .send()
        .await
        .map(|r| (r.status().as_u16(), r))
        .unwrap();
    let rec: Value = rec.json().await.unwrap();
    assert_eq!(
        (s, rec["state"].as_str()),
        (200, Some("DESTROYED")),
        "{rec}"
    );
    worker.stop().await;
    gateway.served.stop();
}

/// One command on a Core, fenced by `generation` when given.
async fn cmd<T: prost::Message + Default>(
    c: &mut modbit_protocol::client::Client,
    kind: &str,
    payload: Vec<u8>,
    generation: Option<u64>,
) -> Result<T, modbit_protocol::client::ClientError> {
    let env = modbit_protocol::v1::CommandEnvelope {
        command_id: Some(modbit_protocol::v1::Id {
            value: uuid::Uuid::now_v7().as_bytes().to_vec(),
        }),
        tenant_id: None,
        user_id: None,
        session_id: None,
        aggregate_id: None,
        expected_generation: generation,
        command_type: kind.into(),
        schema_version: 1,
        payload,
        issued_at: None,
    };
    let ack = c.command(env).await?;
    modbit_protocol::client::Client::result(&ack)
}

/// M8.7 (docs/21 "Handoff local → cloud"; docs/24 "Sync model"; docs/30
/// `POST /v1/handoffs`): a task that began on a local Core — its log, its
/// objects, the repository and the worktree exactly as it was — continues
/// in the cloud: exported as a bundle (no raw secret in it), admitted by
/// the API after the capability-parity check, materialized by a worker
/// into a sandbox, resumed by the same runtime from the same log; a
/// continuation that needs what the cloud does not serve is refused.
#[tokio::test]
async fn qual_m8_7_a_local_task_hands_off_to_the_cloud_and_continues_from_its_checkpoint_in_a_sandbox()
 {
    let Some(store_cfg) = store_config().await else {
        eprintln!(
            "SKIPPED: MODBIT_CLOUD_TEST_DATABASE_URL unset (the hosted cloud job runs this against Postgres and a sandbox)"
        );
        return;
    };
    assert!(
        core_bin().exists(),
        "modbit-core at {}",
        core_bin().display()
    );
    let keep = tempfile::tempdir().unwrap();
    let data = match std::env::var("MODBIT_CLOUD_WORKER_TEST_KEEP_DIR") {
        Ok(d) => std::path::PathBuf::from(d).join("m87"),
        Err(_) => keep.path().to_path_buf(),
    };
    let root = repo(&data.join("laptop-repo"));
    // ---- the laptop: a local Core runs the task's first turns ----
    let local_script = vec![
        json!({"calls": [{"name": "plan.update", "args": {"outcome": "summary.md written from the notes", "expected_files": ["summary.md"]}}]}),
        json!({"calls": [{"name": "change.apply", "args": {"path": "summary.md", "op": "replace", "content": "handed off from the laptop\n"}}]}),
        // Held until the test has asked for the handoff: the run parks at
        // this turn's boundary.
        json!({"gate": true, "calls": []}),
    ];
    let (local_model, _local_seen, gate) = scripted_model_gated(local_script).await;
    let laptop = modbit_cloud_worker::core_process::CoreProcess::spawn_local(
        &core_bin(),
        &data.join("laptop-core"),
    )
    .expect("local core");
    let mut lc = laptop
        .client_as(modbit_protocol::v1::ClientKind::Cli)
        .await
        .expect("local client");
    // The laptop holds two secrets the bundle must never carry: the
    // provider key and the forge token (QUAL-EV-0063).
    let laptop_api_key = "sk-laptop-4f9c2a7e1b3d5c6e8f0a1b2c3d4e5f60";
    let laptop_forge_token = "ghp_laptopforgetoken0123456789abcdefXYZ";
    let _: modbit_protocol::v1::ProviderConfigured = cmd(
        &mut lc,
        "ConfigureProvider",
        modbit_protocol::v1::ConfigureProvider {
            provider: "openai".into(),
            api_key: laptop_api_key.into(),
            base_url: local_model.clone(),
        }
        .encode_to_vec(),
        None,
    )
    .await
    .expect("provider");
    let _: modbit_protocol::v1::ForgeConfigured = cmd(
        &mut lc,
        "ConfigureForge",
        modbit_protocol::v1::ConfigureForge {
            forge: "github".into(),
            token: laptop_forge_token.into(),
            api_base_url: String::new(),
            web_host: String::new(),
        }
        .encode_to_vec(),
        None,
    )
    .await
    .expect("forge");
    let created: modbit_protocol::v1::SessionCreated = cmd(
        &mut lc,
        "CreateSession",
        modbit_protocol::v1::CreateSession { space_id: None }.encode_to_vec(),
        None,
    )
    .await
    .expect("session");
    let local_sid = created.session_id.clone().unwrap();
    let lease: modbit_protocol::v1::SessionLeaseAcquired = cmd(
        &mut lc,
        "AcquireSessionLease",
        modbit_protocol::v1::AcquireSessionLease {
            session_id: Some(local_sid.clone()),
            owner: "laptop".into(),
        }
        .encode_to_vec(),
        None,
    )
    .await
    .expect("lease");
    let g = Some(lease.lease_generation);
    let task: modbit_protocol::v1::TaskCreated = cmd(
        &mut lc,
        "CreateTask",
        modbit_protocol::v1::CreateTask {
            session_id: Some(local_sid.clone()),
            goal_text: "summarize the notes into summary.md".into(),
            workspace_id: None,
            execution_profile: "local_trusted".into(),
            origin: "cli".into(),
            workspace_root: root.clone(),
            issue_url: String::new(),
            issue_json: String::new(),
        }
        .encode_to_vec(),
        g,
    )
    .await
    .expect("task");
    let local_tid = task.task_id.clone().unwrap();
    let tid = modbit_domain::TaskId::from_bytes(local_tid.value.clone().try_into().unwrap());
    let sid = modbit_domain::SessionId::from_bytes(local_sid.value.clone().try_into().unwrap());
    let _: modbit_protocol::v1::TaskRunStarted = cmd(
        &mut lc,
        "StartTask",
        modbit_protocol::v1::StartTask {
            task_id: Some(local_tid.clone()),
            endpoint: "openai".into(),
            model: "gpt-5".into(),
            max_turns: 0,
            max_tool_calls: 0,
            max_no_progress_turns: 0,
            skills: vec![],
        }
        .encode_to_vec(),
        g,
    )
    .await
    .expect("started");
    // The edit landed on the laptop's worktree.
    until("the laptop's edit to land", 120, async || {
        std::fs::read_to_string(std::path::Path::new(&root).join("summary.md"))
            .ok()
            .filter(|s| s.contains("handed off"))
            .map(|_| ())
    })
    .await;
    // ---- the handoff: export, park, upload, admit ----
    let bundle_dir = data.join("bundle");
    // The export parks the run: its pending model call is released so the
    // loop reaches the boundary.
    let mut lc2 = laptop
        .client_as(modbit_protocol::v1::ClientKind::Cli)
        .await
        .expect("local client 2");
    let export_task = tokio::spawn({
        let payload = modbit_protocol::v1::ExportHandoff {
            task_id: Some(local_tid.clone()),
            out_dir: bundle_dir.to_string_lossy().into_owned(),
        }
        .encode_to_vec();
        async move {
            cmd::<modbit_protocol::v1::HandoffExported>(&mut lc2, "ExportHandoff", payload, g).await
        }
    });
    tokio::time::sleep(Duration::from_millis(800)).await;
    gate.notify_waiters();
    let exported = export_task.await.unwrap().expect("exported");
    let manifest: Value = serde_json::from_str(&exported.manifest_json).unwrap();
    assert_eq!(manifest["kind"], "modbit-handoff");
    assert!(manifest["git"]["bundle"].as_bool().unwrap(), "{manifest}");
    assert!(
        manifest["capabilities"]
            .as_array()
            .unwrap()
            .iter()
            .any(|c| c == "fs.write"),
        "{manifest}"
    );
    assert!(
        !manifest["capabilities"]
            .as_array()
            .unwrap()
            .iter()
            .any(|c| c == "browser.control"),
        "{manifest}"
    );
    assert!(
        exported.parts.iter().any(|p| p == "repo.bundle")
            && exported.parts.iter().any(|p| p == "events.jsonl")
    );
    // The export's own record follows the bundle: the bundle carries the
    // laptop's run up to the checkpoint, not the handoff itself.
    let bundle_text = std::fs::read_to_string(bundle_dir.join("events.jsonl")).unwrap();
    assert!(
        !bundle_text.contains("\"TaskHandedOff\""),
        "the export's own record follows the bundle"
    );
    assert!(
        bundle_text.contains("\"CheckpointCommitted\"") && bundle_text.contains("\"RunSuspended\""),
        "the bundle carries the checkpoint and the park"
    );
    // Nothing secret-shaped left the laptop: the bundle is the log, the
    // objects, the repository and the manifest, and no byte of either
    // secret is in any of them (QUAL-EV-0063); the secret handle is.
    let mut bundle_files = Vec::new();
    for entry in std::fs::read_dir(&bundle_dir).unwrap().flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        assert!(
            ["events.jsonl", "repo.bundle", "manifest.json", "objects"].contains(&name.as_str()),
            "{name}"
        );
        if entry.path().is_dir() {
            bundle_files.extend(
                std::fs::read_dir(entry.path())
                    .unwrap()
                    .flatten()
                    .map(|e| e.path()),
            );
        } else {
            bundle_files.push(entry.path());
        }
    }
    assert!(bundle_files.len() >= 4, "{bundle_files:?}");
    for f in &bundle_files {
        let bytes = std::fs::read(f).unwrap();
        for secret in [laptop_api_key, laptop_forge_token] {
            assert!(
                !bytes.windows(secret.len()).any(|w| w == secret.as_bytes()),
                "a secret value is in the bundle: {}",
                f.display()
            );
        }
    }
    assert!(
        manifest["secret_handles"]
            .as_array()
            .is_some_and(|h| h.iter().any(|h| h == "forge-token")),
        "the handle, not the value: {manifest}"
    );
    // The laptop's log says so.
    let snap: modbit_protocol::v1::SessionSnapshot = cmd(
        &mut lc,
        "GetSessionSnapshot",
        modbit_protocol::v1::GetSessionSnapshot {
            session_id: Some(local_sid.clone()),
        }
        .encode_to_vec(),
        None,
    )
    .await
    .unwrap();
    assert!(
        snap.tasks[0].state.starts_with("Waiting"),
        "parked: {}",
        snap.tasks[0].state
    );
    // ---- the cloud: API, gateway, worker ----
    let served = serve(ApiConfig {
        store: store_cfg.clone(),
        token_key: None,
        bind: "127.0.0.1:0".into(),
        rate_capacity: 500,
        rate_per_second: 100.0,
        worker_key: None,
        github_webhook_secret: None,
    })
    .await
    .expect("api");
    let api = Api {
        base: format!("http://{}", served.addr),
        http: reqwest::Client::new(),
        served,
    };
    let store = &api.served.state.store;
    let tenant = store.create_tenant("handoff-test").await.unwrap();
    let (_p, secret) = store.create_principal(tenant, "user", "ada").await.unwrap();
    let (_, tok) = api
        .post("", "/v1/auth/token", json!({"secret": secret}))
        .await;
    let a = tok["access_token"].as_str().unwrap().to_owned();
    // Upload the parts.
    let mut parts = serde_json::Map::new();
    for name in ["events.jsonl", "repo.bundle", "manifest.json"] {
        let bytes = std::fs::read(bundle_dir.join(name)).unwrap();
        let r = api
            .http
            .put(format!("{}/v1/objects", api.base))
            .bearer_auth(&a)
            .body(bytes)
            .send()
            .await
            .unwrap();
        assert_eq!(r.status().as_u16(), 201, "{name}");
        let v: Value = r.json().await.unwrap();
        parts.insert(name.to_owned(), v["hash"].clone());
    }
    for entry in std::fs::read_dir(bundle_dir.join("objects"))
        .unwrap()
        .flatten()
    {
        let bytes = std::fs::read(entry.path()).unwrap();
        let r = api
            .http
            .put(format!("{}/v1/objects", api.base))
            .bearer_auth(&a)
            .body(bytes)
            .send()
            .await
            .unwrap();
        assert_eq!(r.status().as_u16(), 201);
        let v: Value = r.json().await.unwrap();
        assert_eq!(
            v["hash"].as_str().unwrap(),
            entry.file_name().to_string_lossy(),
            "content-addressed on both sides"
        );
    }
    // IMP-EV-0176: a capsule that smuggles authority or a secret value is
    // refused before anything is admitted — a grant-shaped field in the
    // manifest, a secret-shaped value anywhere in it.
    let mut with_grant = manifest.clone();
    with_grant["token"] = json!(laptop_forge_token);
    let (s, e) = api.post(&a, "/v1/handoffs", json!({"command_id": uuid::Uuid::now_v7().to_string(), "manifest": with_grant, "parts": parts})).await;
    assert_eq!(
        (s, e["code"].as_str()),
        (409, Some("CAPSULE_SMUGGLES_AUTHORITY")),
        "{e}"
    );
    let mut with_secret = manifest.clone();
    with_secret["goal_text"] = json!(format!("summarize; the key is {laptop_api_key}"));
    let (s, e) = api.post(&a, "/v1/handoffs", json!({"command_id": uuid::Uuid::now_v7().to_string(), "manifest": with_secret, "parts": parts})).await;
    assert_eq!(
        (s, e["code"].as_str()),
        (409, Some("CAPSULE_SMUGGLES_SECRET")),
        "{e}"
    );
    // Parity: a continuation that needs a capability the cloud does not
    // serve (a host worktree) is refused.
    let mut needs_browser = manifest.clone();
    needs_browser["capabilities"] = json!(["fs.read", "git.worktree"]);
    let (s, e) = api.post(&a, "/v1/handoffs", json!({"command_id": uuid::Uuid::now_v7().to_string(), "manifest": needs_browser, "parts": parts})).await;
    assert_eq!(
        (s, e["code"].as_str()),
        (409, Some("CAPABILITY_PARITY")),
        "{e}"
    );
    // Admitted.
    let c_handoff = uuid::Uuid::now_v7().to_string();
    let (s, admitted) = api
        .post(
            &a,
            "/v1/handoffs",
            json!({"command_id": c_handoff, "manifest": manifest, "parts": parts}),
        )
        .await;
    assert_eq!(s, 201, "{admitted}");
    assert_eq!(admitted["task_id"].as_str().unwrap(), tid.to_string());
    let (s, again) = api
        .post(
            &a,
            "/v1/handoffs",
            json!({"command_id": c_handoff, "manifest": manifest, "parts": parts}),
        )
        .await;
    assert_eq!(
        (s, again["bundle_hash"].as_str()),
        (200, admitted["bundle_hash"].as_str()),
        "a retry replays the recorded admission: {again}"
    );
    let (_, evs) = api
        .get(
            &a,
            &format!("/v1/events?session_id={sid}&after=0&limit=1000"),
        )
        .await;
    let types: Vec<String> = evs["events"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["envelope"]["event_type"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(types[0], "SessionCreated");
    // The bundle's log ends where the laptop parked; the admission follows
    // it here (the laptop's own `TaskHandedOff` records the export there).
    assert!(
        types.iter().any(|t| t == "TaskHandoffAdmitted"),
        "{types:?}"
    );
    assert!(
        types.iter().any(|t| t == "CheckpointCommitted")
            && types.iter().any(|t| t == "RunSuspended"),
        "the checkpoint before the handoff and the parked run are on the log: {types:?}"
    );
    assert!(
        types
            .iter()
            .any(|t| t == "FileChanged" || t == "ToolCallSucceeded"),
        "the laptop's run is on the cloud log: {types:?}"
    );
    // The cloud's continuation: a worker with a sandbox.
    let cloud_script = vec![
        json!({"calls": [
            {"name": "fs.read", "args": {"path": "summary.md"}},
            // The guest image has no git: the repository is read as files.
            {"name": "shell.exec", "args": {"argv": ["/bin/sh", "-c", "cat .git/HEAD; ls .git/refs/heads; cat summary.md"]}}
        ]}),
        json!({"calls": [{"name": "task.complete", "args": {"summary": "continued in the cloud", "self_review": {"findings": []}}}]}),
    ];
    let (cloud_model, cloud_seen) = scripted_model(cloud_script).await;
    let gateway = Gateway::start(&store_cfg, &data.join("gateway")).await;
    let worker = start(worker_config(
        &store_cfg,
        "worker-h",
        &data.join("w"),
        &cloud_model,
        Duration::from_secs(10),
        &gateway,
    ))
    .await
    .expect("worker");
    until(
        "the continuation to reach review in the cloud",
        240,
        async || {
            let (_, v) = api.get(&a, &format!("/v1/sessions/{sid}")).await;
            (v["tasks"][0]["state"] == "READY_FOR_REVIEW").then_some(())
        },
    )
    .await;
    let (_, evs) = api
        .get(
            &a,
            &format!("/v1/events?session_id={sid}&after=0&limit=2000"),
        )
        .await;
    let types: Vec<String> = evs["events"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["envelope"]["event_type"].as_str().unwrap().to_owned())
        .collect();
    for needed in [
        "TaskHandoffAdmitted",
        "TaskWorkspaceRebound",
        "TaskResumed",
        "RunResumed",
        "SandboxLeaseAcquired",
        "TaskReadyForReview",
    ] {
        assert!(
            types.iter().any(|t| t == needed),
            "{needed} on the cloud log: {types:?}"
        );
    }
    let bodies = cloud_seen.lock().unwrap().clone();
    let last = bodies.last().unwrap();
    let tool_texts: Vec<String> = last["messages"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|m| m["role"] == "tool")
        .map(|m| m["content"].as_str().unwrap_or_default().to_owned())
        .collect();
    eprintln!("cloud tool results:\n{}", tool_texts.join("\n----\n"));
    // The continuation's conversation carries the laptop's turns (the plan
    // and the edit are in the messages the cloud model saw).
    assert!(
        last["messages"]
            .as_array()
            .unwrap()
            .iter()
            .any(|m| m["content"]
                .as_str()
                .is_some_and(|c| c.contains("plan version 1 recorded"))),
        "the laptop's transcript continued in the cloud"
    );
    // The cloud's own two tool results come after the laptop's two.
    let cloud_texts: Vec<&String> = tool_texts.iter().skip(2).collect();
    assert_eq!(
        cloud_texts.len(),
        2,
        "the cloud turn's results: {tool_texts:?}"
    );
    assert!(
        cloud_texts.iter().all(|t| t.contains("status: SUCCESS")),
        "the cloud's tools ran in the sandbox: {cloud_texts:?}"
    );
    let read = cloud_texts[0];
    assert!(
        read.contains("handed off from the laptop"),
        "the worktree arrived exactly: {read}"
    );
    let git_text = cloud_texts[1];
    assert!(
        git_text.contains("ref: refs/heads/") && git_text.contains("handed off from the laptop"),
        "the repository arrived in the sandbox with its branch checked out: {git_text}"
    );
    // The history itself, on the worker's materialized workspace (the
    // sandbox was seeded from it): the laptop's commit, and the edit
    // uncommitted, as it was.
    let ws = data
        .join("w")
        .join("sessions")
        .join(sid.to_string())
        .join("handoffs")
        .join(tid.to_string())
        .join("workspace");
    let log = std::process::Command::new("git")
        .args(["-C", &ws.display().to_string(), "log", "--oneline"])
        .output()
        .unwrap();
    assert!(
        String::from_utf8_lossy(&log.stdout).contains("base"),
        "the repository's history arrived: {}",
        String::from_utf8_lossy(&log.stdout)
    );
    let status = std::process::Command::new("git")
        .args(["-C", &ws.display().to_string(), "status", "--short"])
        .output()
        .unwrap();
    assert!(
        String::from_utf8_lossy(&status.stdout).contains("summary.md"),
        "the dirty edit is uncommitted, as it was: {}",
        String::from_utf8_lossy(&status.stdout)
    );
    worker.stop().await;
    gateway.served.stop();
    laptop.stop();
}

/// M8.8 (docs/22 "Cloud browser"): a `cloud_isolated` task's browser is
/// the Chromium inside its sandbox — the Core's browser tools act on it
/// over the gateway's DevTools relay (the page reached through the egress
/// broker, on the admitted host) — and the person, through the Cloud API,
/// watches the page's screencast, takes control, clicks in the view and
/// hands control back; the agent, back in control, reads the page the
/// person left.
#[tokio::test]
async fn qual_m8_8_a_cloud_tasks_browser_runs_inside_its_sandbox_over_cdp_and_streams_its_view_to_the_person()
 {
    let Some(store_cfg) = store_config().await else {
        eprintln!(
            "SKIPPED: MODBIT_CLOUD_TEST_DATABASE_URL unset (the hosted cloud job runs this against Postgres and a sandbox)"
        );
        return;
    };
    assert!(
        core_bin().exists(),
        "modbit-core at {}",
        core_bin().display()
    );
    let forge = fake_forge().await;
    let form_url = format!("{}/form", forge.base_url);
    eprintln!("fake forge (and the greeter) at {}", forge.base_url);
    let script = vec![
        json!({"calls": [{"name": "plan.update", "args": {"outcome": "the greeter greeted us", "expected_files": []}}]}),
        // The agent: the form, filled and followed — every step through
        // the sandbox's browser.
        json!({"calls": [
            {"name": "browser.navigate", "args": {"url": form_url}},
            {"name": "browser.snapshot", "args": {}}
        ]}),
        json!({"calls": [
            {"name": "browser.act", "args": {"ref": {"$ref": {"role": "textbox", "name": "Your name"}}, "action": "fill", "value": "ada"}}
        ]}),
        json!({"calls": [
            {"name": "browser.act", "args": {"ref": {"$ref": {"role": "link", "name": "Greet"}}, "action": "click", "expect": {"url_contains": "/greet?who=ada"}}},
            {"name": "browser.snapshot", "args": {}}
        ]}),
        // Held while the person takes over; when the gate opens the agent
        // reads the page again and completes.
        json!({"gate": true, "calls": [{"name": "browser.snapshot", "args": {}}]}),
        json!({"calls": [{"name": "task.complete", "args": {"summary": "greeted in the cloud browser", "self_review": {"findings": []}}}]}),
    ];
    let (model_base, seen, gate) = scripted_model_gated(script).await;
    // The API verifies worker tokens under the gateway's key: the worker's
    // one token links it to both.
    let keep = tempfile::tempdir().unwrap();
    let data = match std::env::var("MODBIT_CLOUD_WORKER_TEST_KEEP_DIR") {
        Ok(d) => std::path::PathBuf::from(d).join("m88"),
        Err(_) => keep.path().to_path_buf(),
    };
    let gateway = Gateway::start(&store_cfg, &data.join("gateway")).await;
    let served = serve(ApiConfig {
        store: store_cfg.clone(),
        token_key: None,
        bind: "127.0.0.1:0".into(),
        rate_capacity: 500,
        rate_per_second: 100.0,
        worker_key: Some(gateway.key_bytes()),
        github_webhook_secret: None,
    })
    .await
    .expect("api");
    let api = Api {
        base: format!("http://{}", served.addr),
        http: reqwest::Client::new(),
        served,
    };
    let store = &api.served.state.store;
    let tenant = store.create_tenant("browser-test").await.unwrap();
    let (_p, secret) = store.create_principal(tenant, "user", "ada").await.unwrap();
    let (_, tok) = api
        .post("", "/v1/auth/token", json!({"secret": secret}))
        .await;
    let a = tok["access_token"].as_str().unwrap().to_owned();
    let root = repo(&data.join("repo"));
    let (_, created) = api
        .post(
            &a,
            "/v1/sessions",
            json!({"command_id": uuid::Uuid::now_v7().to_string()}),
        )
        .await;
    let sid = created["session_id"].as_str().unwrap().to_owned();
    let (s, task) = api.post(&a, &format!("/v1/sessions/{sid}/tasks"), json!({"command_id": uuid::Uuid::now_v7().to_string(), "goal_text": "greet through the greeter", "execution_profile": "cloud_isolated", "workspace_root": root})).await;
    assert_eq!(s, 201, "{task}");
    let mut cfg = worker_config_with(
        &store_cfg,
        "worker-b",
        &data.join("w"),
        &model_base,
        Duration::from_secs(10),
        &gateway,
        Some(&forge),
    );
    cfg.api = Some(modbit_cloud_worker::ApiLinkConfig {
        base_url: api.base.clone(),
        worker_token: gateway.token_for("worker-b"),
    });
    let worker = start(cfg).await.expect("worker");
    // The worker links to the API outbound.
    until("the worker's link", 30, async || {
        api.served
            .state
            .workers
            .linked("worker-b")
            .await
            .then_some(())
    })
    .await;
    // The agent's turns run until the gated step: the greeting page is up.
    let bsid = until("the browser session on the cloud log", 240, async || {
        let (_, evs) = api
            .get(
                &a,
                &format!("/v1/events?session_id={sid}&after=0&limit=1000"),
            )
            .await;
        let events = evs["events"].as_array()?;
        let opened = events
            .iter()
            .find(|e| e["envelope"]["event_type"] == "BrowserSessionOpened")?;
        let bsid = opened["payload"]["browser_session_id"].as_str()?.to_owned();
        // Four tool-calling turns done: the last snapshot (the greeting) is in.
        let bodies = seen.lock().unwrap();
        let turns = bodies
            .last()
            .and_then(|b| b["messages"].as_array())
            .map(|m| {
                m.iter()
                    .filter(|x| x["role"] == "assistant" && !x["tool_calls"].is_null())
                    .count()
            })
            .unwrap_or(0);
        (turns >= 4).then_some(bsid)
    })
    .await;
    // The log names the host: the Core itself, over the sandbox's relay.
    let (_, evs) = api
        .get(
            &a,
            &format!("/v1/events?session_id={sid}&after=0&limit=1000"),
        )
        .await;
    let events = evs["events"].as_array().unwrap().clone();
    let attached = events
        .iter()
        .find(|e| e["envelope"]["event_type"] == "BrowserHostAttached")
        .expect("BrowserHostAttached");
    assert_eq!(attached["payload"]["host_kind"], "cloud-cdp", "{attached}");
    assert!(
        attached["payload"]["partition"]
            .as_str()
            .is_some_and(|p| p.starts_with("sandbox:")),
        "{attached}"
    );
    let lease = events
        .iter()
        .find(|e| e["envelope"]["event_type"] == "SandboxLeaseAcquired")
        .expect("SandboxLeaseAcquired");
    assert_eq!(lease["payload"]["browser"], true, "{lease}");
    // What the agent saw: the form compiled from the sandbox's Chromium,
    // the fill, the link followed to the greeting.
    let bodies = seen.lock().unwrap().clone();
    let tool_texts: Vec<String> = bodies.last().unwrap()["messages"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|m| m["role"] == "tool")
        .map(|m| m["content"].as_str().unwrap_or_default().to_owned())
        .collect();
    eprintln!("agent tool results:\n{}", tool_texts.join("\n----\n"));
    let nav = &tool_texts[1];
    assert!(
        nav.contains("status: SUCCESS") && nav.contains("Greeter"),
        "the sandbox's browser loaded the page through the broker: {nav}"
    );
    let snap = &tool_texts[2];
    assert!(
        snap.contains("\"role\":\"textbox\"")
            && snap.contains("Your name")
            && snap.contains("Greet"),
        "the page compiled from the guest's accessibility tree: {snap}"
    );
    let fill = &tool_texts[3];
    assert!(
        fill.contains("status: SUCCESS") && fill.contains("inserted 3 chars"),
        "the fill acted inside the guest: {fill}"
    );
    let click = &tool_texts[4];
    assert!(
        click.contains("status: SUCCESS")
            && click.contains("/greet?who=ada")
            && click.contains("\"held\":true")
            && click.contains("hello, ada")
            && click.contains("Again"),
        "the click navigated to the greeting and the postcondition held: {click}"
    );
    let greeted = &tool_texts[5];
    assert!(
        greeted.contains("Greeted") && greeted.contains("/greet?who=ada"),
        "the greeting page, read again: {greeted}"
    );
    // The "Again" link's box, as compiled after the click — where the
    // person will click.
    let again_bounds = {
        let json_text = click.split("output:\n").nth(1).unwrap().trim();
        let v: Value = serde_json::from_str(json_text).unwrap();
        v["delta"]["added"]
            .as_array()
            .unwrap()
            .iter()
            .find(|e| e["name"] == "Again")
            .map(|e| e["bounds"].clone())
            .expect("the Again link's bounds")
    };
    assert!(
        again_bounds["width"].as_u64().unwrap_or(0) > 0,
        "{again_bounds}"
    );
    // ---- the person, through the Cloud API ----
    // Input under the agent's control is refused before it reaches the page.
    let (s, refused) = api
        .post(
            &a,
            &format!("/v1/browser-sessions/{bsid}:input"),
            json!({"session_id": sid, "kind": "click", "x": 10, "y": 10}),
        )
        .await;
    assert_eq!(s, 409, "{refused}");
    assert_eq!(refused["code"], "AGENT_ACTIVE", "{refused}");
    // Take control.
    let (s, ctl) = api
        .post(
            &a,
            &format!("/v1/browser-sessions/{bsid}:control"),
            json!({"session_id": sid, "controller": "USER"}),
        )
        .await;
    assert_eq!(s, 200, "{ctl}");
    assert_eq!(ctl["controller"], "USER", "{ctl}");
    // Watch: frames of the greeting page arrive over the API's stream.
    let ws_url = format!(
        "{}/v1/browser-sessions/{bsid}/stream?session_id={sid}",
        api.base.replace("http://", "ws://")
    );
    use futures_util::StreamExt;
    use tokio_tungstenite::tungstenite::client::IntoClientRequest;
    let mut req = ws_url.as_str().into_client_request().unwrap();
    req.headers_mut()
        .insert("authorization", format!("Bearer {a}").parse().unwrap());
    let (mut ws, _) = tokio_tungstenite::connect_async(req).await.expect("stream");
    let first = tokio::time::timeout(Duration::from_secs(30), ws.next())
        .await
        .expect("an opener")
        .expect("a message")
        .expect("text");
    let opener: Value = serde_json::from_str(first.to_text().unwrap()).unwrap();
    assert_eq!(opener["watching"]["host_kind"], "cloud-cdp", "{opener}");
    assert_eq!(opener["watching"]["controller"], "USER", "{opener}");
    let mut frame = None;
    let deadline = Instant::now() + Duration::from_secs(60);
    while Instant::now() < deadline {
        let Ok(Some(Ok(m))) = tokio::time::timeout(Duration::from_secs(30), ws.next()).await else {
            break;
        };
        let v: Value = serde_json::from_str(m.to_text().unwrap_or("{}")).unwrap_or_default();
        if v["frame"].is_object() {
            frame = Some(v["frame"].clone());
            break;
        }
    }
    let frame = frame.expect("a screencast frame of the cloud browser");
    let jpeg = base64::Engine::decode(
        &base64::engine::general_purpose::STANDARD,
        frame["jpeg_base64"].as_str().unwrap(),
    )
    .unwrap();
    assert!(
        jpeg.len() > 500 && jpeg.starts_with(&[0xFF, 0xD8]),
        "a JPEG frame ({} bytes)",
        jpeg.len()
    );
    assert!(
        frame["page_width"].as_u64().unwrap_or(0) >= 320,
        "the page's size travels with the frame: {}",
        frame["page_width"]
    );
    assert!(
        frame["url"].as_str().is_some_and(|u| u.contains("/greet")),
        "the frame is of the greeting page: {frame}"
    );
    // The person clicks "Again" in the view: the page goes back to the form.
    let x = again_bounds["x"].as_f64().unwrap() + again_bounds["width"].as_f64().unwrap() / 2.0;
    let y = again_bounds["y"].as_f64().unwrap() + again_bounds["height"].as_f64().unwrap() / 2.0;
    let (s, clicked) = api
        .post(
            &a,
            &format!("/v1/browser-sessions/{bsid}:input"),
            json!({"session_id": sid, "kind": "click", "x": x, "y": y}),
        )
        .await;
    assert_eq!(s, 200, "{clicked}");
    assert_eq!(clicked["delivered"], true, "{clicked}");
    // A later frame shows the form again.
    let mut back = false;
    let deadline = Instant::now() + Duration::from_secs(60);
    while Instant::now() < deadline {
        let Ok(Some(Ok(m))) = tokio::time::timeout(Duration::from_secs(30), ws.next()).await else {
            break;
        };
        let v: Value = serde_json::from_str(m.to_text().unwrap_or("{}")).unwrap_or_default();
        if v["frame"]["url"]
            .as_str()
            .is_some_and(|u| u.ends_with("/form"))
        {
            back = true;
            break;
        }
    }
    assert!(
        back,
        "the person's click navigated the cloud browser back to the form"
    );
    drop(ws);
    // Control back to the agent; its next read is the page the person left.
    let (s, ctl) = api
        .post(
            &a,
            &format!("/v1/browser-sessions/{bsid}:control"),
            json!({"session_id": sid, "controller": "AGENT"}),
        )
        .await;
    assert_eq!(s, 200, "{ctl}");
    gate.notify_waiters();
    until("the task to reach review", 180, async || {
        let (_, v) = api.get(&a, &format!("/v1/sessions/{sid}")).await;
        (v["tasks"][0]["state"] == "READY_FOR_REVIEW").then_some(())
    })
    .await;
    let bodies = seen.lock().unwrap().clone();
    let last_texts: Vec<String> = bodies.last().unwrap()["messages"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|m| m["role"] == "tool")
        .map(|m| m["content"].as_str().unwrap_or_default().to_owned())
        .collect();
    let after_person = last_texts
        .get(6)
        .expect("the agent's read after the person's turn");
    assert!(
        after_person.contains("Greeter") && after_person.contains("Your name"),
        "the agent reads the form the person navigated to: {after_person}"
    );
    // Control changes are on the log, both ways.
    let (_, evs) = api
        .get(
            &a,
            &format!("/v1/events?session_id={sid}&after=0&limit=1000"),
        )
        .await;
    let controls: Vec<String> = evs["events"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["envelope"]["event_type"] == "BrowserControlChanged")
        .map(|e| {
            e["payload"]["controller"]
                .as_str()
                .unwrap_or_default()
                .to_owned()
        })
        .collect();
    assert_eq!(controls, vec!["USER", "AGENT"], "{controls:?}");
    worker.stop().await;
    gateway.served.stop();
}

/// M8.9 (docs/21 "Sandbox recovery", E2E-018): a cloud task's sandbox is
/// killed mid-task, after a checkpoint. The call in flight keeps its
/// unknown outcome (never replayed by the Core); the lease loss is
/// detected (`SandboxLost`, the gateway's record `LOST`), a fresh sandbox
/// is provisioned and restored from the latest checkpoint
/// (`SandboxRestored`), and the run resumes in it: the model's next call
/// reads the files the checkpoint carried.
#[tokio::test]
async fn qual_m8_9_a_lost_sandbox_is_replaced_and_restored_from_the_latest_checkpoint_and_the_task_resumes()
 {
    let Some(store_cfg) = store_config().await else {
        eprintln!(
            "SKIPPED: MODBIT_CLOUD_TEST_DATABASE_URL unset (the hosted cloud job runs this against Postgres and a sandbox)"
        );
        return;
    };
    assert!(
        core_bin().exists(),
        "modbit-core at {}",
        core_bin().display()
    );
    let script = vec![
        json!({"calls": [{"name": "plan.update", "args": {"outcome": "work.txt written in the sandbox and read back after a loss", "expected_files": ["work.txt"]}}]}),
        // A turn of work in the guest — the turn boundary checkpoints it.
        json!({"calls": [
            {"name": "shell.exec", "args": {"argv": ["/bin/sh", "-c", "echo v1 > work.txt; echo more >> NOTES.md; echo written"]}},
            {"name": "fs.read", "args": {"path": "work.txt"}}
        ]}),
        // Held until the test has killed the sandbox: this call finds it
        // gone — its outcome is unknown, the sandbox is replaced.
        json!({"gate": true, "calls": [{"name": "shell.exec", "args": {"argv": ["/bin/sh", "-c", "cat work.txt"]}}]}),
        // The model's own retry, in the fresh sandbox.
        json!({"calls": [{"name": "shell.exec", "args": {"argv": ["/bin/sh", "-c", "cat work.txt; cat NOTES.md; echo again"]}}]}),
        json!({"calls": [{"name": "task.complete", "args": {"summary": "recovered", "self_review": {"findings": []}}}]}),
    ];
    let (model_base, seen, gate) = scripted_model_gated(script).await;
    let served = serve(ApiConfig {
        store: store_cfg.clone(),
        token_key: None,
        bind: "127.0.0.1:0".into(),
        rate_capacity: 500,
        rate_per_second: 100.0,
        worker_key: None,
        github_webhook_secret: None,
    })
    .await
    .expect("api");
    let api = Api {
        base: format!("http://{}", served.addr),
        http: reqwest::Client::new(),
        served,
    };
    let store = &api.served.state.store;
    let tenant = store.create_tenant("recovery-test").await.unwrap();
    let (_p, secret) = store.create_principal(tenant, "user", "ada").await.unwrap();
    let (_, tok) = api
        .post("", "/v1/auth/token", json!({"secret": secret}))
        .await;
    let a = tok["access_token"].as_str().unwrap().to_owned();
    let keep = tempfile::tempdir().unwrap();
    let data = match std::env::var("MODBIT_CLOUD_WORKER_TEST_KEEP_DIR") {
        Ok(d) => std::path::PathBuf::from(d).join("m89"),
        Err(_) => keep.path().to_path_buf(),
    };
    let root = repo(&data.join("repo"));
    let gateway = Gateway::start(&store_cfg, &data.join("gateway")).await;
    let (_, created) = api
        .post(
            &a,
            "/v1/sessions",
            json!({"command_id": uuid::Uuid::now_v7().to_string()}),
        )
        .await;
    let sid = created["session_id"].as_str().unwrap().to_owned();
    let (s, task) = api.post(&a, &format!("/v1/sessions/{sid}/tasks"), json!({"command_id": uuid::Uuid::now_v7().to_string(), "goal_text": "write work.txt and read it back", "execution_profile": "cloud_isolated", "workspace_root": root})).await;
    assert_eq!(s, 201, "{task}");
    let worker = start(worker_config(
        &store_cfg,
        "worker-r",
        &data.join("w"),
        &model_base,
        Duration::from_secs(10),
        &gateway,
    ))
    .await
    .expect("worker");
    // The first sandbox, and the checkpoint of the turn that wrote in it.
    let (first_sandbox, checkpoint_id) = until("the turn-boundary checkpoint", 240, async || {
        let (_, evs) = api
            .get(
                &a,
                &format!("/v1/events?session_id={sid}&after=0&limit=1000"),
            )
            .await;
        let events = evs["events"].as_array()?;
        let sandbox = events
            .iter()
            .find(|e| e["envelope"]["event_type"] == "SandboxLeaseAcquired")?["payload"]["sandbox_id"]
            .as_str()?
            .to_owned();
        let cp = events
            .iter()
            .find(|e| e["envelope"]["event_type"] == "CheckpointCommitted")?["payload"]["checkpoint_id"]
            .as_str()?
            .to_owned();
        Some((sandbox, cp))
    })
    .await;
    // The sandbox's process, from the gateway's record; killed outright.
    let (s, rec) = gw_get(
        &gateway,
        "worker-r",
        &format!("/v1/sandboxes/{first_sandbox}?tenant_id={tenant}"),
    )
    .await;
    assert_eq!(s, 200, "{rec}");
    let detail = rec["detail"].as_str().unwrap_or_default().to_owned();
    let pid: u32 = detail
        .split("pid ")
        .nth(1)
        .and_then(|rest| rest.split(|c: char| !c.is_ascii_digit()).next())
        .and_then(|d| d.parse().ok())
        .unwrap_or_else(|| panic!("a pid in the sandbox record's detail: {detail}"));
    let killed = std::process::Command::new("kill")
        .args(["-9", &pid.to_string()])
        .status()
        .unwrap();
    assert!(killed.success(), "kill -9 {pid}");
    eprintln!("killed sandbox {first_sandbox} (pid {pid}); opening the gate");
    tokio::time::sleep(Duration::from_millis(500)).await;
    gate.notify_waiters();
    until(
        "the task to reach review after the recovery",
        240,
        async || {
            let (_, v) = api.get(&a, &format!("/v1/sessions/{sid}")).await;
            (v["tasks"][0]["state"] == "READY_FOR_REVIEW").then_some(())
        },
    )
    .await;
    let (_, evs) = api
        .get(
            &a,
            &format!("/v1/events?session_id={sid}&after=0&limit=2000"),
        )
        .await;
    let events = evs["events"].as_array().unwrap().clone();
    let of = |t: &str| -> Vec<Value> {
        events
            .iter()
            .filter(|e| e["envelope"]["event_type"] == t)
            .map(|e| e["payload"].clone())
            .collect()
    };
    let lost = of("SandboxLost");
    assert_eq!(lost.len(), 1, "one loss on the log: {lost:?}");
    assert_eq!(lost[0]["sandbox_id"], first_sandbox.as_str(), "{lost:?}");
    let leases = of("SandboxLeaseAcquired");
    assert_eq!(
        leases.len(),
        2,
        "the lost sandbox and its replacement: {leases:?}"
    );
    let second_sandbox = leases[1]["sandbox_id"].as_str().unwrap().to_owned();
    assert_ne!(second_sandbox, first_sandbox);
    let restored = of("SandboxRestored");
    assert_eq!(restored.len(), 1, "{restored:?}");
    assert_eq!(
        restored[0]["sandbox_id"],
        second_sandbox.as_str(),
        "{restored:?}"
    );
    assert_eq!(
        restored[0]["replaced"],
        first_sandbox.as_str(),
        "{restored:?}"
    );
    assert_eq!(
        restored[0]["checkpoint_id"],
        checkpoint_id.as_str(),
        "{restored:?}"
    );
    assert!(
        restored[0]["files_written"].as_u64().unwrap_or(0) >= 2,
        "work.txt and NOTES.md restored: {restored:?}"
    );
    // The call in flight at the loss: unknown, never replayed by the Core.
    let unknown = of("ToolCallUnknownOutcome");
    assert_eq!(unknown.len(), 1, "{unknown:?}");
    let checkpoints = of("CheckpointCommitted");
    assert!(
        checkpoints
            .iter()
            .any(|c| c["checkpoint_id"] == checkpoint_id.as_str()),
        "{checkpoints:?}"
    );
    // The gateway's record of the lost sandbox.
    let (_, rec) = gw_get(
        &gateway,
        "worker-r",
        &format!("/v1/sandboxes/{first_sandbox}?tenant_id={tenant}"),
    )
    .await;
    assert_eq!(rec["state"], "LOST", "{rec}");
    // What the model saw.
    let bodies = seen.lock().unwrap().clone();
    let tool_texts: Vec<String> = bodies.last().unwrap()["messages"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|m| m["role"] == "tool")
        .map(|m| m["content"].as_str().unwrap_or_default().to_owned())
        .collect();
    eprintln!("tool results:\n{}", tool_texts.join("\n----\n"));
    let in_flight = &tool_texts[3];
    assert!(
        in_flight.contains("UNKNOWN")
            && in_flight.contains("sandbox_recovery:")
            && in_flight.contains(&second_sandbox),
        "the call at the loss: unknown, and the recovery told: {in_flight}"
    );
    assert!(
        in_flight.contains(&checkpoint_id),
        "the recovery names the checkpoint restored: {in_flight}"
    );
    let retry = &tool_texts[4];
    assert!(
        retry.contains("status: SUCCESS")
            && retry.contains("v1")
            && retry.contains("more")
            && retry.contains(&second_sandbox),
        "the retry ran in the fresh sandbox on the restored worktree: {retry}"
    );
    worker.stop().await;
    gateway.served.stop();
}

/// The structured output of a tool result the model saw (`output:` JSON).
fn structured(text: &str) -> Value {
    text.split("output:\n")
        .nth(1)
        .and_then(|j| serde_json::from_str::<Value>(j.trim()).ok())
        .unwrap_or(Value::Null)
}

/// The tool results the model saw in its last request, in order.
fn tool_results(seen: &Arc<Mutex<Vec<Value>>>) -> Vec<String> {
    let bodies = seen.lock().unwrap();
    bodies
        .last()
        .and_then(|b| b["messages"].as_array())
        .map(|m| {
            m.iter()
                .filter(|x| x["role"] == "tool")
                .map(|x| x["content"].as_str().unwrap_or_default().to_owned())
                .collect()
        })
        .unwrap_or_default()
}

/// IMP-EV-0109 (docs/21 "Execution profiles", REQ-EV-0109: one canonical
/// execution interface with local and cloud adapters): the same tool
/// fixture — a process, a file read, a listing, a stat, a read outside the
/// root — runs under `local_trusted` on a local Core (the host's workspace
/// and terminal broker) and under `cloud_isolated` on a cloud worker (the
/// sandbox), with equivalent effects — the same structured outputs, save
/// the sandbox's identity — and the same event semantics per call.
#[tokio::test]
async fn qual_ev_0109_the_same_tool_fixture_runs_locally_and_in_the_cloud_with_equivalent_effect_and_event_semantics()
 {
    let Some(store_cfg) = store_config().await else {
        eprintln!(
            "SKIPPED: MODBIT_CLOUD_TEST_DATABASE_URL unset (the hosted cloud job runs this against Postgres and a sandbox)"
        );
        return;
    };
    let fixture = || {
        vec![
            json!({"calls": [{"name": "plan.update", "args": {"outcome": "out.txt written and read back", "expected_files": ["out.txt"]}}]}),
            json!({"calls": [
                {"name": "shell.exec", "args": {"argv": ["/bin/sh", "-c", "printf 'hi\\n' > out.txt; cat out.txt; echo status=$?"]}},
                {"name": "fs.read", "args": {"path": "out.txt"}},
                {"name": "fs.list", "args": {"path": "."}},
                {"name": "fs.stat", "args": {"path": "out.txt"}},
                {"name": "fs.read", "args": {"path": "../../etc/passwd"}}
            ]}),
            json!({"calls": [{"name": "task.complete", "args": {"summary": "done", "self_review": {"findings": []}}}]}),
        ]
    };
    let keep = tempfile::tempdir().unwrap();
    let data = match std::env::var("MODBIT_CLOUD_WORKER_TEST_KEEP_DIR") {
        Ok(d) => std::path::PathBuf::from(d).join("ev0109"),
        Err(_) => keep.path().to_path_buf(),
    };
    // ---- local: a local Core, the host's workspace ----
    let (local_model, local_seen) = scripted_model(fixture()).await;
    let local_root = repo(&data.join("local-repo"));
    let laptop = modbit_cloud_worker::core_process::CoreProcess::spawn_local(
        &core_bin(),
        &data.join("local-core"),
    )
    .expect("local core");
    let mut lc = laptop
        .client_as(modbit_protocol::v1::ClientKind::Cli)
        .await
        .expect("local client");
    let _: modbit_protocol::v1::ProviderConfigured = cmd(
        &mut lc,
        "ConfigureProvider",
        modbit_protocol::v1::ConfigureProvider {
            provider: "openai".into(),
            api_key: String::new(),
            base_url: local_model.clone(),
        }
        .encode_to_vec(),
        None,
    )
    .await
    .expect("provider");
    let created: modbit_protocol::v1::SessionCreated = cmd(
        &mut lc,
        "CreateSession",
        modbit_protocol::v1::CreateSession { space_id: None }.encode_to_vec(),
        None,
    )
    .await
    .expect("session");
    let local_sid = created.session_id.clone().unwrap();
    let lease: modbit_protocol::v1::SessionLeaseAcquired = cmd(
        &mut lc,
        "AcquireSessionLease",
        modbit_protocol::v1::AcquireSessionLease {
            session_id: Some(local_sid.clone()),
            owner: "laptop".into(),
        }
        .encode_to_vec(),
        None,
    )
    .await
    .expect("lease");
    let g = Some(lease.lease_generation);
    let task: modbit_protocol::v1::TaskCreated = cmd(
        &mut lc,
        "CreateTask",
        modbit_protocol::v1::CreateTask {
            session_id: Some(local_sid.clone()),
            goal_text: "write out.txt and read it back".into(),
            workspace_id: None,
            execution_profile: "local_trusted".into(),
            origin: "cli".into(),
            workspace_root: local_root.clone(),
            issue_url: String::new(),
            issue_json: String::new(),
        }
        .encode_to_vec(),
        g,
    )
    .await
    .expect("task");
    let local_tid = task.task_id.clone().unwrap();
    let _: modbit_protocol::v1::TaskRunStarted = cmd(
        &mut lc,
        "StartTask",
        modbit_protocol::v1::StartTask {
            task_id: Some(local_tid.clone()),
            endpoint: "openai".into(),
            model: "gpt-5".into(),
            max_turns: 0,
            max_tool_calls: 0,
            max_no_progress_turns: 0,
            skills: vec![],
        }
        .encode_to_vec(),
        g,
    )
    .await
    .expect("started");
    until("the local task to reach review", 120, async || {
        let snap: modbit_protocol::v1::SessionSnapshot = cmd(
            &mut lc,
            "GetSessionSnapshot",
            modbit_protocol::v1::GetSessionSnapshot {
                session_id: Some(local_sid.clone()),
            }
            .encode_to_vec(),
            None,
        )
        .await
        .ok()?;
        (snap.tasks[0].state == "ReadyForReview").then_some(())
    })
    .await;
    // The local log's tool-call events, in order.
    let local_events: Vec<(String, String)> = {
        lc.subscribe(local_sid.clone(), 0).await.unwrap();
        let mut out = Vec::new();
        loop {
            let Ok(Ok(Some(f))) =
                tokio::time::timeout(Duration::from_secs(3), lc.next_event()).await
            else {
                break;
            };
            let e = f.event.unwrap();
            if e.event_type.starts_with("ToolCall") {
                out.push((
                    e.event_type.clone(),
                    e.aggregate_id
                        .map(|i| hex::encode(i.value))
                        .unwrap_or_default(),
                ));
            }
        }
        out
    };
    laptop.stop();
    // ---- cloud: the API, a worker, a sandbox ----
    let (cloud_model, cloud_seen) = scripted_model(fixture()).await;
    let served = serve(ApiConfig {
        store: store_cfg.clone(),
        token_key: None,
        bind: "127.0.0.1:0".into(),
        rate_capacity: 500,
        rate_per_second: 100.0,
        worker_key: None,
        github_webhook_secret: None,
    })
    .await
    .expect("api");
    let api = Api {
        base: format!("http://{}", served.addr),
        http: reqwest::Client::new(),
        served,
    };
    let store = &api.served.state.store;
    let tenant = store.create_tenant("equivalence-test").await.unwrap();
    let (_p, secret) = store.create_principal(tenant, "user", "ada").await.unwrap();
    let (_, tok) = api
        .post("", "/v1/auth/token", json!({"secret": secret}))
        .await;
    let a = tok["access_token"].as_str().unwrap().to_owned();
    let cloud_root = repo(&data.join("cloud-repo"));
    let gateway = Gateway::start(&store_cfg, &data.join("gateway")).await;
    let (_, created) = api
        .post(
            &a,
            "/v1/sessions",
            json!({"command_id": uuid::Uuid::now_v7().to_string()}),
        )
        .await;
    let sid = created["session_id"].as_str().unwrap().to_owned();
    let (s, task) = api.post(&a, &format!("/v1/sessions/{sid}/tasks"), json!({"command_id": uuid::Uuid::now_v7().to_string(), "goal_text": "write out.txt and read it back", "execution_profile": "cloud_isolated", "workspace_root": cloud_root})).await;
    assert_eq!(s, 201, "{task}");
    let worker = start(worker_config(
        &store_cfg,
        "worker-e",
        &data.join("w"),
        &cloud_model,
        Duration::from_secs(10),
        &gateway,
    ))
    .await
    .expect("worker");
    until("the cloud task to reach review", 240, async || {
        let (_, v) = api.get(&a, &format!("/v1/sessions/{sid}")).await;
        (v["tasks"][0]["state"] == "READY_FOR_REVIEW").then_some(())
    })
    .await;
    let (_, evs) = api
        .get(
            &a,
            &format!("/v1/events?session_id={sid}&after=0&limit=2000"),
        )
        .await;
    let cloud_events: Vec<(String, String)> = evs["events"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| {
            e["envelope"]["event_type"]
                .as_str()
                .unwrap_or_default()
                .starts_with("ToolCall")
        })
        .map(|e| {
            (
                e["envelope"]["event_type"].as_str().unwrap().to_owned(),
                e["envelope"]["aggregate_id"].to_string(),
            )
        })
        .collect();
    worker.stop().await;
    gateway.served.stop();
    // ---- equivalence ----
    let local = tool_results(&local_seen);
    let cloud = tool_results(&cloud_seen);
    eprintln!(
        "local:\n{}\n====\ncloud:\n{}",
        local.join("\n----\n"),
        cloud.join("\n----\n")
    );
    assert_eq!(local.len(), cloud.len(), "the same calls were made");
    assert_eq!(local.len(), 6);
    // The effect of each call, in the fields the contract carries on both
    // sides (an adapter adds its own — the sandbox's id, the host's
    // workspace revision — never changes these).
    fn effect(tool: &str, v: &Value) -> Value {
        match tool {
            "shell.exec" => {
                json!({"exit_code": v["exit_code"], "stdout_preview": v["stdout_preview"], "cancelled": v["cancelled"], "timed_out": v["timed_out"], "signal": v["signal"]})
            }
            "fs.read" => {
                json!({"path": v["path"], "content": v["content"], "byte_length": v["byte_length"], "content_hash": v["content_hash"], "truncated": v["truncated"], "encoding": v["encoding"]})
            }
            "fs.list" => {
                let mut entries: Vec<Value> = v["entries"]
                    .as_array()
                    .cloned()
                    .unwrap_or_default()
                    .into_iter()
                    .filter(|e| e["path"] != ".git")
                    .map(|e| json!({"path": e["path"], "kind": e["kind"], "size": e["size"]}))
                    .collect();
                entries.sort_by_key(|e| e["path"].to_string());
                json!({"entries": entries})
            }
            "fs.stat" => json!({"path": v["path"], "kind": v["kind"], "size": v["size"]}),
            _ => Value::Null,
        }
    }
    let tools = [
        "plan.update",
        "shell.exec",
        "fs.read",
        "fs.list",
        "fs.stat",
        "fs.read",
    ];
    let status_of = |t: &str| t.lines().next().unwrap_or_default().to_owned();
    for (i, (l, c)) in local.iter().zip(cloud.iter()).enumerate() {
        assert_eq!(
            status_of(l),
            status_of(c),
            "call {i}: the same status\nlocal: {l}\ncloud: {c}"
        );
        if i == 5 {
            // The read outside the root: refused with the same code on both.
            assert!(
                l.contains("error_code: PATH_OUTSIDE_ROOT")
                    && c.contains("error_code: PATH_OUTSIDE_ROOT"),
                "call {i}\nlocal: {l}\ncloud: {c}"
            );
            continue;
        }
        if i == 0 {
            continue;
        }
        let (lv, cv) = (
            effect(tools[i], &structured(l)),
            effect(tools[i], &structured(c)),
        );
        assert!(tools[i] != "fs.read" || lv["content"] != Value::Null, "{l}");
        assert_eq!(
            lv, cv,
            "call {i} ({}): the same effect\nlocal: {l}\ncloud: {c}",
            tools[i]
        );
    }
    // The process: its exit and its output on both sides.
    let shell = structured(&local[1]);
    assert_eq!(shell["exit_code"], 0);
    assert_eq!(shell["stdout_preview"], "hi\nstatus=0\n");
    // The same event semantics per call: the sequence of tool-call event
    // types, call by call, is identical.
    let seq = |evs: &[(String, String)]| -> Vec<Vec<String>> {
        let mut by_call: Vec<(String, Vec<String>)> = Vec::new();
        for (t, id) in evs {
            match by_call.iter_mut().find(|(i, _)| i == id) {
                Some((_, v)) => v.push(t.clone()),
                None => by_call.push((id.clone(), vec![t.clone()])),
            }
        }
        by_call.into_iter().map(|(_, v)| v).collect()
    };
    let (ls, cs) = (seq(&local_events), seq(&cloud_events));
    assert_eq!(
        ls.len(),
        cs.len(),
        "the same number of calls on both logs: {ls:?} vs {cs:?}"
    );
    assert_eq!(ls, cs, "the same event sequence per call");
    assert!(
        ls.iter()
            .any(|v| v.iter().any(|t| t == "ToolCallSucceeded"))
            && ls.iter().any(|v| v.iter().any(|t| t == "ToolCallFailed")),
        "{ls:?}"
    );
}

/// IMP-EV-0024 (REQ-EV-0024, an experiment; docs/24): the worker's link to
/// the Cloud API is outbound only — the worker binds no port — under the
/// same authenticated worker protocol the gateway speaks. Proven here:
/// identity (a token that does not verify never links; the right one
/// does), reconnect (a dropped link comes back on its own), revocation (an
/// expired token cannot link again once the live link is dropped) and
/// tenant isolation (a request over the link for a session this worker
/// does not hold is refused, and the API only ever asks the holder).
#[tokio::test]
async fn qual_ev_0024_the_workers_outbound_link_proves_identity_reconnect_revocation_and_tenant_isolation()
 {
    let Some(store_cfg) = store_config().await else {
        eprintln!(
            "SKIPPED: MODBIT_CLOUD_TEST_DATABASE_URL unset (the hosted cloud job runs this against Postgres)"
        );
        return;
    };
    let (model_base, _seen) = scripted_model(vec![]).await;
    let key: Vec<u8> = uuid::Uuid::now_v7()
        .as_bytes()
        .iter()
        .chain(uuid::Uuid::new_v4().as_bytes())
        .copied()
        .collect();
    let worker_key = modbit_sandbox::auth::WorkerKey::new(key.clone());
    let served = serve(ApiConfig {
        store: store_cfg.clone(),
        token_key: None,
        bind: "127.0.0.1:0".into(),
        rate_capacity: 500,
        rate_per_second: 100.0,
        worker_key: Some(key),
        github_webhook_secret: None,
    })
    .await
    .expect("api");
    let api = Api {
        base: format!("http://{}", served.addr),
        http: reqwest::Client::new(),
        served,
    };
    let keep = tempfile::tempdir().unwrap();
    let data = keep.path().to_path_buf();
    let gateway = Gateway::start(&store_cfg, &data.join("gateway")).await;
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64;
    let link_cfg = |worker: &str, token: String| {
        let mut cfg = worker_config(
            &store_cfg,
            worker,
            &data.join(worker),
            &model_base,
            Duration::from_secs(10),
            &gateway,
        );
        cfg.api = Some(modbit_cloud_worker::ApiLinkConfig {
            base_url: api.base.clone(),
            worker_token: token,
        });
        cfg
    };
    // Identity: a token under another key never links.
    let other = modbit_sandbox::auth::WorkerKey::random();
    let impostor = start(link_cfg(
        "worker-x",
        other.issue(&modbit_sandbox::auth::WorkerClaims {
            worker_id: "worker-x".into(),
            exp_ms: i64::MAX,
        }),
    ))
    .await
    .expect("worker");
    tokio::time::sleep(Duration::from_secs(3)).await;
    assert!(
        !api.served.state.workers.linked("worker-x").await,
        "a worker whose token does not verify is not linked"
    );
    impostor.stop().await;
    // Identity: the right token links, from an outbound connection.
    let short_lived = worker_key.issue(&modbit_sandbox::auth::WorkerClaims {
        worker_id: "worker-l".into(),
        exp_ms: now_ms + 12_000,
    });
    let worker = start(link_cfg("worker-l", short_lived))
        .await
        .expect("worker");
    until("the worker's link", 30, async || {
        api.served
            .state
            .workers
            .linked("worker-l")
            .await
            .then_some(())
    })
    .await;
    // Tenant isolation: over the link, a session this worker does not hold
    // is refused before anything is touched.
    let link = api.served.state.workers.get("worker-l").await.unwrap();
    let foreign = modbit_domain::SessionId::new();
    let bsid = hex::encode(uuid::Uuid::now_v7().as_bytes());
    let answer = link
        .ask(
            json!({"kind": "input", "session_id": foreign.to_string(), "browser_session_id": bsid, "input": {"kind": "click", "x": 1, "y": 1}}),
            Duration::from_secs(10),
        )
        .await
        .expect("an answer");
    assert_eq!(answer["status"], "REJECTED", "{answer}");
    assert_eq!(answer["code"], "NOT_HOSTED", "{answer}");
    // Reconnect: the live link dropped by the API comes back on its own
    // while the token still verifies.
    assert!(api.served.state.workers.disconnect("worker-l").await);
    until("the link to come back", 30, async || {
        let l = api.served.state.workers.get("worker-l").await?;
        (!Arc::ptr_eq(&l, &link)).then_some(())
    })
    .await;
    // Revocation: once the token has expired, a dropped link cannot come
    // back — the worker retries and is refused each time.
    until("the token to expire", 30, async || {
        let expired = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64
            > now_ms + 12_000;
        expired.then_some(())
    })
    .await;
    assert!(api.served.state.workers.disconnect("worker-l").await);
    tokio::time::sleep(Duration::from_secs(6)).await;
    assert!(
        !api.served.state.workers.linked("worker-l").await,
        "an expired token does not link again"
    );
    worker.stop().await;
    gateway.served.stop();
}

/// IMP-EV-0072 (REQ-EV-0072; docs/24): capabilities are negotiated before
/// dispatch. A worker announces what it serves (the cloud profile's tools,
/// `browser.control` when its gateway's guests have a browser); a task
/// names what it requires. A requirement no live worker serves is an
/// explicit rejection (`NO_CAPABLE_WORKER`); a served one keeps the
/// session for a worker that serves it — an older worker never claims it;
/// and a task an older worker does host runs with a compatible projection:
/// no browser tool offered, no browser asked of its sandbox.
#[tokio::test]
async fn qual_ev_0072_capabilities_are_negotiated_before_dispatch_an_older_worker_gets_a_compatible_projection_or_the_task_is_refused()
 {
    let Some(store_cfg) = store_config().await else {
        eprintln!(
            "SKIPPED: MODBIT_CLOUD_TEST_DATABASE_URL unset (the hosted cloud job runs this against Postgres and a sandbox)"
        );
        return;
    };
    // The same script on both: a look at the browser — outside an older
    // worker's projection, inside a full worker's.
    let script = || {
        vec![
            json!({"calls": [{"name": "plan.update", "args": {"outcome": "noted", "expected_files": []}}]}),
            json!({"calls": [{"name": "browser.snapshot", "args": {}}]}),
            json!({"calls": [{"name": "task.complete", "args": {"summary": "noted", "self_review": {"findings": []}}}]}),
        ]
    };
    let (old_model, old_seen) = scripted_model(script()).await;
    let (new_model, new_seen) = scripted_model(script()).await;
    let served = serve(ApiConfig {
        store: store_cfg.clone(),
        token_key: None,
        bind: "127.0.0.1:0".into(),
        rate_capacity: 500,
        rate_per_second: 100.0,
        worker_key: None,
        github_webhook_secret: None,
    })
    .await
    .expect("api");
    let api = Api {
        base: format!("http://{}", served.addr),
        http: reqwest::Client::new(),
        served,
    };
    let store = &api.served.state.store;
    let tenant = store.create_tenant("negotiation-test").await.unwrap();
    let (_p, secret) = store.create_principal(tenant, "user", "ada").await.unwrap();
    let (_, tok) = api
        .post("", "/v1/auth/token", json!({"secret": secret}))
        .await;
    let a = tok["access_token"].as_str().unwrap().to_owned();
    let keep = tempfile::tempdir().unwrap();
    let data = match std::env::var("MODBIT_CLOUD_WORKER_TEST_KEEP_DIR") {
        Ok(d) => std::path::PathBuf::from(d).join("ev0072"),
        Err(_) => keep.path().to_path_buf(),
    };
    let gateway = Gateway::start(&store_cfg, &data.join("gateway")).await;
    // The gateway's guests have a browser here (the host's Chrome, or the
    // image's): a full worker serves browser.control; the older one does not.
    let (_, health) = gw_get(&gateway, "worker-old", "/v1/health").await;
    assert!(
        health["features"]
            .as_array()
            .is_some_and(|f| f.iter().any(|x| x == "browser")),
        "the gateway declares its guests' browser: {health}"
    );
    // An older worker: the cloud profile's tools, no browser.
    let mut old_cfg = worker_config(
        &store_cfg,
        "worker-old",
        &data.join("old"),
        &old_model,
        Duration::from_secs(10),
        &gateway,
    );
    old_cfg.capabilities = Some(
        [
            "fs.read",
            "fs.write",
            "git.read",
            "shell.exec",
            "network.egress",
            "secret.use",
        ]
        .iter()
        .map(|s| (*s).to_owned())
        .collect(),
    );
    let old = start(old_cfg).await.expect("older worker");
    let live = store.live_workers(60_000).await.unwrap();
    assert!(
        live.iter()
            .any(|(w, caps)| w == "worker-old" && !caps.iter().any(|c| c == "browser.control")),
        "the older worker announced what it serves: {live:?}"
    );
    // Explicit rejection: a task needing the browser while only the older
    // worker is live.
    let (_, created) = api
        .post(
            &a,
            "/v1/sessions",
            json!({"command_id": uuid::Uuid::now_v7().to_string()}),
        )
        .await;
    let needy_sid = created["session_id"].as_str().unwrap().to_owned();
    let root = repo(&data.join("repo"));
    let (s, refused) = api.post(&a, &format!("/v1/sessions/{needy_sid}/tasks"), json!({"command_id": uuid::Uuid::now_v7().to_string(), "goal_text": "browse", "execution_profile": "cloud_isolated", "workspace_root": root, "capabilities": ["browser.control"]})).await;
    assert_eq!(s, 409, "{refused}");
    assert_eq!(refused["code"], "NO_CAPABLE_WORKER", "{refused}");
    // Compatible projection: a task with no browser requirement, hosted by
    // the older worker, runs — with no browser tool offered and no browser
    // asked of its sandbox.
    let (_, created) = api
        .post(
            &a,
            "/v1/sessions",
            json!({"command_id": uuid::Uuid::now_v7().to_string()}),
        )
        .await;
    let plain_sid = created["session_id"].as_str().unwrap().to_owned();
    let plain_root = repo(&data.join("plain-repo"));
    let (s, t) = api.post(&a, &format!("/v1/sessions/{plain_sid}/tasks"), json!({"command_id": uuid::Uuid::now_v7().to_string(), "goal_text": "note", "execution_profile": "cloud_isolated", "workspace_root": plain_root})).await;
    assert_eq!(s, 201, "{t}");
    until(
        "the plain task to reach review on the older worker",
        240,
        async || {
            let (_, v) = api.get(&a, &format!("/v1/sessions/{plain_sid}")).await;
            (v["tasks"][0]["state"] == "READY_FOR_REVIEW").then_some(())
        },
    )
    .await;
    let (_, evs) = api
        .get(
            &a,
            &format!("/v1/events?session_id={plain_sid}&after=0&limit=2000"),
        )
        .await;
    let events = evs["events"].as_array().unwrap();
    let lease = events
        .iter()
        .find(|e| e["envelope"]["event_type"] == "SandboxLeaseAcquired")
        .expect("a sandbox");
    assert_eq!(
        lease["payload"]["browser"], false,
        "no browser asked of the older worker's sandbox: {lease}"
    );
    assert!(
        !events
            .iter()
            .any(|e| e["envelope"]["event_type"] == "BrowserSessionOpened"),
        "no browser session for a browserless sandbox"
    );
    let old_results = tool_results(&old_seen);
    assert!(
        old_results
            .get(1)
            .is_some_and(|t| t.contains("TOOL_NOT_VISIBLE")),
        "the older worker's task has no browser tool in its surface: {old_results:?}"
    );
    // A full worker joins: the browser task is accepted now, recorded as
    // requiring the browser, and hosted by the full worker — never the
    // older one, which keeps polling.
    let full = start(worker_config(
        &store_cfg,
        "worker-new",
        &data.join("new"),
        &new_model,
        Duration::from_secs(10),
        &gateway,
    ))
    .await
    .expect("full worker");
    until(
        "the full worker to announce browser.control",
        30,
        async || {
            store
                .live_workers(60_000)
                .await
                .ok()?
                .iter()
                .any(|(w, caps)| w == "worker-new" && caps.iter().any(|c| c == "browser.control"))
                .then_some(())
        },
    )
    .await;
    let (s, t) = api.post(&a, &format!("/v1/sessions/{needy_sid}/tasks"), json!({"command_id": uuid::Uuid::now_v7().to_string(), "goal_text": "browse", "execution_profile": "cloud_isolated", "workspace_root": root, "capabilities": ["browser.control"]})).await;
    assert_eq!(s, 201, "{t}");
    let needy = modbit_domain::SessionId::parse(&needy_sid).unwrap();
    assert_eq!(
        store.session_requirements(tenant, needy).await.unwrap(),
        vec!["browser.control".to_owned()]
    );
    until("the browser task to reach review", 240, async || {
        let (_, v) = api.get(&a, &format!("/v1/sessions/{needy_sid}")).await;
        (v["tasks"][0]["state"] == "READY_FOR_REVIEW").then_some(())
    })
    .await;
    assert!(
        matches!(
            full.hosting(needy),
            Some(modbit_cloud_worker::Hosting::Hosting { .. })
                | Some(modbit_cloud_worker::Hosting::Released { .. })
        ),
        "the full worker hosted the browser task: {:?}",
        full.hosting(needy)
    );
    assert!(
        old.hosting(needy).is_none(),
        "the older worker never claimed it: {:?}",
        old.hosting(needy)
    );
    let new_results = tool_results(&new_seen);
    assert!(
        new_results
            .get(1)
            .is_some_and(|t| t.contains("status: SUCCESS") && t.contains("about:blank")),
        "the full worker's task reads its sandbox's browser: {new_results:?}"
    );
    old.stop().await;
    full.stop().await;
    gateway.served.stop();
}

/// PX-011 (QUAL-PX-011, docs/24 "Forge webhook intake", docs/29
/// "Issue-to-task intake") — the worker-relay half: a GitHub App's delivery
/// for a repository mapped to a session a worker holds is relayed with the
/// issue, and the worker's Core makes the same canonical task (origin
/// `forge_webhook`, the issue an untrusted context document) and runs it in
/// its sandbox. The webhook path adds no second task model: the events on
/// the cloud log are exactly `TaskCreated`, `TaskQueued`,
/// `ContextDocumentAttached`, `TaskCreatedFromIssue`.
#[tokio::test]
async fn qual_px_011_a_webhook_for_a_held_session_is_relayed_and_the_worker_makes_the_canonical_task()
 {
    use hmac::{Hmac, KeyInit, Mac};
    use sha2::Sha256;
    const SECRET: &[u8] = b"wh-relay-secret";
    let Some(store_cfg) = store_config().await else {
        eprintln!(
            "SKIPPED: MODBIT_CLOUD_TEST_DATABASE_URL unset (the hosted cloud job runs this against Postgres and a sandbox)"
        );
        return;
    };
    assert!(
        core_bin().exists(),
        "modbit-core at {}",
        core_bin().display()
    );
    // The task the worker's Core will run from the webhook's issue: read the
    // issue text it was given as context, then complete.
    // A short script the worker's Core runs for every task it hosts (the
    // seed task that makes the worker claim the session, and the webhook's
    // task after): plan, then complete.
    let script = vec![
        json!({"calls": [{"name": "plan.update", "args": {"outcome": "done", "expected_files": []}}]}),
        json!({"calls": [{"name": "task.complete", "args": {"summary": "done", "self_review": {"findings": []}}}]}),
    ];
    let (model_base, _seen) = scripted_model(script).await;
    let served = serve(ApiConfig {
        store: store_cfg.clone(),
        token_key: None,
        bind: "127.0.0.1:0".into(),
        rate_capacity: 500,
        rate_per_second: 100.0,
        worker_key: None,
        github_webhook_secret: Some(SECRET.to_vec()),
    })
    .await
    .expect("api");
    let api = Api {
        base: format!("http://{}", served.addr),
        http: reqwest::Client::new(),
        served,
    };
    let tenant = api.served.state.store.create_tenant("wh").await.unwrap();
    let (_p, secret) = api
        .served
        .state
        .store
        .create_principal(tenant, "user", "ada")
        .await
        .unwrap();
    let (_, tok) = api
        .post("", "/v1/auth/token", json!({"secret": secret}))
        .await;
    let a = tok["access_token"].as_str().unwrap().to_owned();
    let keep = tempfile::tempdir().unwrap();
    let data = keep.path().to_path_buf();
    let root = repo(&data.join("repo"));
    let gateway = Gateway::start(&store_cfg, &data.join("gateway")).await;
    let (_, created) = api
        .post(
            &a,
            "/v1/sessions",
            json!({"command_id": uuid::Uuid::now_v7().to_string()}),
        )
        .await;
    let sid = created["session_id"].as_str().unwrap().to_owned();
    // Map acme/widgets → this session.
    let (s, mapped) = api
        .post(&a, "/v1/forge/repositories", json!({"repository": "acme/widgets", "session_id": sid, "workspace_root": root, "execution_profile": "cloud_isolated"}))
        .await;
    assert_eq!(s, 201, "{mapped}");
    // A worker claims a session only when it has ready work: a first,
    // ordinary task makes the session ready and the worker take it, so the
    // webhook that follows finds the session held (and is relayed).
    let c_seed = uuid::Uuid::now_v7().to_string();
    let (s, seed) = api.post(&a, &format!("/v1/sessions/{sid}/tasks"), json!({"command_id": c_seed, "goal_text": "seed", "execution_profile": "cloud_isolated", "workspace_root": root})).await;
    assert_eq!(s, 201, "{seed}");
    let seed_tid = seed["task_id"].as_str().unwrap().to_owned();
    let worker = start(worker_config(
        &store_cfg,
        "worker-wh",
        &data.join("w"),
        &model_base,
        Duration::from_secs(10),
        &gateway,
    ))
    .await
    .expect("worker");
    until("the worker to hold the session", 60, async || {
        let (_, v) = api.get(&a, &format!("/v1/sessions/{sid}")).await;
        (v["lease"]["worker_id"] == "worker-wh").then_some(())
    })
    .await;
    // The forge delivers an opened issue, signed under the app's secret.
    let body = serde_json::to_vec(&json!({
        "action": "opened",
        "issue": {"number": 42, "html_url": "https://github.com/acme/widgets/issues/42", "title": "The widget leaks", "state": "open", "user": {"login": "octo"}, "labels": [], "body": "It leaks on shutdown."},
        "repository": {"full_name": "acme/widgets"},
        "installation": {"id": 7}
    }))
    .unwrap();
    let mut mac = Hmac::<Sha256>::new_from_slice(SECRET).unwrap();
    mac.update(&body);
    let sig = format!("sha256={}", hex::encode(mac.finalize().into_bytes()));
    let r = api
        .http
        .post(format!("{}/v1/forge/github/webhook", api.base))
        .header("content-type", "application/json")
        .header("x-github-delivery", "wh-d-1")
        .header("x-github-event", "issues")
        .header("x-hub-signature-256", sig)
        .body(body)
        .send()
        .await
        .unwrap();
    // The session is held: the delivery is relayed, not appended here.
    assert_eq!(r.status(), 202, "relayed while held");
    let relayed: Value = r.json().await.unwrap();
    assert_eq!(relayed["status"], "PENDING", "{relayed}");
    // The worker's Core made the task from the issue (a second task) and
    // ran it to review.
    let seed_tid_c = seed_tid.clone();
    let tid = until(
        "the webhook's task to reach review on the worker",
        120,
        async || {
            let (_, v) = api.get(&a, &format!("/v1/sessions/{sid}")).await;
            v["tasks"].as_array().and_then(|t| {
                t.iter()
                    .find(|x| {
                        x["state"] == "READY_FOR_REVIEW"
                            && x["task_id"].as_str() != Some(seed_tid_c.as_str())
                    })
                    .and_then(|x| x["task_id"].as_str().map(str::to_owned))
            })
        },
    )
    .await;
    // The canonical intake events are on the cloud log, made by the worker.
    let (_, evs) = api
        .get(
            &a,
            &format!("/v1/events?session_id={sid}&after=0&limit=1000"),
        )
        .await;
    let tid_bytes = json!(
        modbit_domain::TaskId::parse(&tid)
            .unwrap()
            .as_bytes()
            .to_vec()
    );
    let for_task: Vec<&Value> = evs["events"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["envelope"]["aggregate_id"] == tid_bytes)
        .collect();
    let intake: Vec<&str> = for_task
        .iter()
        .map(|e| e["envelope"]["event_type"].as_str().unwrap())
        .take(4)
        .collect();
    assert_eq!(
        intake,
        vec![
            "TaskCreated",
            "TaskQueued",
            "ContextDocumentAttached",
            "TaskCreatedFromIssue"
        ],
        "the worker made the canonical task, no second model: {intake:?}"
    );
    let created_ev = for_task
        .iter()
        .find(|e| e["envelope"]["event_type"] == "TaskCreated")
        .unwrap();
    assert_eq!(
        created_ev["payload"]["origin"], "forge_webhook",
        "{created_ev}"
    );
    assert_eq!(created_ev["payload"]["goal_text"], "The widget leaks (#42)");
    let from_issue = for_task
        .iter()
        .find(|e| e["envelope"]["event_type"] == "TaskCreatedFromIssue")
        .unwrap();
    assert_eq!(from_issue["payload"]["provenance"], "forge_webhook");
    assert_eq!(from_issue["payload"]["number"], 42);
    // The delivery ledger records it relayed, named by the worker's task.
    let (_, ledger) = api.get(&a, "/v1/forge/repositories").await;
    let d = ledger["deliveries"]
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["delivery_id"] == "wh-d-1")
        .unwrap();
    assert_eq!(d["outcome"], "relayed", "{d}");
    worker.stop().await;
    gateway.served.stop();
}
