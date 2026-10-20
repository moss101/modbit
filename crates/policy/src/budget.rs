//! The budget-raise policy (REQ-PX-060, QUAL-PX-060: "a limit action that
//! changes a budget without the policy's approval is refused"; docs/10 "maximum
//! request budget" is an organization control enforced by Core).
//!
//! A task's caps (`SetTaskBudgets`, REQ-PX-116) can be lowered by anyone who
//! holds the session. A *raise* spends more than the person or the
//! organization allowed, so it is a decision of the Capability Kernel's owner:
//!
//! * the organization's **ceiling** per dimension (cost, wall clock, live
//!   children) is hard: a raise whose new value is above it is refused with a
//!   typed reason and no approval can lift it (the ceiling is policy, not a
//!   per-task choice);
//! * a raise **within** the ceiling is allowed for the person holding the
//!   session, and needs an **exact-intent approval** when the actor is not a
//!   person (an agent, the Core, an external system: an autonomous run never
//!   raises its own budget) or when the organization says every raise asks
//!   (`raise_approval = always`);
//! * the approval binds the task, the caps in force, the caps asked for and
//!   the number of cap decisions already on the task's log, so it authorizes
//!   exactly one raise: once applied the log has moved on and the same hash
//!   no longer names anything;
//! * an unreadable policy fails closed: every raise is refused.
//!
//! This module is pure: the Core reads the caps from the task's log, asks
//! [`BudgetPolicy::decide`], and records the outcome on the log.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// The tool name an approval of a budget raise carries; the approval is for an
/// effect that is not a tool call, so the name is the effect's identity.
pub const RAISE_EFFECT: &str = "task.budget.raise";

/// The `ask_reason` code a budget-raise approval puts in its scope, in the
/// vocabulary of the approval card.
pub const ASK_REASON: &str = "BUDGET_RAISE";

/// Refusal: a raise above the organization's ceiling.
pub const CEILING_EXCEEDED: &str = "BUDGET_CEILING_EXCEEDED";
/// Refusal: the budget policy could not be read, so no raise is allowed.
pub const POLICY_INVALID: &str = "BUDGET_POLICY_INVALID";
/// The raise waits for a person's exact-intent approval.
pub const NEEDS_APPROVAL: &str = "BUDGET_RAISE_NEEDS_APPROVAL";
/// The raise was asked and a person denied it.
pub const DENIED: &str = "BUDGET_RAISE_DENIED";
/// The raise has an open approval that nobody has answered yet.
pub const APPROVAL_PENDING: &str = "BUDGET_RAISE_APPROVAL_PENDING";

/// The caps of a task as the runtime reads them: `0` is "no cap" for cost and
/// wall clock, and `children` is the effective live-children limit (`0` =
/// delegation forbidden), never the "0 = the default" wire form.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Caps {
    /// Cost cap in minor units; 0 = none.
    pub cost_minor: u64,
    /// Wall-clock cap in milliseconds; 0 = none.
    pub wall_ms: u64,
    /// Live children at once; 0 = no delegation.
    pub children: u32,
}

impl Caps {
    /// The caps a `SetTaskBudgets` states, resolved the way the runtime
    /// resolves them (`forbid_spawn` wins; `max_children == 0` is the default).
    #[must_use]
    pub fn from_set(
        max_cost_minor: u64,
        max_wall_ms: u64,
        max_children: u32,
        forbid_spawn: bool,
        default_children: u32,
    ) -> Self {
        Self {
            cost_minor: max_cost_minor,
            wall_ms: max_wall_ms,
            children: if forbid_spawn {
                0
            } else if max_children > 0 {
                max_children
            } else {
                default_children
            },
        }
    }

    /// The caps of a task that never had any set.
    #[must_use]
    pub const fn unset(default_children: u32) -> Self {
        Self {
            cost_minor: 0,
            wall_ms: 0,
            children: default_children,
        }
    }
}

fn eff(v: u64) -> u64 {
    if v == 0 { u64::MAX } else { v }
}

/// Which dimensions a change raises (a larger effective limit; "none" is the
/// largest of all).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Raised {
    /// The cost cap goes up (or is removed).
    pub cost: bool,
    /// The wall-clock cap goes up (or is removed).
    pub wall: bool,
    /// More live children are allowed.
    pub children: bool,
}

impl Raised {
    /// Whether anything is raised.
    #[must_use]
    pub const fn any(self) -> bool {
        self.cost || self.wall || self.children
    }
}

