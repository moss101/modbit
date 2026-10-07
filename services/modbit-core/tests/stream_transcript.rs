//! PX-041 / PX-042 real-boundary tests: the actual `modbit-core` binary over
//! its real socket and real SQLite log, with a scripted OpenAI-compatible
//! HTTP server as the model. Assistant text streams as events while the model
//! speaks, closes with a completion or an abort record, replays by cursor,
//! survives a Core kill as an aborted stream, and the transcript projection
//! rebuilds the conversation from the log alone.

use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use modbit_protocol::client::{Client, ClientError};
use modbit_protocol::local::{ReadyLine, decode_hex};
use modbit_protocol::v1::{
    CancelTask, ClientKind, CommandEnvelope, CreateSession, CreateTask, GetSessionSnapshot,
    GetTaskStatus, Id, ReadObjectRange, SessionCreated, SessionSnapshot, StartTask, TaskCreated,
    TaskRunStarted, TaskStatus,
};
use prost::Message;
use sha2::Digest;

// ---------------------------------------------------------------- the Core

struct CoreProcess {
    child: Child,
    ready: ReadyLine,
}

impl CoreProcess {
    fn spawn_with_env(data_dir: &std::path::Path, env: &[(&str, &str)]) -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_modbit-core"))
            .arg("--data-dir")
            .arg(data_dir)
            .envs(env.iter().map(|(k, v)| (k.to_string(), v.to_string())))
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap();
        let stdout = child.stdout.take().unwrap();
        let mut lines = BufReader::new(stdout).lines();
        let ready = loop {
            let line = lines
                .next()
                .expect("core exited before ready line")
                .unwrap();
            if let Some(r) = ReadyLine::parse(&line) {
                break r;
            }
        };
        std::thread::spawn(move || for _ in lines {});
        CoreProcess { child, ready }
    }

    fn pid(&self) -> u32 {
        self.child.id()
    }

    async fn client(&self) -> Client {
        Client::connect(
            &self.ready.endpoint,
            &decode_hex(&self.ready.boot_secret_hex).unwrap(),
            ClientKind::Cli,
            "test",
        )
        .await
        .unwrap()
    }

    /// SIGKILL: the process gets no chance to close anything.
    fn kill(&mut self) {
        self.child.kill().unwrap();
        self.child.wait().unwrap();
    }
}

impl Drop for CoreProcess {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn id16(b: u8) -> Id {
    Id { value: vec![b; 16] }
}

fn fresh_id() -> Id {
    Id {
        value: (0..16).map(|_| rand::random::<u8>()).collect(),
    }
}

fn envelope(command_id: Id, command_type: &str, payload: Vec<u8>) -> CommandEnvelope {
    CommandEnvelope {
        command_id: Some(command_id),
        tenant_id: Some(id16(0xA1)),
        user_id: Some(id16(0xB1)),
        session_id: None,
        aggregate_id: None,
        expected_generation: None,
        command_type: command_type.into(),
        schema_version: 1,
        payload,
        issued_at: None,
    }
}

fn envelope_fenced(
    command_id: Id,
    command_type: &str,
    payload: Vec<u8>,
    generation: Option<u64>,
) -> CommandEnvelope {
    let mut e = envelope(command_id, command_type, payload);
    e.expected_generation = generation;
    e
}

/// A session this client owns (the lease generation comes with it).
async fn create_session(c: &mut Client) -> (Id, u64) {
    use modbit_protocol::v1::{AcquireSessionLease, SessionLeaseAcquired};
    let ack = c
        .command(envelope(
            fresh_id(),
            "CreateSession",
            CreateSession { space_id: None }.encode_to_vec(),
        ))
        .await
        .unwrap();
    let session = Client::result::<SessionCreated>(&ack)
        .unwrap()
        .session_id
        .unwrap();
    let ack = c
        .command(envelope(
            fresh_id(),
            "AcquireSessionLease",
            AcquireSessionLease {
                session_id: Some(session.clone()),
                owner: "test".into(),
            }
            .encode_to_vec(),
        ))
        .await
        .unwrap();
    let lease = Client::result::<SessionLeaseAcquired>(&ack)
        .unwrap()
        .lease_generation;
    (session, lease)
}

/// A committed repository with the no-op mandatory check (FIX-03).
fn repo() -> (tempfile::TempDir, String) {
    let repo = tempfile::tempdir().unwrap();
    std::fs::write(repo.path().join("notes.txt"), "alpha\nbeta\n").unwrap();
    std::fs::create_dir_all(repo.path().join(".modbit")).unwrap();
    std::fs::write(
        repo.path().join(".modbit/verification.json"),
        "{\"commands\": [{\"id\": \"fixture-noop\", \"argv\": [\"git\", \"--version\"]}]}",
    )
    .unwrap();
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
            Command::new("git")
                .arg("-C")
                .arg(repo.path())
                .args(&args)
                .status()
                .unwrap()
                .success()
        );
    }
    let root = repo
        .path()
        .canonicalize()
        .unwrap()
        .to_string_lossy()
        .trim_start_matches(r"\\?\")
        .to_owned();
    (repo, root)
}

async fn create_task(c: &mut Client, session: &Id, lease: u64, goal: &str, root: &str) -> Id {
    let ack = c
        .command(envelope_fenced(
            fresh_id(),
            "CreateTask",
            CreateTask {
                session_id: Some(session.clone()),
                goal_text: goal.into(),
                workspace_id: None,
                execution_profile: String::new(),
                origin: "cli".into(),
                workspace_root: root.into(),
                issue_url: String::new(),
                issue_json: String::new(),
            }
            .encode_to_vec(),
            Some(lease),
        ))
        .await
        .unwrap();
    Client::result::<TaskCreated>(&ack)
        .unwrap()
        .task_id
        .unwrap()
}

/// Start (or resume) the task's loop; one text-only turn ends the run.
async fn start_task(c: &mut Client, task: &Id, lease: u64) -> TaskRunStarted {
    let ack = c
        .command(envelope_fenced(
            fresh_id(),
            "StartTask",
            StartTask {
                task_id: Some(task.clone()),
                endpoint: String::new(),
                model: "gpt-5-mini".into(),
                max_turns: 0,
                max_tool_calls: 0,
                max_no_progress_turns: 1,
                skills: vec![],
            }
            .encode_to_vec(),
            Some(lease),
        ))
        .await
        .unwrap();
    Client::result(&ack).unwrap()
}

async fn task_status(c: &mut Client, task: &Id) -> TaskStatus {
    let ack = c
        .command(envelope(
            fresh_id(),
            "GetTaskStatus",
            GetTaskStatus {
                task_id: Some(task.clone()),
            }
            .encode_to_vec(),
        ))
        .await
        .unwrap();
    Client::result(&ack).unwrap()
}

/// Wait until the task's loop is no longer alive (or `secs` pass).
async fn wait_idle(c: &mut Client, task: &Id, secs: u64) -> TaskStatus {
    let deadline = Instant::now() + Duration::from_secs(secs);
    loop {
        let st = task_status(c, task).await;
        if !st.loop_alive || Instant::now() > deadline {
            return st;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

async fn read_object(c: &mut Client, hash: &str) -> Vec<u8> {
    use modbit_protocol::v1::ObjectRangeChunk;
    let mut out = Vec::new();
    loop {
        let ack = c
            .command(envelope(
                fresh_id(),
                "ReadObjectRange",
                ReadObjectRange {
                    object_hash: hash.to_owned(),
                    offset: out.len() as u64,
                    length: 1024 * 1024,
                }
                .encode_to_vec(),
            ))
            .await
            .unwrap();
        let chunk: ObjectRangeChunk = Client::result(&ack).unwrap();
        if chunk.data.is_empty() {
            return out;
        }
        out.extend_from_slice(&chunk.data);
        if out.len() as u64 >= chunk.total_bytes {
            return out;
        }
    }
}

// ------------------------------------------------------------- the events

/// One event as a client received it.
#[derive(Clone, Debug)]
struct Ev {
    offset: u64,
    aggregate: String,
    kind: String,
    sequence: u64,
    /// The event's own payload (the envelope's inline payload).
    payload: serde_json::Value,
    /// When the client received it.
    received: Instant,
    /// When the Core recorded it, in milliseconds.
    recorded_ms: i64,
    aggregate_id: Vec<u8>,
}

fn ev_of(e: modbit_protocol::v1::StoredEventFrame) -> Ev {
    let ev = e.event.unwrap();
    let p: serde_json::Value = serde_json::from_slice(&ev.payload).unwrap_or_default();
    let at = ev.recorded_at.unwrap_or_default();
    Ev {
        offset: e.offset,
        aggregate: ev.aggregate_type,
        kind: ev.event_type,
        sequence: ev.sequence,
        payload: p["payload"].clone(),
        received: Instant::now(),
        recorded_ms: at.seconds * 1000 + i64::from(at.nanos) / 1_000_000,
        aggregate_id: ev.aggregate_id.map(|i| i.value).unwrap_or_default(),
    }
}

/// A live subscriber: everything after `after`, collected as it arrives.
fn collect_live(core: &CoreProcess, session: &Id, after: u64) -> Arc<Mutex<Vec<Ev>>> {
    let out = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&out);
    let endpoint = core.ready.endpoint.clone();
    let secret = decode_hex(&core.ready.boot_secret_hex).unwrap();
    let session = session.clone();
    tokio::spawn(async move {
        let Ok(mut c) = Client::connect(&endpoint, &secret, ClientKind::Cli, "live").await else {
            return;
        };
        if c.subscribe(session, after).await.is_err() {
            return;
        }
        while let Ok(Some(e)) = c.next_event().await {
            sink.lock().unwrap().push(ev_of(e));
        }
    });
    out
}

/// Every event of the session after `after`, replayed by cursor.
async fn replay(core: &CoreProcess, session: &Id, after: u64) -> Vec<Ev> {
    let mut s = core.client().await;
    let ack = s
        .command(envelope(
            fresh_id(),
            "GetSessionSnapshot",
            GetSessionSnapshot {
                session_id: Some(session.clone()),
            }
            .encode_to_vec(),
        ))
        .await
        .unwrap();
    let floor = Client::result::<SessionSnapshot>(&ack)
        .map(|snap| snap.last_offset)
        .unwrap_or(0);
    s.subscribe(session.clone(), after).await.unwrap();
    let mut out: Vec<Ev> = Vec::new();
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        let seen = out.last().map_or(after, |e| e.offset);
        let wait = if seen < floor {
            deadline.saturating_duration_since(Instant::now())
        } else {
            Duration::from_millis(300)
        };
        let Ok(Ok(Some(e))) = tokio::time::timeout(wait, s.next_event()).await else {
            break;
        };
        out.push(ev_of(e));
    }
    out
}

fn of_kind<'a>(events: &'a [Ev], kind: &str) -> Vec<&'a Ev> {
    events.iter().filter(|e| e.kind == kind).collect()
}

