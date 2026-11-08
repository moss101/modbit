//! PX-158 (QUAL-PX-158, PX-E2E-158): image tokens are charged by size in the
//! compaction estimate, on the real Core.
//!
//! Real: the `modbit-core` binary, its store, the gateway's HTTP/SSE client,
//! the Media Pipeline (`fs.read` of real PNG files: decoded, metadata
//! stripped, sent as `image_url` data URLs), the prompt compiler, the
//! calibrated compaction trigger and the compaction ladder. Stand-in: the
//! scripted OpenAI-compatible model's replies, and the provider's token
//! count, which the scripted server computes from the body it actually
//! received by a stated rule (see [`provider_count`]) and reports as
//! `usage.prompt_tokens`, as the API does.
//!
//! The provider rule is deliberately not the Core's estimator: text is the
//! provider's published "about four characters a token" with the cookbook's
//! three tokens a message, and an image is OpenAI's published high-detail
//! tile rule (fit in 2048x2048, shortest side to 768, 85 + 170 per 512 px
//! tile), where the Core charges about one token per 750 px. The Core's
//! provider calibration has to close the gap, as it must against a real
//! endpoint.
//!
//! The margin is the one the Core reserves for what its estimate misses when
//! it derives the trigger from the window (`compaction_model::from_window`:
//! the window less the output budget and `window / 50`). No other margin is
//! named in the code or the dossier.

mod px_common;

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use modbit_protocol::client::Client;
use modbit_protocol::v1::{ReadObjectRange, StartTask, TaskRunStarted};
use prost::Message;
use px_common::*;
use serde_json::{Value, json};

/// The routed model: a vision model with a small window so a run of frames
/// reaches the trigger in a handful of turns.
const WINDOW: u64 = 24_000;
const MODELS: &str = "px158-vision=0.15/0.60;ctx=24000;out=4000;budget=1000;vision=true";
const MODEL: &str = "px158-vision";
/// What the Core reserves for estimator misses at this window.
const MARGIN: u64 = WINDOW / 50;
/// Share of the hard budget at which a worker starts (`SOFT_PERCENT` /
/// `HARD_PERCENT` in `compaction_model`).
const SOFT_OF_HARD: (u64, u64) = (70, 85);
/// The room the Core gives the whole prompt at this window: 85 percent of
/// the window less the answer's budget (`budget=1000` in `MODELS`) and the
/// estimator margin (`compaction_model::from_window`). The hard budget a
/// boundary reports is this less the Core's calibrated estimate of the
/// non-transcript part of the request.
const PROMPT_ROOM: u64 = (WINDOW - 1_000 - MARGIN) * SOFT_OF_HARD.1 / 100;

/// Frames read in a run: enough to cross the trigger, open an epoch and
/// keep reading under it.
const FRAMES: usize = 14;
/// Two common display sizes: one the Core's per-pixel charge and the tile
/// rule nearly agree on (1280x800: 1365 vs 1105 tokens), one where they do
/// not (1920x1080: 1600, the cap, vs 1105).
const DISPLAYS: [(u32, u32); 2] = [(1280, 800), (1920, 1080)];

/// A real `w`x`h` RGB PNG, one flat colour per frame so every frame is
/// distinct bytes.
fn frame_png(n: usize, w: u32, h: u32) -> Vec<u8> {
    let mut out = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut out, w, h);
        enc.set_color(png::ColorType::Rgb);
        enc.set_depth(png::BitDepth::Eight);
        let mut writer = enc.write_header().unwrap();
        let px = [
            (n * 17 % 256) as u8,
            (n * 53 % 256) as u8,
            (n * 91 % 256) as u8,
        ];
        let data: Vec<u8> = (0..w * h).flat_map(|_| px).collect();
        writer.write_image_data(&data).unwrap();
    }
    out
}

/// Prose for the text-only control: a note costs the provider about as much
/// as a frame does.
fn note_text(n: usize) -> String {
    (0..60)
        .map(|i| {
            format!("Note {n} line {i}: the deploy log shows a retry after the cache warmed.\n")
        })
        .collect()
}

// ---------------------------------------------------------------------------
// The provider's count.

fn text_tokens(s: &str) -> u64 {
    (s.len() as u64).div_ceil(4)
}

