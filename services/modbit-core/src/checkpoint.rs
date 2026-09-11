//! Workspace checkpoints in the Core (docs/19 "Checkpoint epochs", docs/31
//! `checkpoints`, docs/54 fault 9; M4.3, REQ-EV-0012/0013): capture the
//! worktree's dirty state into content-addressed objects as a baseline or a
//! delta under a monotonic epoch, commit it only while it is newer than the
//! current checkpoint, and restore a validated chain — every object read
//! back and hash-checked before a byte is written.

use std::collections::{BTreeMap, HashMap};

use modbit_checkpoint::{
    CaptureRequest, ChainError, CheckpointKind, CheckpointManifest, RuntimeCursor, StaleCheckpoint,
    accept_commit, capture as build_manifest, chain_to, materialize, needs_baseline, validate,
};
use modbit_domain::event::{Actor, AggregateType};
use modbit_domain::task::{Task, TaskEvent};
use modbit_domain::{CheckpointId, Timestamp};
use modbit_event_store::{EventStore, ObjectStore};
use modbit_protocol::v1 as wire;
use modbit_workspace::{ChangeOp, ChangeOpKind, WritePrecondition, content_hash};

use crate::runtime::{Lineage, append, typed};
use crate::server::Core;
use crate::tools::{append_file_events, file_changed_events, read_workspace_file};

/// Deltas between baselines (docs/19: "periodic full baseline +
/// intermediate deltas").
const MAX_DELTAS: usize = 8;

/// Object reads for `modbit_checkpoint::validate`.
struct Objects(ObjectStore);

impl modbit_checkpoint::ObjectSource for Objects {
    fn get(&self, hash: &str) -> Result<Vec<u8>, String> {
        self.0.get(hash).map_err(|e| e.to_string())
    }
}

/// docs/54 fault 9 ("stale checkpoint writer races newer epoch"): hold the
/// commit of the checkpoint whose epoch is named, for the milliseconds
/// named, so a test can commit a newer epoch first. `MODBIT_FAULT_CHECKPOINT_DELAY="<epoch>:<ms>"`;
/// unset in production.
fn commit_delay_for(epoch: u32) -> Option<std::time::Duration> {
    let v = std::env::var("MODBIT_FAULT_CHECKPOINT_DELAY").ok()?;
    let (e, ms) = v.split_once(':')?;
    if e.parse::<u32>().ok()? != epoch {
        return None;
    }
    let ms = ms.parse::<u64>().ok()?;
    (ms > 0).then(|| std::time::Duration::from_millis(ms))
}

/// The worktree's dirty state: every path that differs from HEAD (tracked
/// changes and untracked files) stored as objects, and every tracked path
/// deleted from the worktree as [`modbit_checkpoint::DELETED`].
fn dirty_state(
    objects: &ObjectStore,
    ws: &modbit_workspace::WorkspaceService,
    root: &std::path::Path,
) -> anyhow::Result<(BTreeMap<String, String>, Option<String>)> {
    let repo = modbit_git::Repo::open(root)?;
    let head = repo.head().ok();
    let mut out = BTreeMap::new();
    for e in repo.status()? {
        match read_workspace_file(ws, &e.path) {
            Some(bytes) => {
                out.insert(e.path.clone(), objects.put(&bytes)?);
            }
            None if e.code != "??" => {
                out.insert(e.path.clone(), modbit_checkpoint::DELETED.to_owned());
            }
            None => {}
        }
    }
    Ok((out, head))
}

/// The manifests of a task's committed checkpoints, oldest first.
fn manifests(store: &EventStore, task: &Task) -> Vec<CheckpointManifest> {
    store
        .checkpoints(&task.task_id)
        .unwrap_or_default()
        .into_iter()
        .filter(|r| matches!(r.status.as_str(), "CURRENT" | "SUPERSEDED"))
        .filter_map(|r| {
            let bytes = store
                .objects()
                .get(r.manifest_object_hash.as_deref()?)
                .ok()?;
            serde_json::from_slice::<CheckpointManifest>(&bytes).ok()
        })
        .collect()
}

