//! Workspace checkpoints in the Core (docs/19 "Checkpoint epochs", docs/31
//! `checkpoints`, docs/54 fault 9; M4.3, REQ-EV-0012/0013): capture the
//! worktree's dirty state into content-addressed objects as a baseline or a
//! delta under a monotonic epoch, commit it only while it is newer than the
//! current checkpoint, and restore a validated chain — every object read
//! back and hash-checked before a byte is written.

//!
//! REQ-PX-061 / REQ-PX-102 (docs/62, docs/65) extend the same owner: a delta
//! checkpoint at every turn boundary of every profile with a bounded capture
//! cost (see "Capture cost" below), a restore that records a pre-restore
//! checkpoint first so it is exactly reversible (redo), an intent journal
//! that rolls a restore a dead Core left half-written back to that
//! checkpoint, names, and the ledger the turn-addressed fork and the
//! retention collector (`checkpoint_gc`) read.
//!
//! # Capture cost
//!
//! A capture reads the repository's status (git's own stat cache: cost
//! proportional to the files git must re-stat, not to their size) and then,
//! for each dirty path, the file. A path whose size and mtime are the ones
//! of an earlier capture, and that was not racy when it was hashed (the rule
//! git and the FIX-06 snapshot share: a file modified within two seconds of
//! the moment it was hashed is always read again), is not read: its hash is
//! taken from the cache. A turn that changed nothing therefore costs one
//! `git status`, one `stat` and one object existence check per dirty path,
//! writes no content blob, and stores a delta manifest that lists nothing.
//! The work is bounded: a worktree with more than [`MAX_DIRTY_FILES`] dirty
//! paths, or more than [`MAX_CAPTURE_BYTES`] of content to read, is refused
//! (`OVER_BOUNDS`) and the turn records why instead of half a checkpoint. The
//! cost of every commit is on its `CheckpointCommitted` event.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{Arc, Mutex as StdMutex, OnceLock};

use modbit_checkpoint::{
    CaptureRequest, ChainError, CheckpointKind, CheckpointManifest, RuntimeCursor, StaleCheckpoint,
    accept_commit, capture as build_manifest, chain_to, materialize, needs_baseline, validate,
};
use modbit_domain::event::{Actor, AggregateType};
use modbit_domain::task::{Task, TaskEvent};
use modbit_domain::{CheckpointId, Timestamp};
use modbit_event_store::{CommandRecord, EventStore, ObjectStore};
use modbit_protocol::v1 as wire;
use modbit_workspace::{ChangeOp, ChangeOpKind, WritePrecondition, content_hash};

use crate::runtime::{Lineage, append, typed};
use crate::server::Core;
use crate::tools::{append_file_events, file_changed_events, read_workspace_file};

/// Deltas between baselines (docs/19: "periodic full baseline +
/// intermediate deltas").
const MAX_DELTAS: usize = 8;

/// A capture of a worktree with more dirty paths than this is refused.
pub(crate) const MAX_DIRTY_FILES: usize = 20_000;
/// A capture that would read more content than this is refused.
pub(crate) const MAX_CAPTURE_BYTES: u64 = 256 * 1024 * 1024;

/// A file modified within this long of the moment its hash was taken is
/// "racy" (the same rule as `modbit_workspace::snapshot`): a rewrite that
/// keeps its size and mtime cannot be excluded, so it is always read again.
const RACY_NS: u128 = 2_000_000_000;

/// What one capture cost.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct CaptureStats {
    /// Wall-clock milliseconds.
    pub capture_ms: u64,
    /// Dirty files read and hashed.
    pub hashed_files: u32,
    /// Dirty files whose hash came from an earlier capture.
    pub cache_hits: u32,
    /// New content blobs written.
    pub blobs_written: u32,
    /// Bytes those blobs hold.
    pub bytes_written: u64,
}

impl CaptureStats {
    /// The wire view.
    pub(crate) fn wire(self) -> wire::CaptureCost {
        wire::CaptureCost {
            capture_ms: self.capture_ms,
            hashed_files: self.hashed_files,
            cache_hits: self.cache_hits,
            blobs_written: self.blobs_written,
            bytes_written: self.bytes_written,
        }
    }
}

struct CacheEntry {
    size: u64,
    mtime_ns: u128,
    hash: String,
    taken_ns: u128,
}

/// path -> what the last capture learned, per canonical worktree root. A
/// pure cache: it can only skip the read of a file that provably has the
/// content it had, and a hash is used only while its blob still exists.
type RootCache = HashMap<String, CacheEntry>;

fn hash_caches() -> &'static StdMutex<HashMap<PathBuf, RootCache>> {
    static CACHES: OnceLock<StdMutex<HashMap<PathBuf, RootCache>>> = OnceLock::new();
    CACHES.get_or_init(Default::default)
}

fn now_ns() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos())
}

// ---- the gate: captures, restores and the collector --------------------

/// Why the collector or a restore may not start now.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct GateRefused {
    /// `GC_LEASE_HELD` | `RESTORE_IN_FLIGHT` | `GC_IN_FLIGHT`.
    pub code: &'static str,
    /// Detail.
    pub detail: String,
}

/// Who holds the collector's lease.
#[derive(Clone, Debug)]
pub(crate) struct GcLease {
    /// Owner.
    pub owner: String,
    /// Why it runs.
    pub reason: String,
    /// Since when.
    pub since: Timestamp,
}

#[derive(Default)]
struct GateState {
    restores: usize,
    gc: Option<GcLease>,
}

/// Coordinates the three things that must not overlap on one profile's
/// checkpoint store: captures (many, concurrent), restores (counted), and the
/// retention collector (one, exclusive of both).
#[derive(Default)]
pub(crate) struct Gate {
    /// Captures hold the read side from the first blob to the commit; the
    /// collector's sweep holds the write side, so a blob a capture is about
    /// to name is never deleted under it.
    pub(crate) capture: tokio::sync::RwLock<()>,
    state: StdMutex<GateState>,
}

/// A restore in flight; dropping it ends it.
pub(crate) struct RestoreGuard(Arc<Gate>);

impl Drop for RestoreGuard {
    fn drop(&mut self) {
        if let Ok(mut s) = self.0.state.lock() {
            s.restores = s.restores.saturating_sub(1);
        }
    }
}

/// The collector's lease; dropping it releases it.
pub(crate) struct GcGuard(Arc<Gate>);

impl Drop for GcGuard {
    fn drop(&mut self) {
        if let Ok(mut s) = self.0.state.lock() {
            s.gc = None;
        }
    }
}

impl Gate {
    /// A restore begins; refused while the collector holds its lease.
    pub(crate) fn begin_restore(self: &Arc<Self>) -> Result<RestoreGuard, GateRefused> {
        let mut s = self.state.lock().expect("gate");
        if let Some(l) = &s.gc {
            return Err(GateRefused {
                code: "GC_IN_FLIGHT",
                detail: format!(
                    "the checkpoint collector ({}: {}) is running; restore once it ends",
                    l.owner, l.reason
                ),
            });
        }
        s.restores += 1;
        Ok(RestoreGuard(Arc::clone(self)))
    }

    /// The collector takes its lease; refused while one is held or a restore
    /// is in flight.
    pub(crate) fn begin_gc(
        self: &Arc<Self>,
        owner: &str,
        reason: &str,
    ) -> Result<GcGuard, GateRefused> {
        let mut s = self.state.lock().expect("gate");
        if let Some(l) = &s.gc {
            return Err(GateRefused {
                code: "GC_LEASE_HELD",
                detail: format!(
                    "the collector's lease is held by {} ({}) since {}",
                    l.owner, l.reason, l.since.0
                ),
            });
        }
        if s.restores > 0 {
            return Err(GateRefused {
                code: "RESTORE_IN_FLIGHT",
                detail: format!(
                    "{} restore(s) are in flight; a collection never runs under one",
                    s.restores
                ),
            });
        }
        s.gc = Some(GcLease {
            owner: owner.to_owned(),
            reason: reason.to_owned(),
            since: Timestamp::now(),
        });
        Ok(GcGuard(Arc::clone(self)))
    }
}

/// The gate of this Core's profile (one Core per profile directory).
pub(crate) fn gate(core: &Core) -> Arc<Gate> {
    static GATES: OnceLock<StdMutex<HashMap<PathBuf, Arc<Gate>>>> = OnceLock::new();
    let mut m = GATES.get_or_init(Default::default).lock().expect("gates");
    Arc::clone(m.entry(core.data_dir.clone()).or_default())
}

