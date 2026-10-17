//! The live-gateway legs of the Core qualifications (docs/62 QUAL-PX-*): the
//! clauses that name "the live gateway" and that the scripted real-boundary
//! suites cannot close. Each leg drives the REAL `modbit-core` binary over its
//! real socket, SQLite log and git repository, with the REAL provider adapter
//! pointed at the gateway the repository variables name (DR-M9-002), and
//! asserts the QUAL's own acceptance condition on what the Core recorded.
//!
//! * `live_*` tests are `#[ignore]`d unless run with `--ignored` and
//!   `MODBIT_LIVE=1`; they REFUSE (fail) without the credential, the model, a
//!   priced catalog entry and a spend cap. They never skip as a pass. The
//!   `live-core` workflow (`.github/workflows/live-core.yml`) runs them under a
//!   hard spend cap and uploads the result records.
//! * `scripted_*` tests run the very same leg code against a scripted
//!   OpenAI-compatible server in the normal CI. That proves the leg's
//!   mechanics (its commands, its assertions, its record); it says nothing
//!   about a model, and its record says `live: false`.
//!
//! A live leg that fails is a finding about the product or about what the
//! gateway can give; it is reported, never tuned away.
#![cfg(unix)]

mod live_support;
mod px_common;

use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use live_support::{Leg, LiveEnv, percentile, word_count};
use modbit_protocol::client::{Client, ClientError};
use modbit_protocol::v1::{
    ContextAccountingView, GetContextAccounting, GetTaskEconomics, GetTaskStatus, Id,
    ObjectRangeChunk, ReadObjectRange, StartTask, TaskCreated, TaskEconomicsView, TaskRunStarted,
    TaskStatus,
};
use prost::Message;
use px_common::*;
use serde_json::{Value, json};
use sha2::Digest;

// ---------------------------------------------------------------------------
// The fixture: a Core on the live environment, a repository, a session.
// ---------------------------------------------------------------------------

/// A Core process on a fresh profile with a git repository and a session
/// whose lease this fixture holds.
struct Fx {
    env: Arc<LiveEnv>,
    data: tempfile::TempDir,
    _repo: tempfile::TempDir,
    root: String,
    core: CoreProcess,
    c: Client,
    session: Id,
    g: Option<u64>,
    extra_env: Vec<(String, String)>,
}

impl Fx {
    async fn new(env: &Arc<LiveEnv>, files: &[(&str, &str)]) -> Self {
        Self::with_env(env, files, &[]).await
    }

    async fn with_env(env: &Arc<LiveEnv>, files: &[(&str, &str)], extra: &[(&str, &str)]) -> Self {
        let (repo, root) = plain_repo(files);
        let data = tempfile::tempdir().unwrap();
        let extra_env: Vec<(String, String)> = extra
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect();
        let core = Self::spawn(env, data.path(), &extra_env);
        let mut c = core.client().await;
        let (session, g) = session_with_lease(&mut c, 0x10).await;
        Self {
            env: Arc::clone(env),
            data,
            _repo: repo,
            root,
            core,
            c,
            session,
            g,
            extra_env,
        }
    }

    fn spawn(env: &LiveEnv, data: &Path, extra: &[(String, String)]) -> CoreProcess {
        let mut pairs = env.core_env_refs();
        pairs.extend(extra.iter().map(|(k, v)| (k.as_str(), v.as_str())));
        CoreProcess::spawn_with_env(data, &pairs)
    }

    /// SIGKILL the Core and start another on the same profile; the lease is
    /// taken again (a new generation).
    async fn restart(&mut self) {
        self.core.kill();
        self.core = Self::spawn(&self.env, self.data.path(), &self.extra_env);
        self.c = self.core.client().await;
        self.g = Some(self.lease_again().await);
    }

