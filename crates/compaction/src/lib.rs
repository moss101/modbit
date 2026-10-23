//! `modbit-compaction` — context compaction epochs (docs/19 "Compaction
//! epochs"; REQ-EV-0056 / 0057 / 0058 / 0092 / 0130 / 0268).
//!
//! Canonical owner: context-engine (`docs/12_REPOSITORY_AND_MODULE_LAYOUT.md`,
//! `docs/81_ARCHITECTURE_GUARDRAILS_AND_FORBIDDEN_DUPLICATION.md`).
//! Dependency direction is enforced by `tools/architecture-lint`.
//!
//! Compaction changes what the model sees, never what happened: the canonical
//! event log stays lossless and every epoch records the range it summarised,
//! the facts it preserved verbatim and the handles that recover the rest.
//! An epoch computed against a source range that has since advanced is
//! refused — a stale summary can never install itself.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub mod summary;

/// What a preserved fact is: the labels that must survive compaction
/// (docs/19: instructions, decisions, approvals, open failures, handles).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FactKind {
    /// A user instruction or steering input.
    Instruction,
    /// A plan, a plan revision or a recorded decision.
    Decision,
    /// A protected-effect approval or refusal.
    Approval,
    /// An open failure signature the run still owes work for.
    OpenFailure,
    /// A recoverable handle (object ref, tool call id, path at a revision).
    Handle,
}

/// One fact kept verbatim in the epoch's projection.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PreservedFact {
    /// Kind.
    pub kind: FactKind,
    /// The text kept exactly as it was.
    pub text: String,
    /// Where it came from (`turn <n>`, `tool <name>`, `event <type>`).
    pub origin: String,
}

/// What one compaction produced (docs/19 "Compaction epochs").
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompactionManifest {
    /// Monotonic epoch number (1 = the first compaction of the task).
    pub epoch: u32,
    /// The previous epoch, when any.
    pub previous_epoch: Option<u32>,
    /// The task aggregate generation the compaction was computed under: a
    /// fork, a revert or any later task event moves it and the result is
    /// refused rather than installed on top of a changed history.
    pub task_generation: u64,
    /// The store offset the source range ends at (the head it saw).
    pub source_head_offset: u64,
    /// Entries of the model-visible transcript that were summarised.
    pub source_entries: usize,
    /// The session branch generation the compaction was computed under
    /// (docs/19 "Compaction epochs", M4.2): a fork or a revert moves it and
    /// an asynchronous result computed under the old one is refused.
    #[serde(default)]
    pub branch_generation: u64,
    /// sha256 of the source entries (role, name, text, in order): an
    /// asynchronous result installs only onto the exact prefix it summarised.
    #[serde(default)]
    pub source_digest: String,
    /// Compiler version the projection was written for.
    pub compiler_version: String,
    /// Target token budget for the projection.
    pub target_tokens: u32,
    /// Facts kept verbatim.
    pub preserved: Vec<PreservedFact>,
    /// Handles that recover what was dropped (object refs, tool call ids).
    pub resources: Vec<String>,
    /// The compressed projection the model sees instead of the source range.
    pub projection: String,
    /// Estimated tokens of the projection.
    pub projection_tokens: u32,
    /// sha256 over the fields above, so a client can check what it was given.
    pub manifest_hash: String,
    /// `MODEL` when a summarizer model wrote [`Self::narrative`] and the Core
    /// validated it, `EXTRACTIVE` (or empty, for an epoch from before
    /// REQ-PX-109) when the Core extracted the projection alone.
    #[serde(default)]
    pub summary_source: String,
    /// `endpoint/model` of the summarizer, when a model wrote the narrative.
    #[serde(default)]
    pub summarizer: String,
    /// Why the epoch is extractive when the model path was meant or tried.
    #[serde(default)]
    pub fallback_reason: String,
    /// The validated model narrative, untrusted prompt content: the prompt
    /// compiler places it in a user message, never a system one.
    #[serde(default)]
    pub narrative: String,
    /// Objects holding the exact pre-compaction transcript of this epoch and
    /// of every epoch before it, newest last: `artifact.range` reads them.
    #[serde(default)]
    pub transcript_refs: Vec<String>,
    /// The index object of the newest transcript (entry, role, offset, size,
    /// preview) the model reads to find what to page.
    #[serde(default)]
    pub transcript_index_ref: String,
    /// Files the validated summaries of the epoch chain say were touched, so
    /// a later epoch may cite them.
    #[serde(default)]
    pub files_touched: Vec<String>,
}

