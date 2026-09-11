//! `modbit-checkpoint` — layer 5 of the seven-layer durability invariant
//! (docs/19 "Checkpoint epochs"): recoverable worktree and runtime state as
//! a periodic full baseline plus deltas, fenced by a monotonic epoch so a
//! stale writer can never overwrite newer checkpoint state, and restored
//! only after every object it names has been read back and hash-validated.
//!
//! Canonical owner: core-runtime (`docs/12_REPOSITORY_AND_MODULE_LAYOUT.md`,
//! `docs/81_ARCHITECTURE_GUARDRAILS_AND_FORBIDDEN_DUPLICATION.md`).
//! Dependency direction is enforced by `tools/architecture-lint`.
//!
//! This crate owns the manifest, the fencing rule, the chain and the
//! validation. It reads no file and touches no database: the Core captures
//! the worktree into content-addressed objects, hands the entries here, and
//! writes the restored files back through the workspace service.

use std::collections::{BTreeMap, BTreeSet};

use modbit_domain::{CheckpointId, TaskId, Timestamp};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Manifest schema version.
pub const CHECKPOINT_SCHEMA_VERSION: u32 = 1;

/// The "content" of a tracked path that is deleted in the worktree: there is
/// no object to read; restoring it means removing the file.
pub const DELETED: &str = "";

/// Whether a checkpoint carries the whole dirty state or the change since
/// its base (docs/19: "periodic full baseline + intermediate deltas").
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CheckpointKind {
    /// Every file that differs from the Git HEAD (tracked changes and
    /// untracked files), by object hash.
    Baseline,
    /// The files changed or removed since the base checkpoint.
    Delta,
}

/// The runtime cursor a checkpoint carries beside the worktree (docs/19:
/// "runtime/evidence cursor plus worktree deltas instead of transcript
/// snapshots"; REQ-EV-0013).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeCursor {
    /// Store offset the checkpoint was taken at.
    pub event_offset: u64,
    /// Installed compaction epoch, 0 when none.
    pub compaction_epoch: u32,
    /// Session branch generation.
    pub branch_generation: u64,
    /// sha256 of the task's protocol state at capture.
    pub protocol_digest: String,
}

/// What one checkpoint recorded (docs/31 `checkpoints`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CheckpointManifest {
    /// Schema version.
    pub schema_version: u32,
    /// Identity.
    pub checkpoint_id: CheckpointId,
    /// Task.
    pub task_id: TaskId,
    /// Monotonic epoch (1 = the task's first checkpoint).
    pub epoch: u32,
    /// Baseline or delta.
    pub kind: CheckpointKind,
    /// The checkpoint a delta is relative to (the baseline or the previous
    /// delta); `None` for a baseline.
    pub base_checkpoint_id: Option<CheckpointId>,
    /// Workspace revision at capture.
    pub workspace_revision: u64,
    /// Git HEAD at capture, when the root is a repository.
    pub git_head: Option<String>,
    /// Worktree identity.
    pub worktree_id: String,
    /// Path → object hash of its content, or [`DELETED`] for a tracked path
    /// deleted from the worktree. A baseline lists every path that differs
    /// from HEAD; a delta lists the paths whose state differs from the
    /// base's materialized state.
    pub files: BTreeMap<String, String>,
    /// Paths present in the base's materialized state and absent now (delta
    /// only): they are back to HEAD (or gone, if untracked) at this epoch.
    pub removed: Vec<String>,
    /// Runtime cursor.
    pub runtime: RuntimeCursor,
    /// Retrieval index generation at capture.
    pub index_generation: u64,
    /// Capture time.
    pub created_at: Timestamp,
    /// Why the checkpoint was taken (`before_completion`, `before_revert`,
    /// `requested`, ...).
    pub reason: String,
    /// sha256 over every field above, in canonical order.
    pub integrity_hash: String,
}

