//! The live retrieval evaluation's data and scoring (PX-137, QUAL-PX-137):
//! the pinned case file, the retained transcript of one agent on one case,
//! the oracle, the per-profile tables with their intervals, the report digest
//! and the offline `--rescore` that reproduces all of it from the retained
//! transcripts alone.
//!
//! The oracle never asks a model. A case's labels are repository paths; the
//! agent's final message is a JSON object `{"files": [...]}` (when it does
//! not manage that, the path-like tokens of its text are read instead), and
//! the answer is correct when every labelled path is named and no more than
//! two paths beyond the labels are (a list of every file in the repository
//! must not score). Scoring reads only the transcript, so rescoring a
//! retained run needs no network and no checkout.

use std::collections::BTreeMap;
use std::path::Path;

use modbit_bench_context_economics::projection_trial::{Proportion, wilson};
use modbit_bench_context_economics::spend::Price;
use modbit_bench_context_economics::{bootstrap_ci95, mean};
use modbit_retrieval::bench::IndexTimes;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

/// The graph row.
pub const ROW: &str = "px137";
/// The harness version stamped in every report.
pub const HARNESS: &str = "retrieval-live-v1";
/// Case-file schema.
pub const CASES_SCHEMA: &str = "modbit-retrieval-live-cases-v1";
/// Transcript schema.
pub const TRANSCRIPT_SCHEMA: &str = "modbit-retrieval-live-transcript-v1";
/// Fewest cases a live run may score (QUAL-PX-137).
pub const MIN_CASES_LIVE: usize = 50;
/// The corpus size the index times are measured at (QUAL-PX-137).
pub const INDEX_BYTES_LIVE: u64 = 100 * 1024 * 1024;
/// Paths the answer may name beyond the labels and still be correct.
pub const EXTRA_PATHS_ALLOWED: usize = 2;

/// The profiles, in the order the report prints them.
pub const PROFILES: [&str; 2] = ["baseline", "treatment"];

/// One labelled file.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Label {
    /// Path relative to the repository root.
    pub path: String,
    /// A string the file must contain at the pinned commit.
    pub evidence: String,
}

/// One hand-labelled question.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LiveCase {
    /// Id.
    pub id: String,
    /// `behavior` | `symbol_lookup` | `multi_hop`.
    pub kind: String,
    /// The question the agent is asked.
    pub question: String,
    /// The files that answer it; all are required.
    pub labels: Vec<Label>,
}

/// The committed case file.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CaseFile {
    /// Schema id.
    pub schema: String,
    /// Corpus name.
    pub name: String,
    /// Where the repository is cloned from.
    pub repo_url: String,
    /// The pinned commit (40 hex digits).
    pub commit: String,
    /// Provenance of the labels.
    pub description: String,
    /// The cases.
    pub cases: Vec<LiveCase>,
}

impl CaseFile {
    /// Parse and shape-check a case file.
    ///
    /// # Errors
    /// Malformed JSON, a wrong schema, a commit that is not 40 hex digits,
    /// duplicate ids or a case without labels.
    pub fn parse(text: &str) -> Result<Self, String> {
        let f: Self = serde_json::from_str(text).map_err(|e| format!("case file: {e}"))?;
        if f.schema != CASES_SCHEMA {
            return Err(format!(
                "case file schema `{}` is not {CASES_SCHEMA}",
                f.schema
            ));
        }
        if f.commit.len() != 40 || !f.commit.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err("case file commit must be a full 40-digit sha".into());
        }
        let mut ids: Vec<&str> = f.cases.iter().map(|c| c.id.as_str()).collect();
        ids.sort_unstable();
        let n = ids.len();
        ids.dedup();
        if ids.len() != n {
            return Err("case ids are not unique".into());
        }
        if let Some(c) = f
            .cases
            .iter()
            .find(|c| c.labels.is_empty() || c.question.trim().is_empty())
        {
            return Err(format!("case `{}` has no question or no labels", c.id));
        }
        Ok(f)
    }
}

/// sha256 of bytes, hex.
#[must_use]
pub fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

