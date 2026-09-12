//! The Skill Evolution Lab (docs/26, M5.7; REQ-EV-0196..0204, 0237, 0247):
//! an offline, EXPERIMENT-gated loop behind the Skill Registry — sealed
//! execution traces, a knowledge store of patterns, a maintainer that
//! consolidates traces into patterns, a proposer that turns a pattern into
//! one bounded candidate change, and a promotion transaction that only a
//! PROMOTE qualification under a signing key can drive.
//!
//! Three artifacts, three stores, never one: raw experience (`traces/`,
//! immutable once sealed), evolution knowledge (`patterns/` with a compact
//! `index.json`), and candidate skills (`candidates/`, with their
//! qualification and impact record). None of it is Engineering Memory and
//! none of it is recovery truth (REQ-EV-0208): the lab writes under its own
//! directory only, the production agent reads the registry head only, and
//! the Core's recovery never opens the lab.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::Digest;

use crate::{SkillError, SkillPackage, load_package};

/// Why the lab refused.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, thiserror::Error)]
#[serde(tag = "code", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum LabError {
    /// A sealed trace was written again with other content.
    #[error("trace {trace_id} is sealed; corrections reference it, they do not overwrite it")]
    Sealed {
        /// The trace.
        trace_id: String,
    },
    /// A candidate is not atomic or widens authority.
    #[error("candidate refused: {reason}")]
    CandidateRefused {
        /// Why.
        reason: String,
    },
    /// Promotion was asked for without a PROMOTE qualification.
    #[error("promotion refused: {reason}")]
    PromotionRefused {
        /// Why.
        reason: String,
    },
    /// A file could not be read or written.
    #[error("io at `{path}`: {detail}")]
    Io {
        /// Path.
        path: String,
        /// Detail.
        detail: String,
    },
    /// A package under the lab or the registry did not load.
    #[error("package: {0}")]
    Package(SkillError),
}

fn io(path: &Path, e: &std::io::Error) -> LabError {
    LabError::Io {
        path: path.display().to_string(),
        detail: e.to_string(),
    }
}

fn sha_hex(bytes: &[u8]) -> String {
    hex::encode(sha2::Sha256::digest(bytes))
}

/// Canonical JSON bytes of a value (sorted keys), the basis of every digest.
fn canonical<T: Serialize>(v: &T) -> Vec<u8> {
    let value = serde_json::to_value(v).unwrap_or_default();
    serde_json::to_vec(&value).unwrap_or_default()
}

/// The cost a run had.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TraceCost {
    /// Input tokens.
    pub input_tokens: u64,
    /// Output tokens.
    pub output_tokens: u64,
    /// Tool calls.
    pub tool_calls: u32,
    /// Wall time.
    pub wall_ms: u64,
}

/// An `EvolutionTrace` (docs/26 "Storage contracts"): one run of one task,
/// immutable once sealed.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvolutionTrace {
    /// Benchmark / task id.
    pub task: String,
    /// Task class (`bug-repair`, `multi-file`, `comprehension`).
    pub task_class: String,
    /// Repository revision.
    pub repository_revision: String,
    /// Model / provider configuration.
    pub model_config: String,
    /// Instruction manifest hash (skills and rules the run saw).
    pub instruction_manifest_hash: String,
    /// Tool-capability snapshot hash.
    pub tool_capability_snapshot_hash: String,
    /// Environment revision.
    pub environment_revision: String,
    /// Evidence references (event offsets, artifacts).
    pub evidence_refs: Vec<String>,
    /// Normalized observations of what the run did (`read-before-edit`,
    /// `tests-before-complete`, …): what the maintainer consolidates.
    pub observations: Vec<String>,
    /// `VERIFIED` | `FAILED`.
    pub outcome: String,
    /// Verification result summary.
    pub verification_result: String,
    /// Cost.
    pub cost: TraceCost,
    /// `REDACTED` | `RAW`.
    pub redaction_status: String,
}

/// A sealed trace: the trace and the digest that names it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SealedTrace {
    /// Content digest, the trace id.
    pub trace_id: String,
    /// The trace.
    pub trace: EvolutionTrace,
    /// When it was sealed.
    pub sealed_at_ms: i64,
}

/// A correction of a sealed trace: a new record that references it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TraceCorrection {
    /// The sealed trace corrected.
    pub trace_id: String,
    /// What is corrected.
    pub note: String,
    /// The corrected fields, as JSON.
    pub corrected: serde_json::Value,
    /// When.
    pub recorded_at_ms: i64,
}

/// The lab's directory layout.
#[derive(Clone, Debug)]
pub struct Lab {
    root: PathBuf,
}

impl Lab {
    /// Open (creating) a lab under `root`.
    ///
    /// # Errors
    /// The directories cannot be created.
    pub fn open(root: &Path) -> Result<Self, LabError> {
        for sub in [
            "traces",
            "corrections",
            "patterns",
            "candidates",
            "impact",
            "versions",
        ] {
            let p = root.join(sub);
            std::fs::create_dir_all(&p).map_err(|e| io(&p, &e))?;
        }
        Ok(Self {
            root: root.to_path_buf(),
        })
    }

