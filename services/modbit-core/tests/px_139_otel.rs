//! PX-139 (QUAL-PX-139) on the real Core: OpenTelemetry export derived from
//! the canonical log and the accounting record, child cost rolled up, a
//! collector that fails without costing the run anything, and component
//! health that survives a restart.
//!
//! Real: the `modbit-core` binary and its socket, the SQLite store, real
//! child tasks in real worktrees, the exporter's real HTTP requests to a real
//! local OTLP collector on a real socket (a stub that records every request,
//! and can hang, refuse or disappear), the priced model registry. Stand-in:
//! the model, a scripted OpenAI-compatible server. Unix only (the child
//! tasks run `sh`-free git worktrees but the suite shares the Unix harness).
#![cfg(unix)]

mod px_common;

use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use modbit_protocol::client::Client;
use modbit_protocol::v1::{
    ActivateModelRegistry, ComponentHealthList, GetComponentHealth, GetRequestOutcome, Id,
    ModelRegistryView, RequestOutcomeView,
};
use prost::Message;
use px_common::*;
use serde_json::{Value, json};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

// --------------------------------------------------------------- collector

#[derive(Clone, Debug)]
struct Received {
    path: String,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
}

/// How the collector treats a request.
const OK: u8 = 0;
const HANG: u8 = 1;
const RESET: u8 = 2;

struct Collector {
    base: String,
    seen: Arc<Mutex<Vec<Received>>>,
    mode: Arc<AtomicU8>,
    task: tokio::task::JoinHandle<()>,
}