/// One model call of a case.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ModelCall {
    /// 1-based round.
    pub round: u32,
    /// Input tokens the gateway reported.
    pub input_tokens: u64,
    /// Output tokens.
    pub output_tokens: u64,
    /// Cached part of the input.
    pub cached_input_tokens: u64,
    /// The reply carried a usage block.
    pub usage_known: bool,
    /// Latency of the call.
    pub latency_ms: u64,
    /// Why the model stopped.
    pub finish_reason: String,
    /// Priced from the usage.
    pub cost_usd: f64,
    /// The gateway's request id.
    pub request_id: String,
}

/// One tool call of a case.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ToolRecord {
    /// The round of the model reply that asked for it.
    pub round: u32,
    /// The call id the model gave.
    pub call_id: String,
    /// The tool.
    pub tool: String,
    /// The arguments as the model wrote them.
    pub arguments: String,
    /// Whether the HARNESS issued the call instead of the model. A live
    /// transcript must say `false` for every call: retrieval is offered,
    /// never forced.
    pub forced: bool,
    /// The tool refused or failed.
    pub error: bool,
    /// The result was cut.
    pub truncated: bool,
    /// Characters handed back to the model.
    pub result_chars: usize,
    /// Wall time of the tool.
    pub ms: u64,
}

/// One agent on one case under one profile: everything scoring needs.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Transcript {
    /// Schema id.
    pub schema: String,
    /// Row.
    pub row: String,
    /// `baseline` | `treatment`.
    pub profile: String,
    /// Case id.
    pub case_id: String,
    /// Case kind.
    pub kind: String,
    /// The question.
    pub question: String,
    /// The labelled paths (the oracle's input).
    pub labels: Vec<String>,
    /// The model every call named.
    pub model: String,
    /// The conversation, OpenAI chat format: system, user, assistant (with
    /// tool_calls), tool.
    pub messages: Vec<Value>,
    /// Model calls, in order.
    pub model_calls: Vec<ModelCall>,
    /// Tool calls, in order.
    pub tool_calls: Vec<ToolRecord>,
    /// The final assistant text.
    pub final_text: String,
    /// `rounds` or `token_budget` when a cap ended the case.
    pub capped: Option<String>,
    /// Wall time of the whole case.
    pub wall_ms: u64,
}

fn normalize_path(raw: &str) -> Option<String> {
    let mut s = raw
        .trim()
        .trim_matches(|c| matches!(c, '`' | '"' | '\'' | ',' | ';'))
        .replace('\\', "/");
    while let Some(rest) = s.strip_prefix("./") {
        s = rest.to_owned();
    }
    if let Some((head, tail)) = s.rsplit_once(':')
        && !tail.is_empty()
        && tail.chars().all(|c| c.is_ascii_digit() || c == '-')
    {
        s = head.to_owned();
    }
    (!s.is_empty()).then_some(s)
}

/// The paths an answer names, normalised, in order, without repeats.
#[must_use]
pub fn answered_paths(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut push = |p: Option<String>| {
        if let Some(p) = p
            && !out.contains(&p)
        {
            out.push(p);
        }
    };
    if let (Some(a), Some(b)) = (text.find('{'), text.rfind('}'))
        && a < b
        && let Ok(v) = serde_json::from_str::<Value>(&text[a..=b])
        && let Some(files) = v.get("files").and_then(Value::as_array)
    {
        for f in files {
            push(f.as_str().and_then(normalize_path));
        }
        return out;
    }
    for tok in text.split(|c: char| {
        c.is_whitespace() || matches!(c, '(' | ')' | '[' | ']' | '{' | '}' | '<' | '>' | '*')
    }) {
        let tok = tok.trim_end_matches(['.', ',', ';', ':']);
        let Some(p) = normalize_path(tok) else {
            continue;
        };
        let last = p.rsplit('/').next().unwrap_or_default();
        let ext_ok = last.rsplit_once('.').is_some_and(|(stem, ext)| {
            !stem.is_empty()
                && (1..=8).contains(&ext.len())
                && ext.chars().all(|c| c.is_ascii_alphanumeric())
        });
        if ext_ok && (p.contains('/') || last.contains('.')) {
            push(Some(p));
        }
    }
    out
}

/// What the oracle says about one answer.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Verdict {
    /// Labelled paths named.
    pub found: usize,
    /// Labelled paths.
    pub labels: usize,
    /// Paths named that are not labels.
    pub extra: usize,
    /// Every label named and at most [`EXTRA_PATHS_ALLOWED`] extra.
    pub correct: bool,
}

