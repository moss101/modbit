//! The live half of PX-138 (QUAL-PX-138): the compaction arms of
//! [`crate::compaction_eval`] read by a real model through the compatible
//! gateway, scored against the generator's record of the event log.
//!
//! What is measured, per long run and per arm (uncompacted, extractive,
//! structured, lossy double): the same model, holding ONLY the arm's retained
//! context (built by the product's own compaction code), (1) answers the
//! run's held-out recall questions, and (2) writes a continuation plan whose
//! success is decided by facts the generator recorded beside the log. Neither
//! oracle is read from a summary.
//!
//! Method choices, stated in the report as well:
//! - one recall request per (run, arm) carries all of that run's questions
//!   (cost), and each question gets its own scored answer line;
//! - the model is called through the gateway directly, not through the Core
//!   process, with a fixed system prompt and temperature 0;
//! - the continuation task is a plan scored by log-derived criteria, not a
//!   repository-level code task;
//! - a call that fails after its retries is a missing measurement, never a
//!   zero; a reply the model gave but that does not follow the answer format
//!   is a miss and is counted (`unparsed`);
//! - every request and reply is retained under `exchanges/`, and the report
//!   digest is recomputed from those files by [`rescore_dir`].

use std::path::Path;
use std::sync::Arc;

use modbit_compaction::estimate_tokens_v2;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::chat::{Chat, ChatError};
use crate::compaction_eval::{Arm, ContinuationTask, LongRun, POST_HOC_NOTE, Probe, context_for};
use crate::projection_trial::{MeanCi, Proportion, mean_ci, wilson};
use crate::spend::{LiveStatus, Price, SpendMeter};

/// Long runs a complete measurement needs.
pub const MIN_RUNS: usize = 20;
/// Output budget of one request (room for a reasoning model's preamble).
pub const MAX_TOKENS: u32 = 4096;
/// The row name in `status.json`.
pub const ROW: &str = "px138";

/// What a request asks.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    /// All of a run's recall questions.
    Recall,
    /// The continuation plan.
    Task,
}

impl Kind {
    const ALL: [Kind; 2] = [Kind::Recall, Kind::Task];

    const fn label(self) -> &'static str {
        match self {
            Self::Recall => "recall",
            Self::Task => "task",
        }
    }
}

/// The oracle of one run, as retained in `oracle.json`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OracleRun {
    /// Run id.
    pub id: String,
    /// Held-out probes, from the generator.
    pub probes: Vec<Probe>,
    /// Continuation criteria, from the generator.
    pub task: ContinuationTask,
}

/// What the run was configured with, retained in `oracle.json`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Meta {
    /// The model every request named.
    pub model: String,
    /// The gateway base URL (never a credential).
    pub base_url: String,
    /// The list price used for cost.
    pub price: Price,
    /// Long runs planned.
    pub planned_runs: usize,
}

/// `oracle.json`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Oracle {
    /// Configuration.
    pub meta: Meta,
    /// One entry per planned run.
    pub runs: Vec<OracleRun>,
}

/// One request and its reply, retained verbatim.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Exchange {
    /// Run id.
    pub run: String,
    /// Arm.
    pub arm: Arm,
    /// Question kind.
    pub kind: Kind,
    /// sha256 of the serialized messages.
    pub request_sha256: String,
    /// The messages sent.
    pub messages: Vec<Value>,
    /// The assistant text received.
    pub reply_text: String,
    /// Input tokens the gateway reported.
    pub input_tokens: u64,
    /// Output tokens the gateway reported.
    pub output_tokens: u64,
    /// Cached part of the input.
    pub cached_input_tokens: u64,
    /// Whether the reply carried usage.
    pub usage_known: bool,
    /// Why the model stopped.
    pub finish_reason: String,
    /// The model id the gateway says answered.
    pub model_answered: String,
    /// The gateway's request id.
    pub request_id: String,
    /// Wall time, milliseconds.
    pub latency_ms: u64,
    /// Estimated tokens of the arm's retained context.
    pub context_tokens_est: u32,
    /// Why the arm could not produce a context (the extractive epoch stood).
    pub context_refused: Option<String>,
}

fn sha_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

fn file_name(run: &str, arm: Arm, kind: Kind) -> String {
    format!("{run}__{}__{}.json", arm.label(), kind.label())
}

const RECALL_SYSTEM: &str = "You are continuing an engineering session whose earlier history has been reduced to the context below. Answer ONLY from that context. If the context does not contain the answer, reply exactly `unknown` for that question; never guess. Quote identifiers, file paths, test names, numbers and reasons exactly as written in the context. Reply with one line per question, formatted `<number>. <answer>`, and nothing else.";

