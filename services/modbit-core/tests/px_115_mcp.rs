//! PX-115 (QUAL-PX-115) on the real Core and the real MCP test server over
//! stdio: lazy discovery (`external.list` names, `external.describe`
//! schemas), `notifications/tools/list_changed`, the idle reaper with its
//! two-phase shutdown, a large-catalog benchmark, and a server killed
//! mid-call.
//!
//! Real: the `modbit-core` binary and its socket, the Capability Kernel and
//! tool pipeline, a separate `modbit-mcp-testserver` process per server.
//! Stand-in: the model in the benchmark task (a scripted server).

mod px_common;

use modbit_protocol::client::Client;
use modbit_protocol::v1::Id;
use px_common::mcp_support::*;
use px_common::*;
use serde_json::{Value, json};

struct Fx {
    core: CoreProcess,
    c: Client,
    session: Id,
    g: Option<u64>,
    task: Id,
    _repo: tempfile::TempDir,
    _dir: tempfile::TempDir,
}

async fn fixture(servers: Value, env: &[(&str, &str)], profile: &str) -> Fx {
    let (repo, root) = plain_repo(&[("README.md", "# demo\n")]);
    let dir = tempfile::tempdir().unwrap();
    let servers_json = servers.to_string();
    let mut all: Vec<(&str, &str)> = vec![("MODBIT_MCP_SERVERS", servers_json.as_str())];
    all.extend_from_slice(env);
    let core = CoreProcess::spawn_with_env(dir.path(), &all);
    let mut c = core.client().await;
    let (session, g) = session_with_lease(&mut c, 0x20).await;
    let task = create_task(&mut c, &session, g, &root, 0x21, profile, "external tools").await;
    Fx {
        core,
        c,
        session,
        g,
        task,
        _repo: repo,
        _dir: dir,
    }
}

impl Fx {
    async fn call(&mut self, n: u8, tool: &str, args: Value) -> modbit_protocol::v1::ToolInvoked {
        invoke_tool(
            &mut self.c,
            &self.task,
            self.g,
            0x40 + n,
            0x40 + n,
            tool,
            &args.to_string(),
        )
        .await
    }

    async fn list(&mut self, n: u8, args: Value) -> Value {
        let r = self.call(n, "external.list", args).await;
        assert_eq!(r.status, "SUCCESS", "{r:?}");
        serde_json::from_str(&r.structured_output_json).unwrap()
    }
}

fn server_named<'a>(listing: &'a Value, name: &str) -> &'a Value {
    listing["servers"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["server"] == name)
        .unwrap_or_else(|| panic!("no server {name} in {listing}"))
}

fn lifecycle(server: &Value) -> Vec<String> {
    server["lifecycle"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["event"].as_str().unwrap().to_owned())
        .collect()
}

const BIG: &[(&str, &str)] = &[("MODBIT_MCP_MAX_TOOLS", "1000")];

fn big_server(logs: &std::path::Path) -> Value {
    server_entry(
        "big",
        json!({
            "MODBIT_MCP_TESTSRV_BULK": "500",
            "MODBIT_MCP_TESTSRV_LOG": logs.join("big.jsonl").to_string_lossy(),
        }),
        &["bulk17", "bulk42", "search"],
        "TRUSTED",
    )
}