    /// The root.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Seal a trace: its digest names it; writing the same trace again is a
    /// no-op; writing other content under that id is refused.
    ///
    /// # Errors
    /// The trace id is sealed with other content, or the write fails.
    pub fn seal(&self, trace: &EvolutionTrace, sealed_at_ms: i64) -> Result<SealedTrace, LabError> {
        let trace_id = sha_hex(&canonical(trace));
        let path = self.root.join("traces").join(format!("{trace_id}.json"));
        let sealed = SealedTrace {
            trace_id: trace_id.clone(),
            trace: trace.clone(),
            sealed_at_ms,
        };
        if path.exists() {
            let existing: SealedTrace = serde_json::from_slice(
                &std::fs::read(&path).map_err(|e| io(&path, &e))?,
            )
            .map_err(|e| LabError::Io {
                path: path.display().to_string(),
                detail: e.to_string(),
            })?;
            if existing.trace != *trace {
                return Err(LabError::Sealed { trace_id });
            }
            return Ok(existing);
        }
        std::fs::write(
            &path,
            serde_json::to_vec_pretty(&sealed).unwrap_or_default(),
        )
        .map_err(|e| io(&path, &e))?;
        Ok(sealed)
    }

    /// Try to mutate a sealed trace in place: always refused (the digest
    /// is the name; a change is another trace or a correction).
    ///
    /// # Errors
    /// Always `Sealed` when the id exists; `Io` when it does not.
    pub fn try_mutate(&self, trace_id: &str, changed: &EvolutionTrace) -> Result<(), LabError> {
        let path = self.root.join("traces").join(format!("{trace_id}.json"));
        if !path.exists() {
            return Err(LabError::Io {
                path: path.display().to_string(),
                detail: "no such trace".into(),
            });
        }
        if sha_hex(&canonical(changed)) == trace_id {
            return Ok(());
        }
        Err(LabError::Sealed {
            trace_id: trace_id.to_owned(),
        })
    }

    /// Record a correction that references a sealed trace.
    ///
    /// # Errors
    /// The trace does not exist, or the write fails.
    pub fn correct(&self, correction: &TraceCorrection) -> Result<String, LabError> {
        let trace = self
            .root
            .join("traces")
            .join(format!("{}.json", correction.trace_id));
        if !trace.exists() {
            return Err(LabError::Io {
                path: trace.display().to_string(),
                detail: "no such trace to correct".into(),
            });
        }
        let id = sha_hex(&canonical(correction));
        let path = self.root.join("corrections").join(format!("{id}.json"));
        std::fs::write(
            &path,
            serde_json::to_vec_pretty(correction).unwrap_or_default(),
        )
        .map_err(|e| io(&path, &e))?;
        Ok(id)
    }

    /// Every sealed trace.
    ///
    /// # Errors
    /// The directory cannot be read.
    pub fn traces(&self) -> Result<Vec<SealedTrace>, LabError> {
        let dir = self.root.join("traces");
        let mut out = Vec::new();
        for e in std::fs::read_dir(&dir).map_err(|e| io(&dir, &e))?.flatten() {
            if let Ok(bytes) = std::fs::read(e.path())
                && let Ok(t) = serde_json::from_slice::<SealedTrace>(&bytes)
            {
                out.push(t);
            }
        }
        out.sort_by(|a, b| a.trace_id.cmp(&b.trace_id));
        Ok(out)
    }

    /// Every correction.
    ///
    /// # Errors
    /// The directory cannot be read.
    pub fn corrections(&self) -> Result<Vec<TraceCorrection>, LabError> {
        let dir = self.root.join("corrections");
        let mut out = Vec::new();
        for e in std::fs::read_dir(&dir).map_err(|e| io(&dir, &e))?.flatten() {
            if let Ok(bytes) = std::fs::read(e.path())
                && let Ok(t) = serde_json::from_slice::<TraceCorrection>(&bytes)
            {
                out.push(t);
            }
        }
        Ok(out)
    }
}

/// An `EvolutionPattern`: evaluation knowledge, never runtime authority.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvolutionPattern {
    /// Id (digest of claim + scope + maintainer version).
    pub pattern_id: String,
    /// The claim.
    pub claim: String,
    /// Traces supporting it.
    pub supporting: Vec<String>,
    /// Traces contradicting it.
    pub contradicting: Vec<String>,
    /// Scope (task class).
    pub scope: String,
    /// Confidence in basis points: supporting / (supporting + contradicting).
    pub confidence_bp: u32,
    /// Created at maintainer revision.
    pub created_revision: u32,
    /// Updated at maintainer revision.
    pub updated_revision: u32,
    /// Maintainer version.
    pub maintainer_version: String,
    /// Tags.
    pub tags: Vec<String>,
    /// The pattern this one supersedes, if any.
    pub supersedes: Option<String>,
}

/// One entry of the compact index the proposer starts from.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct IndexEntry {
    /// Pattern id.
    pub pattern_id: String,
    /// Claim.
    pub claim: String,
    /// Scope.
    pub scope: String,
    /// Confidence.
    pub confidence_bp: u32,
    /// Bytes of the full pattern record.
    pub bytes: u64,
}

/// The version of the maintainer's consolidation rule.
pub const MAINTAINER_VERSION: &str = "maintainer-1";

/// The Wiki Maintainer (REQ-EV-0198): consolidates sealed traces into
/// patterns — one claim per (task class, observation) — recording success
/// and failure evidence side by side with a confidence, never overwriting:
/// a later revision supersedes and keeps both.
pub struct Maintainer;