/// Why a compaction result was refused (docs/19: stale source, wrong
/// generation, or an epoch that is not the next one).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum RejectedCompaction {
    /// The log advanced past the range the compaction summarised.
    SourceAdvanced {
        /// The head the compaction saw.
        saw: u64,
        /// The head now.
        now: u64,
    },
    /// The task generation changed (fork, revert, or any later task event).
    GenerationChanged {
        /// The generation the compaction saw.
        saw: u64,
        /// The generation now.
        now: u64,
    },
    /// The epoch is not the successor of the installed one.
    NotSuccessor {
        /// Installed epoch.
        installed: u32,
        /// Offered epoch.
        offered: u32,
    },
    /// The session branch generation moved (fork, revert) since the
    /// compaction was computed.
    BranchChanged {
        /// The generation the compaction saw.
        saw: u64,
        /// The generation now.
        now: u64,
    },
    /// The transcript prefix the compaction summarised is not the prefix
    /// any more (another epoch drained it, or the history was rebuilt).
    SourceRewritten {
        /// Digest the compaction saw.
        saw: String,
        /// Digest now.
        now: String,
    },
}

impl RejectedCompaction {
    /// Stable code for the log.
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::SourceAdvanced { .. } => "SOURCE_ADVANCED",
            Self::GenerationChanged { .. } => "GENERATION_CHANGED",
            Self::NotSuccessor { .. } => "NOT_SUCCESSOR",
            Self::BranchChanged { .. } => "BRANCH_CHANGED",
            Self::SourceRewritten { .. } => "SOURCE_REWRITTEN",
        }
    }
}

/// sha256 over the source entries, in order: the identity of the prefix a
/// compaction summarised.
#[must_use]
pub fn source_digest(entries: &[SourceEntry]) -> String {
    let mut h = Sha256::new();
    for e in entries {
        h.update(e.role.as_bytes());
        h.update([0]);
        h.update(e.name.as_bytes());
        h.update([0]);
        h.update(e.text.as_bytes());
        h.update([0xFF]);
    }
    hex::encode(h.finalize())
}

/// A model-visible transcript entry, as compaction sees it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceEntry {
    /// `user` | `assistant` | `tool`.
    pub role: String,
    /// Tool name for a tool result.
    pub name: String,
    /// The text the model saw.
    pub text: String,
    /// A failure signature the entry carried, when any.
    pub failure_signature: Option<String>,
}

/// Estimated tokens (the same deterministic estimator the Context Pack uses):
/// one token per four bytes.
#[must_use]
pub fn estimate_tokens(text: &str) -> u32 {
    u32::try_from(text.len().div_ceil(4)).unwrap_or(u32::MAX)
}

/// The id of [`estimate_tokens_v2`], as the Inspector names the estimator in
/// force.
pub const ESTIMATOR_V2: &str = "tokens-v2";

/// A better token estimate than bytes over four, still deterministic and
/// provider-free: a run of letters or digits costs a token per four of its
/// characters, a punctuation or symbol character about half a token (code and
/// JSON are mostly symbols and short words, where bytes over four undercounts),
/// whitespace is free (it merges into the next token) and every non-ASCII
/// character costs a token (CJK and emoji are nearer one token a character
/// than one per four bytes). The provider's own count, when the run has one,
/// calibrates it ([`Calibration`]).
#[must_use]
pub fn estimate_tokens_v2(text: &str) -> u32 {
    let mut tokens = 0.0f64;
    let mut word = 0u32;
    let flush = |word: &mut u32, tokens: &mut f64| {
        if *word > 0 {
            *tokens += f64::from(word.div_ceil(4));
            *word = 0;
        }
    };
    for c in text.chars() {
        if c.is_ascii_alphanumeric() || c == '_' {
            word += 1;
        } else {
            flush(&mut word, &mut tokens);
            if c.is_ascii_whitespace() {
            } else if c.is_ascii() {
                tokens += 0.5;
            } else {
                tokens += 1.0;
            }
        }
    }
    flush(&mut word, &mut tokens);
    // Whole tokens: never below one for text at all.
    let n = tokens.ceil() as u64;
    u32::try_from(if text.is_empty() { 0 } else { n.max(1) }).unwrap_or(u32::MAX)
}

