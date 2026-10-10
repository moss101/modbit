//! The live retrieval run (PX-137): checkout the pinned repository, put the
//! same model in front of every question twice (baseline tools, then the
//! same tools plus `retrieve`), retain every transcript, measure the index
//! times at the target corpus size, and write `result.json`, `summary.md` and
//! `status.json`. See `live.rs` for scoring and `live_tools.rs` for the tools.
//!
//! Exit codes of the `retrieval-live` binary: 0 only when the status is
//! complete; 1 when the run is incomplete or failed (everything measured is
//! still written, with an explicit `missing` list); 2 when live mode is
//! refused (nothing is written).

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;

use modbit_bench_context_economics::chat::{Chat, ChatError};
use modbit_bench_context_economics::live::live_config;
use modbit_bench_context_economics::spend::{LiveStatus, SpendMeter};
use modbit_retrieval::bench::Case;
use serde_json::{Value, json};

use crate::external::detect_rg_from_env;
use crate::live::{
    CaseFile, CaseMetrics, INDEX_BYTES_LIVE, IndexMeasure, Limits, LiveCase, LiveReport,
    MIN_CASES_LIVE, ModelCall, ROW, RepoInfo, ReportMeta, Retained, TRANSCRIPT_SCHEMA, ToolRecord,
    Transcript, assemble, load_transcripts, metrics, rescore, sha256_hex, summary_md,
    validate_transcript,
};
use crate::live_tools::{Retriever, TOOL_OUTPUT_CHARS, ToolBox, ToolSurface, schemas};

/// The committed pinned case file, compiled in.
pub const DEFAULT_CASES: &str = include_str!("../cases/ripgrep-14.1.1.json");

/// Tool rounds per case; one more call without tools forces the answer.
pub const MAX_ROUNDS: u32 = 12;
/// Input plus output tokens one case may use across all its calls.
pub const CASE_TOKEN_BUDGET: u64 = 150_000;
/// Output tokens per model call.
pub const MAX_OUTPUT_TOKENS: u32 = 2_048;
/// Bytes of one synthetic padding file.
const PAD_FILE_BYTES: usize = 16 * 1024;
/// Cases in a row that may fail before the run gives up on the gateway.
const MAX_CONSECUTIVE_FAILURES: usize = 5;

/// What a run is configured with. [`live_spec`] is the only constructor the
/// CLI uses; the numbers it fixes (fifty cases, the 100 MB corpus) cannot be
/// lowered from the command line or the environment.
#[derive(Clone, Debug)]
pub struct Spec {
    /// The case file.
    pub cases: CaseFile,
    /// sha256 of the case file's bytes.
    pub cases_sha256: String,
    /// Where the result goes.
    pub out_dir: PathBuf,
    /// Where the checkout and the padded corpus are built.
    pub work_dir: PathBuf,
    /// The hard spend cap.
    pub max_cost_usd: f64,
    /// Fewest cases a complete run scores.
    pub min_cases: usize,
    /// Corpus size the index times are measured at.
    pub index_target_bytes: u64,
    /// Caps per case.
    pub limits: Limits,
}

/// The limits every run uses.
#[must_use]
pub fn default_limits() -> Limits {
    Limits {
        max_rounds: MAX_ROUNDS,
        tool_output_chars: TOOL_OUTPUT_CHARS,
        case_token_budget: CASE_TOKEN_BUDGET,
        max_output_tokens: MAX_OUTPUT_TOKENS,
    }
}

/// The live run's spec: at least fifty cases and the 100 MB index corpus are
/// requirements of QUAL-PX-137, fixed here.
///
/// # Errors
/// The case file does not parse.
pub fn live_spec(
    cases_text: &str,
    out_dir: PathBuf,
    work_dir: PathBuf,
    max_cost_usd: f64,
) -> Result<Spec, String> {
    Ok(Spec {
        cases: CaseFile::parse(cases_text)?,
        cases_sha256: sha256_hex(cases_text.as_bytes()),
        out_dir,
        work_dir,
        max_cost_usd,
        min_cases: MIN_CASES_LIVE,
        index_target_bytes: INDEX_BYTES_LIVE,
        limits: default_limits(),
    })
}

