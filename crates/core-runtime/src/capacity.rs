//! Capacity tickets (docs/14 "Capacity tickets", REQ-EV-0272; M6.2): capacity
//! is a typed resource vector — model concurrency, terminal slots, sandbox
//! slots, browser slots, memory, provider quota — not an agent count. A
//! holder consumes a ticket for the vector it needs before it spawns, runs a
//! sandbox or opens a terminal; a ticket has a lease expiry and is fenced by
//! the generation it was granted under, so a stale holder cannot keep
//! capacity a newer owner needs. The pool is pure: the Core owns the clock,
//! the persistence and the effects; this module only says what fits.

use serde::{Deserialize, Serialize};

/// The dimensions of capacity, each an integer count of units.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ResourceVector {
    /// Concurrent model invocations.
    #[serde(default)]
    pub model_concurrency: u32,
    /// Terminal / process slots.
    #[serde(default)]
    pub terminal_slots: u32,
    /// Sandbox leases.
    #[serde(default)]
    pub sandbox_slots: u32,
    /// Browser sessions.
    #[serde(default)]
    pub browser_slots: u32,
    /// Memory, in MiB.
    #[serde(default)]
    pub memory_mib: u32,
    /// Provider quota units (requests in flight against the provider).
    #[serde(default)]
    pub provider_quota: u32,
}

impl ResourceVector {
    /// The vector every dimension of which is `n`.
    #[must_use]
    pub const fn splat(n: u32) -> Self {
        Self {
            model_concurrency: n,
            terminal_slots: n,
            sandbox_slots: n,
            browser_slots: n,
            memory_mib: n,
            provider_quota: n,
        }
    }

    /// What one agent run needs by default: one model slot and one unit of
    /// provider quota; terminals, sandboxes and browsers are taken when
    /// they are opened.
    #[must_use]
    pub const fn one_run() -> Self {
        Self {
            model_concurrency: 1,
            terminal_slots: 0,
            sandbox_slots: 0,
            browser_slots: 0,
            memory_mib: 0,
            provider_quota: 1,
        }
    }

    fn dims(&self) -> [(&'static str, u32); 6] {
        [
            ("model_concurrency", self.model_concurrency),
            ("terminal_slots", self.terminal_slots),
            ("sandbox_slots", self.sandbox_slots),
            ("browser_slots", self.browser_slots),
            ("memory_mib", self.memory_mib),
            ("provider_quota", self.provider_quota),
        ]
    }

    fn get(&self, dim: &str) -> u32 {
        self.dims()
            .iter()
            .find(|(d, _)| *d == dim)
            .map_or(0, |(_, v)| *v)
    }

    /// Whether every dimension of `self` is zero.
    #[must_use]
    pub fn is_zero(&self) -> bool {
        self.dims().iter().all(|(_, v)| *v == 0)
    }

    /// Saturating element-wise sum.
    #[must_use]
    pub fn plus(&self, other: &Self) -> Self {
        Self {
            model_concurrency: self
                .model_concurrency
                .saturating_add(other.model_concurrency),
            terminal_slots: self.terminal_slots.saturating_add(other.terminal_slots),
            sandbox_slots: self.sandbox_slots.saturating_add(other.sandbox_slots),
            browser_slots: self.browser_slots.saturating_add(other.browser_slots),
            memory_mib: self.memory_mib.saturating_add(other.memory_mib),
            provider_quota: self.provider_quota.saturating_add(other.provider_quota),
        }
    }

    /// Saturating element-wise difference.
    #[must_use]
    pub fn minus(&self, other: &Self) -> Self {
        Self {
            model_concurrency: self
                .model_concurrency
                .saturating_sub(other.model_concurrency),
            terminal_slots: self.terminal_slots.saturating_sub(other.terminal_slots),
            sandbox_slots: self.sandbox_slots.saturating_sub(other.sandbox_slots),
            browser_slots: self.browser_slots.saturating_sub(other.browser_slots),
            memory_mib: self.memory_mib.saturating_sub(other.memory_mib),
            provider_quota: self.provider_quota.saturating_sub(other.provider_quota),
        }
    }

    /// The first dimension in which `self` does not cover `needs`, with
    /// what is available and what was needed.
    #[must_use]
    pub fn shortfall(&self, needs: &Self) -> Option<Shortfall> {
        let have = self.dims();
        needs
            .dims()
            .iter()
            .zip(have.iter())
            .find(|((_, n), (_, h))| n > h)
            .map(|((dim, n), (_, h))| Shortfall {
                dimension: (*dim).to_owned(),
                needed: *n,
                available: *h,
            })
    }

    /// Parse `model=4,terminal=8,sandbox=2,browser=1,memory_mib=8192,provider=8`;
    /// dimensions not named keep `base`. Unknown names and non-numbers are
    /// refused with the offending item.
    ///
    /// # Errors
    /// The item that could not be read.
    pub fn parse(spec: &str, base: Self) -> Result<Self, String> {
        let mut v = base;
        for item in spec.split(',').map(str::trim).filter(|s| !s.is_empty()) {
            let (k, val) = item
                .split_once('=')
                .ok_or_else(|| format!("`{item}`: expected name=count"))?;
            let n: u32 = val
                .trim()
                .parse()
                .map_err(|_| format!("`{item}`: `{val}` is not a count"))?;
            match k.trim() {
                "model" | "model_concurrency" => v.model_concurrency = n,
                "terminal" | "terminal_slots" => v.terminal_slots = n,
                "sandbox" | "sandbox_slots" => v.sandbox_slots = n,
                "browser" | "browser_slots" => v.browser_slots = n,
                "memory" | "memory_mib" => v.memory_mib = n,
                "provider" | "provider_quota" => v.provider_quota = n,
                other => return Err(format!("`{item}`: unknown dimension `{other}`")),
            }
        }
        Ok(v)
    }
}

/// Why a ticket was not granted: the first dimension that does not fit.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Shortfall {
    /// Dimension name.
    pub dimension: String,
    /// Units needed.
    pub needed: u32,
    /// Units available after the live tickets.
    pub available: u32,
}