/// The text of a stream's deltas, in order, checking they are contiguous.
fn stream_text(deltas: &[&Ev]) -> String {
    let mut out = String::new();
    for (i, d) in deltas.iter().enumerate() {
        assert_eq!(d.sequence, i as u64 + 1, "deltas are contiguous");
        assert_eq!(d.payload["sequence"], d.sequence);
        assert_eq!(d.payload["provenance"], "MODEL_OUTPUT");
        assert_eq!(d.payload["retention"], "COLLAPSIBLE_ON_COMPLETE");
        out.push_str(d.payload["text"].as_str().unwrap());
    }
    out
}

// ------------------------------------------------- the scripted model

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum End {
    /// The stream finishes normally.
    #[default]
    Finish,
    /// The connection stays open and silent: the Core can be killed here.
    Hang,
    /// The connection is dropped without a finish: a provider failure.
    Drop,
}

/// One scripted model response.
#[derive(Clone, Debug, Default)]
struct Step {
    reasoning: Vec<String>,
    chunks: Vec<String>,
    gap_ms: u64,
    calls: Vec<(String, serde_json::Value)>,
    end: End,
}

impl Step {
    fn text(chunks: Vec<String>, gap_ms: u64) -> Self {
        Step {
            chunks,
            gap_ms,
            ..Step::default()
        }
    }
}

const HEAD: &str = "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\nx-request-id: req_stream\r\nconnection: close\r\ntransfer-encoding: chunked\r\n\r\n";

/// A scripted OpenAI-compatible streaming server: request `n` is answered
/// with `steps[n]` (the last step repeats), chunk by chunk with the step's
/// gap between them.
async fn streaming_model(steps: Vec<Step>) -> String {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let served = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let steps = Arc::new(steps);
    tokio::spawn(async move {
        loop {
            let Ok((mut sock, _)) = listener.accept().await else {
                return;
            };
            let steps = Arc::clone(&steps);
            let n = served.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            tokio::spawn(async move {
                // Read the request (head and body) before answering.
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
                let step = steps
                    .get(n)
                    .or_else(|| steps.last())
                    .cloned()
                    .unwrap_or_default();
                let write = |frame: String| {
                    let data = format!("data: {frame}\n\n");
                    format!("{:x}\r\n{}\r\n", data.len(), data).into_bytes()
                };
                let delta = |d: serde_json::Value| {
                    serde_json::json!({"id":"c","model":"scripted-1","choices":[{"index":0,"delta":d,"finish_reason":null}]}).to_string()
                };
                if sock.write_all(HEAD.as_bytes()).await.is_err() {
                    return;
                }
                for r in &step.reasoning {
                    let _ = sock
                        .write_all(&write(delta(serde_json::json!({"reasoning_content": r}))))
                        .await;
                    let _ = sock.flush().await;
                }
                for c in &step.chunks {
                    if sock
                        .write_all(&write(delta(serde_json::json!({"content": c}))))
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
                match step.end {
                    End::Hang => {
                        tokio::time::sleep(Duration::from_secs(600)).await;
                        return;
                    }
                    End::Drop => {
                        // No finish frame, no chunked terminator: the
                        // connection simply goes away mid-stream.
                        drop(sock);
                        return;
                    }
                    End::Finish => {}
                }
                for (i, (name, args)) in step.calls.iter().enumerate() {
                    let _ = sock
                        .write_all(&write(delta(serde_json::json!({"tool_calls":[{"index":i,"id":format!("call_{n}_{i}"),"type":"function","function":{"name":name,"arguments":args.to_string()}}]}))))
                        .await;
                }
                let finish = if step.calls.is_empty() {
                    "stop"
                } else {
                    "tool_calls"
                };
                let _ = sock
                    .write_all(&write(serde_json::json!({"id":"c","model":"scripted-1","choices":[{"index":0,"delta":{},"finish_reason":finish}],"usage":{"prompt_tokens":100,"completion_tokens":50,"prompt_tokens_details":{"cached_tokens":0}}}).to_string()))
                    .await;
                let _ = sock.write_all(&write("[DONE]".into())).await;
                let _ = sock.write_all(b"0\r\n\r\n").await;
                let _ = sock.flush().await;
                let _ = sock.shutdown().await;
            });
        }
    });
    format!("http://127.0.0.1:{port}")
}

/// 400 words in chunks of five, as a slow model would speak them.
fn four_hundred_words() -> (String, Vec<String>) {
    let words: Vec<String> = (0..400).map(|i| format!("word{i}")).collect();
    let chunks: Vec<String> = words
        .chunks(5)
        .map(|w| format!("{} ", w.join(" ")))
        .collect();
    (chunks.concat(), chunks)
}

fn env_for<'a>(base: &'a str, key: &'a str) -> [(&'a str, &'a str); 3] {
    [
        ("MODBIT_OPENAI_BASE_URL", base),
        ("OPENAI_API_KEY", key),
        ("ANTHROPIC_API_KEY", ""),
    ]
}

const SETTLE: Duration = Duration::from_millis(400);

// ------------------------------------------------------------- PX-041

/// A 400-word answer arrives as strictly increasing deltas within the size
/// and rate bounds, before the completion; their concatenation is the
/// completed message (digest and object); a client that reconnects from a
/// cursor sees the same ordered deltas and the completion record, with no
/// gap and no duplicate.
#[tokio::test]
async fn qual_px_041_a_streamed_answer_arrives_as_bounded_deltas_before_its_completion_and_replays_by_cursor()
 {
    let (full, chunks) = four_hundred_words();
    let base = streaming_model(vec![Step::text(chunks.clone(), 12)]).await;
    let (_repo, root) = repo();
    let dir = tempfile::tempdir().unwrap();
    let core = CoreProcess::spawn_with_env(dir.path(), &env_for(&base, ""));
    let mut c = core.client().await;
    let (session, lease) = create_session(&mut c).await;
    let task = create_task(&mut c, &session, lease, "answer at length", &root).await;
    let live = collect_live(&core, &session, 0);
    let _ = start_task(&mut c, &task, lease).await;
    let st = wait_idle(&mut c, &task, 60).await;
    assert!(!st.loop_alive, "{st:?}");
    tokio::time::sleep(SETTLE).await;

    let live = live.lock().unwrap().clone();
    let deltas: Vec<&Ev> = of_kind(&live, "AssistantTextDelta");
    let done = of_kind(&live, "AssistantMessageCompleted");
    assert_eq!(done.len(), 1, "exactly one completion record");
    assert!(of_kind(&live, "AssistantMessageAborted").is_empty());
    assert!(deltas.len() >= 2, "{} deltas", deltas.len());
    // The 80 chunks the model sent over ~1 s are coalesced: far fewer events.
    assert!(
        deltas.len() < chunks.len() / 2,
        "coalesced: {} deltas for {} chunks",
        deltas.len(),
        chunks.len()
    );
    // One stream; every delta inside the bounds and in order.
    let stream_id = deltas[0].aggregate_id.clone();
    assert!(deltas.iter().all(|d| d.aggregate_id == stream_id));
    assert_eq!(done[0].aggregate_id, stream_id);
    assert!(deltas.windows(2).all(|w| w[0].offset < w[1].offset));
    assert!(
        deltas
            .iter()
            .all(|d| d.payload["text"].as_str().unwrap().len() <= 2048)
    );
    // Lineage rides on every delta.
    for d in &deltas {
        for k in ["run_id", "turn_id", "step_id", "stream_id"] {
            assert!(d.payload[k].is_string(), "{k} missing: {:?}", d.payload);
        }
    }
    // At most one coalesced event per 50 ms (the bound of docs/65): more
    // than the time allows would mean an uncoalesced flood.
    let span_ms = (deltas.last().unwrap().recorded_ms - deltas[0].recorded_ms).max(0) as usize;
    assert!(
        deltas.len() <= span_ms / 50 + 2 + full.len() / 2048,
        "{} deltas in {span_ms} ms",
        deltas.len()
    );
    // Deltas were visible to the client before the completion existed.
    let first_delta = deltas[0];
    assert!(
        first_delta.received + Duration::from_millis(150) < done[0].received,
        "the first delta arrived {:?} before the completion",
        done[0]
            .received
            .saturating_duration_since(first_delta.received)
    );
    assert!(first_delta.offset < done[0].offset);
    // The concatenation is the completed message.
    let text = stream_text(&deltas);
    assert_eq!(text, full);
    let p = &done[0].payload;
    assert_eq!(p["delta_count"], deltas.len() as u64);
    assert_eq!(p["kind"], "TEXT");
    let digest = hex::encode(sha2::Sha256::digest(full.as_bytes()));
    assert_eq!(p["content_hash"], digest);
    assert_eq!(p["text_ref"]["object_hash"], digest);
    assert_eq!(p["text_ref"]["byte_length"], full.len() as u64);
    assert_eq!(read_object(&mut c, &digest).await, full.as_bytes());

    // Cursor replay after a reconnect: the suffix is identical, in order,
    // with no gap and no duplicate; and a full replay agrees with the live view.
    let cut = deltas[1].offset;
    let replayed = replay(&core, &session, cut).await;
    let tail: Vec<(u64, String)> = live
        .iter()
        .filter(|e| e.offset > cut)
        .map(|e| (e.offset, e.kind.clone()))
        .collect();
    let again: Vec<(u64, String)> = replayed
        .iter()
        .map(|e| (e.offset, e.kind.clone()))
        .collect();
    assert_eq!(again, tail);
    let all = replay(&core, &session, 0).await;
    let all_deltas = of_kind(&all, "AssistantTextDelta");
    assert_eq!(stream_text(&all_deltas), full);
    assert_eq!(of_kind(&all, "AssistantMessageCompleted").len(), 1);
    assert_eq!(
        all.iter()
            .filter(|e| e.aggregate == "assistant_stream")
            .map(|e| e.sequence)
            .collect::<Vec<_>>(),
        (1..=deltas.len() as u64 + 1).collect::<Vec<_>>(),
        "the stream's own sequence is gapless, the completion last"
    );
}

