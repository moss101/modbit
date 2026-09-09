//! Turn aggregate (docs/13 "Turn", docs/31 `turns`): one model interaction
//! cycle. `Prepared → Streaming → Executing → Verifying → Completed |
//! Interrupted | Failed`; a tool failure may send `Executing` back to
//! `Streaming` for repair without failing the turn.

use serde::{Deserialize, Serialize};

use crate::ids::{RunId, TurnId};
use crate::state::StateMachine;
use crate::time::Timestamp;

/// Turn lifecycle.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum TurnState {
    /// Context compiled, model not yet invoked.
    Prepared,
    /// Model output streaming.
    Streaming,
    /// Executing requested tool/procedure activity.
    Executing,
    /// Verifying results.
    Verifying,
    /// Finished.
    Completed,
    /// Interrupted by steering/cancellation.
    Interrupted,
    /// Failed.
    Failed,
}

impl StateMachine for TurnState {
    const AGGREGATE: &'static str = "Turn";

    fn can_transition(self, to: Self) -> bool {
        use TurnState::*;
        matches!(
            (self, to),
            (Prepared, Streaming)
                | (Streaming, Executing)
                | (Streaming, Verifying)
                | (Executing, Streaming)
                | (Executing, Verifying)
                | (Verifying, Completed)
                | (Verifying, Streaming)
                | (
                    Prepared | Streaming | Executing | Verifying,
                    Interrupted | Failed
                )
        )
    }

    fn is_terminal(self) -> bool {
        matches!(
            self,
            TurnState::Completed | TurnState::Interrupted | TurnState::Failed
        )
    }
}

/// Turn projection state.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Turn {
    /// Identity.
    pub turn_id: TurnId,
    /// Parent run.
    pub run_id: RunId,
    /// Ordinal within the run (1-based).
    pub ordinal: u32,
    /// Lifecycle state.
    pub state: TurnState,
    /// Model route description (opaque JSON, owned by the gateway).
    pub model_route: Option<serde_json::Value>,
    /// Hash of the projected tool set.
    pub tool_projection_hash: Option<String>,
    /// Context pack id.
    pub context_pack_id: Option<String>,
    /// Aggregate generation.
    pub generation: u64,
    /// Start time.
    pub started_at: Timestamp,
    /// End time.
    pub ended_at: Option<Timestamp>,
}

/// Turn events (docs/30 "Turn/model").
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "event_type")]
pub enum TurnEvent {
    /// `TurnPrepared`.
    TurnPrepared {
        /// Parent run.
        run_id: RunId,
        /// Ordinal.
        ordinal: u32,
    },
    /// `ContextPackCompiled`.
    ContextPackCompiled {
        /// Context pack id.
        context_pack_id: String,
    },
    /// `ToolProjectionSelected`.
    ToolProjectionSelected {
        /// Projection hash.
        tool_projection_hash: String,
    },
    /// `ModelInvocationStarted` (Prepared → Streaming, or Executing/Verifying → Streaming for repair).
    ModelInvocationStarted {
        /// Route.
        model_route: serde_json::Value,
    },
    /// `ModelUsageRecorded`: provider usage for the invocation; no state change.
    ModelUsageRecorded {
        /// Input tokens.
        input_tokens: u64,
        /// Output tokens.
        output_tokens: u64,
        /// Cached input tokens.
        cached_input_tokens: u64,
        /// Route record JSON (requested vs resolved, REQ-EV-0112).
        route: serde_json::Value,
    },
    /// `ModelInvocationCompleted` (Streaming → Executing when actions were requested, else Verifying).
    ModelInvocationCompleted {
        /// Whether the model requested actions.
        requested_actions: bool,
    },
    /// `TurnVerifying` (Executing → Verifying).
    TurnVerifying,
    /// `TurnCompleted`.
    TurnCompleted,
    /// `TurnInterrupted`.
    TurnInterrupted,
    /// `TurnFailed`.
    TurnFailed {
        /// Failure code.
        failure_code: String,
    },
}

impl TurnEvent {
    /// Canonical event type name.
    #[must_use]
    pub const fn event_type(&self) -> &'static str {
        match self {
            Self::TurnPrepared { .. } => "TurnPrepared",
            Self::ContextPackCompiled { .. } => "ContextPackCompiled",
            Self::ToolProjectionSelected { .. } => "ToolProjectionSelected",
            Self::ModelInvocationStarted { .. } => "ModelInvocationStarted",
            Self::ModelUsageRecorded { .. } => "ModelUsageRecorded",
            Self::ModelInvocationCompleted { .. } => "ModelInvocationCompleted",
            Self::TurnVerifying => "TurnVerifying",
            Self::TurnCompleted => "TurnCompleted",
            Self::TurnInterrupted => "TurnInterrupted",
            Self::TurnFailed { .. } => "TurnFailed",
        }
    }
}