/// A granted ticket: the vector it holds, who holds it, until when, and the
/// generation it was granted under (docs/13 "Fencing and epochs").
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Ticket {
    /// Ticket id, unique in the pool.
    pub ticket_id: String,
    /// The holder: `run:<id>`, `agent:<id>`, `terminal:<id>`, ….
    pub holder: String,
    /// What it holds.
    pub holds: ResourceVector,
    /// When it was granted, ms.
    pub granted_at_ms: i64,
    /// When it lapses unless renewed, ms.
    pub expires_at_ms: i64,
    /// The holder's lease generation at grant.
    pub generation: u64,
}

/// Why a ticket operation was refused.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "code", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CapacityRefused {
    /// The pool cannot cover the request now.
    CapacityExhausted {
        /// Which dimension, with the numbers.
        shortfall: Shortfall,
        /// Tickets alive at the time, by holder.
        live: Vec<String>,
    },
    /// The request needs more than the pool ever has: it would never fit.
    ExceedsPool {
        /// Which dimension, with the numbers.
        shortfall: Shortfall,
    },
    /// Nothing was asked for.
    EmptyRequest,
    /// No such ticket.
    UnknownTicket {
        /// Id.
        ticket_id: String,
    },
    /// The caller's generation is older than the ticket's.
    StaleGeneration {
        /// Id.
        ticket_id: String,
        /// The ticket's.
        ticket_generation: u64,
        /// The caller's.
        caller_generation: u64,
    },
}

impl CapacityRefused {
    /// Stable code.
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::CapacityExhausted { .. } => "CAPACITY_EXHAUSTED",
            Self::ExceedsPool { .. } => "EXCEEDS_POOL",
            Self::EmptyRequest => "EMPTY_REQUEST",
            Self::UnknownTicket { .. } => "UNKNOWN_TICKET",
            Self::StaleGeneration { .. } => "STALE_GENERATION",
        }
    }
}

/// The pool: what the host offers and the tickets alive against it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapacityPool {
    /// The whole vector.
    pub limits: ResourceVector,
    /// Live tickets, in grant order.
    pub tickets: Vec<Ticket>,
    /// Ticket ids issued so far (ids are `t-<n>`).
    pub issued: u64,
}

impl CapacityPool {
    /// A pool with `limits`.
    #[must_use]
    pub const fn new(limits: ResourceVector) -> Self {
        Self {
            limits,
            tickets: Vec::new(),
            issued: 0,
        }
    }

