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
        Self::build(env, files, extra, &|_| {}).await
    }

    /// `setup` runs on the profile directory before the Core starts (a skill
    /// package the owner authored, a registry file).
    async fn build(
        env: &Arc<LiveEnv>,
        files: &[(&str, &str)],
        extra: &[(&str, &str)],
        setup: &dyn Fn(&Path),
    ) -> Self {
        let (repo, root) = plain_repo(files);
        let data = tempfile::tempdir().unwrap();
        setup(data.path());
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
        assert_eq!(
            d.sequence,
            i as u64 + 1,
            "the stream's deltas are contiguous"
        );
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
    leg.check(
        "one_completion",
        done.len() == 1,
        format!("{} completions", done.len()),
    );
    leg.check(
        "not_aborted",
        of_kind(&all, "AssistantMessageAborted").is_empty(),
        "a live stream aborted",
    );
    leg.check(
        "streamed_in_pieces",
        deltas.len() >= 2,
        format!("{} deltas", deltas.len()),
    );
    // Bounds: size, order, rate.
    let max_bytes = deltas
        .iter()
        .map(|d| d.payload["text"].as_str().unwrap_or_default().len())
        .max()
        .unwrap_or(0);
    leg.check(
        "delta_size_bound",
        max_bytes <= 2048,
        format!("largest delta {max_bytes} bytes"),
    );
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
        format!(
            "{} deltas in {span_ms} ms (at most {allowed})",
            deltas.len()
        ),
    );
    leg.check(
        "delta_before_completion",
        deltas.iter().all(|d| d.offset < done[0].offset),
        "every delta precedes the completion",
    );
    // The concatenation is the completed message.
    let digest = hex::encode(sha2::Sha256::digest(text.as_bytes()));
    let p = &done[0].payload;
    leg.check(
        "delta_count_matches",
        p["delta_count"] == deltas.len() as u64,
        format!("{}", p["delta_count"]),
    );
    leg.check(
        "content_hash_matches",
        p["content_hash"] == digest,
        format!("{} vs {digest}", p["content_hash"]),
    );
    let object = fx.read_object(&digest).await;
    leg.check(
        "object_is_the_concatenation",
        object == text.as_bytes(),
        format!("{} bytes", object.len()),
    );
    // A client dropped mid-stream resumes by cursor: its events plus the
    // replay after its last offset are exactly the log, no gap, no duplicate.
    let first_part = dropped.lock().unwrap().clone();
    let cut = first_part.last().map_or(0, |e| e.offset);
    let saw_completion = first_part
        .iter()
        .any(|e| e.kind == "AssistantMessageCompleted");
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
        format!(
            "{} + {} events vs {} in the log",
            first_part.len(),
            resumed.len(),
            whole.len()
        ),
    );
    let live_offsets: Vec<u64> = live.iter().map(|e| e.offset).collect();
    leg.check(
        "live_view_equals_log",
        live_offsets == whole,
        format!("{} vs {}", live_offsets.len(), whole.len()),
    );
    // Core-to-client latency of each delta (event timestamp to receipt on the
    // same host). The render in the packaged app is not measured here.
    let lat: Vec<u64> = live
        .iter()
        .filter(|e| e.kind == "AssistantTextDelta" && e.aggregate == stream_id)
        .map(|e| (e.received_ms - e.recorded_ms).max(0) as u64)
        .collect();
    let p95 = percentile(&lat, 95.0);
    leg.count("latency_core_to_client_p95_ms", p95);
    leg.count(
        "latency_core_to_client_max_ms",
        lat.iter().copied().max().unwrap_or(0),
    );
    leg.check(
        "latency_p95_within_100ms",
        p95 <= 100,
        format!("p95 {p95} ms over {} deltas", lat.len()),
    );
    // The planted secret: never in a delta, the completion or the object.
    let leaked = text.contains(SHAPED_SECRET)
        || object
            .windows(SHAPED_SECRET.len())
            .any(|w| w == SHAPED_SECRET.as_bytes());
    leg.check(
        "planted_secret_not_in_the_stream",
        !leaked,
        "a delta or the completed record carries the planted token",
    );
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
    leg.check(
        "one_aborted_record",
        aborted.len() == 1,
        format!("{} abort records", aborted.len()),
    );
    leg.check(
        "source_recovery",
        aborted[0].payload["source"] == "RECOVERY",
        format!("{}", aborted[0].payload["source"]),
    );
    leg.check(
        "code_aborted_by_recovery",
        aborted[0].payload["code"] == "ABORTED_BY_RECOVERY",
        format!("{}", aborted[0].payload["code"]),
    );
    leg.check(
        "no_partial_text_marked_complete",
        of_kind(&all, "AssistantMessageCompleted")
            .iter()
            .all(|c| c.aggregate != aborted[0].aggregate),
        "the aborted stream also completed",
    );
    leg.check(
        "abort_counts_every_delta",
        aborted[0].payload["delta_count"] == deltas.len() as u64
            && aborted[0].offset > deltas.last().unwrap().offset,
        format!(
            "{} vs {} deltas",
            aborted[0].payload["delta_count"],
            deltas.len()
        ),
    );
    leg.count("deltas_before_kill", deltas.len());
    let st = fx.status(&task).await;
    leg.check(
        "loop_not_resumed_mid_token",
        !st.loop_alive,
        format!("{st:?}"),
    );
    // The run proceeds under the existing recovery path: started again, it
    // completes a new stream; the aborted one stays aborted.
    fx.start(&task, 4, 1).await;
    let st = fx.wait(&task, 300).await;
    leg.check("run_ends_after_resume", !st.loop_alive, format!("{st:?}"));
    let after = fx.events().await;
    let completed = of_kind(&after, "AssistantMessageCompleted");
    leg.check(
        "a_new_stream_completes",
        !completed.is_empty(),
        "no completed message after the resume",
    );
    leg.check(
        "aborted_stream_stays_aborted",
        completed
            .iter()
            .all(|c| c.aggregate != aborted[0].aggregate)
            && of_kind(&after, "AssistantMessageAborted").len() == 1,
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
// QUAL-PX-050: queue management and the typed interrupt on a live stream
// ---------------------------------------------------------------------------

use modbit_protocol::v1::{
    EditQueuedInput, InputQueued, InterruptAccepted, ListQueuedInputs, QueueInput,
    QueuedInputChanged, QueuedInputList, RemoveQueuedInput, ReorderQueuedInput, SendQueuedInputNow,
};

const ESSAY: &str = "Write an essay of about 1500 words on the history of the printing press, in plain prose with no headings and no lists. Do not call any tool.";

impl Fx {
    async fn queue(&mut self, task: &Id, id: &str, mode: &str, text: &str) {
        let _: InputQueued = self
            .ok(
                "QueueInput",
                QueueInput {
                    task_id: Some(task.clone()),
                    input_id: id.into(),
                    mode: mode.into(),
                    text: text.into(),
                },
                true,
            )
            .await;
    }

    async fn queued(&mut self, task: &Id, settled: bool) -> QueuedInputList {
        self.ok(
            "ListQueuedInputs",
            ListQueuedInputs {
                task_id: Some(task.clone()),
                include_settled: settled,
            },
            false,
        )
        .await
    }
}

fn queued_ids(l: &QueuedInputList) -> Vec<String> {
    l.items
        .iter()
        .filter(|i| i.state == "QUEUED")
        .map(|i| i.input_id.clone())
        .collect()
}

/// The input ids the log says were dispatched, one list per `TaskSteered`.
fn steered(events: &[Ev]) -> Vec<Vec<String>> {
    of_kind(events, "TaskSteered")
        .iter()
        .map(|e| {
            e.payload["input_ids"]
                .as_array()
                .map(|a| {
                    a.iter()
                        .filter_map(|v| v.as_str().map(str::to_owned))
                        .collect()
                })
                .unwrap_or_default()
        })
        .collect()
}

/// Queue four messages during a live streaming turn, edit one, delete one,
/// reorder: they drain in the order the person left them as separate turns,
/// and the deleted one is never dispatched.
async fn leg_px050_queue(env: Arc<LiveEnv>) {
    let mut leg = Leg::begin(&env, "px050", "queue", 0.20);
    let mut fx = Fx::new(&env, &[("a.txt", "alpha\n")]).await;
    let task = fx.task(ESSAY).await;
    let feed = collect_live(&fx.core, &fx.session, 0).await;
    fx.start(&task, 30, 8).await;
    let seen = until_events(&feed, 120, "the first delta", |e| {
        !of_kind(e, "AssistantTextDelta").is_empty()
    })
    .await;
    leg.check(
        "queued_during_a_streaming_turn",
        of_kind(&seen, "AssistantMessageCompleted").is_empty(),
        "the first answer had already completed when the inputs were queued",
    );
    for (id, text) in [
        ("a", "Message A: reply with just the word ALPHA."),
        ("b", "Message B: reply with just the word BRAVO."),
        ("c", "Message C: reply with just the word CHARLIE."),
        ("d", "Message D: reply with just the word DELTA."),
    ] {
        fx.queue(&task, id, "FOLLOW_UP", text).await;
    }
    let l = fx.queued(&task, false).await;
    leg.check(
        "listed_in_order",
        queued_ids(&l) == ["a", "b", "c", "d"],
        format!("{:?}", queued_ids(&l)),
    );
    leg.check(
        "run_alive_while_queued",
        l.run_alive,
        "the run was not alive",
    );
    let edited = "Message B, edited: reply with just the word BRAVO-EDITED.";
    let ch: QueuedInputChanged = fx
        .ok(
            "EditQueuedInput",
            EditQueuedInput {
                task_id: Some(task.clone()),
                input_id: "b".into(),
                text: edited.into(),
                mode: String::new(),
                model: String::new(),
            },
            true,
        )
        .await;
    leg.check("edit_applied", ch.state == "QUEUED", ch.state.clone());
    let _: QueuedInputChanged = fx
        .ok(
            "RemoveQueuedInput",
            RemoveQueuedInput {
                task_id: Some(task.clone()),
                input_id: "d".into(),
                reason: String::new(),
            },
            true,
        )
        .await;
    let _: QueuedInputChanged = fx
        .ok(
            "ReorderQueuedInput",
            ReorderQueuedInput {
                task_id: Some(task.clone()),
                input_id: "c".into(),
                before_input_id: "a".into(),
            },
            true,
        )
        .await;
    let l = fx.queued(&task, true).await;
    leg.check(
        "order_after_edits",
        queued_ids(&l) == ["c", "a", "b"],
        format!("{:?}", queued_ids(&l)),
    );
    leg.check(
        "edit_text_kept",
        l.items
            .iter()
            .find(|i| i.input_id == "b")
            .is_some_and(|i| i.text == edited && i.edited),
        "the edit was not recorded",
    );
    leg.check(
        "delete_recorded",
        l.items
            .iter()
            .find(|i| i.input_id == "d")
            .is_some_and(|i| i.state == "REMOVED"),
        "d was not removed",
    );
    let st = fx.wait(&task, 300).await;
    leg.check("run_drained_and_ended", !st.loop_alive, format!("{st:?}"));
    tokio::time::sleep(Duration::from_millis(500)).await;
    let ev = fx.events().await;
    let order = steered(&ev);
    leg.check(
        "drained_in_the_order_left",
        order.concat() == ["c", "a", "b"],
        format!("{order:?}"),
    );
    leg.check(
        "each_input_its_own_turn",
        order.iter().all(|ids| ids.len() == 1),
        format!("{order:?}"),
    );
    let completed = of_kind(&ev, "AssistantMessageCompleted").len();
    leg.count("assistant_messages", completed);
    leg.check(
        "a_turn_per_input",
        completed >= 4,
        format!("{completed} assistant messages for the essay and three inputs"),
    );
    let l = fx.queued(&task, true).await;
    leg.check(
        "queue_settled",
        queued_ids(&l).is_empty(),
        "inputs still queued",
    );
    leg.check(
        "three_dispatched_one_removed",
        l.items.iter().filter(|i| i.state == "DISPATCHED").count() == 3
            && l.items.iter().filter(|i| i.state == "REMOVED").count() == 1,
        format!(
            "{:?}",
            l.items
                .iter()
                .map(|i| (&i.input_id, &i.state))
                .collect::<Vec<_>>()
        ),
    );
    // The deleted input is nowhere in the conversation the model was given.
    let page: modbit_protocol::v1::TranscriptPage = fx
        .ok(
            "GetTranscript",
            modbit_protocol::v1::GetTranscript {
                task_id: Some(task.clone()),
                density: modbit_protocol::v1::TranscriptDensity::Detailed as i32,
                after_row: 0,
                limit: 500,
                as_of_offset: 0,
            },
            false,
        )
        .await;
    let rows: Vec<(String, String)> = page
        .rows
        .iter()
        .map(|r| (r.row_id.clone(), r.text.clone()))
        .collect();
    leg.check(
        "deleted_input_absent_from_the_conversation",
        !rows
            .iter()
            .any(|(id, t)| id == "user:in:d" || t.contains("Message D")),
        "message D reached the conversation",
    );
    leg.check(
        "edited_text_in_the_conversation",
        rows.contains(&("user:in:b".into(), edited.into())),
        format!("{rows:?}"),
    );
    fx.charge(&mut leg, &task).await;
    leg.finish();
}

/// Send now on a live stream: the stream ends aborted as a typed user
/// interrupt with the partial text kept, the new turn starts once, a replay of
/// the command (and of the interrupt id under another command id) interrupts
/// nothing more, and the next input is processed after it.
async fn leg_px050_interrupt(env: Arc<LiveEnv>) {
    let mut leg = Leg::begin(&env, "px050", "interrupt", 0.20);
    let mut fx = Fx::new(&env, &[("a.txt", "alpha\n")]).await;
    let task = fx.task(ESSAY).await;
    let feed = collect_live(&fx.core, &fx.session, 0).await;
    fx.start(&task, 30, 3).await;
    until_events(&feed, 120, "partial text", |e| {
        !of_kind(e, "AssistantTextDelta").is_empty()
    })
    .await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    fx.queue(
        &task,
        "x",
        "FOLLOW_UP",
        "Urgent: stop the essay and reply with just the word URGENT.",
    )
    .await;
    let asked = Instant::now();
    let cmd_id = rand_id();
    let send_now = |interrupt: &str| SendQueuedInputNow {
        task_id: Some(task.clone()),
        input_id: "x".into(),
        interrupt_id: interrupt.into(),
        reason: String::new(),
    };
    let ack =
        fx.c.command(envelope_fenced(
            cmd_id.clone(),
            "SendQueuedInputNow",
            send_now("int-1").encode_to_vec(),
            fx.g,
        ))
        .await
        .unwrap();
    let acc: InterruptAccepted = Client::result(&ack).unwrap();
    leg.check(
        "accepted_send_now",
        acc.kind == "SEND_NOW" && acc.run_alive,
        format!("{acc:?}"),
    );
    leg.check(
        "status_accepted_not_replayed",
        ack.status == 1,
        format!("status {}", ack.status),
    );
    fx.queue(
        &task,
        "y",
        "FOLLOW_UP",
        "And then reply with just the word AFTER.",
    )
    .await;
    let ev = until_events(&feed, 60, "the interrupt applied", |e| {
        !of_kind(e, "TaskInterruptApplied").is_empty()
    })
    .await;
    let cut_ms = asked.elapsed().as_millis() as u64;
    leg.count("interrupt_applied_after_ms", cut_ms);
    leg.check(
        "cut_at_the_next_safe_point",
        cut_ms < 15_000,
        format!("{cut_ms} ms"),
    );
    let applied = of_kind(&ev, "TaskInterruptApplied");
    leg.check(
        "stream_was_cut_not_finished",
        applied[0].payload["outcome"] == "STREAM_ABORTED",
        format!("outcome {} (the model's answer ended before the interrupt arrived if this is not STREAM_ABORTED)", applied[0].payload["outcome"]),
    );
    let replay =
        fx.c.command(envelope_fenced(
            cmd_id,
            "SendQueuedInputNow",
            send_now("int-1").encode_to_vec(),
            fx.g,
        ))
        .await
        .unwrap();
    leg.check(
        "replay_of_the_command_is_a_replay",
        replay.status == 2,
        format!("status {}", replay.status),
    );
    let again =
        fx.c.command(envelope_fenced(
            rand_id(),
            "SendQueuedInputNow",
            send_now("int-1").encode_to_vec(),
            fx.g,
        ))
        .await
        .unwrap();
    leg.check(
        "same_interrupt_id_is_the_same_interrupt",
        again.status == 2,
        format!("status {}", again.status),
    );
    let st = fx.wait(&task, 300).await;
    leg.check("run_ended", !st.loop_alive, format!("{st:?}"));
    tokio::time::sleep(Duration::from_millis(500)).await;
    let ev = fx.events().await;
    let aborted = of_kind(&ev, "AssistantMessageAborted");
    leg.check(
        "one_aborted_stream",
        aborted.len() == 1,
        format!("{} aborted streams", aborted.len()),
    );
    leg.check(
        "typed_source",
        aborted[0].payload["source"] == "USER_INTERRUPT"
            && aborted[0].payload["code"] == "USER_SEND_NOW",
        format!(
            "{} / {}",
            aborted[0].payload["source"], aborted[0].payload["code"]
        ),
    );
    let partial: Vec<&Ev> = of_kind(&ev, "AssistantTextDelta")
        .into_iter()
        .filter(|d| d.aggregate == aborted[0].aggregate)
        .collect();
    leg.check(
        "partial_text_kept",
        !partial.is_empty(),
        "no delta of the cut stream is on the log",
    );
    leg.check(
        "delta_count_matches",
        aborted[0].payload["delta_count"] == partial.len() as u64,
        format!("{} vs {}", aborted[0].payload["delta_count"], partial.len()),
    );
    leg.check(
        "cut_stream_never_completes",
        of_kind(&ev, "AssistantMessageCompleted")
            .iter()
            .all(|c| c.aggregate != aborted[0].aggregate),
        "the cut stream also completed",
    );
    leg.check(
        "interrupt_requested_once",
        of_kind(&ev, "TaskInterruptRequested").len() == 1,
        format!("{}", of_kind(&ev, "TaskInterruptRequested").len()),
    );
    let applied = of_kind(&ev, "TaskInterruptApplied");
    leg.check(
        "applied_once_with_dispatch_next",
        applied.len() == 1
            && applied[0].payload["stream_aborted"] == true
            && applied[0].payload["next"] == "DISPATCH",
        format!(
            "{:?}",
            applied.iter().map(|a| &a.payload).collect::<Vec<_>>()
        ),
    );
    let order = steered(&ev);
    let x_turns = order
        .iter()
        .filter(|ids| ids.iter().any(|i| i == "x"))
        .count();
    leg.check("new_turn_started_once", x_turns == 1, format!("{order:?}"));
    leg.check(
        "next_input_after_it",
        order.concat() == ["x", "y"],
        format!("{order:?}"),
    );
    leg.count("partial_deltas", partial.len());
    fx.charge(&mut leg, &task).await;
    leg.finish();
}

/// SIGKILL of the Core with inputs queued on a live run: the edits and the
/// order survive, nothing runs until the person resumes, and the queue then
/// drains in order.
async fn leg_px050_kill(env: Arc<LiveEnv>) {
    let mut leg = Leg::begin(&env, "px050", "kill", 0.20);
    let mut fx = Fx::new(&env, &[("a.txt", "alpha\n")]).await;
    let task = fx.task(ESSAY).await;
    let feed = collect_live(&fx.core, &fx.session, 0).await;
    fx.start(&task, 30, 8).await;
    until_events(&feed, 120, "the first delta", |e| {
        !of_kind(e, "AssistantTextDelta").is_empty()
    })
    .await;
    for (id, text) in [
        ("a", "Message A: reply with just the word ALPHA."),
        ("b", "Message B: reply with just the word BRAVO."),
        ("c", "Message C: reply with just the word CHARLIE."),
    ] {
        fx.queue(&task, id, "FOLLOW_UP", text).await;
    }
    let edited = "Message B, edited: reply with just the word BRAVO-EDITED.";
    let _: QueuedInputChanged = fx
        .ok(
            "EditQueuedInput",
            EditQueuedInput {
                task_id: Some(task.clone()),
                input_id: "b".into(),
                text: edited.into(),
                mode: String::new(),
                model: String::new(),
            },
            true,
        )
        .await;
    let _: QueuedInputChanged = fx
        .ok(
            "ReorderQueuedInput",
            ReorderQueuedInput {
                task_id: Some(task.clone()),
                input_id: "c".into(),
                before_input_id: "a".into(),
            },
            true,
        )
        .await;
    let before = fx.queued(&task, false).await;
    leg.check(
        "order_before_the_kill",
        queued_ids(&before) == ["c", "a", "b"],
        format!("{:?}", queued_ids(&before)),
    );
    drop(feed);
    fx.restart().await;
    let after = fx.queued(&task, false).await;
    leg.check(
        "same_inputs_same_order",
        queued_ids(&after) == ["c", "a", "b"],
        format!("{:?}", queued_ids(&after)),
    );
    leg.check(
        "edit_survived",
        after
            .items
            .iter()
            .find(|i| i.input_id == "b")
            .is_some_and(|i| i.text == edited),
        "the edit was lost",
    );
    leg.check(
        "nothing_runs_until_resumed",
        !after.run_alive,
        "the run was alive after the restart",
    );
    fx.start(&task, 30, 8).await;
    let st = fx.wait(&task, 300).await;
    leg.check("run_ended_after_resume", !st.loop_alive, format!("{st:?}"));
    tokio::time::sleep(Duration::from_millis(500)).await;
    let ev = fx.events().await;
    let order = steered(&ev);
    leg.check(
        "drained_in_order_after_recovery",
        order.concat() == ["c", "a", "b"],
        format!("{order:?}"),
    );
    let done = fx.queued(&task, true).await;
    leg.check(
        "all_three_dispatched",
        queued_ids(&done).is_empty()
            && done
                .items
                .iter()
                .filter(|i| i.state == "DISPATCHED")
                .count()
                == 3,
        format!(
            "{:?}",
            done.items
                .iter()
                .map(|i| (&i.input_id, &i.state))
                .collect::<Vec<_>>()
        ),
    );
    leg.charge(1, 2_500, 3_000);
    fx.charge(&mut leg, &task).await;
    leg.finish();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "live: needs MODBIT_LIVE=1 and the gateway credential (the live-core workflow)"]
async fn live_px050_queue() {
    leg_px050_queue(LiveEnv::from_process()).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "live: needs MODBIT_LIVE=1 and the gateway credential (the live-core workflow)"]
async fn live_px050_interrupt() {
    leg_px050_interrupt(LiveEnv::from_process()).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "live: needs MODBIT_LIVE=1 and the gateway credential (the live-core workflow)"]
async fn live_px050_kill() {
    leg_px050_kill(LiveEnv::from_process()).await;
}

// ---------------------------------------------------------------------------
// QUAL-PX-059: context accounting by category against the provider's count
// ---------------------------------------------------------------------------

fn category<'a>(
    v: &'a ContextAccountingView,
    name: &str,
) -> &'a modbit_protocol::v1::ContextCategoryView {
    v.categories
        .iter()
        .find(|c| c.category == name)
        .unwrap_or_else(|| {
            panic!(
                "no category {name} in {:?}",
                v.categories.iter().map(|c| &c.category).collect::<Vec<_>>()
            )
        })
}