/// Failure injection for the proofs of REQ-PX-061/102: abort the process at
/// a named point, as a `SIGKILL` there would. `MODBIT_FAULT_CHECKPOINT` names
/// the point (`MID_CAPTURE`, `AFTER_PRE_RESTORE`, `AFTER_RESTORE_JOURNAL`,
/// `MID_RESTORE_WRITE`, `AFTER_GC_COLLECT`, `MID_GC_SWEEP`); unset in
/// production.
pub(crate) fn fault_point(point: &str) {
    if std::env::var("MODBIT_FAULT_CHECKPOINT").is_ok_and(|v| v == point) {
        eprintln!("modbit-core: fault injection: aborting at {point}");
        std::process::abort();
    }
}

/// Object reads for `modbit_checkpoint::validate`.
pub(crate) struct Objects(pub(crate) ObjectStore);

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
) -> anyhow::Result<(BTreeMap<String, String>, Option<String>, CaptureStats)> {
    let repo = modbit_git::Repo::open(root)?;
    let head = repo.head().ok();
    let entries = repo.status()?;
    if entries.len() > MAX_DIRTY_FILES {
        anyhow::bail!(
            "OVER_BOUNDS: the worktree has {} dirty paths; a checkpoint is bounded at {MAX_DIRTY_FILES}",
            entries.len()
        );
    }
    let mut stats = CaptureStats::default();
    let mut cache: RootCache = hash_caches()
        .lock()
        .ok()
        .and_then(|mut m| m.remove(root))
        .unwrap_or_default();
    let mut out = BTreeMap::new();
    let mut read_bytes = 0u64;
    for e in entries {
        // A regular file with a known size and mtime may come from the
        // cache; anything else (a symlink, a directory, an unreadable path)
        // takes the ordinary read.
        let meta = ws
            .resolve(&e.path)
            .ok()
            .and_then(|r| std::fs::symlink_metadata(&r.absolute).ok())
            .filter(std::fs::Metadata::is_file);
        let stamp = meta.as_ref().and_then(|m| {
            let mtime = m
                .modified()
                .ok()?
                .duration_since(std::time::UNIX_EPOCH)
                .ok()?
                .as_nanos();
            Some((m.len(), mtime))
        });
        if let Some((size, mtime)) = stamp
            && let Some(c) = cache.get(&e.path)
            && c.size == size
            && c.mtime_ns == mtime
            && c.mtime_ns.saturating_add(RACY_NS) < c.taken_ns
            && objects.contains(&c.hash)
        {
            out.insert(e.path.clone(), c.hash.clone());
            stats.cache_hits += 1;
            continue;
        }
        match read_workspace_file(ws, &e.path) {
            Some(bytes) => {
                read_bytes += bytes.len() as u64;
                if read_bytes > MAX_CAPTURE_BYTES {
                    anyhow::bail!(
                        "OVER_BOUNDS: the dirty files exceed {} MiB; a checkpoint is bounded",
                        MAX_CAPTURE_BYTES / (1024 * 1024)
                    );
                }
                let hash = modbit_event_store::objects::sha256_hex(&bytes);
                if !objects.contains(&hash) {
                    objects.put(&bytes)?;
                    stats.blobs_written += 1;
                    stats.bytes_written += bytes.len() as u64;
                }
                stats.hashed_files += 1;
                if let Some((size, mtime)) = stamp {
                    cache.insert(
                        e.path.clone(),
                        CacheEntry {
                            size,
                            mtime_ns: mtime,
                            hash: hash.clone(),
                            taken_ns: now_ns(),
                        },
                    );
                }
                out.insert(e.path.clone(), hash);
            }
            None if e.code != "??" => {
                out.insert(e.path.clone(), modbit_checkpoint::DELETED.to_owned());
            }
            None => {}
        }
    }
    // Only paths that are dirty now stay remembered: the cache is bounded by
    // the dirty set, never by the history of the task.
    cache.retain(|p, _| out.contains_key(p));
    if let Ok(mut m) = hash_caches().lock() {
        m.insert(root.to_path_buf(), cache);
    }
    Ok((out, head, stats))
}

/// M8.9 (docs/21 "Sandbox recovery"): the dirty state of a cloud task's
/// worktree — the guest's `/workspace` against the seed it was provisioned
/// from (the host copy at the task's root): every file whose content
/// differs or is new goes in as an object, every seed file the guest no
/// longer has as [`modbit_checkpoint::DELETED`]. One hashed listing from
/// the guest, then the reads of what changed. Bounded: a worktree beyond
/// the limits is refused rather than half-captured.
async fn dirty_state_sandbox(
    objects: &ObjectStore,
    sandbox: &dyn modbit_sandbox::port::SandboxPort,
    task_id: &str,
    seed_root: &std::path::Path,
) -> anyhow::Result<(BTreeMap<String, String>, Option<String>, CaptureStats)> {
    const MAX_FILES: usize = 20_000;
    const MAX_BYTES: u64 = 256 * 1024 * 1024;
    let repo = modbit_git::Repo::open(seed_root)?;
    let head = repo.head().ok();
    let snap = sandbox
        .fs_snapshot(task_id, "")
        .await
        .map_err(|e| anyhow::anyhow!("the sandbox's worktree could not be listed: {e}"))?;
    if snap.truncated || snap.entries.len() > MAX_FILES {
        anyhow::bail!(
            "the sandbox's worktree has more than {MAX_FILES} files; a checkpoint of it is refused"
        );
    }
    // The seed, hashed the same way.
    let mut seed: HashMap<String, String> = HashMap::new();
    let mut stack = vec![seed_root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&dir) else {
            continue;
        };
        for e in rd.flatten() {
            let Ok(ft) = e.file_type() else { continue };
            if ft.is_dir() {
                if e.file_name() != ".git" {
                    stack.push(e.path());
                }
                continue;
            }
            if !ft.is_file() {
                continue;
            }
            let Ok(bytes) = std::fs::read(e.path()) else {
                continue;
            };
            let rel = e
                .path()
                .strip_prefix(seed_root)
                .map(|p| p.to_string_lossy().replace('\\', "/"))
                .unwrap_or_default();
            seed.insert(rel, content_hash(&bytes));
        }
    }
    let mut out = BTreeMap::new();
    let mut stats = CaptureStats::default();
    let mut bytes_read = 0u64;
    for entry in &snap.entries {
        if seed.get(&entry.path).is_some_and(|h| *h == entry.sha256) {
            continue;
        }
        bytes_read += entry.size;
        if bytes_read > MAX_BYTES {
            anyhow::bail!(
                "the sandbox's changed files exceed {} MiB; a checkpoint of them is refused",
                MAX_BYTES / (1024 * 1024)
            );
        }
        let guest_path = format!(
            "{}/{}",
            sandbox.identity().workspace_root.trim_end_matches('/'),
            entry.path
        );
        let f = sandbox
            .read_file(task_id, &guest_path, 0)
            .await
            .map_err(|e| anyhow::anyhow!("reading `{}` from the sandbox: {e}", entry.path))?;
        if f.truncated {
            anyhow::bail!(
                "`{}` in the sandbox exceeds the read bound; a checkpoint of it is refused",
                entry.path
            );
        }
        let hash = modbit_event_store::objects::sha256_hex(&f.content);
        if !objects.contains(&hash) {
            objects.put(&f.content)?;
            stats.blobs_written += 1;
            stats.bytes_written += f.content.len() as u64;
        }
        stats.hashed_files += 1;
        out.insert(entry.path.clone(), hash);
    }
    let present: std::collections::HashSet<&str> =
        snap.entries.iter().map(|e| e.path.as_str()).collect();
    for path in seed.keys() {
        if !present.contains(path.as_str()) {
            out.insert(path.clone(), modbit_checkpoint::DELETED.to_owned());
        }
    }
    Ok((out, head, stats))
}