/// Test-only switches. The binary never builds one.
#[doc(hidden)]
#[derive(Clone, Copy, Debug, Default)]
pub struct Knobs {
    /// The mutation the no-forced-retrieval rule must catch: the harness
    /// issues a `retrieve` call for the treatment before the model's first
    /// reply and records it as forced.
    pub force_retrieve: bool,
}

const SYSTEM_PROMPT: &str = "You answer questions about a source-code repository checked out in the current directory. \
Find the source file or files that answer the question. Use the tools to look at the code; do not answer from memory. \
When you are done, reply with ONLY a JSON object of the form {\"files\": [\"path/relative/to/the/repository/root\", ...]} \
naming every file the question needs and nothing else. No prose.";

/// A case that did not produce a scored transcript.
#[derive(Debug)]
pub struct CaseFailure {
    /// The spend cap ended it.
    pub cap: bool,
    /// Why.
    pub message: String,
    /// What had been recorded.
    pub partial: Transcript,
}

fn est_tokens(messages: &[Value], tools: &[Value]) -> u64 {
    let chars: usize = messages.iter().map(|m| m.to_string().len()).sum::<usize>()
        + tools.iter().map(|t| t.to_string().len()).sum::<usize>();
    (chars / 3) as u64 + 64
}

fn ms_since(t: Instant) -> u64 {
    u64::try_from(t.elapsed().as_millis()).unwrap_or(u64::MAX)
}

fn tool_message(id: &str, text: &str) -> Value {
    json!({"role": "tool", "tool_call_id": id, "content": text})
}

fn run_tool(
    tools: &ToolBox,
    round: u32,
    id: &str,
    name: &str,
    arguments: &str,
    forced: bool,
) -> (ToolRecord, String) {
    let t0 = Instant::now();
    let out = match serde_json::from_str::<Value>(arguments) {
        Ok(args) if args.is_object() => tools.execute(name, &args),
        _ => crate::live_tools::ToolOutput {
            text: "invalid arguments: expected a JSON object".to_owned(),
            error: true,
            truncated: false,
        },
    };
    (
        ToolRecord {
            round,
            call_id: id.to_owned(),
            tool: name.to_owned(),
            arguments: arguments.to_owned(),
            forced,
            error: out.error,
            truncated: out.truncated,
            result_chars: out.text.chars().count(),
            ms: ms_since(t0),
        },
        out.text,
    )
}