impl Maintainer {
    /// Consolidate `traces` into patterns at `revision`, superseding the
    /// patterns already in the store for the same claim and scope.
    ///
    /// # Errors
    /// The store cannot be written.
    pub fn consolidate(
        lab: &Lab,
        traces: &[SealedTrace],
        revision: u32,
    ) -> Result<Vec<EvolutionPattern>, LabError> {
        let existing = KnowledgeStore::patterns(lab)?;
        let mut groups: BTreeMap<(String, String), (Vec<String>, Vec<String>)> = BTreeMap::new();
        for t in traces {
            for obs in &t.trace.observations {
                let key = (t.trace.task_class.clone(), obs.clone());
                let entry = groups.entry(key).or_default();
                if t.trace.outcome == "VERIFIED" {
                    entry.0.push(t.trace_id.clone());
                } else {
                    entry.1.push(t.trace_id.clone());
                }
            }
        }
        let mut out = Vec::new();
        for ((scope, obs), (supporting, contradicting)) in groups {
            let claim = format!("in {scope} tasks, `{obs}` accompanies verified completion");
            let total = supporting.len() + contradicting.len();
            let confidence_bp = (supporting.len() * 10_000)
                .checked_div(total)
                .and_then(|v| u32::try_from(v).ok())
                .unwrap_or(0);
            let prior = existing
                .iter()
                .filter(|p| p.claim == claim && p.scope == scope)
                .max_by_key(|p| p.updated_revision)
                .cloned();
            let pattern_id =
                sha_hex(format!("{claim}|{scope}|{MAINTAINER_VERSION}|{revision}").as_bytes());
            let pattern = EvolutionPattern {
                pattern_id,
                claim,
                supporting,
                contradicting,
                scope,
                confidence_bp,
                created_revision: prior.as_ref().map_or(revision, |p| p.created_revision),
                updated_revision: revision,
                maintainer_version: MAINTAINER_VERSION.into(),
                tags: vec![obs],
                supersedes: prior.map(|p| p.pattern_id),
            };
            KnowledgeStore::put(lab, &pattern)?;
            out.push(pattern);
        }
        KnowledgeStore::reindex(lab)?;
        Ok(out)
    }
}

/// The knowledge store: patterns and their compact index.
pub struct KnowledgeStore;

impl KnowledgeStore {
    fn put(lab: &Lab, pattern: &EvolutionPattern) -> Result<(), LabError> {
        let path = lab
            .root
            .join("patterns")
            .join(format!("{}.json", pattern.pattern_id));
        std::fs::write(
            &path,
            serde_json::to_vec_pretty(pattern).unwrap_or_default(),
        )
        .map_err(|e| io(&path, &e))
    }

    /// Every pattern.
    ///
    /// # Errors
    /// The directory cannot be read.
    pub fn patterns(lab: &Lab) -> Result<Vec<EvolutionPattern>, LabError> {
        let dir = lab.root.join("patterns");
        let mut out = Vec::new();
        for e in std::fs::read_dir(&dir).map_err(|e| io(&dir, &e))?.flatten() {
            if e.path().file_name().is_some_and(|n| n == "index.json") {
                continue;
            }
            if let Ok(bytes) = std::fs::read(e.path())
                && let Ok(p) = serde_json::from_slice::<EvolutionPattern>(&bytes)
            {
                out.push(p);
            }
        }
        out.sort_by(|a, b| a.pattern_id.cmp(&b.pattern_id));
        Ok(out)
    }

    /// Rebuild the compact index (the current head of every claim).
    ///
    /// # Errors
    /// The index cannot be written.
    pub fn reindex(lab: &Lab) -> Result<Vec<IndexEntry>, LabError> {
        let patterns = Self::patterns(lab)?;
        let superseded: std::collections::BTreeSet<String> = patterns
            .iter()
            .filter_map(|p| p.supersedes.clone())
            .collect();
        let mut index: Vec<IndexEntry> = patterns
            .iter()
            .filter(|p| !superseded.contains(&p.pattern_id))
            .map(|p| IndexEntry {
                pattern_id: p.pattern_id.clone(),
                claim: p.claim.clone(),
                scope: p.scope.clone(),
                confidence_bp: p.confidence_bp,
                bytes: serde_json::to_vec(p).map(|v| v.len() as u64).unwrap_or(0),
            })
            .collect();
        index.sort_by(|a, b| {
            b.confidence_bp
                .cmp(&a.confidence_bp)
                .then(a.pattern_id.cmp(&b.pattern_id))
        });
        let path = lab.root.join("patterns").join("index.json");
        std::fs::write(&path, serde_json::to_vec_pretty(&index).unwrap_or_default())
            .map_err(|e| io(&path, &e))?;
        Ok(index)
    }

    /// The compact index.
    ///
    /// # Errors
    /// The index cannot be read.
    pub fn index(lab: &Lab) -> Result<Vec<IndexEntry>, LabError> {
        let path = lab.root.join("patterns").join("index.json");
        if !path.exists() {
            return Ok(vec![]);
        }
        serde_json::from_slice(&std::fs::read(&path).map_err(|e| io(&path, &e))?).map_err(|e| {
            LabError::Io {
                path: path.display().to_string(),
                detail: e.to_string(),
            }
        })
    }

    /// Hydrate patterns by id under a token budget (bytes / 4): only what
    /// is hydrated counts; what would exceed the budget is refused and
    /// named.
    ///
    /// # Errors
    /// A pattern file cannot be read.
    pub fn hydrate(lab: &Lab, ids: &[String], token_budget: u64) -> Result<Hydrated, LabError> {
        let mut out = Hydrated::default();
        for id in ids {
            let path = lab.root.join("patterns").join(format!("{id}.json"));
            let bytes = std::fs::read(&path).map_err(|e| io(&path, &e))?;
            let tokens = (bytes.len() as u64).div_ceil(4);
            if out.tokens_used + tokens > token_budget {
                out.refused.push(id.clone());
                continue;
            }
            if let Ok(p) = serde_json::from_slice::<EvolutionPattern>(&bytes) {
                out.tokens_used += tokens;
                out.patterns.push(p);
            }
        }
        Ok(out)
    }
}

