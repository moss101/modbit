//! Retention and garbage collection of checkpoints (REQ-PX-102, docs/62,
//! docs/19 "Checkpoint epochs").
//!
//! A collector run, under a lease with an owner and a reason:
//!
//! 1. decides, per finished task, which checkpoints leave the restorable
//!    set. A live task keeps every checkpoint. For a finished one the kept set
//!    is the labelled checkpoints, the fork parents, the latest, every
//!    pre-restore checkpoint (a redo needs it), the targets of a restore in
//!    doubt, the newest `keep_recent` and anything younger than `min_age`,
//!    plus — transitively — the baseline and deltas any of them is relative
//!    to. With a `max_bytes` bound the floors give way to the bound, newest
//!    first. Everything else is removable;
//! 2. records the intent (`CheckpointGcStarted`) and removes the checkpoints
//!    from the restorable set in ONE event (`CheckpointCollected`): from that
//!    event on a restore of one answers `COLLECTED`, and no surviving chain
//!    names it, so every remaining chain restores;
//! 3. deletes the content blobs that no surviving checkpoint — of any task —
//!    and nothing else on the log names (a mark over the whole log and what
//!    it reaches: the same reachability the handoff export walks), and
//!    records what it did (`CheckpointGcCompleted`).
//!
//! The order is the crash story: before step 2 nothing changed; after it the
//! blobs may still be on disk, and the next run finds them — the sweep always
//! considers every blob of every collected checkpoint of the tasks it
//! covers — so a run killed anywhere is finished by the next one and no chain
//! is ever left broken. A collection never runs under a restore, never beside
//! another collection, and holds off captures while it sweeps. Manifests (a
//! few hundred bytes, named by events) are kept; only content is freed.
//!
//! The collection is an effect with events (started, collected, completed),
//! not an `EffectReceipt`: receipts are bound to a tool call and its kernel
//! decision (docs/23), and a collection has neither.

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::sync::Arc;

use modbit_checkpoint::CheckpointManifest;
use modbit_domain::event::{Actor, AggregateType};
use modbit_domain::state::StateMachine;
use modbit_domain::task::{Task, TaskEvent};
use modbit_event_store::EventStore;
use modbit_event_store::projections::CheckpointRow;
use modbit_protocol::v1::{self as wire, CommandAck, CommandEnvelope};
use prost::Message;

use crate::checkpoint::{fork_parents, gate, ledger, semantic_protections};
use crate::runtime::{Lineage, append, typed};
use crate::server::{Core, accept, id16, reject, require_lease, wire_id};

const DAY_MS: i64 = 24 * 60 * 60 * 1000;

/// The retention policy of a run (REQ-PX-102 defaults).
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
pub(crate) struct Policy {
    /// Newest checkpoints kept regardless of age.
    pub keep_recent: u32,
    /// Checkpoints younger than this are kept.
    pub min_age_ms: i64,
    /// 0 = unbounded; else the oldest unprotected are thinned until a task's
    /// checkpoints fit.
    pub max_bytes: u64,
}

impl Default for Policy {
    fn default() -> Self {
        Self {
            keep_recent: 10,
            min_age_ms: 7 * DAY_MS,
            max_bytes: 0,
        }
    }
}

impl Policy {
    fn from_wire(p: Option<&wire::CheckpointRetentionPolicy>) -> Self {
        let d = Self::default();
        let Some(p) = p else { return d };
        Self {
            keep_recent: if p.keep_recent == 0 {
                d.keep_recent
            } else {
                p.keep_recent
            },
            min_age_ms: if p.min_age_ms <= 0 {
                d.min_age_ms
            } else {
                p.min_age_ms
            },
            max_bytes: p.max_bytes,
        }
    }
}

/// What a decision says about one task.
#[derive(Debug, Default)]
pub(crate) struct Decision {
    /// Checkpoints that leave the restorable set.
    pub removable: Vec<String>,
    /// Every checkpoint kept, with why.
    pub kept: Vec<(String, String)>,
}

