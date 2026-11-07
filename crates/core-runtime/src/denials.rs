//! What the model is told when an effect is refused, and the watch that stops
//! a run that keeps being refused (PX-153, docs/62).
//!
//! A refusal is a person's or a policy's decision about one effect. A model
//! that meets one and then reaches the same effect by another route (a
//! different command, a wrapper, a path around a hook) is working against the
//! decision, and a model that is refused again and again is not learning from
//! it. So every refusal reaches the model with the same plain instruction, and
//! the Core counts the refusals: three in a row, or ten of the last fifty tool
//! results, and the task is parked as needing attention with a typed reason.
//! The park is a stop, not an end: the task resumes with `StartTask` after
//! the person has looked.
//!
//! The watch is a fold over the task's tool results in log order ([`DenialWatch::observe`]),
//! so a Core that restarts rebuilds exactly the count the dead one held.

use std::collections::VecDeque;

use serde::{Deserialize, Serialize};

/// Refusals in a row that park the task.
pub const STREAK_LIMIT: u32 = 3;
/// How many of the latest tool results the window looks at.
pub const WINDOW: usize = 50;
/// Refusals within the window that park the task.
pub const WINDOW_LIMIT: usize = 10;

/// The typed reason the task is parked with.
pub const DENIAL_STREAK: &str = "DENIAL_STREAK";

/// The event types [`DenialWatch::observe_event`] reads.
pub const FOLDED_EVENTS: [&str; 4] = [
    "ToolCallPolicyDecision",
    "ToolCallFailed",
    "ToolCallSucceeded",
    "TaskNeedsAttention",
];

/// What every refusal tells the model: do not go round it.
const NO_DETOUR: &str = "Do not reach the same effect by another route (a different command, tool, path or wrapper). Continue only with a materially safer alternative that needs no further permission; otherwise stop and ask the user what to do.";

/// The guidance a refusal carries, by the typed code of the refusal.
/// `APPROVAL_DENIED`, `APPROVAL_EXPIRED` and `HOOK_DENIED` have their own;
/// every other policy refusal gets the policy one.
#[must_use]
pub fn guidance(code: &str) -> String {
    match code {
        "APPROVAL_DENIED" => format!("The user refused this effect. {NO_DETOUR}"),
        "APPROVAL_EXPIRED" => format!(
            "No one answered in time. That is not a refusal and says nothing about whether the effect is safe or unsafe. Ask again only if the effect is still needed. {NO_DETOUR}"
        ),
        "HOOK_DENIED" => format!("A configured hook refused this call. {NO_DETOUR}"),
        _ => format!("Policy refused this call. {NO_DETOUR}"),
    }
}

/// What a timeout tells the model: it carries no verdict about the command.
pub const TIMEOUT_GUIDANCE: &str = "A timeout is not evidence that the command or its target is dangerous; it means only that no result arrived in time. Look at the target before repeating it, and prefer a smaller step.";

/// Why the watch tripped.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Trip {
    /// [`STREAK_LIMIT`] refusals in a row.
    Streak(u32),
    /// [`WINDOW_LIMIT`] refusals among the last [`WINDOW`] results.
    Window {
        /// Refusals in the window.
        denied: usize,
        /// Results in the window.
        of: usize,
    },
}

impl Trip {
    /// The sentence the notice carries.
    #[must_use]
    pub fn reason(&self) -> String {
        match self {
            Self::Streak(n) => format!(
                "{n} tool calls in a row were refused; the task is parked so you can look at what the agent is attempting. Resume with StartTask, after steering it or changing the run mode or rules"
            ),
            Self::Window { denied, of } => format!(
                "{denied} of the last {of} tool results were refusals; the task is parked so you can look at what the agent is attempting. Resume with StartTask, after steering it or changing the run mode or rules"
            ),
        }
    }
}

/// The count of refusals, folded from tool results in log order. Not part of
/// the state the model reads.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DenialWatch {
    consecutive: u32,
    window: VecDeque<bool>,
}

impl DenialWatch {
    /// One tool result: refused, or not (a success, a failure of the command
    /// itself, an invalid call: anything that is not a refusal).
    pub fn observe(&mut self, denied: bool) {
        self.consecutive = if denied { self.consecutive + 1 } else { 0 };
        self.window.push_back(denied);
        while self.window.len() > WINDOW {
            self.window.pop_front();
        }
    }

    /// Whether the watch says to park the task.
    #[must_use]
    pub fn trip(&self) -> Option<Trip> {
        if self.consecutive >= STREAK_LIMIT {
            return Some(Trip::Streak(self.consecutive));
        }
        let denied = self.window.iter().filter(|d| **d).count();
        (denied >= WINDOW_LIMIT).then_some(Trip::Window {
            denied,
            of: self.window.len(),
        })
    }

    /// Fold one event of the task's log. The refusals are the policy decisions
    /// that deny without asking, and the approvals that were refused or
    /// expired; the results that are not refusals are the calls that succeeded
    /// and the calls that failed for their own reasons. A decision that asks,
    /// a cancelled call and a call of unknown outcome are not results. The
    /// notice that parks the task starts the count again.
    pub fn observe_event(&mut self, event_type: &str, payload: &serde_json::Value) {
        match event_type {
            "ToolCallPolicyDecision"
                if payload["allowed"].as_bool() == Some(false)
                    && payload["approval_required"].as_bool() != Some(true) =>
            {
                self.observe(true);
            }
            "ToolCallFailed" => self.observe(matches!(
                payload["failure_code"].as_str(),
                Some("APPROVAL_DENIED" | "APPROVAL_EXPIRED")
            )),
            "ToolCallSucceeded" => self.observe(false),
            "TaskNeedsAttention"
                if payload["diagnostic"]["code"].as_str() == Some(DENIAL_STREAK) =>
            {
                self.reset();
            }
            _ => {}
        }
    }