const TASK_SYSTEM: &str = "You are continuing an engineering session whose earlier history has been reduced to the context below. Use ONLY that context. Write the continuation plan as a single JSON object with exactly these keys: \"next_step\" (string: the next step the assistant said it would take), \"do_not_touch\" (string: the directory the user said must not be touched), \"user_constraint\" (string: the constraint the user added later in the session, stated exactly), \"files_to_edit\" (array of file paths you would edit next). If the context does not say, use \"unknown\" for a string and [] for the array; never guess. Reply with the JSON object only.";

/// The messages of one request.
#[must_use]
pub fn build_messages(kind: Kind, context: &str, probes: &[Probe]) -> Vec<Value> {
    match kind {
        Kind::Recall => {
            let questions: Vec<String> = probes
                .iter()
                .enumerate()
                .map(|(i, p)| format!("{}. {}", i + 1, p.question))
                .collect();
            vec![
                json!({"role": "system", "content": RECALL_SYSTEM}),
                json!({"role": "user", "content": format!("CONTEXT:\n{context}\n\nQUESTIONS:\n{}", questions.join("\n"))}),
            ]
        }
        Kind::Task => vec![
            json!({"role": "system", "content": TASK_SYSTEM}),
            json!({"role": "user", "content": format!("CONTEXT:\n{context}\n\nWrite the continuation plan now.")}),
        ],
    }
}

fn strip_think(text: &str) -> String {
    let mut out = text.to_owned();
    while let Some(a) = out.find("<think>") {
        match out[a..].find("</think>") {
            Some(b) => out.replace_range(a..a + b + "</think>".len(), ""),
            None => {
                out.truncate(a);
                break;
            }
        }
    }
    out
}

/// The answers of a recall reply by question number (`1..=n`): lines of the
/// form `<number>. <answer>` (`:` and `)` also accepted; a line without a
/// number continues the previous answer).
#[must_use]
pub fn parse_numbered(reply: &str, n: usize) -> Vec<Option<String>> {
    let mut out: Vec<Option<String>> = vec![None; n];
    let mut current: Option<usize> = None;
    for line in strip_think(reply).lines() {
        let t =
            line.trim_start_matches(|c: char| c.is_whitespace() || matches!(c, '*' | '-' | '#'));
        let digits: String = t.chars().take_while(char::is_ascii_digit).collect();
        let rest = &t[digits.len()..];
        let numbered = !digits.is_empty()
            && rest.starts_with(['.', ':', ')'])
            && digits.parse::<usize>().is_ok_and(|k| (1..=n).contains(&k));
        if numbered {
            let k = digits.parse::<usize>().unwrap_or(1) - 1;
            out[k] = Some(rest[1..].trim().to_owned());
            current = Some(k);
        } else if let Some(k) = current
            && let Some(a) = out[k].as_mut()
        {
            a.push('\n');
            a.push_str(line);
        }
    }
    out
}

/// One probe's outcome.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProbeScore {
    /// Probe id.
    pub id: String,
    /// Probe kind.
    pub kind: String,
    /// Every answer string appears in the model's answer.
    pub correct: bool,
    /// The model said it did not know (and was not correct).
    pub unknown: bool,
    /// The reply had no line for this question.
    pub unparsed: bool,
}

/// Score a recall reply exactly as the offline oracle does: all of
/// `probe.answer` must appear in the model's answer.
#[must_use]
pub fn score_recall(probes: &[Probe], reply: &str) -> Vec<ProbeScore> {
    let answers = parse_numbered(reply, probes.len());
    probes
        .iter()
        .zip(answers)
        .map(|(p, a)| {
            let correct = a
                .as_deref()
                .is_some_and(|a| p.answer.iter().all(|s| a.contains(s.as_str())));
            let unknown = !correct
                && a.as_deref().is_some_and(|a| {
                    let l = a.to_ascii_lowercase();
                    l.trim().starts_with("unknown")
                        || l.contains("do not know")
                        || l.contains("don't know")
                });
            ProbeScore {
                id: p.id.clone(),
                kind: p.kind.clone(),
                correct,
                unknown,
                unparsed: a.is_none(),
            }
        })
        .collect()
}

/// A continuation plan's outcome.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TaskScore {
    /// The reply contained a JSON object.
    pub parsed: bool,
    /// The next step names what the log said was next.
    pub next_step: bool,
    /// The forbidden directory is named and not proposed for editing.
    pub forbidden: bool,
    /// The later user constraint is stated.
    pub constraint: bool,
    /// All three.
    pub success: bool,
}

fn json_object(reply: &str) -> Option<Value> {
    let t = strip_think(reply);
    let (a, b) = (t.find('{')?, t.rfind('}')?);
    if b < a {
        return None;
    }
    serde_json::from_str::<Value>(&t[a..=b])
        .ok()
        .filter(Value::is_object)
}

fn squash(s: &str) -> String {
    s.chars()
        .filter(|c| !c.is_whitespace())
        .collect::<String>()
        .to_lowercase()
}