/// The manifests of a task's committed checkpoints, oldest first.
pub(crate) fn manifests(store: &EventStore, task: &Task) -> Vec<CheckpointManifest> {
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

/// The manifests of the chain that ends at `target`, oldest first: the
/// baseline it descends from and every delta between, read by walking the
/// rows' `base_checkpoint_id` links — only the chain's own manifests are
/// read, so the cost of a capture is bounded by [`MAX_DELTAS`], not by how
/// many checkpoints the task has accumulated. `None` when a link or a
/// manifest object is missing.
pub(crate) fn chain_manifests(
    store: &EventStore,
    rows: &[modbit_event_store::projections::CheckpointRow],
    target: &str,
) -> Option<Vec<CheckpointManifest>> {
    let by_id: HashMap<&str, &modbit_event_store::projections::CheckpointRow> = rows
        .iter()
        .filter(|r| matches!(r.status.as_str(), "CURRENT" | "SUPERSEDED"))
        .map(|r| (r.checkpoint_id.as_str(), r))
        .collect();
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    let mut cursor = Some(target.to_owned());
    while let Some(id) = cursor {
        if !seen.insert(id.clone()) {
            return None;
        }
        let row = by_id.get(id.as_str())?;
        let bytes = store
            .objects()
            .get(row.manifest_object_hash.as_deref()?)
            .ok()?;
        let m: CheckpointManifest = serde_json::from_slice(&bytes).ok()?;
        cursor = m.base_checkpoint_id.map(|b| b.to_string());
        out.push(m);
    }
    out.reverse();
    Some(out)
}

/// The current checkpoint's manifest, when any.
pub(crate) fn current(store: &EventStore, task: &Task) -> Option<CheckpointManifest> {
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

/// What a capture is for, beyond the worktree it records.
#[derive(Clone, Debug, Default)]
pub(crate) struct CaptureMeta {
    /// The turn whose boundary this is, with its ordinal in its run.
    pub turn: Option<(modbit_domain::TurnId, u32)>,
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
    capture_with(core, task, lt, actor, kind, reason, CaptureMeta::default()).await
}

/// [`capture`], with the turn it is the boundary of (REQ-PX-061).
pub(crate) async fn capture_with(
    core: &Core,
    task: &Task,
    lt: Lineage,
    actor: &Actor,
    kind: Option<CheckpointKind>,
    reason: &str,
    meta: CaptureMeta,
) -> anyhow::Result<Captured> {
    let started = std::time::Instant::now();
    let root = task
        .workspace_root
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("task has no workspace root"))?;
    // The collector's sweep never runs between this capture's first blob and
    // its commit.
    let g = gate(core);
    let _capturing = g.capture.read().await;
    let (ws, canonical) = core.tools.workspace(root).await?;
    // 1. claim the epoch: the next after every started or committed one.
    let (epoch, base, base_state, since_baseline) = {
        let store = core.store.lock().await;
        let rows = store.checkpoints(&task.task_id).unwrap_or_default();
        let epoch = rows.iter().map(|r| r.epoch).max().unwrap_or(0) + 1;
        let cur_row = rows.iter().find(|r| r.status == "CURRENT");
        let chain = cur_row.and_then(|r| chain_manifests(&store, &rows, &r.checkpoint_id));
        let since_baseline = chain.as_ref().map_or(0, Vec::len);
        let cur = chain.as_ref().and_then(|c| c.last().cloned());
        let base_state = chain
            .as_ref()
            .and_then(|c| materialize(c).ok())
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
    // 2. capture the worktree and the runtime cursor — the guest's worktree
    // when the task runs in a sandbox (M8.9), the host's otherwise.
    let sandbox = core
        .tools
        .sandboxes
        .lock()
        .await
        .get(&task.task_id)
        .cloned();
    let (dirty, git_head, workspace_revision, worktree_id, mut cost) = {
        let ws = ws.lock().await;
        let objects = core.store.lock().await.objects().clone();
        let (dirty, head, cost) = match &sandbox {
            Some(h) => {
                dirty_state_sandbox(&objects, &**h, &task.task_id.to_string(), &canonical).await?
            }
            None => dirty_state(&objects, &ws, &canonical)?,
        };
        let rev = ws.revision();
        (dirty, head, rev.number, rev.worktree_id.clone(), cost)
    };
    fault_point("MID_CAPTURE");
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
    cost.capture_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
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
                let (turn_id, turn_ordinal) = meta
                    .turn
                    .map(|(t, o)| (t.to_string(), o))
                    .unwrap_or_default();
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
                            turn_id,
                            turn_ordinal,
                            capture_ms: cost.capture_ms,
                            hashed_files: cost.hashed_files,
                            cache_hits: cost.cache_hits,
                            blobs_written: cost.blobs_written,
                            bytes_written: cost.bytes_written,
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

// ---- the ledger -----------------------------------------------------------

/// One committed checkpoint as the log records it.
#[derive(Clone, Debug)]
pub(crate) struct Commit {
    /// The checkpoint.
    pub checkpoint_id: String,
    /// The turn whose boundary it is (UUID text), empty when not a turn's.
    pub turn_id: String,
    /// That turn's ordinal in its run; 0 when not a turn's.
    pub turn_ordinal: u32,
    /// Log offset of the commit.
    pub offset: u64,
    /// What the capture cost.
    pub cost: CaptureStats,
}

/// Where a restore stands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RestoreState {
    /// Journalled, neither finished nor rolled back: a Core died inside it,
    /// or it is running now.
    InDoubt,
    /// Finished; the offset of `CheckpointRestored`.
    Done(u64),
    /// Resolved by restoring the pre-restore checkpoint.
    RolledBack,
}

/// One restore as the log records it.
#[derive(Clone, Debug)]
pub(crate) struct RestoreRec {
    /// The `RestoreCheckpoint` command (UUID text).
    pub command_id: String,
    /// The pre-restore checkpoint, empty for a restore that changed nothing.
    pub pre_restore: String,
    /// The checkpoint restored to.
    pub target: String,
    /// It was a redo.
    pub redo: bool,
    /// Where it stands.
    pub state: RestoreState,
}

/// What a task's own log says about its checkpoints beyond the projection:
/// which turn each belongs to, the names, what was collected, the restores.
/// Derived by reading only the checkpoint event types of the task's
/// aggregate.
#[derive(Clone, Debug, Default)]
pub(crate) struct Ledger {
    /// Commits, oldest first.
    pub commits: Vec<Commit>,
    /// Checkpoint id -> name.
    pub names: BTreeMap<String, String>,
    /// Checkpoint ids taken out of the restorable set.
    pub collected: BTreeSet<String>,
    /// Restores, oldest first.
    pub restores: Vec<RestoreRec>,
}

const LEDGER_TYPES: &[&str] = &[
    "CheckpointCommitted",
    "CheckpointNamed",
    "CheckpointCollected",
    "CheckpointRestoreStarted",
    "CheckpointRestored",
    "CheckpointRestoreRolledBack",
];

/// Read the ledger of a task.
pub(crate) fn ledger(store: &EventStore, task: &Task) -> Ledger {
    let mut l = Ledger::default();
    let events = store
        .read_aggregate_of_types(task.task_id.as_bytes(), LEDGER_TYPES, 0)
        .unwrap_or_default();
    for e in events {
        let Ok(p) = store.payload(&e.envelope) else {
            continue;
        };
        let s = |k: &str| p[k].as_str().unwrap_or_default().to_owned();
        match e.envelope.event_type.as_str() {
            "CheckpointCommitted" => l.commits.push(Commit {
                checkpoint_id: s("checkpoint_id"),
                turn_id: s("turn_id"),
                turn_ordinal: p["turn_ordinal"].as_u64().unwrap_or(0) as u32,
                offset: e.offset,
                cost: CaptureStats {
                    capture_ms: p["capture_ms"].as_u64().unwrap_or(0),
                    hashed_files: p["hashed_files"].as_u64().unwrap_or(0) as u32,
                    cache_hits: p["cache_hits"].as_u64().unwrap_or(0) as u32,
                    blobs_written: p["blobs_written"].as_u64().unwrap_or(0) as u32,
                    bytes_written: p["bytes_written"].as_u64().unwrap_or(0),
                },
            }),
            "CheckpointNamed" => {
                // One name per checkpoint, the latest wins; a name moves
                // only if its previous holder is renamed.
                l.names.insert(s("checkpoint_id"), s("name"));
            }
            "CheckpointCollected" => {
                if let Some(ids) = p["checkpoint_ids"].as_array() {
                    l.collected
                        .extend(ids.iter().filter_map(|i| i.as_str().map(str::to_owned)));
                }
            }
            "CheckpointRestoreStarted" => l.restores.push(RestoreRec {
                command_id: s("command_id"),
                pre_restore: s("pre_restore_checkpoint_id"),
                target: s("target_checkpoint_id"),
                redo: false,
                state: RestoreState::InDoubt,
            }),
            "CheckpointRestored" => {
                let command_id = s("command_id");
                let open = l
                    .restores
                    .iter_mut()
                    .rev()
                    .find(|r| r.command_id == command_id && r.state == RestoreState::InDoubt);
                match open {
                    Some(r) if !command_id.is_empty() => {
                        r.state = RestoreState::Done(e.offset);
                        r.redo = p["redo"].as_bool().unwrap_or(false);
                    }
                    // A restore that changed nothing journals nothing first.
                    _ => l.restores.push(RestoreRec {
                        command_id,
                        pre_restore: s("pre_restore_checkpoint_id"),
                        target: s("checkpoint_id"),
                        redo: p["redo"].as_bool().unwrap_or(false),
                        state: RestoreState::Done(e.offset),
                    }),
                }
            }
            "CheckpointRestoreRolledBack" => {
                let command_id = s("command_id");
                if let Some(r) = l
                    .restores
                    .iter_mut()
                    .rev()
                    .find(|r| r.command_id == command_id && r.state == RestoreState::InDoubt)
                {
                    r.state = RestoreState::RolledBack;
                }
            }
            _ => {}
        }
    }
    l
}

/// The checkpoints other tasks of the session were forked from: ids of
/// `task`'s checkpoints named by a `TaskForked` event (REQ-EV-0077).
pub(crate) fn fork_parents(store: &EventStore, task: &Task) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for e in store
        .read_session_of_types(&task.session_id, &["TaskForked"], 0)
        .unwrap_or_default()
    {
        if let Ok(p) = store.payload(&e.envelope)
            && p["from_task_id"].as_str() == Some(task.task_id.to_string().as_str())
            && let Some(c) = p["from_checkpoint_id"].as_str()
        {
            out.insert(c.to_owned());
        }
    }
    out
}

/// How a caller names the checkpoint it means.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum TargetSpec {
    /// The task's current checkpoint.
    Current,
    /// By id.
    Id(CheckpointId),
    /// By the user's label.
    Name(String),
    /// By the ordinal of the turn whose boundary it is (the latest turn with
    /// that ordinal, runs being separate numberings).
    TurnOrdinal(u32),
    /// By the turn.
    Turn(modbit_domain::TurnId),
}