/// Score an answer against labels.
#[must_use]
pub fn judge(labels: &[String], answered: &[String]) -> Verdict {
    let found = labels.iter().filter(|l| answered.contains(l)).count();
    let extra = answered.iter().filter(|a| !labels.contains(a)).count();
    Verdict {
        found,
        labels: labels.len(),
        extra,
        correct: found == labels.len() && extra <= EXTRA_PATHS_ALLOWED,
    }
}

/// The measurements of one transcript.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CaseMetrics {
    /// Case.
    pub case_id: String,
    /// Case kind.
    pub kind: String,
    /// Profile.
    pub profile: String,
    /// Paths the final answer named.
    pub answered: Vec<String>,
    /// Labelled paths named.
    pub found: usize,
    /// Labelled paths.
    pub labels: usize,
    /// Paths named beyond the labels.
    pub extra: usize,
    /// The oracle's verdict.
    pub correct: bool,
    /// Input tokens over all model calls.
    pub input_tokens: u64,
    /// Output tokens.
    pub output_tokens: u64,
    /// Model calls.
    pub model_calls: u64,
    /// Tool calls.
    pub tool_calls: u64,
    /// Tool calls by tool.
    pub tools: BTreeMap<String, u64>,
    /// Whether the model called `retrieve` at least once.
    pub retrieve_called: bool,
    /// Wall time of the case.
    pub wall_ms: u64,
    /// Every model call carried a usage block.
    pub usage_known: bool,
    /// Priced cost.
    pub cost_usd: f64,
    /// Cap that ended the case, if one did.
    pub capped: Option<String>,
}

/// Measure a transcript.
#[must_use]
pub fn metrics(t: &Transcript) -> CaseMetrics {
    let answered = answered_paths(&t.final_text);
    let v = judge(&t.labels, &answered);
    let mut tools: BTreeMap<String, u64> = BTreeMap::new();
    for c in &t.tool_calls {
        *tools.entry(c.tool.clone()).or_default() += 1;
    }
    CaseMetrics {
        case_id: t.case_id.clone(),
        kind: t.kind.clone(),
        profile: t.profile.clone(),
        answered,
        found: v.found,
        labels: v.labels,
        extra: v.extra,
        correct: v.correct,
        input_tokens: t.model_calls.iter().map(|m| m.input_tokens).sum(),
        output_tokens: t.model_calls.iter().map(|m| m.output_tokens).sum(),
        model_calls: t.model_calls.len() as u64,
        tool_calls: t.tool_calls.len() as u64,
        retrieve_called: tools.contains_key("retrieve"),
        tools,
        wall_ms: t.wall_ms,
        usage_known: t.model_calls.iter().all(|m| m.usage_known),
        cost_usd: t.model_calls.iter().map(|m| m.cost_usd).sum(),
        capped: t.capped.clone(),
    }
}

/// Check that a transcript is what a live run produces: no call issued by
/// the harness, every tool call traceable to an assistant message, the
/// baseline never holding `retrieve`.
///
/// # Errors
/// The first inconsistency.
pub fn validate_transcript(t: &Transcript) -> Result<(), String> {
    let at = format!("{}/{}", t.profile, t.case_id);
    if t.schema != TRANSCRIPT_SCHEMA || t.row != ROW {
        return Err(format!("{at}: not a {ROW} transcript"));
    }
    if !PROFILES.contains(&t.profile.as_str()) {
        return Err(format!("{at}: unknown profile `{}`", t.profile));
    }
    if let Some(c) = t.tool_calls.iter().find(|c| c.forced) {
        return Err(format!(
            "{at}: the harness forced a `{}` call; retrieval must be offered, never forced (the no-forced-retrieval rule)",
            c.tool
        ));
    }
    if t.profile == "baseline" && t.tool_calls.iter().any(|c| c.tool == "retrieve") {
        return Err(format!("{at}: the baseline profile has no retrieve tool"));
    }
    // Every recorded call must be one an assistant message asked for.
    let mut asked: Vec<(String, String)> = Vec::new();
    let mut assistant_messages = 0usize;
    let mut tool_messages = 0usize;
    for m in &t.messages {
        match m["role"].as_str() {
            Some("assistant") => {
                assistant_messages += 1;
                for c in m["tool_calls"].as_array().into_iter().flatten() {
                    asked.push((
                        c["id"].as_str().unwrap_or_default().to_owned(),
                        c["function"]["name"]
                            .as_str()
                            .unwrap_or_default()
                            .to_owned(),
                    ));
                }
            }
            Some("tool") => tool_messages += 1,
            _ => {}
        }
    }
    for c in &t.tool_calls {
        if !asked.contains(&(c.call_id.clone(), c.tool.clone())) {
            return Err(format!(
                "{at}: tool call `{}` ({}) was not asked for by any assistant message",
                c.tool, c.call_id
            ));
        }
    }
    if asked.len() != t.tool_calls.len() || tool_messages != t.tool_calls.len() {
        return Err(format!("{at}: tool calls and messages disagree"));
    }
    // The final no-tools call of a rounds-capped case has no assistant message
    // of its own beyond the model calls, so the counts must match exactly.
    if assistant_messages != t.model_calls.len() {
        return Err(format!(
            "{at}: {assistant_messages} assistant messages but {} model calls",
            t.model_calls.len()
        ));
    }
    Ok(())
}