/// A provider connection dropped mid-stream ends the stream as aborted with a
/// provider source: no completion record, and nothing presents the partial
/// text as a message.
#[tokio::test]
async fn qual_px_041_a_provider_that_drops_mid_stream_aborts_the_stream_and_never_completes() {
    let (_, chunks) = four_hundred_words();
    let base = streaming_model(vec![Step {
        chunks: chunks[..30].to_vec(),
        gap_ms: 5,
        end: End::Drop,
        ..Step::default()
    }])
    .await;
    let (_repo, root) = repo();
    let dir = tempfile::tempdir().unwrap();
    let core = CoreProcess::spawn_with_env(dir.path(), &env_for(&base, ""));
    let mut c = core.client().await;
    let (session, lease) = create_session(&mut c).await;
    let task = create_task(&mut c, &session, lease, "answer at length", &root).await;
    let _ = start_task(&mut c, &task, lease).await;
    let _ = wait_idle(&mut c, &task, 60).await;
    tokio::time::sleep(SETTLE).await;
    let all = replay(&core, &session, 0).await;
    let deltas = of_kind(&all, "AssistantTextDelta");
    assert!(!deltas.is_empty(), "the text that arrived was published");
    assert!(of_kind(&all, "AssistantMessageCompleted").is_empty());
    let aborted = of_kind(&all, "AssistantMessageAborted");
    assert_eq!(aborted.len(), 1);
    let p = &aborted[0].payload;
    assert_eq!(p["source"], "PROVIDER");
    assert_eq!(p["code"], "STREAM_INTERRUPTED");
    assert_eq!(p["delta_count"], deltas.len() as u64);
    assert_eq!(aborted[0].aggregate_id, deltas[0].aggregate_id);
    assert_eq!(
        p["bytes_streamed"],
        stream_text(&deltas).len() as u64,
        "the record counts what was streamed"
    );
}

/// A cancelled task closes its open stream as a user interrupt, and the
/// partial text is never presented as a message.
#[tokio::test]
async fn qual_px_041_a_cancelled_turn_ends_its_stream_as_a_user_interrupt() {
    let (_, chunks) = four_hundred_words();
    let base = streaming_model(vec![Step {
        chunks: chunks[..20].to_vec(),
        gap_ms: 5,
        end: End::Hang,
        ..Step::default()
    }])
    .await;
    let (_repo, root) = repo();
    let dir = tempfile::tempdir().unwrap();
    let core = CoreProcess::spawn_with_env(dir.path(), &env_for(&base, ""));
    let mut c = core.client().await;
    let (session, lease) = create_session(&mut c).await;
    let task = create_task(&mut c, &session, lease, "answer at length", &root).await;
    let _ = start_task(&mut c, &task, lease).await;
    // Wait until text is flowing, then cancel mid-stream.
    let deadline = Instant::now() + Duration::from_secs(120);
    loop {
        let seen = replay(&core, &session, 0).await;
        if !of_kind(&seen, "AssistantTextDelta").is_empty() {
            break;
        }
        assert!(Instant::now() < deadline, "no delta arrived");
    }
    c.command(envelope_fenced(
        fresh_id(),
        "CancelTask",
        CancelTask {
            task_id: Some(task.clone()),
        }
        .encode_to_vec(),
        Some(lease),
    ))
    .await
    .unwrap();
    let _ = wait_idle(&mut c, &task, 30).await;
    tokio::time::sleep(SETTLE).await;
    let all = replay(&core, &session, 0).await;
    assert!(of_kind(&all, "AssistantMessageCompleted").is_empty());
    let aborted = of_kind(&all, "AssistantMessageAborted");
    assert_eq!(
        aborted.len(),
        1,
        "{:?}",
        all.iter().map(|e| &e.kind).collect::<Vec<_>>()
    );
    assert_eq!(aborted[0].payload["source"], "USER_INTERRUPT");
    assert_eq!(aborted[0].payload["code"], "CANCELLED");
}

/// A secret the Core holds, planted in model text and split across chunks, is
/// replaced in every delta and in the completed record, and the log holds no
/// copy of it anywhere on disk; a credential-shaped token the Core does not
/// hold is replaced too.
#[tokio::test]
async fn qual_px_041_a_planted_secret_is_redacted_before_append_even_when_split_across_chunks() {
    const KEY: &str = "sk-held-0123456789abcdefghijkl";
    const SHAPED: &str = "ghp_abcdefghijklmnopqrstuvwxyz0123";
    let text = format!("The credential is {KEY} and a token {SHAPED} appear in this answer. Done.");
    // Cut the held key and the shaped token at every awkward place.
    let mut chunks = Vec::new();
    let mut at = 0;
    for step in [3usize, 5, 7, 4, 6, 2, 9] {
        let mut end = (at + step).min(text.len());
        while !text.is_char_boundary(end) {
            end += 1;
        }
        chunks.push(text[at..end].to_owned());
        at = end;
        if at >= text.len() {
            break;
        }
    }
    while at < text.len() {
        let end = (at + 4).min(text.len());
        chunks.push(text[at..end].to_owned());
        at = end;
    }
    // Slower than the coalescing interval, so a flush falls between chunks
    // and a secret really is split across two published deltas.
    let base = streaming_model(vec![Step::text(chunks, 60)]).await;
    let (_repo, root) = repo();
    let dir = tempfile::tempdir().unwrap();
    // The Core holds KEY as a provider credential.
    let core = CoreProcess::spawn_with_env(dir.path(), &env_for(&base, KEY));
    let mut c = core.client().await;
    let (session, lease) = create_session(&mut c).await;
    let task = create_task(&mut c, &session, lease, "say something", &root).await;
    let _ = start_task(&mut c, &task, lease).await;
    let _ = wait_idle(&mut c, &task, 60).await;
    tokio::time::sleep(SETTLE).await;
    let all = replay(&core, &session, 0).await;
    let deltas = of_kind(&all, "AssistantTextDelta");
    assert!(!deltas.is_empty());
    for d in &deltas {
        let t = d.payload["text"].as_str().unwrap();
        assert!(
            !t.contains("held-0123") && !t.contains("abcdefghijklmnop"),
            "a delta carries part of a secret: {t:?}"
        );
    }
    let streamed = stream_text(&deltas);
    assert_eq!(
        streamed,
        "The credential is [redacted] and a token [redacted] appear in this answer. Done."
    );
    let done = of_kind(&all, "AssistantMessageCompleted");
    assert_eq!(done.len(), 1);
    let hash = done[0].payload["text_ref"]["object_hash"].as_str().unwrap();
    assert_eq!(read_object(&mut c, hash).await, streamed.as_bytes());
    // The conversation read model serves the redacted text, never the secret.
    let page = whole_transcript(&mut c, &task, TranscriptDensity::Detailed).await;
    let said = page
        .rows
        .iter()
        .find(|r| kind_of(r) == TranscriptRowKind::AssistantMessage)
        .expect("the assistant row");
    assert_eq!(said.text, streamed);
    assert!(!format!("{page:?}").contains("held-0123"));
    // Nothing on disk holds either secret: the log, the objects, the WAL.
    drop(c);
    let mut core = core;
    core.kill();
    for needle in [KEY, SHAPED] {
        let hit = walk(dir.path()).into_iter().find(|p| {
            std::fs::read(p)
                .map(|b| b.windows(needle.len()).any(|w| w == needle.as_bytes()))
                .unwrap_or(false)
        });
        assert_eq!(hit, None, "{needle} reached the disk");
    }
}