/// Which dimensions `to` raises over `from`.
#[must_use]
pub fn raised(from: &Caps, to: &Caps) -> Raised {
    Raised {
        cost: eff(to.cost_minor) > eff(from.cost_minor),
        wall: eff(to.wall_ms) > eff(from.wall_ms),
        children: to.children > from.children,
    }
}

/// Who asks for the change.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Requester {
    /// A person holding the session.
    Person,
    /// An agent, the Core itself or an external system.
    Autonomous,
}

/// When a raise within the ceiling asks for approval.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RaiseApproval {
    /// Only an autonomous requester asks (the default).
    #[default]
    Autonomous,
    /// Every raise asks, whoever raises it.
    Always,
}

/// The organization's budget policy (the signed bundle's "maximum request
/// budget", docs/10, docs/24; the local Core reads `MODBIT_BUDGET_POLICY`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BudgetPolicy {
    /// Hard ceiling on a task's cost cap, minor units; `None` = no ceiling.
    pub ceiling_cost_minor: Option<u64>,
    /// Hard ceiling on a task's wall-clock cap, ms.
    pub ceiling_wall_ms: Option<u64>,
    /// Hard ceiling on live children.
    pub ceiling_children: Option<u32>,
    /// When a raise within the ceiling asks.
    pub raise_approval: RaiseApproval,
    /// Why the policy could not be read; every raise is refused while set.
    pub invalid: Option<String>,
}

/// What the policy decided about a change of caps.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BudgetDecision {
    /// Apply it; `rule` names why.
    Allow {
        /// The rule that allowed it.
        rule: String,
    },
    /// Apply it only under an exact-intent approval.
    ApprovalRequired {
        /// Why it asks.
        reason: String,
    },
    /// Never, under this policy.
    Deny {
        /// Typed refusal code.
        code: &'static str,
        /// Words for the person.
        reason: String,
    },
}

impl BudgetPolicy {
    /// Parse `ceiling_cost_minor=5000;ceiling_wall_ms=7200000;ceiling_children=2;raise_approval=always`.
    /// A clause the parser does not understand makes the whole policy invalid
    /// (fail closed): a typo must not silently lift a ceiling.
    #[must_use]
    pub fn parse(spec: &str) -> Self {
        let mut p = Self::default();
        for clause in spec.split(';').map(str::trim).filter(|c| !c.is_empty()) {
            let Some((k, v)) = clause.split_once('=') else {
                p.invalid = Some(format!("`{clause}` is not key=value"));
                return p;
            };
            let (k, v) = (k.trim(), v.trim());
            let bad = |what: &str| format!("`{k}={v}`: {what}");
            match k {
                "ceiling_cost_minor" => match v.parse::<u64>() {
                    Ok(n) if n > 0 => p.ceiling_cost_minor = Some(n),
                    _ => {
                        p.invalid = Some(bad("a ceiling is a positive whole number"));
                        return p;
                    }
                },
                "ceiling_wall_ms" => match v.parse::<u64>() {
                    Ok(n) if n > 0 => p.ceiling_wall_ms = Some(n),
                    _ => {
                        p.invalid = Some(bad("a ceiling is a positive whole number"));
                        return p;
                    }
                },
                "ceiling_children" => match v.parse::<u32>() {
                    Ok(n) => p.ceiling_children = Some(n),
                    Err(_) => {
                        p.invalid = Some(bad("a ceiling is a whole number"));
                        return p;
                    }
                },
                "raise_approval" => match v {
                    "autonomous" => p.raise_approval = RaiseApproval::Autonomous,
                    "always" => p.raise_approval = RaiseApproval::Always,
                    _ => {
                        p.invalid = Some(bad("`autonomous` or `always`"));
                        return p;
                    }
                },
                other => {
                    p.invalid = Some(format!("unknown key `{other}`"));
                    return p;
                }
            }
        }
        p
    }

    /// From `MODBIT_BUDGET_POLICY`; empty (no ceiling, a person's raise is
    /// allowed) when unset.
    #[must_use]
    pub fn from_env() -> Self {
        match std::env::var("MODBIT_BUDGET_POLICY") {
            Ok(s) => Self::parse(&s),
            Err(std::env::VarError::NotPresent) => Self::default(),
            Err(e) => Self {
                invalid: Some(e.to_string()),
                ..Self::default()
            },
        }
    }