/// The current checkpoint's manifest, when any.
fn current(store: &EventStore, task: &Task) -> Option<CheckpointManifest> {
    let row = store
        .checkpoints(&task.task_id)
        .unwrap_or_default()
        .into_iter()
        .find(|r| r.status == "CURRENT")?;
    let bytes = store
        .objects()
        .get(row.manifest_object_hash.as_deref()?)
        .ok()?;
    serde_json::from_slice(&bytes).ok()
}

/// The outcome of a capture.
pub(crate) struct Captured {
    /// The manifest (committed or not).
    pub manifest: CheckpointManifest,
    /// Committed, or refused as stale.
    pub committed: Result<(), StaleCheckpoint>,
}

/// Take a checkpoint of the task's worktree and runtime cursor: claim the
/// next epoch on the log, capture, then commit — or record the refusal when
/// a newer epoch became current in between.
pub(crate) async fn capture(
    core: &Core,
    task: &Task,
    lt: Lineage,
    actor: &Actor,
    kind: Option<CheckpointKind>,
    reason: &str,
) -> anyhow::Result<Captured> {
    let root = task
        .workspace_root
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("task has no workspace root"))?;
    let (ws, canonical) = core.tools.workspace(root).await?;
    // 1. claim the epoch: the next after every started or committed one.
    let (epoch, base, base_state, since_baseline) = {
        let store = core.store.lock().await;
        let rows = store.checkpoints(&task.task_id).unwrap_or_default();
        let epoch = rows.iter().map(|r| r.epoch).max().unwrap_or(0) + 1;
        let all = manifests(&store, task);
        let cur = current(&store, task);
        let since_baseline = cur
            .as_ref()
            .and_then(|c| chain_to(&all, c.checkpoint_id).ok())
            .map_or(0, |chain| chain.len());
        let base_state = cur
            .as_ref()
            .and_then(|c| chain_to(&all, c.checkpoint_id).ok())
            .and_then(|chain| materialize(&chain).ok())
            .map(|m| m.files);
        (epoch, cur, base_state, since_baseline)
    };
    let kind = kind.unwrap_or(if needs_baseline(since_baseline, MAX_DELTAS) {
        CheckpointKind::Baseline
    } else {
        CheckpointKind::Delta
    });
    let base = match kind {
        CheckpointKind::Baseline => None,
        CheckpointKind::Delta => base.zip(base_state),
    };
    let kind = if base.is_none() {
        CheckpointKind::Baseline
    } else {
        CheckpointKind::Delta
    };
    let checkpoint_id = CheckpointId::new();
    {
        let mut store = core.store.lock().await;
        append(
            &mut store,
            core,
            lt,
            AggregateType::Task,
            *task.task_id.as_bytes(),
            vec![typed(
                "CheckpointStarted",
                &TaskEvent::CheckpointStarted {
                    checkpoint_id: checkpoint_id.to_string(),
                    epoch,
                    kind: kind_label(kind).to_owned(),
                    base_checkpoint_id: base.as_ref().map(|(b, _)| b.checkpoint_id.to_string()),
                    reason: reason.to_owned(),
                },
                actor.clone(),
            )],
        )
        .map_err(|e| anyhow::anyhow!(e))?;
    }
    // 2. capture the worktree and the runtime cursor.
    let (dirty, git_head, workspace_revision, worktree_id) = {
        let ws = ws.lock().await;
        let objects = core.store.lock().await.objects().clone();
        let (dirty, head) = dirty_state(&objects, &ws, &canonical)?;
        let rev = ws.revision();
        (dirty, head, rev.number, rev.worktree_id.clone())
    };
    let (runtime, index_generation) = {
        let store = core.store.lock().await;
        let compaction_epoch = store
            .compaction_epochs(&task.task_id)
            .unwrap_or_default()
            .into_iter()
            .filter(|r| r.status == "COMMITTED")
            .map(|r| r.epoch)
            .max()
            .unwrap_or(0);
        let branch_generation = store
            .session(&task.session_id)
            .ok()
            .flatten()
            .map_or(0, |s| s.branch_generation);
        let protocol_digest = crate::protocol::reconstruct(&store, &task.task_id).digest();
        (
            RuntimeCursor {
                event_offset: store.last_offset().unwrap_or(0),
                compaction_epoch,
                branch_generation,
                protocol_digest,
            },
            workspace_revision,
        )
    };
    let manifest = build_manifest(&CaptureRequest {
        checkpoint_id,
        task_id: task.task_id,
        epoch,
        dirty: &dirty,
        base: base.as_ref().map(|(b, s)| (b, s)),
        workspace_revision,
        git_head,
        worktree_id,
        runtime,
        index_generation,
        now: Timestamp::now(),
        reason,
    });
    let manifest_ref = core
        .store
        .lock()
        .await
        .objects()
        .put(&serde_json::to_vec(&manifest)?)?;
    // docs/54 fault 9: a held writer.
    if let Some(d) = commit_delay_for(epoch) {
        tokio::time::sleep(d).await;
    }
    // 3. commit, fenced twice: by the rule here and by the projection.
    let committed = {
        let mut store = core.store.lock().await;
        let current_epoch = store
            .checkpoints(&task.task_id)
            .unwrap_or_default()
            .into_iter()
            .filter(|r| matches!(r.status.as_str(), "CURRENT" | "SUPERSEDED"))
            .map(|r| r.epoch)
            .max();
        match accept_commit(&manifest, current_epoch) {
            Ok(()) => {
                let result = append(
                    &mut store,
                    core,
                    lt,
                    AggregateType::Task,
                    *task.task_id.as_bytes(),
                    vec![typed(
                        "CheckpointCommitted",
                        &TaskEvent::CheckpointCommitted {
                            checkpoint_id: checkpoint_id.to_string(),
                            epoch,
                            kind: kind_label(kind).to_owned(),
                            base_checkpoint_id: manifest.base_checkpoint_id.map(|b| b.to_string()),
                            manifest_ref: manifest_ref.clone(),
                            integrity_hash: manifest.integrity_hash.clone(),
                            workspace_revision: manifest.workspace_revision,
                            git_head: manifest.git_head.clone(),
                            files: u32::try_from(manifest.files.len()).unwrap_or(u32::MAX),
                            removed: u32::try_from(manifest.removed.len()).unwrap_or(u32::MAX),
                            event_offset: manifest.runtime.event_offset,
                            index_generation: manifest.index_generation,
                        },
                        actor.clone(),
                    )],
                );
                match result {
                    Ok(_) => Ok(()),
                    Err(e) => {
                        eprintln!(
                            "modbit-core: checkpoint {checkpoint_id} refused by the store: {e}"
                        );
                        Err(StaleCheckpoint::NotNewer {
                            current: current_epoch.unwrap_or(0).max(epoch),
                            offered: epoch,
                        })
                    }
                }
            }
            Err(stale) => Err(stale),
        }
    };
    if let Err(stale) = &committed {
        let (current_epoch, reason) = match stale {
            StaleCheckpoint::NotNewer { current, .. } => (*current, "a newer epoch is current"),
            StaleCheckpoint::IntegrityMismatch { .. } => (0, "manifest integrity mismatch"),
        };
        let mut store = core.store.lock().await;
        let _ = append(
            &mut store,
            core,
            lt,
            AggregateType::Task,
            *task.task_id.as_bytes(),
            vec![typed(
                "CheckpointRejectedStale",
                &TaskEvent::CheckpointRejectedStale {
                    checkpoint_id: checkpoint_id.to_string(),
                    epoch,
                    current_epoch,
                    reason: reason.to_owned(),
                },
                actor.clone(),
            )],
        );
    }
    Ok(Captured {
        manifest,
        committed,
    })
}

