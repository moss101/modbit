//! Capacity tickets in the Core (M6.2, REQ-EV-0272; docs/14 "Capacity
//! tickets", docs/33 "Scheduler"): the pool comes from the environment, a
//! run consumes a ticket for one model slot and one unit of provider quota
//! before its run record exists, renews it every turn under its lease
//! generation, and releases it when its loop ends; a refusal is a typed
//! record on the task (`CapacityDenied`), the task waits for capacity, and
//! nothing — no run, no worktree, no lease — was created. Subagent
//! admission (M6.3) takes its tickets through the same door.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use modbit_core_runtime::capacity::{CapacityPool, CapacityRefused, ResourceVector, Ticket};
use modbit_domain::event::{Actor, AggregateType};
use modbit_domain::task::{Task, TaskEvent};
use modbit_domain::{TaskId, Timestamp};
use modbit_event_store::EventStore;
use modbit_protocol::v1 as wire;

use crate::runtime::{Lineage, append, typed};
use crate::server::Core;

/// The host's pool and the lease length of a ticket.
pub(crate) struct Capacity {
    pool: Mutex<CapacityPool>,
    /// How long a ticket lives without renewal, ms.
    pub ttl_ms: i64,
    /// How long a paused task keeps its run's ticket before giving it back
    /// (`MODBIT_PAUSE_IDLE_BOUND_MS`, REQ-PX-101), ms.
    idle_bound_ms: i64,
    /// The tickets paused tasks hold.
    holds: Mutex<HashMap<TaskId, Hold>>,
}

/// A paused task's hold on the ticket its run took (REQ-PX-101): the
/// capacity stays the task's, renewed, until it is resumed (the same ticket
/// continues), cancelled, or has been paused for the idle bound (given back
/// with a typed record; a later resume re-enters admission).
#[derive(Clone, Debug)]
pub(crate) struct Hold {
    pub ticket_id: String,
    /// The lease generation the ticket was granted under.
    pub generation: u64,
    /// When the pause began, ms.
    pub since_ms: i64,
}

/// `MODBIT_CAPACITY` (`model=4,terminal=8,sandbox=2,browser=2,memory_mib=8192,provider=8`)
/// and `MODBIT_CAPACITY_TTL_MS`; a malformed spec refuses startup, as a
/// malformed policy file does (docs/23).
pub(crate) fn from_env() -> Result<Capacity, String> {
    let defaults = ResourceVector {
        model_concurrency: 4,
        terminal_slots: 8,
        sandbox_slots: 2,
        browser_slots: 2,
        memory_mib: 8192,
        provider_quota: 8,
    };
    let limits = match std::env::var("MODBIT_CAPACITY") {
        Ok(spec) if !spec.trim().is_empty() => {
            ResourceVector::parse(&spec, defaults).map_err(|e| format!("MODBIT_CAPACITY: {e}"))?
        }
        _ => defaults,
    };
    let ttl_ms = std::env::var("MODBIT_CAPACITY_TTL_MS")
        .ok()
        .and_then(|v| v.parse::<i64>().ok())
        .filter(|v| *v > 0)
        .unwrap_or(120_000);
    let idle_bound_ms = std::env::var("MODBIT_PAUSE_IDLE_BOUND_MS")
        .ok()
        .and_then(|v| v.parse::<i64>().ok())
        .filter(|v| *v > 0)
        .unwrap_or(15 * 60 * 1000);
    Ok(Capacity {
        pool: Mutex::new(CapacityPool::new(limits)),
        ttl_ms,
        idle_bound_ms,
        holds: Mutex::new(HashMap::new()),
    })
}

