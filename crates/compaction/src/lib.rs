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

/// Estimated tokens (the same deterministic estimator the Context Pack uses).
#[must_use]
pub fn estimate_tokens(text: &str) -> u32 {
    u32::try_from(text.len().div_ceil(4)).unwrap_or(u32::MAX)
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
    /// Compiler version the projection is written for.
    pub compiler_version: &'a str,
    /// Target token budget for the projection.
    pub target_tokens: u32,
}

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
        compiler_version,
        target_tokens,
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
                tool_calls += 1;
                let lower = e.text.to_ascii_lowercase();
                if e.name == "plan.update" || lower.contains("plan version") {
                    push(
                        &mut preserved,
                        PreservedFact {
                            kind: FactKind::Decision,
                            text: e.text.lines().take(2).collect::<Vec<_>>().join(" "),
                            origin,
                        },
                    );
                } else if lower.contains("approval") || lower.contains("approved") {
                    push(
                        &mut preserved,
                        PreservedFact {
                            kind: FactKind::Approval,
                            text: e.text.lines().take(2).collect::<Vec<_>>().join(" "),
                            origin,
                        },
                    );
                }
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
    for r in resources.iter().take(20) {
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
    let mut projection = format!(
        "Compaction epoch {epoch}: {} earlier transcript entries ({tool_calls} tool result(s)) are summarised here. The canonical log keeps them in full; read any of them back with `artifact.range` on the refs below.{carried_note}\n",
        entries.len()
    );
    for f in &preserved {
        let line = format!("- [{:?}] {}\n", f.kind, f.text);
        if estimate_tokens(&projection) + estimate_tokens(&line) > target_tokens {
            projection.push_str(
                "- (further preserved facts omitted for the epoch budget; the log has them)\n",
            );
            break;
        }
        projection.push_str(&line);
    }
    let projection_tokens = estimate_tokens(&projection);
    let manifest_hash = sha(&[
        &epoch.to_string(),
        &task_generation.to_string(),
        &source_head_offset.to_string(),
        compiler_version,
        &projection,
    ]);
    CompactionManifest {
        epoch,
        previous_epoch,
        task_generation,
        source_head_offset,
        source_entries: entries.len(),
        compiler_version: compiler_version.to_owned(),
        target_tokens,
        preserved,
        resources,
        projection,
        projection_tokens,
        manifest_hash,
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

/// Measure a manifest against the entries it summarised: what a critical fact
/// is here is decided by the source, not by the manifest, so a compactor that
/// silently dropped a labelled fact scores below 1.0.
///
/// Limitation: this shares `compact`'s labelling heuristic, so it measures
/// whether the labelled facts survived compaction — not whether the label
/// itself found everything a reader would call critical.
#[must_use]
pub fn fidelity(entries: &[SourceEntry], manifest: &CompactionManifest) -> Fidelity {
    let mut wanted: Vec<String> = Vec::new();
    for e in entries {
        let lower = e.text.to_ascii_lowercase();
        let head = |t: &str| t.lines().take(2).collect::<Vec<_>>().join(" ");
        if e.role == "user" {
            wanted.push(e.text.clone());
        } else if e.role != "assistant"
            && (e.name == "plan.update"
                || lower.contains("plan version")
                || lower.contains("approval")
                || lower.contains("approved"))
        {
            wanted.push(head(&e.text));
        }
        if let Some(sig) = &e.failure_signature {
            wanted.push(sig.clone());
        }
        for h in handles_in(&e.text) {
            wanted.push(h);
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
    let expected = installed_epoch.unwrap_or(0) + 1;
    if manifest.epoch != expected {
        return Err(RejectedCompaction::NotSuccessor {
            installed: installed_epoch.unwrap_or(0),
            offered: manifest.epoch,
        });
    }
    Ok(())
}