fn walk(root: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d).into_iter().flatten().flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else {
                out.push(p);
            }
        }
    }
    out
}

/// SIGKILL of the Core mid-stream: after the restart the stream is closed as
/// aborted by recovery, no partial text is marked complete, and the task is
/// held by the existing recovery path rather than silently re-running.
#[tokio::test]
async fn qual_px_041_a_core_killed_mid_stream_leaves_an_aborted_stream_after_restart() {
    let (_, chunks) = four_hundred_words();
    let base = streaming_model(vec![Step {
        chunks: chunks[..25].to_vec(),
        gap_ms: 5,
        end: End::Hang,
        ..Step::default()
    }])
    .await;
    let (_repo, root) = repo();
    let dir = tempfile::tempdir().unwrap();
    let mut core = CoreProcess::spawn_with_env(dir.path(), &env_for(&base, ""));
    let mut c = core.client().await;
    let (session, lease) = create_session(&mut c).await;
    let task = create_task(&mut c, &session, lease, "answer at length", &root).await;
    let _ = start_task(&mut c, &task, lease).await;
    let deadline = Instant::now() + Duration::from_secs(120);
    let before = loop {
        let seen = replay(&core, &session, 0).await;
        if of_kind(&seen, "AssistantTextDelta").len() >= 2 {
            break seen;
        }
        assert!(Instant::now() < deadline, "no deltas arrived");
    };
    assert!(
        of_kind(&before, "AssistantMessageAborted").is_empty()
            && of_kind(&before, "AssistantMessageCompleted").is_empty(),
        "the stream is open when the Core dies"
    );
    drop(c);
    core.kill();
    // A second Core on the same profile.
    let core = CoreProcess::spawn_with_env(dir.path(), &env_for(&base, ""));
    let all = replay(&core, &session, 0).await;
    let deltas = of_kind(&all, "AssistantTextDelta");
    assert!(deltas.len() >= 2);
    assert!(of_kind(&all, "AssistantMessageCompleted").is_empty());
    let aborted = of_kind(&all, "AssistantMessageAborted");
    assert_eq!(aborted.len(), 1);
    let p = &aborted[0].payload;
    assert_eq!(p["source"], "RECOVERY");
    assert_eq!(p["code"], "ABORTED_BY_RECOVERY");
    assert_eq!(p["delta_count"], deltas.len() as u64);
    assert_eq!(aborted[0].aggregate_id, deltas[0].aggregate_id);
    // The abort is the stream's last record, after every delta it counts.
    assert!(aborted[0].offset > deltas.last().unwrap().offset);
    let mut c = core.client().await;
    let st = task_status(&mut c, &task).await;
    assert!(
        !st.loop_alive,
        "recovery does not resume a loop mid-token: {st:?}"
    );
    // A second restart finds nothing open and appends nothing more.
    drop(c);
    let mut core = core;
    core.kill();
    let core = CoreProcess::spawn_with_env(dir.path(), &env_for(&base, ""));
    let again = replay(&core, &session, 0).await;
    assert_eq!(of_kind(&again, "AssistantMessageAborted").len(), 1);
}

/// A very long stream stays bounded: every delta within the byte bound, the
/// number of events far below the number of chunks, the Core's memory flat
/// (it holds one pending chunk and the stream's text, never a growing queue),
/// and a stream beyond the stream bound aborts as a runtime failure.
#[tokio::test]
async fn qual_px_041_a_very_long_stream_is_bounded_in_events_and_in_memory_and_a_runaway_aborts() {
    // 3 MiB of text in 64-byte chunks, as fast as the provider can write.
    let piece =
        "0123456789abcdefghijklmnopqrstuvwxyz ABCDEFGHIJKLMNOPQRSTUVWXYZ.\n"[..64].to_owned();
    let chunks: Vec<String> = (0..(3 * 1024 * 1024 / 64)).map(|_| piece.clone()).collect();
    let total = chunks.len() * 64;
    let base = streaming_model(vec![Step::text(chunks.clone(), 0)]).await;
    let (_repo, root) = repo();
    let dir = tempfile::tempdir().unwrap();
    let core = CoreProcess::spawn_with_env(dir.path(), &env_for(&base, ""));
    let mut c = core.client().await;
    let rss_before = rss_kib(core.pid());
    let (session, lease) = create_session(&mut c).await;
    let task = create_task(&mut c, &session, lease, "write a lot", &root).await;
    let _ = start_task(&mut c, &task, lease).await;
    let _ = wait_idle(&mut c, &task, 120).await;
    tokio::time::sleep(SETTLE).await;
    let all = replay(&core, &session, 0).await;
    let deltas = of_kind(&all, "AssistantTextDelta");
    let done = of_kind(&all, "AssistantMessageCompleted");
    assert_eq!(
        done.len(),
        1,
        "{:?}",
        of_kind(&all, "AssistantMessageAborted")
            .first()
            .map(|e| &e.payload)
    );
    assert!(
        deltas
            .iter()
            .all(|d| d.payload["text"].as_str().unwrap().len() <= 2048)
    );
    assert!(
        deltas.len() < chunks.len() / 8,
        "{} events for {} chunks",
        deltas.len(),
        chunks.len()
    );
    assert_eq!(stream_text(&deltas).len(), total);
    assert_eq!(done[0].payload["text_ref"]["byte_length"], total as u64);
    if let (Some(before), Some(after)) = (rss_before, rss_kib(core.pid())) {
        // 3 MiB streamed: the Core grew by a small multiple of that (its
        // copies of the text), not by the sum of every chunk it ever held.
        assert!(
            after < before + 96 * 1024,
            "resident memory grew from {before} KiB to {after} KiB"
        );
    }
    drop(c);
    drop(core);

    // A runaway: 5 MiB is beyond the stream bound.
    let big: Vec<String> = (0..(5 * 1024 * 1024 / 64)).map(|_| piece.clone()).collect();
    let base = streaming_model(vec![Step::text(big, 0)]).await;
    let dir = tempfile::tempdir().unwrap();
    let core = CoreProcess::spawn_with_env(dir.path(), &env_for(&base, ""));
    let mut c = core.client().await;
    let (session, lease) = create_session(&mut c).await;
    let task = create_task(&mut c, &session, lease, "write far too much", &root).await;
    let _ = start_task(&mut c, &task, lease).await;
    let _ = wait_idle(&mut c, &task, 120).await;
    tokio::time::sleep(SETTLE).await;
    let all = replay(&core, &session, 0).await;
    assert!(of_kind(&all, "AssistantMessageCompleted").is_empty());
    let aborted = of_kind(&all, "AssistantMessageAborted");
    assert_eq!(aborted.len(), 1);
    assert_eq!(aborted[0].payload["source"], "RUNTIME");
    assert_eq!(aborted[0].payload["code"], "STREAM_TOO_LARGE");
    assert!(aborted[0].payload["bytes_streamed"].as_u64().unwrap() <= 4 * 1024 * 1024);
}

#[cfg(unix)]
fn rss_kib(pid: u32) -> Option<u64> {
    let out = Command::new("ps")
        .args(["-o", "rss=", "-p", &pid.to_string()])
        .output()
        .ok()?;
    String::from_utf8_lossy(&out.stdout).trim().parse().ok()
}

#[cfg(not(unix))]
fn rss_kib(_pid: u32) -> Option<u64> {
    None
}

/// No client can append a delta: a forged command naming one is refused, and
/// the log holds no stream it did not write.
#[tokio::test]
async fn qual_px_041_a_forged_client_command_cannot_append_a_delta() {
    let base = streaming_model(vec![]).await;
    let dir = tempfile::tempdir().unwrap();
    let core = CoreProcess::spawn_with_env(dir.path(), &env_for(&base, ""));
    let mut c = core.client().await;
    let (session, lease) = create_session(&mut c).await;
    for name in [
        "AssistantTextDelta",
        "AppendAssistantTextDelta",
        "AppendEvent",
        "AssistantMessageCompleted",
    ] {
        let err = c
            .command(envelope_fenced(
                fresh_id(),
                name,
                serde_json::json!({"stream_id": "x", "sequence": 1, "text": "forged"})
                    .to_string()
                    .into_bytes(),
                Some(lease),
            ))
            .await
            .unwrap_err();
        assert!(
            matches!(err, ClientError::Rejected { .. }),
            "{name}: {err:?}"
        );
    }
    let all = replay(&core, &session, 0).await;
    assert!(all.iter().all(|e| e.aggregate != "assistant_stream"));
}