/// Parse the fields a request names a checkpoint with: at most one.
pub(crate) fn target_spec(
    checkpoint_id: &str,
    name: &str,
    turn_ordinal: u32,
    turn_id: Option<&wire::Id>,
) -> Result<TargetSpec, String> {
    let turn = turn_id.filter(|i| !i.value.is_empty());
    let named = [
        !checkpoint_id.is_empty(),
        !name.is_empty(),
        turn_ordinal != 0,
        turn.is_some(),
    ]
    .into_iter()
    .filter(|b| *b)
    .count();
    if named > 1 {
        return Err(
            "name the checkpoint by at most one of checkpoint_id, name, turn_id and turn_ordinal"
                .into(),
        );
    }
    if !checkpoint_id.is_empty() {
        return CheckpointId::parse(checkpoint_id)
            .map(TargetSpec::Id)
            .map_err(|_| "checkpoint_id is not an id".to_owned());
    }
    if !name.is_empty() {
        return Ok(TargetSpec::Name(name.to_owned()));
    }
    if turn_ordinal != 0 {
        return Ok(TargetSpec::TurnOrdinal(turn_ordinal));
    }
    if let Some(t) = turn {
        let bytes: [u8; 16] = t
            .value
            .as_slice()
            .try_into()
            .map_err(|_| "turn_id must be 16 bytes".to_owned())?;
        return Ok(TargetSpec::Turn(modbit_domain::TurnId::from_bytes(bytes)));
    }
    Ok(TargetSpec::Current)
}

/// Resolve a [`TargetSpec`] to a restorable checkpoint of the task (`None`
/// is the current one). A checkpoint the collector removed is refused with
/// the typed `COLLECTED`; a turn that left none, `NO_CHECKPOINT_FOR_TURN`.
pub(crate) fn resolve_target(
    store: &EventStore,
    task: &Task,
    spec: &TargetSpec,
) -> Result<Option<CheckpointId>, RestoreRefused> {
    let refuse = |code: &'static str, detail: String| Err(RestoreRefused { code, detail });
    let rows = store.checkpoints(&task.task_id).unwrap_or_default();
    let restorable = |id: &str| {
        rows.iter()
            .any(|r| r.checkpoint_id == id && matches!(r.status.as_str(), "CURRENT" | "SUPERSEDED"))
    };
    let collected = |id: &str| {
        rows.iter()
            .any(|r| r.checkpoint_id == id && r.status == "COLLECTED")
    };
    let parse = |id: &str| {
        CheckpointId::parse(id)
            .map(Some)
            .map_err(|_| RestoreRefused {
                code: "BAD_CHECKPOINT",
                detail: format!("`{id}` is not a checkpoint id"),
            })
    };
    match spec {
        TargetSpec::Current => Ok(None),
        TargetSpec::Id(id) => {
            if collected(&id.to_string()) {
                return refuse(
                    "COLLECTED",
                    format!(
                        "checkpoint {id} was removed by the retention collector; it can no longer be restored or forked from"
                    ),
                );
            }
            Ok(Some(*id))
        }
        TargetSpec::Name(n) => {
            let l = ledger(store, task);
            match l.names.iter().find(|(_, name)| *name == n) {
                Some((id, _)) if restorable(id) => parse(id),
                Some((id, _)) => refuse(
                    "COLLECTED",
                    format!("the checkpoint named `{n}` ({id}) is no longer restorable"),
                ),
                None => refuse(
                    "UNKNOWN_NAME",
                    format!("no checkpoint of this task is named `{n}`"),
                ),
            }
        }
        TargetSpec::TurnOrdinal(_) | TargetSpec::Turn(_) => {
            let l = ledger(store, task);
            let matching: Vec<&Commit> = l
                .commits
                .iter()
                .filter(|c| match spec {
                    TargetSpec::TurnOrdinal(o) => c.turn_ordinal == *o && !c.turn_id.is_empty(),
                    TargetSpec::Turn(t) => c.turn_id == t.to_string(),
                    _ => false,
                })
                .collect();
            // The latest commit for the turn that is still restorable.
            match matching
                .iter()
                .rev()
                .find(|c| restorable(&c.checkpoint_id))
            {
                Some(c) => parse(&c.checkpoint_id),
                None if !matching.is_empty() => refuse(
                    "COLLECTED",
                    "the checkpoint of that turn was removed by the retention collector".into(),
                ),
                None => refuse(
                    "NO_CHECKPOINT_FOR_TURN",
                    match spec {
                        TargetSpec::TurnOrdinal(o) => format!(
                            "no turn-boundary checkpoint was recorded for a turn with ordinal {o} (a turn that ran no sandbox tool, or whose capture was skipped, leaves none; fork at a neighbouring turn)"
                        ),
                        _ => "no turn-boundary checkpoint was recorded for that turn (a turn that ran no sandbox tool, or whose capture was skipped, leaves none; fork at a neighbouring turn)".into(),
                    },
                ),
            }
        }
    }
}

/// The turn a checkpoint is the boundary of, when it is one.
pub(crate) fn turn_of<'a>(ledger: &'a Ledger, checkpoint_id: &str) -> Option<&'a Commit> {
    ledger
        .commits
        .iter()
        .rev()
        .find(|c| c.checkpoint_id == checkpoint_id && !c.turn_id.is_empty())
}

// ---- names ----------------------------------------------------------------

/// Why a name was refused.
pub(crate) const MAX_NAME: usize = 64;

/// Validate a checkpoint label.
pub(crate) fn valid_name(name: &str) -> Result<(), String> {
    let t = name.trim();
    if t.is_empty() || t != name {
        return Err(
            "a checkpoint name is 1..64 characters with no leading or trailing space".into(),
        );
    }
    if name.chars().count() > MAX_NAME || name.chars().any(char::is_control) {
        return Err(format!(
            "a checkpoint name is at most {MAX_NAME} characters with no control character"
        ));
    }
    Ok(())
}

/// What naming did.
pub(crate) struct Named {
    /// The checkpoint named.
    pub checkpoint_id: CheckpointId,
    /// The log offset.
    pub offset: u64,
    /// The name was already on it (a no-op), or the command id was replayed.
    pub replayed: bool,
}