/// What a hydration brought in, and what it left out.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Hydrated {
    /// Patterns hydrated.
    pub patterns: Vec<EvolutionPattern>,
    /// Tokens counted (bytes / 4 of what was hydrated).
    pub tokens_used: u64,
    /// Ids refused for want of budget.
    pub refused: Vec<String>,
}

/// An atomic patch: one file of the package, replaced whole, bound to the
/// content it replaces.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AtomicPatch {
    /// Package-relative file (`SKILL.md` or `procedures/<name>.js`).
    pub file: String,
    /// Hash of the content replaced (empty for a new file).
    pub before_hash: String,
    /// The new content.
    pub after: String,
}

/// A `SkillCandidate` (docs/26).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkillCandidate {
    /// Candidate id (digest).
    pub candidate_id: String,
    /// Base skill name.
    pub base_name: String,
    /// Base version.
    pub base_version: String,
    /// Base content hash.
    pub base_content_hash: String,
    /// The one change.
    pub patch: AtomicPatch,
    /// PURPOSE.
    pub purpose: String,
    /// Motivating patterns.
    pub motivating_patterns: Vec<String>,
    /// Motivating traces.
    pub motivating_traces: Vec<String>,
    /// Expected behaviour change.
    pub expected_behavior_change: String,
    /// Required tools the candidate declares.
    pub required_tools: Vec<String>,
    /// Capability ceiling the candidate declares.
    pub capability_ceiling: Vec<String>,
    /// Target task classes.
    pub target_task_classes: Vec<String>,
    /// Proposer model / configuration.
    pub proposer_config: String,
    /// When.
    pub created_at_ms: i64,
    /// The version the candidate proposes.
    pub proposed_version: String,
}

/// The proposer's model: given the base, the objective and the hydrated
/// evidence, one instruction line to add. The template proposer is the
/// lab's deterministic experiment stand-in; a model-backed proposer
/// implements the same trait through the provider gateway.
pub trait ProposerModel {
    /// The proposer's configuration string for provenance.
    fn config(&self) -> String;
    /// One bounded instruction line derived from the evidence, or `None`.
    fn propose_line(&self, objective: &str, evidence: &Hydrated) -> Option<(String, String)>;
}

/// A deterministic proposer: the highest-confidence hydrated pattern
/// becomes one instruction line; its purpose names the claim.
pub struct TemplateProposer;

impl ProposerModel for TemplateProposer {
    fn config(&self) -> String {
        "template-proposer-1".into()
    }

    fn propose_line(&self, objective: &str, evidence: &Hydrated) -> Option<(String, String)> {
        let best = evidence
            .patterns
            .iter()
            .max_by_key(|p| (p.confidence_bp, p.pattern_id.clone()))?;
        let tag = best.tags.first().cloned().unwrap_or_default();
        Some((
            format!("- Always: {tag} ({objective})."),
            format!(
                "PURPOSE: {} — {} of {} traces support it",
                best.claim,
                best.supporting.len(),
                best.supporting.len() + best.contradicting.len()
            ),
        ))
    }
}

/// The Skill Proposer (REQ-EV-0199): one bounded behaviour change per
/// candidate, with its evidence named.
pub struct Proposer;

impl Proposer {
    /// Propose a candidate for `base` from the hydrated evidence.
    ///
    /// # Errors
    /// The evidence yields nothing, or the candidate cannot be written.
    pub fn propose(
        lab: &Lab,
        base: &SkillPackage,
        objective: &str,
        evidence: &Hydrated,
        model: &dyn ProposerModel,
        now_ms: i64,
    ) -> Result<SkillCandidate, LabError> {
        let Some((line, purpose)) = model.propose_line(objective, evidence) else {
            return Err(LabError::CandidateRefused {
                reason: "no evidence to propose from".into(),
            });
        };
        let skill_md_path = base.root.join("SKILL.md");
        let before = std::fs::read(&skill_md_path).map_err(|e| io(&skill_md_path, &e))?;
        let before_text = String::from_utf8_lossy(&before).into_owned();
        let after = format!("{}\n{line}\n", before_text.trim_end());
        let proposed_version = bump_patch(&base.manifest.version);
        let after = after.replacen(
            &format!("version: {}", base.manifest.version),
            &format!("version: {proposed_version}"),
            1,
        );
        let patch = AtomicPatch {
            file: "SKILL.md".into(),
            before_hash: sha_hex(&before),
            after,
        };
        let mut candidate = SkillCandidate {
            candidate_id: String::new(),
            base_name: base.manifest.name.clone(),
            base_version: base.manifest.version.clone(),
            base_content_hash: base.content_hash.clone(),
            patch,
            purpose,
            motivating_patterns: evidence
                .patterns
                .iter()
                .map(|p| p.pattern_id.clone())
                .collect(),
            motivating_traces: evidence
                .patterns
                .iter()
                .flat_map(|p| p.supporting.iter().chain(p.contradicting.iter()).cloned())
                .collect(),
            expected_behavior_change: format!("adds one instruction: {line}"),
            required_tools: base.manifest.required_tools.clone(),
            capability_ceiling: base.manifest.capability_ceiling.clone(),
            target_task_classes: evidence.patterns.iter().map(|p| p.scope.clone()).collect(),
            proposer_config: model.config(),
            created_at_ms: now_ms,
            proposed_version,
        };
        candidate.motivating_traces.sort();
        candidate.motivating_traces.dedup();
        candidate.target_task_classes.sort();
        candidate.target_task_classes.dedup();
        candidate.candidate_id = sha_hex(&canonical(&candidate));
        let path = lab
            .root
            .join("candidates")
            .join(format!("{}.json", candidate.candidate_id));
        std::fs::write(
            &path,
            serde_json::to_vec_pretty(&candidate).unwrap_or_default(),
        )
        .map_err(|e| io(&path, &e))?;
        Ok(candidate)
    }

