//! `AuthorizationEpoch` and `CapabilitySnapshot` in the Core (REQ-PX-131,
//! docs/23 "Policy generations").
//!
//! At each model-round boundary the run loop freezes the round's capability
//! view — the projected tools, the policy generation, the lease, the mode
//! posture, the skills — and records it on the task under the next epoch
//! (`CapabilitySnapshotRecorded`). Every kernel decision and receipt written
//! while the round runs carries the epoch and the snapshot's digest, so "what
//! authority did this effect run under" is one lookup. A policy change, a
//! revoked skill or a mode switch made during the round shows up in the next
//! snapshot and never half-applies inside this one: the configuration the
//! kernel decides under is pinned per task and only the boundary re-resolves
//! it (`config::Configurations::try_refresh`).
//!
//! What still takes effect at once is what the kernel re-checks hot on every
//! call: the emergency stop, a revoked lease, the session lease generation.
//! This module records, it does not replace, that path.
//!
//! A restarted Core continues the epoch sequence from the log and, for a
//! round that was interrupted with calls outstanding, decides those calls
//! under the configuration the interrupted round was frozen with
//! ([`resume_round`]) — not whatever the files say after the restart.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use modbit_domain::epoch::{AuthorizationStamp, CapabilitySnapshot, LeaseSnapshot};
use modbit_domain::event::{Actor, AggregateType};
use modbit_domain::lease::CapabilityLease;
use modbit_domain::task::{Task, TaskEvent};
use modbit_domain::toolcall::EffectReceipt;
use modbit_domain::{RunId, TaskId, ToolCallId, TurnId};
use modbit_event_store::EventStore;
use modbit_policy::config::ResolvedConfig;
use sha2::{Digest, Sha256};

use crate::runtime::{Lineage, append, typed};
use crate::server::Core;

/// The event type this module records.
pub(crate) const EVENT: &str = "CapabilitySnapshotRecorded";

/// A frozen round: the snapshot and the digest its stamps name.
#[derive(Clone, Debug)]
pub(crate) struct Frozen {
    /// The snapshot as recorded.
    pub snapshot: CapabilitySnapshot,
    /// Its digest.
    pub hash: String,
}

impl Frozen {
    /// The stamp decisions and receipts of this round carry.
    pub(crate) fn stamp(&self) -> AuthorizationStamp {
        AuthorizationStamp {
            epoch: self.snapshot.epoch,
            snapshot_hash: self.hash.clone(),
        }
    }
}

/// The newest frozen round of each task this Core has seen.
#[derive(Default)]
pub struct Epochs {
    latest: Mutex<HashMap<TaskId, Frozen>>,
}

impl Epochs {
    fn put(&self, task: TaskId, f: Frozen) {
        self.latest
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(task, f);
    }

    fn cached(&self, task: TaskId) -> Option<Frozen> {
        self.latest
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(&task)
            .cloned()
    }

    /// The newest frozen round of `task`: this Core's memory of it, else the
    /// log's (a restarted Core, or a task another process ran). `None` when
    /// the task has never run a round.
    pub(crate) fn latest(&self, store: &EventStore, task: TaskId) -> Option<Frozen> {
        if let Some(f) = self.cached(task) {
            return Some(f);
        }
        let last = records(store, task).ok()?.pop()?;
        let f = Frozen {
            snapshot: last.0,
            hash: last.1,
        };
        self.put(task, f.clone());
        Some(f)
    }

    /// The stamp a decision made for `task` right now carries: the epoch of
    /// the newest round, which a call outside a round (a person's direct
    /// call) is decided under too.
    pub(crate) fn current(&self, store: &EventStore, task: TaskId) -> Option<AuthorizationStamp> {
        self.latest(store, task).map(|f| f.stamp())
    }
}

/// Every snapshot recorded for `task`, in log order, with the digest recorded
/// beside it.
///
/// # Errors
/// The log cannot be read or a record does not parse.
pub(crate) fn records(
    store: &EventStore,
    task: TaskId,
) -> Result<Vec<(CapabilitySnapshot, String)>, String> {
    let events = store
        .read_aggregate_of_types(task.as_bytes(), &[EVENT], 0)
        .map_err(|e| e.to_string())?;
    let mut out = Vec::with_capacity(events.len());
    for e in events {
        let payload = store.payload(&e.envelope).map_err(|e| e.to_string())?;
        match serde_json::from_value::<TaskEvent>(payload) {
            Ok(TaskEvent::CapabilitySnapshotRecorded {
                snapshot,
                snapshot_hash,
            }) => out.push((snapshot, snapshot_hash)),
            Ok(_) => {}
            Err(e) => return Err(format!("a capability snapshot record: {e}")),
        }
    }
    Ok(out)
}