/// Run one agent on one case.
///
/// # Errors
/// The spend cap, or a gateway failure after retries; the failure carries the
/// partial transcript.
pub async fn run_case(
    chat: &Chat,
    meter: &SpendMeter,
    tools: &ToolBox,
    surface: ToolSurface,
    case: &LiveCase,
    limits: &Limits,
    knobs: Knobs,
) -> Result<Transcript, Box<CaseFailure>> {
    let started = Instant::now();
    let schema = schemas(surface);
    let mut t = Transcript {
        schema: TRANSCRIPT_SCHEMA.to_owned(),
        row: ROW.to_owned(),
        profile: surface.name().to_owned(),
        case_id: case.id.clone(),
        kind: case.kind.clone(),
        question: case.question.clone(),
        labels: case.labels.iter().map(|l| l.path.clone()).collect(),
        model: chat.model.clone(),
        messages: vec![
            json!({"role": "system", "content": SYSTEM_PROMPT}),
            json!({"role": "user", "content": case.question}),
        ],
        model_calls: vec![],
        tool_calls: vec![],
        final_text: String::new(),
        capped: None,
        wall_ms: 0,
    };
    if knobs.force_retrieve && surface == ToolSurface::Treatment {
        let args = json!({"query": case.question}).to_string();
        t.messages.push(json!({"role": "assistant", "content": "", "tool_calls": [
            {"id": "forced_0", "type": "function", "function": {"name": "retrieve", "arguments": args}}]}));
        let (rec, text) = run_tool(tools, 0, "forced_0", "retrieve", &args, true);
        t.tool_calls.push(rec);
        t.messages.push(tool_message("forced_0", &text));
    }
    let mut cumulative = 0u64;
    for round in 1..=limits.max_rounds + 1 {
        let final_call = round > limits.max_rounds;
        let offered: &[Value] = if final_call { &[] } else { &schema };
        if final_call {
            t.capped = Some("rounds".to_owned());
            t.messages.push(json!({"role": "user", "content":
                "You have used all of your tool rounds. Reply now with ONLY the JSON object {\"files\": [...]}."}));
        }
        let reserve = chat.price.cost_usd(
            est_tokens(&t.messages, offered),
            u64::from(limits.max_output_tokens),
        );
        let reply = match chat
            .complete(
                meter,
                reserve,
                &t.messages,
                offered,
                limits.max_output_tokens,
            )
            .await
        {
            Ok(r) => r,
            Err(e) => {
                t.wall_ms = ms_since(started);
                return Err(Box::new(CaseFailure {
                    cap: matches!(e, ChatError::Cap(_)),
                    message: e.to_string(),
                    partial: t,
                }));
            }
        };
        t.model_calls.push(ModelCall {
            round,
            input_tokens: reply.input_tokens,
            output_tokens: reply.output_tokens,
            cached_input_tokens: reply.cached_input_tokens,
            usage_known: reply.usage_known,
            latency_ms: reply.latency_ms,
            finish_reason: reply.finish_reason.clone(),
            cost_usd: if reply.usage_known {
                chat.price.cost_usd(reply.input_tokens, reply.output_tokens)
            } else {
                0.0
            },
            request_id: reply.request_id.clone(),
        });
        cumulative += reply.input_tokens + reply.output_tokens;
        let ids: Vec<String> = reply
            .tool_calls
            .iter()
            .enumerate()
            .map(|(i, c)| {
                if c.id.is_empty() {
                    format!("call_{round}_{i}")
                } else {
                    c.id.clone()
                }
            })
            .collect();
        let mut assistant = json!({"role": "assistant", "content": reply.text});
        if !final_call && !reply.tool_calls.is_empty() {
            assistant["tool_calls"] = Value::Array(
                reply
                    .tool_calls
                    .iter()
                    .zip(&ids)
                    .map(|(c, id)| {
                        json!({"id": id, "type": "function",
                        "function": {"name": c.name, "arguments": c.arguments}})
                    })
                    .collect(),
            );
        }
        t.messages.push(assistant);
        if final_call || reply.tool_calls.is_empty() {
            t.final_text = reply.text;
            break;
        }
        for (c, id) in reply.tool_calls.iter().zip(&ids) {
            let (rec, text) = run_tool(tools, round, id, &c.name, &c.arguments, false);
            t.tool_calls.push(rec);
            t.messages.push(tool_message(id, &text));
        }
        if cumulative > limits.case_token_budget {
            t.capped = Some("token_budget".to_owned());
            break;
        }
    }
    t.wall_ms = ms_since(started);
    Ok(t)
}