/// Why a restore did not happen.
#[derive(Debug)]
pub(crate) struct RestoreRefused {
    /// Stable code.
    pub code: &'static str,
    /// Detail.
    pub detail: String,
}

/// What a restore did.
pub(crate) struct Restored {
    /// Target.
    pub checkpoint_id: CheckpointId,
    /// Epoch.
    pub epoch: u32,
    /// Chain walked.
    pub chain: Vec<CheckpointId>,
    /// Files written from objects.
    pub files_written: u32,
    /// Dirty paths returned to HEAD or removed.
    pub files_reverted: u32,
    /// Workspace revision after.
    pub workspace_revision_after: u64,
    /// The restored runtime cursor.
    pub event_offset: u64,
}

/// Restore the worktree to a checkpoint: resolve its chain from the log,
/// read back and hash-check every object it names, and only then write —
/// files the checkpoint has from their objects, files it does not have back
/// to HEAD (tracked) or away (untracked) — in one workspace transaction.
pub(crate) async fn restore(
    core: &Core,
    task: &Task,
    lt: Lineage,
    actor: &Actor,
    target: Option<CheckpointId>,
) -> anyhow::Result<Result<Restored, RestoreRefused>> {
    let root = task
        .workspace_root
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("task has no workspace root"))?;
    let (ws, canonical) = core.tools.workspace(root).await?;
    let (chain, objects) = {
        let store = core.store.lock().await;
        let all = manifests(&store, task);
        let target = match target.or_else(|| current(&store, task).map(|c| c.checkpoint_id)) {
            Some(t) => t,
            None => {
                return Ok(Err(RestoreRefused {
                    code: "NO_CHECKPOINT",
                    detail: "the task has no committed checkpoint".into(),
                }));
            }
        };
        match chain_to(&all, target) {
            Ok(c) => (c, store.objects().clone()),
            Err(e) => return Ok(Err(chain_refusal(e))),
        }
    };
    let state = match materialize(&chain) {
        Ok(s) => s,
        Err(e) => return Ok(Err(chain_refusal(e))),
    };
    // Every object, read back and digest-checked, before anything is written.
    let bytes = match validate(&state, &Objects(objects.clone())) {
        Ok(b) => b,
        Err(e) => return Ok(Err(chain_refusal(e))),
    };
    let repo = modbit_git::Repo::open(&canonical)?;
    let mut ws = ws.lock().await;
    let mut ops: Vec<ChangeOp> = Vec::new();
    let mut pre_bytes: HashMap<String, Option<Vec<u8>>> = HashMap::new();
    let mut reverted = 0u32;
    // Paths dirty now that the checkpoint does not have: back to HEAD, or gone.
    for e in repo.status()? {
        if state.files.contains_key(&e.path) {
            continue;
        }
        let current = read_workspace_file(&ws, &e.path);
        pre_bytes.insert(e.path.clone(), current.clone());
        if e.code == "??" {
            if current.is_some() {
                ops.push(ChangeOp {
                    path: e.path.clone(),
                    kind: ChangeOpKind::Delete,
                    pre: WritePrecondition::default(),
                });
                reverted += 1;
            }
        } else if let Ok(head_bytes) = repo.show("HEAD", &e.path) {
            let kind = if current.is_some() {
                ChangeOpKind::Replace(head_bytes)
            } else {
                ChangeOpKind::Create(head_bytes)
            };
            ops.push(ChangeOp {
                path: e.path.clone(),
                kind,
                pre: WritePrecondition::default(),
            });
            reverted += 1;
        }
    }
    let mut written = 0u32;
    // Paths the checkpoint has as deleted: gone.
    for (path, hash) in &state.files {
        if hash == modbit_checkpoint::DELETED && read_workspace_file(&ws, path).is_some() {
            pre_bytes.insert(path.clone(), read_workspace_file(&ws, path));
            ops.push(ChangeOp {
                path: path.clone(),
                kind: ChangeOpKind::Delete,
                pre: WritePrecondition::default(),
            });
            written += 1;
        }
    }
    for (path, content) in &bytes {
        let current = read_workspace_file(&ws, path);
        if current
            .as_deref()
            .is_some_and(|c| content_hash(c) == state.files[path])
        {
            continue;
        }
        pre_bytes.insert(path.clone(), current.clone());
        let kind = if current.is_some() {
            ChangeOpKind::Replace(content.clone())
        } else {
            ChangeOpKind::Create(content.clone())
        };
        ops.push(ChangeOp {
            path: path.clone(),
            kind,
            pre: WritePrecondition::default(),
        });
        written += 1;
    }
    let pre_revision = ws.revision().number;
    if !ops.is_empty() {
        let changes = ws
            .apply_transaction(&ops)
            .map_err(|e| anyhow::anyhow!("restore transaction: {e}"))?;
        let value = serde_json::json!({ "changes": changes });
        let events = file_changed_events(
            &objects,
            &ws,
            task.task_id,
            modbit_domain::ToolCallId::new(),
            &crate::tools::workspace_changes(&value),
            &pre_bytes,
            pre_revision,
            "checkpoint:",
        );
        let mut st = core.store.lock().await;
        append_file_events(
            &mut st,
            core.tenant_id,
            task.session_id,
            task.task_id,
            &canonical.to_string_lossy(),
            events,
        )?;
    }
    let after = ws.revision().number;
    drop(ws);
    {
        let mut store = core.store.lock().await;
        let _ = append(
            &mut store,
            core,
            lt,
            AggregateType::Task,
            *task.task_id.as_bytes(),
            vec![typed(
                "CheckpointRestored",
                &TaskEvent::CheckpointRestored {
                    checkpoint_id: state.checkpoint_id.to_string(),
                    epoch: state.epoch,
                    chain: state.chain.iter().map(ToString::to_string).collect(),
                    files_written: written,
                    files_reverted: reverted,
                    workspace_revision_after: after,
                    event_offset: state.runtime.event_offset,
                },
                actor.clone(),
            )],
        );
    }
    Ok(Ok(Restored {
        checkpoint_id: state.checkpoint_id,
        epoch: state.epoch,
        chain: state.chain,
        files_written: written,
        files_reverted: reverted,
        workspace_revision_after: after,
        event_offset: state.runtime.event_offset,
    }))
}

