//! Hierarchical budgets (REQ-PX-116, docs/14 contract 5, docs/79 ADC-C03):
//! the arithmetic that makes a child's budget a slice of its parent's.
//!
//! A task's budget has four dimensions. Three are *consumable* and add up
//! across a task and its children: turns, tool calls and cost (minor units
//! of the active model registry). One is a *deadline*: wall clock. A child
//! runs beside its parent, so wall clock does not add; a child's deadline is
//! the parent's remaining time, never more.
//!
//! At admission the child's request is clamped to what the parent has left —
//! its cap minus its own spend minus what its other children hold — and the
//! clamped amount is reserved against the parent. A live child holds the
//! larger of its reservation and what it has spent; a child that is not
//! running (waiting, parked, over) holds only what it spent, so the unused
//! part of its reservation returns to the parent. Admission is therefore
//! the one place the parent's cap can be exceeded through a child, and it
//! refuses to.
//!
//! This module is pure: no I/O, no clocks, no store. The Core reads the log
//! into [`Held`] values and asks here.

use serde::{Deserialize, Serialize};

/// Rounds of its own cost the parent keeps at admission, at the rate it has
/// been spending (see [`remaining_for_children`]).
pub const PARENT_RESERVE_ROUNDS: u64 = 2;

/// What the parent keeps for itself at admission, so delegating never
/// starves the agent that has to wait for and merge the result.
pub const PARENT_RESERVE_TURNS: u64 = 2;
/// Tool calls the parent keeps (see [`PARENT_RESERVE_TURNS`]).
pub const PARENT_RESERVE_TOOL_CALLS: u64 = 4;

/// An amount of the three consumable dimensions, plus elapsed wall clock.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Held {
    /// Turns.
    pub turns: u64,
    /// Tool calls.
    pub tool_calls: u64,
    /// Cost, minor units.
    pub cost_minor: u64,
    /// Wall clock, milliseconds (a deadline: never summed across siblings).
    pub wall_ms: u64,
}

impl Held {
    /// Both operands' consumable dimensions added; wall clock is the larger
    /// (children run beside the parent, not after it).
    #[must_use]
    pub fn plus(self, o: Self) -> Self {
        Self {
            turns: self.turns.saturating_add(o.turns),
            tool_calls: self.tool_calls.saturating_add(o.tool_calls),
            cost_minor: self.cost_minor.saturating_add(o.cost_minor),
            wall_ms: self.wall_ms.max(o.wall_ms),
        }
    }

    /// The consumable dimensions' larger of the two.
    #[must_use]
    pub fn max_consumable(self, o: Self) -> Self {
        Self {
            turns: self.turns.max(o.turns),
            tool_calls: self.tool_calls.max(o.tool_calls),
            cost_minor: self.cost_minor.max(o.cost_minor),
            wall_ms: self.wall_ms.max(o.wall_ms),
        }
    }
}

/// A cap. `None` on cost or wall clock is "no cap".
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Caps {
    /// Turns.
    pub turns: u64,
    /// Tool calls.
    pub tool_calls: u64,
    /// Cost, minor units.
    pub cost_minor: Option<u64>,
    /// Wall clock, milliseconds.
    pub wall_ms: Option<u64>,
}

/// One child as the parent's accounting sees it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChildHold {
    /// What admission reserved (cost and wall 0 = the parent had no cap).
    pub reserved: Held,
    /// What the child has spent, from its own log.
    pub spent: Held,
    /// Whether the child is running now (admitted, running, background).
    pub live: bool,
}

impl ChildHold {
    /// What the child holds against its parent: a live child the larger of
    /// its reservation and its spend (a round can overshoot a reservation by
    /// the round's own cost, and the overshoot is the parent's too); any
    /// other child exactly what it spent.
    #[must_use]
    pub fn held(&self) -> Held {
        if self.live {
            self.reserved.max_consumable(self.spent)
        } else {
            self.spent
        }
    }
}

/// What the parent's children hold in total (consumable dimensions).
#[must_use]
pub fn committed(children: &[ChildHold]) -> Held {
    children.iter().fold(Held::default(), |acc, c| {
        let h = c.held();
        Held {
            turns: acc.turns.saturating_add(h.turns),
            tool_calls: acc.tool_calls.saturating_add(h.tool_calls),
            cost_minor: acc.cost_minor.saturating_add(h.cost_minor),
            wall_ms: 0,
        }
    })
}