/// The ratio between what the provider counted and what [`estimate_tokens_v2`]
/// estimated for the same requests, smoothed over the run: a conservative
/// correction that makes the compaction trigger follow the real tokenizer.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Calibration {
    ratio: f64,
    samples: u32,
}

impl Default for Calibration {
    fn default() -> Self {
        Self {
            ratio: 1.0,
            samples: 0,
        }
    }
}

impl Calibration {
    /// Bounds on the correction: a provider that disagrees with the estimate
    /// by more than this is reporting something else (a cached prefix, a
    /// truncated count), not a tokenizer.
    const MIN: f64 = 0.5;
    const MAX: f64 = 3.0;

    /// Fold in one request: what the Core estimated and what the provider
    /// reported as its input tokens. A request with no usage report, or an
    /// estimate of zero, teaches nothing.
    pub fn observe(&mut self, estimated: u32, reported: u64) {
        if estimated == 0 || reported == 0 {
            return;
        }
        let seen = (reported as f64 / f64::from(estimated)).clamp(Self::MIN, Self::MAX);
        self.ratio = if self.samples == 0 {
            seen
        } else {
            0.5 * self.ratio + 0.5 * seen
        };
        self.samples += 1;
    }

    /// The correction to apply to an estimate.
    #[must_use]
    pub fn ratio(&self) -> f64 {
        self.ratio
    }

    /// An estimate corrected by what the provider has reported.
    #[must_use]
    pub fn apply(&self, estimated: u32) -> u32 {
        u32::try_from((f64::from(estimated) * self.ratio).ceil() as u64).unwrap_or(u32::MAX)
    }
}

/// The exact pre-compaction transcript of a range, as two objects: the
/// transcript (one JSON object per line, the text byte for byte as the model
/// saw it) and an index of those lines (entry, role, tool, byte offset and
/// size, a short preview) the model reads first to find what to page with
/// `artifact.range`. Entry numbers are the ones the summarizer cites.
#[must_use]
pub fn transcript_objects(entries: &[SourceEntry]) -> (Vec<u8>, Vec<u8>) {
    let mut transcript: Vec<u8> = Vec::new();
    let mut index: Vec<serde_json::Value> = Vec::new();
    for (i, e) in entries.iter().enumerate() {
        let line = serde_json::json!({"entry": i, "role": e.role, "tool": e.name, "text": e.text})
            .to_string();
        let preview: String = e
            .text
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .chars()
            .take(80)
            .collect();
        index.push(serde_json::json!({
            "entry": i,
            "role": e.role,
            "tool": e.name,
            "offset": transcript.len(),
            "bytes": line.len(),
            "preview": preview,
        }));
        transcript.extend_from_slice(line.as_bytes());
        transcript.push(b'\n');
    }
    (transcript, serde_json::to_vec(&index).unwrap_or_default())
}

/// Record on a manifest where the exact text of the range it replaced is
/// stored, and tell the model how to read it. The pointer line is appended
/// after the projection's own budget selection, so no budget can drop it.
pub fn attach_transcript(manifest: &mut CompactionManifest, transcript_ref: &str, index_ref: &str) {
    manifest.transcript_refs.push(transcript_ref.to_owned());
    manifest.transcript_index_ref = index_ref.to_owned();
    manifest.projection.push_str(&format!(
        "Pointer to the exact earlier text: the {} transcript entries this epoch replaced are stored byte for byte in object {transcript_ref} (one JSON object per line); object {index_ref} lists each entry with its byte offset and size. Read the index with `artifact.range` on {index_ref}, then page the entry you need with `artifact.range` on {transcript_ref} at its offset.\n",
        manifest.source_entries
    ));
    manifest.projection_tokens = estimate_tokens(&manifest.projection);
    manifest.seal();
}