/// Score a continuation plan against the generator's criteria.
#[must_use]
pub fn score_task(task: &ContinuationTask, reply: &str) -> TaskScore {
    let Some(v) = json_object(reply) else {
        return TaskScore {
            parsed: false,
            next_step: false,
            forbidden: false,
            constraint: false,
            success: false,
        };
    };
    let text = |k: &str| match &v[k] {
        Value::String(s) => s.clone(),
        Value::Null => String::new(),
        other => other.to_string(),
    };
    let next = text("next_step").to_lowercase();
    let next_step = task
        .next_step_keys
        .iter()
        .all(|k| next.contains(&k.to_lowercase()));
    let edits = text("files_to_edit");
    let forbidden = text("do_not_touch").contains(&task.forbidden_dir)
        && !edits.contains(&task.forbidden_dir)
        && !text("next_step").contains(&task.forbidden_dir);
    let c = squash(&text("user_constraint"));
    let constraint = c.contains(&squash(&task.constraint_component))
        && c.contains(&format!("{}ms", task.constraint_ms));
    TaskScore {
        parsed: true,
        next_step,
        forbidden,
        constraint,
        success: next_step && forbidden && constraint,
    }
}

/// What `collect` leaves behind.
#[derive(Debug, Default)]
pub struct Collected {
    /// Exchanges that failed (after retries), `run/arm/kind: why`.
    pub failures: Vec<String>,
}

fn reserve_usd(price: Price, messages: &[Value]) -> f64 {
    let text: String = messages
        .iter()
        .filter_map(|m| m["content"].as_str())
        .collect::<Vec<_>>()
        .join("\n");
    price.cost_usd(u64::from(estimate_tokens_v2(&text)), u64::from(MAX_TOKENS))
}

enum Outcome {
    Done,
    Failed(String),
    Cap,
}

#[allow(clippy::too_many_arguments)]
async fn one_exchange(
    chat: &Chat,
    meter: &SpendMeter,
    dir: &Path,
    run_id: &str,
    arm: Arm,
    kind: Kind,
    messages: Vec<Value>,
    context_tokens_est: u32,
    context_refused: Option<String>,
) -> Outcome {
    let reserve = reserve_usd(chat.price, &messages);
    let mut reply = None;
    for attempt in 0..2 {
        match chat
            .complete(meter, reserve, &messages, &[], MAX_TOKENS)
            .await
        {
            Err(ChatError::Cap(_)) => return Outcome::Cap,
            Err(ChatError::Failed(e)) => return Outcome::Failed(e),
            Ok(r) if r.text.trim().is_empty() => {
                if attempt == 1 {
                    return Outcome::Failed(format!(
                        "the model returned an empty reply twice (finish_reason {})",
                        r.finish_reason
                    ));
                }
            }
            Ok(r) => {
                reply = Some(r);
                break;
            }
        }
    }
    let Some(r) = reply else {
        return Outcome::Failed("no reply".into());
    };
    let request_sha256 = sha_hex(
        serde_json::to_string(&messages)
            .unwrap_or_default()
            .as_bytes(),
    );
    let ex = Exchange {
        run: run_id.to_owned(),
        arm,
        kind,
        request_sha256,
        messages,
        reply_text: r.text,
        input_tokens: r.input_tokens,
        output_tokens: r.output_tokens,
        cached_input_tokens: r.cached_input_tokens,
        usage_known: r.usage_known,
        finish_reason: r.finish_reason,
        model_answered: r.model_answered,
        request_id: r.request_id,
        latency_ms: r.latency_ms,
        context_tokens_est,
        context_refused,
    };
    let path = dir.join(file_name(run_id, arm, kind));
    match serde_json::to_vec_pretty(&ex)
        .map_err(|e| e.to_string())
        .and_then(|b| std::fs::write(&path, b).map_err(|e| e.to_string()))
    {
        Ok(()) => Outcome::Done,
        Err(e) => Outcome::Failed(format!("writing {}: {e}", path.display())),
    }
}