/// What the Core hands over to build a manifest.
#[derive(Clone, Debug)]
pub struct CaptureRequest<'a> {
    /// Identity for the new checkpoint.
    pub checkpoint_id: CheckpointId,
    /// Task.
    pub task_id: TaskId,
    /// Epoch assigned when the checkpoint started.
    pub epoch: u32,
    /// The current dirty state: path → object hash, for every file that
    /// differs from HEAD.
    pub dirty: &'a BTreeMap<String, String>,
    /// The base to take a delta from (its materialized state), or `None`
    /// for a baseline.
    pub base: Option<(&'a CheckpointManifest, &'a BTreeMap<String, String>)>,
    /// Workspace revision.
    pub workspace_revision: u64,
    /// Git HEAD.
    pub git_head: Option<String>,
    /// Worktree identity.
    pub worktree_id: String,
    /// Runtime cursor.
    pub runtime: RuntimeCursor,
    /// Index generation.
    pub index_generation: u64,
    /// Now.
    pub now: Timestamp,
    /// Reason.
    pub reason: &'a str,
}

/// Build the manifest for a capture. A delta lists only what changed since
/// the base's materialized state; a baseline lists the whole dirty state.
#[must_use]
pub fn capture(req: &CaptureRequest<'_>) -> CheckpointManifest {
    let (kind, base_id, files, removed) = match req.base {
        None => (
            CheckpointKind::Baseline,
            None,
            req.dirty.clone(),
            Vec::new(),
        ),
        Some((base, state)) => {
            let files: BTreeMap<String, String> = req
                .dirty
                .iter()
                .filter(|(p, h)| state.get(*p) != Some(*h))
                .map(|(p, h)| (p.clone(), h.clone()))
                .collect();
            let removed: Vec<String> = state
                .keys()
                .filter(|p| !req.dirty.contains_key(*p))
                .cloned()
                .collect();
            (
                CheckpointKind::Delta,
                Some(base.checkpoint_id),
                files,
                removed,
            )
        }
    };
    let mut m = CheckpointManifest {
        schema_version: CHECKPOINT_SCHEMA_VERSION,
        checkpoint_id: req.checkpoint_id,
        task_id: req.task_id,
        epoch: req.epoch,
        kind,
        base_checkpoint_id: base_id,
        workspace_revision: req.workspace_revision,
        git_head: req.git_head.clone(),
        worktree_id: req.worktree_id.clone(),
        files,
        removed,
        runtime: req.runtime.clone(),
        index_generation: req.index_generation,
        created_at: req.now,
        reason: req.reason.to_owned(),
        integrity_hash: String::new(),
    };
    m.integrity_hash = integrity_hash(&m);
    m
}

/// sha256 over the manifest's fields (integrity hash excluded), canonical
/// JSON with sorted keys.
#[must_use]
pub fn integrity_hash(m: &CheckpointManifest) -> String {
    let mut v = serde_json::to_value(m).unwrap_or_default();
    if let Some(o) = v.as_object_mut() {
        o.remove("integrity_hash");
    }
    hex::encode(Sha256::digest(canonical(&v).as_bytes()))
}

/// Why a checkpoint may not become current (docs/19: a stale epoch can never
/// overwrite newer checkpoint state; docs/54 fault 9).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum StaleCheckpoint {
    /// The offered epoch is not newer than the current one.
    NotNewer {
        /// Current committed epoch.
        current: u32,
        /// Offered epoch.
        offered: u32,
    },
    /// The manifest's integrity hash does not match its fields.
    IntegrityMismatch {
        /// Recorded.
        recorded: String,
        /// Computed.
        computed: String,
    },
}