    async fn lease_again(&mut self) -> u64 {
        use modbit_protocol::v1::{AcquireSessionLease, SessionLeaseAcquired};
        let ack = self
            .c
            .command(envelope(
                rand_id(),
                "AcquireSessionLease",
                AcquireSessionLease {
                    session_id: Some(self.session.clone()),
                    owner: "live-core".into(),
                }
                .encode_to_vec(),
            ))
            .await
            .unwrap();
        Client::result::<SessionLeaseAcquired>(&ack)
            .unwrap()
            .lease_generation
    }

    /// A command's decoded result, or its typed refusal as `(code, message)`.
    async fn cmd<R: Message + Default>(
        &mut self,
        command_type: &str,
        msg: impl Message,
        fenced: bool,
    ) -> Result<R, (String, String)> {
        let g = if fenced { self.g } else { None };
        match self
            .c
            .command(envelope_fenced(
                rand_id(),
                command_type,
                msg.encode_to_vec(),
                g,
            ))
            .await
        {
            Ok(ack) => Ok(Client::result(&ack).unwrap()),
            Err(ClientError::Rejected { code, message }) => Err((code, message)),
            Err(e) => panic!("{command_type}: {e}"),
        }
    }

    async fn ok<R: Message + Default>(
        &mut self,
        command_type: &str,
        msg: impl Message,
        fenced: bool,
    ) -> R {
        match self.cmd(command_type, msg, fenced).await {
            Ok(r) => r,
            Err((code, why)) => panic!("{command_type} was refused: {code}: {why}"),
        }
    }

    async fn task(&mut self, goal: &str) -> Id {
        self.task_in_mode(goal, 0).await
    }

    async fn task_in_mode(&mut self, goal: &str, mode: i32) -> Id {
        let created: TaskCreated = self
            .ok(
                "CreateTask",
                modbit_protocol::v1::CreateTask {
                    session_id: Some(self.session.clone()),
                    goal_text: goal.into(),
                    origin: "cli".into(),
                    workspace_root: self.root.clone(),
                    execution_profile: "local_trusted".into(),
                    mode,
                    ..Default::default()
                },
                true,
            )
            .await;
        created.task_id.unwrap()
    }

    /// Start (or resume) the task's loop on the live model.
    async fn start(&mut self, task: &Id, max_turns: u32, no_progress: u32) -> TaskRunStarted {
        let model = self.env.model.clone();
        self.ok(
            "StartTask",
            StartTask {
                task_id: Some(task.clone()),
                endpoint: String::new(),
                model,
                max_turns,
                max_tool_calls: 0,
                max_no_progress_turns: no_progress,
                skills: vec![],
                ..Default::default()
            },
            true,
        )
        .await
    }

    async fn status(&mut self, task: &Id) -> TaskStatus {
        self.ok(
            "GetTaskStatus",
            GetTaskStatus {
                task_id: Some(task.clone()),
            },
            false,
        )
        .await
    }

    /// Wait until the task's loop has ended (or `secs` pass: the status then
    /// still says the loop is alive and the caller's check fails with it).
    async fn wait(&mut self, task: &Id, secs: u64) -> TaskStatus {
        let deadline = Instant::now() + Duration::from_secs(secs);
        loop {
            let st = self.status(task).await;
            if !st.loop_alive || Instant::now() > deadline {
                return st;
            }
            tokio::time::sleep(Duration::from_millis(150)).await;
        }
    }

    async fn economics(&mut self, task: &Id) -> TaskEconomicsView {
        self.ok(
            "GetTaskEconomics",
            GetTaskEconomics {
                task_id: Some(task.clone()),
            },
            false,
        )
        .await
    }

    async fn accounting(&mut self, task: &Id) -> ContextAccountingView {
        self.ok(
            "GetContextAccounting",
            GetContextAccounting {
                task_id: Some(task.clone()),
            },
            false,
        )
        .await
    }

    /// Charge a task's usage (the gateway's own usage frames) to the leg.
    async fn charge(&mut self, leg: &mut Leg, task: &Id) -> TaskEconomicsView {
        let e = self.economics(task).await;
        leg.charge(u64::from(e.model_calls), e.input_tokens, e.output_tokens);
        e
    }

