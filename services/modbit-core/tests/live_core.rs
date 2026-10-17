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
                if !st.failure_class.is_empty() || st.loop_alive {
                    self.diagnose(task).await;
                }
                return st;
            }
            tokio::time::sleep(Duration::from_millis(150)).await;
        }
    }

    /// What the log says about a task that did not end cleanly (printed so a
    /// failed leg can be read from the job log).
    async fn diagnose(&self, task: &Id) {
        let ev = of_task(self.events().await, task);
        eprintln!("--- the last events of a task that did not end cleanly ---");
        for e in ev.iter().rev().take(40).collect::<Vec<_>>().into_iter().rev() {
            let text = e.payload.to_string();
            eprintln!("{} {}", e.kind, text.chars().take(220).collect::<String>());
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

/// The events that belong to one task.
fn of_task(events: Vec<Ev>, task: &Id) -> Vec<Ev> {
    let hex = hex::encode(&task.value);
    events.into_iter().filter(|e| e.task.as_deref() == Some(hex.as_str())).collect()
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
// QUAL-PX-101: pause and resume over the runtime park
// ---------------------------------------------------------------------------

use modbit_protocol::v1::{PauseTask, ResumeTask, TaskPauseResult, TaskResumeResult};

fn rejected_code<T: std::fmt::Debug>(r: Result<T, ClientError>) -> String {
    match r {
        Err(ClientError::Rejected { code, .. }) => code,
        other => panic!("expected a typed refusal, got {other:?}"),
    }
}

/// Pause during a live streaming turn: the answer says the turn is in flight,
/// the turn finishes (its stream completes, never aborts), the run parks at
/// the boundary typed paused with a record of who and why; a duplicate pause
/// acts once; `SIGKILL` of the Core while paused leaves the task paused with
/// the same checkpoint and no lost event; a resume with a stale lease
/// generation is refused and changes nothing; the resume under the current
/// one continues the same run exactly once.
async fn leg_px101_pause(env: Arc<LiveEnv>) {
    let mut leg = Leg::begin(&env, "px101", "pause", 0.15);
    let mut fx = Fx::new(&env, &[("a.txt", "a\n")]).await;
    let task = fx
        .task("Create the files f1.txt, f2.txt, f3.txt and f4.txt, each containing its own file name, one file per step. Before each file write one short paragraph saying what you are about to create. Call plan.update first and task.complete last.")
        .await;
    let feed = collect_live(&fx.core, &fx.session, 0).await;
    let started = fx.start(&task, 30, 6).await;
    until_events(&feed, 120, "a streaming turn", |e| !of_kind(e, "AssistantTextDelta").is_empty()).await;
    let cmd = rand_id();
    let ack = fx.c.command(envelope_fenced(cmd.clone(), "PauseTask", PauseTask { task_id: Some(task.clone()), reason: "lunch".into(), wait_ms: 0 }.encode_to_vec(), fx.g)).await.unwrap();
    let p: TaskPauseResult = Client::result(&ack).unwrap();
    leg.check("pause_accepted", ack.status == 1, format!("status {}", ack.status));
    leg.check(
        "answer_says_the_turn_is_in_flight",
        p.state == "PAUSING" && p.waits_for_boundary && !p.note.is_empty(),
        format!("{p:?} (a turn that had already ended is not a streaming-turn pause)"),
    );
    let deadline = Instant::now() + Duration::from_secs(180);
    let st = loop {
        let st = fx.status(&task).await;
        if (st.wait_reason == "Paused" && !st.loop_alive) || Instant::now() > deadline {
            break st;
        }
        tokio::time::sleep(Duration::from_millis(150)).await;
    };
    leg.check("parked_at_the_boundary", st.state == "Waiting" && st.wait_reason == "Paused" && !st.loop_alive, format!("{st:?}"));
    leg.check("a_pause_is_not_a_fault", st.failure_class.is_empty(), format!("{st:?}"));
    tokio::time::sleep(Duration::from_millis(500)).await;
    let ev = fx.events().await;
    leg.check("no_stream_was_aborted", of_kind(&ev, "AssistantMessageAborted").is_empty(), "a pause cut a stream");
    let streams_open: Vec<&Ev> = of_kind(&ev, "AssistantTextDelta")
        .into_iter()
        .filter(|d| of_kind(&ev, "AssistantMessageCompleted").iter().all(|c| c.aggregate != d.aggregate))
        .collect();
    leg.check("in_flight_turn_finished", streams_open.is_empty(), format!("{} deltas of an unfinished stream", streams_open.len()));
    let paused = of_kind(&ev, "TaskPaused");
    leg.check("one_pause_record", paused.len() == 1, format!("{}", paused.len()));
    leg.check(
        "record_names_who_why_and_the_checkpoint",
        paused[0].payload["reason"] == "lunch" && paused[0].payload["boundary"] == "TURN_BOUNDARY" && !payload_str(paused[0], "checkpoint_id").is_empty(),
        format!("{}", paused[0].payload),
    );
    // A duplicate pause acts once; a fresh pause on a paused task is a no-op answer.
    let dup = fx.c.command(envelope_fenced(cmd, "PauseTask", PauseTask { task_id: Some(task.clone()), reason: "lunch".into(), wait_ms: 0 }.encode_to_vec(), fx.g)).await.unwrap();
    leg.check("duplicate_pause_is_a_replay", dup.status == 2, format!("status {}", dup.status));
    leg.check("duplicate_recorded_nothing", of_kind(&fx.events().await, "TaskPauseRequested").len() == 1, "more than one request on the log");
    let again: TaskPauseResult = fx.ok("PauseTask", PauseTask { task_id: Some(task.clone()), reason: "again".into(), wait_ms: 0 }, true).await;
    leg.check("pause_of_a_paused_task", again.state == "ALREADY_PAUSED", again.state.clone());
    // SIGKILL while paused.
    let cps_before = fx.checkpoints(&task).await;
    let held_events = |ev: &[Ev]| ev.iter().filter(|e| matches!(e.kind.as_str(), "TaskPaused" | "TaskWaiting" | "RunSuspended")).count();
    let held_before = held_events(&fx.events().await);
    let old_generation = fx.g;
    drop(feed);
    fx.restart().await;
    let after = fx.status(&task).await;
    leg.check("paused_after_the_kill", after.state == "Waiting" && after.wait_reason == "Paused" && !after.loop_alive && after.failure_class.is_empty(), format!("{after:?}"));
    leg.check("same_run_state", st.run_state == after.run_state, format!("{} vs {}", st.run_state, after.run_state));
    let cps_after = fx.checkpoints(&task).await;
    leg.check("same_checkpoint_held", cps_after.current_checkpoint_id == cps_before.current_checkpoint_id, format!("{} vs {}", cps_after.current_checkpoint_id, cps_before.current_checkpoint_id));
    leg.check("no_event_lost_or_invented", held_events(&fx.events().await) == held_before, "the pause events changed across the restart");
    // A second client with the previous generation: refused, nothing changes.
    let stale = fx.c.command(envelope_fenced(rand_id(), "ResumeTask", ResumeTask { task_id: Some(task.clone()), ..Default::default() }.encode_to_vec(), old_generation)).await;
    leg.check("stale_resume_refused", rejected_code(stale) == "STALE_LEASE", "the stale generation was not refused as STALE_LEASE");
    let still = fx.status(&task).await;
    leg.check("stale_resume_changed_nothing", still.wait_reason == "Paused" && !still.loop_alive, format!("{still:?}"));
    // The current generation resumes the run, once.
    let usage_before = of_kind(&fx.events().await, "AssistantMessageCompleted").len();
    let rid = rand_id();
    let ack = fx.c.command(envelope_fenced(rid.clone(), "ResumeTask", ResumeTask { task_id: Some(task.clone()), model: env.model.clone(), ..Default::default() }.encode_to_vec(), fx.g)).await.unwrap();
    let r: TaskResumeResult = Client::result(&ack).unwrap();
    leg.check("resumed_the_same_run", r.resumed && r.run_id == started.run_id, format!("{r:?} vs {:?}", started.run_id));
    let dup = fx.c.command(envelope_fenced(rid, "ResumeTask", ResumeTask { task_id: Some(task.clone()), model: env.model.clone(), ..Default::default() }.encode_to_vec(), fx.g)).await.unwrap();
    leg.check("duplicate_resume_is_a_replay", dup.status == 2, format!("status {}", dup.status));
    let st = fx.wait(&task, 300).await;
    leg.check("run_ended_after_resume", !st.loop_alive && st.failure_class.is_empty(), format!("{st:?}"));
    let ev = fx.events().await;
    leg.check("one_pause_one_completion", of_kind(&ev, "TaskPaused").len() == 1, "more than one pause record");
    leg.check("the_run_made_progress_after_the_resume", of_kind(&ev, "AssistantMessageCompleted").len() > usage_before, "no model turn completed after the resume");
    let not_paused = fx.c.command(envelope_fenced(rand_id(), "ResumeTask", ResumeTask { task_id: Some(task.clone()), ..Default::default() }.encode_to_vec(), fx.g)).await;
    leg.check("resume_of_a_running_or_finished_task_is_refused", rejected_code(not_paused) == "NOT_PAUSED", "not NOT_PAUSED");
    leg.note("pause during a running shell command and with a pending approval are model-driven (the model must run a long command or ask) and are proven in the scripted suite only");
    fx.charge(&mut leg, &task).await;
    leg.finish();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "live: needs MODBIT_LIVE=1 and the gateway credential (the live-core workflow)"]
async fn live_px101_pause() {
    leg_px101_pause(LiveEnv::from_process()).await;
}

// ---------------------------------------------------------------------------
// QUAL-PX-051 (and the Core half of PX-100): task modes enforced by the Kernel
// ---------------------------------------------------------------------------

use modbit_protocol::v1::{InvokeTool, SetTaskMode, TaskMode, TaskModeChanged, ToolInvoked};

impl Fx {
    /// One tool call through the Capability Kernel on a task, as a client
    /// makes it (not the model).
    async fn invoke(&mut self, task: &Id, tool: &str, args: Value) -> ToolInvoked {
        self.ok(
            "InvokeTool",
            InvokeTool {
                task_id: Some(task.clone()),
                tool_name: tool.into(),
                arguments_json: args.to_string(),
                tool_call_id: Some(rand_id()),
                output_budget_bytes: 65_536,
            },
            true,
        )
        .await
    }

    async fn set_mode(&mut self, task: &Id, mode: TaskMode) -> TaskModeChanged {
        self.ok(
            "SetTaskMode",
            SetTaskMode {
                task_id: Some(task.clone()),
                mode: mode as i32,
                reason: "live leg".into(),
            },
            true,
        )
        .await
    }
}

/// The denials of the Capability Kernel on the log, as their decision codes.
fn denials(events: &[Ev]) -> Vec<String> {
    of_kind(events, "ToolCallPolicyDecision")
        .iter()
        .filter(|e| e.payload["allowed"] == false)
        .map(|e| e.payload["decision"].as_str().unwrap_or_default().to_owned())
        .collect()
}

const BUGGY_CALC: &str = "def double(x):\n    return x + x + 1\n";
const CALC_TEST: &str = "import unittest\nfrom calc import double\n\n\nclass DoubleTest(unittest.TestCase):\n    def test_double(self):\n        self.assertEqual(double(3), 6)\n\n\nif __name__ == '__main__':\n    unittest.main()\n";
const UNITTEST_CHECK: &str =
    "{\"commands\": [{\"id\": \"unit\", \"argv\": [\"python3\", \"-m\", \"unittest\", \"-q\"]}]}";

/// With the live gateway: in ASK a prompt to edit a file yields a refusal by
/// the Kernel and no write (the model is not even offered one, and a call the
/// client makes anyway is denied); in PLAN the agent records a plan, writes
/// nothing, and once the plan is accepted the task is an AGENT with the plan
/// attached and the write lands; in DEBUG a fix is not accepted before the
/// failure is reproduced.
async fn leg_px051_modes(env: Arc<LiveEnv>) {
    let mut leg = Leg::begin(&env, "px051", "modes", 0.20);
    let mut fx = Fx::new(
        &env,
        &[
            ("a.txt", "alpha\n"),
            ("calc.py", BUGGY_CALC),
            ("test_calc.py", CALC_TEST),
            (".modbit/verification.json", UNITTEST_CHECK),
        ],
    )
    .await;
    // ---- ASK ----
    let ask = fx.task_in_mode("Create the file hello.txt containing the text hi, using change.apply.", TaskMode::Ask as i32).await;
    fx.start(&ask, 8, 3).await;
    let st = fx.wait(&ask, 240).await;
    leg.check("ask_run_ended", !st.loop_alive, format!("{st:?}"));
    let ev = of_task(fx.events().await, &ask);
    leg.check("ask_wrote_nothing", !Path::new(&fx.root).join("hello.txt").exists() && of_kind(&ev, "FileChanged").is_empty(), "hello.txt exists or a FileChanged was logged");
    let posture_denials = denials(&ev).into_iter().filter(|d| d.starts_with("MODE_POSTURE")).count();
    leg.count("ask_model_write_attempts_refused_by_the_kernel", posture_denials);
    leg.check("ask_denials_are_the_modes", denials(&ev).iter().all(|d| d.starts_with("MODE_POSTURE")), format!("{:?}", denials(&ev)));
    // A write the client makes anyway is denied by the Kernel with the mode's code.
    let r = fx.invoke(&ask, "change.apply", json!({"path": "hello.txt", "op": "replace", "content": "hi\n"})).await;
    leg.check("ask_write_denied_by_the_kernel", r.status == "POLICY_DENIED" && r.error_code == "MODE_POSTURE", format!("{r:?}"));
    let r = fx.invoke(&ask, "shell.exec", json!({"argv": ["git", "--version"]})).await;
    leg.check("ask_execution_denied", r.status == "POLICY_DENIED" && r.error_code == "MODE_POSTURE", format!("{r:?}"));
    leg.check("ask_still_wrote_nothing", !Path::new(&fx.root).join("hello.txt").exists(), "hello.txt exists");
    // ---- PLAN ----
    let plan = fx
        .task_in_mode("Plan how to add the file b.txt whose content is the same as a.txt. Record the plan with plan.update (expected_files: b.txt), write nothing, then state the plan in one sentence and stop. Do not call task.complete.", TaskMode::Plan as i32)
        .await;
    fx.start(&plan, 10, 3).await;
    let st = fx.wait(&plan, 240).await;
    leg.check("plan_run_ended", !st.loop_alive, format!("{st:?}"));
    let ev = of_task(fx.events().await, &plan);
    let plans = of_kind(&ev, "PlanRecorded");
    leg.check("plan_recorded", !plans.is_empty(), "the agent recorded no plan");
    leg.check("plan_wrote_nothing", !Path::new(&fx.root).join("b.txt").exists(), "b.txt exists before acceptance");
    let r = fx.invoke(&plan, "change.apply", json!({"path": "b.txt", "op": "replace", "content": "alpha\n"})).await;
    leg.check("plan_write_rejected_until_acceptance", r.error_code == "MODE_POSTURE", format!("{r:?}"));
    let accepted = fx.set_mode(&plan, TaskMode::Agent).await;
    leg.check("acceptance_attaches_the_plan", accepted.accepted_plan_version >= 1 && accepted.mode == TaskMode::Agent as i32, format!("{accepted:?}"));
    fx.queue(&plan, "go", "FOLLOW_UP", "The plan is accepted. Now create b.txt exactly as planned, then finish.").await;
    fx.start(&plan, 12, 4).await;
    let st = fx.wait(&plan, 240).await;
    leg.check("agent_run_ended", !st.loop_alive, format!("{st:?}"));
    leg.check(
        "after_acceptance_the_write_lands",
        std::fs::read_to_string(Path::new(&fx.root).join("b.txt")).is_ok_and(|t| t.contains("alpha")),
        "b.txt was not written after the plan was accepted",
    );
    let set = of_kind(&of_task(fx.events().await, &plan), "TaskModeSet").len();
    leg.check("mode_change_on_the_log", set >= 1, format!("{set}"));
    // ---- DEBUG ----
    let debug = fx
        .task_in_mode("The unit test in test_calc.py fails. Fix calc.py so that it passes, then finish.", TaskMode::Debug as i32)
        .await;
    fx.start(&debug, 20, 5).await;
    let st = fx.wait(&debug, 300).await;
    leg.check("debug_run_ended", !st.loop_alive, format!("{st:?}"));
    let ev: Vec<Ev> = of_task(fx.events().await, &debug);
    let first_write = ev.iter().position(|e| e.kind == "FileChanged");
    let first_repro = ev.iter().position(|e| e.kind == "ReproductionRecorded");
    leg.check(
        "debug_reproduces_before_a_fix_lands",
        first_repro.is_some() && first_write.is_none_or(|w| first_repro.unwrap() < w),
        format!("reproduction at {first_repro:?}, first write at {first_write:?}"),
    );
    let refused = ev.iter().filter(|e| e.kind == "StepFailed" && e.payload["failure_code"] == "HARNESS_REPRODUCTION_REQUIRED").count();
    leg.count("debug_fixes_refused_before_reproduction", refused);
    let calc = std::fs::read_to_string(Path::new(&fx.root).join("calc.py")).unwrap_or_default();
    leg.count("debug_fixed_calc", calc != BUGGY_CALC);
    leg.note("MULTITASK (a bounded child admitted with a ticket), the 15 s expiry of an unanswered mode proposal and consent surviving a restart are not model-dependent or need a second child run; they are proven in the scripted suite only");
    for t in [&ask, &plan, &debug] {
        fx.charge(&mut leg, t).await;
    }
    leg.finish();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "live: needs MODBIT_LIVE=1 and the gateway credential (the live-core workflow)"]
async fn live_px051_modes() {
    leg_px051_modes(LiveEnv::from_process()).await;
}

// ---------------------------------------------------------------------------
// The person at the keyboard for the approvals a live run asks for
// ---------------------------------------------------------------------------

use modbit_protocol::v1::{ApprovalList, ListApprovals, ResolveApproval};

/// What the stand-in person approved, tool by tool, with the exact intent
/// they saw.
type Approved = Arc<Mutex<Vec<(String, String)>>>;

/// Approves, exactly as a thin client does (naming the intent hash it was
/// shown), every approval of a tool for which `allow` holds; leaves every
/// other approval pending. A leg that needs a person says so in its record.
async fn spawn_approver_for(
    core: &CoreProcess,
    session: &Id,
    generation: Option<u64>,
    allow: fn(&str) -> bool,
) -> (tokio::task::JoinHandle<()>, Approved) {
    let approved: Approved = Arc::new(Mutex::new(Vec::new()));
    let log = Arc::clone(&approved);
    let mut c = core.client().await;
    let session = session.clone();
    let handle = tokio::spawn(async move {
        let mut done: std::collections::HashSet<Vec<u8>> = std::collections::HashSet::new();
        loop {
            let ack = c
                .command(envelope(
                    rand_id(),
                    "ListApprovals",
                    ListApprovals {
                        session_id: Some(session.clone()),
                    }
                    .encode_to_vec(),
                ))
                .await;
            let Ok(ack) = ack else { return };
            let list: ApprovalList = Client::result(&ack).unwrap();
            for a in list
                .approvals
                .iter()
                .filter(|a| a.status == "REQUESTED" && allow(&a.tool_name))
            {
                let id = a.approval_id.clone().unwrap();
                if done.insert(id.value.clone()) {
                    let r = c
                        .command(envelope_fenced(
                            rand_id(),
                            "ResolveApproval",
                            ResolveApproval {
                                approval_id: Some(id),
                                approve: true,
                                reason: "live leg: the person approves this tool".into(),
                                intent_hash: a.intent_hash.clone(),
                            }
                            .encode_to_vec(),
                            generation,
                        ))
                        .await;
                    if r.is_ok() {
                        log.lock()
                            .unwrap()
                            .push((a.tool_name.clone(), a.scope_json.clone()));
                    }
                }
            }
            tokio::time::sleep(Duration::from_millis(150)).await;
        }
    });
    (handle, approved)
}

// ---------------------------------------------------------------------------
// QUAL-PX-118: a task in its own worktree
// ---------------------------------------------------------------------------

use modbit_protocol::v1::{GetSessionSnapshot, SessionSnapshot, TaskIsolation};

impl Fx {
    /// A task whose work happens in its own worktree.
    async fn task_isolated(&mut self, goal: &str) -> Id {
        let created: TaskCreated = self
            .ok(
                "CreateTask",
                modbit_protocol::v1::CreateTask {
                    session_id: Some(self.session.clone()),
                    goal_text: goal.into(),
                    origin: "cli".into(),
                    workspace_root: self.root.clone(),
                    execution_profile: "local_trusted".into(),
                    isolation: TaskIsolation::Worktree as i32,
                    ..Default::default()
                },
                true,
            )
            .await;
        created.task_id.unwrap()
    }

    async fn task_view(&mut self, task: &Id) -> modbit_protocol::v1::TaskView {
        let snap: SessionSnapshot = self
            .ok(
                "GetSessionSnapshot",
                GetSessionSnapshot {
                    session_id: Some(self.session.clone()),
                },
                false,
            )
            .await;
        snap.tasks
            .into_iter()
            .find(|t| t.task_id.as_ref() == Some(task))
            .expect("the task is in the snapshot")
    }
}

fn git_out(dir: &str, args: &[&str]) -> String {
    let out = std::process::Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .unwrap();
    String::from_utf8_lossy(&out.stdout).trim().to_owned()
}

fn same_dir(a: &str, b: &Path) -> bool {
    Path::new(a)
        .canonicalize()
        .ok()
        .zip(b.canonicalize().ok())
        .is_some_and(|(x, y)| x == y)
}

fn python_tests_pass(dir: &str) -> bool {
    std::process::Command::new("python3")
        .args(["-m", "unittest", "-q"])
        .current_dir(dir)
        .output()
        .is_ok_and(|o| o.status.success())
}

/// A live task created with `worktree` isolation edits files and runs the
/// tests while the user's checkout (with the user's own uncommitted work in
/// it) stays byte-identical; a path escaping the worktree through `..`, an
/// absolute path and a symlink is refused; an unwritable worktree root fails
/// the creation with the typed reason and writes nothing; a Core killed mid-run
/// resumes in the same worktree.
async fn leg_px118_worktree(env: Arc<LiveEnv>) {
    let mut leg = Leg::begin(&env, "px118", "worktree", 0.25);
    let mut fx = Fx::new(
        &env,
        &[
            ("calc.py", BUGGY_CALC),
            ("test_calc.py", CALC_TEST),
            ("README.md", "# demo\n"),
            (".modbit/verification.json", UNITTEST_CHECK),
        ],
    )
    .await;
    // The user's own work in the checkout: a tracked edit and an untracked file.
    std::fs::write(Path::new(&fx.root).join("README.md"), "# demo, edited by the user\n").unwrap();
    std::fs::write(Path::new(&fx.root).join("notes.txt"), "user notes\n").unwrap();
    let before = tree(&fx.root);
    let head_before = git_out(&fx.root, &["rev-parse", "HEAD"]);
    // An unwritable worktree root: typed refusal, no task, nothing written.
    let in_the_way = fx.data.path().join("worktrees");
    std::fs::write(&in_the_way, b"in the way").unwrap();
    let refused = fx
        .cmd::<TaskCreated>(
            "CreateTask",
            modbit_protocol::v1::CreateTask {
                session_id: Some(fx.session.clone()),
                goal_text: "x".into(),
                origin: "cli".into(),
                workspace_root: fx.root.clone(),
                execution_profile: "local_trusted".into(),
                isolation: TaskIsolation::Worktree as i32,
                ..Default::default()
            },
            true,
        )
        .await;
    leg.check(
        "unwritable_root_is_a_typed_refusal",
        refused.as_ref().err().is_some_and(|(c, m)| c == "ISOLATION_ROOT_UNWRITABLE" && m.contains("nothing was written to the checkout")),
        format!("{refused:?}"),
    );
    leg.check("unwritable_root_wrote_nothing", tree(&fx.root) == before, "the checkout changed");
    std::fs::remove_file(&in_the_way).unwrap();
    // The live task, in its own worktree.
    let (approver, approved) = spawn_approver_for(&fx.core, &fx.session, fx.g, |t| t == "shell.exec" || t == "test.run").await;
    let task = fx
        .task_isolated("The unit test in test_calc.py fails. Fix calc.py so that double(3) returns 6, check it by running the tests with: python3 -m unittest -q (use shell.exec), then finish with task.complete.")
        .await;
    let view = fx.task_view(&task).await;
    let wt = fx.data.path().join("worktrees").join(uuid_text(&task.value));
    leg.check("worktree_exists", wt.join(".git").exists(), format!("{wt:?}"));
    leg.check(
        "task_root_is_the_worktree_not_the_checkout",
        same_dir(&view.workspace_root, &wt) && same_dir(&view.origin_root, Path::new(&fx.root)) && view.isolation == TaskIsolation::Worktree as i32,
        format!("{} / {}", view.workspace_root, view.origin_root),
    );
    leg.check(
        "users_work_was_carried_by_copy",
        std::fs::read_to_string(wt.join("README.md")).is_ok_and(|t| t == "# demo, edited by the user\n") && wt.join("notes.txt").exists(),
        "README.md or notes.txt missing from the worktree",
    );
    fx.start(&task, 25, 6).await;
    let st = fx.wait(&task, 360).await;
    leg.check("run_ended", !st.loop_alive, format!("{st:?}"));
    tokio::time::sleep(Duration::from_millis(500)).await;
    leg.check("checkout_tree_identical", tree(&fx.root) == before, format!("{:?}", tree(&fx.root).keys().collect::<Vec<_>>()));
    leg.check("checkout_head_identical", git_out(&fx.root, &["rev-parse", "HEAD"]) == head_before, "HEAD moved");
    let calc = std::fs::read_to_string(wt.join("calc.py")).unwrap_or_default();
    leg.check("the_worktree_changed", calc != BUGGY_CALC, "calc.py in the worktree is the original");
    leg.check("the_fix_works_in_the_worktree", python_tests_pass(&wt.to_string_lossy()), "the unit tests fail in the worktree");
    let ev = of_task(fx.events().await, &task);
    leg.count("file_changes", of_kind(&ev, "FileChanged").len());
    leg.count("shell_calls_approved_by_the_person", approved.lock().unwrap().len());
    leg.check("the_model_changed_files", !of_kind(&ev, "FileChanged").is_empty(), "no FileChanged on the log");
    // Escapes are refused at the Kernel.
    let r = fx.invoke(&task, "change.apply", json!({"path": "../escape.txt", "op": "replace", "content": "x"})).await;
    leg.check("dotdot_refused", r.status == "APPLICATION_FAILURE" && r.error_code == "PATH_OUTSIDE_ROOT", format!("{r:?}"));
    let target = Path::new(&fx.root).join("calc.py");
    let r = fx.invoke(&task, "change.apply", json!({"path": target.to_string_lossy(), "op": "replace", "content": "pwned\n"})).await;
    leg.check("absolute_path_into_the_checkout_refused", r.status == "APPLICATION_FAILURE", format!("{r:?}"));
    std::os::unix::fs::symlink(&fx.root, wt.join("sneaky")).unwrap();
    let r = fx.invoke(&task, "change.apply", json!({"path": "sneaky/calc.py", "op": "replace", "content": "pwned\n"})).await;
    leg.check("symlink_into_the_checkout_refused", r.status == "APPLICATION_FAILURE", format!("{r:?}"));
    leg.check("nothing_reached_the_checkout", tree(&fx.root) == before, "the checkout changed after the escapes");
    approver.abort();
    // A Core killed in the middle of a live run resumes in the same worktree.
    let second = fx
        .task_isolated("Create the files f1.txt, f2.txt, f3.txt and f4.txt, each containing its own file name, one file per step. Call plan.update first and task.complete last.")
        .await;
    let wt2 = fx.data.path().join("worktrees").join(uuid_text(&second.value));
    let feed = collect_live(&fx.core, &fx.session, 0).await;
    fx.start(&second, 25, 6).await;
    until_events(&feed, 180, "the first file", |e| of_task(e.to_vec(), &second).iter().any(|x| x.kind == "FileChanged")).await;
    drop(feed);
    fx.restart().await;
    let view2 = fx.task_view(&second).await;
    leg.check("same_worktree_after_the_kill", same_dir(&view2.workspace_root, &wt2) && view2.isolation == TaskIsolation::Worktree as i32, format!("{}", view2.workspace_root));
    fx.start(&second, 25, 6).await;
    let st = fx.wait(&second, 360).await;
    leg.check("second_run_ended", !st.loop_alive, format!("{st:?}"));
    let in_wt = (1..=4).filter(|i| wt2.join(format!("f{i}.txt")).exists()).count();
    leg.count("files_in_the_worktree_after_resume", in_wt);
    leg.check("resumed_work_landed_in_the_worktree", in_wt >= 1, "no f*.txt in the worktree");
    leg.check("resumed_work_never_in_the_checkout", (1..=4).all(|i| !Path::new(&fx.root).join(format!("f{i}.txt")).exists()) && tree(&fx.root) == before, "a file reached the checkout");
    leg.note("the reaper of PX-065 (removes an old terminal worktree, spares the running one) is time-driven and model-independent; it is proven in the scripted suite only");
    for t in [&task, &second] {
        fx.charge(&mut leg, t).await;
    }
    leg.finish();
}

fn uuid_text(bytes: &[u8]) -> String {
    let h = hex::encode(bytes);
    format!("{}-{}-{}-{}-{}", &h[0..8], &h[8..12], &h[12..16], &h[16..20], &h[20..32])
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "live: needs MODBIT_LIVE=1 and the gateway credential (the live-core workflow)"]
async fn live_px118_worktree() {
    leg_px118_worktree(LiveEnv::from_process()).await;
}

// ---------------------------------------------------------------------------
// QUAL-PX-109: compaction on a live run driven to the model's window
// ---------------------------------------------------------------------------

use modbit_protocol::v1::{ContextInspectorView, GetContextInspector};

const NEEDLE: &str = "heliotrope-7421";

/// `parts` files of about 15 KB of plain lines (the most of a file the model
/// is shown in one result is 16 KiB); the needle sits in part 03.
fn big_parts(parts: usize) -> Vec<(String, String)> {
    (1..=parts)
        .map(|p| {
            let mut text = String::new();
            for i in 0..230 {
                if p == 3 && i == 40 {
                    text.push_str(&format!("NEEDLE-CODEWORD: {NEEDLE}\n"));
                }
                text.push_str(&format!(
                    "part {p:02} line {i:04}: the quick brown fox jumps over the lazy dog {i}\n"
                ));
            }
            (format!("part-{p:02}.txt"), text)
        })
        .collect()
}

struct CompactionRun {
    task: Id,
    events: Vec<Ev>,
    inspector: ContextInspectorView,
    status: TaskStatus,
}

/// A live run that reads file after file until the Core compacts, then goes on
/// to finish. `summarizer` is `MODBIT_COMPACTION_SUMMARIZER`.
async fn compaction_run(
    env: &Arc<LiveEnv>,
    leg: &mut Leg,
    parts: usize,
    summarizer: &str,
    catalog: Option<String>,
) -> (Fx, CompactionRun) {
    let files = big_parts(parts);
    let mut repo_files: Vec<(&str, &str)> = files.iter().map(|(n, t)| (n.as_str(), t.as_str())).collect();
    repo_files.push(("README.md", "# big files\n"));
    let mut extra: Vec<(&str, &str)> = vec![("MODBIT_COMPACTION_SUMMARIZER", summarizer)];
    let cat = catalog.clone().unwrap_or_default();
    if catalog.is_some() {
        extra.push(("MODBIT_OPENAI_MODELS", cat.as_str()));
    }
    let mut fx = Fx::with_env(env, &repo_files, &extra).await;
    let names: Vec<&str> = files.iter().map(|(n, _)| n.as_str()).collect();
    let goal = format!(
        "First call plan.update once (outcome: read the files, expected_files: an empty list). Then read these files in order, one fs.read call per file with max_bytes 60000, and call no other tool: {}. Do not skip any. When you have read them all, finish with task.complete whose summary states the code word that follows NEEDLE-CODEWORD: in one of them and one sentence on what the last file contains.",
        names.join(", ")
    );
    let task = fx.task(&goal).await;
    // Reading is not progress to the harness: the no-progress bound is
    // widened so that the run is not suspended before it reaches the window.
    fx.start(&task, 60, 40).await;
    let status = fx.wait(&task, 1500).await;
    leg.admit(0.05);
    tokio::time::sleep(Duration::from_millis(500)).await;
    let events = of_task(fx.events().await, &task);
    let inspector: ContextInspectorView = fx.ok("GetContextInspector", GetContextInspector { task_id: Some(task.clone()) }, false).await;
    (fx, CompactionRun { task, events, inspector, status })
}

/// The accounting records of every compiled turn, in order.
async fn compile_records(fx: &mut Fx, events: &[Ev]) -> Vec<Value> {
    let mut out = Vec::new();
    for e in events.iter().filter(|e| e.kind == "StepSucceeded") {
        let Some(hash) = e.payload["output_ref"].as_str() else {
            continue;
        };
        let bytes = fx.read_object(hash).await;
        if let Ok(v) = serde_json::from_slice::<Value>(&bytes)
            && v.get("accounting").is_some()
        {
            out.push(v["accounting"].clone());
        }
    }
    out
}

/// A run driven to the window compacts through the model summarizer (the run's
/// own model), installs the summary with its provenance and a transcript
/// pointer, resumes and completes; the accounting afterwards shows the
/// summarised-conversation category non-zero and the conversation category
/// fallen.
async fn leg_px109_compaction(env: Arc<LiveEnv>) {
    let mut leg = Leg::begin(&env, "px109", "compaction", 0.45);
    let (mut fx, run) = compaction_run(&env, &mut leg, 30, "run", None).await;
    leg.check("run_ended", !run.status.loop_alive && run.status.failure_class.is_empty(), format!("{:?}", run.status));
    let epochs = of_kind(&run.events, "ContextEpochOpened");
    leg.count("epochs", epochs.len());
    let peak_all = run.events.iter().filter(|e| e.kind == "ModelUsageRecorded").filter_map(|e| e.payload["input_tokens"].as_u64()).max().unwrap_or(0);
    leg.count("peak_provider_input_tokens", peak_all);
    leg.check(
        "the_run_compacted",
        !epochs.is_empty(),
        format!("no ContextEpochOpened: the run never reached its compaction budget (peak request {peak_all} tokens; trigger {:?})", run.inspector.compaction_thresholds),
    );
    let first = epochs[0];
    leg.check(
        "model_summary_installed",
        first.payload["summary_source"] == "MODEL" && first.payload["fallback_reason"] == "",
        format!("summary_source {} fallback_reason {}", first.payload["summary_source"], first.payload["fallback_reason"]),
    );
    leg.check(
        "summarizer_is_the_policy_named_model",
        payload_str(first, "summarizer").contains(&env.model),
        payload_str(first, "summarizer"),
    );
    let committed = of_kind(&run.events, "CompactionCommitted");
    leg.check("committed", !committed.is_empty() && committed[0].payload["summary_source"] == "MODEL", format!("{}", committed.len()));
    leg.check(
        "summarizer_usage_recorded_under_its_role",
        of_kind(&run.events, "ModelUsageRecorded").iter().any(|u| u.payload["route"]["role"] == "compaction-summarizer"),
        "no usage under the compaction-summarizer role",
    );
    // The threshold follows the model's window; the run really was near it.
    let th = run.inspector.compaction_thresholds.clone().expect("thresholds");
    let window = env.window.unwrap_or(128_000);
    leg.count("window_tokens", th.context_window_tokens);
    leg.count("hard_budget_tokens", th.hard_budget_tokens);
    leg.check("threshold_from_the_model_window", th.source == "MODEL_WINDOW" && u64::from(th.context_window_tokens) == window, format!("{th:?}"));
    leg.check("budget_is_a_share_of_the_window", u64::from(th.hard_budget_tokens) > window / 2 && u64::from(th.hard_budget_tokens) < window, format!("{th:?}"));
    let epoch_at = run.events.iter().position(|e| e.kind == "ContextEpochOpened").unwrap();
    let before: Vec<u64> = run
        .events
        .iter()
        .take(epoch_at)
        .filter(|e| e.kind == "ModelUsageRecorded" && e.payload["route"]["role"] != "compaction-summarizer")
        .filter_map(|e| e.payload["input_tokens"].as_u64())
        .collect();
    let peak = before.iter().copied().max().unwrap_or(0);
    leg.count("provider_input_tokens_at_the_epoch", peak);
    leg.count("percent_of_window_at_the_epoch", peak * 100 / window.max(1));
    leg.check("driven_near_the_window", peak * 100 / window.max(1) >= 55, format!("the largest request before the epoch was {peak} tokens of a {window}-token window"));
    // The summary's provenance and the transcript pointer.
    let s = &run.inspector.compaction_summaries[0];
    leg.check("inspector_names_the_model", s.summary_source == "MODEL" && s.transcript_ref.len() == 64, format!("{s:?}"));
    let stored = fx.read_object(&s.transcript_ref).await;
    leg.check("pointer_holds_the_exact_earlier_text", String::from_utf8_lossy(&stored).contains("part 01 line 0000"), format!("{} bytes", stored.len()));
    let pointer_uses = of_kind(&run.events, "ToolCallSucceeded").iter().filter(|e| e.payload["tool_name"] == "artifact.range").count();
    leg.count("model_fetches_through_the_pointer", pointer_uses);
    // The accounting after the epoch.
    let v = fx.accounting(&run.task).await;
    leg.check("accounting_sees_the_epoch", v.compaction_epoch >= 1 && v.compaction_summaries >= 1, format!("epoch {} summaries {}", v.compaction_epoch, v.compaction_summaries));
    leg.check("summary_category_non_zero_with_sources", category(&v, "SUMMARY").tokens > 0 && !category(&v, "SUMMARY").sources.is_empty(), format!("{:?}", category(&v, "SUMMARY")));
    let records = compile_records(&mut fx, &run.events).await;
    let est = |r: &Value, k: &str| r["estimates"][k].as_u64().unwrap_or(0);
    let peak_conv = records.iter().map(|r| est(r, "CONVERSATION")).max().unwrap_or(0);
    let last = records.last().expect("a compiled turn");
    leg.count("conversation_peak_estimate", peak_conv);
    leg.count("conversation_last_estimate", est(last, "CONVERSATION"));
    leg.check("conversation_falls_after_the_epoch", est(last, "CONVERSATION") < peak_conv, format!("peak {peak_conv}, last {}", est(last, "CONVERSATION")));
    leg.check("summary_grows_after_the_epoch", est(last, "SUMMARY") > est(&records[0], "SUMMARY"), format!("{} vs {}", est(last, "SUMMARY"), est(&records[0], "SUMMARY")));
    // The task completed: the model finished with task.complete.
    let completes: Vec<&Ev> = run.events.iter().filter(|e| e.payload.to_string().contains("task.complete")).collect();
    leg.check("the_run_finished_with_task_complete", !completes.is_empty(), "no task.complete on the log");
    let answer: String = completes.iter().map(|e| e.payload.to_string()).collect();
    leg.count("final_answer_has_the_needle", answer.contains(NEEDLE));
    leg.note(format!("completion: {}", answer.chars().take(300).collect::<String>()));
    leg.note("the registered window is the catalog's (ctx defaults to 128000 when MODBIT_OPENAI_MODELS does not name one); the Core's trigger is 85 percent of the window less the output budget and a margin, not of the raw window");
    fx.charge(&mut leg, &run.task).await;
    leg.finish();
}

/// A summarizer that fails for real (the gateway rejects a model it does not
/// know) costs the run nothing but the summary: the epoch is the extractive
/// one with a typed reason, and the run still completes.
async fn leg_px109_fallback(env: Arc<LiveEnv>) {
    let mut leg = Leg::begin(&env, "px109", "fallback", 0.45);
    let catalog = format!(
        "{},ghost-summarizer-px109={}/{}",
        env.catalog,
        env.price.input_per_mtok_usd,
        env.price.output_per_mtok_usd
    );
    let (mut fx, run) = compaction_run(&env, &mut leg, 30, "openai/ghost-summarizer-px109", Some(catalog)).await;
    leg.check("run_ended_despite_the_summarizer", !run.status.loop_alive && run.status.failure_class.is_empty(), format!("{:?}", run.status));
    let epochs = of_kind(&run.events, "ContextEpochOpened");
    leg.check("the_run_compacted", !epochs.is_empty(), "no ContextEpochOpened");
    let first = epochs[0];
    leg.check(
        "extractive_fallback_with_a_typed_reason",
        first.payload["summary_source"] == "EXTRACTIVE" && !payload_str(first, "fallback_reason").is_empty(),
        format!("{} / {}", first.payload["summary_source"], first.payload["fallback_reason"]),
    );
    leg.count("fallback_reason", payload_str(first, "fallback_reason"));
    leg.check("extractive_epoch_installed", !of_kind(&run.events, "CompactionCommitted").is_empty(), "no CompactionCommitted");
    leg.check("finished_with_task_complete", run.events.iter().any(|e| e.payload.to_string().contains("task.complete")), "no task.complete on the log");
    fx.charge(&mut leg, &run.task).await;
    leg.finish();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "live: needs MODBIT_LIVE=1 and the gateway credential (the live-core workflow)"]
async fn live_px109_compaction() {
    leg_px109_compaction(LiveEnv::from_process()).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "live: needs MODBIT_LIVE=1 and the gateway credential (the live-core workflow)"]
async fn live_px109_fallback() {
    leg_px109_fallback(LiveEnv::from_process()).await;
}

// ---------------------------------------------------------------------------
// QUAL-PX-116: hierarchical budgets, max_children and read scope, with live
// children
// ---------------------------------------------------------------------------

use modbit_protocol::v1::{ActivateModelRegistry, ModelRegistryView, SetTaskBudgets};

/// The registry prices in hundredths of a cent (scale 4, the most the router
/// accepts): 0.15 USD per million input tokens is 1 500 minor units per
/// million, so a request of ten thousand tokens costs 15.
fn registry_prices(env: &LiveEnv) -> (u64, u64) {
    (
        (env.price.input_per_mtok_usd * 10_000.0).round() as u64,
        (env.price.output_per_mtok_usd * 10_000.0).round() as u64,
    )
}

/// A signed registry that binds the live model with its catalog price at
/// scale 4, so that the Core can meter cost caps in minor units.
fn signed_registry(env: &LiveEnv, key: &ed25519_dalek::SigningKey) -> String {
    use ed25519_dalek::Signer;
    use modbit_providers::registry::{
        Economics, Governance, Latency, QualityFloor, REGISTRY_SCHEMA_VERSION, RegistryDocument,
        RegistryEntry, SignedRegistry,
    };
    let (input, output) = registry_prices(env);
    let now = modbit_domain::Timestamp::now().0;
    let document = RegistryDocument {
        promotion: None,
        schema_version: REGISTRY_SCHEMA_VERSION,
        registry_generation: "registry-live-core".into(),
        stats_version: "stats-1".into(),
        issued_at_ms: now - 60_000,
        expires_at_ms: now + 86_400_000,
        quality_floors: vec![QualityFloor {
            mode: "auto".into(),
            min_quality: 0.72,
            max_cost_minor: 50_000,
            currency: "USD".into(),
            scale: 4,
        }],
        entries: vec![RegistryEntry {
            endpoint: "openai".into(),
            provider: "openai".into(),
            family: "glm".into(),
            model: env.model.clone(),
            roles: vec!["solver".into(), "reviewer".into()],
            input_modalities: vec!["text".into()],
            context_tokens: u32::try_from(env.window.unwrap_or(128_000)).unwrap_or(128_000),
            max_output_tokens: 16_384,
            tools: true,
            vision: false,
            reasoning: false,
            structured_output: true,
            economics: Economics {
                input_per_mtok_minor: input,
                output_per_mtok_minor: output,
                currency: "USD".into(),
                scale: 4,
                cached_input_per_mtok_minor: Some(input),
                cache_write_per_mtok_minor: None,
                cache_ttl_ms: None,
            },
            latency: Latency {
                p50_ms: 900,
                p95_ms: 12_000,
            },
            governance: Governance {
                data_residency: "us".into(),
                retains_prompts: false,
                allowed_profiles: vec![],
            },
            revoked: false,
            fallbacks: vec![],
        }],
    };
    let json = serde_json::to_string(&document).unwrap();
    serde_json::to_string(&SignedRegistry {
        key_id: "ops".into(),
        signature_hex: hex::encode(key.sign(json.as_bytes()).to_bytes()),
        document_json: json,
    })
    .unwrap()
}

impl Fx {
    async fn activate_registry(&mut self, signed: &str) {
        let r: ModelRegistryView = self
            .ok(
                "ActivateModelRegistry",
                ActivateModelRegistry {
                    signed_json: signed.to_owned(),
                    expected_generation: String::new(),
                },
                false,
            )
            .await;
        assert!(r.active, "{r:?}");
    }

    async fn set_budgets(&mut self, task: &Id, cost: u64, wall: u64, children: u32, forbid: bool) {
        let _: modbit_protocol::v1::TaskBudgetsView = self
            .ok(
                "SetTaskBudgets",
                SetTaskBudgets {
                    task_id: Some(task.clone()),
                    max_cost_minor: cost,
                    max_wall_ms: wall,
                    max_children: children,
                    forbid_spawn: forbid,
                },
                true,
            )
            .await;
    }
}

/// The cost of each reported model call an event list records, priced the way
/// the ledger prices it: each part rounded up.
fn call_costs(env: &LiveEnv, events: &[Ev]) -> Vec<u64> {
    let (input, output) = registry_prices(env);
    let c = |tokens: u64, price: u64| (u128::from(tokens) * u128::from(price)).div_ceil(1_000_000) as u64;
    events
        .iter()
        .filter(|e| e.kind == "ModelUsageRecorded" && e.payload["reported"].as_bool() == Some(true))
        .map(|e| {
            let cached = e.payload["cached_input_tokens"].as_u64().unwrap_or(0);
            c(e.payload["input_tokens"].as_u64().unwrap_or(0).saturating_sub(cached), input)
                + c(cached, input)
                + c(e.payload["output_tokens"].as_u64().unwrap_or(0), output)
        })
        .collect()
}

fn child_id(admitted: &Ev) -> Id {
    Id {
        value: modbit_domain::TaskId::parse(admitted.payload["child_task_id"].as_str().unwrap_or_default())
            .map(|t| t.as_bytes().to_vec())
            .unwrap_or_default(),
    }
}

/// The arguments of one `agent.spawn`, with a write scope of its own (a child
/// with no named scope is unbounded and cannot run beside another worker).
fn spawn_call(key: &str, objective: &str, extra: &str) -> String {
    format!("{{\"idempotency_key\":\"{key}\",\"objective\":\"{objective}\",\"write_scope\":[\"scope/{key}/\"]{extra}}}")
}

/// Live children under a parent's cost cap: three children whose requests
/// exceed what the parent has left (the third is clamped, the reservations
/// never exceed the cap), a child that exhausts its cost ends typed
/// `BUDGET_EXHAUSTED`, child spend is in the parent's accounting and nothing
/// stays reserved; `max_children` refuses the extra spawn typed; a
/// configuration layer that denies `agent.spawn` removes delegation; a child
/// reads only its `read_scope`; and a Core killed between the reservation and
/// the child's start comes back with the child admitted once and started.
async fn leg_px116_budgets(env: Arc<LiveEnv>) {
    let mut leg = Leg::begin(&env, "px116", "budgets", 0.60);
    let key = ed25519_dalek::SigningKey::from_bytes(&[116u8; 32]);
    let keys = format!("ops:{}", hex::encode(key.verifying_key().to_bytes()));
    let signed = signed_registry(&env, &key);
    let mut fx = Fx::with_env(
        &env,
        &[
            ("README.md", "# Split work\n"),
            ("src/a/x.txt", "needle alpha\n"),
            ("src/secret/key.txt", "needle SECRET-KEY-VALUE\n"),
            ("scope/k1/.keep", ""),
            ("scope/k2/.keep", ""),
            ("scope/k3/.keep", ""),
        ],
        &[("MODBIT_REGISTRY_KEYS", keys.as_str()), ("MODBIT_CAPACITY", "model=4,provider=8")],
    )
    .await;
    fx.activate_registry(&signed).await;
    let (approver, _approved) = spawn_approver_for(&fx.core, &fx.session, fx.g, |t| t.starts_with("git.merge.") || t.starts_with("git.apply.")).await;

    // ---- A. the cost cap: clamp, exhaust, roll up ----------------------------
    let cap: u64 = 400;
    let parent = fx
        .task(&format!(
            "Do exactly this and nothing else. Step 1: call plan.update with outcome \"three read-only children\" and expected_files [\"README.md\"]. Step 2: call agent.spawn three times with exactly these arguments: {} then {} then {}. Step 3: call agent.wait for k1, then k2, then k3 (timeout_ms 120000 each). Step 4: call task.complete with a one-sentence summary.",
            spawn_call("k1", "CHILD-A read README.md and report its title in one sentence", ",\"max_cost_minor\":120"),
            spawn_call("k2", "CHILD-B read README.md and report its title in one sentence", ",\"max_cost_minor\":6"),
            spawn_call("k3", "CHILD-C read README.md and report its title in one sentence", ",\"max_cost_minor\":2000"),
        ))
        .await;
    fx.set_budgets(&parent, cap, 0, 0, false).await;
    fx.start(&parent, 40, 8).await;
    let st = fx.wait(&parent, 600).await;
    leg.check("parent_run_ended", !st.loop_alive, format!("{st:?}"));
    tokio::time::sleep(Duration::from_millis(800)).await;
    let all = fx.events().await;
    let evs = of_task(all.clone(), &parent);
    let admitted: Vec<&Ev> = of_kind(&evs, "SubagentAdmitted");
    leg.check("three_children_admitted", admitted.len() == 3, format!("{} admitted; refusals {:?}", admitted.len(), of_kind(&evs, "SubagentAdmissionRefused").iter().map(|e| &e.payload).collect::<Vec<_>>()));
    let by_key = |k: &str| admitted.iter().find(|a| a.payload["idempotency_key"] == k).copied();
    let (k1, k2, k3) = (by_key("k1"), by_key("k2"), by_key("k3"));
    leg.check("keys_k1_k2_k3", k1.is_some() && k2.is_some() && k3.is_some(), "the model did not use the three keys");
    let (k1, k2, k3) = (k1.unwrap(), k2.unwrap(), k3.unwrap());
    let reserved = |a: &Ev| a.payload["reserved_cost_minor"].as_u64().unwrap_or(0);
    leg.count("reserved_k1", reserved(k1));
    leg.count("reserved_k2", reserved(k2));
    leg.count("reserved_k3", reserved(k3));
    leg.check("first_two_reserved_as_asked", reserved(k1) == 120 && reserved(k2) == 6, format!("{} / {}", reserved(k1), reserved(k2)));
    leg.check("third_clamped_below_its_request", reserved(k3) < 2_000, format!("{}", reserved(k3)));
    leg.check(
        "clamp_is_named",
        k3.payload["clamped"].as_array().is_some_and(|a| a.iter().any(|n| n.as_str().is_some_and(|n| n.starts_with("max_cost_minor 2000 ->")))),
        format!("{}", k3.payload["clamped"]),
    );
    let own_before_third: u64 = {
        let at = evs.iter().position(|e| std::ptr::eq(e, k3)).unwrap_or(evs.len());
        call_costs(&env, &evs[..at]).iter().sum()
    };
    let total_reserved = reserved(k1) + reserved(k2) + reserved(k3);
    leg.check(
        "reservations_never_exceed_what_the_parent_could_give",
        own_before_third + total_reserved <= cap,
        format!("own {own_before_third} + reserved {total_reserved} against a cap of {cap}"),
    );
    // The cost-capped child ended with the typed stop.
    let k2_evs = of_task(all.clone(), &child_id(k2));
    let stop = k2_evs.iter().find(|e| e.kind == "HarnessBudgetExhausted");
    leg.check(
        "cost_capped_child_exhausted_typed",
        stop.is_some_and(|s| s.payload["budget"] == "max_cost_minor" && s.payload["limit"] == 6 && s.payload["used"].as_u64().unwrap_or(0) >= 6),
        format!("{:?}", stop.map(|s| &s.payload)),
    );
    let results = of_kind(&evs, "SubagentResultRecorded");
    leg.check("budget_exhausted_result_recorded", results.iter().any(|p| p.payload["status"] == "BUDGET_EXHAUSTED"), format!("{:?}", results.iter().map(|p| &p.payload["status"]).collect::<Vec<_>>()));
    leg.count("children_completed", results.iter().filter(|p| p.payload["status"] == "COMPLETED").count());
    // Each child's spend never passes its reservation by more than the call that crossed it.
    let mut kids_spend = 0;
    for a in [k1, k2, k3] {
        let costs = call_costs(&env, &of_task(all.clone(), &child_id(a)));
        let spent: u64 = costs.iter().sum();
        leg.check(
            &format!("child_{}_within_its_reservation", a.payload["idempotency_key"].as_str().unwrap_or("?")),
            spent <= reserved(a) + costs.iter().max().copied().unwrap_or(0),
            format!("spent {spent} against {}", reserved(a)),
        );
        kids_spend += spent;
    }
    let e = fx.economics(&parent).await;
    let own_total: u64 = call_costs(&env, &evs).iter().sum();
    leg.count("parent_own_cost_minor", own_total);
    leg.count("children_cost_minor", kids_spend);
    leg.check("parent_cost_is_its_own_calls", e.cost_minor == own_total, format!("{} vs {own_total}", e.cost_minor));
    leg.check("child_spend_is_in_the_parents_accounting", e.children_cost_minor == kids_spend && e.subtree_cost_minor == own_total + kids_spend, format!("{e:?}"));
    leg.check("parent_total_inside_its_cap", e.subtree_cost_minor <= cap && e.cost_cap_minor == cap, format!("subtree {} of {cap}", e.subtree_cost_minor));
    leg.check("nothing_stays_reserved", e.cost_reserved_minor == 0, format!("{}", e.cost_reserved_minor));

    // ---- B. max_children ------------------------------------------------------
    let p2 = fx
        .task(&format!(
            "Do exactly this and nothing else. Step 1: call plan.update with outcome \"two read-only children\" and expected_files [\"README.md\"]. Step 2: in one reply call agent.spawn twice, in this order, with exactly these arguments: {} and then {}. Step 3: call agent.wait for k1 (timeout_ms 120000). Step 4: call task.complete with a one-sentence summary.",
            spawn_call("k1", "CHILD-D read README.md, then read src/a/x.txt, and report both in one sentence", ""),
            spawn_call("k2", "CHILD-E read README.md and report its title in one sentence", ""),
        ))
        .await;
    fx.set_budgets(&p2, 0, 0, 1, false).await;
    fx.start(&p2, 40, 8).await;
    let st = fx.wait(&p2, 600).await;
    leg.check("max_children_run_ended", !st.loop_alive, format!("{st:?}"));
    tokio::time::sleep(Duration::from_millis(500)).await;
    let evs2 = of_task(fx.events().await, &p2);
    let refused = of_kind(&evs2, "SubagentAdmissionRefused");
    leg.check(
        "extra_spawn_refused_with_a_typed_reason",
        of_kind(&evs2, "SubagentAdmitted").len() == 1
            && refused.len() == 1
            && refused[0].payload["code"] == "MAX_CHILDREN_EXCEEDED"
            && refused[0].payload["stage"] == "LIMIT"
            && refused[0].payload["rolled_back"] == json!([]),
        format!("admitted {}, refusals {:?}", of_kind(&evs2, "SubagentAdmitted").len(), refused.iter().map(|r| &r.payload).collect::<Vec<_>>()),
    );

    // ---- D. the read scope ----------------------------------------------------
    let p3 = fx
        .task(&format!(
            "Do exactly this and nothing else. Step 1: call plan.update with outcome \"one scoped child\" and expected_files [\"README.md\"]. Step 2: call agent.spawn once with exactly these arguments: {}. Step 3: call agent.wait for k1 (timeout_ms 120000). Step 4: call task.complete with a one-sentence summary.",
            spawn_call("k1", "CHILD-F read the file src/secret/key.txt and report its exact content, then read src/a/x.txt", ",\"read_scope\":[\"src/a/\"]"),
        ))
        .await;
    fx.start(&p3, 40, 8).await;
    let st = fx.wait(&p3, 600).await;
    leg.check("scope_run_ended", !st.loop_alive, format!("{st:?}"));
    tokio::time::sleep(Duration::from_millis(500)).await;
    let all3 = fx.events().await;
    let evs3 = of_task(all3.clone(), &p3);
    let adm3 = of_kind(&evs3, "SubagentAdmitted");
    leg.check("scoped_child_admitted", adm3.len() == 1, format!("{}", adm3.len()));
    let kid = of_task(all3, &child_id(adm3[0]));
    let denied = kid.iter().filter(|e| e.kind == "ToolCallPolicyDecision" && e.payload["allowed"] == false).count();
    leg.count("child_reads_refused_by_the_kernel", denied);
    leg.check("read_outside_scope_refused_by_the_kernel", denied >= 1, "the child never attempted the read, or the Kernel allowed it");
    leg.check("secret_never_reached_the_child", !kid.iter().any(|e| e.payload.to_string().contains("SECRET-KEY-VALUE")), "the secret is in the child's log");
    approver.abort();

    // ---- C. a configuration layer that denies agent.spawn -------------------------
    fx.core.kill();
    std::fs::write(fx.data.path().join("config.json"), r#"{"permissions":{"agent.spawn":"DENY"}}"#).unwrap();
    fx.core = Fx::spawn(&fx.env, fx.data.path(), &fx.extra_env);
    fx.c = fx.core.client().await;
    fx.g = Some(fx.lease_again().await);
    fx.activate_registry(&signed).await;
    let p4 = fx.task("Read README.md with fs.read and finish with task.complete. If delegation is available you may delegate, otherwise do it yourself.").await;
    fx.start(&p4, 12, 4).await;
    let st = fx.wait(&p4, 300).await;
    leg.check("forbidden_run_ended", !st.loop_alive, format!("{st:?}"));
    let evs4 = of_task(fx.events().await, &p4);
    let projections = of_kind(&evs4, "ToolProjectionSelected");
    leg.check("projection_recorded", !projections.is_empty(), "no ToolProjectionSelected");
    leg.check(
        "no_agent_tool_offered_under_the_deny",
        projections.iter().all(|e| !e.payload["projected"].to_string().contains("agent.")),
        format!("{}", projections[0].payload["projected"]),
    );
    leg.check("no_child_admitted_under_the_deny", of_kind(&evs4, "SubagentAdmitted").is_empty(), "a child was admitted");
    std::fs::remove_file(fx.data.path().join("config.json")).unwrap();

    // ---- E. a kill between the reservation and the child's start ------------------
    fx.core.kill();
    fx.extra_env.push(("MODBIT_FAULT_SPAWN_KILL".into(), "AFTER_ADMIT".into()));
    fx.core = Fx::spawn(&fx.env, fx.data.path(), &fx.extra_env);
    fx.c = fx.core.client().await;
    fx.g = Some(fx.lease_again().await);
    fx.activate_registry(&signed).await;
    let p5 = fx
        .task(&format!(
            "Do exactly this and nothing else. Step 1: call plan.update with outcome \"one read-only child\" and expected_files [\"README.md\"]. Step 2: call agent.spawn once with exactly these arguments: {}. Step 3: call agent.wait for k1 (timeout_ms 120000). Step 4: call task.complete with a one-sentence summary.",
            spawn_call("k1", "CHILD-G read README.md and report its title in one sentence", ",\"max_cost_minor\":120"),
        ))
        .await;
    fx.set_budgets(&p5, cap, 0, 0, false).await;
    let _ = fx.c.command(envelope_fenced(rand_id(), "StartTask", StartTask { task_id: Some(p5.clone()), endpoint: String::new(), model: env.model.clone(), max_turns: 40, max_tool_calls: 0, max_no_progress_turns: 8, skills: vec![], ..Default::default() }.encode_to_vec(), fx.g)).await;
    // The Core aborts at the injected kill; wait for its socket to go quiet.
    let deadline = Instant::now() + Duration::from_secs(300);
    loop {
        if fx.core.has_exited() || Instant::now() > deadline {
            break;
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    fx.core.kill();
    fx.extra_env.retain(|(k, _)| k != "MODBIT_FAULT_SPAWN_KILL");
    fx.core = Fx::spawn(&fx.env, fx.data.path(), &fx.extra_env);
    fx.c = fx.core.client().await;
    fx.g = Some(fx.lease_again().await);
    fx.activate_registry(&signed).await;
    let (approver2, _) = spawn_approver_for(&fx.core, &fx.session, fx.g, |t| t.starts_with("git.merge.") || t.starts_with("git.apply.")).await;
    let evs5 = of_task(fx.events().await, &p5);
    let adm5: Vec<&Ev> = of_kind(&evs5, "SubagentAdmitted");
    leg.check("reservation_is_on_the_log_once", adm5.len() == 1 && adm5[0].payload["reserved_cost_minor"] == 120, format!("{} admissions", adm5.len()));
    let kid5 = child_id(adm5[0]);
    fx.start(&p5, 40, 8).await;
    let st = fx.wait(&p5, 600).await;
    leg.check("parent_resumed_and_ended", !st.loop_alive, format!("{st:?}"));
    let all5 = fx.events().await;
    let evs5 = of_task(all5.clone(), &p5);
    leg.check("still_one_admission", of_kind(&evs5, "SubagentAdmitted").len() == 1, "the restart admitted the child again");
    leg.check("child_started_from_its_reservation", of_task(all5, &kid5).iter().any(|e| e.kind == "RunStarted"), "the admitted child never started");
    let e5 = fx.economics(&p5).await;
    leg.check("nothing_reserved_after_reconciliation", e5.cost_reserved_minor == 0 && e5.subtree_cost_minor <= cap, format!("{e5:?}"));
    approver2.abort();
    leg.note("the wall-clock exhaustion of a child (BUDGET_EXHAUSTED on max_wall_ms and the return of its reservation) is time-driven and model-independent; it is proven in the scripted suite only");
    // Everything the session used, children included, from the usage the
    // gateway reported for each call on the log.
    let used: Vec<Ev> = fx.events().await.into_iter().filter(|e| e.kind == "ModelUsageRecorded" && e.payload["reported"].as_bool() == Some(true)).collect();
    leg.charge(
        used.len() as u64,
        used.iter().map(|e| e.payload["input_tokens"].as_u64().unwrap_or(0)).sum(),
        used.iter().map(|e| e.payload["output_tokens"].as_u64().unwrap_or(0)).sum(),
    );
    leg.finish();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "live: needs MODBIT_LIVE=1 and the gateway credential (the live-core workflow)"]
async fn live_px116_budgets() {
    leg_px116_budgets(LiveEnv::from_process()).await;
}

// ---------------------------------------------------------------------------
// QUAL-PX-057: run-mode presets and durable allowlist rules over the real
// approval flow
// ---------------------------------------------------------------------------

use modbit_protocol::v1::{
    AddAllowRule, AllowRuleRevoked, AllowRuleView, ApprovalView, EffectReceiptList,
    EffectReceiptView, GetEffectReceipts, GetRunMode, RevokeAllowRule, RunModeView, SetRunMode,
};

/// A directory with one executable `mytool` that writes `out.txt` in its
/// working directory: a program the classifier does not know, so a call to it
/// is a protected effect that asks in the default mode.
fn tool_dir() -> tempfile::TempDir {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("mytool");
    std::fs::write(&path, "#!/bin/sh\necho \"ran $*\" > out.txt\n").unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    dir
}

impl Fx {
    async fn pending_for(&mut self, task: &Id) -> Vec<ApprovalView> {
        let l: ApprovalList = self
            .ok(
                "ListApprovals",
                ListApprovals {
                    session_id: Some(self.session.clone()),
                },
                false,
            )
            .await;
        l.approvals
            .into_iter()
            .filter(|a| a.status == "REQUESTED" && a.task_id.as_ref() == Some(task))
            .collect()
    }

    /// Start the task and wait for either an approval to appear or the run to
    /// end (the live model decides which).
    async fn run_until_asked_or_done(&mut self, task: &Id, turns: u32) -> Vec<ApprovalView> {
        self.start(task, turns, 3).await;
        let deadline = Instant::now() + Duration::from_secs(240);
        loop {
            let asked = self.pending_for(task).await;
            if !asked.is_empty() {
                return asked;
            }
            if !self.status(task).await.loop_alive {
                // The run can end between the approval being asked and our poll.
                return self.pending_for(task).await;
            }
            assert!(Instant::now() < deadline, "the run neither asked nor finished");
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }

    async fn decide(&mut self, a: &ApprovalView, approve: bool) {
        let _: modbit_protocol::v1::ApprovalResolvedAck = self
            .ok(
                "ResolveApproval",
                ResolveApproval {
                    approval_id: a.approval_id.clone(),
                    approve,
                    reason: "live leg".into(),
                    intent_hash: a.intent_hash.clone(),
                },
                true,
            )
            .await;
    }

    /// Deny everything else the task asks until its run ends (the model may
    /// try another way after a refusal).
    async fn deny_until_done(&mut self, task: &Id) {
        let deadline = Instant::now() + Duration::from_secs(240);
        loop {
            for a in self.pending_for(task).await {
                self.decide(&a, false).await;
            }
            if !self.status(task).await.loop_alive || Instant::now() > deadline {
                return;
            }
            tokio::time::sleep(Duration::from_millis(150)).await;
        }
    }

    async fn receipts(&mut self, task: &Id) -> Vec<EffectReceiptView> {
        let l: EffectReceiptList = self
            .ok("GetEffectReceipts", GetEffectReceipts { task_id: Some(task.clone()) }, false)
            .await;
        assert!(l.chain_valid, "the receipt chain is broken");
        l.receipts
    }

    async fn run_mode(&mut self, task: &Id, mode: &str, ack: bool) -> Result<RunModeView, (String, String)> {
        self.cmd("SetRunMode", SetRunMode { task_id: Some(task.clone()), mode: mode.into(), acknowledge_risk: ack }, true).await
    }

    async fn allow_rule(&mut self, task: &Id, pattern: &[&str], scope: &str, fenced: bool) -> Result<AllowRuleView, (String, String)> {
        self.cmd(
            "AddAllowRule",
            AddAllowRule {
                task_id: Some(task.clone()),
                rule_id: String::new(),
                pattern: pattern.iter().map(|p| (*p).to_owned()).collect(),
                scope: scope.into(),
                expires_at_ms: 0,
                covers_always_ask: false,
            },
            fenced,
        )
        .await
    }
}

fn command_goal(argv: &[&str]) -> String {
    format!(
        "Run exactly this one command with shell.exec, arguments {}, then finish with task.complete. Run nothing else, and if it is refused do not try another way.",
        serde_json::to_string(&json!({"argv": argv, "inherit_env": true})).unwrap()
    )
}

fn scope_json(a: &ApprovalView) -> Value {
    serde_json::from_str(&a.scope_json).unwrap_or_default()
}

/// With the live gateway and the real approval flow: in Ask a shell write asks
/// with its exact intent; after `AddAllowRule` for an exact argv prefix the
/// same command runs without asking and a receipt names the rule; a command
/// that adds a redirection or a pipeline to the prefix asks again; an
/// outside-workspace write and a network fetch ask even in Run everything;
/// revoking a rule makes the next run ask; Run everything is gone after a Core
/// restart while a durable mode survives.
async fn leg_px057_allowlist(env: Arc<LiveEnv>) {
    let mut leg = Leg::begin(&env, "px057", "allowlist", 0.30);
    let bin = tool_dir();
    let path = format!("{}:{}", bin.path().display(), std::env::var("PATH").unwrap_or_default());
    let mut fx = Fx::with_env(&env, &[("a.txt", "alpha\n")], &[("PATH", path.as_str())]).await;
    let out = Path::new(&fx.root).join("out.txt");
    let mut tasks: Vec<Id> = Vec::new();

    // ---- Ask: the command asks with its exact intent ----
    let t1 = fx.task(&command_goal(&["mytool", "build"])).await;
    tasks.push(t1.clone());
    let mode: RunModeView = fx.ok("GetRunMode", GetRunMode { task_id: Some(t1.clone()) }, false).await;
    leg.check("default_mode_is_ask", mode.mode == "ASK", mode.mode.clone());
    let asked = fx.run_until_asked_or_done(&t1, 6).await;
    leg.check("a_protected_write_asks_in_ask", asked.len() == 1 && asked[0].tool_name == "shell.exec" && asked[0].effect_class == "ProtectedWrite", format!("{:?}", asked.iter().map(|a| (&a.tool_name, &a.effect_class)).collect::<Vec<_>>()));
    let scope = scope_json(&asked[0]);
    leg.check("ask_names_the_mode_and_reason", scope["run_mode"] == "ASK" && scope["ask_reason"] == "MODE_ASK", format!("{scope}"));
    leg.check("approval_binds_the_exact_intent", !asked[0].intent_hash.is_empty() && scope["intent_hash"] == asked[0].intent_hash.as_str(), format!("{scope}"));
    leg.check("nothing_ran_before_the_decision", !out.exists(), "out.txt exists before the person decided");
    fx.decide(&asked[0], true).await;
    let st = fx.wait(&t1, 240).await;
    leg.check("approved_run_ended", !st.loop_alive, format!("{st:?}"));
    leg.check("approved_command_ran", out.exists(), "out.txt was not written");
    let r1 = fx.receipts(&t1).await;
    leg.check("receipt_names_the_approval", r1.iter().any(|r| r.status == "AUTHORIZED" && r.policy_decision.starts_with("approval:")), format!("{:?}", r1.iter().map(|r| (&r.status, &r.policy_decision)).collect::<Vec<_>>()));
    std::fs::remove_file(&out).unwrap();

    // ---- rules need the lease and a literal prefix; the mode needs the acknowledgement ----
    let t2 = fx.task(&command_goal(&["mytool", "build"])).await;
    tasks.push(t2.clone());
    let r = fx.allow_rule(&t2, &["mytool", "build"], "REPO", false).await;
    leg.check("rule_without_the_lease_refused", r.as_ref().err().is_some_and(|(c, _)| c == "LEASE_REQUIRED"), format!("{r:?}"));
    for (pattern, code) in [(vec!["mytool", "build", "|", "tee"], "PATTERN_SHELL_SYNTAX"), (vec!["sh", "-c"], "PATTERN_WRAPPER"), (vec!["git"], "PATTERN_TOO_BROAD")] {
        let r = fx.allow_rule(&t2, &pattern, "REPO", true).await;
        leg.check(&format!("pattern_{code}"), r.as_ref().err().is_some_and(|(c, _)| c == code), format!("{r:?}"));
    }
    let rule = fx.allow_rule(&t2, &["mytool", "build"], "REPO", true).await.unwrap();
    leg.check("rule_is_active_by_a_user", rule.state == "ACTIVE" && rule.created_by.starts_with("user:") && rule.scope_key == fx.root, format!("{rule:?}"));
    let r = fx.run_mode(&t2, "ALLOWLIST", false).await;
    leg.check("mode_needs_the_acknowledgement", r.as_ref().err().is_some_and(|(c, _)| c == "ACKNOWLEDGEMENT_REQUIRED"), format!("{r:?}"));
    let v = fx.run_mode(&t2, "ALLOWLIST", true).await.unwrap();
    leg.check("allowlist_in_force", v.mode == "ALLOWLIST" && v.rules_in_force == 1, format!("{v:?}"));
    // ---- the covered command runs silently and the receipt names the rule ----
    let asked = fx.run_until_asked_or_done(&t2, 6).await;
    leg.check("covered_command_does_not_ask", asked.is_empty(), format!("{asked:?}"));
    let st = fx.wait(&t2, 240).await;
    leg.check("covered_run_ended", !st.loop_alive, format!("{st:?}"));
    leg.check("covered_command_ran", out.exists(), "out.txt was not written");
    let r2 = fx.receipts(&t2).await;
    leg.check("receipt_names_the_rule", r2.iter().any(|r| r.status == "AUTHORIZED" && r.policy_decision == format!("allowlist:{}", rule.rule_id)), format!("{:?}", r2.iter().map(|r| (&r.status, &r.policy_decision)).collect::<Vec<_>>()));
    std::fs::remove_file(&out).unwrap();
    // ---- a redirection or a pipeline added to the matching prefix asks again ----
    for (name, argv) in [("redirect", vec!["sh", "-c", "mytool build > out2.txt"]), ("pipe", vec!["sh", "-c", "mytool build | cat"])] {
        let t = fx.task(&command_goal(&argv)).await;
        tasks.push(t.clone());
        fx.run_mode(&t, "ALLOWLIST", true).await.unwrap();
        let asked = fx.run_until_asked_or_done(&t, 6).await;
        leg.check(&format!("{name}_asks_again"), asked.len() == 1 && scope_json(&asked[0])["ask_reason"] == "NOT_IN_ALLOWLIST", format!("{:?}", asked.iter().map(scope_json).collect::<Vec<_>>()));
        fx.decide(&asked[0], false).await;
        fx.deny_until_done(&t).await;
    }
    leg.check("denied_commands_never_ran", !Path::new(&fx.root).join("out2.txt").exists() && !out.exists(), "a denied command ran");
    // ---- the always-ask classes ask even in Run everything ----
    for (name, argv, class) in [
        ("outside_workspace", vec!["touch", "../escaped-by-live-leg.txt"], "OUTSIDE_WORKSPACE_WRITE"),
        ("network", vec!["curl", "http://127.0.0.1:9/"], "NETWORK"),
    ] {
        let t = fx.task(&command_goal(&argv)).await;
        tasks.push(t.clone());
        fx.run_mode(&t, "RUN_EVERYTHING", true).await.unwrap();
        let asked = fx.run_until_asked_or_done(&t, 6).await;
        leg.check(
            &format!("{name}_asks_under_run_everything"),
            asked.len() == 1 && scope_json(&asked[0])["run_mode"] == "RUN_EVERYTHING" && scope_json(&asked[0])["ask_reason"] == format!("ALWAYS_ASK:{class}"),
            format!("{:?}", asked.iter().map(scope_json).collect::<Vec<_>>()),
        );
        fx.decide(&asked[0], false).await;
        fx.deny_until_done(&t).await;
    }
    leg.check("outside_write_never_happened", !Path::new(&fx.root).join("../escaped-by-live-leg.txt").exists(), "the outside-workspace write happened");
    // ---- a revoked rule asks again ----
    let t_other = fx.task(&command_goal(&["mytool", "build"])).await;
    tasks.push(t_other.clone());
    let _: AllowRuleRevoked = fx
        .ok("RevokeAllowRule", RevokeAllowRule { task_id: Some(t_other.clone()), rule_id: rule.rule_id.clone(), reason: "no longer wanted".into() }, true)
        .await;
    fx.run_mode(&t_other, "ALLOWLIST", true).await.unwrap();
    let asked = fx.run_until_asked_or_done(&t_other, 6).await;
    leg.check("revoked_rule_no_longer_covers", asked.len() == 1 && scope_json(&asked[0])["ask_reason"] == "NOT_IN_ALLOWLIST", format!("{:?}", asked.iter().map(scope_json).collect::<Vec<_>>()));
    fx.decide(&asked[0], false).await;
    fx.deny_until_done(&t_other).await;
    // ---- Run everything dies with the process; a durable mode does not ----
    let keep = fx.task("keep").await;
    fx.run_mode(&keep, "ALLOWLIST_SANDBOX", true).await.unwrap();
    let t_run = fx.task("run everything").await;
    fx.run_mode(&t_run, "RUN_EVERYTHING", true).await.unwrap();
    fx.restart().await;
    let m_keep: RunModeView = fx.ok("GetRunMode", GetRunMode { task_id: Some(keep.clone()) }, false).await;
    let m_run: RunModeView = fx.ok("GetRunMode", GetRunMode { task_id: Some(t_run.clone()) }, false).await;
    leg.check("durable_mode_survives_the_restart", m_keep.mode == "ALLOWLIST_SANDBOX", m_keep.mode.clone());
    leg.check("run_everything_is_gone_after_the_restart", m_run.mode != "RUN_EVERYTHING", m_run.mode.clone());
    for t in &tasks {
        fx.charge(&mut leg, t).await;
    }
    leg.finish();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "live: needs MODBIT_LIVE=1 and the gateway credential (the live-core workflow)"]
async fn live_px057_allowlist() {
    leg_px057_allowlist(LiveEnv::from_process()).await;
}

// ---------------------------------------------------------------------------
// the scripted stand-in for the offline runs of the same legs
// ---------------------------------------------------------------------------

const SCRIPTED_KEY: &str = "sk-live-0123456789abcdef0123456789";

/// An environment pointing the legs at a scripted server. Its records say
/// `live: false`; the checker refuses them as live evidence.
fn scripted_env(base: &str, results: &Path) -> Arc<LiveEnv> {
    scripted_env_with(
        base,
        results,
        "glm-5.3-flash=0.15/0.50;ctx=200000;out=8192",
    )
}

fn scripted_env_with(base: &str, results: &Path, catalog: &str) -> Arc<LiveEnv> {
    let mut m: std::collections::BTreeMap<&str, String> = [
        ("MODBIT_LIVE", "1"),
        ("MODBIT_LIVE_API_KEY", SCRIPTED_KEY),
        ("MODBIT_LIVE_MODEL", "glm-5.3-flash"),
        ("MODBIT_LIVE_BASE_URL", base),
        ("MODBIT_LIVE_ALLOW_LOOPBACK", "1"),
        ("MODBIT_OPENAI_MODELS", catalog),
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

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn scripted_px101_pause_leg_mechanics() {
    let chunks = slow_chunks(40);
    let (base, _seen) = scripted_model_fn(Arc::new(move |_, results| {
        let steps = [
            json!({"calls": [{"name": "plan.update", "args": {"outcome": "four files", "expected_files": ["f1.txt", "f2.txt"]}}], "delay_ms": 200}),
            json!({"chunks": chunks, "chunk_gap_ms": 100, "calls": [{"name": "change.apply", "args": {"path": "f1.txt", "op": "create", "content": "f1.txt\n"}}]}),
            json!({"calls": [{"name": "change.apply", "args": {"path": "f2.txt", "op": "create", "content": "f2.txt\n"}}], "text": "writing f2"}),
            json!({"calls": [{"name": "task.complete", "args": {"summary": "done", "self_review": {"findings": []}}}], "text": "finishing"}),
        ];
        steps.get(results).cloned().unwrap_or_else(|| json!({"text": "done"}))
    }))
    .await;
    let results = tempfile::tempdir().unwrap();
    leg_px101_pause(scripted_env(&base, results.path())).await;
    let rec: Value = serde_json::from_str(&std::fs::read_to_string(results.path().join("px101-pause.json")).unwrap()).unwrap();
    assert_eq!(rec["passed"], true, "{rec:#}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn scripted_px051_modes_leg_mechanics() {
    // The scripted model asks for a write in every task; the mode decides.
    let (base, _seen) = scripted_model_fn(Arc::new(|body, results| {
        let text = body["messages"].to_string();
        if text.contains("hello.txt") {
            // ASK: the model insists on writing.
            let steps = [
                json!({"calls": [{"name": "change.apply", "args": {"path": "hello.txt", "op": "replace", "content": "hi\n"}}]}),
                json!({"text": "I could not write it."}),
            ];
            steps.get(results).cloned().unwrap_or_else(|| json!({"text": "done"}))
        } else if text.contains("Plan how to add") {
            // PLAN, then the accepted follow-up.
            if text.contains("The plan is accepted") {
                let steps = [
                    json!({"calls": [{"name": "plan.update", "args": {"outcome": "b mirrors a", "expected_files": ["b.txt"]}}]}),
                    json!({"calls": [{"name": "change.apply", "args": {"path": "b.txt", "op": "replace", "content": "alpha\n"}}]}),
                    json!({"calls": [{"name": "task.complete", "args": {"summary": "done", "self_review": {"findings": []}}}]}),
                ];
                // The first request after the follow-up carries the earlier results too.
                let done = text.matches("\"role\":\"tool\"").count();
                steps.get(done.saturating_sub(2)).cloned().unwrap_or_else(|| json!({"text": "done"}))
            } else {
                let steps = [
                    json!({"calls": [{"name": "plan.update", "args": {"outcome": "b mirrors a", "expected_files": ["b.txt"]}}]}),
                    json!({"text": "Plan: b.txt mirrors a.txt."}),
                ];
                steps.get(results).cloned().unwrap_or_else(|| json!({"text": "done"}))
            }
        } else {
            // DEBUG: fix at once; the mode asks for a reproduction first.
            let steps = [
                json!({"calls": [{"name": "plan.update", "args": {"outcome": "fix calc", "expected_files": ["calc.py"]}}]}),
                json!({"calls": [{"name": "change.apply", "args": {"path": "calc.py", "op": "replace", "content": "def double(x):\n    return x + x\n"}}]}),
                json!({"calls": [{"name": "change.apply", "args": {"path": "calc.py", "op": "replace", "content": "def double(x):\n    return x * 2\n"}}]}),
                json!({"calls": [{"name": "task.complete", "args": {"summary": "fixed", "self_review": {"findings": []}}}]}),
            ];
            steps.get(results).cloned().unwrap_or_else(|| json!({"text": "done"}))
        }
    }))
    .await;
    let results = tempfile::tempdir().unwrap();
    leg_px051_modes(scripted_env(&base, results.path())).await;
    let rec: Value = serde_json::from_str(&std::fs::read_to_string(results.path().join("px051-modes.json")).unwrap()).unwrap();
    assert_eq!(rec["passed"], true, "{rec:#}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn scripted_px118_worktree_leg_mechanics() {
    let (base, _seen) = scripted_model_fn(Arc::new(|body, results| {
        let text = body["messages"].to_string();
        if text.contains("test_calc.py fails") || text.contains("The unit test in test_calc.py fails") {
            let steps = [
                json!({"calls": [{"name": "plan.update", "args": {"outcome": "fix calc", "expected_files": ["calc.py"]}}]}),
                json!({"calls": [{"name": "change.apply", "args": {"path": "calc.py", "op": "replace", "content": "def double(x):\n    return x * 2\n"}}]}),
                json!({"calls": [{"name": "shell.exec", "args": {"argv": ["python3", "-m", "unittest", "-q"]}}]}),
                json!({"calls": [{"name": "task.complete", "args": {"summary": "fixed", "self_review": {"findings": []}}}]}),
            ];
            steps.get(results).cloned().unwrap_or_else(|| json!({"text": "done"}))
        } else {
            let steps = [
                json!({"calls": [{"name": "plan.update", "args": {"outcome": "four files", "expected_files": ["f1.txt", "f2.txt", "f3.txt", "f4.txt"]}}]}),
                json!({"calls": [{"name": "change.apply", "args": {"path": "f1.txt", "op": "create", "content": "f1.txt\n"}}], "delay_ms": 1500}),
                json!({"calls": [{"name": "change.apply", "args": {"path": "f2.txt", "op": "create", "content": "f2.txt\n"}}]}),
                json!({"calls": [{"name": "task.complete", "args": {"summary": "done", "self_review": {"findings": []}}}]}),
            ];
            steps.get(results).cloned().unwrap_or_else(|| json!({"text": "done"}))
        }
    }))
    .await;
    let results = tempfile::tempdir().unwrap();
    leg_px118_worktree(scripted_env(&base, results.path())).await;
    let rec: Value = serde_json::from_str(&std::fs::read_to_string(results.path().join("px118-worktree.json")).unwrap()).unwrap();
    assert_eq!(rec["passed"], true, "{rec:#}");
}

/// The reply of a scripted model for the compaction legs: it reads the parts
/// in order, answers the summarizer's request faithfully (or, for the ghost
/// summarizer, with something that is not a summary) and finishes with the
/// needle.
fn compaction_reply(parts: usize) -> Reply {
    // After an epoch the conversation holds fewer tool results, so the model
    // keeps its own count of the files it has read.
    let served = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    Arc::new(move |body, _results| {
        let sys = body["messages"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|m| m["role"] == "system")
            .filter_map(|m| m["content"].as_str())
            .collect::<Vec<_>>()
            .join("\n");
        if sys.contains("context summarizer") {
            if body["model"] == "ghost-summarizer-px109" {
                return json!({"text": "I am sorry, I cannot summarise this."});
            }
            let user = body["messages"]
                .as_array()
                .and_then(|m| m.last())
                .and_then(|m| m["content"].as_str())
                .unwrap_or("{}");
            let entries = serde_json::from_str::<Value>(user).unwrap_or_default()["transcript"]
                .as_array()
                .cloned()
                .unwrap_or_default();
            let users: Vec<Value> = entries.iter().filter(|e| e["role"] == "user").map(|e| e["text"].clone()).collect();
            let summary = json!({
                "primary_request": "read the parts",
                "user_messages": users,
                "plan": "read every part in order",
                "todos": [{"text": "finish reading", "status": "open"}],
                "decisions": [],
                "files": [{"path": "part-01.txt", "note": "read"}],
                "failures": [],
                "progress": "the first parts were read",
                "next_steps": ["read the rest"]
            });
            return json!({"text": summary.to_string()});
        }
        let read = served.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        if read == 0 {
            json!({"calls": [{"name": "plan.update", "args": {"outcome": "read the files", "expected_files": []}}]})
        } else if read <= parts {
            let read = read - 1;
            json!({"calls": [{"name": "fs.read", "args": {"path": format!("part-{:02}.txt", read + 1), "max_bytes": 60000}}]})
        } else {
            json!({"calls": [{"name": "task.complete", "args": {"summary": format!("The code word is {NEEDLE}. The last file is a list of lines."), "self_review": {"findings": []}}}]})
        }
    })
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn scripted_px109_compaction_leg_mechanics() {
    let (base, _seen) = scripted_model_fn(compaction_reply(30)).await;
    let results = tempfile::tempdir().unwrap();
    leg_px109_compaction(scripted_env_with(&base, results.path(), "glm-5.3-flash=0.15/0.50;ctx=64000;out=8192")).await;
    let rec: Value = serde_json::from_str(&std::fs::read_to_string(results.path().join("px109-compaction.json")).unwrap()).unwrap();
    assert_eq!(rec["passed"], true, "{rec:#}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn scripted_px109_fallback_leg_mechanics() {
    let (base, _seen) = scripted_model_fn(compaction_reply(30)).await;
    let results = tempfile::tempdir().unwrap();
    leg_px109_fallback(scripted_env_with(&base, results.path(), "glm-5.3-flash=0.15/0.50;ctx=64000;out=8192")).await;
    let rec: Value = serde_json::from_str(&std::fs::read_to_string(results.path().join("px109-fallback.json")).unwrap()).unwrap();
    assert_eq!(rec["passed"], true, "{rec:#}");
}

/// The scripted model of the budget leg: the parents emit the spawn calls the
/// live goals ask for, the children read and finish.
fn px116_reply() -> Reply {
    let spawn = |key: &str, objective: &str, extra: Value| -> Value {
        let mut args = json!({"idempotency_key": key, "objective": objective, "write_scope": [format!("scope/{key}/")]});
        if let (Some(a), Some(e)) = (args.as_object_mut(), extra.as_object()) {
            for (k, v) in e {
                a.insert(k.clone(), v.clone());
            }
        }
        json!({"calls": [{"name": "agent.spawn", "args": args}]})
    };
    let wait = |key: &str| json!({"calls": [{"name": "agent.wait", "args": {"idempotency_key": key, "timeout_ms": 120_000}}]});
    let plan = json!({"calls": [{"name": "plan.update", "args": {"outcome": "delegate", "expected_files": ["README.md"]}}]});
    let done = json!({"calls": [{"name": "task.complete", "args": {"summary": "done", "self_review": {"findings": []}}}]});
    Arc::new(move |body, results| {
        let text = body["messages"].to_string();
        let step = |steps: Vec<Value>| steps.get(results).cloned().unwrap_or_else(|| json!({"text": "done"}));
        if text.contains("Task goal: CHILD-F") {
            return step(vec![
                json!({"calls": [{"name": "plan.update", "args": {"outcome": "read", "expected_files": []}}]}),
                json!({"calls": [{"name": "fs.read", "args": {"path": "src/secret/key.txt"}}]}),
                json!({"calls": [{"name": "fs.read", "args": {"path": "src/a/x.txt"}}]}),
                done.clone(),
            ]);
        }
        if text.contains("Task goal: CHILD-") {
            return step(vec![
                json!({"calls": [{"name": "plan.update", "args": {"outcome": "read", "expected_files": []}}]}),
                json!({"calls": [{"name": "fs.read", "args": {"path": "README.md"}}]}),
                done.clone(),
            ]);
        }
        if text.contains("three read-only children") {
            return step(vec![
                plan.clone(),
                spawn("k1", "CHILD-A read README.md", json!({"max_cost_minor": 120})),
                spawn("k2", "CHILD-B read README.md", json!({"max_cost_minor": 6})),
                spawn("k3", "CHILD-C read README.md", json!({"max_cost_minor": 2_000})),
                wait("k1"),
                wait("k2"),
                wait("k3"),
                done.clone(),
            ]);
        }
        if text.contains("two read-only children") {
            return step(vec![
                plan.clone(),
                spawn("k1", "CHILD-D read README.md", json!({})),
                spawn("k2", "CHILD-E read README.md", json!({})),
                wait("k1"),
                done.clone(),
            ]);
        }
        if text.contains("one scoped child") {
            return step(vec![
                plan.clone(),
                spawn("k1", "CHILD-F read the secret", json!({"read_scope": ["src/a/"]})),
                wait("k1"),
                done.clone(),
            ]);
        }
        if text.contains("one read-only child") {
            return step(vec![
                plan.clone(),
                spawn("k1", "CHILD-G read README.md", json!({"max_cost_minor": 120})),
                wait("k1"),
                done.clone(),
            ]);
        }
        step(vec![
            plan.clone(),
            json!({"calls": [{"name": "fs.read", "args": {"path": "README.md"}}]}),
            done.clone(),
        ])
    })
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn scripted_px116_budgets_leg_mechanics() {
    let (base, _seen) = scripted_model_fn(px116_reply()).await;
    let results = tempfile::tempdir().unwrap();
    leg_px116_budgets(scripted_env(&base, results.path())).await;
    let rec: Value = serde_json::from_str(&std::fs::read_to_string(results.path().join("px116-budgets.json")).unwrap()).unwrap();
    assert_eq!(rec["passed"], true, "{rec:#}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn scripted_px057_allowlist_leg_mechanics() {
    // A model that runs the command its task's goal names, once.
    let (base, _seen) = scripted_model_fn(Arc::new(|body, results| {
        if results > 0 {
            return json!({"calls": [{"name": "task.complete", "args": {"summary": "done", "self_review": {"findings": []}}}]});
        }
        let text = body["messages"].to_string();
        let start = text.find("arguments ").map_or(0, |i| i + "arguments ".len());
        let rest = &text[start..];
        // The goal embeds the JSON arguments, escaped inside the request body.
        let unescaped = rest.replace("\\\"", "\"");
        let end = unescaped.find("}, then").map_or(unescaped.len(), |i| i + 1);
        let args: Value = serde_json::from_str(&unescaped[..end]).unwrap_or_else(|_| json!({"argv": ["true"]}));
        json!({"calls": [{"name": "shell.exec", "args": args}]})
    }))
    .await;
    let results = tempfile::tempdir().unwrap();
    leg_px057_allowlist(scripted_env(&base, results.path())).await;
    let rec: Value = serde_json::from_str(&std::fs::read_to_string(results.path().join("px057-allowlist.json")).unwrap()).unwrap();
    assert_eq!(rec["passed"], true, "{rec:#}");
}