/// Width and height from a PNG data URL's IHDR.
fn data_url_size(url: &str) -> Option<(u64, u64)> {
    use base64_lite::decode_prefix;
    let b64 = url.split_once("base64,")?.1;
    let head = decode_prefix(b64, 24)?;
    if head.len() < 24 || &head[..8] != b"\x89PNG\r\n\x1a\n" {
        return None;
    }
    let be = |b: &[u8]| u64::from(u32::from_be_bytes([b[0], b[1], b[2], b[3]]));
    Some((be(&head[16..20]), be(&head[20..24])))
}

/// OpenAI's published high-detail image cost: scale to fit 2048x2048, then
/// the shortest side to 768, then 85 tokens plus 170 per 512 px tile. An
/// `image_url` with no `detail` is `auto`, which is high detail for an image
/// this size.
fn openai_image_tokens(w: u64, h: u64) -> u64 {
    let (mut w, mut h) = (w as f64, h as f64);
    if w.max(h) > 2048.0 {
        let s = 2048.0 / w.max(h);
        (w, h) = (w * s, h * s);
    }
    if w.min(h) > 768.0 {
        let s = 768.0 / w.min(h);
        (w, h) = (w * s, h * s);
    }
    let tiles = (w / 512.0).ceil() as u64 * (h / 512.0).ceil() as u64;
    85 + 170 * tiles
}

/// The provider's count of one wire message: three tokens of framing, its
/// text at four characters a token, each image by the tile rule.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Count {
    text: u64,
    images: u64,
    image_count: u64,
}

impl Count {
    fn total(&self) -> u64 {
        self.text + self.images
    }
    fn add(&mut self, o: Count) {
        self.text += o.text;
        self.images += o.images;
        self.image_count += o.image_count;
    }
}

fn message_count(m: &Value) -> Count {
    let mut c = Count {
        text: 3 + text_tokens(m["role"].as_str().unwrap_or_default()),
        ..Count::default()
    };
    match &m["content"] {
        Value::String(s) => c.text += text_tokens(s),
        Value::Array(blocks) => {
            for b in blocks {
                if b["type"] == "image_url" {
                    let (w, h) = data_url_size(b["image_url"]["url"].as_str().unwrap_or_default())
                        .expect("the scripted provider takes PNG images");
                    c.images += openai_image_tokens(w, h);
                    c.image_count += 1;
                } else {
                    c.text += text_tokens(b["text"].as_str().unwrap_or_default());
                }
            }
        }
        _ => {}
    }
    for call in m["tool_calls"].as_array().into_iter().flatten() {
        c.text += text_tokens(call["function"]["name"].as_str().unwrap_or_default());
        c.text += text_tokens(call["function"]["arguments"].as_str().unwrap_or_default());
    }
    if let Some(id) = m["tool_call_id"].as_str() {
        c.text += text_tokens(id);
    }
    c
}

/// The provider's count of a request body, split into the transcript (the
/// messages after the task turn and before the volatile run-state tail, as
/// the prompt compiler lays them out) and everything else.
#[derive(Clone, Copy, Debug, Default)]
struct RequestCount {
    transcript: Count,
    rest: Count,
}

impl RequestCount {
    /// `usage.prompt_tokens`: every message, the tool schemas, and three
    /// tokens priming the reply.
    fn prompt_tokens(&self) -> u64 {
        self.transcript.total() + self.rest.total()
    }
}

fn provider_count(body: &Value) -> RequestCount {
    let messages = body["messages"].as_array().cloned().unwrap_or_default();
    let task_turn = messages
        .iter()
        .position(|m| {
            m["role"] == "user"
                && m["content"]
                    .as_str()
                    .is_some_and(|c| c.starts_with("Task goal:"))
        })
        .expect("the task turn");
    let tail = messages.len() - 1;
    assert!(
        messages[tail]["content"]
            .as_str()
            .is_some_and(|c| c.starts_with("Current run state")),
        "the last message is the volatile tail"
    );
    let mut out = RequestCount::default();
    for (i, m) in messages.iter().enumerate() {
        let c = message_count(m);
        if i > task_turn && i < tail {
            out.transcript.add(c);
        } else {
            out.rest.add(c);
        }
    }
    out.rest.text += 3;
    if let Some(tools) = body.get("tools") {
        out.rest.text += text_tokens(&tools.to_string());
    }
    out
}