/// Write a skill package `dir/<name>/SKILL.md` and return its content hash.
fn skill_package(root: &Path, name: &str, front: &str, body: &str) -> String {
    let d = root.join(name);
    std::fs::create_dir_all(&d).unwrap();
    std::fs::write(
        d.join("SKILL.md"),
        format!("---\nname: {name}\nversion: 1.0.0\n{front}\n---\n{body}\n"),
    )
    .unwrap();
    modbit_skills::load_package(&d).unwrap().content_hash
}

/// After a live multi-turn run with rules, a memory, a trusted skill, an
/// external tool and the delegation tools in play: the categories sum exactly
/// to the total, the total is the count the provider reported for the last
/// request and the estimator's own count is within its declared error of it,
/// the non-zero categories are the ones the request carries, and the window is
/// the catalog's (it follows the catalog entry, not a constant).
async fn leg_px059_accounting(env: Arc<LiveEnv>) {
    use modbit_protocol::v1::{
        MemoryPromoted, MemoryProposed, PromoteMemory, ProposeMemory, RepositoryTrusted,
        SkillTrustResult, TrustRepository, TrustSkill,
    };
    let mut leg = Leg::begin(&env, "px059", "accounting", 0.15);
    let agents = "# Rules for this repository\n\nAlways run the formatter before you finish. Never edit generated files. Prefer small functions and explain each public item in one sentence. Keep commit messages short.\n";
    let skill_hash = std::sync::Arc::new(Mutex::new(String::new()));
    let hash_slot = Arc::clone(&skill_hash);
    let servers = serde_json::to_string(&json!([px_common::mcp_support::server_entry(
        "docs",
        json!({}),
        &["search"],
        "TRUSTED"
    )]))
    .unwrap();
    let mut fx = Fx::build(
        &env,
        &[
            (
                "README.md",
                "# Hash table notes\n\nA small repository used to measure context accounting.\n",
            ),
            ("AGENTS.md", agents),
            ("a.txt", "alpha\nbeta\ngamma\n"),
        ],
        &[("MODBIT_MCP_SERVERS", servers.as_str())],
        &move |data| {
            *hash_slot.lock().unwrap() = skill_package(
                &data.join("skills"),
                "lintfix",
                "description: fix lint warnings in a changed file",
                "Run the linter, fix what it reports, rerun it, and stop when it is clean.",
            );
        },
    )
    .await;
    let root = fx.root.clone();
    let session = fx.session.clone();
    let _: RepositoryTrusted = fx
        .ok(
            "TrustRepository",
            TrustRepository {
                session_id: Some(session),
                workspace_root: root,
                scope: "repository".into(),
            },
            true,
        )
        .await;
    let task = fx.task("Do these steps in order. Step 1: call tool.search once with the query \"external\" so that the external tools become available. Step 2: read README.md with fs.read. Step 3: answer in one sentence with the title of that file. Call no other tool.").await;
    let hash = skill_hash.lock().unwrap().clone();
    let trusted: SkillTrustResult = fx
        .ok(
            "TrustSkill",
            TrustSkill {
                name: "lintfix".into(),
                content_hash: hash,
                task_id: Some(task.clone()),
            },
            false,
        )
        .await;
    leg.check(
        "skill_trusted",
        trusted.trust == "TRUSTED_BY_OWNER",
        trusted.trust.clone(),
    );
    let p: MemoryProposed = fx
        .ok(
            "ProposeMemory",
            ProposeMemory {
                task_id: Some(task.clone()),
                scope: "repository".into(),
                record_type: "convention".into(),
                topic: "indentation".into(),
                content: "Use four spaces for indentation in every language in this project."
                    .into(),
                ..Default::default()
            },
            true,
        )
        .await;
    let promoted: MemoryPromoted = fx
        .ok(
            "PromoteMemory",
            PromoteMemory {
                task_id: Some(task.clone()),
                memory_id: p.memory_id.clone(),
            },
            true,
        )
        .await;
    leg.check(
        "memory_curated",
        promoted.outcome == "curated",
        format!("{promoted:?}"),
    );
    // Before anything is compiled there is nothing to account for.
    let none = fx.accounting(&task).await;
    leg.check(
        "nothing_before_the_first_turn",
        !none.available && none.categories.is_empty(),
        format!("{none:?}"),
    );
    fx.start(&task, 10, 3).await;
    let st = fx.wait(&task, 240).await;
    leg.check("run_ended", !st.loop_alive, format!("{st:?}"));
    let v = fx.accounting(&task).await;
    leg.check("available", v.available, "no accounting after a run");
    leg.check(
        "provider_reported",
        v.total_source == "PROVIDER_REPORTED" && v.provider_reported_input > 0,
        format!("{} / {}", v.total_source, v.provider_reported_input),
    );
    leg.check(
        "total_is_the_providers_count",
        v.total_tokens == v.provider_reported_input,
        format!("{} vs {}", v.total_tokens, v.provider_reported_input),
    );
    leg.check(
        "nine_categories",
        v.categories.len() == 9,
        format!("{}", v.categories.len()),
    );
    let sum: u64 = v.categories.iter().map(|c| c.tokens).sum();
    leg.check(
        "categories_sum_exactly_to_the_total",
        sum == v.total_tokens,
        format!("{sum} vs {}", v.total_tokens),
    );
    // The provider's count against the independent per-step usage ledger.
    let e = fx.economics(&task).await;
    let last_step_input = e
        .runs
        .iter()
        .flat_map(|r| r.steps.iter())
        .rfind(|s| s.input_tokens > 0)
        .map(|s| s.input_tokens);
    leg.count("provider_reported_input", v.provider_reported_input);
    leg.count("estimated_total", v.estimated_total);
    leg.count("estimator_error_bp", v.estimator_error_bp);
    leg.count("declared_error_bp", v.declared_error_bp);
    leg.check(
        "total_matches_the_usage_ledger_of_the_last_step",
        last_step_input == Some(v.provider_reported_input),
        format!("{last_step_input:?} vs {}", v.provider_reported_input),
    );
    leg.check(
        "estimator_within_declared_error",
        v.estimator_error_bp <= v.declared_error_bp && v.declared_error_bp > 0,
        format!(
            "the estimator counted {} against the provider's {}: {} bp off, {} bp declared",
            v.estimated_total, v.provider_reported_input, v.estimator_error_bp, v.declared_error_bp
        ),
    );
    let non_zero = |name: &str| category(&v, name).tokens > 0;
    for name in [
        "SYSTEM",
        "TOOLS",
        "RULES",
        "SKILLS",
        "MCP",
        "MEMORY",
        "SUBAGENTS",
        "CONVERSATION",
    ] {
        leg.check(
            &format!("{}_non_zero", name.to_lowercase()),
            non_zero(name),
            format!("{:?}", category(&v, name)),
        );
    }
    leg.check(
        "summary_names_no_epoch_without_compaction",
        v.compaction_epoch == 0 && category(&v, "SUMMARY").sources.is_empty(),
        format!(
            "epoch {}, {:?}",
            v.compaction_epoch,
            category(&v, "SUMMARY")
        ),
    );
    // The window is the model's own (here the catalog's), and the percent follows.
    let window = env.window.unwrap_or(0);
    leg.check(
        "window_from_the_catalog",
        v.window_tokens == window && window > 0,
        format!("{} ({}) vs {window}", v.window_tokens, v.window_source),
    );
    leg.check(
        "percent_follows_the_window",
        u64::from(v.used_bp) == v.total_tokens * 10_000 / window.max(1),
        format!("{} bp", v.used_bp),
    );
    leg.check(
        "model_is_the_live_model",
        v.model == env.model,
        v.model.clone(),
    );
    // A different catalog window for the same model moves the window and the
    // percent (no second real model exists on the gateway to switch to).
    let narrow = format!(
        "{}={}/{};ctx=100000;out=8192",
        env.model, env.price.input_per_mtok_usd, env.price.output_per_mtok_usd
    );
    let session = fx.session.clone();
    fx.core.kill();
    fx.extra_env.push(("MODBIT_OPENAI_MODELS".into(), narrow));
    fx.core = Fx::spawn(&fx.env, fx.data.path(), &fx.extra_env);
    fx.c = fx.core.client().await;
    fx.g = Some(fx.lease_again().await);
    let _ = session;
    let t2 = fx
        .task("Read a.txt with fs.read and answer in one sentence. Call no other tool.")
        .await;
    fx.start(&t2, 8, 3).await;
    let st = fx.wait(&t2, 240).await;
    leg.check("second_run_ended", !st.loop_alive, format!("{st:?}"));
    let n = fx.accounting(&t2).await;
    leg.check(
        "window_follows_the_catalog_entry",
        n.window_tokens == 100_000,
        format!("{} ({})", n.window_tokens, n.window_source),
    );
    leg.check(
        "percent_follows_the_new_window",
        u64::from(n.used_bp) == n.total_tokens * 10_000 / 100_000,
        format!("{} bp of {}", n.used_bp, n.total_tokens),
    );
    leg.check(
        "second_categories_sum",
        n.categories.iter().map(|c| c.tokens).sum::<u64>() == n.total_tokens,
        "sum",
    );
    leg.note("a model switch to a second real model was not possible: the gateway key serves one priced model (glm-5.3-flash); the window was moved by changing its catalog entry");
    fx.charge(&mut leg, &task).await;
    fx.charge(&mut leg, &t2).await;
    leg.finish();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "live: needs MODBIT_LIVE=1 and the gateway credential (the live-core workflow)"]