/// Label a checkpoint (REQ-PX-102): unique within the task, one per
/// checkpoint, idempotent. `command` makes a retry of the same command a
/// replay.
pub(crate) fn name_checkpoint(
    core: &Core,
    store: &mut EventStore,
    task: &Task,
    actor: &Actor,
    checkpoint_id: Option<CheckpointId>,
    name: &str,
    command: Option<CommandRecord>,
) -> Result<Named, RestoreRefused> {
    let refuse = |code: &'static str, detail: String| RestoreRefused { code, detail };
    valid_name(name).map_err(|d| refuse("BAD_NAME", d))?;
    let rows = store.checkpoints(&task.task_id).unwrap_or_default();
    let id = match checkpoint_id {
        Some(id) => id,
        None => rows
            .iter()
            .find(|r| r.status == "CURRENT")
            .and_then(|r| CheckpointId::parse(&r.checkpoint_id).ok())
            .ok_or_else(|| {
                refuse(
                    "NO_CHECKPOINT",
                    "the task has no committed checkpoint".into(),
                )
            })?,
    };
    let row = rows.iter().find(|r| r.checkpoint_id == id.to_string());
    match row.map(|r| r.status.as_str()) {
        Some("CURRENT" | "SUPERSEDED") => {}
        Some("COLLECTED") => {
            return Err(refuse(
                "COLLECTED",
                format!("checkpoint {id} was removed by the retention collector"),
            ));
        }
        _ => {
            return Err(refuse(
                "UNKNOWN_CHECKPOINT",
                format!("the task has no committed checkpoint {id}"),
            ));
        }
    }
    let l = ledger(store, task);
    if let Some((other, _)) = l
        .names
        .iter()
        .find(|(cid, n)| *n == name && **cid != id.to_string())
    {
        return Err(refuse(
            "NAME_TAKEN",
            format!("`{name}` already names checkpoint {other} of this task"),
        ));
    }
    let event = typed(
        "CheckpointNamed",
        &TaskEvent::CheckpointNamed {
            checkpoint_id: id.to_string(),
            name: name.to_owned(),
        },
        actor.clone(),
    );
    if l.names.get(&id.to_string()).is_some_and(|n| n == name) {
        // Already carried: nothing to record.
        return Ok(Named {
            checkpoint_id: id,
            offset: store.last_offset().unwrap_or(0),
            replayed: true,
        });
    }
    let req = modbit_event_store::AppendRequest {
        tenant_id: core.tenant_id,
        session_id: task.session_id,
        task_id: Some(task.task_id),
        run_id: None,
        turn_id: None,
        step_id: None,
        aggregate_type: AggregateType::Task,
        aggregate_id: *task.task_id.as_bytes(),
        expected_sequence: None,
        events: vec![event],
    };
    let outcome = match command {
        Some(cmd) => store.execute_command(cmd, req),
        None => store
            .append(req)
            .map(modbit_event_store::CommandOutcome::Applied),
    };
    match outcome {
        Ok(modbit_event_store::CommandOutcome::Applied(evs)) => {
            let offset = evs.last().map_or(0, |e| e.offset);
            core.last_offset.send_replace(offset);
            Ok(Named {
                checkpoint_id: id,
                offset,
                replayed: false,
            })
        }
        Ok(modbit_event_store::CommandOutcome::Replayed(evs)) => Ok(Named {
            checkpoint_id: id,
            offset: evs.last().map_or(0, |e| e.offset),
            replayed: true,
        }),
        Err(e) => Err(refuse("STORE", e.to_string())),
    }
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
    /// Caller preconditions checked.
    pub preconditions_checked: u32,
    /// The checkpoint of the state before the restore; restoring it is the
    /// redo. `None` when the restore changed nothing.
    pub pre_restore_checkpoint_id: Option<CheckpointId>,
    /// The command id had already restored; nothing was written again.
    pub replayed: bool,
    /// Paths left as they were by the caller's choice.
    pub kept_paths: Vec<String>,
    /// It was a redo.
    pub redo: bool,
}

/// A checkpoint's files are the worktree's *dirty state relative to the HEAD
/// it was captured at* (docs/19). Restoring it onto a different HEAD does not
/// reproduce the worktree it was taken from: every path that was clean then
/// is now whatever the new HEAD says, while the dirty paths come back from
/// the old base. So when the repository's HEAD is no longer the recorded one
/// (a new commit, an amend, a rebase, a reset, another branch), the restore
/// and its preview are refused, typed `HEAD_DRIFT`, before anything is
/// planned or written (FIX-18). `ForkTask` from the checkpoint is the way to
/// its exact state: the fork's worktree is created at the recorded HEAD.
/// A checkpoint that recorded no HEAD has nothing to compare.
pub(crate) fn head_drift(
    repo: &modbit_git::Repo,
    recorded: Option<&str>,
    checkpoint_id: CheckpointId,
) -> Option<RestoreRefused> {
    let recorded = recorded.filter(|h| !h.is_empty())?;
    let current = repo.head().ok();
    if current.as_deref() == Some(recorded) {
        return None;
    }
    let short = |h: &str| h.chars().take(12).collect::<String>();
    let (now, relation) = match &current {
        None => (
            "no HEAD".to_owned(),
            "the repository has no commit checked out".to_owned(),
        ),
        Some(cur) => (
            short(cur),
            match repo.merge_base(recorded, cur) {
                Ok(base) if base == recorded => {
                    "the repository moved forward: commits were added".to_owned()
                }
                Ok(_) => {
                    "the history diverged (an amend, rebase, reset or another branch)".to_owned()
                }
                // No common ancestor: unrelated histories, or the recorded
                // commit is gone from the repository altogether.
                Err(_) if repo.rev_parse(&format!("{recorded}^{{commit}}")).is_ok() => {
                    "the history diverged (an amend, rebase, reset or another branch)".to_owned()
                }
                Err(_) => "the recorded commit is no longer in the repository (garbage collected)"
                    .to_owned(),
            },
        ),
    };
    Some(RestoreRefused {
        code: "HEAD_DRIFT",
        detail: format!(
            "checkpoint {checkpoint_id} was captured at HEAD {}; the repository is now at {now} ({relation}); a restore would mix the checkpoint's files with a different base. Nothing was written. Fork the task from the checkpoint to get its exact state on its own worktree, or capture a new checkpoint at the current HEAD",
            short(recorded)
        ),
    })
}

/// What a restore is asked to do beyond naming its target.
#[derive(Default)]
pub(crate) struct RestoreOptions {
    /// The caller's optimistic preconditions (REQ-EV-0123): path and the
    /// content hash it last saw.
    pub expected: Vec<(String, String)>,
    /// The `RestoreCheckpoint` command: a replay of it answers with what it
    /// did and writes nothing; its id journals the restore.
    pub command: Option<CommandRecord>,
    /// The target is a pre-restore checkpoint: undo that restore.
    pub redo: bool,
    /// The user's choice for files they edited: these paths keep their
    /// current content.
    pub keep_paths: Vec<String>,
    /// Refuse when the task's current epoch is not this one (0 = unchecked).
    pub expected_epoch: u32,
    /// A rollback of an in-doubt restore: no new pre-restore checkpoint, no
    /// preconditions, no choice — put the recorded state back.
    pub rollback: bool,
}

fn refused<T>(
    code: &'static str,
    detail: impl Into<String>,
) -> anyhow::Result<Result<T, RestoreRefused>> {
    Ok(Err(RestoreRefused {
        code,
        detail: detail.into(),
    }))
}