    /// Validate a candidate against its base (WSK-E2E-003/006): one file of
    /// the package, bound to the base content, no unrelated file, no
    /// widening of tools or capability ceiling, no prohibited executable
    /// behaviour in a procedure.
    ///
    /// # Errors
    /// `CandidateRefused` with the reason.
    pub fn validate(base: &SkillPackage, candidate: &SkillCandidate) -> Result<(), LabError> {
        let refuse = |reason: String| Err(LabError::CandidateRefused { reason });
        if candidate.base_content_hash != base.content_hash {
            return refuse(format!(
                "candidate is for base {} but the base is {}",
                candidate.base_content_hash, base.content_hash
            ));
        }
        let file = candidate.patch.file.as_str();
        if file != "SKILL.md" && !(file.starts_with("procedures/") && file.ends_with(".js")) {
            return refuse(format!(
                "`{file}` is not part of a skill's behaviour (SKILL.md or a procedure)"
            ));
        }
        if file.contains("..") || file.starts_with('/') {
            return refuse(format!("`{file}` leaves the package"));
        }
        let current = std::fs::read(base.root.join(file)).unwrap_or_default();
        let current_hash = if current.is_empty() {
            String::new()
        } else {
            sha_hex(&current)
        };
        if candidate.patch.before_hash != current_hash {
            return refuse("the patch is bound to content the base no longer has".into());
        }
        for t in &candidate.required_tools {
            if !base.manifest.required_tools.contains(t) {
                return refuse(format!("candidate widens required_tools with `{t}`"));
            }
        }
        for c in &candidate.capability_ceiling {
            if !base.manifest.capability_ceiling.contains(c) {
                return refuse(format!("candidate widens capability_ceiling with `{c}`"));
            }
        }
        if file == "SKILL.md" {
            let (m, _) =
                crate::parse_skill_md(&candidate.patch.after).map_err(LabError::Package)?;
            if m.name != base.manifest.name {
                return refuse("candidate renames the skill".into());
            }
            for t in &m.required_tools {
                if !base.manifest.required_tools.contains(t) {
                    return refuse(format!(
                        "candidate manifest widens required_tools with `{t}`"
                    ));
                }
            }
            for c in &m.capability_ceiling {
                if !base.manifest.capability_ceiling.contains(c) {
                    return refuse(format!(
                        "candidate manifest widens capability_ceiling with `{c}`"
                    ));
                }
            }
        }
        if let Some(bad) = prohibited(&candidate.patch.after) {
            return refuse(format!("prohibited executable behaviour: `{bad}`"));
        }
        Ok(())
    }
}

/// Static security scan (docs/26 promotion gate 3): what a skill's text
/// or procedure may not carry.
const PROHIBITED: &[&str] = &[
    "require(",
    "import(",
    "process.env",
    "eval(",
    "new Function(",
    "child_process",
    "fetch(",
    "XMLHttpRequest",
    "WebSocket(",
    "__modbit_invoke",
];

fn prohibited(text: &str) -> Option<&'static str> {
    PROHIBITED.iter().copied().find(|p| text.contains(p))
}

fn bump_patch(version: &str) -> String {
    let mut parts: Vec<u64> = version.split('.').filter_map(|p| p.parse().ok()).collect();
    while parts.len() < 3 {
        parts.push(0);
    }
    parts[2] += 1;
    parts
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(".")
}

/// One paired trial of the qualification suite (docs/26 `SkillQualification`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct QualificationTrial {
    /// Task.
    pub task: String,
    /// Task class.
    pub task_class: String,
    /// Model family.
    pub model: String,
    /// Repeat / seed.
    pub repeat: u32,
    /// `no_skill` | `current` | `candidate`.
    pub arm: String,
    /// Verified completion.
    pub verified: bool,
    /// Safety failures.
    pub safety_failures: u32,
    /// Input tokens.
    pub input_tokens: u64,
    /// Output tokens.
    pub output_tokens: u64,
    /// Tool calls.
    pub tool_calls: u32,
    /// Wall time.
    pub wall_ms: u64,
    /// Cost in minor units.
    pub cost_minor: u64,
}