/// Fixed numbers a run was configured with, retained in the report.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Limits {
    /// Most tool rounds per case.
    pub max_rounds: u32,
    /// Tool output cut, characters.
    pub tool_output_chars: usize,
    /// Per-case token budget (input plus output over all calls).
    pub case_token_budget: u64,
    /// Output tokens allowed per call.
    pub max_output_tokens: u32,
}

/// The pinned repository as the run saw it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RepoInfo {
    /// Clone URL.
    pub url: String,
    /// Pinned commit.
    pub commit: String,
    /// `git rev-parse HEAD` of the checkout.
    pub head: String,
    /// Files in the checkout (tracked).
    pub files: usize,
    /// Bytes of those files.
    pub bytes: u64,
    /// Labelled files that exist and contain their evidence.
    pub labels_verified: usize,
}

/// Cold and incremental index times at the target corpus size.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct IndexMeasure {
    /// What the corpus is made of, in words.
    pub corpus: String,
    /// Bytes of the real checkout in it.
    pub checkout_bytes: u64,
    /// Files of the real checkout in it.
    pub checkout_files: usize,
    /// Bytes of deterministic synthetic padding added to reach the target.
    pub padding_bytes: u64,
    /// Padding files.
    pub padding_files: usize,
    /// The size the run was asked to reach.
    pub target_bytes: u64,
    /// Files the exact index holds.
    pub indexed_files: usize,
    /// Searchable bytes the exact index holds.
    pub indexed_bytes: u64,
    /// Cold-start time per index.
    pub cold: IndexTimes,
    /// Refreshing one file across every index.
    pub incremental_ms: f64,
    /// The file refreshed.
    pub incremental_path: String,
}

/// What a report is assembled around; stored in the report so a rescore can
/// rebuild it from the transcripts.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ReportMeta {
    /// Model.
    pub model: String,
    /// Its list price.
    pub price: Price,
    /// Repository.
    pub repo: RepoInfo,
    /// Case-file name.
    pub cases_name: String,
    /// sha256 of the case file's bytes.
    pub cases_sha256: String,
    /// Case ids, in case-file order.
    pub case_order: Vec<String>,
    /// Limits.
    pub limits: Limits,
    /// Index times (not derivable from transcripts; carried).
    pub index: Option<IndexMeasure>,
}

/// A mean with a bootstrap interval.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MeanCi {
    /// Mean per case.
    pub mean: f64,
    /// Deterministic bootstrap 95% interval of the mean; `None` under two cases.
    pub ci95: Option<(f64, f64)>,
    /// Sum over cases.
    pub total: f64,
}

fn mean_ci(v: &[f64]) -> MeanCi {
    let ci = bootstrap_ci95(v);
    MeanCi {
        mean: mean(v),
        ci95: ci.0.is_finite().then_some(ci),
        total: v.iter().sum(),
    }
}