/// Whether a checkpoint with `offered` epoch may commit over `current`.
///
/// # Errors
/// The epoch is not strictly newer, or the manifest is not what it says.
pub fn accept_commit(
    manifest: &CheckpointManifest,
    current: Option<u32>,
) -> Result<(), StaleCheckpoint> {
    let computed = integrity_hash(manifest);
    if computed != manifest.integrity_hash {
        return Err(StaleCheckpoint::IntegrityMismatch {
            recorded: manifest.integrity_hash.clone(),
            computed,
        });
    }
    if let Some(c) = current
        && manifest.epoch <= c
    {
        return Err(StaleCheckpoint::NotNewer {
            current: c,
            offered: manifest.epoch,
        });
    }
    Ok(())
}

/// Why a chain cannot be restored.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChainError {
    /// No baseline at the head of the chain.
    NoBaseline,
    /// A delta's base is not the checkpoint before it.
    BrokenLink {
        /// The delta.
        checkpoint_id: CheckpointId,
        /// The base it names.
        expected: Option<CheckpointId>,
        /// The checkpoint before it.
        found: CheckpointId,
    },
    /// Epochs do not ascend along the chain.
    EpochOrder {
        /// Earlier.
        previous: u32,
        /// Later.
        next: u32,
    },
    /// An object a manifest names could not be read back with its digest.
    ObjectMismatch {
        /// Path.
        path: String,
        /// Object hash.
        hash: String,
        /// What the reader said.
        detail: String,
    },
    /// A manifest's integrity hash does not match its fields.
    IntegrityMismatch {
        /// The checkpoint.
        checkpoint_id: CheckpointId,
    },
}

/// Content-addressed reads, as the Core's object store provides them.
pub trait ObjectSource {
    /// Read and digest-verify the object; `Err` carries the detail.
    fn get(&self, hash: &str) -> Result<Vec<u8>, String>;
}

/// The state a chain resolves to: the files (path → hash) that differ from
/// HEAD at the target epoch, and the runtime cursor to continue from.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Materialized {
    /// Target checkpoint.
    pub checkpoint_id: CheckpointId,
    /// Target epoch.
    pub epoch: u32,
    /// Path → object hash.
    pub files: BTreeMap<String, String>,
    /// Git HEAD the state is relative to.
    pub git_head: Option<String>,
    /// Workspace revision at the target.
    pub workspace_revision: u64,
    /// Runtime cursor at the target.
    pub runtime: RuntimeCursor,
    /// Every manifest walked, oldest first.
    pub chain: Vec<CheckpointId>,
}

/// Walk `chain` (a baseline followed by its deltas, oldest first, ending at
/// the target) into the state at the target. Validates every link, the
/// epoch order and every manifest's integrity hash; reads nothing.
///
/// # Errors
/// The chain is not a chain.
pub fn materialize(chain: &[CheckpointManifest]) -> Result<Materialized, ChainError> {
    let Some(head) = chain.first() else {
        return Err(ChainError::NoBaseline);
    };
    if head.kind != CheckpointKind::Baseline {
        return Err(ChainError::NoBaseline);
    }
    let mut files: BTreeMap<String, String> = BTreeMap::new();
    let mut previous: Option<&CheckpointManifest> = None;
    for m in chain {
        if integrity_hash(m) != m.integrity_hash {
            return Err(ChainError::IntegrityMismatch {
                checkpoint_id: m.checkpoint_id,
            });
        }
        if let Some(p) = previous {
            if m.base_checkpoint_id != Some(p.checkpoint_id) {
                return Err(ChainError::BrokenLink {
                    checkpoint_id: m.checkpoint_id,
                    expected: m.base_checkpoint_id,
                    found: p.checkpoint_id,
                });
            }
            if m.epoch <= p.epoch {
                return Err(ChainError::EpochOrder {
                    previous: p.epoch,
                    next: m.epoch,
                });
            }
        }
        match m.kind {
            CheckpointKind::Baseline => {
                files = m.files.clone();
            }
            CheckpointKind::Delta => {
                for r in &m.removed {
                    files.remove(r);
                }
                for (p, h) in &m.files {
                    files.insert(p.clone(), h.clone());
                }
            }
        }
        previous = Some(m);
    }
    let target = chain.last().expect("non-empty");
    Ok(Materialized {
        checkpoint_id: target.checkpoint_id,
        epoch: target.epoch,
        files,
        git_head: target.git_head.clone(),
        workspace_revision: target.workspace_revision,
        runtime: target.runtime.clone(),
        chain: chain.iter().map(|m| m.checkpoint_id).collect(),
    })
}