/// Discovery of a five-hundred-tool catalog: names are paged, nothing carries
/// a schema, the cold start is bounded, and every tool is reachable by
/// paging.
#[tokio::test(flavor = "multi_thread")]
async fn qual_px_115_a_five_hundred_tool_catalog_lists_by_name_in_pages_with_no_schemas_and_bounded_latency()
 {
    let logs = tempfile::tempdir().unwrap();
    let mut fx = fixture(json!([big_server(logs.path())]), BIG, "").await;
    let started = std::time::Instant::now();
    let first = fx.list(1, json!({"limit": 200})).await;
    let cold_ms = started.elapsed().as_millis();
    let big = server_named(&first, "big");
    assert_eq!(big["health"]["state"], "READY", "{big}");
    assert_eq!(
        big["tools_total"], 507,
        "500 bulk tools and the 7 the server always has"
    );
    assert_eq!(first["page"]["returned"], 200);
    let text = first.to_string();
    assert!(
        !text.contains("input_schema") && !text.contains("\"properties\""),
        "no schema in a listing"
    );
    assert!(text.contains("\"summary\""));
    // Page through everything.
    let mut names: Vec<String> = Vec::new();
    let mut cursor: Option<String> = None;
    let mut n = 2;
    loop {
        let args = match &cursor {
            Some(c) => json!({"limit": 200, "cursor": c}),
            None => json!({"limit": 200}),
        };
        let page = fx.list(n, args).await;
        n += 1;
        for t in server_named(&page, "big")["tools"].as_array().unwrap() {
            names.push(t["name"].as_str().unwrap().to_owned());
        }
        cursor = page["page"]["next_cursor"].as_str().map(str::to_owned);
        if cursor.is_none() {
            break;
        }
    }
    assert_eq!(names.len(), 507, "{}", names.len());
    let mut unique = names.clone();
    unique.sort();
    unique.dedup();
    assert_eq!(unique.len(), 507, "no tool twice, none missing");
    assert!(names.iter().any(|n| n == "external.big.bulk499"));
    // A filter narrows by name or description.
    let q = fx.list(9, json!({"query": "bulk49", "limit": 200})).await;
    assert_eq!(q["page"]["total"], 11, "bulk49 and bulk490..bulk499");
    // The server was asked for its catalog in pages (a real paged discovery)
    // and only once for all of the above.
    let log = server_log(&logs.path().join("big.jsonl"));
    let lists = log.iter().filter(|m| m["method"] == "tools/list").count();
    assert_eq!(lists, 6, "5 pages of 100 for 507 tools, once: {lists}");
    eprintln!("PX115_COLD_DISCOVERY_MS {cold_ms}");
    assert!(cold_ms < 30_000, "discovery latency bounded: {cold_ms} ms");
    let _ = (&fx.core, &fx.session);
}

/// `external.describe` returns exactly the named schemas, within a byte
/// budget, and says what it left out.
#[tokio::test(flavor = "multi_thread")]
async fn qual_px_115_describe_returns_exactly_the_named_schemas_and_never_authorises_a_call() {
    let logs = tempfile::tempdir().unwrap();
    let effects = logs.path().join("effects.txt");
    let servers = json!([
        server_entry(
            "big",
            json!({
                "MODBIT_MCP_TESTSRV_BULK": "500",
                "MODBIT_MCP_TESTSRV_EFFECTS": effects.to_string_lossy(),
            }),
            &["bulk17", "search"],
            "TRUSTED",
        ),
        server_entry("waiting", json!({}), &["search"], "PROPOSED"),
    ]);
    let mut fx = fixture(servers, BIG, "").await;
    let r = fx
        .call(
            1,
            "external.describe",
            json!({"server": "big", "tools": ["bulk1", "bulk2", "external.big.bulk3", "nope"]}),
        )
        .await;
    assert_eq!(r.status, "SUCCESS", "{r:?}");
    let d: Value = serde_json::from_str(&r.structured_output_json).unwrap();
    assert_eq!(d["trust"], "UNTRUSTED_EXTERNAL_CONTENT");
    let tools = d["tools"].as_array().unwrap();
    assert_eq!(tools.len(), 3, "exactly the named, known tools: {d}");
    for (t, want) in tools.iter().zip(["bulk1", "bulk2", "bulk3"]) {
        assert_eq!(t["tool"], want);
        assert!(t["input_schema"]["properties"]["target"].is_object());
    }
    assert_eq!(d["unknown"], json!(["nope"]));
    assert!(d["schema_bytes"].as_u64().unwrap() > 300);
    assert!(d["catalog_generation"].as_u64().unwrap() >= 1);
    // Describing is not calling: the side-effecting `bulk5` (not a declared
    // read) is describable, and the call is stopped by the Kernel until
    // someone approves it. Nothing ran.
    let r = fx
        .call(
            2,
            "external.describe",
            json!({"server": "big", "tools": ["bulk5"]}),
        )
        .await;
    assert_eq!(r.status, "SUCCESS");
    let r = fx
        .call(
            3,
            "external.call",
            json!({"server": "big", "tool": "bulk5", "arguments": {"target": "x"}}),
        )
        .await;
    assert_ne!(r.status, "SUCCESS", "discovery authorised nothing: {r:?}");
    assert!(!effects.exists());
    // A declared read does run: the host's declaration, not the describe.
    let r = fx
        .call(
            4,
            "external.call",
            json!({"server": "big", "tool": "bulk17", "arguments": {"target": "x"}}),
        )
        .await;
    assert_eq!(r.status, "SUCCESS", "{r:?}");
    // A proposed server is inert: not started, so nothing to describe.
    let r = fx
        .call(
            5,
            "external.describe",
            json!({"server": "waiting", "tools": ["search"]}),
        )
        .await;
    assert_eq!(r.error_code, "EXTERNAL_SERVER_UNTRUSTED", "{r:?}");
    let r = fx
        .call(
            6,
            "external.describe",
            json!({"server": "nowhere", "tools": ["search"]}),
        )
        .await;
    assert_eq!(r.error_code, "EXTERNAL_SERVER_UNKNOWN", "{r:?}");
}