/// The qualification record (docs/26 `SkillQualification`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SkillQualification {
    /// Benchmark version.
    pub benchmark_version: String,
    /// Baseline (current) skill content hash, or empty for none.
    pub baseline_skill: String,
    /// Candidate id.
    pub candidate: String,
    /// Model families.
    pub model_matrix: Vec<String>,
    /// Repetitions.
    pub repetitions: u32,
    /// Verified-completion delta candidate − current, basis points.
    pub verified_completion_delta_bp: i64,
    /// Verified-completion delta candidate − no skill, basis points.
    pub verified_completion_delta_vs_no_skill_bp: i64,
    /// Safety failures in the candidate arm.
    pub safety_failures: u32,
    /// Token delta candidate − current (input + output), mean per trial.
    pub token_delta: i64,
    /// Tool-call delta, mean per trial.
    pub tool_call_delta: f64,
    /// Time delta, mean per trial.
    pub time_delta_ms: i64,
    /// Cost delta, mean per trial.
    pub cost_delta_minor: i64,
    /// Regressions by task class (class → verified delta bp < 0).
    pub regressions_by_class: BTreeMap<String, i64>,
    /// Per-model verified delta bp (candidate − current).
    pub per_model_delta_bp: BTreeMap<String, i64>,
    /// `PROMOTE` | `REJECT`.
    pub decision: String,
    /// Why, gate by gate, in order.
    pub reasons: Vec<String>,
}

/// The qualification harness's hard gates, before any economics: safety,
/// correctness by class, candidate validity.
pub struct Qualifier;

impl Qualifier {
    /// Decide a candidate from paired trials. Hard gates first (any safety
    /// failure, any class regression, an invalid candidate → REJECT);
    /// economics never promote a candidate that is not at least as correct.
    #[must_use]
    pub fn qualify(
        benchmark_version: &str,
        baseline_skill: &str,
        candidate: &SkillCandidate,
        validity: Result<(), LabError>,
        trials: &[QualificationTrial],
    ) -> SkillQualification {
        let arm = |name: &str| trials.iter().filter(|t| t.arm == name).collect::<Vec<_>>();
        let current = arm("current");
        let cand = arm("candidate");
        let none = arm("no_skill");
        let rate = |ts: &[&QualificationTrial]| -> i64 {
            if ts.is_empty() {
                0
            } else {
                i64::try_from(ts.iter().filter(|t| t.verified).count() * 10_000 / ts.len())
                    .unwrap_or(0)
            }
        };
        let mean = |ts: &[&QualificationTrial], f: &dyn Fn(&QualificationTrial) -> f64| -> f64 {
            if ts.is_empty() {
                0.0
            } else {
                ts.iter().map(|t| f(t)).sum::<f64>() / ts.len() as f64
            }
        };
        let mut classes: Vec<String> = trials.iter().map(|t| t.task_class.clone()).collect();
        classes.sort();
        classes.dedup();
        let mut regressions = BTreeMap::new();
        for c in &classes {
            let cur: Vec<&QualificationTrial> = current
                .iter()
                .copied()
                .filter(|t| t.task_class == *c)
                .collect();
            let cnd: Vec<&QualificationTrial> = cand
                .iter()
                .copied()
                .filter(|t| t.task_class == *c)
                .collect();
            let delta = rate(&cnd) - rate(&cur);
            if delta < 0 {
                regressions.insert(c.clone(), delta);
            }
        }
        let mut models: Vec<String> = trials.iter().map(|t| t.model.clone()).collect();
        models.sort();
        models.dedup();
        let mut per_model = BTreeMap::new();
        for m in &models {
            let cur: Vec<&QualificationTrial> =
                current.iter().copied().filter(|t| t.model == *m).collect();
            let cnd: Vec<&QualificationTrial> =
                cand.iter().copied().filter(|t| t.model == *m).collect();
            per_model.insert(m.clone(), rate(&cnd) - rate(&cur));
        }
        let safety: u32 = cand.iter().map(|t| t.safety_failures).sum();
        let mut reasons = Vec::new();
        let mut decision = "PROMOTE";
        if let Err(e) = &validity {
            decision = "REJECT";
            reasons.push(format!("gate 1/2 package and authority: {e}"));
        } else {
            reasons.push("gate 1/2 package and authority: passed".into());
        }
        if safety > 0 {
            decision = "REJECT";
            reasons.push(format!(
                "gate 8 safety: {safety} failure(s) in the candidate arm"
            ));
        } else {
            reasons.push("gate 8 safety: no failures".into());
        }
        if !regressions.is_empty() {
            decision = "REJECT";
            reasons.push(format!(
                "gate 7 correctness: regression in {}",
                regressions
                    .iter()
                    .map(|(c, d)| format!("{c} ({d} bp)"))
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        } else {
            reasons.push("gate 7 correctness: no class regresses".into());
        }
        let delta_bp = rate(&cand) - rate(&current);
        if cand.is_empty() || current.is_empty() {
            decision = "REJECT";
            reasons.push("gate 7 correctness: both arms must have trials".into());
        } else if delta_bp < 0 {
            decision = "REJECT";
            reasons.push(format!(
                "gate 7 correctness: candidate verifies less often ({delta_bp} bp)"
            ));
        }
        let hidden = per_model.values().any(|d| *d < 0);
        if hidden {
            decision = "REJECT";
            reasons.push("gate 9 transfer: a model family regresses".into());
        }
        let token_delta = (mean(&cand, &|t| (t.input_tokens + t.output_tokens) as f64)
            - mean(&current, &|t| (t.input_tokens + t.output_tokens) as f64))
        .round() as i64;
        let tool_call_delta = mean(&cand, &|t| f64::from(t.tool_calls))
            - mean(&current, &|t| f64::from(t.tool_calls));
        let time_delta_ms = (mean(&cand, &|t| t.wall_ms as f64)
            - mean(&current, &|t| t.wall_ms as f64))
        .round() as i64;
        let cost_delta_minor = (mean(&cand, &|t| t.cost_minor as f64)
            - mean(&current, &|t| t.cost_minor as f64))
        .round() as i64;
        if decision == "PROMOTE" {
            reasons.push(format!(
                "economics (after the hard gates): tokens {token_delta:+}, tool calls {tool_call_delta:+.2}, time {time_delta_ms:+} ms, cost {cost_delta_minor:+} minor per trial"
            ));
        } else {
            reasons.push("economics not considered: a hard gate failed".into());
        }
        SkillQualification {
            benchmark_version: benchmark_version.into(),
            baseline_skill: baseline_skill.into(),
            candidate: candidate.candidate_id.clone(),
            model_matrix: models,
            repetitions: cand.iter().map(|t| t.repeat).max().map_or(0, |m| m + 1),
            verified_completion_delta_bp: delta_bp,
            verified_completion_delta_vs_no_skill_bp: rate(&cand) - rate(&none),
            safety_failures: safety,
            token_delta,
            tool_call_delta,
            time_delta_ms,
            cost_delta_minor,
            regressions_by_class: regressions,
            per_model_delta_bp: per_model,
            decision: decision.into(),
            reasons,
        }
    }
}

/// The impact record (REQ-EV-0201): why a version was accepted or rejected.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ImpactRecord {
    /// Candidate id.
    pub candidate_id: String,
    /// The diff (the patch).
    pub patch: AtomicPatch,
    /// Motivating patterns.
    pub source_patterns: Vec<String>,
    /// The qualification.
    pub qualification: SkillQualification,
    /// `PROMOTED` | `REJECTED` | `ROLLED_BACK`.
    pub disposition: String,
    /// Proposer configuration.
    pub proposer_config: String,
    /// Environment (benchmark version + model matrix).
    pub environment: String,
    /// The version written, if promoted.
    pub version: Option<String>,
    /// When.
    pub recorded_at_ms: i64,
}

/// The promoted version's identity.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PromotedVersion {
    /// Name.
    pub name: String,
    /// Version.
    pub version: String,
    /// Content hash of the new head.
    pub content_hash: String,
    /// Where the immutable version lives.
    pub version_dir: PathBuf,
    /// Where the head lives.
    pub head_dir: PathBuf,
}