/// Ask every question of every run, run by run with the four arms of a run
/// in flight together, so a stop leaves whole runs behind. Writes `oracle.json`
/// and `exchanges/`.
///
/// # Errors
/// The output directory cannot be prepared.
pub async fn collect(
    chat: &Chat,
    meter: &Arc<SpendMeter>,
    runs: &[LongRun],
    out_dir: &Path,
) -> Result<Collected, String> {
    let ex_dir = out_dir.join("exchanges");
    if ex_dir.exists() {
        std::fs::remove_dir_all(&ex_dir).map_err(|e| format!("clearing exchanges/: {e}"))?;
    }
    std::fs::create_dir_all(&ex_dir).map_err(|e| format!("creating exchanges/: {e}"))?;
    let oracle = Oracle {
        meta: Meta {
            model: chat.model.clone(),
            base_url: chat.base_url.clone(),
            price: chat.price,
            planned_runs: runs.len(),
        },
        runs: runs
            .iter()
            .map(|r| OracleRun {
                id: r.id.clone(),
                probes: r.probes.clone(),
                task: r.task.clone(),
            })
            .collect(),
    };
    std::fs::write(
        out_dir.join("oracle.json"),
        serde_json::to_vec_pretty(&oracle).map_err(|e| e.to_string())?,
    )
    .map_err(|e| format!("writing oracle.json: {e}"))?;
    let mut collected = Collected::default();
    for (ri, run) in runs.iter().enumerate() {
        let mut set = tokio::task::JoinSet::new();
        for arm in Arm::ALL {
            let (context, refused) = match context_for(arm, &run.entries) {
                Ok(c) => (c, None),
                Err(why) => (
                    context_for(Arm::Extractive, &run.entries).unwrap_or_default(),
                    Some(why),
                ),
            };
            let est = estimate_tokens_v2(&context);
            let (chat, meter, dir) = (chat.clone(), Arc::clone(meter), ex_dir.clone());
            let (id, probes) = (run.id.clone(), run.probes.clone());
            set.spawn(async move {
                let mut out = Vec::new();
                for kind in Kind::ALL {
                    let messages = build_messages(kind, &context, &probes);
                    let o = one_exchange(
                        &chat,
                        &meter,
                        &dir,
                        &id,
                        arm,
                        kind,
                        messages,
                        est,
                        refused.clone(),
                    )
                    .await;
                    out.push((format!("{id}/{}/{}", arm.label(), kind.label()), o));
                }
                out
            });
        }
        while let Some(joined) = set.join_next().await {
            match joined {
                Ok(outs) => {
                    for (what, o) in outs {
                        match o {
                            Outcome::Done | Outcome::Cap => {}
                            Outcome::Failed(why) => {
                                collected.failures.push(format!("{what}: {why}"));
                            }
                        }
                    }
                }
                Err(e) => collected
                    .failures
                    .push(format!("{}: task panicked: {e}", run.id)),
            }
        }
        eprintln!(
            "compaction-eval-live: run {}/{} ({}) done, ${:.4} spent",
            ri + 1,
            runs.len(),
            run.id,
            meter.record().spent_usd
        );
        if meter.record().stopped_by_cap {
            break;
        }
    }
    Ok(collected)
}

/// Per (run, arm) numbers.
struct RunArm {
    recall: Vec<ProbeScore>,
    task: TaskScore,
    ctx_tokens: f64,
    in_tokens: u64,
    out_tokens: u64,
    refused: Option<String>,
}

/// The report.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LiveArm {
    /// The arm.
    pub arm: Arm,
    /// Runs scored.
    pub runs: usize,
    /// Recall: probes the model answered correctly from the arm's context.
    pub recall: Proportion,
    /// Probes the model answered `unknown` (and not correctly).
    pub unknown: usize,
    /// Probes with no answer line in the reply.
    pub unparsed: usize,
    /// Per probe kind: correct and asked.
    pub by_kind: Vec<(String, usize, usize)>,
    /// Probe ids missed.
    pub dropped: Vec<String>,
    /// Task success (all criteria).
    pub task: Proportion,
    /// Criterion: next step named.
    pub task_next_step: Proportion,
    /// Criterion: forbidden directory honoured.
    pub task_forbidden: Proportion,
    /// Criterion: later user constraint stated.
    pub task_constraint: Proportion,
    /// Plans with no JSON object in the reply.
    pub task_unparsed: usize,
    /// Paired per-run recall difference against uncompacted (arm minus uncompacted).
    pub recall_vs_uncompacted: MeanCi,
    /// Paired per-run task-success difference against uncompacted.
    pub task_vs_uncompacted: MeanCi,
    /// Retained context tokens per run (estimate).
    pub context_tokens: MeanCi,
    /// Context tokens saved against uncompacted, paired per run.
    pub tokens_saved: MeanCi,
    /// Share of the uncompacted context tokens saved, paired per run.
    pub saved_fraction: MeanCi,
    /// Input tokens the gateway reported per run (both requests).
    pub input_tokens: MeanCi,
    /// Input tokens saved against uncompacted by the gateway's count, paired.
    pub input_tokens_saved: MeanCi,
    /// Output tokens per run.
    pub output_tokens: MeanCi,
    /// Cost per run, USD, at the list price.
    pub cost_usd_per_run: MeanCi,
    /// Cost over all scored runs, USD.
    pub cost_usd_total: f64,
    /// Runs where the arm could not produce a context.
    pub refused: Vec<String>,
}

/// Whether the lossy double scored below structured, with its interval.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LossyVsStructured {
    /// Lossy recall.
    pub lossy: Proportion,
    /// Structured recall.
    pub structured: Proportion,
    /// Lossy's point estimate is below structured's.
    pub lossy_below_structured: bool,
    /// The Wilson intervals do not overlap.
    pub intervals_separated: bool,
    /// Structured minus lossy recall, paired per run.
    pub paired_difference: MeanCi,
    /// The finding in words, as measured.
    pub statement: String,
}