    async fn read_object(&mut self, hash: &str) -> Vec<u8> {
        let mut out = Vec::new();
        loop {
            let chunk: ObjectRangeChunk = self
                .ok(
                    "ReadObjectRange",
                    ReadObjectRange {
                        object_hash: hash.to_owned(),
                        offset: out.len() as u64,
                        length: 1024 * 1024,
                    },
                    false,
                )
                .await;
            if chunk.data.is_empty() {
                return out;
            }
            out.extend_from_slice(&chunk.data);
            if out.len() as u64 >= chunk.total_bytes {
                return out;
            }
        }
    }

    async fn events(&self) -> Vec<Ev> {
        replay_ev(&self.core, &self.session, 0).await
    }
}

// ---- the events a client received ----

/// One event as a client received it.
#[derive(Clone, Debug)]
struct Ev {
    offset: u64,
    kind: String,
    sequence: u64,
    payload: Value,
    recorded_ms: i64,
    aggregate: Vec<u8>,
    task: Option<String>,
    /// When this process received it (ms since the epoch); 0 for a replay.
    received_ms: i64,
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
}

fn ev_of(e: modbit_protocol::v1::StoredEventFrame) -> Ev {
    let ev = e.event.unwrap();
    let p: Value = serde_json::from_slice(&ev.payload).unwrap_or_default();
    let at = ev.recorded_at.unwrap_or_default();
    Ev {
        offset: e.offset,
        kind: ev.event_type,
        sequence: ev.sequence,
        payload: p["payload"].clone(),
        recorded_ms: at.seconds * 1000 + i64::from(at.nanos) / 1_000_000,
        aggregate: ev.aggregate_id.map(|i| i.value).unwrap_or_default(),
        task: ev.task_id.map(|t| hex::encode(t.value)),
        received_ms: now_ms(),
    }
}

/// Every event of the session after `after`, replayed by cursor.
async fn replay_ev(core: &CoreProcess, session: &Id, after: u64) -> Vec<Ev> {
    use modbit_protocol::v1::{GetSessionSnapshot, SessionSnapshot};
    let mut s = core.client().await;
    let ack = s
        .command(envelope(
            rand_id(),
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
    let deadline = Instant::now() + Duration::from_secs(120);
    loop {
        let seen = out.last().map_or(after, |e| e.offset);
        let wait = if seen < floor {
            deadline.saturating_duration_since(Instant::now())
        } else {
            Duration::from_millis(400)
        };
        let Ok(Ok(Some(e))) = tokio::time::timeout(wait, s.next_event()).await else {
            break;
        };
        out.push(ev_of(e));
    }
    out
}

/// A live subscriber: everything after `after`, collected as it arrives.
async fn collect_live(core: &CoreProcess, session: &Id, after: u64) -> Arc<Mutex<Vec<Ev>>> {
    let out = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&out);
    let session = session.clone();
    let mut c = core.client().await;
    tokio::spawn(async move {
        if c.subscribe(session, after).await.is_err() {
            return;
        }
        while let Ok(Some(e)) = c.next_event().await {
            sink.lock().unwrap().push(ev_of(e));
        }
    });
    out
}

async fn until_events(
    feed: &Arc<Mutex<Vec<Ev>>>,
    secs: u64,
    what: &str,
    pred: impl Fn(&[Ev]) -> bool,
) -> Vec<Ev> {
    let deadline = Instant::now() + Duration::from_secs(secs);
    loop {
        let ev = feed.lock().unwrap().clone();
        if pred(&ev) {
            return ev;
        }
        assert!(
            Instant::now() < deadline,
            "timed out waiting for {what}; the log held {:?}",
            ev.iter().map(|e| e.kind.as_str()).collect::<Vec<_>>()
        );
        tokio::time::sleep(Duration::from_millis(40)).await;
    }
}

fn of_kind<'a>(events: &'a [Ev], kind: &str) -> Vec<&'a Ev> {
    events.iter().filter(|e| e.kind == kind).collect()
}