    /// The caps a task actually runs under: its own, clamped to the
    /// organization's ceilings. A task that never set a cap still runs under
    /// the ceiling.
    #[must_use]
    pub fn clamp(&self, c: Caps) -> Caps {
        let cut = |v: u64, ceil: Option<u64>| match ceil {
            Some(m) => eff(v).min(m),
            None => v,
        };
        Caps {
            cost_minor: cut(c.cost_minor, self.ceiling_cost_minor),
            wall_ms: cut(c.wall_ms, self.ceiling_wall_ms),
            children: self
                .ceiling_children
                .map_or(c.children, |m| c.children.min(m)),
        }
    }

    /// Decide a change of caps from `from` (in force) to `to` (asked for).
    /// Anything that lowers or keeps every cap is allowed: a smaller budget
    /// never needs a decision.
    #[must_use]
    pub fn decide(&self, from: &Caps, to: &Caps, who: Requester) -> BudgetDecision {
        let r = raised(from, to);
        if !r.any() {
            return BudgetDecision::Allow {
                rule: "not-a-raise".into(),
            };
        }
        if let Some(why) = &self.invalid {
            return BudgetDecision::Deny {
                code: POLICY_INVALID,
                reason: format!(
                    "the budget policy could not be read ({why}); no raise is allowed until it can"
                ),
            };
        }
        if r.cost
            && let Some(m) = self.ceiling_cost_minor
            && eff(to.cost_minor) > m
        {
            return BudgetDecision::Deny {
                code: CEILING_EXCEEDED,
                reason: format!(
                    "the cost cap may not exceed {m} minor currency units by organization policy; {} was asked for",
                    show(to.cost_minor)
                ),
            };
        }
        if r.wall
            && let Some(m) = self.ceiling_wall_ms
            && eff(to.wall_ms) > m
        {
            return BudgetDecision::Deny {
                code: CEILING_EXCEEDED,
                reason: format!(
                    "the time cap may not exceed {m} ms by organization policy; {} was asked for",
                    show(to.wall_ms)
                ),
            };
        }
        if r.children
            && let Some(m) = self.ceiling_children
            && to.children > m
        {
            return BudgetDecision::Deny {
                code: CEILING_EXCEEDED,
                reason: format!(
                    "no more than {m} live children are allowed by organization policy; {} were asked for",
                    to.children
                ),
            };
        }
        match (who, self.raise_approval) {
            (Requester::Autonomous, _) => BudgetDecision::ApprovalRequired {
                reason: "a run never raises its own budget; a person approves this exact raise"
                    .into(),
            },
            (Requester::Person, RaiseApproval::Always) => BudgetDecision::ApprovalRequired {
                reason: "organization policy asks for an approval of every budget raise".into(),
            },
            (Requester::Person, RaiseApproval::Autonomous) => BudgetDecision::Allow {
                rule: "within-ceiling".into(),
            },
        }
    }
}

fn show(v: u64) -> String {
    if v == 0 {
        "no cap".into()
    } else {
        v.to_string()
    }
}