/// Minimal standard base64 decoding of a prefix (the test needs only the
/// PNG signature and IHDR).
mod base64_lite {
    pub fn decode_prefix(s: &str, bytes: usize) -> Option<Vec<u8>> {
        let val = |c: u8| -> Option<u32> {
            Some(match c {
                b'A'..=b'Z' => u32::from(c - b'A'),
                b'a'..=b'z' => u32::from(c - b'a') + 26,
                b'0'..=b'9' => u32::from(c - b'0') + 52,
                b'+' => 62,
                b'/' => 63,
                _ => return None,
            })
        };
        let mut out = Vec::new();
        for chunk in s.as_bytes().chunks(4) {
            if out.len() >= bytes || chunk.len() < 4 {
                break;
            }
            let mut n = 0u32;
            let mut pad = 0;
            for &c in chunk {
                n <<= 6;
                if c == b'=' {
                    pad += 1;
                } else {
                    n |= val(c)?;
                }
            }
            out.push((n >> 16) as u8);
            if pad < 2 {
                out.push((n >> 8) as u8);
            }
            if pad < 1 {
                out.push(n as u8);
            }
        }
        Some(out)
    }
}

// ---------------------------------------------------------------------------
// The run.

fn has_epoch(body: &Value) -> bool {
    body["messages"].as_array().is_some_and(|m| {
        m.iter().any(|x| {
            x["role"] == "system"
                && x["content"]
                    .as_str()
                    .unwrap_or_default()
                    .split("Compaction epoch ")
                    .skip(1)
                    .any(|rest| rest.starts_with(|c: char| c.is_ascii_digit()))
        })
    })
}

/// The script: a plan, then one `fs.read` per file, then done. Requests are
/// counted, not tool results, so a shortened transcript does not rewind it.
fn reply(files: Vec<String>) -> Reply {
    let n = Arc::new(AtomicUsize::new(0));
    Arc::new(move |_body, _results| {
        let i = n.fetch_add(1, Ordering::SeqCst);
        if i == 0 {
            return json!({"calls": [{"name": "plan.update", "args": {"outcome": "look at every file in turn", "expected_files": []}}]});
        }
        match files.get(i - 1) {
            Some(f) => json!({"calls": [{"name": "fs.read", "args": {"path": f}}]}),
            None => {
                json!({"calls": [{"name": "task.complete", "args": {"summary": "looked at every file", "self_review": {"findings": []}}}]})
            }
        }
    })
}

struct Run {
    events: Vec<Value>,
    bodies: Vec<Value>,
    status: modbit_protocol::v1::TaskStatus,
    client: Client,
    _core: CoreProcess,
    _dir: tempfile::TempDir,
    _repo: tempfile::TempDir,
}

async fn run(files: Vec<(String, Vec<u8>)>, goal: &str) -> Run {
    let names: Vec<String> = files.iter().map(|(n, _)| n.clone()).collect();
    let (repo, root) = plain_repo(&[("README.md", "frames\n")]);
    for (n, bytes) in &files {
        std::fs::write(std::path::Path::new(&root).join(n), bytes).unwrap();
    }
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
            "files",
        ],
    ] {
        assert!(
            std::process::Command::new("git")
                .arg("-C")
                .arg(&root)
                .args(&args)
                .status()
                .unwrap()
                .success()
        );
    }
    let meter: Meter = Arc::new(|body: &Value| provider_count(body).prompt_tokens());
    let (base, seen) = scripted_model_metered(reply(names), Some(meter)).await;
    let dir = tempfile::tempdir().unwrap();
    let mut env = model_env(&base);
    env.push(("MODBIT_OPENAI_MODELS".into(), MODELS.into()));
    let refs: Vec<(&str, &str)> = env.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
    let core = CoreProcess::spawn_with_env(dir.path(), &refs);
    let mut c = core.client().await;
    let (session, g) = session_with_lease(&mut c, 0x58).await;
    let task = create_task(&mut c, &session, g, &root, 0x59, "local_trusted", goal).await;
    let ack = c
        .command(envelope_fenced(
            id16(0x5A),
            "StartTask",
            StartTask {
                task_id: Some(task.clone()),
                endpoint: String::new(),
                model: MODEL.into(),
                max_turns: 40,
                max_tool_calls: 0,
                max_no_progress_turns: 40,
                skills: vec![],
                ..Default::default()
            }
            .encode_to_vec(),
            g,
        ))
        .await
        .unwrap();
    let _: TaskRunStarted = Client::result(&ack).unwrap();
    let status = wait_task(&mut c, &task, 240).await;
    let events = replay(&core, &session).await;
    let bodies = seen.lock().unwrap().clone();
    Run {
        events,
        bodies,
        status,
        client: c,
        _core: core,
        _dir: dir,
        _repo: repo,
    }
}