/// The text of one stream's deltas in order; panics unless they are contiguous.
fn stream_text(deltas: &[&Ev]) -> String {
    let mut out = String::new();
    for (i, d) in deltas.iter().enumerate() {
        assert_eq!(d.sequence, i as u64 + 1, "the stream's deltas are contiguous");
        out.push_str(d.payload["text"].as_str().unwrap_or_default());
    }
    out
}

/// The stream with the most deltas: the long answer, whatever tool calls the
/// model made around it.
fn longest_stream(events: &[Ev]) -> Option<Vec<u8>> {
    let mut counts: std::collections::HashMap<Vec<u8>, usize> = std::collections::HashMap::new();
    for d in of_kind(events, "AssistantTextDelta") {
        *counts.entry(d.aggregate.clone()).or_default() += 1;
    }
    counts.into_iter().max_by_key(|(_, n)| *n).map(|(k, _)| k)
}

fn payload_str(e: &Ev, key: &str) -> String {
    e.payload[key].as_str().unwrap_or_default().to_owned()
}

// ---------------------------------------------------------------------------
// QUAL-PX-041: the streamed answer
// ---------------------------------------------------------------------------

const SHAPED_SECRET: &str = "ghp_abcdefghijklmnopqrstuvwxyz0123";

/// A 400-word answer through the Core on the live gateway: strictly
/// increasing delta events inside the 50 ms / 2 KiB bounds, their
/// concatenation equal to the completed message (digest and object), a client
/// dropped mid-stream resuming by cursor with no gap and no duplicate, a
/// planted secret never in a delta or the completed record, and the
/// Core-to-client latency of each delta.
async fn leg_px041_stream(env: Arc<LiveEnv>) {
    let mut leg = Leg::begin(&env, "px041", "stream", 0.10);
    let mut fx = Fx::new(&env, &[("notes.txt", "alpha\nbeta\n")]).await;
    let goal = format!(
        "Write an explanation of about 400 words of how a hash table resolves collisions: open addressing and separate chaining, with the trade-offs. Plain prose, no headings, no lists, and do not call any tool. End the answer with one last line that repeats this test string exactly as written: {SHAPED_SECRET}"
    );
    let task = fx.task(&goal).await;
    // Two subscribers from the start: one that stays, one that will be dropped
    // mid-stream.
    let stay = collect_live(&fx.core, &fx.session, 0).await;
    let dropped: Arc<Mutex<Vec<Ev>>> = Arc::new(Mutex::new(Vec::new()));
    {
        let sink = Arc::clone(&dropped);
        let mut dc = fx.core.client().await;
        dc.subscribe(fx.session.clone(), 0).await.unwrap();
        tokio::spawn(async move {
            // Read until the second delta has arrived, then drop the connection.
            let mut deltas = 0;
            while let Ok(Some(e)) = dc.next_event().await {
                let ev = ev_of(e);
                if ev.kind == "AssistantTextDelta" {
                    deltas += 1;
                }
                sink.lock().unwrap().push(ev);
                if deltas >= 2 {
                    return;
                }
            }
        });
    }
    let started = Instant::now();
    fx.start(&task, 4, 1).await;
    let st = fx.wait(&task, 240).await;
    leg.check("run_ended", !st.loop_alive, format!("{st:?}"));
    tokio::time::sleep(Duration::from_millis(600)).await;
    let live = stay.lock().unwrap().clone();
    let all = fx.events().await;
    let stream_id = longest_stream(&all).expect("the model produced no streamed text at all");
    let deltas: Vec<&Ev> = of_kind(&all, "AssistantTextDelta")
        .into_iter()
        .filter(|d| d.aggregate == stream_id)
        .collect();
    let done: Vec<&Ev> = of_kind(&all, "AssistantMessageCompleted")
        .into_iter()
        .filter(|d| d.aggregate == stream_id)
        .collect();
    leg.check("one_completion", done.len() == 1, format!("{} completions", done.len()));
    leg.check(
        "not_aborted",
        of_kind(&all, "AssistantMessageAborted").is_empty(),
        "a live stream aborted",
    );
    leg.check("streamed_in_pieces", deltas.len() >= 2, format!("{} deltas", deltas.len()));
    // Bounds: size, order, rate.
    let max_bytes = deltas
        .iter()
        .map(|d| d.payload["text"].as_str().unwrap_or_default().len())
        .max()
        .unwrap_or(0);
    leg.check("delta_size_bound", max_bytes <= 2048, format!("largest delta {max_bytes} bytes"));
    leg.check(
        "strictly_increasing",
        deltas.windows(2).all(|w| w[0].offset < w[1].offset),
        "offsets",
    );
    let text = stream_text(&deltas);
    let span_ms = (deltas.last().unwrap().recorded_ms - deltas[0].recorded_ms).max(0) as usize;
    let allowed = span_ms / 50 + 2 + text.len() / 2048;
    leg.check(
        "rate_bound",
        deltas.len() <= allowed,
        format!("{} deltas in {span_ms} ms (at most {allowed})", deltas.len()),
    );
    leg.check(
        "delta_before_completion",
        deltas.iter().all(|d| d.offset < done[0].offset),
        "every delta precedes the completion",
    );
    // The concatenation is the completed message.
    let digest = hex::encode(sha2::Sha256::digest(text.as_bytes()));
    let p = &done[0].payload;
    leg.check("delta_count_matches", p["delta_count"] == deltas.len() as u64, format!("{}", p["delta_count"]));
    leg.check("content_hash_matches", p["content_hash"] == digest, format!("{} vs {digest}", p["content_hash"]));
    let object = fx.read_object(&digest).await;
    leg.check("object_is_the_concatenation", object == text.as_bytes(), format!("{} bytes", object.len()));
    // A client dropped mid-stream resumes by cursor: its events plus the
    // replay after its last offset are exactly the log, no gap, no duplicate.
    let first_part = dropped.lock().unwrap().clone();
    let cut = first_part.last().map_or(0, |e| e.offset);
    let saw_completion = first_part.iter().any(|e| e.kind == "AssistantMessageCompleted");
    leg.check(
        "client_dropped_before_completion",
        !saw_completion && !first_part.is_empty(),
        format!("{} events before the drop", first_part.len()),
    );
    let resumed = replay_ev(&fx.core, &fx.session, cut).await;
    let mut joined: Vec<u64> = first_part.iter().map(|e| e.offset).collect();
    joined.extend(resumed.iter().map(|e| e.offset));
    let whole: Vec<u64> = all.iter().map(|e| e.offset).collect();
    leg.check(
        "resume_has_no_gap_and_no_duplicate",
        joined == whole,
        format!("{} + {} events vs {} in the log", first_part.len(), resumed.len(), whole.len()),
    );
    let live_offsets: Vec<u64> = live.iter().map(|e| e.offset).collect();
    leg.check("live_view_equals_log", live_offsets == whole, format!("{} vs {}", live_offsets.len(), whole.len()));
    // Core-to-client latency of each delta (event timestamp to receipt on the
    // same host). The render in the packaged app is not measured here.
    let lat: Vec<u64> = live
        .iter()
        .filter(|e| e.kind == "AssistantTextDelta" && e.aggregate == stream_id)
        .map(|e| (e.received_ms - e.recorded_ms).max(0) as u64)
        .collect();
    let p95 = percentile(&lat, 95.0);
    leg.count("latency_core_to_client_p95_ms", p95);
    leg.count("latency_core_to_client_max_ms", lat.iter().copied().max().unwrap_or(0));
    leg.check("latency_p95_within_100ms", p95 <= 100, format!("p95 {p95} ms over {} deltas", lat.len()));
    // The planted secret: never in a delta, the completion or the object.
    let leaked = text.contains(SHAPED_SECRET) || object.windows(SHAPED_SECRET.len()).any(|w| w == SHAPED_SECRET.as_bytes());
    leg.check("planted_secret_not_in_the_stream", !leaked, "a delta or the completed record carries the planted token");
    let echoed = text.contains("[redacted]");
    leg.count("secret_echoed_and_redacted", echoed);
    if !echoed {
        leg.note("the model did not echo the planted token, so the redaction clause was not exercised by this run (the scripted suite covers it)");
    }
    leg.count("deltas", deltas.len());
    leg.count("words", word_count(&text));
    leg.count("bytes", text.len());
    leg.count("stream_span_ms", span_ms);
    leg.count("wall_ms", started.elapsed().as_millis() as u64);
    fx.charge(&mut leg, &task).await;
    leg.note("event-timestamp-to-render at p95 in the packaged app is not measured here: no display on the runner; the Core-to-client leg is");
    leg.finish();
}