    /// Lapse every ticket whose expiry is at or before `now`; returns them.
    /// A lapsed ticket's holder is told through the event the Core records;
    /// nothing here stops the holder — the fence does, at its next append.
    pub fn expire(&mut self, now_ms: i64) -> Vec<Ticket> {
        let (lapsed, live): (Vec<Ticket>, Vec<Ticket>) = self
            .tickets
            .drain(..)
            .partition(|t| t.expires_at_ms <= now_ms);
        self.tickets = live;
        lapsed
    }

    /// The vector the live tickets hold.
    #[must_use]
    pub fn held(&self) -> ResourceVector {
        self.tickets
            .iter()
            .fold(ResourceVector::default(), |acc, t| acc.plus(&t.holds))
    }

    /// What is left for a new ticket.
    #[must_use]
    pub fn available(&self) -> ResourceVector {
        self.limits.minus(&self.held())
    }

    /// Grant a ticket for `needs` to `holder` until `expires_at_ms`, or
    /// refuse with the first dimension that does not fit. All or nothing:
    /// a refusal changes nothing (REQ-EV-0272: no partial side effects).
    ///
    /// # Errors
    /// [`CapacityRefused`].
    pub fn allocate(
        &mut self,
        holder: &str,
        needs: ResourceVector,
        now_ms: i64,
        expires_at_ms: i64,
        generation: u64,
    ) -> Result<Ticket, CapacityRefused> {
        if needs.is_zero() {
            return Err(CapacityRefused::EmptyRequest);
        }
        self.expire(now_ms);
        if let Some(shortfall) = self.limits.shortfall(&needs) {
            return Err(CapacityRefused::ExceedsPool { shortfall });
        }
        if let Some(shortfall) = self.available().shortfall(&needs) {
            return Err(CapacityRefused::CapacityExhausted {
                shortfall,
                live: self.tickets.iter().map(|t| t.holder.clone()).collect(),
            });
        }
        self.issued += 1;
        let ticket = Ticket {
            ticket_id: format!("t-{}", self.issued),
            holder: holder.to_owned(),
            holds: needs,
            granted_at_ms: now_ms,
            expires_at_ms,
            generation,
        };
        self.tickets.push(ticket.clone());
        Ok(ticket)
    }

    /// Extend a ticket's lease; a caller with an older generation than the
    /// ticket's cannot (a stale owner keeps nothing alive).
    ///
    /// # Errors
    /// [`CapacityRefused`].
    pub fn renew(
        &mut self,
        ticket_id: &str,
        expires_at_ms: i64,
        caller_generation: u64,
    ) -> Result<&Ticket, CapacityRefused> {
        let t = self
            .tickets
            .iter_mut()
            .find(|t| t.ticket_id == ticket_id)
            .ok_or_else(|| CapacityRefused::UnknownTicket {
                ticket_id: ticket_id.to_owned(),
            })?;
        if caller_generation < t.generation {
            return Err(CapacityRefused::StaleGeneration {
                ticket_id: ticket_id.to_owned(),
                ticket_generation: t.generation,
                caller_generation,
            });
        }
        t.expires_at_ms = expires_at_ms.max(t.expires_at_ms);
        Ok(t)
    }

    /// Release a ticket; returns it, or `None` when it was not live (a
    /// release is idempotent: a second release changes nothing).
    pub fn release(&mut self, ticket_id: &str) -> Option<Ticket> {
        let i = self.tickets.iter().position(|t| t.ticket_id == ticket_id)?;
        Some(self.tickets.remove(i))
    }

    /// Release every ticket of `holder`; returns them.
    pub fn release_holder(&mut self, holder: &str) -> Vec<Ticket> {
        let (gone, live): (Vec<Ticket>, Vec<Ticket>) =
            self.tickets.drain(..).partition(|t| t.holder == holder);
        self.tickets = live;
        gone
    }

    /// The live ticket of `holder`, if any.
    #[must_use]
    pub fn ticket_of(&self, holder: &str) -> Option<&Ticket> {
        self.tickets.iter().find(|t| t.holder == holder)
    }