/// Under the reviewer's profile a tool is describable and the call is
/// refused by the Kernel: discovery is not authority.
#[tokio::test(flavor = "multi_thread")]
async fn qual_px_115_a_tool_describable_under_the_reviewer_profile_is_refused_by_the_kernel_when_called()
 {
    let logs = tempfile::tempdir().unwrap();
    let effects = logs.path().join("effects.txt");
    let servers = json!([server_entry(
        "docs",
        json!({"MODBIT_MCP_TESTSRV_EFFECTS": effects.to_string_lossy()}),
        &["search"],
        "TRUSTED",
    )]);
    let mut fx = fixture(servers, &[], "review_isolated").await;
    let r = fx
        .call(
            1,
            "external.describe",
            json!({"server": "docs", "tools": ["search", "note"]}),
        )
        .await;
    assert_eq!(r.status, "SUCCESS", "{r:?}");
    let d: Value = serde_json::from_str(&r.structured_output_json).unwrap();
    assert_eq!(d["tools"].as_array().unwrap().len(), 2);
    let r = fx
        .call(
            2,
            "external.call",
            json!({"server": "docs", "tool": "search", "arguments": {"q": "x"}}),
        )
        .await;
    assert_ne!(r.status, "SUCCESS", "the Kernel refused the call: {r:?}");
    let r = fx
        .call(
            3,
            "external.call",
            json!({"server": "docs", "tool": "note", "arguments": {"text": "x"}}),
        )
        .await;
    assert_ne!(r.status, "SUCCESS");
    assert!(!effects.exists(), "nothing reached the server");
}