/// Read every object the materialized state names and verify it against its
/// digest, before anything is written (docs/19: "Restore validates every
/// object hash before making the checkpoint current"). Returns the bytes by
/// path; deleted paths ([`DELETED`]) have no bytes and are not in the map.
///
/// # Errors
/// The first object that cannot be read back with its digest.
pub fn validate(
    state: &Materialized,
    objects: &dyn ObjectSource,
) -> Result<BTreeMap<String, Vec<u8>>, ChainError> {
    let mut out = BTreeMap::new();
    for (path, hash) in &state.files {
        if hash == DELETED {
            continue;
        }
        let bytes = objects
            .get(hash)
            .map_err(|detail| ChainError::ObjectMismatch {
                path: path.clone(),
                hash: hash.clone(),
                detail,
            })?;
        let digest = hex::encode(Sha256::digest(&bytes));
        if digest != *hash {
            return Err(ChainError::ObjectMismatch {
                path: path.clone(),
                hash: hash.clone(),
                detail: format!("content digest {digest}"),
            });
        }
        out.insert(path.clone(), bytes);
    }
    Ok(out)
}

/// The chain that ends at `target` among `manifests` (any order): the
/// baseline it descends from and every delta between, oldest first.
///
/// # Errors
/// The target or a link is missing.
pub fn chain_to(
    manifests: &[CheckpointManifest],
    target: CheckpointId,
) -> Result<Vec<CheckpointManifest>, ChainError> {
    let mut out = Vec::new();
    let mut cursor = Some(target);
    let mut seen = BTreeSet::new();
    while let Some(id) = cursor {
        if !seen.insert(id) {
            return Err(ChainError::NoBaseline);
        }
        let Some(m) = manifests.iter().find(|m| m.checkpoint_id == id) else {
            return Err(ChainError::BrokenLink {
                checkpoint_id: id,
                expected: None,
                found: id,
            });
        };
        cursor = m.base_checkpoint_id;
        out.push(m.clone());
    }
    out.reverse();
    if out.first().map(|m| m.kind) != Some(CheckpointKind::Baseline) {
        return Err(ChainError::NoBaseline);
    }
    Ok(out)
}

/// Whether the next checkpoint should be a baseline: the first one, or when
/// the delta chain since the last baseline is `max_deltas` long.
#[must_use]
pub fn needs_baseline(chain_since_baseline: usize, max_deltas: usize) -> bool {
    chain_since_baseline == 0 || chain_since_baseline > max_deltas
}

