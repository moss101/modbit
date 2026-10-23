//! QUAL-PX-050 / QUAL-PX-057 / QUAL-PX-059 on the real Core: the actual
//! `modbit-core` binary over its real socket, its real SQLite log, the real
//! Capability Kernel, tool pipeline and execution broker, a real Git
//! repository, and the repository's standard model stand-in — a scripted
//! OpenAI-compatible streaming server that records every request body.
//!
//! * PX-050: queued inputs are listed, edited, removed, reordered and sent
//!   now; they drain in order as separate turns, survive a SIGKILL of the
//!   Core, and a send-now cuts a live model stream (an aborted stream with
//!   source USER_INTERRUPT, the partial text kept) or a shell command the
//!   broker is running, then starts the next turn once.
//! * PX-057: run modes and durable allowlist rules over the kernel, with the
//!   real approval flow; the always-ask classes ask under the widest mode.
//! * PX-059: the nine categories sum to the provider-reported total.
#![cfg(unix)]

mod px_common;

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use modbit_protocol::client::{Client, ClientError};
use modbit_protocol::v1::{
    AddAllowRule, AllowRuleList, AllowRuleRevoked, AllowRuleView, ApprovalList,
    ContextAccountingView, EditQueuedInput, EffectReceiptList, GetContextAccounting, GetRunMode,
    Id, InputQueued, InterruptAccepted, InterruptTask, ListAllowRules, ListApprovals,
    ListQueuedInputs, QueueInput, QueuedInputChanged, QueuedInputList, RemoveQueuedInput,
    ReorderQueuedInput, ResolveApproval, ResumeTask, RevokeAllowRule, RunModeView,
    SendQueuedInputNow, SetRunMode, StartTask, TaskResumeResult, TaskRunStarted,
};
use prost::Message;
use px_common::*;
use serde_json::{Value, json};

// ---- commands ----

/// A command's decoded result, or its typed refusal as `(code, message)`.
async fn cmd<R: Message + Default>(
    c: &mut Client,
    command_type: &str,
    msg: impl Message,
    generation: Option<u64>,
) -> Result<R, (String, String)> {
    match c
        .command(envelope_fenced(
            rand_id(),
            command_type,
            msg.encode_to_vec(),
            generation,
        ))
        .await
    {
        Ok(ack) => Ok(Client::result(&ack).unwrap()),
        Err(ClientError::Rejected { code, message }) => Err((code, message)),
        Err(e) => panic!("{command_type}: {e}"),
    }
}

/// The same command under a fixed command id, with the raw ack's status.
async fn cmd_with_id<R: Message + Default>(
    c: &mut Client,
    command_id: Id,
    command_type: &str,
    msg: impl Message,
    generation: Option<u64>,
) -> Result<(R, i32), (String, String)> {
    match c
        .command(envelope_fenced(
            command_id,
            command_type,
            msg.encode_to_vec(),
            generation,
        ))
        .await
    {
        Ok(ack) => Ok((Client::result(&ack).unwrap(), ack.status)),
        Err(ClientError::Rejected { code, message }) => Err((code, message)),
        Err(e) => panic!("{command_type}: {e}"),
    }
}

fn code_of<T>(r: Result<T, (String, String)>) -> String {
    match r {
        Ok(_) => "OK".into(),
        Err((c, _)) => c,
    }
}

async fn queue(c: &mut Client, task: &Id, g: Option<u64>, id: &str, mode: &str, text: &str) {
    let r: Result<InputQueued, _> = cmd(
        c,
        "QueueInput",
        QueueInput {
            task_id: Some(task.clone()),
            input_id: id.into(),
            mode: mode.into(),
            text: text.into(),
        },
        g,
    )
    .await;
    r.unwrap();
}

async fn list_queue(c: &mut Client, task: &Id, settled: bool) -> QueuedInputList {
    cmd(
        c,
        "ListQueuedInputs",
        ListQueuedInputs {
            task_id: Some(task.clone()),
            include_settled: settled,
        },
        None,
    )
    .await
    .unwrap()
}

fn queued_ids(l: &QueuedInputList) -> Vec<String> {
    l.items
        .iter()
        .filter(|i| i.state == "QUEUED")
        .map(|i| i.input_id.clone())
        .collect()
}

async fn start(
    c: &mut Client,
    task: &Id,
    g: Option<u64>,
    model: &str,
    max_no_progress: u32,
) -> TaskRunStarted {
    cmd(
        c,
        "StartTask",
        StartTask {
            task_id: Some(task.clone()),
            endpoint: String::new(),
            model: model.into(),
            max_turns: 30,
            max_tool_calls: 0,
            max_no_progress_turns: max_no_progress,
            skills: vec![],
            ..Default::default()
        },
        g,
    )
    .await
    .unwrap()
}

// ---- the scripted streaming model ----

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum End {
    #[default]
    Finish,
    /// The connection stays open and silent after the chunks.
    Hang,
}

/// One scripted response.
#[derive(Clone, Debug, Default)]
struct Step {
    chunks: Vec<String>,
    gap_ms: u64,
    calls: Vec<(String, Value)>,
    end: End,
    /// What the provider reports counting (prompt, completion); default is a
    /// count derived from the request size.
    usage: Option<(u64, u64)>,
}

impl Step {
    fn text(t: &str) -> Self {
        Step {
            chunks: vec![t.into()],
            ..Step::default()
        }
    }
    fn slow(chunks: usize, gap_ms: u64) -> Self {
        Step {
            chunks: (0..chunks).map(|i| format!("word{i} ")).collect(),
            gap_ms,
            ..Step::default()
        }
    }
    fn hang_after(chunks: usize) -> Self {
        Step {
            chunks: (0..chunks).map(|i| format!("partial{i} ")).collect(),
            gap_ms: 20,
            end: End::Hang,
            ..Step::default()
        }
    }
    fn call(name: &str, args: Value) -> Self {
        Step {
            calls: vec![(name.into(), args)],
            ..Step::default()
        }
    }
}

type Script = Arc<dyn Fn(&Value, usize) -> Step + Send + Sync>;

const HEAD: &str = "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\nx-request-id: req_ctl\r\nconnection: close\r\ntransfer-encoding: chunked\r\n\r\n";

/// A scripted OpenAI-compatible streaming server: request `n` is answered by
/// `script(body, n)`. Every request body is recorded.
async fn model(script: Script) -> (String, Seen) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let served = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let seen: Seen = Arc::new(Mutex::new(Vec::new()));
    let seen2 = Arc::clone(&seen);
    tokio::spawn(async move {
        loop {
            let Ok((mut sock, _)) = listener.accept().await else {
                return;
            };
            let script = Arc::clone(&script);
            let seen = Arc::clone(&seen2);
            let n = served.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            tokio::spawn(async move {
                let mut buf = Vec::new();
                let mut tmp = [0u8; 8192];
                let (head_end, len) = loop {
                    let r = sock.read(&mut tmp).await.unwrap_or(0);
                    if r == 0 {
                        return;
                    }
                    buf.extend_from_slice(&tmp[..r]);
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
                        break (i + 4, len);
                    }
                };
                while buf.len() < head_end + len {
                    let r = sock.read(&mut tmp).await.unwrap_or(0);
                    if r == 0 {
                        break;
                    }
                    buf.extend_from_slice(&tmp[..r]);
                }
                let body: Value =
                    serde_json::from_slice(&buf[head_end..head_end + len]).unwrap_or_default();
                let step = script(&body, n);
                let prompt_tokens = serde_json::to_string(&body["messages"])
                    .map(|m| m.len().div_ceil(4))
                    .unwrap_or(0) as u64;
                seen.lock().unwrap().push(body);
                let write = |frame: String| {
                    let data = format!("data: {frame}\n\n");
                    format!("{:x}\r\n{}\r\n", data.len(), data).into_bytes()
                };
                let delta = |d: Value| {
                    json!({"id":"c","model":"scripted-1","choices":[{"index":0,"delta":d,"finish_reason":null}]}).to_string()
                };
                if sock.write_all(HEAD.as_bytes()).await.is_err() {
                    return;
                }
                for c in &step.chunks {
                    if sock
                        .write_all(&write(delta(json!({"content": c}))))
                        .await
                        .is_err()
                    {
                        return;
                    }
                    let _ = sock.flush().await;
                    if step.gap_ms > 0 {
                        tokio::time::sleep(Duration::from_millis(step.gap_ms)).await;
                    }
                }
                if step.end == End::Hang {
                    tokio::time::sleep(Duration::from_secs(600)).await;
                    return;
                }
                for (i, (name, args)) in step.calls.iter().enumerate() {
                    let _ = sock
                        .write_all(&write(delta(json!({"tool_calls":[{"index":i,"id":format!("call_{n}_{i}"),"type":"function","function":{"name":name,"arguments":args.to_string()}}]}))))
                        .await;
                }
                let finish = if step.calls.is_empty() {
                    "stop"
                } else {
                    "tool_calls"
                };
                let (prompt, completion) = step.usage.unwrap_or((prompt_tokens, 10));
                let _ = sock
                    .write_all(&write(json!({"id":"c","model":"scripted-1","choices":[{"index":0,"delta":{},"finish_reason":finish}],"usage":{"prompt_tokens":prompt,"completion_tokens":completion,"prompt_tokens_details":{"cached_tokens":0}}}).to_string()))
                    .await;
                let _ = sock.write_all(&write("[DONE]".into())).await;
                let _ = sock.write_all(b"0\r\n\r\n").await;
                let _ = sock.flush().await;
                let _ = sock.shutdown().await;
            });
        }
    });
    (format!("http://127.0.0.1:{port}"), seen)
}