/// `notifications/tools/list_changed` invalidates the cached catalog: a tool
/// the server added becomes discoverable at the next read, with no restart,
/// and an unchanged catalog is not re-read.
#[tokio::test(flavor = "multi_thread")]
async fn qual_px_115_list_changed_refreshes_the_catalog_without_a_restart() {
    let logs = tempfile::tempdir().unwrap();
    let log = logs.path().join("grow.jsonl");
    let pid = logs.path().join("grow.pid");
    let servers = json!([server_entry(
        "grow",
        json!({
            "MODBIT_MCP_TESTSRV_LIST_CHANGED": "1",
            "MODBIT_MCP_TESTSRV_LOG": log.to_string_lossy(),
            "MODBIT_MCP_TESTSRV_PIDFILE": pid.to_string_lossy(),
        }),
        &["grow", "late_tool", "search"],
        "TRUSTED",
    )]);
    let mut fx = fixture(servers, &[], "").await;
    let before = fx.list(1, json!({"limit": 200})).await;
    let g1 = server_named(&before, "grow")["catalog_generation"]
        .as_u64()
        .unwrap();
    assert!(
        !before.to_string().contains("late_tool"),
        "not declared yet"
    );
    let lists = |l: &[Value]| l.iter().filter(|m| m["method"] == "tools/list").count();
    assert_eq!(lists(&server_log(&log)), 1);
    // The same catalog again is served from the cache: no new tools/list.
    let _ = fx.list(2, json!({"limit": 200})).await;
    assert_eq!(
        lists(&server_log(&log)),
        1,
        "an unchanged catalog is not re-read"
    );
    let first_pid = pid_of(&pid).unwrap();
    // The server grows its catalog and says so.
    let r = fx
        .call(
            3,
            "external.call",
            json!({"server": "grow", "tool": "grow", "arguments": {}}),
        )
        .await;
    assert_eq!(r.status, "SUCCESS", "{r:?}");
    assert!(
        eventually(10, || server_log(&log)
            .iter()
            .any(|m| m["method"] == "tools/call"))
        .await
    );
    // The next read sees it.
    let after = fx.list(4, json!({"query": "late", "limit": 50})).await;
    let grow = server_named(&after, "grow");
    let names: Vec<&str> = grow["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["tool"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["late_tool"], "{after}");
    assert!(grow["catalog_generation"].as_u64().unwrap() > g1);
    assert!(
        lifecycle(grow).contains(&"CATALOG_CHANGED".to_owned()),
        "{grow}"
    );
    assert_eq!(pid_of(&pid), Some(first_pid), "no restart");
    // And it is describable and callable (the host declared it a read).
    let r = fx
        .call(
            5,
            "external.describe",
            json!({"server": "grow", "tools": ["late_tool"]}),
        )
        .await;
    let d: Value = serde_json::from_str(&r.structured_output_json).unwrap();
    assert_eq!(d["tools"][0]["tool"], "late_tool", "{d}");
    let r = fx
        .call(
            6,
            "external.call",
            json!({"server": "grow", "tool": "late_tool", "arguments": {}}),
        )
        .await;
    assert_eq!(r.status, "SUCCESS", "{r:?}");
    assert_eq!(
        lists(&server_log(&log)),
        2,
        "exactly one re-read after the notification"
    );
}

/// An idle server is stopped when its input closes (graceful), and the next
/// use starts a fresh process: lazily, with the history on the listing.
#[cfg(unix)]
#[tokio::test(flavor = "multi_thread")]
async fn qual_px_115_an_idle_server_is_reaped_gracefully_and_the_next_call_reconnects() {
    let logs = tempfile::tempdir().unwrap();
    let pid = logs.path().join("idle.pid");
    let servers = json!([server_entry(
        "idle",
        json!({"MODBIT_MCP_TESTSRV_PIDFILE": pid.to_string_lossy()}),
        &["search"],
        "TRUSTED",
    )]);
    let mut fx = fixture(
        servers,
        &[
            ("MODBIT_MCP_IDLE_TTL_MS", "600"),
            ("MODBIT_MCP_SHUTDOWN_GRACE_MS", "1500"),
        ],
        "",
    )
    .await;
    let r = fx
        .call(
            1,
            "external.call",
            json!({"server": "idle", "tool": "search", "arguments": {"q": "a"}}),
        )
        .await;
    assert_eq!(r.status, "SUCCESS", "{r:?}");
    let first = pid_of(&pid).unwrap();
    assert!(process_exists(first));
    // Unused past the bound, the process goes away: input closed, it exits,
    // and it is reaped (a zombie would still answer `kill -0`).
    assert!(
        eventually(15, || !process_exists(first)).await,
        "the idle server {first} was not reaped"
    );
    // The next call reconnects.
    let r = fx
        .call(
            2,
            "external.call",
            json!({"server": "idle", "tool": "search", "arguments": {"q": "b"}}),
        )
        .await;
    assert_eq!(r.status, "SUCCESS", "{r:?}");
    let second = pid_of(&pid).unwrap();
    assert_ne!(first, second, "a fresh process");
    let listing = fx.list(3, json!({"limit": 5})).await;
    let events = lifecycle(server_named(&listing, "idle"));
    assert_eq!(
        &events[..3],
        ["STARTED", "REAPED_GRACEFUL", "STARTED"],
        "{events:?}"
    );
}

/// A server that ignores its closed input is killed after the grace period:
/// nothing outlives the reaper.
#[cfg(unix)]
#[tokio::test(flavor = "multi_thread")]
async fn qual_px_115_a_server_that_ignores_the_shutdown_is_killed_after_the_grace_period() {
    let logs = tempfile::tempdir().unwrap();
    let pid = logs.path().join("stubborn.pid");
    let servers = json!([server_entry(
        "stubborn",
        json!({
            "MODBIT_MCP_TESTSRV_PIDFILE": pid.to_string_lossy(),
            "MODBIT_MCP_TESTSRV_MODE": "stubborn",
        }),
        &["search"],
        "TRUSTED",
    )]);
    let mut fx = fixture(
        servers,
        &[
            ("MODBIT_MCP_IDLE_TTL_MS", "500"),
            ("MODBIT_MCP_SHUTDOWN_GRACE_MS", "700"),
        ],
        "",
    )
    .await;
    let r = fx
        .call(
            1,
            "external.call",
            json!({"server": "stubborn", "tool": "search", "arguments": {"q": "a"}}),
        )
        .await;
    assert_eq!(r.status, "SUCCESS", "{r:?}");
    let first = pid_of(&pid).unwrap();
    assert!(
        eventually(20, || !process_exists(first)).await,
        "the stubborn server {first} outlived the grace period"
    );
    let r = fx
        .call(
            2,
            "external.call",
            json!({"server": "stubborn", "tool": "search", "arguments": {"q": "b"}}),
        )
        .await;
    assert_eq!(r.status, "SUCCESS", "{r:?}");
    let listing = fx.list(3, json!({"limit": 5})).await;
    let events = lifecycle(server_named(&listing, "stubborn"));
    assert_eq!(
        &events[..3],
        ["STARTED", "REAPED_KILLED", "STARTED"],
        "{events:?}"
    );
}

/// A server killed while a call is in flight: the call fails with a typed
/// code (never a hang, never a claim it succeeded), the dead process is
/// reaped, and the next call starts a fresh one.
#[cfg(unix)]
#[tokio::test(flavor = "multi_thread")]
async fn qual_px_115_a_server_killed_mid_call_fails_typed_is_reaped_and_the_next_call_reconnects() {
    let logs = tempfile::tempdir().unwrap();
    let pid = logs.path().join("victim.pid");
    let log = logs.path().join("victim.jsonl");
    let servers = json!([server_entry(
        "victim",
        json!({
            "MODBIT_MCP_TESTSRV_PIDFILE": pid.to_string_lossy(),
            "MODBIT_MCP_TESTSRV_LOG": log.to_string_lossy(),
        }),
        &["slow", "search"],
        "TRUSTED",
    )]);
    let mut fx = fixture(servers, &[], "").await;
    let r = fx
        .call(
            1,
            "external.call",
            json!({"server": "victim", "tool": "search", "arguments": {"q": "a"}}),
        )
        .await;
    assert_eq!(r.status, "SUCCESS", "{r:?}");
    let first = pid_of(&pid).unwrap();
    // A second connection issues a 30 s call; the server dies under it.
    let mut other = fx.core.client().await;
    let (task, g) = (fx.task.clone(), fx.g);
    let slow = tokio::spawn(async move {
        invoke_tool(
            &mut other,
            &task,
            g,
            0x70,
            0x70,
            "external.call",
            &json!({"server": "victim", "tool": "slow", "arguments": {"ms": 30000}}).to_string(),
        )
        .await
    });
    assert!(
        eventually(10, || server_log(&log).iter().any(|m| m["method"]
            == "tools/call"
            && m["params"]["name"] == "slow"))
        .await,
        "the slow call reached the server"
    );
    kill_9(first);
    let r = tokio::time::timeout(std::time::Duration::from_secs(20), slow)
        .await
        .expect("the call did not hang")
        .unwrap();
    assert_ne!(r.status, "SUCCESS", "{r:?}");
    assert!(
        ["EXTERNAL_TRANSPORT_LOST", "EXTERNAL_OUTCOME_UNKNOWN"].contains(&r.error_code.as_str()),
        "typed failure, got {r:?}"
    );
    assert!(
        eventually(10, || !process_exists(first)).await,
        "the dead server {first} was not reaped (a zombie remains)"
    );
    let r = fx
        .call(
            2,
            "external.call",
            json!({"server": "victim", "tool": "search", "arguments": {"q": "again"}}),
        )
        .await;
    assert_eq!(r.status, "SUCCESS", "{r:?}");
    assert_ne!(pid_of(&pid).unwrap(), first, "a fresh process");
    let listing = fx.list(3, json!({"limit": 5})).await;
    let events = lifecycle(server_named(&listing, "victim"));
    assert!(events.contains(&"LOST".to_owned()), "{events:?}");
}

/// The large-catalog benchmark: the same task, a five-hundred-tool server,
/// lazy discovery against the eager discovery it replaces. The model's
/// request never carries an external schema and stays inside the schema
/// budget; what the model reads to find and use one tool is a small
/// fraction of the whole catalog. The model is scripted: bytes and tool
/// calls are the Core's, success is the task completing.
#[tokio::test(flavor = "multi_thread")]
async fn qual_px_115_benchmark_lazy_discovery_against_eager_on_a_five_hundred_tool_server() {
    let logs = tempfile::tempdir().unwrap();
    let script = vec![
        json!({"calls": [{"name": "tool.search", "args": {"query": "external"}}]}),
        json!({"calls": [{"name": "plan.update", "args": {"outcome": "use bulk17", "expected_files": ["notes.md"]}}]}),
        json!({"calls": [{"name": "external.list", "args": {"server": "big", "query": "bulk17", "limit": 5}}]}),
        json!({"calls": [{"name": "external.describe", "args": {"server": "big", "tools": ["bulk17"]}}]}),
        json!({"calls": [{"name": "external.call", "args": {"server": "big", "tool": "bulk17", "arguments": {"target": "t"}}}]}),
        json!({"calls": [{"name": "task.complete", "args": {"summary": "used bulk17", "self_review": {"findings": []}}}]}),
    ];
    let (base, seen) = scripted_model(script, vec![]).await;
    let mut env: Vec<(String, String)> = model_env(&base);
    env.push(("MODBIT_MCP_MAX_TOOLS".into(), "1000".into()));
    let env_refs: Vec<(&str, &str)> = env.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
    let mut fx = fixture(json!([big_server(logs.path())]), &env_refs, "").await;
    start_task(&mut fx.c, &fx.task, fx.g, 0x30, "gpt-5-mini").await;
    let st = wait_task(&mut fx.c, &fx.task, 120).await;
    assert_eq!(
        st.state, "ReadyForReview",
        "{} {}",
        st.state, st.attention_reason
    );
    let bodies = seen.lock().unwrap().clone();
    assert_eq!(bodies.len(), 6);
    // Never an external schema in a request, always inside the budget.
    for b in &bodies {
        let bytes = request_schema_bytes(b);
        assert!(
            bytes <= 32 * 1024,
            "{bytes} bytes of schemas in one request"
        );
        for n in request_tool_names(b) {
            assert!(
                !n.contains("bulk"),
                "an external tool schema was projected: {n}"
            );
        }
    }
    assert!(
        !request_tool_names(&bodies[0])
            .iter()
            .any(|n| n.starts_with("external.")),
        "the external family is deferred until found"
    );
    // What the model read: the list page and one described schema.
    let results = |i: usize| -> Vec<String> {
        bodies[i]["messages"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|m| m["role"] == "tool")
            .map(|m| m["content"].as_str().unwrap_or_default().to_owned())
            .collect()
    };
    let last = results(5);
    let (listed, described, called) = (&last[2], &last[3], &last[4]);
    assert!(
        listed.contains("bulk17") && !listed.contains("\"properties\""),
        "{listed}"
    );
    assert!(
        described.contains("\"target\"") && described.contains("schema_bytes"),
        "{described}"
    );
    assert!(called.contains("bulk17:"), "{called}");
    let lazy_bytes = listed.len() + described.len();
    // The eager alternative: every schema of the catalog in the model's context.
    let mut eager_bytes = 0usize;
    let all: Vec<String> = (0..500).map(|i| format!("bulk{i}")).collect();
    for (n, chunk) in all.chunks(8).enumerate() {
        let r = fx
            .call(
                100 + (n % 100) as u8,
                "external.describe",
                json!({"server": "big", "tools": chunk}),
            )
            .await;
        assert_eq!(r.status, "SUCCESS");
        let d: Value = serde_json::from_str(&r.structured_output_json).unwrap();
        eager_bytes += d["schema_bytes"].as_u64().unwrap() as usize;
    }
    let input_tokens: u64 = bodies
        .iter()
        .map(|b| (serde_json::to_string(&b["messages"]).unwrap().len() / 4) as u64)
        .sum();
    eprintln!(
        "PX115_BENCH {}",
        json!({
            "provider": "scripted (not a live provider)",
            "catalog_tools": 507,
            "lazy": {"model_calls": 6, "max_request_schema_bytes": bodies.iter().map(request_schema_bytes).max(), "bytes_read_to_find_and_describe_one_tool": lazy_bytes, "prompt_tokens_over_the_run": input_tokens, "task_success": true},
            "eager": {"schema_and_description_bytes_for_the_whole_catalog": eager_bytes, "would_exceed_request_budget": eager_bytes > 32 * 1024},
        })
    );
    assert!(
        lazy_bytes * 20 < eager_bytes,
        "lazy {lazy_bytes} vs eager {eager_bytes}"
    );
    assert!(
        eager_bytes > 32 * 1024,
        "eager discovery would not fit the schema budget"
    );
    let _ = (&fx.core, &fx.session);
}