fn canonical(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::Object(m) => {
            let mut keys: Vec<&String> = m.keys().collect();
            keys.sort();
            let parts: Vec<String> = keys
                .into_iter()
                .map(|k| {
                    format!(
                        "{}:{}",
                        serde_json::to_string(k).unwrap_or_default(),
                        canonical(&m[k])
                    )
                })
                .collect();
            format!("{{{}}}", parts.join(","))
        }
        serde_json::Value::Array(a) => {
            let parts: Vec<String> = a.iter().map(canonical).collect();
            format!("[{}]", parts.join(","))
        }
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Mem(BTreeMap<String, Vec<u8>>);
    impl ObjectSource for Mem {
        fn get(&self, hash: &str) -> Result<Vec<u8>, String> {
            self.0.get(hash).cloned().ok_or_else(|| "missing".into())
        }
    }

    fn h(bytes: &[u8]) -> String {
        hex::encode(Sha256::digest(bytes))
    }

    fn req<'a>(
        task: TaskId,
        epoch: u32,
        dirty: &'a BTreeMap<String, String>,
        base: Option<(&'a CheckpointManifest, &'a BTreeMap<String, String>)>,
    ) -> CaptureRequest<'a> {
        CaptureRequest {
            checkpoint_id: CheckpointId::new(),
            task_id: task,
            epoch,
            dirty,
            base,
            workspace_revision: u64::from(epoch) * 10,
            git_head: Some("abc".into()),
            worktree_id: "wt".into(),
            runtime: RuntimeCursor {
                event_offset: u64::from(epoch) * 100,
                compaction_epoch: 0,
                branch_generation: 0,
                protocol_digest: "p".into(),
            },
            index_generation: 1,
            now: Timestamp(1),
            reason: "test",
        }
    }

    #[test]
    fn baseline_then_deltas_materialize_to_the_final_state_and_validate_every_object() {
        let task = TaskId::new();
        let a1 = h(b"a1");
        let b1 = h(b"b1");
        let a2 = h(b"a2");
        let c1 = h(b"c1");
        let dirty1: BTreeMap<String, String> =
            [("a.txt".into(), a1.clone()), ("b.txt".into(), b1.clone())].into();
        let base = capture(&req(task, 1, &dirty1, None));
        assert_eq!(base.kind, CheckpointKind::Baseline);
        assert_eq!(base.files.len(), 2);
        let state1 = materialize(std::slice::from_ref(&base)).unwrap();
        // a changed, b reverted to HEAD, c appeared.
        let dirty2: BTreeMap<String, String> =
            [("a.txt".into(), a2.clone()), ("c.txt".into(), c1.clone())].into();
        let delta = capture(&req(task, 2, &dirty2, Some((&base, &state1.files))));
        assert_eq!(delta.kind, CheckpointKind::Delta);
        assert_eq!(
            delta.files,
            [
                ("a.txt".to_owned(), a2.clone()),
                ("c.txt".to_owned(), c1.clone())
            ]
            .into()
        );
        assert_eq!(delta.removed, vec!["b.txt".to_owned()]);
        assert_eq!(delta.base_checkpoint_id, Some(base.checkpoint_id));
        let state2 = materialize(&[base.clone(), delta.clone()]).unwrap();
        assert_eq!(state2.files, dirty2);
        assert_eq!(state2.epoch, 2);
        assert_eq!(state2.chain, vec![base.checkpoint_id, delta.checkpoint_id]);
        // chain_to finds the same chain from an unordered set.
        let found = chain_to(&[delta.clone(), base.clone()], delta.checkpoint_id).unwrap();
        assert_eq!(found, vec![base.clone(), delta.clone()]);
        // Validation reads every object and refuses a corrupt one before
        // anything is written.
        let objects = Mem([(a2.clone(), b"a2".to_vec()), (c1.clone(), b"c1".to_vec())].into());
        let bytes = validate(&state2, &objects).unwrap();
        assert_eq!(bytes["a.txt"], b"a2");
        let corrupt = Mem([(a2.clone(), b"a2".to_vec()), (c1.clone(), b"XX".to_vec())].into());
        assert!(matches!(
            validate(&state2, &corrupt),
            Err(ChainError::ObjectMismatch { ref path, .. }) if path == "c.txt"
        ));
        let missing = Mem([(a2.clone(), b"a2".to_vec())].into());
        assert!(matches!(
            validate(&state2, &missing),
            Err(ChainError::ObjectMismatch { ref path, .. }) if path == "c.txt"
        ));
    }

    #[test]
    fn a_stale_epoch_never_commits_over_a_newer_one_and_tampering_is_caught() {
        let task = TaskId::new();
        let dirty: BTreeMap<String, String> = [("a.txt".to_owned(), h(b"a"))].into();
        let n = capture(&req(task, 3, &dirty, None));
        let n1 = capture(&req(task, 4, &dirty, None));
        assert!(accept_commit(&n1, Some(2)).is_ok());
        assert!(accept_commit(&n, Some(2)).is_ok());
        // N+1 committed first: N is stale and cannot become current.
        assert_eq!(
            accept_commit(&n, Some(4)),
            Err(StaleCheckpoint::NotNewer {
                current: 4,
                offered: 3
            })
        );
        assert_eq!(
            accept_commit(&n1, Some(4)),
            Err(StaleCheckpoint::NotNewer {
                current: 4,
                offered: 4
            }),
            "an epoch never commits twice"
        );
        let mut tampered = n1.clone();
        tampered.files.insert("z.txt".into(), h(b"z"));
        assert!(matches!(
            accept_commit(&tampered, None),
            Err(StaleCheckpoint::IntegrityMismatch { .. })
        ));
        assert!(matches!(
            materialize(&[tampered]),
            Err(ChainError::IntegrityMismatch { .. })
        ));
    }

    #[test]
    fn broken_chains_are_refused() {
        let task = TaskId::new();
        let dirty: BTreeMap<String, String> = [("a.txt".to_owned(), h(b"a"))].into();
        let base = capture(&req(task, 1, &dirty, None));
        let other = capture(&req(task, 2, &dirty, None));
        let state = materialize(std::slice::from_ref(&base)).unwrap();
        let delta = capture(&req(task, 3, &dirty, Some((&other, &state.files))));
        assert!(matches!(
            materialize(&[base.clone(), delta.clone()]),
            Err(ChainError::BrokenLink { .. })
        ));
        assert_eq!(
            materialize(std::slice::from_ref(&delta)),
            Err(ChainError::NoBaseline)
        );
        assert_eq!(materialize(&[]), Err(ChainError::NoBaseline));
        let mut backwards = capture(&req(task, 1, &dirty, Some((&other, &state.files))));
        backwards.integrity_hash = integrity_hash(&backwards);
        assert!(matches!(
            materialize(&[other.clone(), backwards]),
            Err(ChainError::EpochOrder { .. })
        ));
        assert!(needs_baseline(0, 8));
        assert!(!needs_baseline(3, 8));
        assert!(needs_baseline(9, 8));
    }
}