/// Reasoning is published only when policy allows it, as a stream of its own
/// kind; the answer's text stream is unaffected.
#[tokio::test]
async fn qual_px_041_reasoning_is_streamed_only_where_policy_allows() {
    let step = Step {
        reasoning: vec![
            "Considering the question. ".into(),
            "Then answering. ".into(),
        ],
        chunks: vec!["The answer ".into(), "is forty-two.".into()],
        gap_ms: 60,
        ..Step::default()
    };
    for (allowed, expect_reasoning) in [(false, false), (true, true)] {
        let base = streaming_model(vec![step.clone()]).await;
        let (_repo, root) = repo();
        let dir = tempfile::tempdir().unwrap();
        let mut env = env_for(&base, "").to_vec();
        if allowed {
            env.push(("MODBIT_STREAM_REASONING", "1"));
        }
        let core = CoreProcess::spawn_with_env(dir.path(), &env);
        let mut c = core.client().await;
        let (session, lease) = create_session(&mut c).await;
        let task = create_task(&mut c, &session, lease, "think", &root).await;
        let _ = start_task(&mut c, &task, lease).await;
        let _ = wait_idle(&mut c, &task, 60).await;
        tokio::time::sleep(SETTLE).await;
        let all = replay(&core, &session, 0).await;
        let deltas = of_kind(&all, "AssistantTextDelta");
        let text: Vec<&Ev> = deltas
            .iter()
            .copied()
            .filter(|d| d.payload["kind"] == "TEXT")
            .collect();
        let reasoning: Vec<&Ev> = deltas
            .iter()
            .copied()
            .filter(|d| d.payload["kind"] == "REASONING")
            .collect();
        assert_eq!(stream_text(&text), "The answer is forty-two.");
        let page = whole_transcript(&mut c, &task, TranscriptDensity::Detailed).await;
        let said = page
            .rows
            .iter()
            .find(|r| kind_of(r) == TranscriptRowKind::AssistantMessage)
            .unwrap();
        assert_eq!(said.text, "The answer is forty-two.");
        assert_eq!(said.hints.as_ref().unwrap().has_reasoning, expect_reasoning);
        assert_eq!(!reasoning.is_empty(), expect_reasoning, "allowed={allowed}");
        if expect_reasoning {
            assert!(
                stream_text(&reasoning).contains("Considering the question."),
                "{reasoning:?}"
            );
            assert_ne!(reasoning[0].aggregate_id, text[0].aggregate_id);
            assert_eq!(of_kind(&all, "AssistantMessageCompleted").len(), 2);
        }
    }
}

// ------------------------------------------------------------- PX-042

use modbit_protocol::v1::transcript_row::Facts;
use modbit_protocol::v1::{
    AgentHeader, AgentHeaders, AgentStatusClass, ArchiveTask, GetAgentHeaders, GetTranscript,
    MarkRead, QueueInput, ReadMarked, TaskArchived, TranscriptDensity, TranscriptPage,
    TranscriptRow, TranscriptRowKind,
};

/// A committed repository whose check is `git --version` (FIX-03) and whose
/// one file the agent is to fix.
fn coding_repo() -> (tempfile::TempDir, String, String) {
    let (repo, root) = repo();
    std::fs::write(repo.path().join("qty.txt"), "quantity = -5\n").unwrap();
    for args in [
        vec!["add", "-A"],
        vec![
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@e",
            "commit",
            "-q",
            "-m",
            "qty",
        ],
    ] {
        assert!(
            Command::new("git")
                .arg("-C")
                .arg(repo.path())
                .args(&args)
                .status()
                .unwrap()
                .success()
        );
    }
    let hash = hex::encode(sha2::Sha256::digest(
        std::fs::read(repo.path().join("qty.txt")).unwrap(),
    ));
    (repo, root, hash)
}

/// The coding script: two reads in one turn, plan, a failing check, a wrong
/// edit, a failing check, the right edit, a passing check, completion. The
/// check is `git grep`, so it runs the same on every platform.
fn coding_steps(hash: &str) -> Vec<Step> {
    use serde_json::json;
    let check =
        json!({"argv": ["git", "grep", "-q", "validated", "--", "qty.txt"], "inherit_env": true});
    let said = |t: &str| vec![t.to_owned()];
    vec![
        Step {
            chunks: said("Reading the file first."),
            calls: vec![
                ("fs.read".into(), json!({"path": "qty.txt"})),
                ("fs.stat".into(), json!({"path": "qty.txt"})),
            ],
            ..Step::default()
        },
        Step {
            chunks: said("Planning."),
            calls: vec![(
                "plan.update".into(),
                json!({"outcome": "reject negative quantities", "expected_files": ["qty.txt"], "verification": ["git grep validated"], "protected_effects": []}),
            )],
            ..Step::default()
        },
        Step {
            calls: vec![("test.run".into(), check.clone())],
            ..Step::default()
        },
        Step {
            calls: vec![(
                "change.apply".into(),
                json!({"path": "qty.txt", "op": "replace", "content": "quantity = 5\n", "expected_content_hash": hash}),
            )],
            ..Step::default()
        },
        Step {
            calls: vec![("test.run".into(), check.clone())],
            ..Step::default()
        },
        Step {
            chunks: said("The check failed; the file must say validated."),
            calls: vec![(
                "change.apply".into(),
                json!({"path": "qty.txt", "op": "replace", "content": "quantity = 5 # validated: negatives rejected\n"}),
            )],
            ..Step::default()
        },
        Step {
            calls: vec![("test.run".into(), check)],
            ..Step::default()
        },
        Step {
            calls: vec![(
                "task.complete".into(),
                json!({"summary": "Negative quantities are rejected and the check passes.", "self_review": {"findings": [{"text": "the check passes at the candidate revision", "resolved": true}], "verification": ["git grep validated"]}}),
            )],
            ..Step::default()
        },
    ]
}

async fn start_with(c: &mut Client, task: &Id, lease: u64, no_progress: u32) {
    let ack = c
        .command(envelope_fenced(
            fresh_id(),
            "StartTask",
            StartTask {
                task_id: Some(task.clone()),
                endpoint: String::new(),
                model: "gpt-5-mini".into(),
                max_turns: 0,
                max_tool_calls: 0,
                max_no_progress_turns: no_progress,
                skills: vec![],
            }
            .encode_to_vec(),
            Some(lease),
        ))
        .await
        .unwrap();
    let _: TaskRunStarted = Client::result(&ack).unwrap();
}

async fn get_transcript(
    c: &mut Client,
    task: &Id,
    density: TranscriptDensity,
    after_row: u32,
    limit: u32,
    as_of: u64,
) -> Result<TranscriptPage, ClientError> {
    let ack = c
        .command(envelope(
            fresh_id(),
            "GetTranscript",
            GetTranscript {
                task_id: Some(task.clone()),
                density: density as i32,
                after_row,
                limit,
                as_of_offset: as_of,
            }
            .encode_to_vec(),
        ))
        .await?;
    Ok(Client::result(&ack).unwrap())
}

async fn whole_transcript(c: &mut Client, task: &Id, density: TranscriptDensity) -> TranscriptPage {
    get_transcript(c, task, density, 0, 500, 0).await.unwrap()
}

async fn get_headers(c: &mut Client, session: &Id, include_archived: bool) -> AgentHeaders {
    let ack = c
        .command(envelope(
            fresh_id(),
            "GetAgentHeaders",
            GetAgentHeaders {
                session_id: Some(session.clone()),
                include_archived,
            }
            .encode_to_vec(),
        ))
        .await
        .unwrap();
    Client::result(&ack).unwrap()
}

fn header_of<'a>(h: &'a AgentHeaders, task: &Id) -> &'a AgentHeader {
    h.headers
        .iter()
        .find(|x| x.task_id.as_ref() == Some(task))
        .unwrap_or_else(|| panic!("no header for the task"))
}

fn kind_of(r: &TranscriptRow) -> TranscriptRowKind {
    TranscriptRowKind::try_from(r.kind).unwrap()
}

/// Every row of a view, groups flattened into their children, boundaries and
/// dividers left out: the atoms the view stands for.
fn atoms_of(rows: &[TranscriptRow]) -> Vec<String> {
    let mut out = Vec::new();
    for r in rows {
        match kind_of(r) {
            TranscriptRowKind::WorkGroup => out.extend(atoms_of(&r.children)),
            TranscriptRowKind::TimeBoundary | TranscriptRowKind::UnreadDivider => {}
            _ => out.push(r.row_id.clone()),
        }
    }
    out
}

fn rows_equal(a: &[TranscriptRow], b: &[TranscriptRow]) -> bool {
    a == b
}

fn assistant_texts(rows: &[TranscriptRow]) -> Vec<String> {
    rows.iter()
        .filter(|r| kind_of(r) == TranscriptRowKind::AssistantMessage)
        .map(|r| r.text.clone())
        .collect()
}

fn tool_names(rows: &[TranscriptRow]) -> Vec<String> {
    let mut out = Vec::new();
    for r in rows {
        match (&r.facts, kind_of(r)) {
            (Some(Facts::Tool(t)), _) => out.push(t.tool_name.clone()),
            (_, TranscriptRowKind::WorkGroup) => out.extend(tool_names(&r.children)),
            _ => {}
        }
    }
    out
}

async fn queue_input(c: &mut Client, task: &Id, lease: u64, text: &str) {
    let ack = c
        .command(envelope_fenced(
            fresh_id(),
            "QueueInput",
            QueueInput {
                task_id: Some(task.clone()),
                input_id: format!("in-{}", rand::random::<u32>()),
                mode: "COLLECT".into(),
                text: text.into(),
            }
            .encode_to_vec(),
            Some(lease),
        ))
        .await
        .unwrap();
    assert_eq!(
        ack.status,
        modbit_protocol::v1::CommandStatus::Accepted as i32
    );
}

/// The coding task end to end through the scripted model: a ReadyForReview
/// task with a real conversation behind it.
async fn finished_coding_task(
    core: &CoreProcess,
    c: &mut Client,
    session: &Id,
    lease: u64,
    root: &str,
) -> Id {
    let _ = core;
    let task = create_task(c, session, lease, "Reject negative quantities.", root).await;
    start_with(c, &task, lease, 0).await;
    let st = wait_idle(c, &task, 120).await;
    assert_eq!(st.state, "ReadyForReview", "{st:?}");
    task
}