/// One profile's column.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ProfileReport {
    /// `baseline` | `treatment`.
    pub profile: String,
    /// Cases scored (paired cases only).
    pub cases: usize,
    /// Cases whose answer the oracle accepted, with a Wilson interval.
    pub accuracy: Option<Proportion>,
    /// Accuracy by case kind.
    pub accuracy_by_kind: BTreeMap<String, Proportion>,
    /// Labelled paths named / labelled paths, pooled.
    pub label_recall: Option<Proportion>,
    /// Input tokens per case.
    pub input_tokens: MeanCi,
    /// Output tokens per case.
    pub output_tokens: MeanCi,
    /// Tool calls per case.
    pub tool_calls: MeanCi,
    /// Model calls per case.
    pub model_calls: MeanCi,
    /// Wall time per case, milliseconds.
    pub wall_ms: MeanCi,
    /// Priced cost over the cases.
    pub cost_usd: f64,
    /// Tool calls by tool, summed.
    pub tools: BTreeMap<String, u64>,
    /// Cases ended by a cap (scored on what they answered).
    pub capped_cases: usize,
    /// Cases in which the model called `retrieve` (treatment).
    pub cases_using_retrieve: usize,
    /// Every model call reported usage.
    pub usage_known: bool,
}

/// A paired comparison of the treatment against the baseline.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Paired {
    /// What was compared.
    pub measure: String,
    /// `higher` | `lower`: which direction is better.
    pub better: String,
    /// Cases where the treatment was better.
    pub wins: usize,
    /// Worse.
    pub losses: usize,
    /// Equal.
    pub ties: usize,
    /// Mean of treatment minus baseline.
    pub mean_delta: f64,
    /// Bootstrap 95% interval of the mean delta.
    pub ci95: Option<(f64, f64)>,
    /// The interval excludes zero.
    pub significant: bool,
}

/// The report.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LiveReport {
    /// Harness version.
    pub harness: String,
    /// What it was assembled around.
    pub meta: ReportMeta,
    /// Cases in the case file.
    pub case_count: usize,
    /// Cases with a scored transcript under both profiles.
    pub paired_count: usize,
    /// The baseline column (real ripgrep + read_file).
    pub baseline: ProfileReport,
    /// The treatment column (the same tools + `retrieve`, offered not forced).
    pub treatment: ProfileReport,
    /// Treatment against baseline, per measure.
    pub paired: Vec<Paired>,
    /// sha256 over the scoring inputs and outputs of every retained transcript.
    pub report_digest: String,
    /// What the numbers do and do not establish.
    pub method: String,
}

fn profile_report(profile: &str, rows: &[&CaseMetrics]) -> ProfileReport {
    let col = |f: &dyn Fn(&CaseMetrics) -> f64| -> Vec<f64> { rows.iter().map(|m| f(m)).collect() };
    let mut by_kind: BTreeMap<String, (usize, usize)> = BTreeMap::new();
    let mut tools: BTreeMap<String, u64> = BTreeMap::new();
    for m in rows {
        let e = by_kind.entry(m.kind.clone()).or_default();
        e.1 += 1;
        e.0 += usize::from(m.correct);
        for (t, n) in &m.tools {
            *tools.entry(t.clone()).or_default() += n;
        }
    }
    let labels: usize = rows.iter().map(|m| m.labels).sum();
    ProfileReport {
        profile: profile.to_owned(),
        cases: rows.len(),
        accuracy: (!rows.is_empty())
            .then(|| wilson(rows.iter().filter(|m| m.correct).count(), rows.len())),
        accuracy_by_kind: by_kind
            .into_iter()
            .map(|(k, (c, n))| (k, wilson(c, n)))
            .collect(),
        label_recall: (labels > 0).then(|| wilson(rows.iter().map(|m| m.found).sum(), labels)),
        input_tokens: mean_ci(&col(&|m| m.input_tokens as f64)),
        output_tokens: mean_ci(&col(&|m| m.output_tokens as f64)),
        tool_calls: mean_ci(&col(&|m| m.tool_calls as f64)),
        model_calls: mean_ci(&col(&|m| m.model_calls as f64)),
        wall_ms: mean_ci(&col(&|m| m.wall_ms as f64)),
        cost_usd: rows.iter().map(|m| m.cost_usd).sum(),
        tools,
        capped_cases: rows.iter().filter(|m| m.capped.is_some()).count(),
        cases_using_retrieve: rows.iter().filter(|m| m.retrieve_called).count(),
        usage_known: rows.iter().all(|m| m.usage_known),
    }
}