// ---------------------------------------------------------------------------
// Session branching (REQ-EV-0077, REQ-EV-0122, REQ-EV-0123)
// ---------------------------------------------------------------------------

/// Capsule schema version.
pub const CARRYOVER_SCHEMA_VERSION: u32 = 1;

/// What a fork may carry from its source (docs/40 REQ-EV-0122: "selected
/// decisions/evidence"). Pending approvals and in-flight calls are never
/// carried: they are bound to the source's intents.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Carry {
    /// The latest plan and its expected files.
    Plan,
    /// Answered questions.
    Decisions,
    /// Retrieval records whose content the fork still has.
    Evidence,
    /// The latest compiled context and the installed compaction epoch.
    Context,
}

impl Carry {
    /// Every kind.
    pub const ALL: [Self; 4] = [Self::Plan, Self::Decisions, Self::Evidence, Self::Context];

    /// Stable label.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Plan => "PLAN",
            Self::Decisions => "DECISIONS",
            Self::Evidence => "EVIDENCE",
            Self::Context => "CONTEXT",
        }
    }

    /// Parse a label.
    #[must_use]
    pub fn parse(label: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|c| c.label().eq_ignore_ascii_case(label.trim()))
    }
}

/// A decision the source made that the fork inherits: a question and its
/// recorded answer.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CarriedDecision {
    /// Question id on the source.
    pub question_id: String,
    /// The question.
    pub question: String,
    /// Why it was asked.
    pub reason: String,
    /// Chosen option, if any.
    pub option_id: Option<String>,
    /// Free text, if any.
    pub text: Option<String>,
}