/// The transcript of a finished coding task is the conversation, rebuilt from
/// the log: user message, assistant messages from their completion records,
/// tool cards with edit line counts, a footer per turn and a tail status; the
/// three densities hold the same atoms and differ only in grouping; pages and
/// cursors are exact; and after a SIGKILL and restart the same rows come back.
#[tokio::test]
async fn qual_px_042_the_transcript_reproduces_the_conversation_in_three_densities_and_after_a_restart()
 {
    let (_repo, root, hash) = coding_repo();
    let base = streaming_model(coding_steps(&hash)).await;
    let dir = tempfile::tempdir().unwrap();
    let mut core = CoreProcess::spawn_with_env(dir.path(), &env_for(&base, ""));
    let mut c = core.client().await;
    let (session, lease) = create_session(&mut c).await;
    let task = finished_coding_task(&core, &mut c, &session, lease, &root).await;
    tokio::time::sleep(SETTLE).await;

    let detailed = whole_transcript(&mut c, &task, TranscriptDensity::Detailed).await;
    let balanced = whole_transcript(&mut c, &task, TranscriptDensity::Balanced).await;
    let compact = whole_transcript(&mut c, &task, TranscriptDensity::Compact).await;
    let default = get_transcript(&mut c, &task, TranscriptDensity::Unspecified, 0, 0, 0)
        .await
        .unwrap();
    assert_eq!(
        default.density,
        TranscriptDensity::Compact as i32,
        "COMPACT is the default"
    );
    assert!(detailed.events_read > 0, "a transcript read loads the log");

    // Detailed: the conversation as it happened.
    let rows = &detailed.rows;
    assert_eq!(kind_of(&rows[0]), TranscriptRowKind::UserMessage);
    assert_eq!(rows[0].text, "Reject negative quantities.");
    assert_eq!(
        assistant_texts(rows),
        [
            "Reading the file first.",
            "Planning.",
            "The check failed; the file must say validated."
        ]
    );
    for r in rows
        .iter()
        .filter(|r| kind_of(r) == TranscriptRowKind::AssistantMessage)
    {
        let Some(Facts::Stream(s)) = &r.facts else {
            panic!("no stream facts")
        };
        assert_eq!(s.phase, "COMPLETED");
        assert!(r.hints.as_ref().unwrap().renderable);
        assert_eq!(r.hints.as_ref().unwrap().status, "COMPLETE");
        assert_eq!(
            r.text_ref, s.content_hash,
            "the whole text is the completed object"
        );
        assert_eq!(read_object(&mut c, &r.text_ref).await, r.text.as_bytes());
    }
    assert_eq!(
        tool_names(rows),
        [
            "fs.read",
            "fs.stat",
            "plan.update",
            "test.run",
            "change.apply",
            "test.run",
            "change.apply",
            "test.run"
        ]
    );
    let edits: Vec<&TranscriptRow> = rows
        .iter()
        .filter(|r| matches!(&r.facts, Some(Facts::Tool(t)) if t.tool_name == "change.apply"))
        .collect();
    assert_eq!(edits.len(), 2);
    for e in &edits {
        let h = e.hints.as_ref().unwrap();
        assert_eq!((h.lines_added, h.lines_removed), (1, 1), "edit line counts");
        let Some(Facts::Tool(t)) = &e.facts else {
            panic!()
        };
        assert_eq!(
            (t.tool_class.as_str(), t.state.as_str(), t.paths.clone()),
            ("EDIT", "SUCCEEDED", vec!["qty.txt".to_owned()])
        );
        assert_eq!(h.status, "SUCCEEDED");
        assert!(!t.arguments_ref.is_empty() && !t.result_ref.is_empty());
    }
    let reads = rows
        .iter()
        .filter(|r| matches!(&r.facts, Some(Facts::Tool(t)) if t.tool_class == "READ"))
        .count();
    assert_eq!(reads, 2);
    let footers: Vec<&TranscriptRow> = rows
        .iter()
        .filter(|r| kind_of(r) == TranscriptRowKind::TurnFooter)
        .collect();
    assert_eq!(footers.len(), 8);
    assert!(
        footers
            .iter()
            .all(|f| f.hints.as_ref().unwrap().status == "COMPLETED")
    );
    let last = rows.last().unwrap();
    assert_eq!(kind_of(last), TranscriptRowKind::TailStatus);
    let Some(Facts::Tail(t)) = &last.facts else {
        panic!()
    };
    assert_eq!(t.phase, "READY_FOR_REVIEW");
    // Ordinals are 1..n and offsets never go backwards.
    assert!(
        rows.iter()
            .enumerate()
            .all(|(i, r)| r.ordinal == i as u32 + 1)
    );

    // The densities hold the same atoms in the same order; they differ only
    // in grouping.
    let atoms = atoms_of(&detailed.rows);
    assert_eq!(atoms_of(&balanced.rows), atoms);
    assert_eq!(atoms_of(&compact.rows), atoms);
    fn groups(p: &TranscriptPage) -> Vec<&TranscriptRow> {
        p.rows
            .iter()
            .filter(|r| kind_of(r) == TranscriptRowKind::WorkGroup)
            .collect()
    }
    assert!(groups(&detailed).is_empty());
    // Balanced folds the two reads of the first turn, and nothing else.
    let bg = groups(&balanced);
    assert_eq!(bg.len(), 1);
    let Some(Facts::Group(g)) = &bg[0].facts else {
        panic!()
    };
    assert_eq!(
        (g.reads, g.steps, g.label.as_str()),
        (2, 2, "Explored 2 items")
    );
    assert_eq!(bg[0].children.len(), 2);
    // Compact folds every finished turn's steps under one "Worked for" row.
    let cg = groups(&compact);
    assert_eq!(cg.len(), 7, "seven turns acted; the eighth only completed");
    for g in &cg {
        let Some(Facts::Group(f)) = &g.facts else {
            panic!()
        };
        assert!(f.label.starts_with("Worked for"), "{}", f.label);
        assert!(!f.open);
    }
    assert_eq!(cg[0].children.len(), 2);
    assert!(compact.rows.len() < detailed.rows.len());
    let Some(Facts::Group(edit_group)) = &cg[3].facts else {
        panic!()
    };
    assert_eq!(
        (
            edit_group.edits,
            edit_group.lines_added,
            edit_group.lines_removed
        ),
        (1, 1, 1)
    );

    // Pages and cursors: pages of three rows, read at one consistent offset,
    // concatenate to the whole view with no gap and no duplicate.
    let pin = detailed.as_of_offset;
    let mut after = 0;
    let mut paged: Vec<TranscriptRow> = Vec::new();
    loop {
        let p = get_transcript(&mut c, &task, TranscriptDensity::Detailed, after, 3, pin)
            .await
            .unwrap();
        assert_eq!(p.total_rows, detailed.total_rows);
        assert!(p.rows.len() <= 3);
        paged.extend(p.rows.clone());
        if !p.has_more {
            assert_eq!(p.next_after_row, 0);
            break;
        }
        assert_eq!(p.next_after_row, paged.len() as u32);
        after = p.next_after_row;
    }
    assert!(rows_equal(&paged, &detailed.rows));
    // The log keeps growing; a page read at the pinned offset does not move.
    queue_input(&mut c, &task, lease, "one more thing").await;
    let pinned = get_transcript(&mut c, &task, TranscriptDensity::Detailed, 3, 3, pin)
        .await
        .unwrap();
    assert!(rows_equal(&pinned.rows, &detailed.rows[3..6]));
    let moved = whole_transcript(&mut c, &task, TranscriptDensity::Detailed).await;
    assert_eq!(moved.total_rows, detailed.total_rows + 1);
    assert_eq!(
        moved
            .rows
            .iter()
            .filter(|r| kind_of(r) == TranscriptRowKind::UserMessage)
            .count(),
        2
    );
    let queued = moved
        .rows
        .iter()
        .find(|r| r.row_id.starts_with("user:in:"))
        .unwrap();
    let Some(Facts::User(u)) = &queued.facts else {
        panic!()
    };
    assert_eq!(
        (u.source.as_str(), u.mode.as_str(), queued.text.as_str()),
        ("queued_input", "COLLECT", "one more thing")
    );
    // Refusals carry the Core's own codes.
    let err = get_transcript(
        &mut c,
        &task,
        TranscriptDensity::Detailed,
        detailed.total_rows + 5,
        3,
        pin,
    )
    .await
    .unwrap_err();
    assert!(
        matches!(err, ClientError::Rejected { ref code, .. } if code == "INVALID_CURSOR"),
        "{err:?}"
    );
    let err = get_transcript(
        &mut c,
        &task,
        TranscriptDensity::Detailed,
        0,
        3,
        moved.last_offset + 1000,
    )
    .await
    .unwrap_err();
    assert!(
        matches!(err, ClientError::Rejected { ref code, .. } if code == "INVALID_CURSOR"),
        "{err:?}"
    );
    let err = get_transcript(&mut c, &id16(0x77), TranscriptDensity::Detailed, 0, 3, 0)
        .await
        .unwrap_err();
    assert!(
        matches!(err, ClientError::Rejected { ref code, .. } if code == "UNKNOWN_TASK"),
        "{err:?}"
    );
    // The projection of a task of another session is refused.
    let mut other = envelope(
        fresh_id(),
        "GetTranscript",
        GetTranscript {
            task_id: Some(task.clone()),
            ..Default::default()
        }
        .encode_to_vec(),
    );
    other.session_id = Some(id16(0x66));
    let err = c.command(other).await.unwrap_err();
    assert!(
        matches!(err, ClientError::Rejected { ref code, .. } if code == "WRONG_SESSION"),
        "{err:?}"
    );

    // SIGKILL and restart: the same rows come back, byte for byte, at the
    // offset they were read at, in every density.
    let before: Vec<TranscriptPage> = {
        let mut v = Vec::new();
        for d in [
            TranscriptDensity::Detailed,
            TranscriptDensity::Balanced,
            TranscriptDensity::Compact,
        ] {
            v.push(get_transcript(&mut c, &task, d, 0, 500, pin).await.unwrap());
        }
        v
    };
    drop(c);
    core.kill();
    let core = CoreProcess::spawn_with_env(dir.path(), &env_for(&base, ""));
    let mut c = core.client().await;
    for (i, d) in [
        TranscriptDensity::Detailed,
        TranscriptDensity::Balanced,
        TranscriptDensity::Compact,
    ]
    .into_iter()
    .enumerate()
    {
        let after = get_transcript(&mut c, &task, d, 0, 500, pin).await.unwrap();
        assert!(
            rows_equal(&after.rows, &before[i].rows),
            "{d:?} differs after the restart"
        );
        assert_eq!(after.total_rows, before[i].total_rows);
    }
}