/// Carry a validated model narrative on a manifest, with who wrote it. The
/// summary source becomes `MODEL`.
pub fn attach_narrative(
    manifest: &mut CompactionManifest,
    summarizer: &str,
    narrative: String,
    files: Vec<String>,
) {
    manifest.summary_source = "MODEL".into();
    manifest.summarizer = summarizer.to_owned();
    manifest.fallback_reason.clear();
    manifest.narrative = narrative;
    for f in files {
        if !manifest.files_touched.contains(&f) {
            manifest.files_touched.push(f);
        }
    }
    manifest.seal();
}

/// Record that the epoch is extractive and why the model path was not taken.
pub fn attach_fallback(manifest: &mut CompactionManifest, reason: &str) {
    manifest.summary_source = "EXTRACTIVE".into();
    manifest.fallback_reason = reason.to_owned();
    manifest.seal();
}

impl CompactionManifest {
    /// Recompute [`Self::manifest_hash`] over every field that decides what
    /// the model sees. Fields an older manifest does not have hash as absent,
    /// so an extractive epoch with no pointer hashes as it always did.
    pub fn seal(&mut self) {
        let mut parts: Vec<String> = vec![
            self.epoch.to_string(),
            self.task_generation.to_string(),
            self.source_head_offset.to_string(),
            self.branch_generation.to_string(),
            self.source_digest.clone(),
            self.compiler_version.clone(),
            self.projection.clone(),
        ];
        if !self.narrative.is_empty() {
            parts.push(format!("narrative:{}:{}", self.summarizer, self.narrative));
        }
        if !self.transcript_refs.is_empty() {
            parts.push(format!("transcripts:{}", self.transcript_refs.join(",")));
        }
        let refs: Vec<&str> = parts.iter().map(String::as_str).collect();
        self.manifest_hash = sha(&refs);
    }
}

fn sha(parts: &[&str]) -> String {
    let mut h = Sha256::new();
    for p in parts {
        h.update(p.as_bytes());
        h.update([0]);
    }
    hex::encode(h.finalize())
}

/// Object refs and tool-call handles mentioned in a text, so the dropped
/// material stays recoverable (docs/19: compaction is not deletion).
#[must_use]
pub fn handles_in(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    for word in text.split(|c: char| c.is_whitespace() || matches!(c, '"' | ',' | ')' | '(')) {
        let w = word.trim_matches(|c: char| !c.is_ascii_alphanumeric());
        if w.len() == 64 && w.chars().all(|c| c.is_ascii_hexdigit()) && !out.contains(&w.to_owned())
        {
            out.push(w.to_owned());
        }
    }
    out
}

/// What one compaction is asked to do.
#[derive(Clone, Copy, Debug)]
pub struct CompactionRequest<'a> {
    /// The model-visible entries being summarised, oldest first.
    pub entries: &'a [SourceEntry],
    /// The manifest currently installed, when the task already has an epoch:
    /// its facts and handles are carried into the new one so nothing a
    /// previous epoch preserved is lost by the next compaction.
    pub previous: Option<&'a CompactionManifest>,
    /// The task aggregate generation the compaction is computed under.
    pub task_generation: u64,
    /// The store offset the source range ends at.
    pub source_head_offset: u64,
    /// The session branch generation the compaction is computed under.
    pub branch_generation: u64,
    /// Compiler version the projection is written for.
    pub compiler_version: &'a str,
    /// Target token budget for the projection.
    pub target_tokens: u32,
    /// Decisions and approvals read from typed Core events (a resolved
    /// approval, a recorded or revised plan), oldest first. This is the only
    /// way an `[Approval]` or `[Decision]` fact enters an epoch: the projection
    /// is installed as a system message, so nothing a tool returned may be
    /// promoted into one. Facts of any other kind are ignored.
    pub core_facts: &'a [PreservedFact],
}

