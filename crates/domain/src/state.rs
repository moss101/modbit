//! Shared state-machine machinery. Every aggregate state enum implements
//! [`StateMachine`] with an explicit transition table copied from docs/13;
//! anything outside the table is an [`InvalidTransition`], never silently applied.

use std::fmt;

/// A transition the state machine forbids.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("invalid {aggregate} transition {from} -> {to}")]
pub struct InvalidTransition {
    /// Aggregate kind.
    pub aggregate: &'static str,
    /// Current state name.
    pub from: String,
    /// Requested state name.
    pub to: String,
}

/// Explicit transition table.
pub trait StateMachine: Copy + PartialEq + fmt::Debug {
    /// Aggregate name for diagnostics.
    const AGGREGATE: &'static str;

    /// Whether `self -> to` is in the table.
    fn can_transition(self, to: Self) -> bool;

    /// Terminal states admit no further transition.
    fn is_terminal(self) -> bool;

    /// Check a transition, returning the target on success.
    fn transition(self, to: Self) -> Result<Self, InvalidTransition> {
        if self.can_transition(to) {
            Ok(to)
        } else {
            Err(InvalidTransition {
                aggregate: Self::AGGREGATE,
                from: format!("{self:?}"),
                to: format!("{to:?}"),
            })
        }
    }
}