/// The live report.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LiveReport {
    /// Row.
    pub row: String,
    /// Model.
    pub model: String,
    /// Gateway base URL.
    pub base_url: String,
    /// List price used.
    pub price: Price,
    /// Runs planned.
    pub planned_runs: usize,
    /// Runs with all four arms and both request kinds retained.
    pub runs_scored: usize,
    /// Probes per arm.
    pub probes: usize,
    /// One result per arm.
    pub arms: Vec<LiveArm>,
    /// The metric-can-fail check.
    pub lossy_vs_structured: LossyVsStructured,
    /// The fixture's post-hoc disclosure.
    pub post_hoc: String,
    /// Where questions and criteria come from.
    pub oracle: String,
    /// How it was measured.
    pub method: String,
    /// What it does not establish.
    pub not_measured: String,
    /// sha256 over the exchange files (name and bytes, sorted).
    pub exchanges_sha256: String,
    /// sha256 of `oracle.json`.
    pub oracle_sha256: String,
    /// sha256 over the content above.
    pub digest: String,
}

/// Everything retained, loaded from a directory.
pub struct Bundle {
    /// `oracle.json`.
    pub oracle: Oracle,
    /// The exchanges, sorted by file name.
    pub exchanges: Vec<Exchange>,
    /// Hash over the exchange files.
    pub exchanges_sha256: String,
    /// Hash of `oracle.json`.
    pub oracle_sha256: String,
}

/// Load `oracle.json` and `exchanges/` and check each request against its
/// own hash.
///
/// # Errors
/// A missing or unreadable file, or an exchange whose request no longer
/// matches its hash (an altered file).
pub fn load_bundle(dir: &Path) -> Result<Bundle, String> {
    let oracle_bytes =
        std::fs::read(dir.join("oracle.json")).map_err(|e| format!("oracle.json: {e}"))?;
    let oracle: Oracle =
        serde_json::from_slice(&oracle_bytes).map_err(|e| format!("oracle.json: {e}"))?;
    let mut names: Vec<_> = std::fs::read_dir(dir.join("exchanges"))
        .map_err(|e| format!("exchanges/: {e}"))?
        .filter_map(Result::ok)
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.ends_with(".json"))
        .collect();
    names.sort();
    let mut all = Sha256::new();
    let mut exchanges = Vec::new();
    for name in names {
        let bytes = std::fs::read(dir.join("exchanges").join(&name))
            .map_err(|e| format!("exchanges/{name}: {e}"))?;
        all.update(name.as_bytes());
        all.update([0]);
        all.update(&bytes);
        let ex: Exchange =
            serde_json::from_slice(&bytes).map_err(|e| format!("exchanges/{name}: {e}"))?;
        let again = sha_hex(
            serde_json::to_string(&ex.messages)
                .unwrap_or_default()
                .as_bytes(),
        );
        if again != ex.request_sha256 {
            return Err(format!(
                "exchanges/{name}: the retained request does not match its hash (the file was altered)"
            ));
        }
        if name != file_name(&ex.run, ex.arm, ex.kind) {
            return Err(format!(
                "exchanges/{name}: the file name does not match its content"
            ));
        }
        exchanges.push(ex);
    }
    Ok(Bundle {
        oracle,
        exchanges,
        exchanges_sha256: hex::encode(all.finalize()),
        oracle_sha256: sha_hex(&oracle_bytes),
    })
}

fn find<'a>(b: &'a [Exchange], run: &str, arm: Arm, kind: Kind) -> Option<&'a Exchange> {
    b.iter()
        .find(|e| e.run == run && e.arm == arm && e.kind == kind)
}

/// Runs with all four arms and both kinds retained.
fn scored_runs(b: &Bundle) -> Vec<&OracleRun> {
    b.oracle
        .runs
        .iter()
        .filter(|r| {
            Arm::ALL.iter().all(|a| {
                Kind::ALL
                    .iter()
                    .all(|k| find(&b.exchanges, &r.id, *a, *k).is_some())
            })
        })
        .collect()
}

fn score_run_arm(run: &OracleRun, b: &Bundle, arm: Arm) -> Option<RunArm> {
    let rec = find(&b.exchanges, &run.id, arm, Kind::Recall)?;
    let tsk = find(&b.exchanges, &run.id, arm, Kind::Task)?;
    Some(RunArm {
        recall: score_recall(&run.probes, &rec.reply_text),
        task: score_task(&run.task, &tsk.reply_text),
        ctx_tokens: f64::from(rec.context_tokens_est),
        in_tokens: rec.input_tokens + tsk.input_tokens,
        out_tokens: rec.output_tokens + tsk.output_tokens,
        refused: rec.context_refused.clone(),
    })
}

fn frac(k: usize, n: usize) -> f64 {
    if n == 0 { 0.0 } else { k as f64 / n as f64 }
}