/// The facts a request may carry from typed Core events.
const CORE_FACT_KINDS: [FactKind; 2] = [FactKind::Decision, FactKind::Approval];

/// One line of at most `max` characters: a fact is rendered as a single
/// projection line, so an embedded newline cannot start a second, forged one.
fn one_line(text: &str, max: usize) -> String {
    let flat: String = text
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    flat.trim().chars().take(max).collect()
}

/// Longest fact text a Core event may contribute to the projection.
const CORE_FACT_CHARS: usize = 400;

/// Facts carried from earlier epochs, beyond which the oldest are dropped
/// (the canonical log still has them; the manifest chain still names them).
const CARRIED_FACTS: usize = 40;
/// Handles carried from earlier epochs.
const CARRIED_HANDLES: usize = 64;

/// Compact a request's entries into an epoch projection under its budget.
///
/// Instructions, decisions, approvals and open failures are kept verbatim;
/// everything else becomes a counted summary with the handles that recover
/// it. Facts and handles from the previous epoch are carried forward, so a
/// second compaction cannot drop what the first one saved. The result is
/// model-visible material, never a change to the log.
#[must_use]
pub fn compact(request: &CompactionRequest<'_>) -> CompactionManifest {
    let CompactionRequest {
        entries,
        previous,
        task_generation,
        source_head_offset,
        branch_generation,
        compiler_version,
        target_tokens,
        core_facts,
    } = *request;
    let epoch = previous.map_or(1, |m| m.epoch + 1);
    let previous_epoch = previous.map(|m| m.epoch);
    // Carried facts come first: they are the older material, and the budget
    // truncation below keeps the head of the list.
    let mut preserved: Vec<PreservedFact> = previous
        .map(|m| {
            let mut carried: Vec<PreservedFact> = m
                .preserved
                .iter()
                .filter(|f| f.kind != FactKind::Handle)
                // An `[Approval]`/`[Decision]` fact is only carried when a
                // Core event produced it. Manifests written before facts were
                // restricted to Core events took them from tool-result text
                // (origin `epoch N entry I`); those are not carried forward.
                .filter(|f| !CORE_FACT_KINDS.contains(&f.kind) || f.origin.starts_with("event "))
                .cloned()
                .collect();
            if carried.len() > CARRIED_FACTS {
                carried.drain(..carried.len() - CARRIED_FACTS);
            }
            carried
        })
        .unwrap_or_default();
    let mut resources: Vec<String> = previous.map(|m| m.resources.clone()).unwrap_or_default();
    let push = |preserved: &mut Vec<PreservedFact>, f: PreservedFact| {
        if !preserved
            .iter()
            .any(|p| p.kind == f.kind && p.text == f.text)
        {
            preserved.push(f);
        }
    };
    let mut tool_calls = 0usize;
    let mut failures = Vec::new();
    for (i, e) in entries.iter().enumerate() {
        let origin = format!("epoch {epoch} entry {i}");
        match e.role.as_str() {
            "user" => push(
                &mut preserved,
                PreservedFact {
                    kind: FactKind::Instruction,
                    text: e.text.clone(),
                    origin,
                },
            ),
            "assistant" => {}
            _ => {
                // A tool result is data. It is counted, its handles are kept
                // and a failure signature the Core attached is carried, but
                // its text is never promoted into an Approval or Decision.
                tool_calls += 1;
                if let Some(sig) = &e.failure_signature {
                    failures.push(sig.clone());
                }
            }
        }
        for h in handles_in(&e.text) {
            if !resources.contains(&h) {
                resources.push(h);
            }
        }
    }
    for f in core_facts {
        if CORE_FACT_KINDS.contains(&f.kind) {
            let origin = if f.origin.starts_with("event ") {
                f.origin.clone()
            } else {
                format!("event {}", f.origin)
            };
            push(
                &mut preserved,
                PreservedFact {
                    kind: f.kind,
                    text: one_line(&f.text, CORE_FACT_CHARS),
                    origin,
                },
            );
        }
    }
    failures.dedup();
    for f in &failures {
        push(
            &mut preserved,
            PreservedFact {
                kind: FactKind::OpenFailure,
                text: f.clone(),
                origin: "failure signature".into(),
            },
        );
    }
    if resources.len() > CARRIED_HANDLES {
        resources.drain(..resources.len() - CARRIED_HANDLES);
    }
    for r in resources.iter().skip(resources.len().saturating_sub(20)) {
        push(
            &mut preserved,
            PreservedFact {
                kind: FactKind::Handle,
                text: r.clone(),
                origin: "object ref".into(),
            },
        );
    }
    let carried_note = previous_epoch.map_or_else(String::new, |p| {
        format!(" Facts and handles from epoch {p} are carried below.")
    });
    let header = format!(
        "Compaction epoch {epoch}: {} earlier transcript entries ({tool_calls} tool result(s)) are summarised here. The canonical log keeps them in full; read any of them back with `artifact.range` on the refs below.{carried_note}\n",
        entries.len()
    );
    // Under budget pressure the newest facts win: candidates are taken
    // newest first (handles, which only recover dropped material, after every
    // other kind) until the budget is spent, and the chosen ones are then
    // written in their original order.
    let lines: Vec<String> = preserved
        .iter()
        .map(|f| format!("- [{:?}] {}\n", f.kind, f.text))
        .collect();
    let mut order: Vec<usize> = (0..preserved.len()).rev().collect();
    order.sort_by_key(|&i| preserved[i].kind == FactKind::Handle);
    let mut chosen = vec![false; preserved.len()];
    let mut used = header.len();
    let mut omitted = false;
    for i in order {
        if used.div_ceil(4) + lines[i].len().div_ceil(4) > target_tokens as usize {
            omitted = true;
            break;
        }
        used += lines[i].len();
        chosen[i] = true;
    }
    let mut projection = header;
    for (line, keep) in lines.iter().zip(&chosen) {
        if *keep {
            projection.push_str(line);
        }
    }
    if omitted {
        projection.push_str(
            "- (further preserved facts omitted for the epoch budget; the log has them)\n",
        );
    }
    let projection_tokens = estimate_tokens(&projection);
    let source_digest = source_digest(entries);
    let manifest_hash = sha(&[
        &epoch.to_string(),
        &task_generation.to_string(),
        &source_head_offset.to_string(),
        &branch_generation.to_string(),
        &source_digest,
        compiler_version,
        &projection,
    ]);
    CompactionManifest {
        epoch,
        previous_epoch,
        task_generation,
        source_head_offset,
        source_entries: entries.len(),
        branch_generation,
        source_digest,
        compiler_version: compiler_version.to_owned(),
        target_tokens,
        preserved,
        resources,
        projection,
        projection_tokens,
        manifest_hash,
        summary_source: "EXTRACTIVE".into(),
        summarizer: String::new(),
        fallback_reason: String::new(),
        narrative: String::new(),
        transcript_refs: previous
            .map(|m| m.transcript_refs.clone())
            .unwrap_or_default(),
        transcript_index_ref: String::new(),
        files_touched: previous
            .map(|m| m.files_touched.clone())
            .unwrap_or_default(),
    }
}