fn paired(
    measure: &str,
    higher_better: bool,
    pairs: &[(&CaseMetrics, &CaseMetrics)],
    f: &dyn Fn(&CaseMetrics) -> f64,
) -> Paired {
    let deltas: Vec<f64> = pairs.iter().map(|(b, t)| f(t) - f(b)).collect();
    let better = |d: f64| if higher_better { d > 0.0 } else { d < 0.0 };
    let ci = bootstrap_ci95(&deltas);
    Paired {
        measure: measure.to_owned(),
        better: if higher_better { "higher" } else { "lower" }.to_owned(),
        wins: deltas.iter().filter(|d| better(**d)).count(),
        losses: deltas.iter().filter(|d| **d != 0.0 && !better(**d)).count(),
        ties: deltas.iter().filter(|d| **d == 0.0).count(),
        mean_delta: mean(&deltas),
        ci95: ci.0.is_finite().then_some(ci),
        significant: ci.0.is_finite() && (ci.0 > 0.0 || ci.1 < 0.0),
    }
}

/// A retained transcript with the sha256 of the bytes it was read from.
pub struct Retained {
    /// The transcript.
    pub transcript: Transcript,
    /// sha256 of its file.
    pub file_sha256: String,
}

/// Build the report from the retained transcripts. Pure: a rescore calls
/// this with the transcripts it read back, and must obtain the same report.
#[must_use]
pub fn assemble(meta: &ReportMeta, retained: &[Retained]) -> LiveReport {
    let by_key: BTreeMap<(&str, &str), &Retained> = retained
        .iter()
        .map(|r| {
            (
                (r.transcript.case_id.as_str(), r.transcript.profile.as_str()),
                r,
            )
        })
        .collect();
    let all: BTreeMap<(&str, &str), CaseMetrics> = by_key
        .iter()
        .map(|(k, r)| (*k, metrics(&r.transcript)))
        .collect();
    let mut pairs: Vec<(&CaseMetrics, &CaseMetrics)> = Vec::new();
    for id in &meta.case_order {
        if let (Some(b), Some(t)) = (
            all.get(&(id.as_str(), "baseline")),
            all.get(&(id.as_str(), "treatment")),
        ) {
            pairs.push((b, t));
        }
    }
    let base: Vec<&CaseMetrics> = pairs.iter().map(|p| p.0).collect();
    let treat: Vec<&CaseMetrics> = pairs.iter().map(|p| p.1).collect();
    let cmp = vec![
        paired("accuracy", true, &pairs, &|m| {
            f64::from(u8::from(m.correct))
        }),
        paired("input_tokens", false, &pairs, &|m| m.input_tokens as f64),
        paired("output_tokens", false, &pairs, &|m| m.output_tokens as f64),
        paired("tool_calls", false, &pairs, &|m| m.tool_calls as f64),
        paired("wall_ms", false, &pairs, &|m| m.wall_ms as f64),
    ];
    let mut h = Sha256::new();
    h.update(
        format!(
            "{ROW}|{}|{}|{}|{}\n",
            meta.model,
            meta.repo.commit,
            meta.cases_sha256,
            meta.case_order.len()
        )
        .as_bytes(),
    );
    for ((case, profile), r) in &by_key {
        let m = &all[&(*case, *profile)];
        h.update(
            format!(
                "{profile}|{case}|{}|{}|{}|{}|{}|{}|{}|{:?}|{}\n",
                r.file_sha256,
                m.answered.join(","),
                m.correct,
                m.input_tokens,
                m.output_tokens,
                m.model_calls,
                m.tool_calls,
                m.tools,
                m.wall_ms
            )
            .as_bytes(),
        );
    }
    LiveReport {
        harness: HARNESS.to_owned(),
        meta: meta.clone(),
        case_count: meta.case_order.len(),
        paired_count: pairs.len(),
        baseline: profile_report("baseline", &base),
        treatment: profile_report("treatment", &treat),
        paired: cmp,
        report_digest: hex::encode(h.finalize()),
        method: "Same model, same questions, same pinned checkout for both profiles. BASELINE = the external baseline surface: a real ripgrep process behind `search` plus `read_file`. TREATMENT = those same two tools plus `retrieve` (the Modbit retrieval planner, structural profile, in-process); it is OFFERED, never forced: the transcripts record forced=false on every call and the validator rejects any other value, and the report counts the cases in which the model chose to call it. Correctness is the oracle's, never a model's: every labelled file must be named in the final JSON answer, with at most two extra paths. Accuracy intervals are Wilson; token, tool-call and time means carry deterministic bootstrap intervals; the paired rows are treatment minus baseline per case. Decoding is at temperature 0 but a hosted model is not bit-reproducible, so one run is one sample; the rescore reproduces the report from the retained transcripts, not from the model. Wall times include the gateway's latency and are not comparable across machines. Index times are for the corpus named in the index block, which says how much of it is synthetic padding.".to_owned(),
    }
}

