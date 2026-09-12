//! Capacity tickets in the Core (M6.2, REQ-EV-0272; docs/14 "Capacity
//! tickets", docs/33 "Scheduler"): the pool comes from the environment, a
//! run consumes a ticket for one model slot and one unit of provider quota
//! before its run record exists, renews it every turn under its lease
//! generation, and releases it when its loop ends; a refusal is a typed
//! record on the task (`CapacityDenied`), the task waits for capacity, and
//! nothing — no run, no worktree, no lease — was created. Subagent
//! admission (M6.3) takes its tickets through the same door.

use std::sync::Mutex;

use modbit_core_runtime::capacity::{CapacityPool, CapacityRefused, ResourceVector, Ticket};
use modbit_domain::Timestamp;
use modbit_domain::event::{Actor, AggregateType};
use modbit_domain::task::{Task, TaskEvent};
use modbit_event_store::EventStore;
use modbit_protocol::v1 as wire;

use crate::runtime::{Lineage, append, typed};
use crate::server::Core;

/// The host's pool and the lease length of a ticket.
pub(crate) struct Capacity {
    pool: Mutex<CapacityPool>,
    /// How long a ticket lives without renewal, ms.
    pub ttl_ms: i64,
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
    Ok(Capacity {
        pool: Mutex::new(CapacityPool::new(limits)),
        ttl_ms,
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