/// SIGKILL of the Core mid-stream on the live gateway: after the restart the
/// stream is closed as aborted by recovery, nothing partial is marked
/// complete, and the run then proceeds under the existing recovery path.
async fn leg_px041_kill(env: Arc<LiveEnv>) {
    let mut leg = Leg::begin(&env, "px041", "kill", 0.15);
    let mut fx = Fx::new(&env, &[("notes.txt", "alpha\nbeta\n")]).await;
    let task = fx
        .task("Write an explanation of about 1500 words of how a B-tree keeps itself balanced, in plain prose, no headings, no lists, and do not call any tool.")
        .await;
    let feed = collect_live(&fx.core, &fx.session, 0).await;
    fx.start(&task, 4, 1).await;
    let before = until_events(&feed, 120, "two deltas", |e| {
        of_kind(e, "AssistantTextDelta").len() >= 2
    })
    .await;
    leg.check(
        "stream_open_at_the_kill",
        of_kind(&before, "AssistantMessageCompleted").is_empty()
            && of_kind(&before, "AssistantMessageAborted").is_empty(),
        "the stream had already ended when the Core was killed (the answer was too short for this check)",
    );
    drop(feed);
    fx.restart().await;
    let all = fx.events().await;
    let deltas = of_kind(&all, "AssistantTextDelta");
    let aborted = of_kind(&all, "AssistantMessageAborted");
    leg.check("one_aborted_record", aborted.len() == 1, format!("{} abort records", aborted.len()));
    leg.check("source_recovery", aborted[0].payload["source"] == "RECOVERY", format!("{}", aborted[0].payload["source"]));
    leg.check("code_aborted_by_recovery", aborted[0].payload["code"] == "ABORTED_BY_RECOVERY", format!("{}", aborted[0].payload["code"]));
    leg.check(
        "no_partial_text_marked_complete",
        of_kind(&all, "AssistantMessageCompleted").iter().all(|c| c.aggregate != aborted[0].aggregate),
        "the aborted stream also completed",
    );
    leg.check(
        "abort_counts_every_delta",
        aborted[0].payload["delta_count"] == deltas.len() as u64
            && aborted[0].offset > deltas.last().unwrap().offset,
        format!("{} vs {} deltas", aborted[0].payload["delta_count"], deltas.len()),
    );
    leg.count("deltas_before_kill", deltas.len());
    let st = fx.status(&task).await;
    leg.check("loop_not_resumed_mid_token", !st.loop_alive, format!("{st:?}"));
    // The run proceeds under the existing recovery path: started again, it
    // completes a new stream; the aborted one stays aborted.
    fx.start(&task, 4, 1).await;
    let st = fx.wait(&task, 300).await;
    leg.check("run_ends_after_resume", !st.loop_alive, format!("{st:?}"));
    let after = fx.events().await;
    let completed = of_kind(&after, "AssistantMessageCompleted");
    leg.check("a_new_stream_completes", !completed.is_empty(), "no completed message after the resume");
    leg.check(
        "aborted_stream_stays_aborted",
        completed.iter().all(|c| c.aggregate != aborted[0].aggregate) && of_kind(&after, "AssistantMessageAborted").len() == 1,
        "the aborted stream changed",
    );
    // The first request's usage never arrived (killed): charge it at an
    // upper estimate so the cap stays honest.
    leg.charge(1, 2_500, 3_000);
    fx.charge(&mut leg, &task).await;
    leg.finish();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "live: needs MODBIT_LIVE=1 and the gateway credential (the live-core workflow)"]
