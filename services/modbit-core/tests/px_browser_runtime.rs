//! PX-121, PX-122, PX-123 (QUAL-PX-121..123) on the real Core: the real
//! `modbit-core` binary over its real socket and store, a desktop-kind
//! client standing in for the browser host (the page is the stand-in; the
//! protocol, the tools, the kernel and the log are real), and a scripted
//! model. Covered: an input of unknown outcome latches the session and only
//! a fresh observation reconciles it; every refusal carries the
//! machine-readable escalation; the host's change notice makes the Core read
//! and journal the page with no model call; the known-state map and a
//! fingerprint the model holds survive a killed Core.
mod px_common;

use modbit_browser::compiler::{reference_of, reference_of_ident};
use modbit_protocol::client::Client;
use modbit_protocol::v1::{
    AttachBrowserHost, BrowserHostAttached, BrowserHostNotice, BrowserHostNoticed,
    BrowserHostResponse, BrowserRuntimeView, BrowserSessionOpened, ClientKind, GetBrowserRuntime,
    Id, OpenBrowserSession,
};
use prost::Message;
use px_common::*;
use serde_json::{Value, json};
use std::sync::{Arc, Mutex};

fn page(version: u64, extra: bool) -> Value {
    let mut nodes = vec![
        json!({"id": "1", "role": "RootWebArea", "name": "Fixture", "depth": 0}),
        json!({"id": "2", "parent": "1", "role": "main", "name": "", "depth": 1}),
        json!({"id": "3", "parent": "2", "role": "button", "name": "Press", "depth": 2, "backend_dom_node_id": 30}),
        json!({"id": "4", "parent": "2", "role": "textbox", "name": "Note", "depth": 2, "backend_dom_node_id": 40, "dom_ident": "id:note"}),
    ];
    if extra {
        nodes.push(json!({"id": "5", "parent": "2", "role": "button", "name": "Later", "depth": 2, "backend_dom_node_id": 50}));
    }
    json!({"kind": "snapshot", "state": {"url": "https://app.test/", "title": "Fixture", "ready": true, "state_version": version}, "nodes": nodes, "truncated": false})
}

/// A desktop-kind client attached as the host of the session.
async fn attach(core: &CoreProcess, bsid: &Id, task: &Id, partition: &str, n: u8) -> Client {
    let mut host = core.client_of_kind(ClientKind::Desktop).await;
    let ack = host
        .command(envelope(
            id16(n),
            "AttachBrowserHost",
            AttachBrowserHost {
                browser_session_id: Some(bsid.clone()),
                host_kind: "test-host".into(),
                partition: partition.into(),
                sandboxed: true,
                context_isolated: true,
                node_integration: false,
                task_id: Some(task.clone()),
            }
            .encode_to_vec(),
        ))
        .await
        .unwrap();
    let _: BrowserHostAttached = Client::result(&ack).unwrap();
    host
}

/// Serve the host's requests in the background; `answer` decides each (None = silence).
fn serve(
    mut host: Client,
    answer: impl Fn(&Value) -> Option<Value> + Send + 'static,
) -> Arc<Mutex<Vec<Value>>> {
    let served: Arc<Mutex<Vec<Value>>> = Arc::default();
    let s2 = served.clone();
    tokio::spawn(async move {
        while let Ok(Some(req)) = host.next_browser_request().await {
            let r: Value = serde_json::from_str(&req.request_json).unwrap();
            s2.lock().unwrap().push(r.clone());
            if let Some(resp) = answer(&r) {
                host.command(envelope(
                    rand_id(),
                    "BrowserHostResponse",
                    BrowserHostResponse {
                        request_id: req.request_id,
                        response_json: resp.to_string(),
                    }
                    .encode_to_vec(),
                ))
                .await
                .unwrap();
            }
        }
    });
    served
}

async fn open_session(c: &mut Client, task: &Id, g: Option<u64>, n: u8) -> BrowserSessionOpened {
    let ack = c
        .command(envelope_fenced(
            id16(n),
            "OpenBrowserSession",
            OpenBrowserSession {
                task_id: Some(task.clone()),
            }
            .encode_to_vec(),
            g,
        ))
        .await
        .unwrap();
    Client::result(&ack).unwrap()
}