fn of_type<'a>(ev: &'a [Value], t: &str) -> Vec<&'a Value> {
    ev.iter().filter(|e| e["event_type"] == t).collect()
}

fn payload(e: &Value) -> &Value {
    &e["payload"]["payload"]
}

async fn read_object(c: &mut Client, hash: &str) -> Value {
    let ack = c
        .command(envelope(
            rand_id(),
            "ReadObjectRange",
            ReadObjectRange {
                object_hash: hash.to_owned(),
                offset: 0,
                length: 1024 * 1024,
            }
            .encode_to_vec(),
        ))
        .await
        .unwrap();
    let chunk: modbit_protocol::v1::ObjectRangeChunk = Client::result(&ack).unwrap();
    serde_json::from_slice(&chunk.data).unwrap_or_default()
}

/// The Core's own estimate of each request, as its compile step recorded it
/// (`request_estimate`, PX-059), in request order.
async fn request_estimates(r: &mut Run) -> Vec<u64> {
    let mut out = Vec::new();
    for e in r
        .events
        .iter()
        .filter(|e| e["event_type"] == "StepSucceeded")
    {
        let Some(hash) = payload(e)["output_ref"].as_str() else {
            continue;
        };
        let record = read_object(&mut r.client, hash).await;
        if let Some(n) = record["accounting"]["request_estimate"].as_u64() {
            out.push(n);
        }
    }
    out
}

/// The provider-reported input tokens of each solver request, in order.
fn reported(ev: &[Value]) -> Vec<u64> {
    of_type(ev, "ModelUsageRecorded")
        .iter()
        .filter(|e| payload(e)["route"]["role"] != "compaction-summarizer")
        .map(|e| payload(e)["input_tokens"].as_u64().unwrap())
        .collect()
}

/// Where the Core's trigger fired and what the provider counted around it.
struct Trigger {
    /// Index of the first request compiled after the trigger fired. A soft
    /// trigger compacts off the loop, so this request still carries the
    /// transcript the Core measured.
    at: usize,
    /// The hard budget the Core derived at that boundary, and the soft
    /// threshold the worker starts at.
    hard: u64,
    soft: u64,
    /// The Core's calibrated estimate of the rest of the request (system
    /// prompt, tool schemas) at that boundary: `PROMPT_ROOM - hard`.
    overhead: u64,
    /// The provider's count of the transcript of each request.
    transcript: Vec<Count>,
}

fn first_trigger(r: &Run) -> Trigger {
    let started = of_type(&r.events, "CompactionStarted");
    let first = started.first().unwrap_or_else(|| {
        panic!(
            "no compaction started; provider transcript counts: {:?}",
            r.bodies
                .iter()
                .map(|b| provider_count(b).transcript.total())
                .collect::<Vec<_>>()
        )
    });
    let p = payload(first);
    assert_eq!(p["budget_source"], "MODEL_WINDOW", "{p}");
    assert_eq!(p["window_tokens"].as_u64(), Some(WINDOW), "{p}");
    assert_eq!(p["mode"], "ASYNC", "the soft trigger fires first: {p}");
    let offset = first["offset"].as_u64().unwrap();
    let at = of_type(&r.events, "ModelUsageRecorded")
        .iter()
        .filter(|e| payload(e)["route"]["role"] != "compaction-summarizer")
        .filter(|e| e["offset"].as_u64().unwrap() < offset)
        .count();
    let hard = p["budget_tokens"].as_u64().unwrap();
    assert!(
        hard < PROMPT_ROOM && hard > 2_000,
        "the hard budget is the prompt room less the overhead, above the floor: {p}"
    );
    Trigger {
        at,
        hard,
        soft: hard * SOFT_OF_HARD.0 / SOFT_OF_HARD.1,
        overhead: PROMPT_ROOM - hard,
        transcript: r
            .bodies
            .iter()
            .map(|b| provider_count(b).transcript)
            .collect(),
    }
}