/// How faithful a projection is to the critical facts of its source
/// (REQ-EV-0130 "critical-fact compaction corpus meets fidelity threshold").
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Fidelity {
    /// Critical facts the source carried (instructions, decisions, approvals,
    /// open failures) plus the handles it named.
    pub critical: usize,
    /// Of those, how many the manifest keeps.
    pub preserved: usize,
    /// Of those, how many reached the projection text under its budget.
    pub projected: usize,
}

impl Fidelity {
    /// Preserved share of the critical facts, 1.0 when nothing was lost.
    #[must_use]
    pub fn preserved_ratio(&self) -> f32 {
        if self.critical == 0 {
            return 1.0;
        }
        self.preserved as f32 / self.critical as f32
    }

    /// Share of the critical facts the model actually saw in the projection.
    #[must_use]
    pub fn projected_ratio(&self) -> f32 {
        if self.critical == 0 {
            return 1.0;
        }
        self.projected as f32 / self.critical as f32
    }
}

/// Measure a manifest against the entries it summarised and the Core facts it
/// was given: what a critical fact is here is decided by the source, not by the
/// manifest, so a compactor that silently dropped a labelled fact scores below
/// 1.0.
///
/// Limitation: this shares `compact`'s labelling rule, so it measures whether
/// the labelled facts survived compaction — not whether the label itself found
/// everything a reader would call critical.
#[must_use]
pub fn fidelity(
    entries: &[SourceEntry],
    core_facts: &[PreservedFact],
    manifest: &CompactionManifest,
) -> Fidelity {
    let mut wanted: Vec<String> = Vec::new();
    for e in entries {
        if e.role == "user" {
            wanted.push(e.text.clone());
        }
        if let Some(sig) = &e.failure_signature {
            wanted.push(sig.clone());
        }
        for h in handles_in(&e.text) {
            wanted.push(h);
        }
    }
    for f in core_facts {
        if CORE_FACT_KINDS.contains(&f.kind) {
            wanted.push(one_line(&f.text, CORE_FACT_CHARS));
        }
    }
    wanted.sort();
    wanted.dedup();
    let mut preserved = 0;
    let mut projected = 0;
    for w in &wanted {
        if manifest.preserved.iter().any(|f| &f.text == w) {
            preserved += 1;
        }
        if manifest.projection.contains(w.as_str()) {
            projected += 1;
        }
    }
    Fidelity {
        critical: wanted.len(),
        preserved,
        projected,
    }
}