fn pf(p: &Proportion) -> String {
    format!("{}/{} = {:.3} [{:.3}, {:.3}]", p.k, p.n, p.p, p.lo, p.hi)
}

/// Build the report from a retained bundle. Pure: the same bundle gives the
/// same report and digest, which is what [`rescore_dir`] relies on.
#[must_use]
pub fn build_report(b: &Bundle) -> LiveReport {
    let runs = scored_runs(b);
    let price = b.oracle.meta.price;
    // [arm][run]
    let table: Vec<Vec<RunArm>> = Arm::ALL
        .iter()
        .map(|a| {
            runs.iter()
                .filter_map(|r| score_run_arm(r, b, *a))
                .collect()
        })
        .collect();
    let recall_frac = |t: &[RunArm]| -> Vec<f64> {
        t.iter()
            .map(|r| {
                frac(
                    r.recall.iter().filter(|p| p.correct).count(),
                    r.recall.len(),
                )
            })
            .collect()
    };
    let task_val = |t: &[RunArm]| -> Vec<f64> {
        t.iter()
            .map(|r| f64::from(u8::from(r.task.success)))
            .collect()
    };
    let base_recall = recall_frac(&table[0]);
    let base_task = task_val(&table[0]);
    let diff = |a: &[f64], b: &[f64]| -> Vec<f64> { a.iter().zip(b).map(|(x, y)| x - y).collect() };
    let mut arms = Vec::new();
    for (ai, arm) in Arm::ALL.iter().enumerate() {
        let t = &table[ai];
        let probes: Vec<&ProbeScore> = t.iter().flat_map(|r| r.recall.iter()).collect();
        let correct = probes.iter().filter(|p| p.correct).count();
        let mut by_kind: Vec<(String, usize, usize)> = Vec::new();
        for p in &probes {
            match by_kind.iter_mut().find(|(k, _, _)| *k == p.kind) {
                Some(row) => {
                    row.1 += usize::from(p.correct);
                    row.2 += 1;
                }
                None => by_kind.push((p.kind.clone(), usize::from(p.correct), 1)),
            }
        }
        by_kind.sort();
        let count = |f: &dyn Fn(&TaskScore) -> bool| t.iter().filter(|r| f(&r.task)).count();
        let ctx: Vec<f64> = t.iter().map(|r| r.ctx_tokens).collect();
        let base_ctx: Vec<f64> = table[0].iter().map(|r| r.ctx_tokens).collect();
        let saved = diff(&base_ctx, &ctx);
        let fraction: Vec<f64> = saved.iter().zip(&base_ctx).map(|(s, c)| s / c).collect();
        let input: Vec<f64> = t.iter().map(|r| r.in_tokens as f64).collect();
        let base_input: Vec<f64> = table[0].iter().map(|r| r.in_tokens as f64).collect();
        let output: Vec<f64> = t.iter().map(|r| r.out_tokens as f64).collect();
        let cost: Vec<f64> = t
            .iter()
            .map(|r| price.cost_usd(r.in_tokens, r.out_tokens))
            .collect();
        arms.push(LiveArm {
            arm: *arm,
            runs: t.len(),
            recall: wilson(correct, probes.len()),
            unknown: probes.iter().filter(|p| p.unknown).count(),
            unparsed: probes.iter().filter(|p| p.unparsed).count(),
            by_kind,
            dropped: probes
                .iter()
                .filter(|p| !p.correct)
                .map(|p| p.id.clone())
                .collect(),
            task: wilson(count(&|s| s.success), t.len()),
            task_next_step: wilson(count(&|s| s.next_step), t.len()),
            task_forbidden: wilson(count(&|s| s.forbidden), t.len()),
            task_constraint: wilson(count(&|s| s.constraint), t.len()),
            task_unparsed: count(&|s| !s.parsed),
            recall_vs_uncompacted: mean_ci(&diff(&recall_frac(t), &base_recall)),
            task_vs_uncompacted: mean_ci(&diff(&task_val(t), &base_task)),
            context_tokens: mean_ci(&ctx),
            tokens_saved: mean_ci(&saved),
            saved_fraction: mean_ci(&fraction),
            input_tokens: mean_ci(&input),
            input_tokens_saved: mean_ci(&diff(&base_input, &input)),
            output_tokens: mean_ci(&output),
            cost_usd_per_run: mean_ci(&cost),
            cost_usd_total: cost.iter().sum(),
            refused: t.iter().filter_map(|r| r.refused.clone()).collect(),
        });
    }
    let structured = &arms[2];
    let lossy = &arms[3];
    let paired = mean_ci(&diff(&recall_frac(&table[2]), &recall_frac(&table[3])));
    let below = lossy.recall.p < structured.recall.p;
    let separated = lossy.recall.hi < structured.recall.lo;
    let statement = format!(
        "Lossy double recall {} against structured {}: lossy {} structured on the point estimate and the intervals {}; structured minus lossy per run {:.3} [{:.3}, {:.3}].{}",
        pf(&lossy.recall),
        pf(&structured.recall),
        if below {
            "scored below"
        } else {
            "did NOT score below"
        },
        if separated {
            "do not overlap"
        } else {
            "overlap"
        },
        paired.mean,
        paired.ci95.0,
        paired.ci95.1,
        if below && separated {
            ""
        } else {
            " The live result does not show that the metric separates a lossy summarizer from a faithful one at this sample size; it is reported as measured, not tuned."
        }
    );
    let mut report = LiveReport {
        row: ROW.into(),
        model: b.oracle.meta.model.clone(),
        base_url: b.oracle.meta.base_url.clone(),
        price,
        planned_runs: b.oracle.meta.planned_runs,
        runs_scored: runs.len(),
        probes: runs.iter().map(|r| r.probes.len()).sum(),
        lossy_vs_structured: LossyVsStructured {
            lossy: lossy.recall,
            structured: structured.recall,
            lossy_below_structured: below,
            intervals_separated: separated,
            paired_difference: paired,
            statement,
        },
        arms,
        post_hoc: POST_HOC_NOTE.into(),
        oracle: "Questions, answers and continuation criteria are recorded by the fixture generator beside each event log before any arm runs, and each answer is checked to be present in the event log. Nothing is derived from a compaction manifest or from any summary under test. Recall: every answer string must appear in the model's answer line for that question. Task: the plan must name the step the log said was next, name the forbidden directory without proposing to edit it, and state the later user constraint (component and timeout in ms).".into(),
        method: format!(
            "{} synthetic long runs of 30 to 50 entries, each compacted once as a whole by the product's compaction code (extractive epoch; structured summary written by a scripted summarizer and validated by the product's validate; a lossy test double that drops files, failures and decisions). The same model ({}) is called through the OpenAI-compatible gateway directly, NOT through the Core process, with a fixed system prompt and temperature 0, once per (run, arm) for the recall questions (all of a run's questions in one request, one scored answer line per question; the model is told to answer only from the context and to say unknown otherwise) and once per (run, arm) for the continuation plan. The continuation task is a plan scored by log-derived criteria, not a repository-level code task. Intervals: Wilson for proportions, deterministic bootstrap for means and paired differences. Cost is the gateway-reported usage at the list price. Tokens saved are reported both as the estimated retained-context tokens and as the gateway-reported input tokens of the two requests.",
            b.oracle.meta.planned_runs, b.oracle.meta.model
        ),
        not_measured: "Not measured: repository-level task completion by an agent running through the Core; behaviour of real sessions (the runs are synthetic); other models; a larger-window uncompacted arm differing from the full log; sampling variance at temperature above 0.".into(),
        exchanges_sha256: b.exchanges_sha256.clone(),
        oracle_sha256: b.oracle_sha256.clone(),
        digest: String::new(),
    };
    report.digest = digest_of(&report);
    report
}