async fn live_px059_accounting() {
    leg_px059_accounting(LiveEnv::from_process()).await;
}

// ---------------------------------------------------------------------------
// QUAL-PX-061: per-turn checkpoints and an exactly reversible restore
// ---------------------------------------------------------------------------

use std::collections::BTreeMap;

use modbit_protocol::v1::{
    CheckpointList, CheckpointRestoreResult, FileHash, ListCheckpoints, PreviewRewind,
    RestoreCheckpoint, RewindPreview,
};

/// Every file of a worktree (not `.git`) by content hash: what "exactly" is
/// compared with.
fn tree(root: &str) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    let mut stack = vec![std::path::PathBuf::from(root)];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d).into_iter().flatten().flatten() {
            let p = e.path();
            if p.file_name().is_some_and(|n| n == ".git") {
                continue;
            }
            if p.is_dir() {
                stack.push(p);
            } else if let Ok(b) = std::fs::read(&p) {
                out.insert(
                    p.strip_prefix(root)
                        .unwrap()
                        .to_string_lossy()
                        .replace('\\', "/"),
                    hex::encode(sha2::Sha256::digest(b)),
                );
            }
        }
    }
    out
}

/// Snapshots of the worktree taken the moment each `CheckpointCommitted`
/// reaches this client: what the files were at that turn's boundary, observed
/// from outside the Core. The model's next write is a whole model round away.
async fn snapshot_on_checkpoints(
    core: &CoreProcess,
    session: &Id,
    root: &str,
) -> Arc<Mutex<BTreeMap<String, (u32, BTreeMap<String, String>)>>> {
    let out = Arc::new(Mutex::new(BTreeMap::new()));
    let sink = Arc::clone(&out);
    let root = root.to_owned();
    let session = session.clone();
    let mut c = core.client().await;
    tokio::spawn(async move {
        if c.subscribe(session, 0).await.is_err() {
            return;
        }
        while let Ok(Some(e)) = c.next_event().await {
            let ev = ev_of(e);
            if ev.kind == "CheckpointCommitted" {
                let id = payload_str(&ev, "checkpoint_id");
                let ordinal =
                    u32::try_from(ev.payload["turn_ordinal"].as_u64().unwrap_or(0)).unwrap_or(0);
                sink.lock().unwrap().insert(id, (ordinal, tree(&root)));
            }
        }
    });
    out
}