/// What a parent has left for a new child.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Remaining {
    /// Turns.
    pub turns: u64,
    /// Tool calls.
    pub tool_calls: u64,
    /// Cost; `None` = the parent has no cost cap.
    pub cost_minor: Option<u64>,
    /// Wall clock until the parent's deadline; `None` = no deadline.
    pub wall_ms: Option<u64>,
}

/// The parent's cap less its own spend and what its children hold.
#[must_use]
pub fn remaining(cap: &Caps, own: &Held, children: &Held) -> Remaining {
    Remaining {
        turns: cap
            .turns
            .saturating_sub(own.turns)
            .saturating_sub(children.turns),
        tool_calls: cap
            .tool_calls
            .saturating_sub(own.tool_calls)
            .saturating_sub(children.tool_calls),
        cost_minor: cap.cost_minor.map(|c| {
            c.saturating_sub(own.cost_minor)
                .saturating_sub(children.cost_minor)
        }),
        wall_ms: cap.wall_ms.map(|w| w.saturating_sub(own.wall_ms)),
    }
}

/// What a parent has left to give its children: [`remaining`], less what
/// the parent keeps to wait for them and finish — its own average cost per
/// turn so far, for [`PARENT_RESERVE_ROUNDS`] rounds. Without it a parent that
/// reserved its whole cap for children would be exhausted by its own next
/// round boundary and could never collect their results.
#[must_use]
pub fn remaining_for_children(cap: &Caps, own: &Held, children: &Held) -> Remaining {
    let mut rem = remaining(cap, own, children);
    let per_turn = own.cost_minor / own.turns.max(1);
    let keep = per_turn.saturating_mul(PARENT_RESERVE_ROUNDS);
    rem.cost_minor = rem.cost_minor.map(|c| c.saturating_sub(keep));
    rem
}

/// What a spawn asked for; 0 is "not stated".
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Request {
    /// Turns.
    pub turns: u64,
    /// Tool calls.
    pub tool_calls: u64,
    /// Cost, minor units.
    pub cost_minor: u64,
    /// Wall clock, milliseconds.
    pub wall_ms: u64,
}

/// What admission grants and reserves.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Grant {
    /// Turns.
    pub turns: u64,
    /// Tool calls.
    pub tool_calls: u64,
    /// Cost; `None` = the parent has no cost cap, so the child has none.
    pub cost_minor: Option<u64>,
    /// Wall clock; `None` = the parent has no deadline.
    pub wall_ms: Option<u64>,
}

/// Why a child cannot be admitted.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Insufficient {
    /// The dimension that left nothing to give.
    pub dimension: &'static str,
    /// What the parent has left in it.
    pub remaining: u64,
}

impl std::fmt::Display for Insufficient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "the parent has {} {} left to give a child",
            self.remaining, self.dimension
        )
    }
}

/// The turns a child gets when the spawn names none, before the clamp.
pub const DEFAULT_CHILD_TURNS: u64 = 20;