/// A retrieval record the fork inherits: the fork's worktree has the same
/// bytes at the path, so the retrieve-before-edit gate is satisfied.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CarriedEvidence {
    /// Path.
    pub path: String,
    /// Content hash at the retrieval, equal to the fork's content.
    pub content_hash: String,
}

/// An approval of the source that was pending at the fork and deliberately
/// not carried (docs/40 QUAL-EV-0122: "no stale pending approval").
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DroppedApproval {
    /// Approval id on the source.
    pub approval_id: String,
    /// The tool call it gates.
    pub tool_call_id: String,
    /// Why it was not carried.
    pub reason: String,
}

/// The `BranchCarryoverCapsule` (REQ-EV-0122): a content-addressed record
/// of exactly what a fork took from its source, and what it left behind.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BranchCarryoverCapsule {
    /// Schema version.
    pub schema_version: u32,
    /// Source task.
    pub source_task_id: TaskId,
    /// Source's latest run at the fork, when any (as UUID text).
    pub source_run_id: Option<String>,
    /// The checkpoint the worktree was materialized from.
    pub source_checkpoint_id: CheckpointId,
    /// Its epoch.
    pub source_epoch: u32,
    /// The runtime cursor the checkpoint recorded.
    pub source_event_offset: u64,
    /// The fork task.
    pub fork_task_id: TaskId,
    /// The session branch generation the fork opened.
    pub branch_generation: u64,
    /// What was asked to be carried.
    pub carried: Vec<Carry>,
    /// The plan carried: object hash, expected files, version.
    pub plan: Option<CarriedPlan>,
    /// Decisions carried.
    pub decisions: Vec<CarriedDecision>,
    /// Evidence carried.
    pub evidence: Vec<CarriedEvidence>,
    /// Context carried: the latest Context Pack object and compaction
    /// manifest of the source, when any.
    pub context: Option<CarriedContext>,
    /// Pending approvals not carried.
    pub approvals_dropped: Vec<DroppedApproval>,
    /// In-flight or unfinished tool calls of the source, not carried.
    pub calls_dropped: Vec<String>,
    /// The fork's worktree: path → object hash, exactly the checkpoint's.
    pub worktree_files: BTreeMap<String, String>,
    /// Git HEAD the worktree is relative to.
    pub git_head: Option<String>,
    /// When.
    pub created_at: Timestamp,
}

/// The plan a fork carries.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CarriedPlan {
    /// Object hash of the plan JSON.
    pub plan_ref: String,
    /// Expected files.
    pub expected_files: Vec<String>,
    /// Version.
    pub version: u32,
}

/// The context a fork carries.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CarriedContext {
    /// The latest Context Pack object of the source, when any.
    pub context_pack_ref: Option<String>,
    /// The installed compaction epoch and its manifest, when any.
    pub compaction_epoch: u32,
    /// Manifest object hash.
    pub compaction_manifest_ref: Option<String>,
}

/// One line of a rewind preview (REQ-EV-0123): what a restore to the
/// checkpoint would do to one path, without doing it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RewindEntry {
    /// Path.
    pub path: String,
    /// `WRITE` (from an object) | `DELETE` (the checkpoint has it deleted)
    /// | `REVERT_TO_HEAD` (dirty now, absent from the checkpoint) |
    /// `REMOVE_UNTRACKED` (untracked now, absent from the checkpoint) |
    /// `UNCHANGED`.
    pub action: String,
    /// Content hash now, when the path exists.
    pub current_hash: Option<String>,
    /// Content hash after the restore, when the path would exist.
    pub target_hash: Option<String>,
}