/// An aborted stream is a partial message and says so: a cancelled turn, and a
/// Core killed mid-stream, leave rows whose phase is ABORTED with their typed
/// source, not renderable as messages; while streaming the row is STREAMING
/// with the text so far and the tail says the agent is responding.
#[tokio::test]
async fn qual_px_042_a_partial_message_is_never_presented_as_final_text() {
    let (_, chunks) = four_hundred_words();
    let base = streaming_model(vec![Step {
        chunks: chunks[..20].to_vec(),
        gap_ms: 5,
        end: End::Hang,
        ..Step::default()
    }])
    .await;
    let (_repo, root) = repo();
    let dir = tempfile::tempdir().unwrap();
    let mut core = CoreProcess::spawn_with_env(dir.path(), &env_for(&base, ""));
    let mut c = core.client().await;
    let (session, lease) = create_session(&mut c).await;
    let task = create_task(&mut c, &session, lease, "answer at length", &root).await;
    start_with(&mut c, &task, lease, 1).await;
    // While the model is mid-sentence.
    let deadline = Instant::now() + Duration::from_secs(120);
    let live = loop {
        let p = whole_transcript(&mut c, &task, TranscriptDensity::Detailed).await;
        if p.rows
            .iter()
            .any(|r| kind_of(r) == TranscriptRowKind::AssistantMessage && !r.text.is_empty())
        {
            break p;
        }
        assert!(Instant::now() < deadline, "no streaming row");
        tokio::time::sleep(Duration::from_millis(50)).await;
    };
    let streaming = live
        .rows
        .iter()
        .find(|r| kind_of(r) == TranscriptRowKind::AssistantMessage)
        .unwrap();
    let Some(Facts::Stream(s)) = &streaming.facts else {
        panic!()
    };
    assert_eq!(s.phase, "OPEN");
    assert_eq!(streaming.hints.as_ref().unwrap().status, "STREAMING");
    assert!(
        streaming.text.starts_with("word0 word1"),
        "{}",
        streaming.text
    );
    assert!(streaming.text_ref.is_empty(), "no completed object yet");
    let Some(Facts::Tail(t)) = &live.rows.last().unwrap().facts else {
        panic!()
    };
    assert_eq!(
        (t.phase.as_str(), t.label.as_str()),
        ("RUNNING", "Responding")
    );
    // The Core dies mid-stream.
    drop(c);
    core.kill();
    let core = CoreProcess::spawn_with_env(dir.path(), &env_for(&base, ""));
    let mut c = core.client().await;
    let p = whole_transcript(&mut c, &task, TranscriptDensity::Detailed).await;
    let msgs: Vec<&TranscriptRow> = p
        .rows
        .iter()
        .filter(|r| kind_of(r) == TranscriptRowKind::AssistantMessage)
        .collect();
    assert_eq!(msgs.len(), 1);
    let Some(Facts::Stream(s)) = &msgs[0].facts else {
        panic!()
    };
    assert_eq!(
        (
            s.phase.as_str(),
            s.abort_source.as_str(),
            s.abort_code.as_str()
        ),
        ("ABORTED", "RECOVERY", "ABORTED_BY_RECOVERY")
    );
    let h = msgs[0].hints.as_ref().unwrap();
    assert!(!h.renderable, "a client does not draw it as a message");
    assert_eq!(h.status, "ABORTED");
    assert!(
        msgs[0].text.starts_with("word0"),
        "the partial text stays readable"
    );
    assert!(msgs[0].text_ref.is_empty(), "there is no completed text");
    assert!(
        p.rows
            .iter()
            .all(|r| r.hints.as_ref().is_none_or(|h| h.status != "COMPLETE")
                || kind_of(r) != TranscriptRowKind::AssistantMessage)
    );
}