/// Read every retained transcript under `dir/transcripts/<profile>/`.
///
/// # Errors
/// An unreadable or unparsable transcript.
pub fn load_transcripts(dir: &Path) -> Result<Vec<Retained>, String> {
    let mut out = Vec::new();
    for profile in PROFILES {
        let sub = dir.join("transcripts").join(profile);
        let Ok(rd) = std::fs::read_dir(&sub) else {
            continue;
        };
        let mut files: Vec<_> = rd.flatten().map(|e| e.path()).collect();
        files.sort();
        for p in files {
            if p.extension().and_then(|e| e.to_str()) != Some("json")
                || p.file_name()
                    .is_some_and(|n| n.to_string_lossy().ends_with(".error.json"))
            {
                continue;
            }
            let bytes = std::fs::read(&p).map_err(|e| format!("reading {}: {e}", p.display()))?;
            let transcript: Transcript = serde_json::from_slice(&bytes)
                .map_err(|e| format!("{} is not a transcript: {e}", p.display()))?;
            let stem = p
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default();
            if stem != transcript.case_id || transcript.profile != profile {
                return Err(format!(
                    "{} names case `{}` profile `{}`: a transcript must live at transcripts/<profile>/<case>.json",
                    p.display(),
                    transcript.case_id,
                    transcript.profile
                ));
            }
            out.push(Retained {
                file_sha256: sha256_hex(&bytes),
                transcript,
            });
        }
    }
    Ok(out)
}

/// The outcome of `--rescore`.
#[derive(Debug)]
pub struct Rescored {
    /// The report rebuilt from the transcripts.
    pub report: LiveReport,
    /// The digest `result.json` carried.
    pub stored_digest: String,
    /// Whether the rebuilt report equals the stored one exactly.
    pub reproduced: bool,
}