impl Capacity {
    /// Consume a ticket for `needs` on behalf of `holder`, recording the
    /// grant or the refusal on the task. A refusal reserves nothing.
    ///
    /// # Errors
    /// [`CapacityRefused`] as the pool said it.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn acquire(
        &self,
        store: &mut EventStore,
        core: &Core,
        task: &Task,
        lt: Lineage,
        actor: &Actor,
        holder: &str,
        needs: ResourceVector,
        generation: u64,
    ) -> Result<Ticket, CapacityRefused> {
        let now = Timestamp::now().0;
        let (result, lapsed) = {
            let mut pool = self.pool.lock().expect("capacity pool");
            let lapsed = pool.expire(now);
            let r = pool.allocate(holder, needs, now, now + self.ttl_ms, generation);
            (r, lapsed)
        };
        let mut events: Vec<modbit_event_store::NewEvent> = lapsed
            .iter()
            .map(|t| {
                typed(
                    "CapacityTicketReleased",
                    &TaskEvent::CapacityTicketReleased {
                        ticket_id: t.ticket_id.clone(),
                        holder: t.holder.clone(),
                        reason: "LAPSED".into(),
                    },
                    actor.clone(),
                )
            })
            .collect();
        match &result {
            Ok(t) => events.push(typed(
                "CapacityTicketGranted",
                &TaskEvent::CapacityTicketGranted {
                    ticket_id: t.ticket_id.clone(),
                    holder: t.holder.clone(),
                    holds: serde_json::to_value(t.holds).unwrap_or_default(),
                    expires_at_ms: t.expires_at_ms,
                    generation: t.generation,
                },
                actor.clone(),
            )),
            Err(e) => {
                let (dimension, needed, available, live) = match e {
                    CapacityRefused::CapacityExhausted { shortfall, live } => (
                        shortfall.dimension.clone(),
                        shortfall.needed,
                        shortfall.available,
                        live.clone(),
                    ),
                    CapacityRefused::ExceedsPool { shortfall } => (
                        shortfall.dimension.clone(),
                        shortfall.needed,
                        shortfall.available,
                        vec![],
                    ),
                    _ => (String::new(), 0, 0, vec![]),
                };
                events.push(typed(
                    "CapacityDenied",
                    &TaskEvent::CapacityDenied {
                        holder: holder.to_owned(),
                        needs: serde_json::to_value(needs).unwrap_or_default(),
                        code: e.code().into(),
                        dimension,
                        needed,
                        available,
                        live,
                    },
                    actor.clone(),
                ));
            }
        }
        let _ = append(
            store,
            core,
            lt,
            AggregateType::Task,
            *task.task_id.as_bytes(),
            events,
        );
        result
    }

    /// Renew a ticket's lease under `generation`; a stale owner's renewal
    /// is refused and the ticket lapses at its expiry.
    pub(crate) fn renew(&self, ticket_id: &str, generation: u64) -> Result<(), CapacityRefused> {
        let now = Timestamp::now().0;
        let mut pool = self.pool.lock().expect("capacity pool");
        pool.renew(ticket_id, now + self.ttl_ms, generation)
            .map(|_| ())
    }

    /// Release a ticket, recording it on the task; idempotent.
    pub(crate) fn release(
        &self,
        store: &mut EventStore,
        core: &Core,
        task: &Task,
        lt: Lineage,
        actor: &Actor,
        ticket_id: &str,
    ) {
        let released = {
            let mut pool = self.pool.lock().expect("capacity pool");
            pool.release(ticket_id)
        };
        if let Some(t) = released {
            let _ = append(
                store,
                core,
                lt,
                AggregateType::Task,
                *task.task_id.as_bytes(),
                vec![typed(
                    "CapacityTicketReleased",
                    &TaskEvent::CapacityTicketReleased {
                        ticket_id: t.ticket_id,
                        holder: t.holder,
                        reason: "RELEASED".into(),
                    },
                    actor.clone(),
                )],
            );
        }
    }

    /// The idle bound a pause holds its ticket for, ms.
    pub(crate) fn idle_bound_ms(&self) -> i64 {
        self.idle_bound_ms
    }

    /// A run that a person paused keeps its ticket: record the hold, and let
    /// [`spawn_hold_reaper`] keep it alive and give it back at the bound.
    pub(crate) fn hold(&self, task: TaskId, ticket_id: &str, generation: u64) {
        self.holds.lock().expect("holds").insert(
            task,
            Hold {
                ticket_id: ticket_id.to_owned(),
                generation,
                since_ms: Timestamp::now().0,
            },
        );
    }

    /// The task's hold, if it has one.
    pub(crate) fn hold_of(&self, task: &TaskId) -> Option<Hold> {
        self.holds.lock().expect("holds").get(task).cloned()
    }

    /// Resume: the ticket the pause kept, renewed under the resumer's lease
    /// generation and handed back to the run — or `None` when the pause no
    /// longer holds one (given back at the bound, lapsed), so the resume goes
    /// through admission like any start. Taking the hold ends it.
    pub(crate) fn reuse_hold(&self, task: &TaskId, generation: u64) -> Option<String> {
        let hold = self.holds.lock().expect("holds").remove(task)?;
        let now = Timestamp::now().0;
        let mut pool = self.pool.lock().expect("capacity pool");
        let _ = pool.expire(now);
        match pool.renew(&hold.ticket_id, now + self.ttl_ms, generation) {
            Ok(_) => Some(hold.ticket_id),
            Err(_) => {
                // A ticket that cannot be renewed is not ours to keep.
                pool.release(&hold.ticket_id);
                None
            }
        }
    }

    /// Remove the hold only when it is still `ticket_id`'s; `true` when this
    /// call ended it (so exactly one of resume, cancel and the reaper gives
    /// the ticket back).
    fn end_hold(&self, task: &TaskId, ticket_id: &str) -> bool {
        let mut holds = self.holds.lock().expect("holds");
        if holds.get(task).is_some_and(|h| h.ticket_id == ticket_id) {
            holds.remove(task);
            true
        } else {
            false
        }
    }

    /// A paused task is cancelled: the ticket it kept goes back to the pool.
    pub(crate) fn drop_hold(
        &self,
        store: &mut EventStore,
        core: &Core,
        task: &Task,
        lt: Lineage,
        actor: &Actor,
    ) {
        let Some(hold) = self.hold_of(&task.task_id) else {
            return;
        };
        if self.end_hold(&task.task_id, &hold.ticket_id) {
            self.release(store, core, task, lt, actor, &hold.ticket_id);
        }
    }

    /// The pool as a client sees it.
    pub(crate) fn view(&self) -> wire::CapacityView {
        let now = Timestamp::now().0;
        let mut pool = self.pool.lock().expect("capacity pool");
        let _ = pool.expire(now);
        let vector = |v: ResourceVector| wire::ResourceVectorView {
            model_concurrency: v.model_concurrency,
            terminal_slots: v.terminal_slots,
            sandbox_slots: v.sandbox_slots,
            browser_slots: v.browser_slots,
            memory_mib: v.memory_mib,
            provider_quota: v.provider_quota,
        };
        wire::CapacityView {
            limits: Some(vector(pool.limits)),
            held: Some(vector(pool.held())),
            available: Some(vector(pool.available())),
            ttl_ms: self.ttl_ms,
            tickets: pool
                .tickets
                .iter()
                .map(|t| wire::CapacityTicketView {
                    ticket_id: t.ticket_id.clone(),
                    holder: t.holder.clone(),
                    holds: Some(vector(t.holds)),
                    granted_at_ms: t.granted_at_ms,
                    expires_at_ms: t.expires_at_ms,
                    generation: t.generation,
                })
                .collect(),
        }
    }
}