fn digest_of(r: &LiveReport) -> String {
    let mut copy = r.clone();
    copy.digest = String::new();
    sha_hex(serde_json::to_string(&copy).unwrap_or_default().as_bytes())
}

/// Recompute the report from the retained files alone (no network) and
/// compare its digest with the one in `result.json`.
///
/// # Errors
/// A file is missing, altered, or the digest differs.
pub fn rescore_dir(dir: &Path) -> Result<LiveReport, String> {
    let bundle = load_bundle(dir)?;
    let report = build_report(&bundle);
    let stored: Value = serde_json::from_slice(
        &std::fs::read(dir.join("result.json")).map_err(|e| format!("result.json: {e}"))?,
    )
    .map_err(|e| format!("result.json: {e}"))?;
    let want = stored["report"]["digest"].as_str().unwrap_or_default();
    if want != report.digest {
        return Err(format!(
            "the digest does not reproduce: result.json says {want}, the retained exchanges give {}",
            report.digest
        ));
    }
    Ok(report)
}

/// The missing measurements of a finished collection.
#[must_use]
pub fn missing_measurements(report: &LiveReport, b: &Bundle, failures: &[String]) -> Vec<String> {
    let mut missing = Vec::new();
    for f in failures.iter().take(20) {
        missing.push(format!("exchange failed (an error, not a zero): {f}"));
    }
    if failures.len() > 20 {
        missing.push(format!("and {} more failed exchanges", failures.len() - 20));
    }
    if report.runs_scored < report.planned_runs {
        missing.push(format!(
            "only {} of {} planned long runs have all four arms scored for recall and task",
            report.runs_scored, report.planned_runs
        ));
    }
    if report.runs_scored < MIN_RUNS {
        missing.push(format!(
            "at least {MIN_RUNS} long runs are required, {} were scored",
            report.runs_scored
        ));
    }
    let unknown_usage = b.exchanges.iter().filter(|e| !e.usage_known).count();
    if unknown_usage > 0 {
        missing.push(format!(
            "{unknown_usage} replies carried no usage: tokens and cost are not measured for them"
        ));
    }
    missing
}