/// Read, unread, archive and undo are Core events: idempotent under a repeated
/// command id, reversible, visible in the headers and as an unread divider in
/// the transcript; a running task is not archived.
#[tokio::test]
async fn qual_px_042_read_markers_and_archive_are_idempotent_reversible_core_events() {
    let (_repo, root, hash) = coding_repo();
    let mut steps = coding_steps(&hash);
    // The second task of the session never finishes its first turn.
    steps.push(Step {
        chunks: vec!["Thinking about it.".into()],
        end: End::Hang,
        ..Step::default()
    });
    let base = streaming_model(steps).await;
    let dir = tempfile::tempdir().unwrap();
    let core = CoreProcess::spawn_with_env(dir.path(), &env_for(&base, ""));
    let mut c = core.client().await;
    let (session, lease) = create_session(&mut c).await;
    let task = finished_coding_task(&core, &mut c, &session, lease, &root).await;
    tokio::time::sleep(SETTLE).await;

    let h = get_headers(&mut c, &session, false).await;
    let a = header_of(&h, &task);
    assert_eq!(
        a.status_class,
        AgentStatusClass::ReadyForReviewUnseen as i32
    );
    assert!(a.unread);
    assert_eq!(a.read_offset, 0);

    let mark = |up_to: u64| {
        envelope_fenced(
            id16(0x51),
            "MarkRead",
            MarkRead {
                task_id: Some(task.clone()),
                up_to_offset: up_to,
            }
            .encode_to_vec(),
            Some(lease),
        )
    };
    let ack = c.command(mark(0)).await.unwrap();
    assert_eq!(
        ack.status,
        modbit_protocol::v1::CommandStatus::Accepted as i32
    );
    let marked: ReadMarked = Client::result(&ack).unwrap();
    assert!(marked.offset > 0 && marked.read_offset > 0);
    // The same command id replays; one event, one outcome.
    let again = c.command(mark(0)).await.unwrap();
    assert_eq!(
        again.status,
        modbit_protocol::v1::CommandStatus::Replayed as i32
    );
    let replayed: ReadMarked = Client::result(&again).unwrap();
    assert_eq!(replayed, marked);
    // Marking what is already read records nothing.
    let ack = c
        .command(envelope_fenced(
            fresh_id(),
            "MarkRead",
            MarkRead {
                task_id: Some(task.clone()),
                up_to_offset: 1,
            }
            .encode_to_vec(),
            Some(lease),
        ))
        .await
        .unwrap();
    assert_eq!(Client::result::<ReadMarked>(&ack).unwrap().offset, 0);
    // A marker beyond the log is refused; a stale lease is fenced like any command.
    let err = c
        .command(envelope_fenced(
            fresh_id(),
            "MarkRead",
            MarkRead {
                task_id: Some(task.clone()),
                up_to_offset: 10_000_000,
            }
            .encode_to_vec(),
            Some(lease),
        ))
        .await
        .unwrap_err();
    assert!(
        matches!(err, ClientError::Rejected { ref code, .. } if code == "INVALID_CURSOR"),
        "{err:?}"
    );
    let err = c
        .command(envelope(
            fresh_id(),
            "MarkRead",
            MarkRead {
                task_id: Some(task.clone()),
                up_to_offset: 0,
            }
            .encode_to_vec(),
        ))
        .await
        .unwrap_err();
    assert!(
        matches!(err, ClientError::Rejected { ref code, .. } if code == "LEASE_REQUIRED"),
        "{err:?}"
    );

    let h = get_headers(&mut c, &session, false).await;
    let a = header_of(&h, &task);
    assert_eq!(a.status_class, AgentStatusClass::ReadyForReviewSeen as i32);
    assert!(!a.unread);
    assert_eq!(a.read_offset, marked.read_offset);
    // New rows after the marker sit behind an unread divider.
    queue_input(&mut c, &task, lease, "a follow-up").await;
    let p = whole_transcript(&mut c, &task, TranscriptDensity::Detailed).await;
    assert_eq!(p.read_offset, marked.read_offset);
    let at = p
        .rows
        .iter()
        .position(|r| kind_of(r) == TranscriptRowKind::UnreadDivider)
        .expect("an unread divider");
    assert_eq!(kind_of(&p.rows[at + 1]), TranscriptRowKind::UserMessage);
    assert_eq!(p.rows[at].text, "1 new");
    assert!(p.rows[..at].iter().all(|r| r.offset <= marked.read_offset));

    // Archive: gone from the list, back with include_archived, undone by the
    // inverse; a repeated command id changes nothing.
    let archive = |id: u8, archived: bool, task: &Id| {
        envelope_fenced(
            id16(id),
            "ArchiveTask",
            ArchiveTask {
                task_id: Some(task.clone()),
                archived,
            }
            .encode_to_vec(),
            Some(lease),
        )
    };
    let ack = c.command(archive(0x52, true, &task)).await.unwrap();
    let done: TaskArchived = Client::result(&ack).unwrap();
    assert!(done.archived && done.offset > 0);
    let ack = c.command(archive(0x52, true, &task)).await.unwrap();
    assert_eq!(
        ack.status,
        modbit_protocol::v1::CommandStatus::Replayed as i32
    );
    let ack = c.command(archive(0x53, true, &task)).await.unwrap();
    assert_eq!(
        Client::result::<TaskArchived>(&ack).unwrap().offset,
        0,
        "already archived"
    );
    assert!(
        get_headers(&mut c, &session, false)
            .await
            .headers
            .is_empty()
    );
    let h = get_headers(&mut c, &session, true).await;
    assert!(header_of(&h, &task).archived);
    let ack = c.command(archive(0x54, false, &task)).await.unwrap();
    assert!(!Client::result::<TaskArchived>(&ack).unwrap().archived);
    let h = get_headers(&mut c, &session, false).await;
    assert!(!header_of(&h, &task).archived);
    let all = replay(&core, &session, 0).await;
    assert_eq!(
        of_kind(&all, "ConversationArchived").len(),
        1,
        "one event per archive"
    );
    assert_eq!(of_kind(&all, "ConversationUnarchived").len(), 1);
    assert_eq!(of_kind(&all, "ConversationRead").len(), 1);
    assert!(
        all.iter()
            .filter(|e| e.aggregate == "conversation")
            .all(|e| e.aggregate_id != task.value)
    );

    // A running task is not archived; the client cancels it first.
    let running = create_task(&mut c, &session, lease, "think", &root).await;
    start_with(&mut c, &running, lease, 1).await;
    let deadline = Instant::now() + Duration::from_secs(120);
    while task_status(&mut c, &running).await.state != "Running" {
        assert!(Instant::now() < deadline);
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let err = c.command(archive(0x55, true, &running)).await.unwrap_err();
    assert!(
        matches!(err, ClientError::Rejected { ref code, .. } if code == "TASK_RUNNING"),
        "{err:?}"
    );
    // Read markers and archives survive a restart (they are on the log).
    let mut core = core;
    drop(c);
    core.kill();
    let core = CoreProcess::spawn_with_env(dir.path(), &env_for(&base, ""));
    let mut c = core.client().await;
    let h = get_headers(&mut c, &session, true).await;
    assert_eq!(header_of(&h, &task).read_offset, marked.read_offset);
}

/// The agent list: a header per task with the status class decided by the
/// Core in the precedence of AFW-B02, edit totals and unread from the log,
/// served for 200 tasks without loading a transcript or an object.
#[tokio::test]
async fn qual_px_042_headers_for_two_hundred_tasks_carry_core_decided_classes_and_load_no_transcript()
 {
    let (_repo, root, hash) = coding_repo();
    let mut steps = coding_steps(&hash);
    // Request 8: a turn of words and no action, which ends a run that may
    // not stall; request 9: a model that never finishes.
    steps.push(Step::text(vec!["I am not sure what to do next.".into()], 0));
    steps.push(Step {
        chunks: vec!["Working on it.".into()],
        end: End::Hang,
        ..Step::default()
    });
    let base = streaming_model(steps).await;
    let dir = tempfile::tempdir().unwrap();
    let core = CoreProcess::spawn_with_env(dir.path(), &env_for(&base, ""));
    let mut c = core.client().await;
    let (session, lease) = create_session(&mut c).await;
    // 195 drafts, one of them cancelled.
    let mut drafts = Vec::new();
    for i in 0..195 {
        drafts.push(create_task(&mut c, &session, lease, &format!("draft {i}"), "").await);
    }
    let cancelled = drafts[0].clone();
    c.command(envelope_fenced(
        fresh_id(),
        "CancelTask",
        CancelTask {
            task_id: Some(cancelled.clone()),
        }
        .encode_to_vec(),
        Some(lease),
    ))
    .await
    .unwrap();
    // Ready for review (unread), needs attention, running.
    let ready = finished_coding_task(&core, &mut c, &session, lease, &root).await;
    let stalled = create_task(&mut c, &session, lease, "decide something", &root).await;
    start_with(&mut c, &stalled, lease, 1).await;
    let st = wait_idle(&mut c, &stalled, 60).await;
    assert_eq!(st.state, "Waiting", "{st:?}");
    let running = create_task(&mut c, &session, lease, "keep going", &root).await;
    start_with(&mut c, &running, lease, 1).await;
    let deadline = Instant::now() + Duration::from_secs(120);
    while task_status(&mut c, &running).await.state != "Running" {
        assert!(Instant::now() < deadline);
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    // One more draft, archived: the last class of the precedence.
    let archived = create_task(&mut c, &session, lease, "put away", "").await;
    c.command(envelope_fenced(
        fresh_id(),
        "ArchiveTask",
        ArchiveTask {
            task_id: Some(archived.clone()),
            archived: true,
        }
        .encode_to_vec(),
        Some(lease),
    ))
    .await
    .unwrap();
    tokio::time::sleep(SETTLE).await;

    let started = Instant::now();
    let h = get_headers(&mut c, &session, true).await;
    let took = started.elapsed();
    assert_eq!(h.headers.len(), 195 + 4);
    let class = |t: &Id| AgentStatusClass::try_from(header_of(&h, t).status_class).unwrap();
    assert_eq!(class(&ready), AgentStatusClass::ReadyForReviewUnseen);
    assert_eq!(class(&stalled), AgentStatusClass::NeedsAttention);
    assert!(header_of(&h, &stalled).attention_items >= 1);
    assert_eq!(class(&running), AgentStatusClass::Running);
    assert_eq!(class(&drafts[1]), AgentStatusClass::Draft);
    assert_eq!(header_of(&h, &drafts[1]).status_label, "Draft");
    assert_eq!(class(&cancelled), AgentStatusClass::Completed);
    assert_eq!(header_of(&h, &cancelled).status_label, "Cancelled");
    assert_eq!(class(&archived), AgentStatusClass::Archived);
    assert!(header_of(&h, &archived).archived);
    // The stalled task is both Waiting and in need of attention: the class
    // with precedence wins, and the state is still reported as it is.
    assert_eq!(header_of(&h, &stalled).task_state, "Waiting");
    // Facts of the finished task, from the log.
    let r = header_of(&h, &ready);
    assert_eq!((r.files_changed, r.lines_added, r.lines_removed), (1, 2, 2));
    assert_eq!(r.title, "Reject negative quantities.");
    assert_eq!(r.origin, "cli");
    assert_eq!(r.execution_location, "local");
    assert!(!r.subtitle.is_empty() && root.ends_with(&r.subtitle));
    assert!(r.unread && !r.archived && !r.pending_approval && !r.subagent);
    assert!(r.last_offset > r.read_offset);
    // The read loaded no transcript: no object at all, and only the few task
    // events attention derives its class from; far fewer than one transcript.
    let transcript = whole_transcript(&mut c, &ready, TranscriptDensity::Detailed).await;
    assert_eq!(h.objects_read, 0);
    assert!(
        h.events_read < transcript.events_read / 2,
        "headers loaded {} events, one transcript {}",
        h.events_read,
        transcript.events_read
    );
    assert!(took < Duration::from_secs(5), "{took:?}");
    // Newest activity first.
    let times: Vec<i64> = h
        .headers
        .iter()
        .map(|x| {
            x.updated_at
                .as_ref()
                .map_or(0, |t| t.seconds * 1000 + i64::from(t.nanos) / 1_000_000)
        })
        .collect();
    assert!(times.windows(2).all(|w| w[0] >= w[1]));
    // The default list leaves the archived one out; reading is not writing:
    // the header read appended nothing.
    let plain = get_headers(&mut c, &session, false).await;
    assert_eq!(plain.headers.len(), 195 + 3);
    assert_eq!(plain.last_offset, h.last_offset);
    // After a restart the same classes come from the log alone.
    drop(c);
    let mut core = core;
    core.kill();
    let core = CoreProcess::spawn_with_env(dir.path(), &env_for(&base, ""));
    let mut c = core.client().await;
    let h2 = get_headers(&mut c, &session, true).await;
    assert_eq!(h2.headers.len(), h.headers.len());
    assert_eq!(
        AgentStatusClass::try_from(header_of(&h2, &ready).status_class).unwrap(),
        AgentStatusClass::ReadyForReviewUnseen
    );
    assert_eq!(
        AgentStatusClass::try_from(header_of(&h2, &stalled).status_class).unwrap(),
        AgentStatusClass::NeedsAttention
    );
    // The task that was running when the Core died now needs the person
    // (recovery suspends it): attention outranks running.
    assert_eq!(
        AgentStatusClass::try_from(header_of(&h2, &running).status_class).unwrap(),
        AgentStatusClass::NeedsAttention
    );
}