/// Keep a paused task's ticket alive until the idle bound, then give it back
/// with one typed record (`PausedCapacityReleased`). Ends quietly when the hold
/// is resumed, cancelled or gone first.
pub(crate) fn spawn_hold_reaper(core: Arc<Core>, task: Task, ticket_id: String) {
    tokio::spawn(async move {
        let renew_every = (core.capacity.ttl_ms / 3).max(50);
        loop {
            let Some(hold) = core.capacity.hold_of(&task.task_id) else {
                return;
            };
            if hold.ticket_id != ticket_id {
                return;
            }
            let bound = core.capacity.idle_bound_ms();
            let held = Timestamp::now().0 - hold.since_ms;
            if held >= bound {
                if core.capacity.end_hold(&task.task_id, &ticket_id) {
                    let released = core
                        .capacity
                        .pool
                        .lock()
                        .expect("capacity pool")
                        .release(&ticket_id);
                    if let Some(t) = released {
                        let mut store = core.store.lock().await;
                        let _ = append(
                            &mut store,
                            &core,
                            Lineage::task(core.tenant_id, task.session_id, task.task_id),
                            AggregateType::Task,
                            *task.task_id.as_bytes(),
                            vec![typed(
                                "PausedCapacityReleased",
                                &TaskEvent::PausedCapacityReleased {
                                    ticket_id: t.ticket_id,
                                    holder: t.holder,
                                    held_ms: u64::try_from(held).unwrap_or(0),
                                    idle_bound_ms: u64::try_from(bound).unwrap_or(0),
                                    reason: "IDLE_BOUND".into(),
                                },
                                Actor::Core("capacity".into()),
                            )],
                        );
                    }
                }
                return;
            }
            tokio::time::sleep(std::time::Duration::from_millis(
                u64::try_from((bound - held).min(renew_every)).unwrap_or(50),
            ))
            .await;
            // Keep the ticket alive for the pause: it is the task's until the
            // bound. One that can no longer be renewed has lapsed (or been
            // taken back); there is nothing left to hold.
            if core.capacity.renew(&ticket_id, hold.generation).is_err() {
                core.capacity.end_hold(&task.task_id, &ticket_id);
                return;
            }
        }
    });
}