fn manifest_of(store: &EventStore, row: &CheckpointRow) -> Option<CheckpointManifest> {
    let bytes = store
        .objects()
        .get(row.manifest_object_hash.as_deref()?)
        .ok()?;
    serde_json::from_slice(&bytes).ok()
}

/// Bytes of the content a checkpoint's manifest names (what it adds).
fn bytes_of(store: &EventStore, row: &CheckpointRow) -> u64 {
    let Some(m) = manifest_of(store, row) else {
        return 0;
    };
    m.files
        .values()
        .filter(|h| !h.is_empty())
        .filter_map(|h| std::fs::metadata(store.objects().root().join(&h[..2]).join(&h[2..])).ok())
        .map(|m| m.len())
        .sum()
}

/// Decide what a policy removes from a task.
pub(crate) fn decide(store: &EventStore, task: &Task, policy: &Policy, now_ms: i64) -> Decision {
    let rows = store.checkpoints(&task.task_id).unwrap_or_default();
    let live: Vec<&CheckpointRow> = rows
        .iter()
        .filter(|r| matches!(r.status.as_str(), "CURRENT" | "SUPERSEDED"))
        .collect();
    let mut d = Decision::default();
    if !task.state.is_terminal() {
        d.kept = live
            .iter()
            .map(|r| (r.checkpoint_id.clone(), "LIVE".to_owned()))
            .collect();
        return d;
    }
    let l = ledger(store, task);
    let mut kept: BTreeMap<String, Vec<&'static str>> =
        semantic_protections(&rows, &l, &fork_parents(store, task));
    let by_id: std::collections::HashMap<&str, &CheckpointRow> = live
        .iter()
        .map(|r| (r.checkpoint_id.as_str(), *r))
        .collect();
    // The chain of `id` that is not already kept.
    let ancestors = |id: &str, kept: &BTreeMap<String, Vec<&'static str>>| -> Vec<String> {
        let mut out = Vec::new();
        let mut cursor = by_id.get(id).and_then(|r| r.base_checkpoint_id.clone());
        while let Some(a) = cursor {
            if kept.contains_key(&a) || out.contains(&a) {
                break;
            }
            cursor = by_id
                .get(a.as_str())
                .and_then(|r| r.base_checkpoint_id.clone());
            out.push(a);
        }
        out
    };
    let mut ordered: Vec<&CheckpointRow> = live.clone();
    ordered.sort_by_key(|r| std::cmp::Reverse(r.epoch));
    if policy.max_bytes == 0 {
        for (i, r) in ordered.iter().enumerate() {
            let why = if i < policy.keep_recent as usize {
                "RECENT"
            } else if now_ms - r.created_at.0 < policy.min_age_ms {
                "YOUNG"
            } else {
                continue;
            };
            if !kept.contains_key(&r.checkpoint_id) {
                for a in ancestors(&r.checkpoint_id, &kept) {
                    kept.entry(a).or_default().push("CHAIN");
                }
                kept.entry(r.checkpoint_id.clone()).or_default().push(why);
            }
        }
    } else {
        let mut used: u64 = live
            .iter()
            .filter(|r| kept.contains_key(&r.checkpoint_id))
            .map(|r| bytes_of(store, r))
            .sum();
        for r in &ordered {
            if kept.contains_key(&r.checkpoint_id) {
                continue;
            }
            let anc = ancestors(&r.checkpoint_id, &kept);
            let cost = bytes_of(store, r)
                + anc
                    .iter()
                    .filter_map(|a| by_id.get(a.as_str()))
                    .map(|a| bytes_of(store, a))
                    .sum::<u64>();
            if used + cost <= policy.max_bytes {
                used += cost;
                for a in anc {
                    kept.entry(a).or_default().push("CHAIN");
                }
                kept.entry(r.checkpoint_id.clone())
                    .or_default()
                    .push("RECENT");
            }
        }
    }
    for r in &live {
        match kept.get(&r.checkpoint_id) {
            Some(why) => d.kept.push((r.checkpoint_id.clone(), why.join(","))),
            None => d.removable.push(r.checkpoint_id.clone()),
        }
    }
    d
}