/// QUAL-PX-158 "within the calibrated margin", both ways: every request the
/// Core estimated after the provider first reported is within the margin of
/// what the provider then reported (the estimate as the Core's calibration
/// corrects it, by the Core's own `Calibration`), and the trigger fired where
/// the provider-reported usage reached the threshold: at a request whose
/// reported input is over the soft threshold plus the Core's overhead, or
/// short of it by no more than the margin, and not one request later than
/// the reported input crossed it beyond the margin.
///
/// The comparison is on the whole request because that is what a provider
/// reports and what the Core's calibration corrects: one factor for the
/// whole request. How a request splits between transcript and the rest is
/// not reported, so it cannot be calibrated, and judging the trigger on the
/// mock's own transcript count would test the estimator's split rather than
/// the agreement with reported usage (a larger tool schema once moved that
/// split by more than the margin without the Core's trigger moving).
async fn assert_within_margin(r: &mut Run, label: &str) -> Trigger {
    let estimates = request_estimates(r).await;
    let reported = reported(&r.events);
    assert_eq!(
        estimates.len(),
        reported.len(),
        "{label}: one compile record and one usage report per request"
    );
    assert_eq!(reported.len(), r.bodies.len(), "{label}");
    for (k, body) in r.bodies.iter().enumerate() {
        assert_eq!(
            reported[k],
            provider_count(body).prompt_tokens(),
            "{label}: the usage on the log is what the provider reported for request {k}"
        );
    }
    let mut calibration = modbit_compaction::Calibration::default();
    for (k, (e, u)) in estimates.iter().zip(&reported).enumerate() {
        let corrected = u64::from(calibration.apply(u32::try_from(*e).unwrap()));
        if k > 0 {
            assert!(
                corrected.abs_diff(*u) <= MARGIN,
                "{label}: request {k}: the Core estimated {e} ({corrected} calibrated), the provider reported {u}; the margin is {MARGIN}"
            );
        }
        calibration.observe(u32::try_from(*e).unwrap(), *u);
    }
    let t = first_trigger(r);
    let at = t.at;
    assert!(at >= 1 && at < r.bodies.len(), "{label}: trigger at {at}");
    assert!(
        !has_epoch(&r.bodies[at]),
        "{label}: the request after a soft trigger still carries the measured transcript"
    );
    // The soft threshold in the provider's terms: the transcript threshold
    // plus the rest of the request, as the Core estimated it.
    let threshold = t.soft + t.overhead;
    let before = reported[at - 1];
    let measured = reported[at];
    let crossed = reported.iter().position(|u| *u > threshold);
    eprintln!(
        "PX-158 {label}: trigger before request {at}; reported input crossed the soft threshold {threshold} (transcript {} + overhead {}) at request {crossed:?} (None: the epoch installed first); reported input {before} -> {measured}; provider transcript {} -> {}; hard {}; margin {MARGIN}; CompactionStarted.tokens_before {}",
        t.soft,
        t.overhead,
        t.transcript[at - 1].total(),
        t.transcript[at].total(),
        t.hard,
        payload(of_type(&r.events, "CompactionStarted")[0])["tokens_before"]
    );
    assert!(
        measured + MARGIN > threshold,
        "{label}: the Core compacted at a reported input of {measured}, under the threshold {threshold} by more than the margin {MARGIN}"
    );
    assert!(
        before <= threshold + MARGIN,
        "{label}: the reported input crossed the threshold {threshold} by more than the margin a request earlier ({before}) and the Core did not compact"
    );
    t
}

/// A shortened transcript never holds an image without its caption: in
/// every request, each attachment message follows the tool result it
/// belongs to, which names the frame and announces the attachment, and
/// that result follows the call that asked for it. Returns how many images
/// requests under an epoch carried.
fn assert_no_orphan_image(r: &Run, label: &str) -> usize {
    let mut under_epoch = 0;
    for (k, body) in r.bodies.iter().enumerate() {
        let messages = body["messages"].as_array().unwrap();
        let epoch = has_epoch(body);
        let mut announced = 0;
        for (i, m) in messages.iter().enumerate() {
            if m["role"] == "tool"
                && m["content"]
                    .as_str()
                    .is_some_and(|c| c.contains("attachment(s) for this call follow"))
            {
                announced += 1;
            }
            let Some(blocks) = m["content"].as_array() else {
                continue;
            };
            let images = blocks.iter().filter(|b| b["type"] == "image_url").count();
            if images == 0 {
                continue;
            }
            let caption = &messages[i - 1];
            let call_id = caption["tool_call_id"].as_str().unwrap_or_else(|| {
                panic!("{label}: request {k}: image at {i} without its tool result: {caption:.300}")
            });
            assert!(
                blocks[0]["text"]
                    .as_str()
                    .is_some_and(|t| t.contains(call_id)),
                "{label}: request {k}: the attachment names its call"
            );
            let text = caption["content"].as_str().unwrap();
            assert!(
                text.contains("\"source\":\"frame-")
                    && text.contains("\"mime\":\"image/png\"")
                    && text.contains(&format!("{images} attachment(s) for this call follow")),
                "{label}: request {k}: the caption names the frame and its attachment: {text:.400}"
            );
            let asked = messages[..i - 1]
                .iter()
                .rev()
                .find(|m| m["role"] == "assistant")
                .unwrap_or_else(|| panic!("{label}: request {k}: a result with no call"));
            assert!(
                asked["tool_calls"]
                    .as_array()
                    .is_some_and(|c| c.iter().any(|c| c["id"] == call_id)),
                "{label}: request {k}: the caption answers a call the transcript still holds"
            );
            if epoch {
                under_epoch += images;
            }
        }
        // And no caption lost its image.
        assert_eq!(
            announced,
            provider_count(body).transcript.image_count as usize,
            "{label}: request {k}: every announced attachment is there"
        );
    }
    under_epoch
}