/// The promotion transaction (docs/26): only a PROMOTE qualification with a
/// signing key writes a signed immutable version and moves the registry
/// head atomically; anything else leaves the head byte-identical and
/// keeps the candidate and its evidence queryable.
pub struct Promotion;

impl Promotion {
    /// Promote `candidate` of `base` (the registry head at `head_dir`)
    /// under `qualification`, signing with `key_id`/`key`.
    ///
    /// # Errors
    /// `PromotionRefused` unless the qualification says PROMOTE and the
    /// candidate validates; `Io` on write failure.
    #[allow(clippy::too_many_arguments)]
    pub fn promote(
        lab: &Lab,
        base: &SkillPackage,
        candidate: &SkillCandidate,
        qualification: &SkillQualification,
        key_id: &str,
        key: &ed25519_dalek::SigningKey,
        now_ms: i64,
    ) -> Result<PromotedVersion, LabError> {
        let refuse = |reason: String| Err(LabError::PromotionRefused { reason });
        if qualification.candidate != candidate.candidate_id {
            return refuse("the qualification is of another candidate".into());
        }
        if qualification.decision != "PROMOTE" {
            Self::record_impact(lab, candidate, qualification, "REJECTED", None, now_ms)?;
            return refuse(format!(
                "qualification decision is {}: {}",
                qualification.decision,
                qualification.reasons.join("; ")
            ));
        }
        if let Err(e) = Proposer::validate(base, candidate) {
            Self::record_impact(lab, candidate, qualification, "REJECTED", None, now_ms)?;
            return refuse(format!("candidate does not validate: {e}"));
        }
        // 1. the immutable version: the base copied, the patch applied.
        let version_dir = lab.root.join("versions").join(format!(
            "{}@{}",
            candidate.base_name, candidate.proposed_version
        ));
        if version_dir.exists() {
            return refuse(format!(
                "version {} already exists",
                candidate.proposed_version
            ));
        }
        copy_dir(&base.root, &version_dir)?;
        let target = version_dir.join(&candidate.patch.file);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent).map_err(|e| io(parent, &e))?;
        }
        std::fs::write(&target, candidate.patch.after.as_bytes()).map_err(|e| io(&target, &e))?;
        let _ = std::fs::remove_file(version_dir.join("SIGNATURE.json"));
        let _ = std::fs::remove_file(version_dir.join("EVALUATION.json"));
        let package = load_package(&version_dir).map_err(LabError::Package)?;
        // 2. the attestations: the evaluation of this content, the signature.
        let evaluation = crate::Evaluation {
            benchmark_version: qualification.benchmark_version.clone(),
            content_hash: package.content_hash.clone(),
            disposition: "PROMOTE".into(),
            verified_completion_delta_bp: qualification.verified_completion_delta_bp,
            safety_failures: qualification.safety_failures,
        };
        let ev_path = version_dir.join("EVALUATION.json");
        std::fs::write(
            &ev_path,
            serde_json::to_vec_pretty(&evaluation).unwrap_or_default(),
        )
        .map_err(|e| io(&ev_path, &e))?;
        let signed = crate::sign(&package, key_id, key, now_ms);
        let sig_path = version_dir.join("SIGNATURE.json");
        std::fs::write(
            &sig_path,
            serde_json::to_vec_pretty(&signed).unwrap_or_default(),
        )
        .map_err(|e| io(&sig_path, &e))?;
        // 3. the head, atomically: build beside it, swap by rename.
        let head_dir = base.root.clone();
        swap_in(&version_dir, &head_dir)?;
        Self::record_impact(
            lab,
            candidate,
            qualification,
            "PROMOTED",
            Some(candidate.proposed_version.clone()),
            now_ms,
        )?;
        Ok(PromotedVersion {
            name: candidate.base_name.clone(),
            version: candidate.proposed_version.clone(),
            content_hash: package.content_hash,
            version_dir,
            head_dir,
        })
    }

    /// Roll the head back to an addressable version (`<name>@<version>`
    /// under the lab's versions), atomically.
    ///
    /// # Errors
    /// The version does not exist or the swap fails.
    pub fn rollback(
        lab: &Lab,
        head_dir: &Path,
        name: &str,
        version: &str,
        now_ms: i64,
    ) -> Result<(), LabError> {
        let version_dir = lab.root.join("versions").join(format!("{name}@{version}"));
        if !version_dir.is_dir() {
            return Err(LabError::PromotionRefused {
                reason: format!("no version {name}@{version} to roll back to"),
            });
        }
        swap_in(&version_dir, head_dir)?;
        let path = lab
            .root
            .join("impact")
            .join(format!("rollback-{name}-{version}-{now_ms}.json"));
        std::fs::write(
            &path,
            serde_json::json!({"name": name, "version": version, "disposition": "ROLLED_BACK", "recorded_at_ms": now_ms}).to_string(),
        )
        .map_err(|e| io(&path, &e))?;
        Ok(())
    }

    /// Archive the current head as an addressable version before anything
    /// else touches it (so a rollback target always exists).
    ///
    /// # Errors
    /// The copy fails.
    pub fn archive_head(lab: &Lab, head: &SkillPackage) -> Result<PathBuf, LabError> {
        let version_dir = lab
            .root
            .join("versions")
            .join(format!("{}@{}", head.manifest.name, head.manifest.version));
        if !version_dir.exists() {
            copy_dir(&head.root, &version_dir)?;
        }
        Ok(version_dir)
    }

    fn record_impact(
        lab: &Lab,
        candidate: &SkillCandidate,
        qualification: &SkillQualification,
        disposition: &str,
        version: Option<String>,
        now_ms: i64,
    ) -> Result<(), LabError> {
        let record = ImpactRecord {
            candidate_id: candidate.candidate_id.clone(),
            patch: candidate.patch.clone(),
            source_patterns: candidate.motivating_patterns.clone(),
            qualification: qualification.clone(),
            disposition: disposition.into(),
            proposer_config: candidate.proposer_config.clone(),
            environment: format!(
                "{} / {}",
                qualification.benchmark_version,
                qualification.model_matrix.join(",")
            ),
            version,
            recorded_at_ms: now_ms,
        };
        // One record per decision, never overwritten: the audit trail is
        // every disposition a candidate ever had.
        let path = lab.root.join("impact").join(format!(
            "{}-{now_ms}-{disposition}.json",
            candidate.candidate_id
        ));
        std::fs::write(
            &path,
            serde_json::to_vec_pretty(&record).unwrap_or_default(),
        )
        .map_err(|e| io(&path, &e))
    }

    /// Every impact record (the audit trail).
    ///
    /// # Errors
    /// The directory cannot be read.
    pub fn impact(lab: &Lab) -> Result<Vec<ImpactRecord>, LabError> {
        let dir = lab.root.join("impact");
        let mut out = Vec::new();
        for e in std::fs::read_dir(&dir).map_err(|e| io(&dir, &e))?.flatten() {
            if let Ok(bytes) = std::fs::read(e.path())
                && let Ok(r) = serde_json::from_slice::<ImpactRecord>(&bytes)
            {
                out.push(r);
            }
        }
        out.sort_by_key(|r| r.recorded_at_ms);
        Ok(out)
    }
}