async fn live_px041_stream() {
    leg_px041_stream(LiveEnv::from_process()).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "live: needs MODBIT_LIVE=1 and the gateway credential (the live-core workflow)"]
async fn live_px041_kill() {
    leg_px041_kill(LiveEnv::from_process()).await;
}

// ---------------------------------------------------------------------------
// the scripted stand-in for the offline runs of the same legs
// ---------------------------------------------------------------------------

const SCRIPTED_KEY: &str = "sk-live-0123456789abcdef0123456789";

/// An environment pointing the legs at a scripted server. Its records say
/// `live: false`; the checker refuses them as live evidence.
fn scripted_env(base: &str, results: &Path) -> Arc<LiveEnv> {
    let mut m: std::collections::BTreeMap<&str, String> = [
        ("MODBIT_LIVE", "1"),
        ("MODBIT_LIVE_API_KEY", SCRIPTED_KEY),
        ("MODBIT_LIVE_MODEL", "glm-5.3-flash"),
        ("MODBIT_LIVE_BASE_URL", base),
        ("MODBIT_LIVE_ALLOW_LOOPBACK", "1"),
        ("MODBIT_OPENAI_MODELS", "glm-5.3-flash=0.15/0.50;ctx=200000;out=8192".into()),
        ("MODBIT_LIVE_MAX_COST_USD", "5".into()),
    ]
    .into_iter()
    .map(|(k, v)| (k, v.to_owned()))
    .collect();
    m.insert("MODBIT_LIVE_RESULTS_DIR", results.to_string_lossy().into_owned());
    let mut env = LiveEnv::from_vars(&|k| m.get(k).cloned()).unwrap();
    env.live = false;
    Arc::new(env)
}