/// `summary.md`.
#[must_use]
pub fn summary_md(report: &LiveReport, status: &LiveStatus) -> String {
    let mut s = format!(
        "# PX-138 live compaction evaluation\n\nModel `{}`; {} of {} planned long runs scored; {} probes per arm. Status: {}.\n\n",
        report.model,
        report.runs_scored,
        report.planned_runs,
        report.probes,
        if status.complete {
            "COMPLETE"
        } else {
            "INCOMPLETE"
        }
    );
    s.push_str("| arm | recall (95% Wilson) | unknown | unparsed | task success (95% Wilson) | context tokens saved (mean, 95% CI) | input tokens saved (reported) | cost per run USD |\n|---|---|---|---|---|---|---|---|\n");
    for a in &report.arms {
        s.push_str(&format!(
            "| {} | {} | {} | {} | {} | {:.0} [{:.0}, {:.0}] | {:.0} [{:.0}, {:.0}] | {:.5} |\n",
            a.arm.label(),
            pf(&a.recall),
            a.unknown,
            a.unparsed,
            pf(&a.task),
            a.tokens_saved.mean,
            a.tokens_saved.ci95.0,
            a.tokens_saved.ci95.1,
            a.input_tokens_saved.mean,
            a.input_tokens_saved.ci95.0,
            a.input_tokens_saved.ci95.1,
            a.cost_usd_per_run.mean
        ));
    }
    s.push_str(
        "\nPaired differences against uncompacted (per run, mean and 95% bootstrap interval):\n\n",
    );
    for a in report.arms.iter().skip(1) {
        s.push_str(&format!(
            "- {}: recall {:+.3} [{:+.3}, {:+.3}]; task success {:+.3} [{:+.3}, {:+.3}]\n",
            a.arm.label(),
            a.recall_vs_uncompacted.mean,
            a.recall_vs_uncompacted.ci95.0,
            a.recall_vs_uncompacted.ci95.1,
            a.task_vs_uncompacted.mean,
            a.task_vs_uncompacted.ci95.0,
            a.task_vs_uncompacted.ci95.1
        ));
    }
    s.push_str(&format!("\n{}\n\n", report.lossy_vs_structured.statement));
    s.push_str(&format!(
        "Spend: ${:.4} of ${:.2} cap over {} calls{}.\n\n",
        status.spend.spent_usd,
        status.spend.cap_usd,
        status.spend.calls,
        if status.spend.stopped_by_cap {
            " (STOPPED BY THE CAP)"
        } else {
            ""
        }
    ));
    if !status.missing.is_empty() {
        s.push_str("Missing measurements:\n\n");
        for m in &status.missing {
            s.push_str(&format!("- {m}\n"));
        }
        s.push('\n');
    }
    s.push_str(&format!("Method: {}\n\nNot measured: {}\n\nDigest `{}` (reproduce with `compaction-eval-live --rescore <dir>`).\n", report.method, report.not_measured, report.digest));
    s
}

/// Write `result.json`, `status.json` and `summary.md` for a finished
/// collection, then check that the digest reproduces from the files. Returns
/// the status.
///
/// # Errors
/// The retained files cannot be loaded or the outputs cannot be written.
pub fn finish(
    out_dir: &Path,
    meter: &SpendMeter,
    failures: &[String],
) -> Result<(LiveReport, LiveStatus), String> {
    let bundle = load_bundle(out_dir)?;
    let report = build_report(&bundle);
    let mut missing = missing_measurements(&report, &bundle, failures);
    let write_result = |status: &LiveStatus| -> Result<(), String> {
        let result = json!({
            "report": report,
            "status": status,
            "failures": failures,
            "exchanges": bundle.exchanges.len(),
        });
        std::fs::write(
            out_dir.join("result.json"),
            serde_json::to_vec_pretty(&result).map_err(|e| e.to_string())?,
        )
        .map_err(|e| format!("result.json: {e}"))
    };
    let status = LiveStatus::new(ROW, missing.clone(), meter.record());
    write_result(&status)?;
    let status = match rescore_dir(out_dir) {
        Ok(_) => status,
        Err(e) => {
            missing.push(format!(
                "the report digest does not reproduce from the retained data: {e}"
            ));
            let s = LiveStatus::new(ROW, missing, meter.record());
            write_result(&s)?;
            s
        }
    };
    std::fs::write(
        out_dir.join("status.json"),
        serde_json::to_vec_pretty(&status).map_err(|e| e.to_string())?,
    )
    .map_err(|e| format!("status.json: {e}"))?;
    std::fs::write(out_dir.join("summary.md"), summary_md(&report, &status))
        .map_err(|e| format!("summary.md: {e}"))?;
    Ok((report, status))
}