/// Every content hash the log and what it reaches still names, except the
/// manifests of collected checkpoints (their content is what is being
/// freed). A conservative mark: any 64-hex token in any event payload,
/// followed through small JSON objects.
fn mark(store: &EventStore) -> HashSet<String> {
    let mut refs: BTreeSet<String> = BTreeSet::new();
    let mut commits: Vec<(String, String)> = Vec::new();
    let mut collected: HashSet<String> = HashSet::new();
    let mut after = 0u64;
    loop {
        let page = store.read_all_after(after, 2000).unwrap_or_default();
        if page.is_empty() {
            break;
        }
        for e in &page {
            after = e.offset;
            let Ok(p) = store.payload(&e.envelope) else {
                continue;
            };
            match e.envelope.event_type.as_str() {
                "CheckpointCommitted" => {
                    commits.push((
                        p["checkpoint_id"].as_str().unwrap_or_default().to_owned(),
                        p["manifest_ref"].as_str().unwrap_or_default().to_owned(),
                    ));
                }
                "CheckpointCollected" => {
                    if let Some(ids) = p["checkpoint_ids"].as_array() {
                        collected.extend(ids.iter().filter_map(|i| i.as_str().map(str::to_owned)));
                    }
                }
                _ => crate::handoff::hashes_in(&p, &mut refs),
            }
        }
    }
    for (id, manifest) in commits {
        if !collected.contains(&id) && !manifest.is_empty() {
            refs.insert(manifest);
        }
    }
    let mut all: HashSet<String> = HashSet::new();
    let mut frontier: Vec<String> = refs.into_iter().collect();
    let mut depth = 0;
    while !frontier.is_empty() && depth < 4 {
        let mut next = BTreeSet::new();
        for h in frontier {
            if !all.insert(h.clone()) {
                continue;
            }
            // Only small JSON objects can name other objects.
            let path = store.objects().root().join(&h[..2]).join(&h[2..]);
            let small = std::fs::metadata(&path).is_ok_and(|m| m.len() <= 1024 * 1024);
            if small
                && let Ok(bytes) = store.objects().get(&h)
                && matches!(bytes.first(), Some(b'{' | b'['))
            {
                crate::handoff::hashes_in_bytes(&bytes, &mut next);
            }
        }
        frontier = next.into_iter().filter(|h| !all.contains(h)).collect();
        depth += 1;
    }
    all
}

/// The content hashes of every collected checkpoint of a task.
fn collected_content(store: &EventStore, task: &Task) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for r in store
        .checkpoints(&task.task_id)
        .unwrap_or_default()
        .iter()
        .filter(|r| r.status == "COLLECTED")
    {
        if let Some(m) = manifest_of(store, r) {
            out.extend(m.files.values().filter(|h| !h.is_empty()).cloned());
        }
    }
    out
}

#[allow(clippy::too_many_arguments)]
fn report_view(
    gc_id: &str,
    owner: &str,
    scanned: u32,
    removed_ids: Vec<String>,
    kept: &[(String, String)],
    blobs_removed: u32,
    bytes_freed: u64,
    blobs_retained: u32,
    dry_run: bool,
    tasks: u32,
    offset: u64,
) -> wire::CheckpointGcReport {
    wire::CheckpointGcReport {
        gc_id: gc_id.to_owned(),
        owner: owner.to_owned(),
        scanned,
        removed: removed_ids.len() as u32,
        blobs_removed,
        bytes_freed,
        blobs_retained,
        removed_ids,
        kept: kept
            .iter()
            .map(|(id, why)| wire::CheckpointKept {
                checkpoint_id: id.clone(),
                why: why.clone(),
            })
            .collect(),
        dry_run,
        tasks,
        offset,
    }
}