/// [`restore`] with the options of REQ-PX-061: exactly reversible (a
/// pre-restore checkpoint is committed, and its id returned, before the
/// first byte changes), journalled (so a Core that dies inside it is rolled
/// back at its next start), idempotent per command, fenced by epoch.
pub(crate) async fn restore_with(
    core: &Core,
    task: &Task,
    lt: Lineage,
    actor: &Actor,
    target: Option<CheckpointId>,
    opts: RestoreOptions,
) -> anyhow::Result<Result<Restored, RestoreRefused>> {
    let root = task
        .workspace_root
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("task has no workspace root"))?;
    // A retry of the same command answers with what it did.
    if let Some(cmd) = &opts.command {
        let store = core.store.lock().await;
        match store.prior_command(cmd) {
            Ok(Some(modbit_event_store::CommandOutcome::Replayed(evs)))
            | Ok(Some(modbit_event_store::CommandOutcome::Applied(evs))) => {
                if let Some(e) = evs
                    .iter()
                    .find(|e| e.envelope.event_type == "CheckpointRestored")
                    && let Ok(p) = store.payload(&e.envelope)
                {
                    return Ok(Ok(restored_from_event(&p, true)));
                }
            }
            Ok(None) => {}
            Err(modbit_event_store::Error::IdempotencyConflict { .. }) => {
                return refused(
                    "IDEMPOTENCY_CONFLICT",
                    "this command id was used for a different request",
                );
            }
            Err(e) => return Err(anyhow::anyhow!(e)),
        }
    }
    let g = gate(core);
    let _restoring = match g.begin_restore() {
        Ok(guard) => guard,
        Err(r) => return refused(r.code, r.detail),
    };
    let (ws_arc, canonical) = core.tools.workspace(root).await?;
    let (chain, objects) = {
        let store = core.store.lock().await;
        let rows = store.checkpoints(&task.task_id).unwrap_or_default();
        // The epoch fence: a caller that last saw another current epoch is
        // acting on a stale picture of the task.
        let current_epoch = rows
            .iter()
            .find(|r| r.status == "CURRENT")
            .map_or(0, |r| r.epoch);
        if opts.expected_epoch != 0 && opts.expected_epoch != current_epoch {
            return refused(
                "STALE_EPOCH",
                format!(
                    "the caller saw checkpoint epoch {}, the task is at epoch {current_epoch}; list the checkpoints again",
                    opts.expected_epoch
                ),
            );
        }
        let target = match target.or_else(|| {
            rows.iter()
                .find(|r| r.status == "CURRENT")
                .and_then(|r| CheckpointId::parse(&r.checkpoint_id).ok())
        }) {
            Some(t) => t,
            None => return refused("NO_CHECKPOINT", "the task has no committed checkpoint"),
        };
        if rows
            .iter()
            .any(|r| r.checkpoint_id == target.to_string() && r.status == "COLLECTED")
        {
            return refused(
                "COLLECTED",
                format!(
                    "checkpoint {target} was removed by the retention collector; it can no longer be restored"
                ),
            );
        }
        let l = ledger(&store, task);
        if opts.redo {
            // REQ-PX-061: a redo undoes one restore, and only while nothing
            // happened after it.
            let Some(rec) = l
                .restores
                .iter()
                .rev()
                .find(|r| r.pre_restore == target.to_string() && !r.pre_restore.is_empty())
            else {
                return refused(
                    "NOT_A_PRE_RESTORE",
                    format!(
                        "checkpoint {target} is not the pre-restore checkpoint of any restore of this task"
                    ),
                );
            };
            let RestoreState::Done(done_at) = rec.state else {
                return refused(
                    "NOT_A_PRE_RESTORE",
                    format!(
                        "the restore that recorded checkpoint {target} did not finish; nothing to redo"
                    ),
                );
            };
            if let Some(later) = l.commits.iter().find(|c| c.offset > done_at) {
                return refused(
                    "REDO_SUPERSEDED",
                    format!(
                        "checkpoint {} was committed after that restore; the work has moved on and a redo would discard it",
                        later.checkpoint_id
                    ),
                );
            }
        }
        // The chain from the rows: only the checkpoints it names are read.
        let Some(chain) = chain_manifests(&store, &rows, &target.to_string()) else {
            // Say why the way the whole-store walk always has.
            let all = manifests(&store, task);
            return match chain_to(&all, target) {
                Err(e) => Ok(Err(chain_refusal(e))),
                Ok(_) => refused("BROKEN_LINK", "a manifest of the chain could not be read"),
            };
        };
        (chain, store.objects().clone())
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
    // FIX-18: the base the checkpoint's files are relative to must still be
    // the repository's HEAD.
    if let Some(refused) = head_drift(&repo, state.git_head.as_deref(), state.checkpoint_id) {
        return Ok(Err(refused));
    }
    let mut ws = ws_arc.lock().await;
    // REQ-EV-0123: the caller's optimistic preconditions — the content it
    // last saw at each path (a preview) — are checked before anything is
    // planned; one mismatch refuses the whole restore.
    for (path, hash) in &opts.expected {
        let now = read_workspace_file(&ws, path)
            .map(|b| content_hash(&b))
            .unwrap_or_default();
        if now != *hash {
            return Ok(Err(RestoreRefused {
                code: "HASH_MISMATCH",
                detail: format!(
                    "`{path}` changed since it was previewed: expected {}, found {}",
                    if hash.is_empty() {
                        "absent"
                    } else {
                        hash.as_str()
                    },
                    if now.is_empty() {
                        "absent"
                    } else {
                        now.as_str()
                    }
                ),
            }));
        }
    }
    let keep: BTreeSet<&str> = opts.keep_paths.iter().map(String::as_str).collect();
    let mut kept: Vec<String> = Vec::new();
    let mut ops: Vec<ChangeOp> = Vec::new();
    let mut pre_bytes: HashMap<String, Option<Vec<u8>>> = HashMap::new();
    let mut reverted = 0u32;
    // Every op carries the content it was planned against, so a write that
    // lands between the plan and the transaction refuses the transaction
    // (the change engine's own precondition), never overwrites it.
    let pre_for = |current: &Option<Vec<u8>>| WritePrecondition {
        expected_content_hash: current.as_deref().map(content_hash),
        expected_workspace_revision: None,
        expect_absent: current.is_none(),
    };
    // Paths dirty now that the checkpoint does not have: back to HEAD, or gone.
    for e in repo.status()? {
        if state.files.contains_key(&e.path) {
            continue;
        }
        if keep.contains(e.path.as_str()) {
            kept.push(e.path.clone());
            continue;
        }
        let current = read_workspace_file(&ws, &e.path);
        pre_bytes.insert(e.path.clone(), current.clone());
        if e.code == "??" {
            if current.is_some() {
                ops.push(ChangeOp {
                    path: e.path.clone(),
                    kind: ChangeOpKind::Delete,
                    pre: pre_for(&current),
                });
                reverted += 1;
            }
        } else if let Ok(head_bytes) = repo.show("HEAD", &e.path) {
            let kind = if current.is_some() {
                ChangeOpKind::ReplaceExact(head_bytes)
            } else {
                ChangeOpKind::Create(head_bytes)
            };
            ops.push(ChangeOp {
                path: e.path.clone(),
                kind,
                pre: pre_for(&current),
            });
            reverted += 1;
        } else if current.is_some() {
            // A path the index holds but HEAD does not (a staged addition, as the snapshot leaves new
            // files): there is nothing at HEAD to go back to, so back to HEAD is gone. The preview names
            // it REVERT_TO_HEAD; leaving it in place would make the restore a no-op for it.
            ops.push(ChangeOp {
                path: e.path.clone(),
                kind: ChangeOpKind::Delete,
                pre: pre_for(&current),
            });
            reverted += 1;
        }
    }
    let mut written = 0u32;
    // Paths the checkpoint has as deleted: gone.
    for (path, hash) in &state.files {
        if hash == modbit_checkpoint::DELETED {
            if keep.contains(path.as_str()) {
                kept.push(path.clone());
                continue;
            }
            let current = read_workspace_file(&ws, path);
            if current.is_some() {
                pre_bytes.insert(path.clone(), current.clone());
                ops.push(ChangeOp {
                    path: path.clone(),
                    kind: ChangeOpKind::Delete,
                    pre: pre_for(&current),
                });
                written += 1;
            }
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
        if keep.contains(path.as_str()) {
            kept.push(path.clone());
            continue;
        }
        pre_bytes.insert(path.clone(), current.clone());
        let kind = if current.is_some() {
            ChangeOpKind::ReplaceExact(content.clone())
        } else {
            ChangeOpKind::Create(content.clone())
        };
        ops.push(ChangeOp {
            path: path.clone(),
            kind,
            pre: pre_for(&current),
        });
        written += 1;
    }
    kept.sort();
    kept.dedup();
    let command_text = opts
        .command
        .as_ref()
        .map(|c| c.command_id.to_string())
        .unwrap_or_else(|| CheckpointId::new().to_string());
    // REQ-PX-061: the state about to be replaced is committed as a
    // checkpoint first, so the restore is exactly reversible. A restore that
    // changes nothing has nothing to reverse.
    let mut pre_restore: Option<CheckpointId> = None;
    if !ops.is_empty() && !opts.rollback {
        // The capture takes the workspace lock itself; the plan above does
        // not need it while the state is recorded.
        drop(ws);
        let captured = capture(core, task, lt, actor, None, "pre_restore").await;
        ws = ws_arc.lock().await;
        match captured {
            Ok(c) if c.committed.is_ok() => pre_restore = Some(c.manifest.checkpoint_id),
            Ok(_) => {
                return refused(
                    "PRE_RESTORE_FAILED",
                    "the state before the restore could not be recorded (a newer checkpoint epoch won the race); nothing was changed",
                );
            }
            Err(e) => {
                return refused(
                    "PRE_RESTORE_FAILED",
                    format!(
                        "the state before the restore could not be recorded: {e}; nothing was changed"
                    ),
                );
            }
        }
        fault_point("AFTER_PRE_RESTORE");
        // The journal: from here until `CheckpointRestored`, a Core that
        // dies is found by its next start and rolled back.
        {
            let mut store = core.store.lock().await;
            let _ = append(
                &mut store,
                core,
                lt,
                AggregateType::Task,
                *task.task_id.as_bytes(),
                vec![typed(
                    "CheckpointRestoreStarted",
                    &TaskEvent::CheckpointRestoreStarted {
                        command_id: command_text.clone(),
                        pre_restore_checkpoint_id: pre_restore
                            .map(|c| c.to_string())
                            .unwrap_or_default(),
                        target_checkpoint_id: state.checkpoint_id.to_string(),
                    },
                    actor.clone(),
                )],
            );
        }
        fault_point("AFTER_RESTORE_JOURNAL");
    }
    let pre_revision = ws.revision().number;
    if !ops.is_empty() {
        // Failure injection (REQ-PX-061): half the transaction lands, then
        // the process dies — the mixture the journal exists to resolve.
        if std::env::var("MODBIT_FAULT_CHECKPOINT").is_ok_and(|v| v == "MID_RESTORE_WRITE")
            && ops.len() > 1
        {
            let half = ops.len() / 2;
            let _ = ws.apply_transaction(&ops[..half]);
            fault_point("MID_RESTORE_WRITE");
        }
        let changes = match ws.apply_transaction(&ops) {
            Ok(c) => c,
            Err(e) => {
                let text = e.to_string();
                // The transaction rolled itself back: the worktree is what
                // it was, and the journal says so.
                if !opts.rollback {
                    let mut store = core.store.lock().await;
                    let _ = append(
                        &mut store,
                        core,
                        lt,
                        AggregateType::Task,
                        *task.task_id.as_bytes(),
                        vec![typed(
                            "CheckpointRestoreRolledBack",
                            &TaskEvent::CheckpointRestoreRolledBack {
                                command_id: command_text.clone(),
                                pre_restore_checkpoint_id: pre_restore
                                    .map(|c| c.to_string())
                                    .unwrap_or_default(),
                                target_checkpoint_id: state.checkpoint_id.to_string(),
                                reason: format!("RESTORE_REFUSED: {text}"),
                            },
                            actor.clone(),
                        )],
                    );
                }
                if text.contains("precondition failed") {
                    return Ok(Err(RestoreRefused {
                        code: "HASH_MISMATCH",
                        detail: format!("the worktree changed under the restore: {text}"),
                    }));
                }
                return Err(anyhow::anyhow!("restore transaction: {e}"));
            }
        };
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
            "",
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
    // The pre-restore checkpoint became the newest; "the current checkpoint"
    // must go on naming the state the worktree is in, so a restore leaves a
    // checkpoint of the state it wrote (a delta, and empty when the restore
    // changed nothing the base did not have). It is committed before the
    // restore is recorded, so it is not "work after the restore" to a redo.
    if !ops.is_empty()
        && let Err(e) = capture(core, task, lt, actor, None, "post_restore").await
    {
        eprintln!("modbit-core: the checkpoint after a restore could not be taken: {e}");
    }
    let restored = Restored {
        checkpoint_id: state.checkpoint_id,
        epoch: state.epoch,
        chain: state.chain.clone(),
        files_written: written,
        files_reverted: reverted,
        workspace_revision_after: after,
        event_offset: state.runtime.event_offset,
        preconditions_checked: opts.expected.len() as u32,
        pre_restore_checkpoint_id: pre_restore,
        replayed: false,
        kept_paths: kept,
        redo: opts.redo,
    };
    if opts.rollback {
        return Ok(Ok(restored));
    }
    {
        let mut store = core.store.lock().await;
        let event = typed(
            "CheckpointRestored",
            &TaskEvent::CheckpointRestored {
                checkpoint_id: state.checkpoint_id.to_string(),
                epoch: state.epoch,
                chain: state.chain.iter().map(ToString::to_string).collect(),
                files_written: written,
                files_reverted: reverted,
                workspace_revision_after: after,
                event_offset: state.runtime.event_offset,
                preconditions_checked: opts.expected.len() as u32,
                pre_restore_checkpoint_id: pre_restore.map(|c| c.to_string()).unwrap_or_default(),
                command_id: command_text,
                redo: opts.redo,
            },
            actor.clone(),
        );
        let req = modbit_event_store::AppendRequest {
            tenant_id: core.tenant_id,
            session_id: task.session_id,
            task_id: Some(task.task_id),
            run_id: None,
            turn_id: None,
            step_id: None,
            aggregate_type: AggregateType::Task,
            aggregate_id: *task.task_id.as_bytes(),
            expected_sequence: None,
            events: vec![event],
        };
        let outcome = match opts.command {
            Some(cmd) => store.execute_command(cmd, req),
            None => store
                .append(req)
                .map(modbit_event_store::CommandOutcome::Applied),
        };
        match outcome {
            Ok(modbit_event_store::CommandOutcome::Applied(evs)) => {
                if let Some(last) = evs.last() {
                    core.last_offset.send_replace(last.offset);
                }
            }
            Ok(modbit_event_store::CommandOutcome::Replayed(_)) => {}
            Err(e) => eprintln!("modbit-core: CheckpointRestored was not recorded: {e}"),
        }
    }
    Ok(Ok(restored))
}

/// The result a replayed restore returns: what its `CheckpointRestored`
/// event says.
fn restored_from_event(p: &serde_json::Value, replayed: bool) -> Restored {
    let id = |k: &str| {
        p[k].as_str()
            .filter(|s| !s.is_empty())
            .and_then(|s| CheckpointId::parse(s).ok())
    };
    Restored {
        checkpoint_id: id("checkpoint_id").unwrap_or_else(CheckpointId::new),
        epoch: p["epoch"].as_u64().unwrap_or(0) as u32,
        chain: p["chain"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|c| c.as_str().and_then(|s| CheckpointId::parse(s).ok()))
                    .collect()
            })
            .unwrap_or_default(),
        files_written: p["files_written"].as_u64().unwrap_or(0) as u32,
        files_reverted: p["files_reverted"].as_u64().unwrap_or(0) as u32,
        workspace_revision_after: p["workspace_revision_after"].as_u64().unwrap_or(0),
        event_offset: p["event_offset"].as_u64().unwrap_or(0),
        preconditions_checked: p["preconditions_checked"].as_u64().unwrap_or(0) as u32,
        pre_restore_checkpoint_id: id("pre_restore_checkpoint_id"),
        replayed,
        kept_paths: Vec::new(),
        redo: p["redo"].as_bool().unwrap_or(false),
    }
}