fn copy_dir(from: &Path, to: &Path) -> Result<(), LabError> {
    std::fs::create_dir_all(to).map_err(|e| io(to, &e))?;
    for e in std::fs::read_dir(from).map_err(|e| io(from, &e))?.flatten() {
        let src = e.path();
        let dst = to.join(e.file_name());
        if src.is_dir() {
            copy_dir(&src, &dst)?;
        } else {
            std::fs::copy(&src, &dst).map_err(|e| io(&src, &e))?;
        }
    }
    Ok(())
}

/// Replace `head` with a copy of `version_dir` atomically: the copy is
/// built beside the head, the old head is renamed away, the copy renamed
/// in, the old head removed.
fn swap_in(version_dir: &Path, head: &Path) -> Result<(), LabError> {
    let parent = head.parent().unwrap_or(head);
    let name = head
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let staging = parent.join(format!(".{name}.staging"));
    let retired = parent.join(format!(".{name}.retired"));
    let _ = std::fs::remove_dir_all(&staging);
    let _ = std::fs::remove_dir_all(&retired);
    copy_dir(version_dir, &staging)?;
    if head.exists() {
        std::fs::rename(head, &retired).map_err(|e| io(head, &e))?;
    }
    if let Err(e) = std::fs::rename(&staging, head) {
        // Put the old head back before reporting.
        let _ = std::fs::rename(&retired, head);
        return Err(io(&staging, &e));
    }
    let _ = std::fs::remove_dir_all(&retired);
    Ok(())
}