impl Fx {
    async fn checkpoints(&mut self, task: &Id) -> CheckpointList {
        self.ok(
            "ListCheckpoints",
            ListCheckpoints {
                task_id: Some(task.clone()),
            },
            false,
        )
        .await
    }

    async fn restore(&mut self, mut req: RestoreCheckpoint, task: &Id) -> CheckpointRestoreResult {
        req.task_id = Some(task.clone());
        self.ok("RestoreCheckpoint", req, true).await
    }
}

/// Live turns each leave a checkpoint; restoring turn 1 deletes the files the
/// later turns created and restores the changed ones byte for byte (compared
/// with the tree observed from outside at that turn); the redo restores the
/// pre-restore state exactly; a file the user edited after the checkpoint is
/// refused without the choice and kept with it; a replayed restore command
/// restores once; a Core killed in the middle of a restore comes back to the
/// pre-restore state, never a mixture.
async fn leg_px061_checkpoints(env: Arc<LiveEnv>) {
    let mut leg = Leg::begin(&env, "px061", "checkpoints", 0.10);
    let mut fx = Fx::new(&env, &[("a.txt", "a\n"), ("b.txt", "b\n")]).await;
    let task = fx
        .task("Do exactly these three steps, one tool call per step, and call no other tool except plan.update first and task.complete last. Step 1: create the file one.txt containing the text one. Step 2: create the file two.txt containing the text two. Step 3: replace the content of a.txt with the text a-changed. Then finish.")
        .await;
    let snaps = snapshot_on_checkpoints(&fx.core, &fx.session, &fx.root).await;
    fx.start(&task, 20, 4).await;
    let st = fx.wait(&task, 300).await;
    leg.check("run_ended", !st.loop_alive, format!("{st:?}"));
    tokio::time::sleep(Duration::from_millis(600)).await;
    let at_end = tree(&fx.root);
    leg.check(
        "the_model_did_the_work",
        at_end.contains_key("one.txt")
            && at_end.contains_key("two.txt")
            && at_end.get("a.txt") != tree_of_base().get("a.txt"),
        format!("{:?}", at_end.keys().collect::<Vec<_>>()),
    );
    let l = fx.checkpoints(&task).await;
    let turn_cps: Vec<_> = l
        .checkpoints
        .iter()
        .filter(|c| c.reason == "turn_boundary")
        .collect();
    let mut ordinals: Vec<u32> = turn_cps.iter().map(|c| c.turn_ordinal).collect();
    ordinals.sort_unstable();
    ordinals.dedup();
    leg.count("turn_checkpoints", turn_cps.len());
    leg.count("turns", ordinals.len());
    leg.check(
        "every_turn_has_a_checkpoint",
        ordinals.len() >= 3
            && ordinals == (1..=u32::try_from(ordinals.len()).unwrap()).collect::<Vec<_>>(),
        format!("ordinals {ordinals:?}"),
    );
    let snaps_now = snaps.lock().unwrap().clone();
    leg.check(
        "snapshots_taken_for_every_checkpoint",
        snaps_now.len() >= turn_cps.len(),
        format!("{} snapshots", snaps_now.len()),
    );
    // The state at the end of turn 1 (the plan): none of the files exists yet.
    let cp1 = l
        .checkpoints
        .iter()
        .rev()
        .find(|c| c.turn_ordinal == 1 && c.reason == "turn_boundary")
        .expect("turn 1 checkpoint")
        .clone();
    let at_turn1 = snaps_now
        .get(&cp1.checkpoint_id)
        .map(|(_, t)| t.clone())
        .expect("a snapshot of turn 1");
    leg.check(
        "turn1_precedes_the_work",
        !at_turn1.contains_key("one.txt") && at_turn1 != at_end,
        format!("{:?}", at_turn1.keys().collect::<Vec<_>>()),
    );
    // Restore turn 1 by ordinal, with a command id we replay.
    let cmd = rand_id();
    let req = |c: &Id| RestoreCheckpoint {
        task_id: Some(c.clone()),
        turn_ordinal: 1,
        ..Default::default()
    };
    let ack =
        fx.c.command(envelope_fenced(
            cmd.clone(),
            "RestoreCheckpoint",
            req(&task).encode_to_vec(),
            fx.g,
        ))
        .await
        .unwrap();
    let r: CheckpointRestoreResult = Client::result(&ack).unwrap();
    leg.check(
        "restored",
        r.restored && !r.pre_restore_checkpoint_id.is_empty(),
        format!("{r:?}"),
    );
    leg.check(
        "created_files_deleted_changed_restored_byte_for_byte",
        tree(&fx.root) == at_turn1,
        format!("{:?} vs {:?}", tree(&fx.root), at_turn1),
    );
    std::fs::write(format!("{}/probe.txt", fx.root), "x").unwrap();
    let ack2 =
        fx.c.command(envelope_fenced(
            cmd,
            "RestoreCheckpoint",
            req(&task).encode_to_vec(),
            fx.g,
        ))
        .await
        .unwrap();
    let r2: CheckpointRestoreResult = Client::result(&ack2).unwrap();
    leg.check(
        "replayed_command_restores_once",
        ack2.status == 2 && r2.replayed && Path::new(&format!("{}/probe.txt", fx.root)).exists(),
        format!("status {}, {r2:?}", ack2.status),
    );
    std::fs::remove_file(format!("{}/probe.txt", fx.root)).unwrap();
    // The redo restores the state before the restore, exactly.
    let redo = fx
        .restore(
            RestoreCheckpoint {
                checkpoint_id: r.pre_restore_checkpoint_id.clone(),
                redo: true,
                ..Default::default()
            },
            &task,
        )
        .await;
    leg.check(
        "redo_restored",
        redo.restored && redo.redo,
        format!("{redo:?}"),
    );
    leg.check(
        "redo_equals_the_pre_restore_state",
        tree(&fx.root) == at_end,
        "file hashes differ from the state before the restore",
    );
    // A file the user edits after the checkpoint is listed by the preview; an
    // edit made after the person looked is refused without the choice.
    std::fs::write(format!("{}/b.txt", fx.root), "first edit by the user\n").unwrap();
    let preview: RewindPreview = fx
        .ok(
            "PreviewRewind",
            PreviewRewind {
                task_id: Some(task.clone()),
                turn_ordinal: 1,
                ..Default::default()
            },
            false,
        )
        .await;
    let listed = preview.entries.iter().find(|e| e.path == "b.txt");
    leg.check(
        "user_edited_file_is_listed",
        listed.is_some_and(|e| {
            e.action != "UNCHANGED"
                && e.current_hash == hex::encode(sha2::Sha256::digest(b"first edit by the user\n"))
        }),
        format!("{listed:?}"),
    );
    let expected: Vec<FileHash> = preview
        .entries
        .iter()
        .filter(|e| e.action != "UNCHANGED")
        .map(|e| FileHash {
            path: e.path.clone(),
            content_hash: e.current_hash.clone(),
        })
        .collect();
    std::fs::write(
        format!("{}/b.txt", fx.root),
        "edited by the user after the preview\n",
    )
    .unwrap();
    let edited = tree(&fx.root);
    let refused = fx
        .restore(
            RestoreCheckpoint {
                turn_ordinal: 1,
                expected: expected.clone(),
                ..Default::default()
            },
            &task,
        )
        .await;
    leg.check(
        "user_edit_refuses_the_restore_without_the_choice",
        !refused.restored && refused.refusal == "HASH_MISMATCH" && tree(&fx.root) == edited,
        format!("{refused:?}"),
    );
    // The same edit with the choice made: kept, everything else restored.
    let kept = fx
        .restore(
            RestoreCheckpoint {
                turn_ordinal: 1,
                keep_paths: vec!["b.txt".into()],
                ..Default::default()
            },
            &task,
        )
        .await;
    leg.check(
        "restore_with_the_choice",
        kept.restored && kept.kept_paths.contains(&"b.txt".to_owned()),
        format!("{kept:?}"),
    );
    leg.check(
        "edited_file_kept_others_restored",
        std::fs::read_to_string(format!("{}/b.txt", fx.root)).unwrap()
            == "edited by the user after the preview\n"
            && !Path::new(&format!("{}/one.txt", fx.root)).exists(),
        "b.txt or one.txt wrong",
    );
    // Back to the end state for the kill.
    let back = fx
        .restore(
            RestoreCheckpoint {
                checkpoint_id: kept.pre_restore_checkpoint_id.clone(),
                redo: true,
                ..Default::default()
            },
            &task,
        )
        .await;
    leg.check("redo_after_choice", back.restored, format!("{back:?}"));
    let pre_kill = tree(&fx.root);
    // SIGKILL in the middle of a restore: half the files written, then the
    // next Core resolves to exactly the pre-restore state.
    fx.core.kill();
    fx.extra_env
        .push(("MODBIT_FAULT_CHECKPOINT".into(), "MID_RESTORE_WRITE".into()));
    fx.core = Fx::spawn(&fx.env, fx.data.path(), &fx.extra_env);
    fx.c = fx.core.client().await;
    fx.g = Some(fx.lease_again().await);
    let died =
        fx.c.command(envelope_fenced(
            rand_id(),
            "RestoreCheckpoint",
            RestoreCheckpoint {
                task_id: Some(task.clone()),
                turn_ordinal: 1,
                ..Default::default()
            }
            .encode_to_vec(),
            fx.g,
        ))
        .await;
    leg.check(
        "core_died_mid_restore",
        died.is_err(),
        "the injected fault did not stop the Core",
    );
    let mixture = tree(&fx.root);
    leg.count("files_in_the_mixture", mixture.len());
    fx.core.kill();
    fx.extra_env.retain(|(k, _)| k != "MODBIT_FAULT_CHECKPOINT");
    fx.core = Fx::spawn(&fx.env, fx.data.path(), &fx.extra_env);
    fx.c = fx.core.client().await;
    fx.g = Some(fx.lease_again().await);
    leg.check(
        "never_a_mixture",
        tree(&fx.root) == pre_kill,
        format!(
            "the mixture had {} files; after recovery {} files, expected {}",
            mixture.len(),
            tree(&fx.root).len(),
            pre_kill.len()
        ),
    );
    fx.charge(&mut leg, &task).await;
    leg.finish();
}