fn git(dir: &Path, args: &[&str]) -> Result<String, String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .env("GIT_TERMINAL_PROMPT", "0")
        .output()
        .map_err(|e| format!("starting git: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "git {}: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Clone `url` into `work_dir/repo` (unless it is already there), check out
/// `commit` detached, and verify that HEAD is exactly that commit and the
/// tree is clean.
///
/// # Errors
/// Any git failure, a HEAD that is not the pinned commit, a dirty tree.
pub fn prepare_checkout(url: &str, commit: &str, work_dir: &Path) -> Result<PathBuf, String> {
    let repo = work_dir.join("repo");
    if !repo.join(".git").exists() {
        std::fs::create_dir_all(work_dir)
            .map_err(|e| format!("creating {}: {e}", work_dir.display()))?;
        let out = Command::new("git")
            .arg("-C")
            .arg(work_dir)
            .args(["clone", "--quiet", "--no-checkout", url, "repo"])
            .env("GIT_TERMINAL_PROMPT", "0")
            .output()
            .map_err(|e| format!("starting git: {e}"))?;
        if !out.status.success() {
            return Err(format!(
                "git clone {url}: {}",
                String::from_utf8_lossy(&out.stderr).trim()
            ));
        }
        git(&repo, &["checkout", "--quiet", "--detach", commit])?;
    }
    let head = git(&repo, &["rev-parse", "HEAD"])?
        .trim()
        .to_ascii_lowercase();
    if head != commit.to_ascii_lowercase() {
        return Err(format!("HEAD is {head}, not the pinned commit {commit}"));
    }
    if !git(&repo, &["status", "--porcelain"])?.trim().is_empty() {
        return Err("the checkout has local changes".to_owned());
    }
    Ok(repo)
}

/// Labels whose file is missing or lacks its evidence string.
#[must_use]
pub fn verify_labels(root: &Path, cases: &[LiveCase]) -> Vec<String> {
    let mut bad = Vec::new();
    for c in cases {
        for l in &c.labels {
            let p = l
                .path
                .split('/')
                .fold(root.to_path_buf(), |acc, seg| acc.join(seg));
            match std::fs::read(&p) {
                Ok(b) if String::from_utf8_lossy(&b).contains(&l.evidence) => {}
                Ok(_) => bad.push(format!(
                    "{}: `{}` does not contain `{}`",
                    c.id, l.path, l.evidence
                )),
                Err(_) => bad.push(format!(
                    "{}: `{}` does not exist at the pinned commit",
                    c.id, l.path
                )),
            }
        }
    }
    bad
}

fn tracked_files(root: &Path) -> Result<Vec<String>, String> {
    Ok(git(root, &["ls-files", "-z"])?
        .split('\0')
        .filter(|p| !p.is_empty())
        .map(str::to_owned)
        .collect())
}

/// What building the padded corpus produced.
#[derive(Clone, Debug)]
pub struct Padded {
    /// Bytes of the real checkout copied in.
    pub checkout_bytes: u64,
    /// Real files copied in.
    pub checkout_files: usize,
    /// Synthetic bytes.
    pub padding_bytes: u64,
    /// Synthetic files.
    pub padding_files: usize,
}

const WORDS: [&str; 48] = [
    "buffer", "matcher", "pattern", "stream", "cursor", "window", "offset", "digest", "filter",
    "record", "region", "scanner", "token", "parser", "walker", "sorter", "reader", "writer",
    "tracker", "planner", "bundle", "channel", "context", "counter", "decoder", "encoder",
    "handler", "indexer", "journal", "keeper", "loader", "mapper", "monitor", "packer", "queue",
    "router", "runner", "slicer", "tagger", "timer", "tracer", "updater", "vector", "worker",
    "yielder", "zipper", "binder", "caller",
];

/// Deterministic Rust-looking padding text of exactly `PAD_FILE_BYTES` bytes.
fn pad_text(n: u64) -> String {
    let mut state = 0x9E37_79B9_7F4A_7C15u64 ^ n.wrapping_mul(0xD134_2543_DE82_EF95);
    let mut next = move || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        state
    };
    let mut s = format!(
        "//! Synthetic padding module {n}. Generated by the retrieval benchmark; not part of the repository.\n\n"
    );
    let mut k = 0u64;
    while s.len() < PAD_FILE_BYTES {
        let (a, b, c) = (
            WORDS[(next() % 48) as usize],
            WORDS[(next() % 48) as usize],
            WORDS[(next() % 48) as usize],
        );
        let seed = next() % 100_000;
        s.push_str(&format!(
            "/// Folds the {a} of a {b} into a {c} checksum.\npub fn pad_{n}_{k}_{a}_{b}(input: &[u8]) -> u64 {{\n    let mut acc = {seed}u64;\n    for byte in input {{\n        acc = acc.wrapping_mul(31).wrapping_add(u64::from(*byte)) ^ {seed};\n    }}\n    acc\n}}\n\n"
        ));
        k += 1;
    }
    s.truncate(PAD_FILE_BYTES);
    s
}

/// Copy the checkout's tracked files into `dest` and add deterministic
/// padding files until the corpus holds `target_bytes`, then make `dest` a
/// Git repository so the index sees the same file set as in a real workspace.
///
/// # Errors
/// File-system or git failures.
pub fn build_padded(checkout: &Path, dest: &Path, target_bytes: u64) -> Result<Padded, String> {
    if dest.exists() {
        std::fs::remove_dir_all(dest).map_err(|e| format!("clearing {}: {e}", dest.display()))?;
    }
    std::fs::create_dir_all(dest).map_err(|e| e.to_string())?;
    let (mut bytes, mut files) = (0u64, 0usize);
    for rel in tracked_files(checkout)? {
        let from = rel
            .split('/')
            .fold(checkout.to_path_buf(), |a, s| a.join(s));
        let to = rel.split('/').fold(dest.to_path_buf(), |a, s| a.join(s));
        if let Some(parent) = to.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        if let Ok(n) = std::fs::copy(&from, &to) {
            bytes += n;
            files += 1;
        }
    }
    let mut padding = Padded {
        checkout_bytes: bytes,
        checkout_files: files,
        padding_bytes: 0,
        padding_files: 0,
    };
    let mut i = 0u64;
    while padding.checkout_bytes + padding.padding_bytes < target_bytes {
        let dir = dest.join("padding").join(format!("p{:04}", i / 200));
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let text = pad_text(i);
        std::fs::write(dir.join(format!("synth_{i:06}.rs")), &text).map_err(|e| e.to_string())?;
        padding.padding_bytes += text.len() as u64;
        padding.padding_files += 1;
        i += 1;
    }
    git(dest, &["init", "--quiet"])?;
    Ok(padding)
}

/// Cold and incremental index times of the real indexes over the padded corpus.
///
/// # Errors
/// Building the corpus or the indexes.
pub fn measure_index(
    checkout: &Path,
    work_dir: &Path,
    target_bytes: u64,
    first: &LiveCase,
) -> Result<IndexMeasure, String> {
    let dest = work_dir.join("padded-corpus");
    let padded = build_padded(checkout, &dest, target_bytes)?;
    let case = Case {
        id: first.id.clone(),
        query: first.question.clone(),
        intent: String::new(),
        relevant: first.labels.iter().map(|l| l.path.clone()).collect(),
        impacted: vec![],
    };
    let report = modbit_retrieval::bench::run(&dest, "retrieval-live index corpus", &[case], 10)?;
    let _ = std::fs::remove_dir_all(&dest);
    Ok(IndexMeasure {
        corpus: format!(
            "the pinned checkout ({} files, {} bytes) plus {} deterministic synthetic padding files ({} bytes) to reach the target; the padding is not repository content",
            padded.checkout_files,
            padded.checkout_bytes,
            padded.padding_files,
            padded.padding_bytes
        ),
        checkout_bytes: padded.checkout_bytes,
        checkout_files: padded.checkout_files,
        padding_bytes: padded.padding_bytes,
        padding_files: padded.padding_files,
        target_bytes,
        indexed_files: report.files,
        indexed_bytes: report.bytes,
        cold: report.cold_index,
        incremental_ms: report.incremental_ms,
        incremental_path: report.incremental_path,
    })
}

fn write_json(path: &Path, v: &impl serde::Serialize) -> Result<(), String> {
    if let Some(p) = path.parent() {
        std::fs::create_dir_all(p).map_err(|e| format!("creating {}: {e}", p.display()))?;
    }
    let text = serde_json::to_vec_pretty(v).map_err(|e| e.to_string())?;
    std::fs::write(path, text).map_err(|e| format!("writing {}: {e}", path.display()))
}

fn spend_line(s: &modbit_bench_context_economics::spend::SpendRecord) -> String {
    format!(
        "Spend: ${:.4} of the ${:.2} cap over {} model calls ({} input / {} output tokens){}.",
        s.spent_usd,
        s.cap_usd,
        s.calls,
        s.input_tokens,
        s.output_tokens,
        if s.stopped_by_cap {
            "; STOPPED BY THE CAP"
        } else {
            ""
        }
    )
}

/// Everything the run learned, before the files are written.
struct Outcome {
    meta: ReportMeta,
    missing: Vec<String>,
    errors: Vec<String>,
}

fn write_outputs(
    spec: &Spec,
    meter: &SpendMeter,
    o: &Outcome,
    digest_reproduced: bool,
) -> Result<LiveStatus, String> {
    let retained = load_transcripts(&spec.out_dir).unwrap_or_default();
    let report: LiveReport = assemble(&o.meta, &retained);
    let per_case: Vec<CaseMetrics> = retained.iter().map(|r| metrics(&r.transcript)).collect();
    let status = LiveStatus::new(ROW, o.missing.clone(), meter.record());
    let result = json!({
        "row": ROW,
        "harness": report.harness,
        "complete": status.complete,
        "missing": status.missing,
        "errors": o.errors,
        "spend": status.spend,
        "digest_reproduced": digest_reproduced,
        "report": report,
        "per_case": per_case,
    });
    write_json(&spec.out_dir.join("result.json"), &result)?;
    write_json(&spec.out_dir.join("status.json"), &status)?;
    let summary = summary_md(&report, &status.missing, &spend_line(&status.spend));
    std::fs::write(spec.out_dir.join("summary.md"), summary).map_err(|e| e.to_string())?;
    Ok(status)
}

fn empty_repo(spec: &Spec) -> RepoInfo {
    RepoInfo {
        url: spec.cases.repo_url.clone(),
        commit: spec.cases.commit.clone(),
        head: String::new(),
        files: 0,
        bytes: 0,
        labels_verified: 0,
    }
}

/// Run the whole evaluation and write the result bundle. Returns the status;
/// the caller turns it into an exit code.
///
/// # Errors
/// Only a failure to write the bundle itself.
pub async fn execute(
    spec: &Spec,
    chat: &Chat,
    meter: &SpendMeter,
    knobs: Knobs,
) -> Result<LiveStatus, String> {
    std::fs::create_dir_all(&spec.out_dir)
        .map_err(|e| format!("creating {}: {e}", spec.out_dir.display()))?;
    let mut o = Outcome {
        meta: ReportMeta {
            model: chat.model.clone(),
            price: chat.price,
            repo: empty_repo(spec),
            cases_name: spec.cases.name.clone(),
            cases_sha256: spec.cases_sha256.clone(),
            case_order: spec.cases.cases.iter().map(|c| c.id.clone()).collect(),
            limits: spec.limits.clone(),
            index: None,
        },
        missing: vec![],
        errors: vec![],
    };
    // Start from a clean transcripts directory: a stale transcript from an
    // earlier run must not be scored as this run's.
    let _ = std::fs::remove_dir_all(spec.out_dir.join("transcripts"));

    if spec.cases.cases.len() < spec.min_cases {
        o.missing.push(format!(
            "case set below the required {}: the case file has {} cases",
            spec.min_cases,
            spec.cases.cases.len()
        ));
        return write_outputs(spec, meter, &o, false);
    }
    let rg = match detect_rg_from_env() {
        Ok(rg) => rg,
        Err(e) => {
            o.missing.push(format!("baseline binary: {e}"));
            return write_outputs(spec, meter, &o, false);
        }
    };
    let repo = match prepare_checkout(&spec.cases.repo_url, &spec.cases.commit, &spec.work_dir) {
        Ok(r) => r,
        Err(e) => {
            o.missing.push(format!("pinned checkout: {e}"));
            return write_outputs(spec, meter, &o, false);
        }
    };
    let bad = verify_labels(&repo, &spec.cases.cases);
    if !bad.is_empty() {
        o.missing.push(format!(
            "label verification failed for {} labels, e.g. {}",
            bad.len(),
            bad[0]
        ));
        return write_outputs(spec, meter, &o, false);
    }
    let files = tracked_files(&repo)?;
    o.meta.repo = RepoInfo {
        url: spec.cases.repo_url.clone(),
        commit: spec.cases.commit.clone(),
        head: git(&repo, &["rev-parse", "HEAD"])?.trim().to_owned(),
        bytes: files
            .iter()
            .filter_map(|f| {
                std::fs::metadata(f.split('/').fold(repo.clone(), |a, s| a.join(s))).ok()
            })
            .map(|m| m.len())
            .sum(),
        files: files.len(),
        labels_verified: spec.cases.cases.iter().map(|c| c.labels.len()).sum(),
    };

    let retriever = match Retriever::build(&repo) {
        Ok(r) => r,
        Err(e) => {
            o.missing.push(format!("treatment retrieval index: {e}"));
            return write_outputs(spec, meter, &o, false);
        }
    };
    let baseline_tools = ToolBox::new(&repo, rg.clone(), None)?;
    let treatment_tools = ToolBox::new(&repo, rg, Some(retriever))?;

    let mut consecutive_failures = 0usize;
    'cases: for (i, case) in spec.cases.cases.iter().enumerate() {
        // Both profiles per case, so a stop leaves a balanced result; the
        // order alternates so neither profile always runs first.
        let order = if i % 2 == 0 {
            [ToolSurface::Baseline, ToolSurface::Treatment]
        } else {
            [ToolSurface::Treatment, ToolSurface::Baseline]
        };
        for surface in order {
            let tools = if surface == ToolSurface::Baseline {
                &baseline_tools
            } else {
                &treatment_tools
            };
            match run_case(chat, meter, tools, surface, case, &spec.limits, knobs).await {
                Ok(t) => {
                    consecutive_failures = 0;
                    write_json(
                        &spec
                            .out_dir
                            .join("transcripts")
                            .join(surface.name())
                            .join(format!("{}.json", case.id)),
                        &t,
                    )?;
                }
                Err(f) => {
                    write_json(
                        &spec
                            .out_dir
                            .join("transcripts")
                            .join(surface.name())
                            .join(format!("{}.error.json", case.id)),
                        &json!({"case": case.id, "profile": surface.name(), "stopped_by_cap": f.cap,
                                "error": f.message, "partial": f.partial}),
                    )?;
                    if f.cap {
                        break 'cases;
                    }
                    o.errors
                        .push(format!("{}/{}: {}", surface.name(), case.id, f.message));
                    consecutive_failures += 1;
                    if consecutive_failures >= MAX_CONSECUTIVE_FAILURES {
                        o.errors.push(format!(
                            "the run gave up after {MAX_CONSECUTIVE_FAILURES} consecutive failed cases: the gateway is not usable"
                        ));
                        break 'cases;
                    }
                }
            }
        }
    }

    // Index times at the target corpus size.
    match measure_index(
        &repo,
        &spec.work_dir,
        spec.index_target_bytes,
        &spec.cases.cases[0],
    ) {
        Ok(ix) if ix.indexed_bytes >= spec.index_target_bytes => o.meta.index = Some(ix),
        Ok(ix) => {
            let (got, want) = (ix.indexed_bytes, spec.index_target_bytes);
            o.meta.index = Some(ix);
            o.missing.push(format!(
                "index times: the index held {got} searchable bytes, below the {want} target"
            ));
        }
        Err(e) => o
            .missing
            .push(format!("index times at the target corpus size: {e}")),
    }

    // Validate what was retained and list what is missing.
    let retained = load_transcripts(&spec.out_dir)?;
    let mut invalid = false;
    for r in &retained {
        if let Err(e) = validate_transcript(&r.transcript) {
            invalid = true;
            o.missing.push(format!("invalid transcript: {e}"));
        }
    }
    let have = |case: &str, profile: &str| {
        retained
            .iter()
            .any(|r| r.transcript.case_id == case && r.transcript.profile == profile)
    };
    let unpaired: Vec<&str> = spec
        .cases
        .cases
        .iter()
        .filter(|c| !(have(&c.id, "baseline") && have(&c.id, "treatment")))
        .map(|c| c.id.as_str())
        .collect();
    if !unpaired.is_empty() {
        o.missing.push(format!(
            "{} of {} cases are not scored under both profiles (first: {})",
            unpaired.len(),
            spec.cases.cases.len(),
            unpaired[0]
        ));
    }
    for e in o.errors.iter().take(10) {
        o.missing.push(format!("case error: {e}"));
    }
    if o.errors.len() > 10 {
        o.missing
            .push(format!("... and {} more case errors", o.errors.len() - 10));
    }
    if retained
        .iter()
        .any(|r| r.transcript.model_calls.iter().any(|m| !m.usage_known))
    {
        o.missing.push("token usage: the gateway returned replies without a usage block, so input tokens are not measured".to_owned());
    }
    let paired = spec.cases.cases.len() - unpaired.len();
    if paired < spec.min_cases {
        o.missing.push(format!(
            "only {paired} cases are scored under both profiles; {} are required",
            spec.min_cases
        ));
    }

    // Write, then prove the digest reproduces from what was written.
    write_outputs(spec, meter, &o, false)?;
    if !invalid && !retained.is_empty() {
        match rescore(&spec.out_dir) {
            Ok(_) => return write_outputs(spec, meter, &o, true),
            Err(e) => {
                o.missing.push(format!("report digest: {e}"));
                return write_outputs(spec, meter, &o, false);
            }
        }
    }
    if retained.is_empty() {
        o.missing.push("no transcript was retained".to_owned());
    }
    write_outputs(spec, meter, &o, false)
}