/// What a round is frozen from.
pub(crate) struct Round<'a> {
    /// The run.
    pub run_id: RunId,
    /// The round's turn.
    pub turn_id: TurnId,
    /// The turn's ordinal in the run.
    pub ordinal: u32,
    /// The mode posture in force.
    pub mode: &'a str,
    /// The tools projected to the model, in any order.
    pub projected: &'a [String],
    /// The projection digest of the request.
    pub projection_hash: &'a str,
    /// The skills selected for the round.
    pub skills: Vec<String>,
}

fn lease_snapshot(l: &CapabilityLease) -> LeaseSnapshot {
    let mut operations = l.operations.clone();
    operations.sort();
    let mut resources = l.resources.clone();
    resources.sort();
    LeaseSnapshot {
        lease_id: l.lease_id.to_string(),
        generation: l.generation,
        effect_ceiling: format!("{:?}", l.effect_ceiling),
        operations,
        resources_digest: hex::encode(Sha256::digest(resources.join("\n").as_bytes())),
    }
}

/// Freeze the round: compute the snapshot from the configuration pinned for
/// the task right now, the lease it holds, and the round's projection, record
/// it under the next epoch and make it the epoch decisions are stamped with.
/// Nothing is stamped until the record is on the log.
///
/// # Errors
/// The record could not be appended (a stale session lease, a store failure)
/// or the configuration could not be stored. The caller stops the round:
/// no decision is ever stamped with an epoch that is not on the log.
pub(crate) fn freeze_round(
    core: &Core,
    store: &mut EventStore,
    lt: Lineage,
    task: &Task,
    round: &Round<'_>,
    actor: &Actor,
) -> Result<Frozen, String> {
    let config: Arc<ResolvedConfig> = core.tools.configurations.for_task(
        task.task_id,
        &core.data_dir,
        task.workspace_root.as_deref(),
    );
    // The resolved configuration is kept so an interrupted round can be
    // finished under it. A configuration that carries a credential in the
    // Core's custody (a server definition whose environment names one) is not
    // written down at all: nothing persisted may hold a held secret, and such
    // a round simply re-resolves on resume.
    let config_json = serde_json::to_vec(&*config).map_err(|e| e.to_string())?;
    let config_ref = if core
        .tools
        .redactor()
        .error_text(&String::from_utf8_lossy(&config_json))
        .as_bytes()
        == config_json.as_slice()
    {
        store
            .objects()
            .put(&config_json)
            .map_err(|e| e.to_string())?
    } else {
        String::new()
    };
    let lease = store
        .leases_for_task(&task.task_id)
        .map_err(|e| e.to_string())?
        .into_iter()
        .find(|l| l.is_valid(modbit_domain::Timestamp::now()));
    let previous = core
        .tools
        .epochs
        .latest(store, task.task_id)
        .map_or(0, |f| f.snapshot.epoch);
    let mut projected = round.projected.to_vec();
    projected.sort();
    projected.dedup();
    let mut skills = round.skills.clone();
    skills.sort();
    skills.dedup();
    let snapshot = CapabilitySnapshot {
        epoch: previous + 1,
        run_id: round.run_id.to_string(),
        turn_id: round.turn_id.to_string(),
        round: round.ordinal,
        config_generation: modbit_policy::config::generation(&config),
        config_ref,
        mode: round.mode.to_owned(),
        execution_profile: task.execution_profile.clone(),
        lease: lease.as_ref().map(lease_snapshot),
        projected_tools: projected,
        projection_hash: round.projection_hash.to_owned(),
        skills,
    };
    let hash = modbit_policy::epoch::snapshot_hash(&snapshot);
    append(
        store,
        core,
        lt,
        AggregateType::Task,
        *task.task_id.as_bytes(),
        vec![typed(
            EVENT,
            &TaskEvent::CapabilitySnapshotRecorded {
                snapshot: snapshot.clone(),
                snapshot_hash: hash.clone(),
            },
            actor.clone(),
        )],
    )?;
    let frozen = Frozen { snapshot, hash };
    core.tools.epochs.put(task.task_id, frozen.clone());
    Ok(frozen)
}