    /// The dimension by name of what is available, for a view.
    #[must_use]
    pub fn available_in(&self, dimension: &str) -> u32 {
        self.available().get(dimension)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pool() -> CapacityPool {
        CapacityPool::new(
            ResourceVector::parse(
                "model=2,terminal=4,sandbox=1,browser=1,memory=4096,provider=2",
                ResourceVector::default(),
            )
            .unwrap(),
        )
    }

    #[test]
    fn a_ticket_is_all_or_nothing_and_exhaustion_names_the_dimension() {
        let mut p = pool();
        let a = p
            .allocate("run:a", ResourceVector::one_run(), 1_000, 61_000, 1)
            .unwrap();
        let _b = p
            .allocate("run:b", ResourceVector::one_run(), 1_000, 61_000, 1)
            .unwrap();
        assert_eq!(p.available().model_concurrency, 0);
        let before = p.clone();
        let err = p
            .allocate("run:c", ResourceVector::one_run(), 2_000, 62_000, 1)
            .unwrap_err();
        assert_eq!(err.code(), "CAPACITY_EXHAUSTED");
        assert!(matches!(
            &err,
            CapacityRefused::CapacityExhausted { shortfall, live }
                if shortfall.dimension == "model_concurrency"
                    && shortfall.needed == 1
                    && shortfall.available == 0
                    && live == &vec!["run:a".to_owned(), "run:b".to_owned()]
        ));
        assert_eq!(p, before, "a refusal reserves nothing");
        // A request the pool could never satisfy is said so.
        let err = p
            .allocate(
                "run:d",
                ResourceVector {
                    sandbox_slots: 3,
                    ..Default::default()
                },
                2_000,
                62_000,
                1,
            )
            .unwrap_err();
        assert_eq!(err.code(), "EXCEEDS_POOL");
        // Releasing one frees exactly its vector.
        assert!(p.release(&a.ticket_id).is_some());
        assert!(p.release(&a.ticket_id).is_none(), "release is idempotent");
        assert_eq!(p.available().model_concurrency, 1);
        assert!(
            p.allocate("run:c", ResourceVector::one_run(), 3_000, 63_000, 1)
                .is_ok()
        );
        assert_eq!(p.issued, 3);
    }

    #[test]
    fn tickets_lapse_at_expiry_and_a_stale_generation_cannot_renew() {
        let mut p = pool();
        let t = p
            .allocate("run:a", ResourceVector::one_run(), 1_000, 5_000, 2)
            .unwrap();
        // A newer owner may renew; an older one may not.
        assert!(p.renew(&t.ticket_id, 9_000, 2).is_ok());
        let err = p.renew(&t.ticket_id, 20_000, 1).unwrap_err();
        assert_eq!(err.code(), "STALE_GENERATION");
        assert_eq!(p.ticket_of("run:a").unwrap().expires_at_ms, 9_000);
        // At expiry the ticket lapses and its capacity returns.
        assert!(p.expire(8_999).is_empty());
        let lapsed = p.expire(9_000);
        assert_eq!(lapsed.len(), 1);
        assert_eq!(p.available().model_concurrency, 2);
        assert_eq!(
            p.renew(&t.ticket_id, 20_000, 2).unwrap_err().code(),
            "UNKNOWN_TICKET"
        );
        // An allocation expires the lapsed on the way.
        let _ = p
            .allocate("run:b", ResourceVector::one_run(), 10_000, 10_500, 3)
            .unwrap();
        let _ = p
            .allocate("run:c", ResourceVector::one_run(), 10_600, 20_000, 3)
            .unwrap();
        assert_eq!(p.tickets.len(), 1, "run:b lapsed before run:c was granted");
        assert_eq!(p.release_holder("run:c").len(), 1);
        assert!(p.tickets.is_empty());
    }

    #[test]
    fn the_vector_parses_and_refuses_what_it_cannot_read() {
        let base = ResourceVector::splat(1);
        let v = ResourceVector::parse("model=4, provider=8", base).unwrap();
        assert_eq!(
            (v.model_concurrency, v.provider_quota, v.terminal_slots),
            (4, 8, 1)
        );
        assert!(ResourceVector::parse("model=four", base).is_err());
        assert!(ResourceVector::parse("gpu=1", base).is_err());
        assert!(ResourceVector::parse("model", base).is_err());
        assert!(ResourceVector::default().is_zero());
        let mut p = CapacityPool::new(base);
        assert_eq!(
            p.allocate("x", ResourceVector::default(), 0, 1, 1)
                .unwrap_err()
                .code(),
            "EMPTY_REQUEST"
        );
    }
}