fn usage() -> &'static str {
    "usage: retrieval-live --out-dir <dir> [--max-cost-usd <usd>] [--work-dir <dir>] [--cases <file>]\n       retrieval-live --rescore <result-dir>\n\
     Live mode needs MODBIT_LIVE=1, MODBIT_LIVE_API_KEY, MODBIT_LIVE_MODEL (and the MODBIT_OPENAI_* gateway variables); the cap may be given as MODBIT_LIVE_MAX_COST_USD."
}

fn arg_value(args: &[String], flag: &str) -> Option<String> {
    args.iter()
        .position(|a| a == flag)
        .and_then(|i| args.get(i + 1))
        .cloned()
        .or_else(|| {
            args.iter()
                .find_map(|a| a.strip_prefix(&format!("{flag}=")).map(str::to_owned))
        })
}

/// Rescore a retained result directory (no network).
#[must_use]
pub fn rescore_main(dir: &Path) -> i32 {
    match rescore(dir) {
        Ok(r) => {
            println!(
                "{}",
                summary_md(&r.report, &[], "Rescored from the retained transcripts.")
            );
            println!("report digest reproduced: {}", r.stored_digest);
            0
        }
        Err(e) => {
            eprintln!("RESCORE FAILED: {e}");
            1
        }
    }
}