impl Collector {
    async fn start(mode: u8) -> Self {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let seen: Arc<Mutex<Vec<Received>>> = Arc::default();
        let mode = Arc::new(AtomicU8::new(mode));
        let (seen2, mode2) = (Arc::clone(&seen), Arc::clone(&mode));
        let task = tokio::spawn(async move {
            loop {
                let Ok((mut sock, _)) = listener.accept().await else {
                    return;
                };
                let (seen, mode) = (Arc::clone(&seen2), Arc::clone(&mode2));
                tokio::spawn(async move {
                    let mut buf = Vec::new();
                    let mut tmp = [0u8; 16 * 1024];
                    let (head_end, len, head) = loop {
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
                    while buf.len() < head_end + len {
                        let n = sock.read(&mut tmp).await.unwrap_or(0);
                        if n == 0 {
                            break;
                        }
                        buf.extend_from_slice(&tmp[..n]);
                    }
                    let mut lines = head.lines();
                    let path = lines
                        .next()
                        .and_then(|l| l.split_whitespace().nth(1))
                        .unwrap_or_default()
                        .to_owned();
                    let headers = lines
                        .filter_map(|l| {
                            let (k, v) = l.split_once(':')?;
                            Some((k.trim().to_lowercase(), v.trim().to_owned()))
                        })
                        .collect();
                    seen.lock().unwrap().push(Received {
                        path,
                        headers,
                        body: buf[head_end..(head_end + len).min(buf.len())].to_vec(),
                    });
                    match mode.load(Ordering::SeqCst) {
                        HANG => tokio::time::sleep(Duration::from_secs(120)).await,
                        RESET => {}
                        _ => {
                            let _ = sock
                                .write_all(
                                    b"HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: 2\r\nconnection: close\r\n\r\n{}",
                                )
                                .await;
                            let _ = sock.flush().await;
                        }
                    }
                });
            }
        });
        Collector {
            base: format!("http://127.0.0.1:{port}"),
            seen,
            mode,
            task,
        }
    }

    /// The collector disappears: its port refuses connections.
    fn stop(&self) {
        self.task.abort();
    }

    fn requests(&self) -> Vec<Received> {
        self.seen.lock().unwrap().clone()
    }

    fn spans(&self) -> Vec<Value> {
        self.requests()
            .iter()
            .filter(|r| r.path == "/v1/traces")
            .flat_map(|r| {
                let v: Value = serde_json::from_slice(&r.body).unwrap_or_default();
                v["resourceSpans"]
                    .as_array()
                    .cloned()
                    .unwrap_or_default()
                    .into_iter()
                    .flat_map(|rs| rs["scopeSpans"].as_array().cloned().unwrap_or_default())
                    .flat_map(|ss| ss["spans"].as_array().cloned().unwrap_or_default())
                    .collect::<Vec<_>>()
            })
            .collect()
    }

    fn set_mode(&self, mode: u8) {
        self.mode.store(mode, Ordering::SeqCst);
    }
}

fn attr(span: &Value, key: &str) -> Option<Value> {
    span["attributes"].as_array()?.iter().find_map(|a| {
        (a["key"] == key).then(|| {
            let v = &a["value"];
            if let Some(s) = v["stringValue"].as_str() {
                json!(s)
            } else if let Some(i) = v["intValue"].as_str() {
                json!(i.parse::<i64>().unwrap_or(0))
            } else if let Some(b) = v["boolValue"].as_bool() {
                json!(b)
            } else {
                v["doubleValue"].clone()
            }
        })
    })
}

fn int(span: &Value, key: &str) -> i64 {
    attr(span, key).and_then(|v| v.as_i64()).unwrap_or(-1)
}

// ------------------------------------------------------------------ helpers

fn accounting_registry(
    generation: &str,
    key: &ed25519_dalek::SigningKey,
    models: &[(&str, u64, u64, u64)],
) -> String {
    use ed25519_dalek::Signer;
    use modbit_providers::registry::{
        Economics, Governance, Latency, QualityFloor, REGISTRY_SCHEMA_VERSION, RegistryDocument,
        RegistryEntry, SignedRegistry,
    };
    let now = modbit_domain::Timestamp::now().0;
    let document = RegistryDocument {
        promotion: None,
        schema_version: REGISTRY_SCHEMA_VERSION,
        registry_generation: generation.into(),
        stats_version: "stats-1".into(),
        issued_at_ms: now - 60_000,
        expires_at_ms: now + 86_400_000,
        quality_floors: vec![QualityFloor {
            mode: "auto".into(),
            min_quality: 0.72,
            max_cost_minor: 5_000,
            currency: "USD".into(),
            scale: 2,
        }],
        entries: models
            .iter()
            .map(|(model, input, cached, output)| RegistryEntry {
                endpoint: "openai".into(),
                provider: "openai".into(),
                family: "gpt-5".into(),
                model: (*model).into(),
                roles: vec!["solver".into(), "reviewer".into()],
                input_modalities: vec!["text".into()],
                context_tokens: 400_000,
                max_output_tokens: 64_000,
                tools: true,
                vision: false,
                reasoning: true,
                structured_output: true,
                economics: Economics {
                    input_per_mtok_minor: *input,
                    output_per_mtok_minor: *output,
                    currency: "USD".into(),
                    scale: 2,
                    cached_input_per_mtok_minor: Some(*cached),
                    cache_write_per_mtok_minor: None,
                    cache_ttl_ms: None,
                },
                latency: Latency {
                    p50_ms: 900,
                    p95_ms: 4_200,
                },
                governance: Governance {
                    data_residency: "us".into(),
                    retains_prompts: false,
                    allowed_profiles: vec![],
                },
                revoked: false,
                fallbacks: vec![],
            })
            .collect(),
    };
    let json = serde_json::to_string(&document).unwrap();
    serde_json::to_string(&SignedRegistry {
        key_id: "ops".into(),
        signature_hex: hex::encode(key.sign(json.as_bytes()).to_bytes()),
        document_json: json,
    })
    .unwrap()
}

async fn activate_registry(c: &mut Client, signed: &str) {
    let ack = c
        .command(envelope(
            rand_id(),
            "ActivateModelRegistry",
            ActivateModelRegistry {
                signed_json: signed.to_owned(),
                expected_generation: String::new(),
            }
            .encode_to_vec(),
        ))
        .await
        .unwrap();
    let r: ModelRegistryView = Client::result(&ack).unwrap();
    assert!(r.active, "{r:?}");
}

async fn outcome(c: &mut Client, task: &Id) -> Value {
    let ack = c
        .command(envelope(
            rand_id(),
            "GetRequestOutcome",
            GetRequestOutcome {
                task_id: Some(task.clone()),
            }
            .encode_to_vec(),
        ))
        .await
        .unwrap();
    let v: RequestOutcomeView = Client::result(&ack).unwrap();
    serde_json::from_str(&v.record_json).unwrap()
}

async fn health(core: &CoreProcess) -> ComponentHealthList {
    let mut c = core.client().await;
    let ack = c
        .command(envelope(
            rand_id(),
            "GetComponentHealth",
            GetComponentHealth {}.encode_to_vec(),
        ))
        .await
        .unwrap();
    Client::result(&ack).unwrap()
}

fn plan() -> Value {
    json!({"calls": [{"name": "plan.update", "args": {"outcome": "note", "expected_files": ["notes.md"]}}]})
}

fn complete() -> Value {
    json!({"calls": [{"name": "task.complete", "args": {"summary": "done", "self_review": {"findings": []}}}]})
}

fn child_script(dir: &str, file: &str) -> Vec<Value> {
    vec![
        json!({"calls": [{"name": "plan.update", "args": {"outcome": "the file exists", "expected_files": [format!("{dir}/{file}")]}}]}),
        json!({"calls": [{"name": "change.apply", "args": {"path": format!("{dir}/{file}"), "op": "replace", "content": "x\n"}}]}),
        json!({"calls": [{"name": "task.complete", "args": {"summary": "done", "self_review": {"findings": []}}}]}),
    ]
}

/// StartTask with an explicit turn budget (a parent that delegates needs
/// more than the shared harness default so its children's slices are real).
async fn start_task_turns(c: &mut Client, task: &Id, g: Option<u64>, id: u8, max_turns: u32) {
    let ack = c
        .command(envelope_fenced(
            id16(id),
            "StartTask",
            modbit_protocol::v1::StartTask {
                task_id: Some(task.clone()),
                endpoint: String::new(),
                model: "gpt-5-mini".into(),
                max_turns,
                max_tool_calls: 0,
                max_no_progress_turns: 6,
                skills: vec![],
                ..Default::default()
            }
            .encode_to_vec(),
            g,
        ))
        .await
        .unwrap();
    let _: modbit_protocol::v1::TaskRunStarted = Client::result(&ack).unwrap();
}

const GOAL_SECRET: &str = "PLANTED-GOAL-SECRET-91827364";
const FILE_SECRET: &str = "PLANTED-FILE-SECRET-5566778899";
const HEADER_SECRET: &str = "planted-collector-token-abcdef0123";
const PRICE: u64 = 50_000;

fn spawn(dir: &std::path::Path, base: &str, extra: &[(&str, &str)]) -> CoreProcess {
    let mut env = model_env(base);
    env.push(("MODBIT_HEALTH_INTERVAL_MS".into(), "300".into()));
    env.push(("MODBIT_CAPACITY".into(), "model=4,provider=8".into()));
    for (k, v) in extra {
        env.push(((*k).into(), (*v).into()));
    }
    let refs: Vec<(&str, &str)> = env.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
    CoreProcess::spawn_with_env(dir, &refs)
}

fn otlp_env(collector: &Collector) -> Vec<(&str, &str)> {
    vec![
        ("MODBIT_OTLP_ENDPOINT", collector.base.as_str()),
        ("MODBIT_OTLP_INTERVAL_MS", "200"),
        ("MODBIT_OTLP_HEADERS_ENV", "x-modbit-test=OTEL_TEST_SECRET"),
        ("OTEL_TEST_SECRET", HEADER_SECRET),
    ]
}

async fn eventually<F: FnMut() -> bool>(secs: u64, what: &str, mut done: F) {
    let deadline = Instant::now() + Duration::from_secs(secs);
    while Instant::now() < deadline {
        if done() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert!(done(), "timed out waiting for {what}");
}

/// A quick task: a plan and a completion.
async fn quick_task(c: &mut Client, session: &Id, g: Option<u64>, root: &str, n: u8) -> (Id, f64) {
    let task = create_task(c, session, g, root, n, "local_trusted", "note something").await;
    let at = Instant::now();
    start_task(c, &task, g, n + 1, "gpt-5-mini").await;
    let st = wait_task(c, &task, 120).await;
    assert!(!st.loop_alive, "{st:?}");
    (task, at.elapsed().as_secs_f64())
}

// -------------------------------------------------------------------- tests

/// QUAL-PX-139: a parent with two children exports one trace; spans show the
/// parent-child links and each span's cost; the parent's cost is its own
/// plus its children's from the accounting record; the exported costs are the
/// accounting record's; a planted secret in a goal, a file body, a provider
/// key and an exporter header appears in no span, metric or request body.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[allow(clippy::too_many_lines)]
async fn qual_px_139_a_parent_with_two_children_exports_one_trace_with_the_accounting_costs() {
    let key = ed25519_dalek::SigningKey::from_bytes(&[139u8; 32]);
    let keys = format!("ops:{}", hex::encode(key.verifying_key().to_bytes()));
    let signed = accounting_registry(
        "registry-px139",
        &key,
        &[("gpt-5-mini", PRICE, PRICE, PRICE)],
    );
    let (repo, root) = plain_repo(&[
        ("notes.md", &format!("notes {FILE_SECRET}\n")),
        ("src/a/.keep", ""),
        ("src/b/.keep", ""),
    ]);
    let parent = vec![
        plan(),
        json!({"calls": [{"name": "agent.spawn", "args": {"idempotency_key": "k1", "objective": "create src/a/a.txt", "write_scope": ["src/a/"]}}]}),
        json!({"calls": [{"name": "agent.spawn", "args": {"idempotency_key": "k2", "objective": "create src/b/b.txt", "write_scope": ["src/b/"]}}]}),
        json!({"calls": [{"name": "agent.wait", "args": {"idempotency_key": "k1", "timeout_ms": 60000}}]}),
        json!({"calls": [{"name": "agent.wait", "args": {"idempotency_key": "k2", "timeout_ms": 60000}}]}),
        json!({"calls": [{"name": "fs.read", "args": {"path": "notes.md"}}]}),
        complete(),
    ];
    let (a, b) = (
        child_script("src/a", "a.txt"),
        child_script("src/b", "b.txt"),
    );
    let reply: Reply = Arc::new(move |body, results| {
        let text = body["messages"].to_string();
        let pick = |s: &Vec<Value>| {
            s.get(results)
                .cloned()
                .unwrap_or_else(|| json!({"text": "nothing further"}))
        };
        let which = if text.contains("Task goal: create src/a/a.txt") {
            "a"
        } else if text.contains("Task goal: create src/b/b.txt") {
            "b"
        } else {
            "parent"
        };
        match which {
            "a" => pick(&a),
            "b" => pick(&b),
            _ => pick(&parent),
        }
    });
    let (base, seen) = scripted_model_fn(reply).await;
    let collector = Collector::start(OK).await;
    let dir = tempfile::tempdir().unwrap();
    let mut extra = otlp_env(&collector);
    extra.push(("MODBIT_REGISTRY_KEYS", keys.as_str()));
    // The provider key is a planted secret too: it is in the Core's custody.
    extra.push(("OPENAI_API_KEY", "sk-planted-provider-key-0011223344556677"));
    let core = spawn(dir.path(), &base, &extra);
    let mut c = core.client().await;
    activate_registry(&mut c, &signed).await;
    let (session, g) = session_with_lease(&mut c, 0x31).await;
    let task = create_task(
        &mut c,
        &session,
        g,
        &root,
        0x32,
        "local_trusted",
        &format!("delegate two files and read the notes {GOAL_SECRET}"),
    )
    .await;
    start_task_turns(&mut c, &task, g, 0x33, 40).await;
    let st = wait_task(&mut c, &task, 180).await;
    assert!(!st.loop_alive, "{st:?}");
    if st.state != "ReadyForReview" {
        let evs = replay(&core, &session).await;
        let names: Vec<String> = evs
            .iter()
            .filter(|e| e["event_type"] == "ToolCallProposed")
            .map(|e| {
                format!(
                    "{}:{}",
                    &e["task_id"].as_str().unwrap_or("")[..4],
                    e["payload"]["payload"]["tool_name"]
                )
            })
            .collect();
        let kinds: Vec<String> = evs
            .iter()
            .filter(|e| {
                matches!(
                    e["event_type"].as_str().unwrap_or(""),
                    "TaskWaiting"
                        | "TaskNeedsAttention"
                        | "SubagentAdmitted"
                        | "SubagentResultRecorded"
                        | "TaskFailed"
                )
            })
            .map(|e| {
                format!(
                    "{}:{} {}",
                    &e["task_id"].as_str().unwrap_or("")[..4],
                    e["event_type"],
                    e["payload"]["payload"]
                )
            })
            .collect();
        panic!("{st:?}\n{names:?}\n{kinds:#?}");
    }

    // Wait for the three runs' spans to arrive.
    eventually(60, "three run spans at the collector", || {
        collector
            .spans()
            .iter()
            .filter(|s| s["name"] == "modbit.run")
            .count()
            >= 3
    })
    .await;
    let spans = collector.spans();
    let runs: Vec<&Value> = spans.iter().filter(|s| s["name"] == "modbit.run").collect();
    assert_eq!(runs.len(), 3, "{runs:#?}");
    // One trace.
    let trace = runs[0]["traceId"].as_str().unwrap();
    assert!(
        spans.iter().all(|s| s["traceId"] == trace),
        "a parent and its children are one trace"
    );
    // The parent's run span is the root; each child's run span hangs off a
    // subagent span, which hangs off one of the parent's turns.
    let parent_run = runs
        .iter()
        .find(|s| s.get("parentSpanId").is_none())
        .expect("the parent's run span has no parent");
    let child_runs: Vec<&&Value> = runs
        .iter()
        .filter(|s| s.get("parentSpanId").is_some())
        .collect();
    assert_eq!(child_runs.len(), 2);
    let subagents: Vec<&Value> = spans
        .iter()
        .filter(|s| s["name"] == "modbit.subagent")
        .collect();
    assert_eq!(subagents.len(), 2, "{subagents:#?}");
    let turn_ids: std::collections::HashSet<&str> = spans
        .iter()
        .filter(|s| s["name"] == "modbit.turn")
        .filter_map(|s| s["spanId"].as_str())
        .collect();
    for sub in &subagents {
        assert!(
            turn_ids.contains(sub["parentSpanId"].as_str().unwrap()),
            "a subagent span hangs off a turn: {sub:#?}"
        );
    }
    for cr in &child_runs {
        assert!(
            subagents.iter().any(|s| s["spanId"] == cr["parentSpanId"]),
            "a child's run hangs off its subagent span: {cr:#?}"
        );
    }
    // Tool-call and model-request spans carry their parents and tokens.
    let tool_names: Vec<Value> = spans
        .iter()
        .filter(|s| s["name"] == "modbit.tool_call")
        .filter_map(|s| attr(s, "modbit.tool.name"))
        .collect();
    assert!(tool_names.contains(&json!("fs.read")), "{tool_names:?}");
    assert!(
        tool_names.contains(&json!("change.apply")),
        "{tool_names:?}"
    );
    assert!(
        spans
            .iter()
            .filter(|s| s["name"] == "modbit.tool_call")
            .all(|s| turn_ids.contains(s["parentSpanId"].as_str().unwrap())),
        "a tool call hangs off its turn"
    );
    let model_spans: Vec<&Value> = spans
        .iter()
        .filter(|s| s["name"] == "modbit.model_request")
        .collect();
    assert!(!model_spans.is_empty());
    assert!(
        model_spans
            .iter()
            .all(|s| int(s, "gen_ai.usage.input_tokens") > 0)
    );

    // The costs are the accounting record's.
    let rec = outcome(&mut c, &task).await;
    let total = rec["cost"]["total_minor"].as_i64().unwrap();
    let kids_rollup = rec["cost"]["children_minor"].as_i64().unwrap();
    let subtree = rec["cost"]["subtree_minor"].as_i64().unwrap();
    assert!(total > 0, "{rec}");
    assert_eq!(subtree, total + kids_rollup);
    assert_eq!(int(parent_run, "modbit.cost.minor"), total);
    assert_eq!(int(parent_run, "modbit.cost.children_minor"), kids_rollup);
    assert_eq!(int(parent_run, "modbit.cost.subtree_minor"), subtree);
    // ... and the rollup is the children's own records.
    let children: Vec<Value> = rec["cost"]["children"].as_array().unwrap().clone();
    assert_eq!(children.len(), 2, "{rec}");
    let mut child_total = 0;
    for ch in &children {
        let id = ch["task_id"].as_str().unwrap();
        let cid = Id {
            value: modbit_domain::TaskId::parse(id)
                .unwrap()
                .as_bytes()
                .to_vec(),
        };
        let own = outcome(&mut c, &cid).await;
        let own_total = own["cost"]["total_minor"].as_i64().unwrap();
        assert!(own_total > 0);
        assert_eq!(ch["total_minor"].as_i64().unwrap(), own_total);
        child_total += own_total;
        // The child's exported run span costs what its own record says.
        let run = child_runs
            .iter()
            .find(|s| attr(s, "modbit.task.id") == Some(json!(id)))
            .expect("the child's run span");
        assert_eq!(int(run, "modbit.cost.minor"), own_total);
    }
    assert_eq!(
        kids_rollup, child_total,
        "the parent's rollup is the sum of its children's records"
    );
    // The turns of the parent add up to the parent's own cost.
    let parent_task = attr(parent_run, "modbit.task.id").unwrap();
    let parent_turns: i64 = spans
        .iter()
        .filter(|s| s["name"] == "modbit.turn")
        .filter(|s| s["parentSpanId"] == parent_run["spanId"])
        .map(|s| int(s, "modbit.cost.minor"))
        .sum();
    assert_eq!(parent_turns, total, "{parent_task}");

    // Metrics went out too, off the same records.
    eventually(30, "a metrics export", || {
        collector.requests().iter().any(|r| r.path == "/v1/metrics")
    })
    .await;
    let metrics_body = collector
        .requests()
        .iter()
        .rev()
        .find(|r| r.path == "/v1/metrics")
        .map(|r| String::from_utf8_lossy(&r.body).into_owned())
        .unwrap();
    assert!(metrics_body.contains("modbit.tokens") && metrics_body.contains("modbit.cost"));

    // No secret anywhere that left the Core: the goal, the file body the
    // model read, the provider key in custody, the exporter's own header.
    let reqs = collector.requests();
    assert!(!reqs.is_empty());
    for r in &reqs {
        let body = String::from_utf8_lossy(&r.body);
        for secret in [
            GOAL_SECRET,
            FILE_SECRET,
            HEADER_SECRET,
            "sk-planted-provider-key",
        ] {
            assert!(
                !body.contains(secret),
                "{secret} left the Core in {}",
                r.path
            );
        }
        // The header carried its value to the collector (and only there).
        assert_eq!(
            r.headers
                .iter()
                .find(|(k, _)| k == "x-modbit-test")
                .map(|(_, v)| v.as_str()),
            Some(HEADER_SECRET)
        );
    }
    // The secrets did reach the model's prompts (so the test means something).
    let prompts = seen
        .lock()
        .unwrap()
        .iter()
        .map(Value::to_string)
        .collect::<String>();
    assert!(prompts.contains(FILE_SECRET) && prompts.contains(GOAL_SECRET));
    // Every attribute key that left is an allow-listed one.
    for s in &spans {
        for a in s["attributes"].as_array().unwrap() {
            let k = a["key"].as_str().unwrap();
            assert!(
                ["modbit.", "gen_ai."].iter().any(|p| k.starts_with(p)),
                "unexpected attribute {k}"
            );
        }
    }

    // The exporter reports itself.
    let h = health(&core).await;
    let export = h.export.unwrap();
    assert!(export.enabled);
    assert_eq!(export.endpoint, collector.base, "origin only");
    assert!(export.spans_sent >= spans.len() as u64 - 1, "{export:?}");
    assert_eq!(export.spans_lost + export.spans_dropped_overflow, 0);
    let telemetry = h.components.iter().find(|c| c.name == "telemetry").unwrap();
    assert_eq!(telemetry.state, "OK", "{telemetry:?}");
    drop(repo);
}

/// With export off — not configured, a device policy that says `off`, or an
/// endpoint the network allow-list does not admit — nothing is sent.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn qual_px_139_with_export_off_nothing_is_sent() {
    let (repo, root) = plain_repo(&[("notes.md", "notes\n")]);
    let (base, _seen) = scripted_model(vec![plan(), complete()], vec![]).await;
    let collector = Collector::start(OK).await;

    // (a) Not configured: the default.
    {
        let dir = tempfile::tempdir().unwrap();
        let core = spawn(dir.path(), &base, &[]);
        let mut c = core.client().await;
        let (session, g) = session_with_lease(&mut c, 0x41).await;
        let _ = quick_task(&mut c, &session, g, &root, 0x42).await;
        let h = health(&core).await;
        let e = h.export.unwrap();
        assert!(
            !e.enabled && e.requests_ok == 0 && e.requests_failed == 0,
            "{e:?}"
        );
        assert!(e.disabled_reason.contains("not configured"), "{e:?}");
    }
    // (b) Configured, but the device says telemetry is off.
    {
        let dir = tempfile::tempdir().unwrap();
        let device = dir.path().join("device-policy.json");
        std::fs::write(&device, r#"{"device": {"telemetry": "off"}}"#).unwrap();
        let mut extra = otlp_env(&collector);
        extra.push(("MODBIT_DEVICE_POLICY", device.to_str().unwrap()));
        let core = spawn(dir.path(), &base, &extra);
        let mut c = core.client().await;
        let (session, g) = session_with_lease(&mut c, 0x43).await;
        let _ = quick_task(&mut c, &session, g, &root, 0x44).await;
        let e = health(&core).await.export.unwrap();
        assert!(!e.enabled, "{e:?}");
        assert!(e.disabled_reason.contains("telemetry"), "{e:?}");
    }
    // (c) Configured, but the endpoint's host is not on the allow-list.
    {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("admin-config.json"),
            r#"{"network_allow": ["otel.corp.example"]}"#,
        )
        .unwrap();
        let core = spawn(dir.path(), &base, &otlp_env(&collector));
        let mut c = core.client().await;
        let (session, g) = session_with_lease(&mut c, 0x45).await;
        let _ = quick_task(&mut c, &session, g, &root, 0x46).await;
        let e = health(&core).await.export.unwrap();
        assert!(!e.enabled, "{e:?}");
        assert!(e.disabled_reason.contains("allow-list"), "{e:?}");
    }
    tokio::time::sleep(Duration::from_millis(1500)).await;
    assert!(
        collector.requests().is_empty(),
        "nothing was sent: {:?}",
        collector.requests().len()
    );
    drop(repo);
}

/// A collector that hangs, refuses or disappears mid-run never delays or
/// fails the task, and the loss is counted, not hidden.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn qual_px_139_a_hung_refusing_or_vanished_collector_costs_the_run_nothing_and_the_loss_is_counted()
 {
    let (repo, root) = plain_repo(&[("notes.md", "notes\n")]);
    // Baseline: the same task with no export at all.
    let (base, _seen) = scripted_model(vec![plan(), complete()], vec![]).await;
    let baseline = {
        let dir = tempfile::tempdir().unwrap();
        let core = spawn(dir.path(), &base, &[]);
        let mut c = core.client().await;
        let (session, g) = session_with_lease(&mut c, 0x51).await;
        let (_, secs) = quick_task(&mut c, &session, g, &root, 0x52).await;
        secs
    };

    // A collector that accepts and never answers.
    let hung = Collector::start(HANG).await;
    let dir = tempfile::tempdir().unwrap();
    let core = spawn(dir.path(), &base, &otlp_env(&hung));
    let mut c = core.client().await;
    let (session, g) = session_with_lease(&mut c, 0x53).await;
    let (t1, hung_secs) = quick_task(&mut c, &session, g, &root, 0x54).await;
    let st = wait_task(&mut c, &t1, 30).await;
    assert_eq!(st.state, "ReadyForReview", "{st:?}");
    assert!(
        hung_secs <= baseline + 4.0,
        "a hung collector delayed the run: {hung_secs:.2}s against {baseline:.2}s"
    );
    // ... and it is counted.
    let deadline = Instant::now() + Duration::from_secs(60);
    let hung_stats = loop {
        let e = health(&core).await.export.unwrap();
        if e.requests_failed >= 3 && e.spans_lost > 0 {
            break e;
        }
        assert!(
            Instant::now() < deadline,
            "the loss was never counted: {e:?}"
        );
        tokio::time::sleep(Duration::from_millis(250)).await;
    };
    assert_eq!(hung_stats.spans_sent, 0, "{hung_stats:?}");
    assert!(!hung_stats.last_error.is_empty());
    assert_eq!(
        hung_stats.spans_queued,
        hung_stats.spans_sent
            + hung_stats.spans_lost
            + hung_stats.spans_dropped_overflow
            + hung_stats.queue_depth,
        "every span is sent, lost, dropped or still queued: {hung_stats:?}"
    );
    let comp = health(&core).await;
    let telemetry = comp
        .components
        .iter()
        .find(|c| c.name == "telemetry")
        .unwrap();
    assert_ne!(telemetry.state, "OK", "{telemetry:?}");
    drop(core);

    // A collector that is up, then killed in the middle of the next run.
    let live = Collector::start(OK).await;
    let slow: Reply = Arc::new(move |_body, results| match results {
        0 => plan(),
        // The model is slow here, which is when the collector dies.
        _ => {
            json!({"delay_ms": 2500, "calls": [{"name": "task.complete", "args": {"summary": "done", "self_review": {"findings": []}}}]})
        }
    });
    let (slow_base, _s) = scripted_model_fn(slow).await;
    let dir2 = tempfile::tempdir().unwrap();
    let core2 = spawn(dir2.path(), &slow_base, &otlp_env(&live));
    let mut c2 = core2.client().await;
    let (session2, g2) = session_with_lease(&mut c2, 0x55).await;
    let (_t_ok, _) = quick_task(&mut c2, &session2, g2, &root, 0x56).await;
    eventually(30, "the first run's spans at the live collector", || {
        live.spans().iter().any(|s| s["name"] == "modbit.run")
    })
    .await;
    let sent_before = health(&core2).await.export.unwrap().spans_sent;
    assert!(sent_before > 0);
    // A second run; the collector disappears while it is mid-flight.
    let t2 = create_task(
        &mut c2,
        &session2,
        g2,
        &root,
        0x58,
        "local_trusted",
        "again",
    )
    .await;
    let started = Instant::now();
    start_task(&mut c2, &t2, g2, 0x59, "gpt-5-mini").await;
    tokio::time::sleep(Duration::from_millis(800)).await;
    live.stop();
    let st = wait_task(&mut c2, &t2, 60).await;
    assert!(!st.loop_alive, "{st:?}");
    assert_eq!(st.state, "ReadyForReview", "{st:?}");
    assert!(
        started.elapsed().as_secs_f64() < 2.5 + baseline + 6.0,
        "the run was held up: {:?}",
        started.elapsed()
    );
    let deadline = Instant::now() + Duration::from_secs(60);
    let after = loop {
        let e = health(&core2).await.export.unwrap();
        if e.spans_lost > 0 {
            break e;
        }
        assert!(
            Instant::now() < deadline,
            "the loss was never counted: {e:?}"
        );
        tokio::time::sleep(Duration::from_millis(250)).await;
    };
    assert_eq!(
        after.spans_sent, sent_before,
        "nothing more was delivered: {after:?}"
    );
    assert!(after.requests_failed > 0);
    assert_eq!(
        after.spans_queued,
        after.spans_sent + after.spans_lost + after.spans_dropped_overflow + after.queue_depth,
        "{after:?}"
    );
    hung.set_mode(OK);
    drop(repo);
}

/// After a restart the health report is the previous run's last known state
/// with its age, marked as not yet observed — never "unknown".
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn qual_px_139_after_a_restart_health_reports_the_previous_state_with_its_age() {
    let (repo, root) = plain_repo(&[("notes.md", "notes\n")]);
    let (base, _seen) = scripted_model(vec![plan(), complete()], vec![]).await;
    let dir = tempfile::tempdir().unwrap();
    let mut core = spawn(dir.path(), &base, &[]);
    let mut c = core.client().await;
    let (session, g) = session_with_lease(&mut c, 0x61).await;
    let _ = quick_task(&mut c, &session, g, &root, 0x62).await;
    // The provider has been used; its health is observed OK.
    let deadline = Instant::now() + Duration::from_secs(30);
    let before = loop {
        let h = health(&core).await;
        if let Some(p) = h
            .components
            .iter()
            .find(|c| c.name == "provider:openai" && c.state == "OK")
        {
            break p.clone();
        }
        assert!(Instant::now() < deadline, "never observed OK: {h:#?}");
        tokio::time::sleep(Duration::from_millis(200)).await;
    };
    assert!(before.observed_this_run);
    drop(c);
    core.kill();
    tokio::time::sleep(Duration::from_millis(1200)).await;
    // A new Core on the same profile with no provider configured: it cannot
    // observe `provider:openai`, so what it reports is the previous run's.
    let core2 = CoreProcess::spawn_with_env(
        dir.path(),
        &[
            ("OPENAI_API_KEY", ""),
            ("ANTHROPIC_API_KEY", ""),
            ("MODBIT_HEALTH_INTERVAL_MS", "300"),
        ],
    );
    let h = health(&core2).await;
    let prev = h
        .components
        .iter()
        .find(|c| c.name == "provider:openai")
        .expect("the previous run's provider health is reported, not forgotten");
    assert_eq!(prev.state, "OK", "{prev:?}");
    assert!(!prev.observed_this_run, "{prev:?}");
    assert!(prev.age_ms >= 1000, "its age is reported: {prev:?}");
    // The file is rewritten when a component changes and at least every 30 s,
    // so the persisted observation can be older than the last one this run
    // made, never newer.
    assert!(
        prev.observed_at_ms <= before.observed_at_ms,
        "{prev:?} {before:?}"
    );
    assert!(before.observed_at_ms - prev.observed_at_ms <= 31_000);
    // A component this run does observe is fresh.
    let telemetry = h.components.iter().find(|c| c.name == "telemetry").unwrap();
    assert!(
        telemetry.observed_this_run && telemetry.state == "DISABLED",
        "{telemetry:?}"
    );
    assert!(dir.path().join("component-health.json").exists());
    drop(repo);
}