/// At start (REQ-PX-061): every restore a dead Core left in doubt — its
/// journal opened, never closed — is rolled back to the pre-restore
/// checkpoint it recorded first, so the worktree is exactly what it was
/// before the restore began, never a mixture. The rollback is itself
/// recorded. A rollback that cannot be made (the repository moved, an
/// object is gone) is reported and tried again at the next start.
pub(crate) async fn recover_in_doubt_restores(core: &Core) -> usize {
    let tasks: Vec<Task> = {
        let store = core.store.lock().await;
        // Every task that has any checkpoint.
        let mut ids: BTreeSet<[u8; 16]> = BTreeSet::new();
        for (t, _) in store.all_live_checkpoints().unwrap_or_default() {
            ids.insert(*t.as_bytes());
        }
        ids.into_iter()
            .filter_map(|b| {
                store
                    .task(&modbit_domain::TaskId::from_bytes(b))
                    .ok()
                    .flatten()
            })
            .collect()
    };
    let actor = Actor::Core("recovery".into());
    let mut rolled_back = 0;
    for task in tasks {
        let doubtful: Vec<RestoreRec> = {
            let store = core.store.lock().await;
            ledger(&store, &task)
                .restores
                .into_iter()
                .filter(|r| r.state == RestoreState::InDoubt)
                .collect()
        };
        for rec in doubtful.into_iter().rev() {
            let lt = Lineage::task(core.tenant_id, task.session_id, task.task_id);
            let outcome = match CheckpointId::parse(&rec.pre_restore) {
                Ok(pre) => {
                    restore_with(
                        core,
                        &task,
                        lt,
                        &actor,
                        Some(pre),
                        RestoreOptions {
                            rollback: true,
                            ..Default::default()
                        },
                    )
                    .await
                }
                Err(_) => Ok(Err(RestoreRefused {
                    code: "NO_PRE_RESTORE",
                    detail: "the journal names no pre-restore checkpoint".into(),
                })),
            };
            match outcome {
                Ok(Ok(_)) => {
                    let mut store = core.store.lock().await;
                    let _ = append(
                        &mut store,
                        core,
                        lt,
                        AggregateType::Task,
                        *task.task_id.as_bytes(),
                        vec![typed(
                            "CheckpointRestoreRolledBack",
                            &TaskEvent::CheckpointRestoreRolledBack {
                                command_id: rec.command_id.clone(),
                                pre_restore_checkpoint_id: rec.pre_restore.clone(),
                                target_checkpoint_id: rec.target.clone(),
                                reason: "CORE_RESTARTED_MID_RESTORE".into(),
                            },
                            actor.clone(),
                        )],
                    );
                    rolled_back += 1;
                    eprintln!(
                        "modbit-core: task {}: a restore to {} was interrupted; rolled back to the pre-restore checkpoint {}",
                        task.task_id, rec.target, rec.pre_restore
                    );
                }
                Ok(Err(r)) => eprintln!(
                    "modbit-core: task {}: an interrupted restore could not be rolled back ({}: {}); it is tried again at the next start",
                    task.task_id, r.code, r.detail
                ),
                Err(e) => eprintln!(
                    "modbit-core: task {}: an interrupted restore could not be rolled back: {e}",
                    task.task_id
                ),
            }
        }
    }
    rolled_back
}