/// Recompute the report from the retained transcripts of `dir`, with no
/// network and no checkout, and compare it with the stored `result.json`.
///
/// # Errors
/// Missing or malformed files, an invalid transcript, or a report that does
/// not reproduce (the message names the first difference).
pub fn rescore(dir: &Path) -> Result<Rescored, String> {
    let text = std::fs::read_to_string(dir.join("result.json"))
        .map_err(|e| format!("reading result.json: {e}"))?;
    let stored: Value = serde_json::from_str(&text).map_err(|e| format!("result.json: {e}"))?;
    let stored_report = stored
        .get("report")
        .ok_or("result.json has no report")?
        .clone();
    let meta: ReportMeta = serde_json::from_value(
        stored_report
            .get("meta")
            .ok_or("the stored report has no meta")?
            .clone(),
    )
    .map_err(|e| format!("report meta: {e}"))?;
    let retained = load_transcripts(dir)?;
    for r in &retained {
        validate_transcript(&r.transcript)?;
        if !meta.case_order.contains(&r.transcript.case_id) {
            return Err(format!(
                "transcript for unknown case `{}`",
                r.transcript.case_id
            ));
        }
    }
    let report = assemble(&meta, &retained);
    let stored_digest = stored_report["report_digest"]
        .as_str()
        .unwrap_or_default()
        .to_owned();
    // Through the same text round trip the stored report took, so a float
    // printed and parsed once compares equal to itself.
    let rebuilt: Value =
        serde_json::from_str(&serde_json::to_string(&report).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
    let reproduced = rebuilt == stored_report;
    if !reproduced {
        let why = if report.report_digest != stored_digest {
            format!(
                "the digest differs: stored {stored_digest}, recomputed {}",
                report.report_digest
            )
        } else {
            let mut first = String::new();
            if let (Some(a), Some(b)) = (rebuilt.as_object(), stored_report.as_object()) {
                for (k, v) in a {
                    if b.get(k) != Some(v) {
                        first = format!("`{k}`: stored {:?}, recomputed {v}", b.get(k));
                        break;
                    }
                }
            }
            format!(
                "the digest matches but a stored table differs from the recomputed one ({first})"
            )
        };
        return Err(format!(
            "the report does not reproduce from the retained transcripts: {why}"
        ));
    }
    Ok(Rescored {
        report,
        stored_digest,
        reproduced,
    })
}

fn ci_text(ci: Option<(f64, f64)>, digits: usize) -> String {
    ci.map_or_else(
        || "no interval (under two cases)".to_owned(),
        |(lo, hi)| format!("[{lo:.digits$}, {hi:.digits$}]"),
    )
}

fn prop_text(p: Option<Proportion>) -> String {
    p.map_or_else(
        || "n/a".to_owned(),
        |p| {
            format!(
                "{}/{} = {:.3} (Wilson 95% [{:.3}, {:.3}])",
                p.k, p.n, p.p, p.lo, p.hi
            )
        },
    )
}

/// The human summary.
#[must_use]
pub fn summary_md(r: &LiveReport, missing: &[String], spend_line: &str) -> String {
    let mut s = String::new();
    s.push_str(&format!(
        "# PX-137 live retrieval evaluation\n\nModel `{}`; repository {} @ `{}`; {} of {} cases scored under both profiles.\n\n",
        r.meta.model, r.meta.repo.url, r.meta.repo.commit, r.paired_count, r.case_count
    ));
    s.push_str(&format!("{spend_line}\n\n"));
    if missing.is_empty() {
        s.push_str("Status: COMPLETE.\n\n");
    } else {
        s.push_str("Status: INCOMPLETE. Missing:\n");
        for m in missing {
            s.push_str(&format!("- {m}\n"));
        }
        s.push('\n');
    }
    s.push_str("| profile | accuracy | input tokens / case | output tokens / case | tool calls / case | wall ms / case |\n|---|---|---|---|---|---|\n");
    for p in [&r.baseline, &r.treatment] {
        let role = if p.profile == "baseline" {
            "baseline (rg + read_file)"
        } else {
            "treatment (+ retrieve)"
        };
        s.push_str(&format!(
            "| {role} | {} | {:.0} {} | {:.0} {} | {:.2} {} | {:.0} {} |\n",
            prop_text(p.accuracy),
            p.input_tokens.mean,
            ci_text(p.input_tokens.ci95, 0),
            p.output_tokens.mean,
            ci_text(p.output_tokens.ci95, 0),
            p.tool_calls.mean,
            ci_text(p.tool_calls.ci95, 2),
            p.wall_ms.mean,
            ci_text(p.wall_ms.ci95, 0),
        ));
    }
    s.push_str(&format!(
        "\nTreatment called `retrieve` in {} of {} cases (offered, never forced). Cases ended by a cap: baseline {}, treatment {}.\n\n",
        r.treatment.cases_using_retrieve, r.treatment.cases, r.baseline.capped_cases, r.treatment.capped_cases
    ));
    s.push_str("Treatment minus baseline, per case:\n\n| measure | better | wins | losses | ties | mean delta | 95% bootstrap |\n|---|---|---|---|---|---|---|\n");
    for p in &r.paired {
        s.push_str(&format!(
            "| {} | {} | {} | {} | {} | {:.3} | {} |\n",
            p.measure,
            p.better,
            p.wins,
            p.losses,
            p.ties,
            p.mean_delta,
            ci_text(p.ci95, 3)
        ));
    }
    if let Some(ix) = &r.meta.index {
        s.push_str(&format!(
            "\nIndex times at {:.0} MB: {}. Cold start {:.0} ms (exact {:.0}, BM25 {:.0}, symbols {:.0}, semantic {:.0}, graph {:.0}); incremental refresh of one file {:.1} ms.\n",
            ix.indexed_bytes as f64 / 1_048_576.0,
            ix.corpus,
            ix.cold.total_ms,
            ix.cold.exact_ms,
            ix.cold.lexical_ms,
            ix.cold.symbols_ms,
            ix.cold.semantic_ms,
            ix.cold.graph_ms,
            ix.incremental_ms
        ));
    }
    s.push_str(&format!(
        "\nReport digest `{}` (reproduce with `retrieval-live --rescore <dir>`).\n\n{}\n",
        r.report_digest, r.method
    ));
    s
}