/// QUAL-PX-158, frames leg: a run that reads screen frames reaches the
/// compaction threshold at the request where the provider-reported usage
/// does, within the calibrated margin, at two display sizes; counted as
/// captions the same transcript is nowhere near the threshold; and the
/// shortened transcript never holds an image without its caption.
#[tokio::test(flavor = "multi_thread")]
async fn qual_px_158_frames_reach_the_compaction_threshold_where_reported_usage_does() {
    for (w, h) in DISPLAYS {
        let label = format!("{w}x{h}");
        let files: Vec<(String, Vec<u8>)> = (1..=FRAMES)
            .map(|n| (format!("frame-{n:02}.png"), frame_png(n, w, h)))
            .collect();
        let mut r = run(files, "look at each screen frame in turn").await;
        assert_eq!(r.status.state, "ReadyForReview", "{label}: {:?}", r.status);
        assert!(!r.status.loop_alive);
        let t = assert_within_margin(&mut r, &label).await;
        // The frames are what crossed it: the transcript the Core compacted
        // carried one image per frame read, at the tile cost of this size.
        let at = &t.transcript[t.at];
        assert_eq!(
            at.image_count as usize,
            t.at - 1,
            "{label}: one frame per read"
        );
        assert_eq!(
            at.images,
            at.image_count * openai_image_tokens(w.into(), h.into())
        );
        // Counted as captions (the frames at zero), the transcript the Core
        // compacted is under the threshold by more than the margin even at
        // the most the calibration may scale an estimate (3x,
        // `Calibration::MAX`): a frame counted as a label never gets there.
        assert!(
            at.text * 3 + MARGIN < t.soft,
            "{label}: captions alone ({} provider tokens) would have reached {}",
            at.text,
            t.soft
        );
        // The worker's epoch installed, the run kept reading frames under
        // it, and no shortened request holds an image without its caption.
        assert!(
            !of_type(&r.events, "ContextEpochOpened").is_empty(),
            "{label}: the epoch installed"
        );
        let under_epoch = assert_no_orphan_image(&r, &label);
        assert!(
            under_epoch >= 3,
            "{label}: requests under an epoch carried frames ({under_epoch})"
        );
    }
}

/// QUAL-PX-158, text-only control: a transcript with no image reaches the
/// threshold where the provider-reported usage does, within the same
/// margin, and no request carries an image. That the estimate is the one it
/// was before PX-158 is the differential run recorded in
/// evidence/m10/PX-158/mutation-check.txt: this test, against the Core
/// with `message_tokens` reverted to the text estimate alone, triggers at
/// the same request with the same `tokens_before`.
#[tokio::test(flavor = "multi_thread")]
async fn qual_px_158_a_text_only_transcript_is_estimated_as_before() {
    let files: Vec<(String, Vec<u8>)> = (1..=FRAMES)
        .map(|n| (format!("note-{n:02}.txt"), note_text(n).into_bytes()))
        .collect();
    let mut r = run(files, "read each note in turn").await;
    assert_eq!(r.status.state, "ReadyForReview", "{:?}", r.status);
    for (k, body) in r.bodies.iter().enumerate() {
        assert!(
            !body.to_string().contains("image_url"),
            "request {k} carries no image"
        );
    }
    let t = assert_within_margin(&mut r, "text-only").await;
    assert_eq!(t.transcript[t.at].image_count, 0);
    assert!(!of_type(&r.events, "ContextEpochOpened").is_empty());
}