fn invalid(from: TurnState, to: &str) -> crate::InvalidTransition {
    crate::InvalidTransition {
        aggregate: "Turn",
        from: format!("{from:?}"),
        to: to.into(),
    }
}

impl Turn {
    /// Build from `TurnPrepared`.
    pub fn create(
        turn_id: TurnId,
        event: &TurnEvent,
        at: Timestamp,
    ) -> Result<Self, crate::InvalidTransition> {
        match event {
            TurnEvent::TurnPrepared { run_id, ordinal } => Ok(Self {
                turn_id,
                run_id: *run_id,
                ordinal: *ordinal,
                state: TurnState::Prepared,
                model_route: None,
                tool_projection_hash: None,
                context_pack_id: None,
                generation: 1,
                started_at: at,
                ended_at: None,
            }),
            other => Err(crate::InvalidTransition {
                aggregate: "Turn",
                from: "<none>".into(),
                to: other.event_type().into(),
            }),
        }
    }

    /// Apply a subsequent event.
    pub fn apply(
        &mut self,
        event: &TurnEvent,
        at: Timestamp,
    ) -> Result<(), crate::InvalidTransition> {
        use TurnState::*;
        let next = match event {
            TurnEvent::TurnPrepared { .. } => return Err(invalid(self.state, "TurnPrepared")),
            TurnEvent::ContextPackCompiled { context_pack_id } => {
                if self.state != Prepared {
                    return Err(invalid(self.state, "ContextPackCompiled"));
                }
                self.context_pack_id = Some(context_pack_id.clone());
                None
            }
            TurnEvent::ToolProjectionSelected {
                tool_projection_hash,
            } => {
                if self.state != Prepared {
                    return Err(invalid(self.state, "ToolProjectionSelected"));
                }
                self.tool_projection_hash = Some(tool_projection_hash.clone());
                None
            }
            TurnEvent::ModelInvocationStarted { model_route } => {
                self.state.transition(Streaming)?;
                self.model_route = Some(model_route.clone());
                Some(Streaming)
            }
            TurnEvent::ModelUsageRecorded { .. } => {
                if self.state != Streaming {
                    return Err(invalid(self.state, "ModelUsageRecorded"));
                }
                None
            }
            TurnEvent::ModelInvocationCompleted { requested_actions } => {
                Some(if *requested_actions {
                    Executing
                } else {
                    Verifying
                })
            }
            TurnEvent::TurnVerifying => Some(Verifying),
            TurnEvent::TurnCompleted => Some(Completed),
            TurnEvent::TurnInterrupted => Some(Interrupted),
            TurnEvent::TurnFailed { .. } => Some(Failed),
        };
        if let Some(to) = next {
            self.state = self.state.transition(to)?;
            if to.is_terminal() {
                self.ended_at = Some(at);
            }
        }
        self.generation += 1;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repair_loop_returns_to_streaming_without_failing_the_turn() {
        let mut t = Turn::create(
            TurnId::new(),
            &TurnEvent::TurnPrepared {
                run_id: RunId::new(),
                ordinal: 1,
            },
            Timestamp(1),
        )
        .unwrap();
        t.apply(
            &TurnEvent::ContextPackCompiled {
                context_pack_id: "cp1".into(),
            },
            Timestamp(2),
        )
        .unwrap();
        t.apply(
            &TurnEvent::ModelInvocationStarted {
                model_route: serde_json::json!({"model":"x"}),
            },
            Timestamp(3),
        )
        .unwrap();
        t.apply(
            &TurnEvent::ModelInvocationCompleted {
                requested_actions: true,
            },
            Timestamp(4),
        )
        .unwrap();
        assert_eq!(t.state, TurnState::Executing);
        t.apply(
            &TurnEvent::ModelInvocationStarted {
                model_route: serde_json::json!({"model":"x"}),
            },
            Timestamp(5),
        )
        .unwrap();
        assert_eq!(t.state, TurnState::Streaming);
        t.apply(
            &TurnEvent::ModelInvocationCompleted {
                requested_actions: false,
            },
            Timestamp(6),
        )
        .unwrap();
        t.apply(&TurnEvent::TurnCompleted, Timestamp(7)).unwrap();
        assert!(
            t.apply(
                &TurnEvent::ContextPackCompiled {
                    context_pack_id: "late".into()
                },
                Timestamp(8)
            )
            .is_err()
        );
        assert_eq!(t.context_pack_id.as_deref(), Some("cp1"));
    }
}