/// A run that resumes with calls outstanding re-enters the round those calls
/// belong to. That round was frozen before the Core stopped: its configuration
/// is pinned again from the record (so a policy file edited while the Core was
/// down applies at the next boundary, not to the remainder of this round),
/// and the epoch stays what it was. Returns the round, or `None` when the task
/// has no recorded round or its configuration object cannot be read — the
/// caller then resolves the configuration afresh, as a new round would.
pub(crate) fn resume_round(core: &Core, store: &EventStore, task: TaskId) -> Option<Frozen> {
    let frozen = core.tools.epochs.latest(store, task)?;
    if frozen.snapshot.config_ref.is_empty() {
        return None;
    }
    let bytes = store.objects().get(&frozen.snapshot.config_ref).ok()?;
    let config: ResolvedConfig = serde_json::from_slice(&bytes).ok()?;
    if modbit_policy::config::generation(&config) != frozen.snapshot.config_generation {
        // The object is not what the snapshot froze: never pin it.
        return None;
    }
    core.tools.configurations.install(task, Arc::new(config));
    Some(frozen)
}

/// The snapshots of `task` as a wire list, with whether their record is
/// consistent (`after_epoch` leaves out the older ones).
///
/// # Errors
/// The log cannot be read.
pub(crate) fn view(
    core: &Core,
    store: &EventStore,
    task: TaskId,
    after_epoch: u64,
) -> Result<modbit_protocol::v1::CapabilitySnapshotList, String> {
    let all = records(store, task)?;
    let verdict = modbit_policy::epoch::verify_snapshots(&all);
    let snapshots = all
        .iter()
        .filter(|(s, _)| s.epoch > after_epoch)
        .map(|(s, h)| modbit_protocol::v1::CapabilitySnapshotView {
            epoch: s.epoch,
            run_id: s.run_id.clone(),
            turn_id: s.turn_id.clone(),
            round: s.round,
            config_generation: s.config_generation.clone(),
            mode: s.mode.clone(),
            execution_profile: s.execution_profile.clone(),
            lease_id: s
                .lease
                .as_ref()
                .map(|l| l.lease_id.clone())
                .unwrap_or_default(),
            lease_generation: s.lease.as_ref().map_or(0, |l| l.generation),
            lease_operations: s
                .lease
                .as_ref()
                .map(|l| l.operations.clone())
                .unwrap_or_default(),
            projected_tools: s.projected_tools.clone(),
            projection_hash: s.projection_hash.clone(),
            skills: s.skills.clone(),
            snapshot_hash: h.clone(),
            recorded_at_ms: 0,
        })
        .collect();
    Ok(modbit_protocol::v1::CapabilitySnapshotList {
        snapshots,
        current_epoch: core
            .tools
            .epochs
            .latest(store, task)
            .map_or(0, |f| f.snapshot.epoch),
        valid: verdict.is_ok(),
        detail: verdict.err().unwrap_or_default(),
    })
}

/// The stamp a call's allowed policy decision carries, from the call's own
/// events (the write-ahead record at dispatch).
fn decision_stamp(store: &EventStore, call: &ToolCallId) -> Option<AuthorizationStamp> {
    let events = store
        .read_aggregate_of_types(call.as_bytes(), &["ToolCallPolicyDecision"], 0)
        .ok()?;
    events.into_iter().rev().find_map(|e| {
        let p = store.payload(&e.envelope).ok()?;
        if p["allowed"] != true {
            return None;
        }
        serde_json::from_value(p.get("authorization")?.clone()).ok()
    })
}

/// The whole check of the receipt chain: every hash recomputes and links to
/// its predecessor, and every receipt names the epoch of the decision it
/// executed under (REQ-PX-131).
///
/// # Errors
/// The first inconsistency.
pub(crate) fn verify_chain(store: &EventStore, chain: &[EffectReceipt]) -> Result<(), String> {
    modbit_policy::ledger::verify_chain(chain)?;
    verify_receipts(store, chain)
}

/// Verify that every receipt of `chain` names the epoch of the decision it
/// executed under and that the epoch's snapshot is the recorded one.
///
/// # Errors
/// The first receipt that disagrees with its decision or its snapshot.
pub(crate) fn verify_receipts(store: &EventStore, chain: &[EffectReceipt]) -> Result<(), String> {
    let mut hashes: HashMap<TaskId, HashMap<u64, String>> = HashMap::new();
    for r in chain {
        hashes.entry(r.task_id).or_insert_with(|| {
            records(store, r.task_id)
                .unwrap_or_default()
                .into_iter()
                .map(|(s, h)| (s.epoch, h))
                .collect()
        });
    }
    modbit_policy::epoch::verify_receipt_epochs(
        chain,
        |call| decision_stamp(store, call),
        |task, epoch| hashes.get(task)?.get(&epoch).cloned(),
    )
}