fn chain_refusal(e: ChainError) -> RestoreRefused {
    let code = match &e {
        ChainError::NoBaseline => "NO_BASELINE",
        ChainError::BrokenLink { .. } => "BROKEN_LINK",
        ChainError::EpochOrder { .. } => "EPOCH_ORDER",
        ChainError::ObjectMismatch { .. } => "OBJECT_MISMATCH",
        ChainError::IntegrityMismatch { .. } => "INTEGRITY_MISMATCH",
    };
    RestoreRefused {
        code,
        detail: format!("{e:?}"),
    }
}

/// Stable label for a kind.
pub(crate) const fn kind_label(kind: CheckpointKind) -> &'static str {
    match kind {
        CheckpointKind::Baseline => "BASELINE",
        CheckpointKind::Delta => "DELTA",
    }
}

/// Parse a kind label; `None` for empty (the engine decides).
pub(crate) fn kind_of(label: &str) -> Result<Option<CheckpointKind>, String> {
    match label {
        "" => Ok(None),
        "BASELINE" => Ok(Some(CheckpointKind::Baseline)),
        "DELTA" => Ok(Some(CheckpointKind::Delta)),
        other => Err(format!("unknown checkpoint kind `{other}`")),
    }
}

/// Wire view of a checkpoint row.
pub(crate) fn view(r: &modbit_event_store::projections::CheckpointRow) -> wire::CheckpointView {
    let git_head = r
        .git_state_json
        .as_deref()
        .and_then(|j| serde_json::from_str::<serde_json::Value>(j).ok())
        .and_then(|v| v["head"].as_str().map(str::to_owned))
        .unwrap_or_default();
    let event_offset = r
        .runtime_state_ref
        .as_deref()
        .and_then(|s| s.strip_prefix("offset:"))
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    wire::CheckpointView {
        checkpoint_id: r.checkpoint_id.clone(),
        epoch: r.epoch,
        kind: r.kind.clone(),
        base_checkpoint_id: r.base_checkpoint_id.clone().unwrap_or_default(),
        status: r.status.clone(),
        workspace_revision: r.workspace_revision,
        manifest_ref: r.manifest_object_hash.clone().unwrap_or_default(),
        integrity_hash: r.integrity_hash.clone().unwrap_or_default(),
        git_head,
        files: r.files,
        removed: r.removed,
        event_offset,
        index_generation: r.index_generation,
        reason: r.reason.clone(),
        created_at_ms: r.created_at.0,
        committed_at_ms: r.committed_at.map_or(0, |t| t.0),
    }
}

/// The wire view of a task's checkpoints.
pub(crate) fn list(store: &EventStore, task: &Task) -> wire::CheckpointList {
    let rows = store.checkpoints(&task.task_id).unwrap_or_default();
    let current = rows.iter().find(|r| r.status == "CURRENT");
    wire::CheckpointList {
        current_checkpoint_id: current.map(|r| r.checkpoint_id.clone()).unwrap_or_default(),
        current_epoch: current.map_or(0, |r| r.epoch),
        checkpoints: rows.iter().map(view).collect(),
    }
}