    /// The park happened: the count starts again, so a resume is not parked
    /// again by what it was parked for.
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// Refusals in a row now.
    #[must_use]
    pub fn consecutive(&self) -> u32 {
        self.consecutive
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn three_in_a_row_trip_and_an_allowed_effect_in_between_resets() {
        let mut w = DenialWatch::default();
        for _ in 0..2 {
            w.observe(true);
        }
        assert_eq!(w.trip(), None);
        w.observe(false);
        assert_eq!(w.consecutive(), 0);
        for _ in 0..2 {
            w.observe(true);
        }
        assert_eq!(w.trip(), None, "the allowed effect reset the streak");
        w.observe(true);
        assert_eq!(w.trip(), Some(Trip::Streak(3)));
    }

    #[test]
    fn a_model_that_changes_approach_after_the_first_refusal_is_never_parked() {
        let mut w = DenialWatch::default();
        w.observe(true);
        for _ in 0..200 {
            w.observe(false);
            assert_eq!(w.trip(), None);
        }
    }

    #[test]
    fn ten_of_the_last_fifty_trip_though_none_are_consecutive() {
        let mut w = DenialWatch::default();
        for i in 0..49 {
            w.observe(i % 5 == 4);
        }
        // 9 refusals so far (every fifth of 49), none adjacent.
        assert_eq!(w.trip(), None);
        w.observe(true);
        assert_eq!(w.trip(), Some(Trip::Window { denied: 10, of: 50 }));
    }

    #[test]
    fn old_refusals_leave_the_window() {
        let mut w = DenialWatch::default();
        for _ in 0..9 {
            w.observe(true);
            w.observe(false);
        }
        assert_eq!(w.trip(), None);
        for _ in 0..60 {
            w.observe(false);
        }
        w.observe(true);
        assert_eq!(w.trip(), None, "the first nine are past the window");
    }

    #[test]
    fn the_log_fold_counts_refusals_once_and_ignores_what_is_not_a_result() {
        use serde_json::json;
        let mut w = DenialWatch::default();
        // A decision that asks is not a result; the refusal that follows it is.
        w.observe_event(
            "ToolCallPolicyDecision",
            &json!({"allowed": false, "approval_required": true}),
        );
        assert_eq!(w.consecutive(), 0);
        w.observe_event(
            "ToolCallFailed",
            &json!({"failure_code": "APPROVAL_DENIED"}),
        );
        w.observe_event(
            "ToolCallFailed",
            &json!({"failure_code": "APPROVAL_EXPIRED"}),
        );
        // A deny decision without an approval is a refusal on its own.
        w.observe_event(
            "ToolCallPolicyDecision",
            &json!({"allowed": false, "approval_required": false}),
        );
        assert_eq!(w.trip(), Some(Trip::Streak(3)));
        // An allowed decision is not a result either; the success is.
        w.observe_event(
            "ToolCallPolicyDecision",
            &json!({"allowed": true, "approval_required": false}),
        );
        assert_eq!(w.consecutive(), 3);
        w.observe_event("ToolCallSucceeded", &json!({"result_ref": "x"}));
        assert_eq!(w.consecutive(), 0);
        // A command that failed for its own reasons is a result, not a refusal.
        for _ in 0..2 {
            w.observe_event("ToolCallFailed", &json!({"failure_code": "EXIT_NONZERO"}));
        }
        assert_eq!(w.trip(), None);
        // The notice that parks the task starts the count again.
        for _ in 0..3 {
            w.observe(true);
        }
        w.observe_event(
            "TaskNeedsAttention",
            &json!({"reason": "r", "diagnostic": {"code": "DENIAL_STREAK"}}),
        );
        assert_eq!(w.trip(), None);
        // Another attention notice leaves the count alone.
        w.observe(true);
        w.observe_event(
            "TaskNeedsAttention",
            &json!({"diagnostic": {"code": "NO_PROGRESS"}}),
        );
        assert_eq!(w.consecutive(), 1);
    }

    #[test]
    fn a_park_starts_the_count_again() {
        let mut w = DenialWatch::default();
        for _ in 0..3 {
            w.observe(true);
        }
        assert!(w.trip().is_some());
        w.reset();
        assert_eq!(w.trip(), None);
        w.observe(true);
        assert_eq!(w.trip(), None);
    }

    #[test]
    fn every_refusal_kind_carries_the_no_detour_instruction() {
        for code in [
            "APPROVAL_DENIED",
            "APPROVAL_EXPIRED",
            "HOOK_DENIED",
            "POLICY_DENIED",
            "PROFILE_NOT_ALLOWED",
        ] {
            let g = guidance(code);
            assert!(g.contains("another route"), "{code}: {g}");
            assert!(g.contains("materially safer alternative"), "{code}: {g}");
            assert!(g.contains("stop and ask"), "{code}: {g}");
        }
        assert!(guidance("APPROVAL_EXPIRED").contains("not a refusal"));
        assert!(guidance("APPROVAL_DENIED").contains("refused this effect"));
        assert!(guidance("HOOK_DENIED").contains("hook"));
        assert!(TIMEOUT_GUIDANCE.contains("not evidence"));
    }
}