/// Plan a restore to `state` against the worktree as it is now: `current`
/// is every path the worktree has dirty or untracked, with its content
/// hash (`None` = a tracked path deleted from the worktree). Pure: reads
/// nothing, writes nothing, so a preview is a plan that is not applied.
#[must_use]
pub fn plan_rewind(
    state: &Materialized,
    current: &BTreeMap<String, Option<String>>,
    tracked_at_head: &BTreeSet<String>,
) -> Vec<RewindEntry> {
    let mut out = Vec::new();
    for (path, target) in &state.files {
        let now = current.get(path).cloned().flatten();
        if target == DELETED {
            if now.is_some() || (!current.contains_key(path) && tracked_at_head.contains(path)) {
                out.push(RewindEntry {
                    path: path.clone(),
                    action: "DELETE".into(),
                    current_hash: now,
                    target_hash: None,
                });
            } else {
                out.push(RewindEntry {
                    path: path.clone(),
                    action: "UNCHANGED".into(),
                    current_hash: None,
                    target_hash: None,
                });
            }
        } else if now.as_deref() == Some(target.as_str()) {
            out.push(RewindEntry {
                path: path.clone(),
                action: "UNCHANGED".into(),
                current_hash: now,
                target_hash: Some(target.clone()),
            });
        } else {
            out.push(RewindEntry {
                path: path.clone(),
                action: "WRITE".into(),
                current_hash: now,
                target_hash: Some(target.clone()),
            });
        }
    }
    for (path, now) in current {
        if state.files.contains_key(path) {
            continue;
        }
        let action = if tracked_at_head.contains(path) {
            "REVERT_TO_HEAD"
        } else {
            "REMOVE_UNTRACKED"
        };
        out.push(RewindEntry {
            path: path.clone(),
            action: action.into(),
            current_hash: now.clone(),
            target_hash: None,
        });
    }
    out.sort_by(|a, b| a.path.cmp(&b.path));
    out
}

#[cfg(test)]
mod branching_tests {
    use super::*;

    fn state(files: &[(&str, &str)]) -> Materialized {
        Materialized {
            checkpoint_id: CheckpointId::new(),
            epoch: 1,
            files: files
                .iter()
                .map(|(p, h)| ((*p).to_owned(), (*h).to_owned()))
                .collect(),
            git_head: None,
            workspace_revision: 3,
            runtime: RuntimeCursor::default(),
            chain: vec![],
        }
    }

    #[test]
    fn a_rewind_plan_names_every_action_and_nothing_else() {
        let s = state(&[("a.txt", "h-a"), ("gone.txt", DELETED), ("same.txt", "h-s")]);
        let mut current = BTreeMap::new();
        current.insert("a.txt".to_owned(), Some("h-a2".to_owned()));
        current.insert("same.txt".to_owned(), Some("h-s".to_owned()));
        current.insert("extra.txt".to_owned(), Some("h-x".to_owned()));
        current.insert("tracked-dirty.txt".to_owned(), Some("h-t".to_owned()));
        current.insert("gone.txt".to_owned(), Some("h-g".to_owned()));
        let tracked: BTreeSet<String> = ["tracked-dirty.txt", "gone.txt", "a.txt", "same.txt"]
            .into_iter()
            .map(str::to_owned)
            .collect();
        let plan = plan_rewind(&s, &current, &tracked);
        let by: BTreeMap<&str, &str> = plan
            .iter()
            .map(|e| (e.path.as_str(), e.action.as_str()))
            .collect();
        assert_eq!(by["a.txt"], "WRITE");
        assert_eq!(by["gone.txt"], "DELETE");
        assert_eq!(by["same.txt"], "UNCHANGED");
        assert_eq!(by["extra.txt"], "REMOVE_UNTRACKED");
        assert_eq!(by["tracked-dirty.txt"], "REVERT_TO_HEAD");
        assert_eq!(plan.len(), 5);
    }

    #[test]
    fn carry_labels_round_trip() {
        for c in Carry::ALL {
            assert_eq!(Carry::parse(c.label()), Some(c));
        }
        assert_eq!(Carry::parse("nope"), None);
    }
}