/// The user-role message texts of one recorded request, in order.
fn user_texts(body: &Value) -> Vec<String> {
    body["messages"]
        .as_array()
        .map(|m| {
            m.iter()
                .filter(|x| x["role"] == "user")
                .filter_map(|x| x["content"].as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default()
}

/// The labelled inputs (`[FOLLOW_UP] text`, `[SEND_NOW] text`, ...) the
/// request carries, in order.
fn labelled(body: &Value) -> Vec<String> {
    user_texts(body)
        .into_iter()
        .filter(|t| {
            ["[FOLLOW_UP] ", "[STEER] ", "[COLLECT] ", "[SEND_NOW] "]
                .iter()
                .any(|p| t.starts_with(p))
        })
        .collect()
}

fn env_of(base: &str, extra: &[(&str, &str)]) -> Vec<(String, String)> {
    let mut env = model_env(base);
    for (k, v) in extra {
        env.push(((*k).into(), (*v).into()));
    }
    env
}

fn spawn(dir: &std::path::Path, env: &[(String, String)]) -> CoreProcess {
    let refs: Vec<(&str, &str)> = env.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
    CoreProcess::spawn_with_env(dir, &refs)
}

/// An event's own payload (the replay shapes it as `{kind, payload}`).
fn pl(e: &Value) -> &Value {
    &e["payload"]["payload"]
}

fn kinds(events: &[Value], kind: &str) -> Vec<Value> {
    events
        .iter()
        .filter(|e| e["event_type"] == kind)
        .cloned()
        .collect()
}

/// A live subscriber on the session: every event from the start, as the
/// replay helper shapes them, collected as it arrives.
#[derive(Clone)]
struct Live(Arc<Mutex<Vec<Value>>>);

async fn live(core: &CoreProcess, session: &Id) -> Live {
    let out = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&out);
    let mut c = core.client().await;
    let session = session.clone();
    tokio::spawn(async move {
        if c.subscribe(session, 0).await.is_err() {
            return;
        }
        while let Ok(Some(e)) = c.next_event().await {
            let ev = e.event.unwrap();
            sink.lock().unwrap().push(json!({
                "offset": e.offset,
                "aggregate_type": ev.aggregate_type,
                "aggregate_id": hex::encode(&ev.aggregate_id.unwrap().value),
                "event_type": ev.event_type,
                "task_id": ev.task_id.map(|t| hex::encode(&t.value)),
                "payload": serde_json::from_slice::<Value>(&ev.payload).unwrap_or_default(),
            }));
        }
    });
    Live(out)
}

impl Live {
    fn all(&self) -> Vec<Value> {
        self.0.lock().unwrap().clone()
    }

    /// Wait until `pred` holds over what has arrived, or panic.
    async fn until(&self, secs: u64, what: &str, pred: impl Fn(&[Value]) -> bool) -> Vec<Value> {
        let deadline = Instant::now() + Duration::from_secs(secs);
        loop {
            let ev = self.all();
            if pred(&ev) {
                return ev;
            }
            assert!(Instant::now() < deadline, "timed out waiting for {what}");
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }
}

// =====================================================================
// PX-050
// =====================================================================

/// Queue three messages during a streaming turn: edit one, delete one,
/// reorder; they drain in the order the person left them, each as its own
/// turn, and the deleted one is never dispatched.
#[tokio::test]
async fn qual_px_050_queue_edit_delete_and_reorder_take_effect_and_drain_in_order_as_separate_turns()
 {
    let (_repo, root) = plain_repo(&[("a.txt", "alpha\n")]);
    // Turn 1 streams slowly (about 3 s) so the person queues during it; every
    // later turn answers at once.
    let (base, seen) = model(Arc::new(|_, n| {
        if n == 0 {
            Step::slow(30, 100)
        } else {
            Step::text("noted")
        }
    }))
    .await;
    let dir = tempfile::tempdir().unwrap();
    let core = spawn(dir.path(), &env_of(&base, &[]));
    let mut c = core.client().await;
    let (session, g) = session_with_lease(&mut c, 0x10).await;
    let feed = live(&core, &session).await;
    let task = create_task(&mut c, &session, g, &root, 0x11, "local_trusted", "talk").await;
    start(&mut c, &task, g, "gpt-5-mini", 8).await;
    feed.until(30, "the first delta", |e| {
        !kinds(e, "AssistantTextDelta").is_empty()
    })
    .await;
    for (id, text) in [
        ("a", "message A"),
        ("b", "message B"),
        ("c", "message C"),
        ("d", "message D"),
    ] {
        queue(&mut c, &task, g, id, "FOLLOW_UP", text).await;
    }
    // The tray lists them in the order they were queued, with position and mode.
    let l = list_queue(&mut c, &task, false).await;
    assert_eq!(queued_ids(&l), ["a", "b", "c", "d"]);
    assert_eq!(l.queued, 4);
    assert!(l.run_alive);
    assert_eq!(l.items[2].position, 3);
    assert_eq!(l.items[0].mode, "FOLLOW_UP");
    // Edit B, delete D, move C before A.
    let ch: QueuedInputChanged = cmd(
        &mut c,
        "EditQueuedInput",
        EditQueuedInput {
            task_id: Some(task.clone()),
            input_id: "b".into(),
            text: "message B edited".into(),
            mode: String::new(),
            model: String::new(),
        },
        g,
    )
    .await
    .unwrap();
    assert_eq!(ch.state, "QUEUED");
    let _: QueuedInputChanged = cmd(
        &mut c,
        "RemoveQueuedInput",
        RemoveQueuedInput {
            task_id: Some(task.clone()),
            input_id: "d".into(),
            reason: String::new(),
        },
        g,
    )
    .await
    .unwrap();
    let ch: QueuedInputChanged = cmd(
        &mut c,
        "ReorderQueuedInput",
        ReorderQueuedInput {
            task_id: Some(task.clone()),
            input_id: "c".into(),
            before_input_id: "a".into(),
        },
        g,
    )
    .await
    .unwrap();
    assert_eq!(ch.position, 1);
    let l = list_queue(&mut c, &task, true).await;
    assert_eq!(queued_ids(&l), ["c", "a", "b"]);
    assert_eq!(
        l.items.iter().find(|i| i.input_id == "b").unwrap().text,
        "message B edited"
    );
    assert!(l.items.iter().find(|i| i.input_id == "b").unwrap().edited);
    assert_eq!(
        l.items.iter().find(|i| i.input_id == "d").unwrap().state,
        "REMOVED"
    );
    // Let the run drain the queue.
    wait_task(&mut c, &task, 90).await;
    let bodies = seen.lock().unwrap().clone();
    // Each later request carries one more of the inputs, in the order left.
    let want = [
        "[FOLLOW_UP] message C",
        "[FOLLOW_UP] message A",
        "[FOLLOW_UP] message B edited",
    ];
    let mut per_request: Vec<Vec<String>> = bodies.iter().map(labelled).collect();
    per_request.retain(|l| !l.is_empty());
    assert_eq!(
        per_request.iter().map(Vec::len).collect::<Vec<_>>()[..3],
        [1, 2, 3],
        "three separate turns, one more input each: {per_request:?}"
    );
    assert_eq!(per_request[2], want, "in the order the person left them");
    for b in &bodies {
        assert!(
            !user_texts(b).iter().any(|t| t.contains("message D")),
            "a deleted input is never dispatched"
        );
        assert!(
            !user_texts(b).iter().any(|t| t == "[FOLLOW_UP] message B"),
            "the edit replaced the text"
        );
    }
    // The queue now shows them settled; the log names what each dispatch took.
    let l = list_queue(&mut c, &task, true).await;
    assert!(queued_ids(&l).is_empty());
    assert_eq!(
        l.items.iter().filter(|i| i.state == "DISPATCHED").count(),
        3
    );
    let ev = replay(&core, &session).await;
    let steered = kinds(&ev, "TaskSteered");
    let dispatched: Vec<String> = steered
        .iter()
        .flat_map(|e| pl(e)["input_ids"].as_array().cloned().unwrap_or_default())
        .filter_map(|v| v.as_str().map(str::to_owned))
        .collect();
    assert_eq!(dispatched, ["c", "a", "b"]);
    // The conversation shows what the model was given: the edit, not what was
    // typed first, and nothing of the deleted input.
    let page: modbit_protocol::v1::TranscriptPage = cmd(
        &mut c,
        "GetTranscript",
        modbit_protocol::v1::GetTranscript {
            task_id: Some(task.clone()),
            density: modbit_protocol::v1::TranscriptDensity::Detailed as i32,
            after_row: 0,
            limit: 500,
            as_of_offset: 0,
        },
        None,
    )
    .await
    .unwrap();
    let rows: Vec<(String, String)> = page
        .rows
        .iter()
        .map(|r| (r.row_id.clone(), r.text.clone()))
        .collect();
    assert!(
        rows.contains(&("user:in:b".into(), "message B edited".into())),
        "{rows:?}"
    );
    assert!(rows.iter().any(|(id, _)| id == "user:in:a"));
    assert!(
        !rows
            .iter()
            .any(|(id, t)| id == "user:in:d" || t.contains("message D")),
        "a deleted input is not in the conversation: {rows:?}"
    );
}

async fn lease_again(c: &mut Client, session: &Id) -> Option<u64> {
    use modbit_protocol::v1::{AcquireSessionLease, SessionLeaseAcquired};
    let r: SessionLeaseAcquired = cmd(
        c,
        "AcquireSessionLease",
        AcquireSessionLease {
            session_id: Some(session.clone()),
            owner: "test".into(),
        },
        None,
    )
    .await
    .unwrap();
    Some(r.lease_generation)
}

/// `SIGKILL` of the Core with inputs queued: the edits and the order are the
/// log's, the queue is the same after recovery, and it drains in order. A
/// steering input that had been applied before a second kill is still in the
/// model's transcript when the run comes back.
#[tokio::test]
async fn qual_px_050_the_queue_survives_sigkill_and_drains_in_order_after_recovery() {
    let (_repo, root) = plain_repo(&[("a.txt", "alpha\n")]);
    let dir = tempfile::tempdir().unwrap();
    // Phase A: the model streams a little and hangs; the person queues three
    // inputs and manages them; the Core is killed with the stream open.
    let (base_a, _) = model(Arc::new(|_, _| Step::hang_after(3))).await;
    let mut core = spawn(dir.path(), &env_of(&base_a, &[]));
    let mut c = core.client().await;
    let (session, mut g) = session_with_lease(&mut c, 0x10).await;
    let feed = live(&core, &session).await;
    let task = create_task(&mut c, &session, g, &root, 0x11, "local_trusted", "talk").await;
    start(&mut c, &task, g, "gpt-5-mini", 8).await;
    feed.until(30, "the first delta", |e| {
        !kinds(e, "AssistantTextDelta").is_empty()
    })
    .await;
    for (id, text) in [("a", "message A"), ("b", "message B"), ("c", "message C")] {
        queue(&mut c, &task, g, id, "FOLLOW_UP", text).await;
    }
    let _: QueuedInputChanged = cmd(
        &mut c,
        "EditQueuedInput",
        EditQueuedInput {
            task_id: Some(task.clone()),
            input_id: "b".into(),
            text: "message B edited".into(),
            ..Default::default()
        },
        g,
    )
    .await
    .unwrap();
    let _: QueuedInputChanged = cmd(
        &mut c,
        "ReorderQueuedInput",
        ReorderQueuedInput {
            task_id: Some(task.clone()),
            input_id: "c".into(),
            before_input_id: "a".into(),
        },
        g,
    )
    .await
    .unwrap();
    let before = list_queue(&mut c, &task, false).await;
    assert_eq!(queued_ids(&before), ["c", "a", "b"]);
    drop(c);
    drop(feed);
    core.kill();
    drop(core);

    // Phase B: a new Core on the same profile. The queue is what it was.
    let (base_b, seen_b) = model(Arc::new(|_, n| {
        if n == 0 {
            Step::text("ok")
        } else {
            Step::hang_after(2)
        }
    }))
    .await;
    let mut core = spawn(dir.path(), &env_of(&base_b, &[]));
    let mut c = core.client().await;
    g = lease_again(&mut c, &session).await;
    let after = list_queue(&mut c, &task, false).await;
    assert_eq!(
        queued_ids(&after),
        ["c", "a", "b"],
        "same inputs, same order"
    );
    let b = after.items.iter().find(|i| i.input_id == "b").unwrap();
    assert_eq!(b.text, "message B edited", "the edit survived");
    assert!(!after.run_alive, "nothing runs until the person resumes");
    // Resume: the first boundary dispatches C; the second request hangs, and
    // the Core is killed with C applied and A and B still queued.
    start(&mut c, &task, g, "gpt-5-mini", 8).await;
    let deadline = Instant::now() + Duration::from_secs(60);
    while seen_b.lock().unwrap().len() < 2 {
        assert!(Instant::now() < deadline, "the second request never came");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    {
        let bodies = seen_b.lock().unwrap();
        assert_eq!(labelled(&bodies[0]), ["[FOLLOW_UP] message C"]);
    }
    {
        let bodies = seen_b.lock().unwrap();
        assert_eq!(
            labelled(&bodies[1]),
            ["[FOLLOW_UP] message C", "[FOLLOW_UP] message A"],
            "the second turn is A's"
        );
    }
    let mid = list_queue(&mut c, &task, true).await;
    assert_eq!(queued_ids(&mid), ["b"]);
    drop(c);
    core.kill();
    drop(core);

    // Phase C: the third Core. C and A were applied before the kill: the
    // model sees them again in the rebuilt transcript, and B drains after.
    let (base_c, seen_c) = model(Arc::new(|_, _| Step::text("ok"))).await;
    let core = spawn(dir.path(), &env_of(&base_c, &[]));
    let mut c = core.client().await;
    g = lease_again(&mut c, &session).await;
    let still = list_queue(&mut c, &task, false).await;
    assert_eq!(queued_ids(&still), ["b"]);
    start(&mut c, &task, g, "gpt-5-mini", 3).await;
    wait_task(&mut c, &task, 90).await;
    let bodies = seen_c.lock().unwrap().clone();
    let per: Vec<Vec<String>> = bodies.iter().map(labelled).collect();
    assert_eq!(
        per[0],
        [
            "[FOLLOW_UP] message C",
            "[FOLLOW_UP] message A",
            "[FOLLOW_UP] message B edited"
        ],
        "C and A survived in the transcript and B followed them: {per:?}"
    );
    let done = list_queue(&mut c, &task, true).await;
    assert!(queued_ids(&done).is_empty());
    assert_eq!(
        done.items
            .iter()
            .filter(|i| i.state == "DISPATCHED")
            .count(),
        3
    );
}

/// Send now on a live stream: the stream ends aborted with source
/// USER_INTERRUPT and its partial text kept, the model never sees the partial
/// text as a message, the sent input starts the next turn once, a replay of
/// the command interrupts once, and the next input is processed after it.
#[tokio::test]
async fn qual_px_050_send_now_cuts_a_live_stream_as_a_user_interrupt_and_starts_the_new_turn_once()
{
    let (_repo, root) = plain_repo(&[("a.txt", "alpha\n")]);
    let (base, seen) = model(Arc::new(|_, n| {
        if n == 0 {
            Step::hang_after(5)
        } else {
            Step::text("after the interrupt")
        }
    }))
    .await;
    let dir = tempfile::tempdir().unwrap();
    let core = spawn(dir.path(), &env_of(&base, &[]));
    let mut c = core.client().await;
    let (session, g) = session_with_lease(&mut c, 0x10).await;
    let feed = live(&core, &session).await;
    let task = create_task(&mut c, &session, g, &root, 0x11, "local_trusted", "talk").await;
    start(&mut c, &task, g, "gpt-5-mini", 3).await;
    feed.until(30, "partial text", |e| {
        !kinds(e, "AssistantTextDelta").is_empty()
    })
    .await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    queue(&mut c, &task, g, "x", "FOLLOW_UP", "urgent please").await;
    let asked = Instant::now();
    let cmd_id = rand_id();
    let (acc, status): (InterruptAccepted, i32) = cmd_with_id(
        &mut c,
        cmd_id.clone(),
        "SendQueuedInputNow",
        SendQueuedInputNow {
            task_id: Some(task.clone()),
            input_id: "x".into(),
            interrupt_id: "int-1".into(),
            reason: String::new(),
        },
        g,
    )
    .await
    .unwrap();
    assert_eq!((acc.kind.as_str(), acc.run_alive), ("SEND_NOW", true));
    assert_eq!(status, 1, "accepted, not replayed");
    // The next input is queued behind it and is processed after it.
    queue(&mut c, &task, g, "y", "FOLLOW_UP", "and then this").await;
    let ev = feed
        .until(30, "the interrupt applied", |e| {
            !kinds(e, "TaskInterruptApplied").is_empty()
        })
        .await;
    assert!(
        asked.elapsed() < Duration::from_secs(10),
        "the stream is cut at the next safe point, not at the end of the response"
    );
    // A replay of the same command, and of the same interrupt under another
    // command id, interrupt nothing more.
    let (_, replay_status): (InterruptAccepted, i32) = cmd_with_id(
        &mut c,
        cmd_id,
        "SendQueuedInputNow",
        SendQueuedInputNow {
            task_id: Some(task.clone()),
            input_id: "x".into(),
            interrupt_id: "int-1".into(),
            reason: String::new(),
        },
        g,
    )
    .await
    .unwrap();
    assert_eq!(replay_status, 2, "a replay");
    let again: (InterruptAccepted, i32) = cmd_with_id(
        &mut c,
        rand_id(),
        "SendQueuedInputNow",
        SendQueuedInputNow {
            task_id: Some(task.clone()),
            input_id: "x".into(),
            interrupt_id: "int-1".into(),
            reason: String::new(),
        },
        g,
    )
    .await
    .unwrap();
    assert_eq!(again.1, 2, "the same interrupt id is the same interrupt");
    wait_task(&mut c, &task, 60).await;
    let ev2 = replay(&core, &session).await;
    assert!(ev2.len() >= ev.len());
    // The stream ended as an aborted stream: a typed user interrupt, its
    // partial text kept, never completed.
    let aborted = kinds(&ev2, "AssistantMessageAborted");
    assert_eq!(aborted.len(), 1, "{aborted:?}");
    assert_eq!(pl(&aborted[0])["source"], "USER_INTERRUPT");
    assert_eq!(pl(&aborted[0])["code"], "USER_SEND_NOW");
    assert!(
        kinds(&ev2, "AssistantMessageCompleted")
            .iter()
            .all(|e| e["aggregate_id"] != aborted[0]["aggregate_id"]),
        "the cut stream never completes"
    );
    let deltas: Vec<Value> = kinds(&ev2, "AssistantTextDelta")
        .into_iter()
        .filter(|d| d["aggregate_id"] == aborted[0]["aggregate_id"])
        .collect();
    assert!(!deltas.is_empty(), "the partial text stays on the log");
    assert_eq!(pl(&aborted[0])["delta_count"], deltas.len() as u64);
    // The interrupt is on the log once, and so is what it did.
    assert_eq!(kinds(&ev2, "TaskInterruptRequested").len(), 1);
    let applied = kinds(&ev2, "TaskInterruptApplied");
    assert_eq!(applied.len(), 1);
    assert_eq!(pl(&applied[0])["outcome"], "STREAM_ABORTED");
    assert_eq!(pl(&applied[0])["stream_aborted"], true);
    assert_eq!(pl(&applied[0])["next"], "DISPATCH");
    // The new turn started once, from the sent input; the partial text was
    // never put in front of the model as a message.
    let bodies = seen.lock().unwrap().clone();
    assert_eq!(labelled(&bodies[1]), ["[SEND_NOW] urgent please"]);
    for b in &bodies[1..] {
        assert!(
            !serde_json::to_string(&b["messages"])
                .unwrap()
                .contains("partial0"),
            "a cut response is not a message"
        );
    }
    assert!(
        bodies
            .iter()
            .any(|b| labelled(b) == ["[SEND_NOW] urgent please", "[FOLLOW_UP] and then this"]),
        "the next input was processed after it: {:?}",
        bodies.iter().map(labelled).collect::<Vec<_>>()
    );
    let sent_now = bodies
        .iter()
        .filter(|b| labelled(b).iter().any(|l| l.starts_with("[SEND_NOW]")))
        .count();
    assert!(sent_now >= 2, "it stays in the conversation");
    let first_with: Vec<usize> = bodies
        .iter()
        .enumerate()
        .filter(|(_, b)| labelled(b).iter().any(|l| l == "[SEND_NOW] urgent please"))
        .map(|(i, _)| i)
        .collect();
    assert_eq!(
        first_with.first(),
        Some(&1),
        "it started the very next request"
    );
}

/// Send now while the model is running a long shell command: the broker
/// cancels the process, the call ends cancelled, the interrupt is recorded
/// with what it cancelled, and the next turn starts from the sent input.
#[tokio::test]
async fn qual_px_050_send_now_cancels_a_long_shell_command_through_the_broker() {
    let (_repo, root) = plain_repo(&[("a.txt", "alpha\n")]);
    let (base, seen) = model(Arc::new(|_, n| {
        if n == 0 {
            Step::call("shell.exec", json!({"argv": ["sleep", "997"]}))
        } else {
            Step::text("after the interrupt")
        }
    }))
    .await;
    let dir = tempfile::tempdir().unwrap();
    let core = spawn(dir.path(), &env_of(&base, &[]));
    let mut c = core.client().await;
    let (session, g) = session_with_lease(&mut c, 0x10).await;
    let feed = live(&core, &session).await;
    let task = create_task(&mut c, &session, g, &root, 0x11, "local_trusted", "wait").await;
    start(&mut c, &task, g, "gpt-5-mini", 3).await;
    // The command is really running.
    let deadline = Instant::now() + Duration::from_secs(60);
    while !process_running("sleep 997") {
        assert!(Instant::now() < deadline, "the command never started");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    queue(&mut c, &task, g, "x", "FOLLOW_UP", "never mind, do this").await;
    let asked = Instant::now();
    let _: InterruptAccepted = cmd(
        &mut c,
        "SendQueuedInputNow",
        SendQueuedInputNow {
            task_id: Some(task.clone()),
            input_id: "x".into(),
            interrupt_id: String::new(),
            reason: String::new(),
        },
        g,
    )
    .await
    .unwrap();
    let ev = feed
        .until(30, "the interrupt applied", |e| {
            !kinds(e, "TaskInterruptApplied").is_empty()
        })
        .await;
    assert!(
        asked.elapsed() < Duration::from_secs(15),
        "the command was cancelled, not waited out"
    );
    let applied = kinds(&ev, "TaskInterruptApplied");
    assert_eq!(pl(&applied[0])["outcome"], "TOOL_CANCELLED", "{applied:?}");
    assert_eq!(
        pl(&applied[0])["cancelled_calls"].as_array().unwrap().len(),
        1
    );
    assert_eq!(pl(&applied[0])["next"], "DISPATCH");
    // The process is gone: the broker killed it.
    let gone = Instant::now() + Duration::from_secs(15);
    while process_running("sleep 997") {
        assert!(Instant::now() < gone, "the process outlived the interrupt");
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    wait_task(&mut c, &task, 60).await;
    let bodies = seen.lock().unwrap().clone();
    assert_eq!(
        labelled(&bodies[1]),
        ["[SEND_NOW] never mind, do this"],
        "the new turn started from the sent input"
    );
    // The model was told the call was cancelled, not that it failed.
    let results = serde_json::to_string(&bodies[1]["messages"]).unwrap();
    assert!(results.contains("CANCELLED"), "{results}");
    // One shell call, and it is not recorded as having succeeded.
    let ev = replay(&core, &session).await;
    assert!(kinds(&ev, "ToolCallSucceeded").is_empty());
    assert_eq!(kinds(&ev, "TaskInterruptApplied").len(), 1);
}

fn process_running(needle: &str) -> bool {
    std::process::Command::new("pgrep")
        .args(["-f", needle])
        .output()
        .map(|o| !o.stdout.is_empty())
        .unwrap_or(false)
}

/// Stop: the turn ends at a safe point with a user-interrupt abort that is
/// not a runtime or transport abort, the run pauses, and the person resumes it.
#[tokio::test]
async fn qual_px_050_stop_ends_the_turn_as_a_user_interrupt_pauses_the_run_and_resume_continues() {
    let (_repo, root) = plain_repo(&[("a.txt", "alpha\n")]);
    let (base, seen) = model(Arc::new(|_, n| {
        if n == 0 {
            Step::hang_after(4)
        } else {
            Step::text("back again")
        }
    }))
    .await;
    let dir = tempfile::tempdir().unwrap();
    let core = spawn(dir.path(), &env_of(&base, &[]));
    let mut c = core.client().await;
    let (session, g) = session_with_lease(&mut c, 0x10).await;
    let feed = live(&core, &session).await;
    let task = create_task(&mut c, &session, g, &root, 0x11, "local_trusted", "talk").await;
    // Nothing is running yet: there is nothing to stop.
    assert_eq!(
        code_of(
            cmd::<InterruptAccepted>(
                &mut c,
                "InterruptTask",
                InterruptTask {
                    task_id: Some(task.clone()),
                    ..Default::default()
                },
                g
            )
            .await
        ),
        "NOT_RUNNING"
    );
    start(&mut c, &task, g, "gpt-5-mini", 3).await;
    feed.until(30, "partial text", |e| {
        !kinds(e, "AssistantTextDelta").is_empty()
    })
    .await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    let asked = Instant::now();
    let acc: InterruptAccepted = cmd(
        &mut c,
        "InterruptTask",
        InterruptTask {
            task_id: Some(task.clone()),
            interrupt_id: "stop-1".into(),
            reason: String::new(),
        },
        g,
    )
    .await
    .unwrap();
    assert_eq!(acc.kind, "STOP");
    let ev = feed
        .until(30, "the run paused", |e| !kinds(e, "TaskPaused").is_empty())
        .await;
    assert!(
        asked.elapsed() < Duration::from_secs(10),
        "stop lands within seconds"
    );
    let aborted = kinds(&ev, "AssistantMessageAborted");
    assert_eq!(aborted.len(), 1);
    assert_eq!(pl(&aborted[0])["source"], "USER_INTERRUPT");
    assert_eq!(pl(&aborted[0])["code"], "USER_STOP");
    let applied = kinds(&ev, "TaskInterruptApplied");
    assert_eq!(pl(&applied[0])["next"], "PAUSE");
    assert_eq!(pl(&applied[0])["outcome"], "STREAM_ABORTED");
    // Nothing was dispatched, and only the one request was made.
    assert!(kinds(&ev, "TaskSteered").is_empty());
    assert_eq!(seen.lock().unwrap().len(), 1);
    // The person continues: a message queued while stopped is processed.
    queue(&mut c, &task, g, "after", "FOLLOW_UP", "carry on").await;
    let _: TaskResumeResult = cmd(
        &mut c,
        "ResumeTask",
        ResumeTask {
            task_id: Some(task.clone()),
            ..Default::default()
        },
        g,
    )
    .await
    .unwrap();
    wait_task(&mut c, &task, 60).await;
    let bodies = seen.lock().unwrap().clone();
    assert!(bodies.len() >= 2);
    assert_eq!(labelled(&bodies[1]), ["[FOLLOW_UP] carry on"]);
}

/// Refusals: queue commands need the session lease; a dispatched, deleted or
/// unknown input is refused; an untrusted input is not rewritten; nothing is
/// written by a refused command.
#[tokio::test]
async fn qual_px_050_queue_commands_are_fenced_and_refuse_what_cannot_change() {
    let (_repo, root) = plain_repo(&[("a.txt", "alpha\n")]);
    let (base, _seen) = model(Arc::new(|_, n| {
        if n == 0 {
            Step::slow(25, 100)
        } else {
            Step::text("ok")
        }
    }))
    .await;
    let dir = tempfile::tempdir().unwrap();
    let core = spawn(dir.path(), &env_of(&base, &[]));
    let mut c = core.client().await;
    let (session, g) = session_with_lease(&mut c, 0x10).await;
    let feed = live(&core, &session).await;
    let task = create_task(&mut c, &session, g, &root, 0x11, "local_trusted", "talk").await;
    start(&mut c, &task, g, "gpt-5-mini", 3).await;
    feed.until(30, "the first delta", |e| {
        !kinds(e, "AssistantTextDelta").is_empty()
    })
    .await;
    queue(&mut c, &task, g, "a", "FOLLOW_UP", "message A").await;
    queue(&mut c, &task, g, "b", "FOLLOW_UP", "message B").await;
    tokio::time::sleep(Duration::from_millis(400)).await;
    let before = feed.all().len();
    // No lease, or a stale one: refused, nothing written.
    for (ty, msg) in [
        (
            "EditQueuedInput",
            EditQueuedInput {
                task_id: Some(task.clone()),
                input_id: "a".into(),
                text: "sneaky".into(),
                ..Default::default()
            }
            .encode_to_vec(),
        ),
        (
            "RemoveQueuedInput",
            RemoveQueuedInput {
                task_id: Some(task.clone()),
                input_id: "a".into(),
                reason: String::new(),
            }
            .encode_to_vec(),
        ),
        (
            "ReorderQueuedInput",
            ReorderQueuedInput {
                task_id: Some(task.clone()),
                input_id: "b".into(),
                before_input_id: "a".into(),
            }
            .encode_to_vec(),
        ),
        (
            "SendQueuedInputNow",
            SendQueuedInputNow {
                task_id: Some(task.clone()),
                input_id: "a".into(),
                ..Default::default()
            }
            .encode_to_vec(),
        ),
        (
            "InterruptTask",
            InterruptTask {
                task_id: Some(task.clone()),
                ..Default::default()
            }
            .encode_to_vec(),
        ),
    ] {
        for generation in [None, g.map(|x| x + 7)] {
            let r = c
                .command(envelope_fenced(rand_id(), ty, msg.clone(), generation))
                .await;
            let code = match r {
                Err(ClientError::Rejected { code, .. }) => code,
                other => panic!("{ty} without the lease: {other:?}"),
            };
            assert!(
                code == "LEASE_REQUIRED" || code == "STALE_LEASE",
                "{ty}: {code}"
            );
        }
    }
    tokio::time::sleep(Duration::from_millis(300)).await;
    let l = list_queue(&mut c, &task, false).await;
    assert_eq!(queued_ids(&l), ["a", "b"]);
    assert_eq!(l.items[0].text, "message A");
    assert!(
        !feed
            .all()
            .iter()
            .skip(before)
            .any(|e| e["event_type"].as_str().is_some_and(|t| matches!(
                t,
                "TaskInputEdited"
                    | "TaskInputRemoved"
                    | "TaskInputReordered"
                    | "TaskInterruptRequested"
            )))
    );
    // Unknown input, a blank edit, a bad mode, a missing target.
    let edit = |id: &str, text: &str, mode: &str| EditQueuedInput {
        task_id: Some(task.clone()),
        input_id: id.into(),
        text: text.into(),
        mode: mode.into(),
        model: String::new(),
    };
    assert_eq!(
        code_of(
            cmd::<QueuedInputChanged>(&mut c, "EditQueuedInput", edit("nope", "x", ""), g).await
        ),
        "UNKNOWN_INPUT"
    );
    assert_eq!(
        code_of(cmd::<QueuedInputChanged>(&mut c, "EditQueuedInput", edit("a", "", ""), g).await),
        "EMPTY_EDIT"
    );
    assert_eq!(
        code_of(
            cmd::<QueuedInputChanged>(&mut c, "EditQueuedInput", edit("a", "", "LOUD"), g).await
        ),
        "BAD_PAYLOAD"
    );
    assert_eq!(
        code_of(
            cmd::<QueuedInputChanged>(
                &mut c,
                "ReorderQueuedInput",
                ReorderQueuedInput {
                    task_id: Some(task.clone()),
                    input_id: "a".into(),
                    before_input_id: "ghost".into(),
                },
                g
            )
            .await
        ),
        "UNKNOWN_INPUT"
    );
    // A deleted input cannot be edited, reordered or sent.
    let _: QueuedInputChanged = cmd(
        &mut c,
        "RemoveQueuedInput",
        RemoveQueuedInput {
            task_id: Some(task.clone()),
            input_id: "a".into(),
            reason: String::new(),
        },
        g,
    )
    .await
    .unwrap();
    for (ty, bytes) in [
        ("EditQueuedInput", edit("a", "again", "").encode_to_vec()),
        (
            "ReorderQueuedInput",
            ReorderQueuedInput {
                task_id: Some(task.clone()),
                input_id: "a".into(),
                before_input_id: "b".into(),
            }
            .encode_to_vec(),
        ),
        (
            "SendQueuedInputNow",
            SendQueuedInputNow {
                task_id: Some(task.clone()),
                input_id: "a".into(),
                ..Default::default()
            }
            .encode_to_vec(),
        ),
        (
            "RemoveQueuedInput",
            RemoveQueuedInput {
                task_id: Some(task.clone()),
                input_id: "a".into(),
                reason: String::new(),
            }
            .encode_to_vec(),
        ),
    ] {
        let r = c.command(envelope_fenced(rand_id(), ty, bytes, g)).await;
        match r {
            Err(ClientError::Rejected { code, .. }) => {
                assert_eq!(code, "INPUT_NOT_QUEUED", "{ty}")
            }
            other => panic!("{ty} on a deleted input: {other:?}"),
        }
    }
    // Once the run has dispatched an input it is history: it cannot move.
    wait_task(&mut c, &task, 90).await;
    let l = list_queue(&mut c, &task, true).await;
    assert!(
        l.items
            .iter()
            .any(|i| i.input_id == "b" && i.state == "DISPATCHED")
    );
    let r = c
        .command(envelope_fenced(
            rand_id(),
            "ReorderQueuedInput",
            ReorderQueuedInput {
                task_id: Some(task.clone()),
                input_id: "b".into(),
                before_input_id: String::new(),
            }
            .encode_to_vec(),
            g,
        ))
        .await;
    match r {
        Err(ClientError::Rejected { code, .. }) => {
            // The run has ended, so the task may be over; either way the
            // dispatched input is refused or the whole task is closed.
            assert!(
                code == "INPUT_NOT_QUEUED" || code == "TASK_TERMINAL",
                "{code}"
            );
        }
        other => panic!("reordering a dispatched input: {other:?}"),
    }
}

// =====================================================================
// PX-057
// =====================================================================

/// The goal of the task a request belongs to (`Task goal: ...`).
fn goal_of(body: &Value) -> String {
    user_texts(body)
        .iter()
        .find_map(|t| {
            t.strip_prefix("Task goal: ")
                .map(|g| g.lines().next().unwrap_or_default().to_owned())
        })
        .unwrap_or_default()
}

fn has_tool_result(body: &Value) -> bool {
    body["messages"]
        .as_array()
        .is_some_and(|m| m.iter().any(|x| x["role"] == "tool"))
}

/// A model that runs the command its task's goal names, once, and then says it
/// is done. The commands are the ones the tests ask about.
fn command_model() -> Script {
    Arc::new(|body, _| {
        if has_tool_result(body) {
            return Step::text("done");
        }
        let argv: Vec<&str> = match goal_of(body).as_str() {
            "g-build" => vec!["mytool", "build"],
            "g-build-fast" => vec!["mytool", "build", "--fast"],
            "g-chain" => vec!["sh", "-c", "mytool build && mytool build --fast"],
            "g-redirect" => vec!["sh", "-c", "mytool build > out2.txt"],
            "g-pipe" => vec!["sh", "-c", "mytool build | cat"],
            "g-gitconfig" => vec!["git", "config", "user.name", "someone"],
            "g-outside" => vec!["touch", "../escaped.txt"],
            "g-curl" => vec!["curl", "http://127.0.0.1:9/"],
            // A call that declares it needs more than it has (AFW-F11).
            "g-escalate" => vec!["mytool", "build"],
            _ => return Step::text("nothing to do"),
        };
        let mut args = json!({"argv": argv, "inherit_env": true});
        if goal_of(body) == "g-escalate" {
            args["escalation"] = json!("network");
        }
        Step::call("shell.exec", args)
    })
}

struct Bin {
    dir: tempfile::TempDir,
}

/// A directory with one executable `mytool` that writes `out.txt` in its
/// working directory: a program the classifier does not know, so a call to it
/// is a protected effect that asks in the default mode.
fn tool_dir() -> Bin {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("mytool");
    std::fs::write(&path, "#!/bin/sh\necho \"ran $*\" > out.txt\n").unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    Bin { dir }
}

fn path_with(bin: &Bin) -> String {
    format!(
        "{}:{}",
        bin.dir.path().display(),
        std::env::var("PATH").unwrap_or_default()
    )
}

async fn pending_approvals(c: &mut Client, session: &Id) -> Vec<modbit_protocol::v1::ApprovalView> {
    let l: ApprovalList = cmd(
        c,
        "ListApprovals",
        ListApprovals {
            session_id: Some(session.clone()),
        },
        None,
    )
    .await
    .unwrap();
    l.approvals
        .into_iter()
        .filter(|a| a.status == "REQUESTED")
        .collect()
}

async fn receipts(c: &mut Client, task: &Id) -> Vec<modbit_protocol::v1::EffectReceiptView> {
    let l: EffectReceiptList = cmd(
        c,
        "GetEffectReceipts",
        modbit_protocol::v1::GetEffectReceipts {
            task_id: Some(task.clone()),
        },
        None,
    )
    .await
    .unwrap();
    assert!(l.chain_valid);
    l.receipts
}

async fn set_mode(
    c: &mut Client,
    task: &Id,
    g: Option<u64>,
    mode: &str,
    ack: bool,
) -> Result<RunModeView, (String, String)> {
    cmd(
        c,
        "SetRunMode",
        SetRunMode {
            task_id: Some(task.clone()),
            mode: mode.into(),
            acknowledge_risk: ack,
        },
        g,
    )
    .await
}

async fn add_rule(
    c: &mut Client,
    task: &Id,
    g: Option<u64>,
    pattern: &[&str],
    scope: &str,
    expires_at_ms: i64,
) -> Result<AllowRuleView, (String, String)> {
    cmd(
        c,
        "AddAllowRule",
        AddAllowRule {
            task_id: Some(task.clone()),
            rule_id: String::new(),
            pattern: pattern.iter().map(|s| (*s).to_owned()).collect(),
            scope: scope.into(),
            expires_at_ms,
            covers_always_ask: false,
        },
        g,
    )
    .await
}

/// A task for `goal` in the session; not yet started.
async fn task_for(
    c: &mut Client,
    session: &Id,
    g: Option<u64>,
    root: &str,
    n: u8,
    goal: &str,
) -> Id {
    create_task(c, session, g, root, n, "local_trusted", goal).await
}

/// Start `task` and wait for either an approval to appear or the run to end.
/// Returns the approvals pending for it.
async fn run_until_asked_or_done(
    c: &mut Client,
    session: &Id,
    task: &Id,
    g: Option<u64>,
) -> Vec<modbit_protocol::v1::ApprovalView> {
    start(c, task, g, "gpt-5-mini", 1).await;
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        let asked: Vec<_> = pending_approvals(c, session)
            .await
            .into_iter()
            .filter(|a| a.task_id.as_ref() == Some(task))
            .collect();
        if !asked.is_empty() {
            return asked;
        }
        let ack = c
            .command(envelope(
                rand_id(),
                "GetTaskStatus",
                modbit_protocol::v1::GetTaskStatus {
                    task_id: Some(task.clone()),
                }
                .encode_to_vec(),
            ))
            .await
            .unwrap();
        let st: modbit_protocol::v1::TaskStatus = Client::result(&ack).unwrap();
        if !st.loop_alive {
            return vec![];
        }
        assert!(Instant::now() < deadline, "neither asked nor finished");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

async fn decide(
    c: &mut Client,
    a: &modbit_protocol::v1::ApprovalView,
    approve: bool,
    g: Option<u64>,
) {
    let _: modbit_protocol::v1::ApprovalResolvedAck = cmd(
        c,
        "ResolveApproval",
        ResolveApproval {
            approval_id: a.approval_id.clone(),
            approve,
            reason: String::new(),
            intent_hash: a.intent_hash.clone(),
        },
        g,
    )
    .await
    .unwrap();
}

fn scope_of(a: &modbit_protocol::v1::ApprovalView) -> Value {
    serde_json::from_str(&a.scope_json).unwrap()
}

/// In Ask a shell write asks with its exact intent; with a rule and
/// ALLOWLIST the same command runs without asking and the receipt names the
/// rule; what adds a redirection or a pipeline to the prefix asks again; a
/// compound command is covered by its sub-commands; revoking, expiry and
/// scope each make the next run ask.
#[tokio::test]
async fn qual_px_057_ask_allowlist_rules_scope_expiry_and_revocation_against_the_real_approval_flow()
 {
    let (_repo, root) = plain_repo(&[("a.txt", "alpha\n")]);
    let (base, _seen) = model(command_model()).await;
    let bin = tool_dir();
    let dir = tempfile::tempdir().unwrap();
    let core = spawn(dir.path(), &env_of(&base, &[("PATH", &path_with(&bin))]));
    let mut c = core.client().await;
    let (session, g) = session_with_lease(&mut c, 0x10).await;
    let out = std::path::Path::new(&root).join("out.txt");

    // ---- Ask (the default): the command asks with its exact intent.
    let t1 = task_for(&mut c, &session, g, &root, 0x11, "g-build").await;
    assert_eq!(
        cmd::<RunModeView>(
            &mut c,
            "GetRunMode",
            GetRunMode {
                task_id: Some(t1.clone())
            },
            None
        )
        .await
        .unwrap()
        .mode,
        "ASK"
    );
    let asked = run_until_asked_or_done(&mut c, &session, &t1, g).await;
    assert_eq!(asked.len(), 1, "a protected effect asks in Ask");
    assert_eq!(asked[0].tool_name, "shell.exec");
    assert_eq!(asked[0].effect_class, "ProtectedWrite");
    let scope = scope_of(&asked[0]);
    assert_eq!(scope["run_mode"], "ASK");
    assert_eq!(scope["ask_reason"], "MODE_ASK");
    assert!(!out.exists(), "nothing ran before the person decided");
    decide(&mut c, &asked[0], true, g).await;
    wait_task(&mut c, &t1, 60).await;
    assert!(out.exists(), "the approved command really ran");
    let r1 = receipts(&mut c, &t1).await;
    assert!(
        r1.iter()
            .any(|r| r.status == "AUTHORIZED" && r.policy_decision.starts_with("approval:"))
    );
    std::fs::remove_file(&out).unwrap();

    // ---- A rule alone changes nothing in Ask; the mode needs the person's
    // acknowledgement; a rule is refused without the lease and when its
    // pattern is not a literal prefix.
    let t2 = task_for(&mut c, &session, g, &root, 0x12, "g-build").await;
    assert_eq!(
        code_of(add_rule(&mut c, &t2, None, &["mytool", "build"], "REPO", 0).await),
        "LEASE_REQUIRED"
    );
    assert_eq!(
        code_of(
            add_rule(
                &mut c,
                &t2,
                g.map(|x| x + 9),
                &["mytool", "build"],
                "REPO",
                0
            )
            .await
        ),
        "STALE_LEASE"
    );
    for (pattern, code) in [
        (vec!["mytool", "build", "|", "tee"], "PATTERN_SHELL_SYNTAX"),
        (vec!["mytool", "$(id)"], "PATTERN_SHELL_SYNTAX"),
        (vec!["sh", "-c"], "PATTERN_WRAPPER"),
        (vec!["git"], "PATTERN_TOO_BROAD"),
        (vec![], "EMPTY_PATTERN"),
    ] {
        assert_eq!(
            code_of(add_rule(&mut c, &t2, g, &pattern, "REPO", 0).await),
            code,
            "{pattern:?}"
        );
    }
    assert_eq!(
        code_of(add_rule(&mut c, &t2, g, &["mytool", "build"], "GALAXY", 0).await),
        "BAD_PAYLOAD"
    );
    assert_eq!(
        code_of(add_rule(&mut c, &t2, g, &["mytool", "build"], "USER", 1).await),
        "EXPIRY_IN_PAST"
    );
    let rule = add_rule(&mut c, &t2, g, &["mytool", "build"], "REPO", 0)
        .await
        .unwrap();
    assert_eq!(
        (rule.scope.as_str(), rule.state.as_str()),
        ("REPO", "ACTIVE")
    );
    assert_eq!(rule.scope_key, root);
    assert!(rule.created_by.starts_with("user:"));
    assert_eq!(
        code_of(set_mode(&mut c, &t2, g, "ALLOWLIST", false).await),
        "ACKNOWLEDGEMENT_REQUIRED"
    );
    assert_eq!(
        code_of(set_mode(&mut c, &t2, g, "SOMETIMES", true).await),
        "UNKNOWN_RUN_MODE"
    );
    let v = set_mode(&mut c, &t2, g, "ALLOWLIST", true).await.unwrap();
    assert_eq!(
        (v.mode.as_str(), v.acknowledged, v.session_only),
        ("ALLOWLIST", true, false)
    );
    assert_eq!(v.always_ask.len(), 7);
    assert_eq!(v.rules_in_force, 1);

    // ---- ALLOWLIST: the covered command runs without asking, and the
    // receipt names the rule.
    let asked = run_until_asked_or_done(&mut c, &session, &t2, g).await;
    assert!(
        asked.is_empty(),
        "a covered command does not ask: {asked:?}"
    );
    wait_task(&mut c, &t2, 60).await;
    assert!(out.exists());
    let r2 = receipts(&mut c, &t2).await;
    assert!(
        r2.iter().any(|r| r.status == "AUTHORIZED"
            && r.policy_decision == format!("allowlist:{}", rule.rule_id)),
        "the receipt names the rule: {r2:?}"
    );
    std::fs::remove_file(&out).unwrap();

    // A longer command with the prefix, and a compound whose every
    // sub-command is covered, run silently.
    for (n, goal) in [(0x13, "g-build-fast"), (0x14, "g-chain")] {
        let t = task_for(&mut c, &session, g, &root, n, goal).await;
        set_mode(&mut c, &t, g, "ALLOWLIST", true).await.unwrap();
        let asked = run_until_asked_or_done(&mut c, &session, &t, g).await;
        assert!(asked.is_empty(), "{goal}: {asked:?}");
        wait_task(&mut c, &t, 60).await;
        assert!(
            receipts(&mut c, &t)
                .await
                .iter()
                .any(|r| r.policy_decision == format!("allowlist:{}", rule.rule_id))
        );
    }

    // A redirection and a pipeline added to the matching prefix ask again.
    for (n, goal) in [(0x15, "g-redirect"), (0x16, "g-pipe")] {
        let t = task_for(&mut c, &session, g, &root, n, goal).await;
        set_mode(&mut c, &t, g, "ALLOWLIST", true).await.unwrap();
        let asked = run_until_asked_or_done(&mut c, &session, &t, g).await;
        assert_eq!(asked.len(), 1, "{goal} must ask");
        assert_eq!(
            scope_of(&asked[0])["ask_reason"],
            "NOT_IN_ALLOWLIST",
            "{goal}"
        );
        decide(&mut c, &asked[0], false, g).await;
        wait_task(&mut c, &t, 60).await;
    }
    assert!(!std::path::Path::new(&root).join("out2.txt").exists());

    // Scope: a rule for one task never matches another's command.
    let t_scoped = task_for(&mut c, &session, g, &root, 0x17, "g-build").await;
    let t_other = task_for(&mut c, &session, g, &root, 0x18, "g-build").await;
    // Revoke the repository rule first (from a live task it applies to).
    let _: AllowRuleRevoked = cmd(
        &mut c,
        "RevokeAllowRule",
        RevokeAllowRule {
            task_id: Some(t_scoped.clone()),
            rule_id: rule.rule_id.clone(),
            reason: "no longer wanted".into(),
        },
        g,
    )
    .await
    .unwrap();
    assert_eq!(
        code_of(
            cmd::<AllowRuleRevoked>(
                &mut c,
                "RevokeAllowRule",
                RevokeAllowRule {
                    task_id: Some(t_scoped.clone()),
                    rule_id: rule.rule_id.clone(),
                    reason: String::new(),
                },
                g
            )
            .await
        ),
        "RULE_NOT_ACTIVE"
    );
    assert_eq!(
        code_of(
            cmd::<AllowRuleRevoked>(
                &mut c,
                "RevokeAllowRule",
                RevokeAllowRule {
                    task_id: Some(t_scoped.clone()),
                    rule_id: "ghost".into(),
                    reason: String::new(),
                },
                g
            )
            .await
        ),
        "UNKNOWN_RULE"
    );
    // Revoked: the next run asks.
    set_mode(&mut c, &t_other, g, "ALLOWLIST", true)
        .await
        .unwrap();
    let asked = run_until_asked_or_done(&mut c, &session, &t_other, g).await;
    assert_eq!(asked.len(), 1, "a revoked rule no longer covers it");
    assert_eq!(scope_of(&asked[0])["ask_reason"], "NOT_IN_ALLOWLIST");
    decide(&mut c, &asked[0], false, g).await;
    wait_task(&mut c, &t_other, 60).await;
    let listed: AllowRuleList = cmd(
        &mut c,
        "ListAllowRules",
        ListAllowRules {
            task_id: None,
            include_inactive: true,
        },
        None,
    )
    .await
    .unwrap();
    let row = listed
        .rules
        .iter()
        .find(|r| r.rule_id == rule.rule_id)
        .unwrap();
    assert_eq!(row.state, "REVOKED");
    assert!(row.revoked_by.starts_with("user:"));
    let active: AllowRuleList = cmd(
        &mut c,
        "ListAllowRules",
        ListAllowRules {
            task_id: None,
            include_inactive: false,
        },
        None,
    )
    .await
    .unwrap();
    assert!(active.rules.is_empty());

    // Scope: a task-scoped rule applies to its own task only.
    let t_a = task_for(&mut c, &session, g, &root, 0x19, "g-build").await;
    let t_b = task_for(&mut c, &session, g, &root, 0x1a, "g-build").await;
    let task_rule = add_rule(&mut c, &t_a, g, &["mytool", "build"], "TASK", 0)
        .await
        .unwrap();
    assert_eq!(task_rule.scope_key, hex_uuid(&t_a));
    set_mode(&mut c, &t_a, g, "ALLOWLIST", true).await.unwrap();
    set_mode(&mut c, &t_b, g, "ALLOWLIST", true).await.unwrap();
    let asked = run_until_asked_or_done(&mut c, &session, &t_b, g).await;
    assert_eq!(asked.len(), 1, "a rule outside its scope never matches");
    decide(&mut c, &asked[0], false, g).await;
    wait_task(&mut c, &t_b, 60).await;
    let asked = run_until_asked_or_done(&mut c, &session, &t_a, g).await;
    assert!(asked.is_empty(), "its own task is covered");
    wait_task(&mut c, &t_a, 60).await;
    // And b cannot revoke what does not apply to it.
    let t_c = task_for(&mut c, &session, g, &root, 0x1b, "g-build").await;
    assert_eq!(
        code_of(
            cmd::<AllowRuleRevoked>(
                &mut c,
                "RevokeAllowRule",
                RevokeAllowRule {
                    task_id: Some(t_c.clone()),
                    rule_id: task_rule.rule_id.clone(),
                    reason: String::new(),
                },
                g
            )
            .await
        ),
        "RULE_OUT_OF_SCOPE"
    );

    // Expiry: a rule with a short life covers until it ends, then asks again.
    let now = now_ms();
    let expiring = add_rule(&mut c, &t_c, g, &["mytool", "build"], "USER", now + 4_000)
        .await
        .unwrap();
    assert_eq!(expiring.expires_at_ms, now + 4_000);
    let t_e1 = task_for(&mut c, &session, g, &root, 0x1c, "g-build").await;
    set_mode(&mut c, &t_e1, g, "ALLOWLIST", true).await.unwrap();
    let asked = run_until_asked_or_done(&mut c, &session, &t_e1, g).await;
    assert!(asked.is_empty(), "before expiry");
    wait_task(&mut c, &t_e1, 60).await;
    while now_ms() < now + 4_500 {
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    let t_e2 = task_for(&mut c, &session, g, &root, 0x1d, "g-build").await;
    set_mode(&mut c, &t_e2, g, "ALLOWLIST", true).await.unwrap();
    let asked = run_until_asked_or_done(&mut c, &session, &t_e2, g).await;
    assert_eq!(asked.len(), 1, "an expired rule asks again");
    decide(&mut c, &asked[0], false, g).await;
    wait_task(&mut c, &t_e2, 60).await;
    let listed: AllowRuleList = cmd(
        &mut c,
        "ListAllowRules",
        ListAllowRules {
            task_id: None,
            include_inactive: true,
        },
        None,
    )
    .await
    .unwrap();
    assert_eq!(
        listed
            .rules
            .iter()
            .find(|r| r.rule_id == expiring.rule_id)
            .unwrap()
            .state,
        "EXPIRED"
    );
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64
}

fn hex_uuid(id: &Id) -> String {
    let h = hex::encode(&id.value);
    format!(
        "{}-{}-{}-{}-{}",
        &h[..8],
        &h[8..12],
        &h[12..16],
        &h[16..20],
        &h[20..]
    )
}

/// The widest preset never lifts the always-ask classes; it approves the
/// in-envelope effects that are not in one; it dies with the Core process
/// while the durable modes and rules do not.
#[tokio::test]
async fn qual_px_057_always_ask_classes_ask_under_run_everything_and_run_everything_dies_with_the_process()
 {
    let (repo, root) = plain_repo(&[("a.txt", "alpha\n")]);
    let (base, _seen) = model(command_model()).await;
    let bin = tool_dir();
    let dir = tempfile::tempdir().unwrap();
    let env = env_of(&base, &[("PATH", &path_with(&bin))]);
    let mut core = spawn(dir.path(), &env);
    let mut c = core.client().await;
    let (session, mut g) = session_with_lease(&mut c, 0x10).await;
    let out = std::path::Path::new(&root).join("out.txt");

    // A durable mode and a durable rule, to outlive the restart.
    let t_keep = task_for(&mut c, &session, g, &root, 0x11, "g-build").await;
    let keep_rule = add_rule(&mut c, &t_keep, g, &["mytool", "keep"], "REPO", 0)
        .await
        .unwrap();
    set_mode(&mut c, &t_keep, g, "ALLOWLIST_SANDBOX", true)
        .await
        .unwrap();

    // Run everything: the acknowledgement is required every time.
    let t = task_for(&mut c, &session, g, &root, 0x12, "g-build").await;
    assert_eq!(
        code_of(set_mode(&mut c, &t, g, "RUN_EVERYTHING", false).await),
        "ACKNOWLEDGEMENT_REQUIRED"
    );
    let v = set_mode(&mut c, &t, g, "RUN_EVERYTHING", true)
        .await
        .unwrap();
    assert_eq!(
        (v.mode.as_str(), v.session_only, v.acknowledged),
        ("RUN_EVERYTHING", true, true)
    );
    assert!(v.warning.contains("prompt injection"));
    assert_eq!(
        code_of(set_mode(&mut c, &t, g, "RUN_EVERYTHING", false).await),
        "ACKNOWLEDGEMENT_REQUIRED",
        "again, even in the mode it is already in"
    );
    // An in-envelope effect outside the always-ask classes runs without asking.
    let asked = run_until_asked_or_done(&mut c, &session, &t, g).await;
    assert!(asked.is_empty());
    wait_task(&mut c, &t, 60).await;
    assert!(out.exists());
    let rr = receipts(&mut c, &t).await;
    assert!(
        rr.iter()
            .any(|r| r.status == "AUTHORIZED" && r.policy_decision == "run_mode:RUN_EVERYTHING"),
        "{:?}",
        rr.iter()
            .map(|r| (r.status.clone(), r.policy_decision.clone()))
            .collect::<Vec<_>>()
    );

    // The always-ask classes ask, each with its typed reason, and nothing
    // runs until the person decides (here: denies).
    for (n, goal, class) in [
        (0x13u8, "g-gitconfig", "PROTECTED_PATH"),
        (0x14, "g-outside", "OUTSIDE_WORKSPACE_WRITE"),
        (0x15, "g-curl", "NETWORK"),
        (0x17, "g-escalate", "ESCALATION"),
    ] {
        let t = task_for(&mut c, &session, g, &root, n, goal).await;
        set_mode(&mut c, &t, g, "RUN_EVERYTHING", true)
            .await
            .unwrap();
        let asked = run_until_asked_or_done(&mut c, &session, &t, g).await;
        assert_eq!(asked.len(), 1, "{goal} must ask under Run everything");
        let scope = scope_of(&asked[0]);
        assert_eq!(scope["run_mode"], "RUN_EVERYTHING", "{goal}");
        assert_eq!(scope["ask_reason"], format!("ALWAYS_ASK:{class}"), "{goal}");
        assert_eq!(scope["ask_classes"][0], class, "{goal}");
        decide(&mut c, &asked[0], false, g).await;
        wait_task(&mut c, &t, 60).await;
    }
    assert!(
        !repo.path().parent().unwrap().join("escaped.txt").exists(),
        "the outside write never happened"
    );
    let cfg = std::process::Command::new("git")
        .arg("-C")
        .arg(&root)
        .args(["config", "--get", "user.name"])
        .output()
        .unwrap();
    assert_ne!(String::from_utf8_lossy(&cfg.stdout).trim(), "someone");

    // A person who approves one of them gets exactly one run of it, and it
    // asks again the next time: Run everything never became the approval.
    let t = task_for(&mut c, &session, g, &root, 0x16, "g-gitconfig").await;
    set_mode(&mut c, &t, g, "RUN_EVERYTHING", true)
        .await
        .unwrap();
    let asked = run_until_asked_or_done(&mut c, &session, &t, g).await;
    assert_eq!(asked.len(), 1);
    decide(&mut c, &asked[0], true, g).await;
    wait_task(&mut c, &t, 60).await;
    let cfg = std::process::Command::new("git")
        .arg("-C")
        .arg(&root)
        .args(["config", "--get", "user.name"])
        .output()
        .unwrap();
    assert_eq!(String::from_utf8_lossy(&cfg.stdout).trim(), "someone");

    // SIGKILL the Core. Run everything is gone with the process; the durable
    // mode and the durable rule are not.
    let t_run = t;
    drop(c);
    core.kill();
    drop(core);
    let core = spawn(dir.path(), &env);
    let mut c = core.client().await;
    g = lease_again(&mut c, &session).await;
    let after = cmd::<RunModeView>(
        &mut c,
        "GetRunMode",
        GetRunMode {
            task_id: Some(t_run.clone()),
        },
        None,
    )
    .await
    .unwrap();
    assert_eq!(
        after.mode, "ASK",
        "Run everything does not survive a restart"
    );
    let kept = cmd::<RunModeView>(
        &mut c,
        "GetRunMode",
        GetRunMode {
            task_id: Some(t_keep.clone()),
        },
        None,
    )
    .await
    .unwrap();
    assert_eq!(kept.mode, "ALLOWLIST_SANDBOX", "a durable mode does");
    assert_eq!(kept.rules_in_force, 1);
    let rules: AllowRuleList = cmd(
        &mut c,
        "ListAllowRules",
        ListAllowRules {
            task_id: None,
            include_inactive: false,
        },
        None,
    )
    .await
    .unwrap();
    assert!(
        rules
            .rules
            .iter()
            .any(|r| r.rule_id == keep_rule.rule_id && r.state == "ACTIVE")
    );
    // And moving back down needs no acknowledgement.
    let v = set_mode(&mut c, &t_keep, g, "ASK", false).await.unwrap();
    assert_eq!(v.mode, "ASK");
}

// =====================================================================
// PX-059
// =====================================================================

async fn accounting(c: &mut Client, task: &Id) -> ContextAccountingView {
    cmd(
        c,
        "GetContextAccounting",
        GetContextAccounting {
            task_id: Some(task.clone()),
        },
        None,
    )
    .await
    .unwrap()
}

fn category<'a>(
    v: &'a ContextAccountingView,
    name: &str,
) -> &'a modbit_protocol::v1::ContextCategoryView {
    v.categories.iter().find(|c| c.category == name).unwrap()
}

/// A model that reads a file on its first `reads` requests and then answers,
/// reporting a prompt size of its own for each request (so the provider's
/// count is never the estimator's).
fn reading_model(reads: usize) -> Script {
    Arc::new(move |_, n| {
        let usage = Some((2_000 + 777 * n as u64, 40));
        let mut step = if n < reads {
            Step::call("fs.read", json!({"path": "a.txt"}))
        } else {
            Step::text("that is all")
        };
        step.usage = usage;
        step
    })
}

/// A real multi-turn run: the nine categories sum to what the provider
/// reported for the last request, apportioned by the stated rule; the window
/// is the model's own and the percent follows from it; the inspector carries
/// the same breakdown; a client-supplied breakdown has no field to arrive in.
#[tokio::test]
async fn qual_px_059_the_categories_sum_to_the_provider_reported_total_and_the_window_is_the_models()
 {
    let (_repo, root) = plain_repo(&[("a.txt", "alpha\nbeta\ngamma\n")]);
    let (base, seen) = model(reading_model(3)).await;
    let dir = tempfile::tempdir().unwrap();
    let spec = "px-wide=0.1/0.2;ctx=64000;out=2048,px-narrow=0.1/0.2;ctx=16000;out=2048";
    let core = spawn(
        dir.path(),
        &env_of(&base, &[("MODBIT_OPENAI_MODELS", spec)]),
    );
    let mut c = core.client().await;
    let (session, g) = session_with_lease(&mut c, 0x10).await;

    // Before anything is compiled there is nothing to account for.
    let t0 = create_task(&mut c, &session, g, &root, 0x11, "local_trusted", "look").await;
    let none = accounting(&mut c, &t0).await;
    assert!(!none.available);
    assert_eq!(none.rounding_rule, "LARGEST_REMAINDER");
    assert!(none.categories.is_empty());

    start(&mut c, &t0, g, "px-wide", 3).await;
    wait_task(&mut c, &t0, 90).await;
    let bodies = seen.lock().unwrap().clone();
    let turns = bodies.len();
    assert!(turns >= 4, "a multi-turn run: {turns}");
    // The provider reported 2000 + 777 n for request n; the last one is what
    // the breakdown accounts for.
    let reported = 2_000 + 777 * (turns as u64 - 1);
    let v = accounting(&mut c, &t0).await;
    assert!(v.available);
    assert_eq!(v.total_source, "PROVIDER_REPORTED");
    assert_eq!(v.provider_reported_input, reported);
    assert_eq!(v.total_tokens, reported);
    assert_eq!(v.categories.len(), 9);
    assert_eq!(
        v.categories
            .iter()
            .map(|x| x.category.as_str())
            .collect::<Vec<_>>(),
        [
            "SYSTEM",
            "TOOLS",
            "RULES",
            "SKILLS",
            "MCP",
            "MEMORY",
            "SUMMARY",
            "SUBAGENTS",
            "CONVERSATION"
        ]
    );
    // The invariant: the categories sum exactly to the total.
    assert_eq!(
        v.categories.iter().map(|x| x.tokens).sum::<u64>(),
        v.total_tokens
    );
    // Each part is within one token of its exact share of the total.
    let est_sum: u64 = v.categories.iter().map(|x| x.estimated_tokens).sum();
    assert_eq!(est_sum, v.estimated_total);
    for cat in &v.categories {
        let exact = cat.estimated_tokens as f64 * v.total_tokens as f64 / est_sum as f64;
        assert!(
            (cat.tokens as f64 - exact).abs() < 1.0,
            "{}: {} vs {exact}",
            cat.category,
            cat.tokens
        );
    }
    // The estimator's own count is reported beside the provider's, with the
    // error it observed and the bound it declares.
    assert_eq!(
        v.estimator_error_bp as u64,
        v.estimated_total.abs_diff(reported) * 10_000 / reported
    );
    assert_eq!(v.declared_error_bp, 2500);
    // What the request is made of is non-zero where the request has it.
    assert!(category(&v, "SYSTEM").tokens > 0);
    assert!(category(&v, "TOOLS").tokens > 0);
    assert!(category(&v, "CONVERSATION").tokens > 0);
    assert!(!category(&v, "TOOLS").sources.is_empty(), "the tool names");
    assert!(category(&v, "TOOLS").sources.iter().any(|n| n == "fs.read"));
    assert!(
        category(&v, "SUBAGENTS").tokens > 0,
        "agent.* definitions and the delegation rule"
    );
    assert!(
        category(&v, "SUBAGENTS")
            .sources
            .iter()
            .any(|n| n == "agent.spawn")
    );
    assert_eq!(category(&v, "MEMORY").tokens, 0, "no memory was injected");
    assert!(category(&v, "SYSTEM").share_bp > 0);
    assert!(
        v.categories
            .iter()
            .map(|x| u64::from(x.share_bp))
            .sum::<u64>()
            .abs_diff(10_000)
            < 10,
        "the shares add up to the whole"
    );
    // The model-aware window and the percent.
    assert_eq!(
        (v.window_tokens, v.window_source.as_str()),
        (64_000, "ENDPOINT")
    );
    assert_eq!(v.used_bp as u64, reported * 10_000 / 64_000);
    assert_eq!(
        (v.endpoint.as_str(), v.model.as_str()),
        ("openai", "px-wide")
    );
    assert_eq!(v.turn_ordinal as usize, turns);
    // The same figures are in the inspector.
    let ack = c
        .command(envelope(
            rand_id(),
            "GetContextInspector",
            modbit_protocol::v1::GetContextInspector {
                task_id: Some(t0.clone()),
            }
            .encode_to_vec(),
        ))
        .await
        .unwrap();
    let ins: modbit_protocol::v1::ContextInspectorView = Client::result(&ack).unwrap();
    let a = ins.accounting.expect("the inspector carries the breakdown");
    assert_eq!(a.total_tokens, v.total_tokens);
    assert_eq!(
        a.categories.iter().map(|x| x.tokens).collect::<Vec<_>>(),
        v.categories.iter().map(|x| x.tokens).collect::<Vec<_>>()
    );
    // The model is told how much of its window it has used, from the provider's
    // own count of the request before: the last request carries it.
    let last = bodies.last().unwrap();
    let state = user_texts(last).into_iter().last().unwrap();
    assert!(state.contains("\"window_tokens\":64000"), "{state}");
    assert!(
        state.contains(&format!("\"last_request_tokens\":{}", reported - 777)),
        "{state}"
    );
    // The budgets of PX-116 are reported beside it, from the same log.
    let b = v.budgets.unwrap();
    assert_eq!(
        (b.max_children, b.live_children, b.forbid_spawn),
        (3, 0, false)
    );

    // Another model, another window: the same run shape on the narrow one
    // changes the window and the percent for a similar count.
    let t1 = create_task(&mut c, &session, g, &root, 0x12, "local_trusted", "look").await;
    start(&mut c, &t1, g, "px-narrow", 3).await;
    wait_task(&mut c, &t1, 90).await;
    let n = accounting(&mut c, &t1).await;
    assert_eq!((n.window_tokens, n.model.as_str()), (16_000, "px-narrow"));
    assert_eq!(
        n.used_bp as u64,
        n.total_tokens * 10_000 / 16_000,
        "the percent follows the model's window, not a constant"
    );
    assert!(n.used_bp > v.used_bp / 2, "{} vs {}", n.used_bp, v.used_bp);
    assert_eq!(
        n.categories.iter().map(|x| x.tokens).sum::<u64>(),
        n.total_tokens
    );
}

/// A provider that reports no count leaves the estimator's: the total says it
/// is an estimate, and the categories still sum to it.
#[tokio::test]
async fn qual_px_059_without_a_provider_count_the_total_is_the_estimate_and_says_so() {
    let (_repo, root) = plain_repo(&[("a.txt", "alpha\n")]);
    let (base, _seen) = model(Arc::new(|_, _| {
        let mut s = Step::text("hello");
        s.usage = Some((0, 5));
        s
    }))
    .await;
    let dir = tempfile::tempdir().unwrap();
    let core = spawn(dir.path(), &env_of(&base, &[]));
    let mut c = core.client().await;
    let (session, g) = session_with_lease(&mut c, 0x10).await;
    let t = create_task(
        &mut c,
        &session,
        g,
        &root,
        0x11,
        "local_trusted",
        "say hello",
    )
    .await;
    start(&mut c, &t, g, "gpt-5-mini", 1).await;
    wait_task(&mut c, &t, 60).await;
    let v = accounting(&mut c, &t).await;
    assert!(v.available);
    assert_eq!(v.total_source, "ESTIMATED");
    assert_eq!(v.provider_reported_input, 0);
    assert_eq!(v.total_tokens, v.estimated_total);
    assert_eq!(
        v.categories.iter().map(|x| x.tokens).sum::<u64>(),
        v.total_tokens
    );
    for cat in &v.categories {
        assert_eq!(cat.tokens, cat.estimated_tokens, "{}", cat.category);
    }
    assert_eq!(v.estimator_error_bp, 0, "no count to compare with");
    // The window is the model's own, from its endpoint's catalog.
    assert_eq!(v.window_source, "ENDPOINT");
    assert!(v.window_tokens > 0);
    assert_eq!(
        u64::from(v.used_bp),
        v.total_tokens * 10_000 / v.window_tokens
    );
}

/// Rules and memory the request carries are counted in their own categories,
/// with what they are made of named.
#[tokio::test]
async fn qual_px_059_rules_and_memory_the_request_carries_are_counted_in_their_own_categories() {
    use modbit_protocol::v1::{MemoryPromoted, MemoryProposed, PromoteMemory, ProposeMemory};
    let agents = "# Rules for this repository\n\nAlways run the formatter before you finish. Never edit generated files. Prefer small functions and explain each public item in one sentence. Keep commit messages short.\n";
    let (_plain, plain_root) = plain_repo(&[("a.txt", "alpha\n")]);
    let (_ruled, ruled_root) = plain_repo(&[("a.txt", "alpha\n"), ("AGENTS.md", agents)]);
    let (base, _seen) = model(Arc::new(|_, _| Step::text("noted"))).await;
    let dir = tempfile::tempdir().unwrap();
    let core = spawn(dir.path(), &env_of(&base, &[]));
    let mut c = core.client().await;
    let (session, g) = session_with_lease(&mut c, 0x10).await;
    // The ruled repository is trusted, so its rules load.
    let _: modbit_protocol::v1::RepositoryTrusted = cmd(
        &mut c,
        "TrustRepository",
        modbit_protocol::v1::TrustRepository {
            session_id: Some(session.clone()),
            workspace_root: ruled_root.clone(),
            scope: "repository".into(),
        },
        g,
    )
    .await
    .unwrap();
    let plain_task = create_task(
        &mut c,
        &session,
        g,
        &plain_root,
        0x11,
        "local_trusted",
        "hi",
    )
    .await;
    let ruled_task = create_task(
        &mut c,
        &session,
        g,
        &ruled_root,
        0x12,
        "local_trusted",
        "hi",
    )
    .await;
    // A curated memory for the user.
    let p: MemoryProposed = cmd(
        &mut c,
        "ProposeMemory",
        ProposeMemory {
            task_id: Some(ruled_task.clone()),
            scope: "repository".into(),
            record_type: "convention".into(),
            topic: "indentation".into(),
            content: "Use four spaces for indentation in every language in this project.".into(),
            ..Default::default()
        },
        g,
    )
    .await
    .unwrap();
    let r: MemoryPromoted = cmd(
        &mut c,
        "PromoteMemory",
        PromoteMemory {
            task_id: Some(ruled_task.clone()),
            memory_id: p.memory_id.clone(),
        },
        g,
    )
    .await
    .unwrap();
    assert_eq!(r.outcome, "curated", "{r:?}");
    for t in [&plain_task, &ruled_task] {
        start(&mut c, t, g, "gpt-5-mini", 1).await;
        wait_task(&mut c, t, 60).await;
    }
    let plain = accounting(&mut c, &plain_task).await;
    let ruled = accounting(&mut c, &ruled_task).await;
    assert!(
        category(&ruled, "RULES").estimated_tokens > category(&plain, "RULES").estimated_tokens,
        "AGENTS.md is in the rules category: {} vs {}",
        category(&ruled, "RULES").estimated_tokens,
        category(&plain, "RULES").estimated_tokens
    );
    assert!(
        !category(&ruled, "RULES").sources.is_empty(),
        "the layers that are in force are named"
    );
    assert_eq!(
        ruled.instruction_layers_in_force as usize,
        category(&ruled, "RULES").sources.len()
    );
    assert_eq!(category(&plain, "MEMORY").tokens, 0);
    assert!(
        category(&ruled, "MEMORY").tokens > 0,
        "the promoted memory is injected and counted"
    );
    assert!(category(&ruled, "MEMORY").sources.contains(&p.memory_id));
    assert_eq!(ruled.memory_items_injected, 1);
    for v in [&plain, &ruled] {
        assert_eq!(
            v.categories.iter().map(|x| x.tokens).sum::<u64>(),
            v.total_tokens
        );
    }
}

// ---- the send behavior (AFW-E05) ----

use modbit_protocol::v1::{GetSendBehavior, SendBehaviorView, SetSendBehavior};

async fn set_behavior(
    c: &mut Client,
    task: &Id,
    g: Option<u64>,
    while_running: &str,
    send_now: &str,
) -> Result<SendBehaviorView, (String, String)> {
    cmd(
        c,
        "SetSendBehavior",
        SetSendBehavior {
            task_id: Some(task.clone()),
            while_running: while_running.into(),
            send_now: send_now.into(),
        },
        g,
    )
    .await
}

/// The Core applies the setting: plain Enter (`DEFAULT`) while a turn runs
/// queues, collects, steers or stops-and-sends as the task's setting says,
/// and Send now interrupts or steers as the second setting says. A client
/// only chooses; the labels state the real consequence.
#[tokio::test]
async fn qual_px_050_the_core_applies_the_send_behavior_a_client_only_chooses() {
    let (_repo, root) = plain_repo(&[("a.txt", "alpha\n")]);
    let (base, seen) = model(Arc::new(|_, n| {
        if n == 0 {
            Step::hang_after(3)
        } else {
            Step::text("ok")
        }
    }))
    .await;
    let dir = tempfile::tempdir().unwrap();
    let core = spawn(dir.path(), &env_of(&base, &[]));
    let mut c = core.client().await;
    let (session, g) = session_with_lease(&mut c, 0x10).await;
    let feed = live(&core, &session).await;
    let task = create_task(&mut c, &session, g, &root, 0x11, "local_trusted", "talk").await;
    // Defaults: queue, and Send now interrupts. The consequence is the Core's.
    let v: SendBehaviorView = cmd(
        &mut c,
        "GetSendBehavior",
        GetSendBehavior {
            task_id: Some(task.clone()),
        },
        None,
    )
    .await
    .unwrap();
    assert_eq!(
        (v.while_running.as_str(), v.send_now.as_str()),
        ("QUEUE", "INTERRUPT")
    );
    assert!(v.while_running_consequence.contains("not stopped"));
    assert_eq!(v.while_running_values.len(), 4);
    // A bad value, an empty command and a missing lease change nothing.
    assert_eq!(
        code_of(set_behavior(&mut c, &task, g, "LOUDLY", "").await),
        "BAD_PAYLOAD"
    );
    assert_eq!(
        code_of(set_behavior(&mut c, &task, g, "", "").await),
        "EMPTY_EDIT"
    );
    assert_eq!(
        code_of(set_behavior(&mut c, &task, None, "STEER", "").await),
        "LEASE_REQUIRED"
    );
    start(&mut c, &task, g, "gpt-5-mini", 3).await;
    feed.until(30, "partial text", |e| {
        !kinds(e, "AssistantTextDelta").is_empty()
    })
    .await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    // QUEUE: a DEFAULT input waits as a follow-up.
    queue(&mut c, &task, g, "q1", "DEFAULT", "default one").await;
    let l = list_queue(&mut c, &task, false).await;
    assert_eq!(l.items[0].mode, "FOLLOW_UP");
    assert!(l.run_alive);
    // COLLECT.
    set_behavior(&mut c, &task, g, "COLLECT", "").await.unwrap();
    queue(&mut c, &task, g, "q2", "DEFAULT", "default two").await;
    let l = list_queue(&mut c, &task, false).await;
    assert_eq!(l.items[1].mode, "COLLECT");
    // The explicit modes are unchanged by the setting.
    queue(&mut c, &task, g, "q3", "FOLLOW_UP", "explicit").await;
    assert_eq!(
        list_queue(&mut c, &task, false).await.items[2].mode,
        "FOLLOW_UP"
    );
    for id in ["q1", "q2", "q3"] {
        let _: QueuedInputChanged = cmd(
            &mut c,
            "RemoveQueuedInput",
            RemoveQueuedInput {
                task_id: Some(task.clone()),
                input_id: id.into(),
                reason: String::new(),
            },
            g,
        )
        .await
        .unwrap();
    }
    // STOP_AND_SEND: the very same plain Enter cuts the stream as a typed
    // user interrupt and the input starts the next turn.
    let v = set_behavior(&mut c, &task, g, "STOP_AND_SEND", "STEER")
        .await
        .unwrap();
    assert_eq!(
        (v.while_running.as_str(), v.send_now.as_str()),
        ("STOP_AND_SEND", "STEER")
    );
    assert!(v.while_running_consequence.contains("stopped at once"));
    assert!(v.send_now_consequence.contains("safe point"));
    queue(&mut c, &task, g, "go", "DEFAULT", "stop and send this").await;
    let ev = feed
        .until(30, "the interrupt applied", |e| {
            !kinds(e, "TaskInterruptApplied").is_empty()
        })
        .await;
    let aborted = kinds(&ev, "AssistantMessageAborted");
    assert_eq!(aborted.len(), 1);
    assert_eq!(pl(&aborted[0])["source"], "USER_INTERRUPT");
    assert_eq!(pl(&aborted[0])["code"], "USER_SEND_NOW");
    wait_task(&mut c, &task, 60).await;
    let bodies = seen.lock().unwrap().clone();
    assert_eq!(labelled(&bodies[1]), ["[SEND_NOW] stop and send this"]);
}

/// Send now with the setting `STEER` does not interrupt: the input becomes a
/// STEER, which replaces the response at the next safe boundary.
#[tokio::test]
async fn qual_px_050_send_now_set_to_steer_records_a_steer_and_no_interrupt() {
    let (_repo, root) = plain_repo(&[("a.txt", "alpha\n")]);
    let (base, seen) = model(Arc::new(|_, n| {
        if n == 0 {
            Step::hang_after(3)
        } else {
            Step::text("ok")
        }
    }))
    .await;
    let dir = tempfile::tempdir().unwrap();
    let core = spawn(dir.path(), &env_of(&base, &[]));
    let mut c = core.client().await;
    let (session, g) = session_with_lease(&mut c, 0x10).await;
    let feed = live(&core, &session).await;
    let task = create_task(&mut c, &session, g, &root, 0x11, "local_trusted", "talk").await;
    set_behavior(&mut c, &task, g, "", "STEER").await.unwrap();
    start(&mut c, &task, g, "gpt-5-mini", 3).await;
    feed.until(30, "partial text", |e| {
        !kinds(e, "AssistantTextDelta").is_empty()
    })
    .await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    queue(&mut c, &task, g, "x", "FOLLOW_UP", "redirect to this").await;
    let acc: InterruptAccepted = cmd(
        &mut c,
        "SendQueuedInputNow",
        SendQueuedInputNow {
            task_id: Some(task.clone()),
            input_id: "x".into(),
            ..Default::default()
        },
        g,
    )
    .await
    .unwrap();
    assert_eq!(acc.kind, "STEER");
    wait_task(&mut c, &task, 60).await;
    let ev = replay(&core, &session).await;
    assert!(
        kinds(&ev, "TaskInterruptRequested").is_empty(),
        "no interrupt was recorded"
    );
    let aborted = kinds(&ev, "AssistantMessageAborted");
    assert_eq!(aborted.len(), 1);
    assert_eq!(pl(&aborted[0])["code"], "STEER_INTERRUPT");
    let bodies = seen.lock().unwrap().clone();
    assert_eq!(labelled(&bodies[1]), ["[STEER] redirect to this"]);
}

/// Every ContextCompile record of a task, in order: the estimator's count of
/// each category for each compiled request (read back from the stored step
/// outputs, which is where the Core keeps them).
async fn compile_records(c: &mut Client, events: &[Value]) -> Vec<Value> {
    use modbit_protocol::v1::{ObjectRangeChunk, ReadObjectRange};
    let mut out = Vec::new();
    for e in events.iter().filter(|e| e["event_type"] == "StepSucceeded") {
        let Some(hash) = pl(e)["output_ref"].as_str() else {
            continue;
        };
        let chunk: ObjectRangeChunk = cmd(
            c,
            "ReadObjectRange",
            ReadObjectRange {
                object_hash: hash.to_owned(),
                offset: 0,
                length: 1024 * 1024,
            },
            None,
        )
        .await
        .unwrap();
        if let Ok(v) = serde_json::from_slice::<Value>(&chunk.data)
            && v.get("accounting").is_some()
        {
            out.push(v["accounting"].clone());
        }
    }
    out
}

/// After a compaction epoch the summary category grows beyond the placeholder
/// and the conversation category falls: the breakdown follows what was
/// actually compiled, turn by turn.
#[tokio::test]
async fn qual_px_059_after_a_compaction_epoch_the_summary_grows_and_the_conversation_falls() {
    let big: String = (0..400)
        .map(|i| format!("line {i}: the quick brown fox jumps over the lazy dog {i}\n"))
        .collect();
    let (_repo, root) = plain_repo(&[("a.txt", big.as_str())]);
    let (base, _seen) = model(Arc::new(|_, n| {
        if n < 9 {
            Step::call("fs.read", json!({"path": "a.txt"}))
        } else {
            Step::text("done reading")
        }
    }))
    .await;
    let dir = tempfile::tempdir().unwrap();
    let core = spawn(
        dir.path(),
        &env_of(&base, &[("MODBIT_COMPACTION_TOKEN_BUDGET", "3000")]),
    );
    let mut c = core.client().await;
    let (session, g) = session_with_lease(&mut c, 0x10).await;
    let task = create_task(&mut c, &session, g, &root, 0x11, "local_trusted", "read").await;
    start(&mut c, &task, g, "gpt-5-mini", 3).await;
    wait_task(&mut c, &task, 180).await;
    let ev = replay(&core, &session).await;
    assert!(
        !kinds(&ev, "ContextEpochOpened").is_empty(),
        "the run compacted"
    );
    let records = compile_records(&mut c, &ev).await;
    assert!(records.len() >= 6, "{}", records.len());
    let est = |r: &Value, k: &str| r["estimates"][k].as_u64().unwrap();
    let first = &records[0];
    let last = records.last().unwrap();
    // The conversation grew while the transcript did; the epoch cut it.
    let peak = records
        .iter()
        .map(|r| est(r, "CONVERSATION"))
        .max()
        .unwrap();
    assert!(
        est(last, "CONVERSATION") < peak,
        "the conversation falls after the epoch: peak {peak}, last {}",
        est(last, "CONVERSATION")
    );
    assert!(
        est(last, "SUMMARY") > est(first, "SUMMARY"),
        "the summarised conversation is counted: {} vs {}",
        est(last, "SUMMARY"),
        est(first, "SUMMARY")
    );
    // And the view the Core serves says so too.
    let v = accounting(&mut c, &task).await;
    assert!(v.compaction_epoch >= 1);
    assert_eq!(
        v.categories.iter().map(|x| x.tokens).sum::<u64>(),
        v.total_tokens
    );
    assert!(category(&v, "SUMMARY").estimated_tokens > 0);
    assert!(
        !category(&v, "SUMMARY").sources.is_empty(),
        "the epochs are named"
    );
}