fn tool_texts(seen: &Seen) -> Vec<String> {
    let bodies = seen.lock().unwrap().clone();
    bodies
        .last()
        .map(|b| {
            b["messages"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|m| m["role"] == "tool")
                .map(|m| m["content"].as_str().unwrap_or_default().to_owned())
                .collect()
        })
        .unwrap_or_default()
}

/// Every tool result the model was ever shown, in the order first shown (a later request may no longer carry an old result).
fn all_tool_texts(seen: &Seen) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for b in seen.lock().unwrap().iter() {
        for m in b["messages"].as_array().into_iter().flatten() {
            if m["role"] == "tool" {
                let c = m["content"].as_str().unwrap_or_default().to_owned();
                if !out.contains(&c) {
                    out.push(c);
                }
            }
        }
    }
    out
}

fn call(name: &str, args: Value) -> Value {
    json!({"calls": [{"name": name, "args": args}]})
}

async fn runtime(c: &mut Client, bsid: &Id, task: &Id) -> BrowserRuntimeView {
    let ack = c
        .command(envelope(
            rand_id(),
            "GetBrowserRuntime",
            GetBrowserRuntime {
                browser_session_id: Some(bsid.clone()),
                task_id: Some(task.clone()),
            }
            .encode_to_vec(),
        ))
        .await
        .unwrap();
    Client::result(&ack).unwrap()
}