fn four_hundred_words() -> (String, Vec<String>) {
    let words: Vec<String> = (0..400).map(|i| format!("word{i}")).collect();
    let chunks: Vec<String> = words.chunks(5).map(|w| format!("{} ", w.join(" "))).collect();
    (chunks.concat(), chunks)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn scripted_px041_stream_leg_mechanics() {
    let (_, chunks) = four_hundred_words();
    let (base, _seen) = scripted_model_fn(Arc::new(move |_, _| {
        json!({"chunks": chunks, "chunk_gap_ms": 15})
    }))
    .await;
    let results = tempfile::tempdir().unwrap();
    leg_px041_stream(scripted_env(&base, results.path())).await;
    let rec: Value = serde_json::from_str(
        &std::fs::read_to_string(results.path().join("px041-stream.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(rec["passed"], true, "{rec:#}");
    assert_eq!(rec["live"], false);
    assert!(rec["counts"]["deltas"].as_u64().unwrap() >= 2);
    assert_eq!(rec["counts"]["words"], 400);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn scripted_px041_kill_leg_mechanics() {
    let (_, chunks) = four_hundred_words();
    // The first request streams slowly and is cut by the kill; the request
    // after the restart answers at once.
    let calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let (base, _seen) = scripted_model_fn(Arc::new(move |_, _| {
        if calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst) == 0 {
            json!({"chunks": chunks, "chunk_gap_ms": 80})
        } else {
            json!({"text": "the answer, in short"})
        }
    }))
    .await;
    let results = tempfile::tempdir().unwrap();
    leg_px041_kill(scripted_env(&base, results.path())).await;
    let rec: Value = serde_json::from_str(
        &std::fs::read_to_string(results.path().join("px041-kill.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(rec["passed"], true, "{rec:#}");
}