/// The `retrieval-live` entry point. `var` reads the environment.
#[must_use]
pub fn cli_main(args: &[String], var: &dyn Fn(&str) -> Option<String>) -> i32 {
    if let Some(dir) = arg_value(args, "--rescore") {
        return rescore_main(Path::new(&dir));
    }
    let cfg = match live_config(var) {
        Ok(c) => c,
        Err(refused) => {
            eprintln!("{refused}");
            return 2;
        }
    };
    let Some(out_dir) = arg_value(args, "--out-dir") else {
        eprintln!("LIVE: NOT RUN (--out-dir is required)\n{}", usage());
        return 2;
    };
    let cap = match arg_value(args, "--max-cost-usd") {
        Some(v) => v
            .trim()
            .parse::<f64>()
            .ok()
            .filter(|c| c.is_finite() && *c > 0.0),
        None => cfg.max_cost_usd,
    };
    let Some(cap) = cap else {
        eprintln!(
            "LIVE: NOT RUN (a positive --max-cost-usd or MODBIT_LIVE_MAX_COST_USD is required; a live run never starts without a spend cap)"
        );
        return 2;
    };
    if !cfg.run.env.iter().any(|(k, _)| k == "OPENAI_API_KEY") {
        eprintln!(
            "LIVE: NOT RUN (retrieval-live drives an OpenAI-compatible endpoint; MODBIT_LIVE_PROVIDER must be openai)"
        );
        return 2;
    }
    let lookup = |k: &str| {
        cfg.run
            .env
            .iter()
            .find(|(n, _)| n == k)
            .map(|(_, v)| v.clone())
            .or_else(|| var(k))
    };
    let chat = match Chat::from_lookup(&lookup, &cfg.run.model) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("LIVE: NOT RUN ({e})");
            return 2;
        }
    };
    let meter = match SpendMeter::new(cap) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("LIVE: NOT RUN ({e})");
            return 2;
        }
    };
    let cases_text = match arg_value(args, "--cases") {
        Some(p) => match std::fs::read_to_string(&p) {
            Ok(t) => t,
            Err(e) => {
                eprintln!("reading {p}: {e}");
                return 1;
            }
        },
        None => DEFAULT_CASES.to_owned(),
    };
    let work_dir = arg_value(args, "--work-dir").map_or_else(
        || std::env::temp_dir().join("modbit-retrieval-live"),
        PathBuf::from,
    );
    let spec = match live_spec(&cases_text, PathBuf::from(out_dir), work_dir, cap) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("the case file is unusable: {e}");
            return 1;
        }
    };
    let rt = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(rt) => rt,
        Err(e) => {
            eprintln!("starting the runtime: {e}");
            return 1;
        }
    };
    match rt.block_on(execute(&spec, &chat, &meter, Knobs::default())) {
        Ok(status) => {
            println!(
                "{}",
                std::fs::read_to_string(spec.out_dir.join("summary.md")).unwrap_or_default()
            );
            if status.complete {
                0
            } else {
                eprintln!("INCOMPLETE: {}", status.missing.join("; "));
                1
            }
        }
        Err(e) => {
            eprintln!("the run failed: {e}");
            1
        }
    }
}

/// Retained transcripts as the harness wrote them (for tests).
#[doc(hidden)]
pub fn retained_for_tests(dir: &Path) -> Result<Vec<Retained>, String> {
    load_transcripts(dir)
}