fn tree_of_base() -> BTreeMap<String, String> {
    let mut m = BTreeMap::new();
    m.insert(
        "a.txt".to_owned(),
        hex::encode(sha2::Sha256::digest(b"a\n")),
    );
    m
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "live: needs MODBIT_LIVE=1 and the gateway credential (the live-core workflow)"]
async fn live_px061_checkpoints() {
    leg_px061_checkpoints(LiveEnv::from_process()).await;
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
        (
            "MODBIT_OPENAI_MODELS",
            "glm-5.3-flash=0.15/0.50;ctx=200000;out=8192",
        ),
        ("MODBIT_LIVE_MAX_COST_USD", "5"),
    ]
    .into_iter()
    .map(|(k, v)| (k, v.to_owned()))
    .collect();
    m.insert(
        "MODBIT_LIVE_RESULTS_DIR",
        results.to_string_lossy().into_owned(),
    );
    let mut env = LiveEnv::from_vars(&|k| m.get(k).cloned()).unwrap();
    env.live = false;
    Arc::new(env)
}

fn four_hundred_words() -> (String, Vec<String>) {
    let words: Vec<String> = (0..400).map(|i| format!("word{i}")).collect();
    let chunks: Vec<String> = words
        .chunks(5)
        .map(|w| format!("{} ", w.join(" ")))
        .collect();
    (chunks.concat(), chunks)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn scripted_px041_stream_leg_mechanics() {
    let (_, chunks) = four_hundred_words();
    let (base, _seen) = scripted_model_fn(Arc::new(
        move |_, _| json!({"chunks": chunks, "chunk_gap_ms": 15}),
    ))
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

fn slow_chunks(n: usize) -> Vec<String> {
    (0..n).map(|i| format!("word{i} ")).collect()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn scripted_px050_queue_leg_mechanics() {
    let calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let chunks = slow_chunks(60);
    let (base, _seen) = scripted_model_fn(Arc::new(move |_, _| {
        if calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst) == 0 {
            json!({"chunks": chunks, "chunk_gap_ms": 100})
        } else {
            json!({"text": "noted"})
        }
    }))
    .await;
    let results = tempfile::tempdir().unwrap();
    leg_px050_queue(scripted_env(&base, results.path())).await;
    let rec: Value = serde_json::from_str(
        &std::fs::read_to_string(results.path().join("px050-queue.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(rec["passed"], true, "{rec:#}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn scripted_px050_interrupt_leg_mechanics() {
    let calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let chunks = slow_chunks(300);
    let (base, _seen) = scripted_model_fn(Arc::new(move |_, _| {
        if calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst) == 0 {
            json!({"chunks": chunks, "chunk_gap_ms": 50})
        } else {
            json!({"text": "noted"})
        }
    }))
    .await;
    let results = tempfile::tempdir().unwrap();
    leg_px050_interrupt(scripted_env(&base, results.path())).await;
    let rec: Value = serde_json::from_str(
        &std::fs::read_to_string(results.path().join("px050-interrupt.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(rec["passed"], true, "{rec:#}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn scripted_px050_kill_leg_mechanics() {
    let calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let chunks = slow_chunks(300);
    let (base, _seen) = scripted_model_fn(Arc::new(move |_, _| {
        if calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst) == 0 {
            json!({"chunks": chunks, "chunk_gap_ms": 100})
        } else {
            json!({"text": "noted"})
        }
    }))
    .await;
    let results = tempfile::tempdir().unwrap();
    leg_px050_kill(scripted_env(&base, results.path())).await;
    let rec: Value = serde_json::from_str(
        &std::fs::read_to_string(results.path().join("px050-kill.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(rec["passed"], true, "{rec:#}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn scripted_px059_accounting_leg_mechanics() {
    // The scripted provider reports the size of the whole request body as its
    // count (a stand-in for a provider's own tokenizer).
    let (base, _seen) = scripted_model_fn(Arc::new(|body, results| {
        let used = serde_json::to_string(body).map_or(0, |b| b.len().div_ceil(4));
        if results == 0 {
            json!({"calls": [{"name": "tool.search", "args": {"query": "external"}}], "usage": [used, 30]})
        } else if results == 1 {
            json!({"calls": [{"name": "fs.read", "args": {"path": "README.md"}}], "usage": [used, 30]})
        } else {
            json!({"text": "Hash table notes", "usage": [used, 10]})
        }
    }))
    .await;
    let results = tempfile::tempdir().unwrap();
    leg_px059_accounting(scripted_env(&base, results.path())).await;
    let rec: Value = serde_json::from_str(
        &std::fs::read_to_string(results.path().join("px059-accounting.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(rec["passed"], true, "{rec:#}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn scripted_px061_checkpoints_leg_mechanics() {
    let (base, _seen) = scripted_model_fn(Arc::new(|_, results| {
        let steps = [
            json!({"calls": [{"name": "plan.update", "args": {"outcome": "three steps", "expected_files": ["one.txt", "two.txt", "a.txt"]}}]}),
            json!({"calls": [{"name": "change.apply", "args": {"path": "one.txt", "op": "create", "content": "one\n"}}], "delay_ms": 300}),
            json!({"calls": [{"name": "change.apply", "args": {"path": "two.txt", "op": "create", "content": "two\n"}}], "delay_ms": 300}),
            json!({"calls": [{"name": "change.apply", "args": {"path": "a.txt", "op": "replace", "content": "a-changed\n"}}], "delay_ms": 300}),
            json!({"calls": [{"name": "task.complete", "args": {"summary": "done", "self_review": {"findings": []}}}], "delay_ms": 300}),
        ];
        steps.get(results).cloned().unwrap_or_else(|| json!({"text": "done"}))
    }))
    .await;
    let results = tempfile::tempdir().unwrap();
    leg_px061_checkpoints(scripted_env(&base, results.path())).await;
    let rec: Value = serde_json::from_str(
        &std::fs::read_to_string(results.path().join("px061-checkpoints.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(rec["passed"], true, "{rec:#}");
}