/// The intent an approval of a raise binds: the task, the caps in force, the
/// caps asked for, and how many cap decisions the task's log already holds.
/// The last term makes the approval single-use: applying the raise moves the
/// log on, and the old hash names a raise that can no longer be asked.
#[must_use]
pub fn intent_hash(task_hex: &str, from: &Caps, to: &Caps, decisions_on_log: u64) -> String {
    let mut h = Sha256::new();
    h.update(
        format!(
            "{RAISE_EFFECT}\0{task_hex}\0{}/{}/{}\0{}/{}/{}\0{decisions_on_log}",
            from.cost_minor, from.wall_ms, from.children, to.cost_minor, to.wall_ms, to.children
        )
        .as_bytes(),
    );
    hex::encode(h.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;

    const D: u32 = 3;
    fn caps(c: u64, w: u64, k: u32) -> Caps {
        Caps {
            cost_minor: c,
            wall_ms: w,
            children: k,
        }
    }

    #[test]
    fn lowering_and_equal_are_never_decisions() {
        let p = BudgetPolicy::parse("ceiling_cost_minor=10;raise_approval=always");
        assert!(matches!(
            p.decide(&caps(0, 0, D), &caps(5, 100, 1), Requester::Autonomous),
            BudgetDecision::Allow { .. }
        ));
        assert!(matches!(
            p.decide(&caps(5, 100, 1), &caps(5, 100, 1), Requester::Autonomous),
            BudgetDecision::Allow { .. }
        ));
    }

    #[test]
    fn removing_a_cap_is_a_raise() {
        let r = raised(&caps(5, 100, 1), &caps(0, 100, 1));
        assert!(r.cost && !r.wall && !r.children);
    }

    #[test]
    fn a_person_within_the_ceiling_is_allowed_and_a_run_asks() {
        let p = BudgetPolicy::parse("ceiling_cost_minor=1000");
        let (from, to) = (caps(100, 0, D), caps(900, 0, D));
        assert!(matches!(
            p.decide(&from, &to, Requester::Person),
            BudgetDecision::Allow { .. }
        ));
        assert!(matches!(
            p.decide(&from, &to, Requester::Autonomous),
            BudgetDecision::ApprovalRequired { .. }
        ));
    }

    #[test]
    fn above_the_ceiling_is_refused_for_everyone_and_no_cap_is_above_any_ceiling() {
        let p =
            BudgetPolicy::parse("ceiling_cost_minor=1000;ceiling_wall_ms=60000;ceiling_children=2");
        for who in [Requester::Person, Requester::Autonomous] {
            for to in [caps(1001, 0, D), caps(0, 0, D)] {
                assert!(
                    matches!(
                        p.decide(&caps(100, 1000, 1), &to, who),
                        BudgetDecision::Deny {
                            code: CEILING_EXCEEDED,
                            ..
                        }
                    ),
                    "{to:?}"
                );
            }
            assert!(matches!(
                p.decide(&caps(100, 1000, 1), &caps(100, 60_001, 1), who),
                BudgetDecision::Deny {
                    code: CEILING_EXCEEDED,
                    ..
                }
            ));
            assert!(matches!(
                p.decide(&caps(100, 1000, 1), &caps(100, 1000, 3), who),
                BudgetDecision::Deny {
                    code: CEILING_EXCEEDED,
                    ..
                }
            ));
        }
    }

    #[test]
    fn a_ceiling_tightened_after_the_fact_does_not_trap_a_lowering() {
        let p = BudgetPolicy::parse("ceiling_cost_minor=100");
        assert!(matches!(
            p.decide(&caps(5000, 0, D), &caps(4000, 0, D), Requester::Person),
            BudgetDecision::Allow { .. }
        ));
    }

    #[test]
    fn always_asks_a_person_too() {
        let p = BudgetPolicy::parse("raise_approval=always");
        assert!(matches!(
            p.decide(&caps(5, 0, D), &caps(6, 0, D), Requester::Person),
            BudgetDecision::ApprovalRequired { .. }
        ));
    }

    #[test]
    fn an_unreadable_policy_refuses_every_raise() {
        for spec in [
            "ceiling_cost_minor=lots",
            "ceiling_cost=5",
            "nonsense",
            "raise_approval=sometimes",
            "ceiling_wall_ms=0",
        ] {
            let p = BudgetPolicy::parse(spec);
            assert!(p.invalid.is_some(), "{spec}");
            for who in [Requester::Person, Requester::Autonomous] {
                assert!(matches!(
                    p.decide(&caps(5, 0, D), &caps(6, 0, D), who),
                    BudgetDecision::Deny {
                        code: POLICY_INVALID,
                        ..
                    }
                ));
            }
            // A lowering is still fine.
            assert!(matches!(
                p.decide(&caps(5, 0, D), &caps(4, 0, D), Requester::Person),
                BudgetDecision::Allow { .. }
            ));
        }
    }

    #[test]
    fn clamp_applies_the_ceiling_to_a_task_with_no_caps() {
        let p = BudgetPolicy::parse("ceiling_cost_minor=50;ceiling_children=1");
        assert_eq!(p.clamp(Caps::unset(D)), caps(50, 0, 1));
        assert_eq!(p.clamp(caps(10, 7, 0)), caps(10, 7, 0));
    }

    #[test]
    fn the_hash_names_the_exact_raise_and_the_moment() {
        let a = intent_hash("t", &caps(1, 2, 3), &caps(4, 5, 6), 1);
        assert_eq!(a, intent_hash("t", &caps(1, 2, 3), &caps(4, 5, 6), 1));
        assert_ne!(a, intent_hash("t", &caps(1, 2, 3), &caps(4, 6, 6), 1));
        assert_ne!(a, intent_hash("t", &caps(1, 2, 3), &caps(4, 5, 6), 2));
        assert_ne!(a, intent_hash("u", &caps(1, 2, 3), &caps(4, 5, 6), 1));
    }
}