/// QUAL-PX-121: a click whose host dies mid-action latches UNKNOWN, refuses the next action without sending it, clears only after a fresh observation, and the receipt says UNKNOWN; refusals carry the escalation field.
#[tokio::test(flavor = "multi_thread")]
async fn qual_px_121_an_unknown_outcome_latches_the_session_until_a_fresh_observation_and_every_refusal_carries_its_escalation()
 {
    let (_repo, root) = plain_repo(&[("README.md", "# browse\n")]);
    let press = reference_of("button", "Press", &["main:".to_owned()], 0);
    let script = vec![
        call(
            "plan.update",
            json!({"outcome": "the latch is exercised", "expected_files": []}),
        ),
        call("browser.navigate", json!({"url": "https://app.test/"})),
        call("browser.snapshot", json!({"mode": "full"})),
        // The host fails after the input may have been dispatched.
        call("browser.act", json!({"ref": press, "action": "click"})),
        // The model repeats it blind: refused, never sent.
        call("browser.act", json!({"ref": press, "action": "click"})),
        // A navigation is an input too.
        call("browser.navigate", json!({"url": "https://app.test/other"})),
        // A fresh observation reconciles; then the click is allowed again.
        call("browser.snapshot", json!({})),
        call("browser.act", json!({"ref": press, "action": "click"})),
        // A refusal the host makes: typed, with its escalation.
        call(
            "browser.navigate",
            json!({"url": "http://169.254.169.254/"}),
        ),
        call(
            "browser.scroll",
            json!({"direction": "down", "script": "alert(1)"}),
        ),
        json!({"calls": [{"name": "task.complete", "args": {"summary": "latched and reconciled", "self_review": {"findings": []}}}]}),
    ];
    let (base, seen) = scripted_model(script, vec![]).await;
    let dir = tempfile::tempdir().unwrap();
    let env = model_env(&base);
    let env: Vec<(&str, &str)> = env.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
    let core = CoreProcess::spawn_with_env(dir.path(), &env);
    let mut c = core.client().await;
    let (session, g) = session_with_lease(&mut c, 0x10).await;
    let task = create_task(&mut c, &session, g, &root, 0x11, "local_trusted", "latch").await;
    let opened = open_session(&mut c, &task, g, 0x12).await;
    let bsid = opened.browser_session_id.clone().unwrap();
    let host = attach(&core, &bsid, &task, &opened.partition, 0x13).await;
    let acts = Arc::new(Mutex::new(0u32));
    let acts2 = acts.clone();
    let served = serve(host, move |r| match r["kind"].as_str().unwrap() {
        "navigate" if r["url"].as_str().is_some_and(|u| u.contains("169.254")) => Some(
            json!({"kind": "error", "code": "TARGET_NOT_ALLOWED", "message": "169.254.169.254 is a cloud-metadata address"}),
        ),
        "navigate" => Some(
            json!({"kind": "state", "state": {"url": r["url"], "title": "Fixture", "ready": true, "state_version": 1}}),
        ),
        "snapshot" => Some(page(1, false)),
        "act" => {
            let mut n = acts2.lock().unwrap();
            *n += 1;
            if *n == 1 {
                Some(
                    json!({"kind": "error", "code": "OUTCOME_UNKNOWN", "message": "the page's process failed during the click"}),
                )
            } else {
                Some(
                    json!({"kind": "acted", "state": {"url": "https://app.test/", "title": "Fixture", "ready": true, "state_version": 2}, "navigated": false, "detail": "click at 10,10"}),
                )
            }
        }
        other => Some(json!({"kind": "error", "code": "UNSUPPORTED", "message": other})),
    });
    start_task(&mut c, &task, g, 0x14, "gpt-5").await;
    let st = wait_task(&mut c, &task, 120).await;
    assert!(!st.loop_alive, "{st:?}");
    let t = tool_texts(&seen);
    // The unknown outcome is the call's status, with the latch named.
    assert!(t[3].contains("status: UNKNOWNOUTCOME"), "{}", t[3]);
    assert!(
        t[3].contains("\"latched\":true") && t[3].contains("\"escalation\":\"re_observe\""),
        "{}",
        t[3]
    );
    // The blind repeat is refused with the latch's code and never reached the host.
    assert!(
        t[4].contains("UNKNOWN_OUTCOME_LATCHED") && t[4].contains("\"escalation\":\"re_observe\""),
        "{}",
        t[4]
    );
    assert!(
        t[5].contains("UNKNOWN_OUTCOME_LATCHED"),
        "a navigation is an input too: {}",
        t[5]
    );
    // Only the read lifts it; the read says so.
    assert!(t[6].contains("\"reconciled\""), "{}", t[6]);
    assert!(t[7].contains("status: SUCCESS"), "{}", t[7]);
    assert_eq!(
        *acts.lock().unwrap(),
        2,
        "the latched click never reached the host"
    );
    assert!(
        !served
            .lock()
            .unwrap()
            .iter()
            .any(|r| r["kind"] == "navigate" && r["url"] == "https://app.test/other"),
        "the latched navigation never reached the host"
    );
    // A host refusal is typed with the escalation of the shared taxonomy; a script is not an argument.
    assert!(
        t[8].contains("TARGET_NOT_ALLOWED") && t[8].contains("\"escalation\":\"ask_user\""),
        "{}",
        t[8]
    );
    assert!(
        t[9].contains("SCHEMA_VIOLATION") || t[9].contains("INVALID"),
        "{}",
        t[9]
    );
    // The log: the unknown outcome, the receipt that says so, the reconciliation.
    let evs = replay(&core, &session).await;
    let of = |ty: &str| {
        evs.iter()
            .filter(|e| e["event_type"] == ty)
            .collect::<Vec<_>>()
    };
    assert_eq!(of("BrowserOutcomeUnknown").len(), 1);
    assert_eq!(of("BrowserOutcomeReconciled").len(), 1);
    assert!(
        of("EffectReceiptAppended")
            .iter()
            .any(|e| e["payload"]["payload"]["receipt"]["status"] == "UNKNOWN_OUTCOME"),
        "{:#?}",
        of("EffectReceiptAppended")
    );
    assert!(!of("ToolCallUnknownOutcome").is_empty());
    assert!(runtime(&mut c, &bsid, &task).await.latch.is_empty());
}