/// `RunCheckpointGc`.
pub(crate) async fn run_command(
    core: &Arc<Core>,
    env: &CommandEnvelope,
    p: wire::RunCheckpointGc,
    actor: &Actor,
) -> CommandAck {
    let cid = env.command_id.clone();
    let Some(session_id) = p
        .session_id
        .as_ref()
        .and_then(id16)
        .map(modbit_domain::SessionId::from_bytes)
    else {
        return reject(cid, "BAD_PAYLOAD", "session_id required");
    };
    if let Err(ack) = require_lease(core, &cid, env, &session_id).await {
        return ack;
    }
    let tasks: Vec<Task> = {
        let store = core.store.lock().await;
        match p.task_id.as_ref().and_then(id16) {
            Some(b) => match store.task(&modbit_domain::TaskId::from_bytes(b)) {
                Ok(Some(t)) if t.session_id == session_id => vec![t],
                Ok(Some(_)) => {
                    return reject(cid, "UNKNOWN_TASK", "the task is not in that session");
                }
                Ok(None) => return reject(cid, "UNKNOWN_TASK", wire_hex(b)),
                Err(e) => return reject(cid, crate::server::error_code(&e), e.to_string()),
            },
            None => store.tasks_for_session(&session_id).unwrap_or_default(),
        }
    };
    let policy = Policy::from_wire(p.policy.as_ref());
    let owner = format!("core-{}:{actor:?}", core.recovery().boot_generation);
    let reason = if p.reason.is_empty() {
        "requested".to_owned()
    } else {
        p.reason.clone()
    };
    let now_ms = modbit_domain::Timestamp::now().0;
    let gc_id = modbit_domain::CheckpointId::new().to_string();
    if p.dry_run {
        let store = core.store.lock().await;
        let (mut removed, mut kept, mut scanned) = (Vec::new(), Vec::new(), 0u32);
        for t in &tasks {
            let d = decide(&store, t, &policy, now_ms);
            scanned += (d.removable.len() + d.kept.len()) as u32;
            removed.extend(d.removable);
            kept.extend(d.kept);
        }
        return accept(
            cid,
            false,
            report_view(
                &gc_id,
                &owner,
                scanned,
                removed,
                &kept,
                0,
                0,
                0,
                true,
                tasks.len() as u32,
                0,
            )
            .encode_to_vec(),
        );
    }
    let g = gate(core);
    let _lease = match g.begin_gc(&owner, &reason) {
        Ok(l) => l,
        Err(r) => return reject(cid, r.code, r.detail),
    };
    if let Some(ms) = std::env::var("MODBIT_FAULT_CHECKPOINT_GC_HOLD_MS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
    {
        tokio::time::sleep(std::time::Duration::from_millis(ms)).await;
    }
    // Captures wait while the collector decides and sweeps.
    let _exclusive = g.capture.write().await;
    let policy_json = serde_json::to_string(&policy).unwrap_or_default();
    let (mut scanned, mut removed_all, mut kept_all) = (0u32, Vec::<String>::new(), Vec::new());
    let mut started: BTreeSet<[u8; 16]> = BTreeSet::new();
    let mut removed_by_task: BTreeMap<[u8; 16], u32> = BTreeMap::new();
    let mut scanned_by_task: BTreeMap<[u8; 16], u32> = BTreeMap::new();
    // Phase 1: the restorable set shrinks, atomically per task.
    for t in &tasks {
        let mut store = core.store.lock().await;
        let d = decide(&store, t, &policy, now_ms);
        scanned += (d.removable.len() + d.kept.len()) as u32;
        scanned_by_task.insert(
            *t.task_id.as_bytes(),
            (d.removable.len() + d.kept.len()) as u32,
        );
        removed_by_task.insert(*t.task_id.as_bytes(), d.removable.len() as u32);
        kept_all.extend(d.kept.clone());
        if d.removable.is_empty() {
            continue;
        }
        let lt = Lineage::task(core.tenant_id, t.session_id, t.task_id);
        let intent = append(
            &mut store,
            core,
            lt,
            AggregateType::Task,
            *t.task_id.as_bytes(),
            vec![typed(
                "CheckpointGcStarted",
                &TaskEvent::CheckpointGcStarted {
                    gc_id: gc_id.clone(),
                    owner: owner.clone(),
                    reason: reason.clone(),
                    policy_json: policy_json.clone(),
                    candidates: d.removable.clone(),
                    kept: d.kept.iter().map(|(i, w)| format!("{i}:{w}")).collect(),
                },
                actor.clone(),
            )],
        );
        if let Err(e) = intent {
            return reject(cid, "STORE", e);
        }
        started.insert(*t.task_id.as_bytes());
        if let Err(e) = append(
            &mut store,
            core,
            lt,
            AggregateType::Task,
            *t.task_id.as_bytes(),
            vec![typed(
                "CheckpointCollected",
                &TaskEvent::CheckpointCollected {
                    gc_id: gc_id.clone(),
                    checkpoint_ids: d.removable.clone(),
                },
                actor.clone(),
            )],
        ) {
            return reject(cid, "STORE", e);
        }
        removed_all.extend(d.removable);
    }
    crate::checkpoint::fault_point("AFTER_GC_COLLECT");
    // Phase 2: the sweep — blobs nothing names any more.
    let (mut blobs_removed, mut bytes_freed, mut blobs_retained) = (0u32, 0u64, 0u32);
    let marked = {
        let store = core.store.lock().await;
        mark(&store)
    };
    let mut last_offset = 0u64;
    for t in &tasks {
        let mut store = core.store.lock().await;
        let candidates = collected_content(&store, t);
        let (mut n, mut bytes, mut kept_n) = (0u32, 0u64, 0u32);
        let removed_here = removed_by_task
            .get(t.task_id.as_bytes())
            .copied()
            .unwrap_or(0);
        for h in &candidates {
            if marked.contains(h) {
                kept_n += 1;
                continue;
            }
            if let Ok(Some(len)) = store.objects().remove(h) {
                n += 1;
                bytes += len;
                crate::checkpoint::fault_point("MID_GC_SWEEP");
            }
        }
        blobs_removed += n;
        bytes_freed += bytes;
        blobs_retained += kept_n;
        let lt = Lineage::task(core.tenant_id, t.session_id, t.task_id);
        let did = started.contains(t.task_id.as_bytes()) || n > 0;
        if did {
            let mut events = Vec::new();
            if !started.contains(t.task_id.as_bytes()) {
                // A sweep that finished work an earlier, interrupted run began.
                events.push(typed(
                    "CheckpointGcStarted",
                    &TaskEvent::CheckpointGcStarted {
                        gc_id: gc_id.clone(),
                        owner: owner.clone(),
                        reason: format!("{reason} (sweep of earlier collections)"),
                        policy_json: policy_json.clone(),
                        candidates: vec![],
                        kept: vec![],
                    },
                    actor.clone(),
                ));
            }
            events.push(typed(
                "CheckpointGcCompleted",
                &TaskEvent::CheckpointGcCompleted {
                    gc_id: gc_id.clone(),
                    scanned: scanned_by_task
                        .get(t.task_id.as_bytes())
                        .copied()
                        .unwrap_or(0),
                    removed: removed_here,
                    blobs_removed: n,
                    bytes_freed: bytes,
                    blobs_retained: kept_n,
                },
                actor.clone(),
            ));
            if let Ok(o) = append(
                &mut store,
                core,
                lt,
                AggregateType::Task,
                *t.task_id.as_bytes(),
                events,
            ) {
                last_offset = o;
            }
        }
    }
    if last_offset > 0 {
        core.last_offset.send_replace(last_offset);
    }
    accept(
        cid,
        false,
        report_view(
            &gc_id,
            &owner,
            scanned,
            removed_all,
            &kept_all,
            blobs_removed,
            bytes_freed,
            blobs_retained,
            false,
            tasks.len() as u32,
            last_offset,
        )
        .encode_to_vec(),
    )
}

fn wire_hex(b: [u8; 16]) -> String {
    let _ = wire_id;
    b.iter().map(|x| format!("{x:02x}")).collect()
}