/// Clamp a request to the parent's remainder (the reservation is the
/// returned grant).
///
/// An unstated cost is a fair share: the parent's remainder split over the
/// parent and the child slots still free (`slots_left`), so one child with
/// no stated cost cannot take everything the parent has. An unstated wall
/// clock is the parent's remaining time. A stated amount is never raised,
/// only clamped.
///
/// # Errors
/// Nothing left to give in turns, tool calls or cost — the child would be
/// born exhausted.
pub fn clamp(
    req: &Request,
    default_tool_calls: u64,
    rem: &Remaining,
    slots_left: u32,
) -> Result<Grant, Insufficient> {
    let turn_room = rem.turns.saturating_sub(PARENT_RESERVE_TURNS);
    let call_room = rem.tool_calls.saturating_sub(PARENT_RESERVE_TOOL_CALLS);
    // An unstated amount is the default or a fair share of the room — the
    // parent and the child slots still free each take an equal part — so one
    // child that states nothing cannot take what the others would need.
    let share = |room: u64| (room / (u64::from(slots_left) + 1)).max(1);
    let want_turns = if req.turns == 0 {
        DEFAULT_CHILD_TURNS.min(share(turn_room))
    } else {
        req.turns
    };
    let want_calls = if req.tool_calls == 0 {
        default_tool_calls.min(share(call_room))
    } else {
        req.tool_calls
    };
    let turns = want_turns.min(turn_room);
    if turns == 0 {
        return Err(Insufficient {
            dimension: "turns",
            remaining: rem.turns,
        });
    }
    let tool_calls = want_calls.min(call_room);
    if tool_calls == 0 {
        return Err(Insufficient {
            dimension: "tool calls",
            remaining: rem.tool_calls,
        });
    }
    let cost_minor = match rem.cost_minor {
        None => None,
        Some(left) => {
            let want = if req.cost_minor == 0 {
                left / (u64::from(slots_left) + 1)
            } else {
                req.cost_minor
            };
            let grant = want.min(left);
            if grant == 0 {
                return Err(Insufficient {
                    dimension: "cost",
                    remaining: left,
                });
            }
            Some(grant)
        }
    };
    let wall_ms = match rem.wall_ms {
        None => None,
        Some(left) => {
            let want = if req.wall_ms == 0 { left } else { req.wall_ms };
            let grant = want.min(left);
            if grant == 0 {
                return Err(Insufficient {
                    dimension: "wall clock",
                    remaining: left,
                });
            }
            Some(grant)
        }
    };
    // A child the parent did not cap in cost or wall clock still carries
    // whatever the spawn itself asked for: that is its own, stated limit.
    let cost_minor = cost_minor.or((req.cost_minor > 0).then_some(req.cost_minor));
    let wall_ms = wall_ms.or((req.wall_ms > 0).then_some(req.wall_ms));
    Ok(Grant {
        turns,
        tool_calls,
        cost_minor,
        wall_ms,
    })
}