/// QUAL-PX-121 (the remaining clauses, on the real Core with the host's stand-in): `browser.console` and `browser.network` return what the page produced with secrets redacted and the untrusted label, in a bounded read; `browser.capture` asks for the bounded viewport and the media budget refuses an oversized image; `browser.wait` resolves on a condition and times out typed with the escalation; and a model-supplied script or expression is not an argument of any of these tools - the Kernel's schema refuses it and the host never hears of it.
#[tokio::test(flavor = "multi_thread")]
async fn qual_px_121_feedback_reads_are_redacted_bounded_and_untrusted_and_a_script_argument_reaches_no_tool()
 {
    use base64::Engine as _;
    let (_repo, root) = plain_repo(&[("README.md", "# browse\n")]);
    // The script is indexed by the number of tool results the model has seen, so a turn of several
    // calls takes that many places; and a run that makes no progress for six turns suspends, so reads share turns.
    let mut script = vec![
        call(
            "plan.update",
            json!({"outcome": "the feedback tools are exercised", "expected_files": []}),
        ),
        call("browser.navigate", json!({"url": "https://app.test/"})),
    ];
    let mut at = std::collections::BTreeMap::new();
    let turns: Vec<Vec<(&'static str, &'static str, Value)>> = vec![
        // A script or expression is never an argument, and an unbounded read is not expressible: nothing reaches the host.
        vec![
            (
                "console_script",
                "browser.console",
                json!({"script": "document.cookie"}),
            ),
            (
                "network_script",
                "browser.network",
                json!({"expression": "fetch('https://evil.test')"}),
            ),
            (
                "capture_script",
                "browser.capture",
                json!({"viewport": true, "script": "window.stop()"}),
            ),
            (
                "wait_script",
                "browser.wait",
                json!({"until": {"text": "x"}, "script": "while(1){}"}),
            ),
            (
                "scroll_script",
                "browser.scroll",
                json!({"direction": "down", "script": "alert(1)"}),
            ),
            (
                "console_unbounded",
                "browser.console",
                json!({"limit": 100000}),
            ),
        ],
        // The reads themselves.
        vec![
            ("console", "browser.console", json!({"level": "errors"})),
            ("network", "browser.network", json!({"failed_only": true})),
        ],
        // The viewport, then an image beyond the pixel budget.
        vec![(
            "capture",
            "browser.capture",
            json!({"viewport": true, "reason": "layout"}),
        )],
        vec![(
            "capture_huge",
            "browser.capture",
            json!({"viewport": true, "reason": "layout again"}),
        )],
        // Wait: times out typed, and resolves.
        vec![
            (
                "wait_timeout",
                "browser.wait",
                json!({"until": {"text": "never appears anywhere"}, "timeout_ms": 400}),
            ),
            (
                "wait_ok",
                "browser.wait",
                json!({"until": {"text": "Press"}, "timeout_ms": 4000}),
            ),
        ],
    ];
    for turn in turns {
        let first = script.len();
        let mut calls = Vec::new();
        for (label, name, args) in turn {
            at.insert(label, first + calls.len());
            calls.push(json!({"name": name, "args": args}));
        }
        let n = calls.len();
        script.push(json!({ "calls": calls }));
        script.extend(std::iter::repeat_n(json!({}), n - 1));
    }
    script.push(json!({"calls": [{"name": "task.complete", "args": {"summary": "read the feedback", "self_review": {"findings": []}}}]}));
    let (base, seen) = scripted_model(script, vec![]).await;
    let dir = tempfile::tempdir().unwrap();
    let env = model_env(&base);
    let env: Vec<(&str, &str)> = env.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
    let core = CoreProcess::spawn_with_env(dir.path(), &env);
    let mut c = core.client().await;
    let (session, g) = session_with_lease(&mut c, 0x30).await;
    let task = create_task(
        &mut c,
        &session,
        g,
        &root,
        0x31,
        "local_trusted",
        "feedback",
    )
    .await;
    let opened = open_session(&mut c, &task, g, 0x32).await;
    let bsid = opened.browser_session_id.clone().unwrap();
    let host = attach(&core, &bsid, &task, &opened.partition, 0x33).await;
    let one_pixel = base64::engine::general_purpose::STANDARD
        .decode("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNkYPhfDwAChwGA60e6kgAAAABJRU5ErkJggg==")
        .unwrap();
    let mut huge = one_pixel.clone();
    // IHDR width and height declared as 20000 x 20000 (400 million pixels), before any decoding.
    huge[16..20].copy_from_slice(&20_000u32.to_be_bytes());
    huge[20..24].copy_from_slice(&20_000u32.to_be_bytes());
    let (small_b64, huge_b64) = (
        base64::engine::general_purpose::STANDARD.encode(&one_pixel),
        base64::engine::general_purpose::STANDARD.encode(&huge),
    );
    let captures = Arc::new(Mutex::new(0u32));
    let captures2 = captures.clone();
    let state =
        json!({"url": "https://app.test/", "title": "Fixture", "ready": true, "state_version": 1});
    let served = serve(host, move |r| match r["kind"].as_str().unwrap() {
        "navigate" => Some(json!({"kind": "state", "state": state.clone()})),
        "snapshot" => Some(page(1, false)),
        "console" => Some(
            json!({"kind": "console", "next_seq": 2, "dropped": 0, "entries": [
                {"seq": 1, "level": "error", "source": "console", "text": "boom token=abcd1234efgh in https://user:hunter2@app.test/x?api_key=sekrit123", "at_ms": 1},
                {"seq": 2, "level": "log", "source": "console", "text": "fine", "at_ms": 2}
            ]}),
        ),
        "network" => Some(
            json!({"kind": "network", "next_seq": 1, "dropped": 0, "entries": [
                {"seq": 1, "method": "GET", "url": "https://app.test/api/missing?token=sekrit123&x=1", "status": 500, "resource_type": "Fetch", "failed": "net::ERR_FAILED", "at_ms": 1}
            ]}),
        ),
        "capture" => {
            let mut n = captures2.lock().unwrap();
            *n += 1;
            let png = if *n == 1 {
                small_b64.clone()
            } else {
                huge_b64.clone()
            };
            Some(
                json!({"kind": "capture", "state": state.clone(), "png_base64": png, "clip": null, "width": 1, "height": 1}),
            )
        }
        other => Some(json!({"kind": "error", "code": "UNSUPPORTED", "message": other})),
    });
    start_task(&mut c, &task, g, 0x34, "gpt-5").await;
    let st = wait_task(&mut c, &task, 120).await;
    assert!(!st.loop_alive, "{st:?}");
    let t = all_tool_texts(&seen);
    assert!(
        t.len() > at["wait_ok"],
        "the run ended early: {t:#?}\n{st:?}"
    );
    // Refused by the schema before any host request.
    let ix = |l: &str| at[l];
    for (name, i) in [
        ("console", ix("console_script")),
        ("network", ix("network_script")),
        ("capture", ix("capture_script")),
        ("wait", ix("wait_script")),
        ("scroll", ix("scroll_script")),
    ] {
        assert!(
            t[i].contains("SCHEMA_VIOLATION") || t[i].contains("INVALID"),
            "{name}: {}",
            t[i]
        );
        assert!(
            !t[i].contains("status: SUCCESS"),
            "{name}: a script is not an argument: {}",
            t[i]
        );
    }
    assert!(
        t[ix("console_unbounded")].contains("SCHEMA_VIOLATION")
            || t[ix("console_unbounded")].contains("INVALID"),
        "an unbounded read is refused: {}",
        t[ix("console_unbounded")]
    );
    // The reads: secrets gone, the untrusted label on, the benign line filtered by the level.
    assert!(
        t[ix("console")].contains("status: SUCCESS")
            && t[ix("console")].contains("UNTRUSTED_WEB_CONTENT"),
        "{}",
        t[ix("console")]
    );
    for secret in ["abcd1234efgh", "hunter2", "sekrit123"] {
        assert!(
            !t[ix("console")].contains(secret) && !t[ix("network")].contains(secret),
            "{secret} leaked: {} / {}",
            t[ix("console")],
            t[ix("network")]
        );
    }
    assert!(
        t[ix("console")].contains("[REDACTED]") && t[ix("console")].contains("boom"),
        "{}",
        t[ix("console")]
    );
    assert!(
        !t[ix("console")].contains("\"fine\""),
        "the level filter applies: {}",
        t[ix("console")]
    );
    assert!(
        t[ix("network")].contains("UNTRUSTED_WEB_CONTENT")
            && t[ix("network")].contains("\"status\":500")
            && t[ix("network")].contains("token=[REDACTED]"),
        "{}",
        t[ix("network")]
    );
    // The viewport is asked for at the 1280 by 800 budget; an image beyond the media budget is refused typed.
    assert!(
        t[ix("capture")].contains("status: SUCCESS")
            && t[ix("capture")].contains("\"viewport\":true"),
        "{}",
        t[ix("capture")]
    );
    assert!(
        t[ix("capture_huge")].contains("MEDIA_BUDGET_EXCEEDED"),
        "{}",
        t[ix("capture_huge")]
    );
    // Wait: typed timeout with the escalation; then a resolution.
    assert!(
        t[ix("wait_timeout")].contains("WAIT_TIMEOUT")
            && t[ix("wait_timeout")].contains("\"escalation\":\"re_observe\""),
        "{}",
        t[ix("wait_timeout")]
    );
    assert!(
        t[ix("wait_ok")].contains("status: SUCCESS") && t[ix("wait_ok")].contains("\"waited_ms\""),
        "{}",
        t[ix("wait_ok")]
    );
    // What the host heard: the schema-refused calls never arrived; the permitted ones did, once each.
    let kinds: Vec<String> = served
        .lock()
        .unwrap()
        .iter()
        .filter_map(|r| r["kind"].as_str().map(str::to_owned))
        .collect();
    let count = |k: &str| kinds.iter().filter(|x| x.as_str() == k).count();
    assert_eq!(count("console"), 1, "{kinds:?}");
    assert_eq!(count("network"), 1, "{kinds:?}");
    assert_eq!(count("capture"), 2, "{kinds:?}");
    assert_eq!(count("scroll"), 0, "{kinds:?}");
    let fit_ok = served
        .lock()
        .unwrap()
        .iter()
        .filter(|r| r["kind"] == "capture")
        .all(|r| r["fit"] == json!([1280, 800]));
    assert!(
        fit_ok,
        "the viewport is bounded at the host: {:?}",
        served.lock().unwrap()
    );
    // Each call that ran went through the Kernel: proposed, decided by the policy, then dispatched - in that order, per call.
    let evs = replay(&core, &session).await;
    let by_tool = |tool: &str, ty: &str| -> Vec<&Value> {
        let ids: Vec<&Value> = evs
            .iter()
            .filter(|e| {
                e["event_type"] == "ToolCallProposed"
                    && e["payload"]["payload"]["tool_name"] == tool
            })
            .map(|e| &e["aggregate_id"])
            .collect();
        evs.iter()
            .filter(|e| e["event_type"] == ty && ids.contains(&&e["aggregate_id"]))
            .collect()
    };
    for (tool, ran) in [
        ("browser.console", 1),
        ("browser.network", 1),
        ("browser.capture", 2),
        ("browser.wait", 2),
        ("browser.scroll", 0),
    ] {
        let dispatched = by_tool(tool, "ToolCallDispatched");
        assert_eq!(dispatched.len(), ran, "{tool} dispatched");
        for d in dispatched {
            let decision = evs
                .iter()
                .find(|e| {
                    e["event_type"] == "ToolCallPolicyDecision"
                        && e["aggregate_id"] == d["aggregate_id"]
                })
                .unwrap_or_else(|| panic!("{tool}: no policy decision before dispatch"));
            assert_eq!(decision["payload"]["payload"]["allowed"], true, "{tool}");
            assert!(
                decision["offset"].as_u64() < d["offset"].as_u64(),
                "{tool}: decided before dispatched"
            );
        }
    }
}