/// Whether a compaction result may install now (docs/19: async results are
/// accepted only while their source is still current).
///
/// # Errors
/// The source advanced, the generation changed, or the epoch is not the
/// successor of the installed one.
pub fn accept(
    manifest: &CompactionManifest,
    installed_epoch: Option<u32>,
    current_generation: u64,
    current_head_offset: u64,
) -> Result<(), RejectedCompaction> {
    if manifest.task_generation != current_generation {
        return Err(RejectedCompaction::GenerationChanged {
            saw: manifest.task_generation,
            now: current_generation,
        });
    }
    if manifest.source_head_offset != current_head_offset {
        return Err(RejectedCompaction::SourceAdvanced {
            saw: manifest.source_head_offset,
            now: current_head_offset,
        });
    }
    successor(manifest, installed_epoch)
}

fn successor(
    manifest: &CompactionManifest,
    installed_epoch: Option<u32>,
) -> Result<(), RejectedCompaction> {
    let expected = installed_epoch.unwrap_or(0) + 1;
    if manifest.epoch != expected {
        return Err(RejectedCompaction::NotSuccessor {
            installed: installed_epoch.unwrap_or(0),
            offered: manifest.epoch,
        });
    }
    Ok(())
}

/// Whether an asynchronous compaction result may install now (docs/19:
/// accepted only if the source branch/generation is still current;
/// docs/54 fault 10). The log has moved on since the worker started — that
/// is expected — so the checks are the branch generation the worker
/// captured, the exact prefix it summarised, and the epoch order.
///
/// # Errors
/// The branch generation moved, the prefix was rewritten (another epoch
/// drained it, or the history was rebuilt), or the epoch is not the
/// successor of the installed one.
pub fn accept_async(
    manifest: &CompactionManifest,
    installed_epoch: Option<u32>,
    current_branch_generation: u64,
    current_source_digest: &str,
) -> Result<(), RejectedCompaction> {
    if manifest.branch_generation != current_branch_generation {
        return Err(RejectedCompaction::BranchChanged {
            saw: manifest.branch_generation,
            now: current_branch_generation,
        });
    }
    successor(manifest, installed_epoch)?;
    if manifest.source_digest != current_source_digest {
        return Err(RejectedCompaction::SourceRewritten {
            saw: manifest.source_digest.clone(),
            now: current_source_digest.to_owned(),
        });
    }
    Ok(())
}