/// What a child that already spent `used` may have when it runs again
/// (follow-up, resume after a park or a restart): the capsule's own cap,
/// less than which the parent's remainder — reserved afresh — allows. A
/// dimension with nothing left comes back equal to `used`, so the child's
/// first round boundary ends it `BUDGET_EXHAUSTED` rather than hiding a
/// refusal.
#[must_use]
pub fn regrant(capsule: &Caps, used: &Held, rem: &Remaining) -> Caps {
    let turns = capsule.turns.min(
        used.turns
            .saturating_add(rem.turns.saturating_sub(PARENT_RESERVE_TURNS)),
    );
    let tool_calls = capsule.tool_calls.min(
        used.tool_calls
            .saturating_add(rem.tool_calls.saturating_sub(PARENT_RESERVE_TOOL_CALLS)),
    );
    let cost_minor = match (capsule.cost_minor, rem.cost_minor) {
        (Some(c), Some(r)) => Some(c.min(used.cost_minor.saturating_add(r))),
        (None, Some(r)) => Some(used.cost_minor.saturating_add(r)),
        (c, None) => c,
    };
    let wall_ms = match (capsule.wall_ms, rem.wall_ms) {
        (Some(c), Some(r)) => Some(c.min(used.wall_ms.saturating_add(r))),
        (None, Some(r)) => Some(used.wall_ms.saturating_add(r)),
        (c, None) => c,
    };
    Caps {
        turns,
        tool_calls,
        cost_minor,
        wall_ms,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn caps(turns: u64, calls: u64, cost: Option<u64>, wall: Option<u64>) -> Caps {
        Caps {
            turns,
            tool_calls: calls,
            cost_minor: cost,
            wall_ms: wall,
        }
    }

    #[test]
    fn the_third_child_is_clamped_to_what_the_first_two_left() {
        let cap = caps(200, 1_000, Some(100), None);
        let own = Held {
            cost_minor: 10,
            ..Held::default()
        };
        let mut kids: Vec<ChildHold> = Vec::new();
        let mut grants = Vec::new();
        for _ in 0..3 {
            let rem = remaining(&cap, &own, &committed(&kids));
            let g = clamp(
                &Request {
                    cost_minor: 40,
                    ..Request::default()
                },
                100,
                &rem,
                3,
            )
            .unwrap();
            kids.push(ChildHold {
                reserved: Held {
                    turns: g.turns,
                    tool_calls: g.tool_calls,
                    cost_minor: g.cost_minor.unwrap(),
                    wall_ms: 0,
                },
                spent: Held::default(),
                live: true,
            });
            grants.push(g.cost_minor.unwrap());
        }
        assert_eq!(
            grants,
            vec![40, 40, 10],
            "90 of 100 after the parent's own 10"
        );
        assert_eq!(
            own.cost_minor + committed(&kids).cost_minor,
            cap.cost_minor.unwrap()
        );
        // Nothing is left for a fourth.
        let rem = remaining(&cap, &own, &committed(&kids));
        let e = clamp(&Request::default(), 100, &rem, 0).unwrap_err();
        assert_eq!(e.dimension, "cost");
    }

    #[test]
    fn an_unused_reservation_returns_when_the_child_stops_running() {
        let cap = caps(60, 300, Some(100), None);
        let own = Held::default();
        let reserved = Held {
            turns: 20,
            tool_calls: 50,
            cost_minor: 60,
            wall_ms: 0,
        };
        let spent = Held {
            turns: 4,
            tool_calls: 9,
            cost_minor: 17,
            wall_ms: 0,
        };
        let live = ChildHold {
            reserved,
            spent,
            live: true,
        };
        let over = ChildHold {
            live: false,
            ..live
        };
        assert_eq!(
            remaining(&cap, &own, &committed(&[live])).cost_minor,
            Some(40)
        );
        assert_eq!(
            remaining(&cap, &own, &committed(&[over])).cost_minor,
            Some(83),
            "the 43 unused come back"
        );
    }

    #[test]
    fn a_live_child_that_overshot_holds_its_spend_not_its_reservation() {
        let c = ChildHold {
            reserved: Held {
                cost_minor: 10,
                ..Held::default()
            },
            spent: Held {
                cost_minor: 14,
                ..Held::default()
            },
            live: true,
        };
        assert_eq!(c.held().cost_minor, 14);
    }

    #[test]
    fn wall_clock_is_a_deadline_not_a_sum() {
        let cap = caps(60, 300, None, Some(10_000));
        let own = Held {
            wall_ms: 4_000,
            ..Held::default()
        };
        let rem = remaining(&cap, &own, &Held::default());
        assert_eq!(rem.wall_ms, Some(6_000));
        let a = clamp(&Request::default(), 100, &rem, 3).unwrap();
        let b = clamp(
            &Request {
                wall_ms: 99_000,
                ..Request::default()
            },
            100,
            &rem,
            3,
        )
        .unwrap();
        assert_eq!((a.wall_ms, b.wall_ms), (Some(6_000), Some(6_000)));
    }

    #[test]
    fn the_parent_keeps_a_reserve_of_turns_and_calls() {
        let rem = Remaining {
            turns: 5,
            tool_calls: 10,
            cost_minor: None,
            wall_ms: None,
        };
        let g = clamp(
            &Request {
                turns: 50,
                tool_calls: 500,
                ..Request::default()
            },
            100,
            &rem,
            1,
        )
        .unwrap();
        assert_eq!((g.turns, g.tool_calls), (3, 6));
        let none = Remaining { turns: 2, ..rem };
        assert_eq!(
            clamp(&Request::default(), 100, &none, 1)
                .unwrap_err()
                .dimension,
            "turns"
        );
    }

    #[test]
    fn a_regrant_never_exceeds_the_capsule_or_the_parents_remainder() {
        let capsule = caps(20, 100, Some(60), Some(30_000));
        let used = Held {
            turns: 12,
            tool_calls: 40,
            cost_minor: 50,
            wall_ms: 9_000,
        };
        let rem = Remaining {
            turns: 8,
            tool_calls: 3,
            cost_minor: Some(4),
            wall_ms: Some(1_000),
        };
        let g = regrant(&capsule, &used, &rem);
        assert_eq!(g.turns, 18);
        assert_eq!(
            g.tool_calls, 40,
            "the parent has nothing beyond its reserve"
        );
        assert_eq!(g.cost_minor, Some(54));
        assert_eq!(g.wall_ms, Some(10_000));
    }

    proptest! {
        /// However the parent's cap, the children's requests and the order
        /// in which children stop running, what the children hold plus the
        /// parent's own spend never exceeds the cap (consumables), and no
        /// grant is past the parent's deadline.
        #[test]
        fn children_can_never_exceed_the_parents_cap(
            cap_turns in 6u64..200,
            cap_calls in 10u64..500,
            cap_cost in 0u64..10_000,
            cap_wall in 0u64..100_000,
            own_turns in 0u64..6,
            own_cost in 0u64..100,
            ops in proptest::collection::vec(
                (0u64..80, 0u64..300, 0u64..6_000, 0u64..60_000, 0u32..5, any::<bool>(), 0u64..100),
                1..24,
            ),
        ) {
            let cap = Caps {
                turns: cap_turns,
                tool_calls: cap_calls,
                cost_minor: (cap_cost > 0).then_some(cap_cost),
                wall_ms: (cap_wall > 0).then_some(cap_wall),
            };
            let own = Held { turns: own_turns.min(cap_turns), tool_calls: 0, cost_minor: own_cost.min(cap_cost), wall_ms: 0 };
            let mut kids: Vec<ChildHold> = Vec::new();
            for (turns, calls, cost, wall, slots, stop_one, spend_pct) in ops {
                // A child may stop running (its reservation returns).
                if stop_one && let Some(k) = kids.iter_mut().find(|k| k.live) {
                    k.live = false;
                }
                let rem = remaining_for_children(&cap, &own, &committed(&kids));
                let req = Request { turns, tool_calls: calls, cost_minor: cost, wall_ms: wall };
                if let Ok(g) = clamp(&req, 300, &rem, slots) {
                    prop_assert!(g.turns <= rem.turns);
                    prop_assert!(g.tool_calls <= rem.tool_calls);
                    if let (Some(gc), Some(rc)) = (g.cost_minor, rem.cost_minor) {
                        prop_assert!(gc <= rc);
                    }
                    if let (Some(gw), Some(rw)) = (g.wall_ms, rem.wall_ms) {
                        prop_assert!(gw <= rw);
                    }
                    let reserved = Held {
                        turns: g.turns,
                        tool_calls: g.tool_calls,
                        cost_minor: if cap.cost_minor.is_some() { g.cost_minor.unwrap_or(0) } else { 0 },
                        wall_ms: 0,
                    };
                    // The child spends a share of its grant, never more.
                    let spent = Held {
                        turns: reserved.turns * spend_pct / 100,
                        tool_calls: reserved.tool_calls * spend_pct / 100,
                        cost_minor: reserved.cost_minor * spend_pct / 100,
                        wall_ms: 0,
                    };
                    kids.push(ChildHold { reserved, spent, live: true });
                }
                let total = committed(&kids);
                prop_assert!(own.turns + total.turns <= cap.turns);
                prop_assert!(own.tool_calls + total.tool_calls <= cap.tool_calls);
                if let Some(c) = cap.cost_minor {
                    prop_assert!(own.cost_minor + total.cost_minor <= c, "cost {} + {} > {}", own.cost_minor, total.cost_minor, c);
                }
            }
        }

        /// A regranted child is never given more than it had, and never more
        /// than the parent can now cover.
        #[test]
        fn a_regrant_is_bounded_by_both_sides(
            ct in 1u64..100, cc in 1u64..400, cost in 0u64..5_000,
            ut in 0u64..100, uc in 0u64..400, ucost in 0u64..5_000,
            rt in 0u64..100, rc in 0u64..400, rcost in 0u64..5_000,
        ) {
            let capsule = Caps { turns: ct, tool_calls: cc, cost_minor: (cost > 0).then_some(cost), wall_ms: None };
            let used = Held { turns: ut.min(ct), tool_calls: uc.min(cc), cost_minor: ucost.min(cost), wall_ms: 0 };
            let rem = Remaining { turns: rt, tool_calls: rc, cost_minor: Some(rcost), wall_ms: None };
            let g = regrant(&capsule, &used, &rem);
            prop_assert!(g.turns <= capsule.turns);
            prop_assert!(g.tool_calls <= capsule.tool_calls);
            prop_assert!(g.turns <= used.turns + rt);
            prop_assert!(g.cost_minor.unwrap() <= used.cost_minor + rcost);
            if let Some(c) = capsule.cost_minor { prop_assert!(g.cost_minor.unwrap() <= c); }
        }
    }
}
