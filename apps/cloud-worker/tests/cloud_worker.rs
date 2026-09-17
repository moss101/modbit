//! M8.2 qualification (docs/24 "Cloud Core Worker", docs/33 "Session kernel
//! lease", "Cloud worker lifecycle"): a real worker over a real Postgres and
//! MinIO claims the session's fenced lease, spawns the real `modbit-core`
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
                // The n-th step answers the n-th request that follows a tool-calling
                // turn (a step may carry several calls).
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

/// The Sandbox Gateway the workers' Cores provision from (M8.5): the
/// MicroVM backend where the job provides Firecracker, the kernel and the
/// signed image; the reference backend (its guest signed by a key of the
/// test's own) elsewhere.
struct Gateway {
    base_url: String,
    served: modbit_sandbox_gateway::Served,
    /// `microvm` | `reference`.
    backend: &'static str,
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
                    },
                    "reference",
                )
            }
        };
        let served = modbit_sandbox_gateway::serve(modbit_sandbox_gateway::Config {
            store: store.clone(),
            worker_key: None,
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
        }
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
            "SKIPPED: MODBIT_CLOUD_TEST_DATABASE_URL unset (the hosted cloud job runs this against Postgres and MinIO)"
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
            {"name": "shell.exec", "args": {"argv": ["/bin/sh", "-c", "echo x > .git/hooks/pre-commit && echo wrote-hook || echo hook-refused"]}}
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
    let worker = start(worker_config(
        &store_cfg,
        "worker-s",
        &data.join("w"),
        &model_base,
        Duration::from_secs(10),
        &gateway,
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
        !payload.to_string().to_lowercase().contains("credential"),
        "{payload}"
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