pub(crate) fn chain_refusal(e: ChainError) -> RestoreRefused {
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

/// Why each checkpoint of a task cannot be collected, whatever the policy
/// (REQ-PX-102): a name, a fork taken from it, being the latest, being the
/// state before a restore (a redo needs it), being the target of a restore in
/// doubt, and — transitively — being a link of the chain of any of those. The
/// age and count floors of a policy are added by the collector.
pub(crate) fn semantic_protections(
    rows: &[modbit_event_store::projections::CheckpointRow],
    ledger: &Ledger,
    fork_parents: &BTreeSet<String>,
) -> BTreeMap<String, Vec<&'static str>> {
    let live: Vec<&modbit_event_store::projections::CheckpointRow> = rows
        .iter()
        .filter(|r| matches!(r.status.as_str(), "CURRENT" | "SUPERSEDED"))
        .collect();
    let mut out: BTreeMap<String, Vec<&'static str>> = BTreeMap::new();
    fn add(out: &mut BTreeMap<String, Vec<&'static str>>, id: &str, why: &'static str) {
        let e = out.entry(id.to_owned()).or_default();
        if !e.contains(&why) {
            e.push(why);
        }
    }
    for r in &live {
        if r.status == "CURRENT" {
            add(&mut out, &r.checkpoint_id, "LATEST");
        }
        if r.reason.starts_with("pre_restore") {
            add(&mut out, &r.checkpoint_id, "PRE_RESTORE");
        }
        if ledger.names.contains_key(&r.checkpoint_id) {
            add(&mut out, &r.checkpoint_id, "NAMED");
        }
        if fork_parents.contains(&r.checkpoint_id) {
            add(&mut out, &r.checkpoint_id, "FORK_PARENT");
        }
    }
    for rec in ledger
        .restores
        .iter()
        .filter(|r| r.state == RestoreState::InDoubt)
    {
        add(&mut out, &rec.target, "RESTORE_TARGET");
        if !rec.pre_restore.is_empty() {
            add(&mut out, &rec.pre_restore, "RESTORE_TARGET");
        }
    }
    // A kept checkpoint keeps its whole chain: a delta is nothing without
    // the baseline and the deltas it is relative to.
    let by_id: HashMap<&str, &modbit_event_store::projections::CheckpointRow> = live
        .iter()
        .map(|r| (r.checkpoint_id.as_str(), *r))
        .collect();
    let roots: Vec<String> = out.keys().cloned().collect();
    for root in roots {
        let mut cursor = by_id
            .get(root.as_str())
            .and_then(|r| r.base_checkpoint_id.clone());
        let mut guard = 0;
        while let Some(id) = cursor {
            guard += 1;
            if guard > 10_000 {
                break;
            }
            add(&mut out, &id, "CHAIN");
            cursor = by_id
                .get(id.as_str())
                .and_then(|r| r.base_checkpoint_id.clone());
        }
    }
    out
}

/// Wire view of a checkpoint row, with what the task's log adds: its label,
/// the turn whose boundary it is, what a capture of it cost, and why the
/// collector keeps it.
pub(crate) fn view_with(
    r: &modbit_event_store::projections::CheckpointRow,
    ledger: &Ledger,
    retention: &BTreeMap<String, Vec<&'static str>>,
) -> wire::CheckpointView {
    let head = r
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
    let turn = turn_of(ledger, &r.checkpoint_id);
    let commit = ledger
        .commits
        .iter()
        .rev()
        .find(|c| c.checkpoint_id == r.checkpoint_id);
    wire::CheckpointView {
        checkpoint_id: r.checkpoint_id.clone(),
        epoch: r.epoch,
        kind: r.kind.clone(),
        base_checkpoint_id: r.base_checkpoint_id.clone().unwrap_or_default(),
        status: r.status.clone(),
        workspace_revision: r.workspace_revision,
        manifest_ref: r.manifest_object_hash.clone().unwrap_or_default(),
        integrity_hash: r.integrity_hash.clone().unwrap_or_default(),
        git_head: head,
        files: r.files,
        removed: r.removed,
        event_offset,
        index_generation: r.index_generation,
        reason: r.reason.clone(),
        created_at_ms: r.created_at.0,
        committed_at_ms: r.committed_at.map_or(0, |t| t.0),
        name: ledger
            .names
            .get(&r.checkpoint_id)
            .cloned()
            .unwrap_or_default(),
        turn_id: turn
            .and_then(|c| modbit_domain::TurnId::parse(&c.turn_id).ok())
            .map(|t| wire::Id {
                value: t.as_bytes().to_vec(),
            }),
        turn_ordinal: turn.map_or(0, |c| c.turn_ordinal),
        retention: retention
            .get(&r.checkpoint_id)
            .map(|v| v.iter().map(|s| (*s).to_owned()).collect())
            .unwrap_or_default(),
        cost: commit.map(|c| c.cost.wire()),
    }
}

/// The wire view of a task's checkpoints.
pub(crate) fn list(store: &EventStore, task: &Task) -> wire::CheckpointList {
    let rows = store.checkpoints(&task.task_id).unwrap_or_default();
    let current = rows.iter().find(|r| r.status == "CURRENT");
    let l = ledger(store, task);
    let retention = semantic_protections(&rows, &l, &fork_parents(store, task));
    wire::CheckpointList {
        current_checkpoint_id: current.map(|r| r.checkpoint_id.clone()).unwrap_or_default(),
        current_epoch: current.map_or(0, |r| r.epoch),
        checkpoints: rows.iter().map(|r| view_with(r, &l, &retention)).collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// REQ-PX-102: a collection never runs under a restore, never beside
    /// another collection, and a restore never starts under a collection.
    #[test]
    fn the_gate_keeps_the_collector_and_restores_apart() {
        let g = Arc::new(Gate::default());
        let restore = g.begin_restore().expect("a restore may start");
        let refused = g.begin_gc("a", "x").err().expect("not under a restore");
        assert_eq!(refused.code, "RESTORE_IN_FLIGHT");
        drop(restore);
        let lease = g.begin_gc("a", "x").expect("the lease is free again");
        assert_eq!(g.begin_gc("b", "y").err().unwrap().code, "GC_LEASE_HELD");
        assert_eq!(g.begin_restore().err().unwrap().code, "GC_IN_FLIGHT");
        drop(lease);
        assert!(g.begin_restore().is_ok());
    }

    /// A name is 1..64 characters, no control characters, no edge space.
    #[test]
    fn names_are_validated() {
        assert!(valid_name("before the refactor").is_ok());
        assert!(valid_name("").is_err());
        assert!(valid_name(" x").is_err());
        assert!(valid_name("a\nb").is_err());
        assert!(valid_name(&"x".repeat(65)).is_err());
        assert!(valid_name(&"x".repeat(64)).is_ok());
    }

    /// The target of a restore is named by at most one field.
    #[test]
    fn a_target_is_named_one_way() {
        assert_eq!(target_spec("", "", 0, None), Ok(TargetSpec::Current));
        assert_eq!(
            target_spec("", "keep", 0, None),
            Ok(TargetSpec::Name("keep".into()))
        );
        assert_eq!(target_spec("", "", 3, None), Ok(TargetSpec::TurnOrdinal(3)));
        assert!(target_spec("", "keep", 3, None).is_err());
        assert!(target_spec("not-an-id", "", 0, None).is_err());
    }
}