/// QUAL-PX-122: a change notice makes the Core read, compile and journal the page with no model call; the known-state map and the held fingerprint survive a killed Core, so the same element keeps its reference and `since_fingerprint` still answers.
#[tokio::test(flavor = "multi_thread")]
async fn qual_px_122_a_change_notice_is_journaled_as_a_delta_without_a_model_call_and_the_known_state_survives_a_killed_core()
 {
    let (_repo, root) = plain_repo(&[("README.md", "# browse\n")]);
    let dir = tempfile::tempdir().unwrap();
    let mut core = CoreProcess::spawn_with_env(dir.path(), &[]);
    let mut c = core.client().await;
    let (session, g) = session_with_lease(&mut c, 0x20).await;
    let task = create_task(&mut c, &session, g, &root, 0x21, "local_trusted", "observe").await;
    let opened = open_session(&mut c, &task, g, 0x22).await;
    let bsid = opened.browser_session_id.clone().unwrap();
    let host = attach(&core, &bsid, &task, &opened.partition, 0x23).await;
    let changed = Arc::new(Mutex::new(false));
    let changed2 = changed.clone();
    let served = serve(host, move |r| match r["kind"].as_str().unwrap() {
        "snapshot" => {
            assert_eq!(r["observer"], true, "the Core's own read is flagged: {r}");
            Some(page(
                if *changed2.lock().unwrap() { 2 } else { 1 },
                *changed2.lock().unwrap(),
            ))
        }
        _ => None,
    });
    let mut notifier = core.client_of_kind(ClientKind::Desktop).await;
    let notice = |seq: u64| {
        envelope(
            rand_id(),
            "BrowserHostNotice",
            BrowserHostNotice {
                browser_session_id: Some(bsid.clone()),
                notice_json:
                    json!({"change_seq": seq, "kind": "mutation", "added": 1, "coalesced": 40})
                        .to_string(),
            }
            .encode_to_vec(),
        )
    };
    let ack = notifier.command(notice(1)).await.unwrap();
    let n: BrowserHostNoticed = Client::result(&ack).unwrap();
    assert!(n.accepted);
    // The first read is the first page the Core holds.
    let first = wait_runtime(&mut c, &bsid, &task, |r| {
        r.compiled_seq >= 1 && !r.last_fingerprint.is_empty()
    })
    .await;
    *changed.lock().unwrap() = true;
    // Thousands of mutations a second reach the Core as a few notices; the page changed.
    for seq in 2..=40 {
        notifier.command(notice(seq)).await.unwrap();
    }
    let after = wait_runtime(&mut c, &bsid, &task, |r| {
        r.compiled_seq >= 40 && r.last_fingerprint != first.last_fingerprint
    })
    .await;
    assert!(after.known_entities >= 4, "{after:?}");
    assert!(after.history >= 2);
    assert!(
        after.delivered_fingerprint.is_empty(),
        "the model has been shown nothing: {after:?}"
    );
    let reads = served
        .lock()
        .unwrap()
        .iter()
        .filter(|r| r["kind"] == "snapshot")
        .count();
    assert!(
        reads <= 12,
        "{reads} Core reads for 40 notices: the Core folds them"
    );
    // Journaled as a semantic delta; the model never ran.
    let evs = replay(&core, &session).await;
    let changes: Vec<&Value> = evs
        .iter()
        .filter(|e| e["event_type"] == "BrowserPageChanged")
        .collect();
    assert!(!changes.is_empty());
    let last = changes.last().unwrap()["payload"]["payload"].clone();
    assert_eq!(last["added"], 1, "{last}");
    assert_eq!(last["to_fingerprint"], after.last_fingerprint);
    assert!(last["page_ref"].as_str().unwrap().len() == 64);
    assert!(
        evs.iter()
            .all(|e| e["event_type"] != "ToolCallRequested" && e["event_type"] != "ModelRequested")
    );
    // A killed Core: the known-state map is rebuilt from the log and the persisted page.
    drop(notifier);
    core.kill();
    let core2 = CoreProcess::spawn_with_env(dir.path(), &[]);
    let mut c2 = core2.client().await;
    let (_s2, _g2) = (&session, g);
    let restored = runtime(&mut c2, &bsid, &task).await;
    assert_eq!(
        restored.last_fingerprint, after.last_fingerprint,
        "the held page is the persisted one"
    );
    assert!(restored.known_entities >= 4, "{restored:?}");
    assert_eq!(restored.history, 1);
    // The element keeps its reference: the identity is the DOM attribute, not the position.
    let _ = reference_of_ident("textbox", "", "id:note");
}

async fn wait_runtime(
    c: &mut Client,
    bsid: &Id,
    task: &Id,
    ok: impl Fn(&BrowserRuntimeView) -> bool,
) -> BrowserRuntimeView {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    loop {
        let r = runtime(c, bsid, task).await;
        if ok(&r) {
            return r;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "runtime never reached the state: {r:?}"
        );
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
}
