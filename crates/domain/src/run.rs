//! Run aggregate (docs/13 "Run", docs/31 `runs`): one concrete execution
//! attempt or continuation of a task, fenced by the session kernel lease
//! generation.
//!
//! `Pending → Running ↔ Suspended → Completed | Failed | Cancelled`.

use serde::{Deserialize, Serialize};

use crate::ids::{RunId, TaskId};
use crate::state::StateMachine;
use crate::time::Timestamp;

/// Where the run executes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OwnerLocation {
    /// Local Core.
    Local,
    /// Cloud worker.
    Cloud,
}

/// Run lifecycle.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RunState {
    /// Created, not executing.
    Pending,
    /// Executing under a kernel lease.
    Running,
    /// Detached (handoff, restart) and resumable.
    Suspended,
    /// Finished successfully.
    Completed,
    /// Finished with failure.
    Failed,
    /// Cancelled.
    Cancelled,
}

impl StateMachine for RunState {
    const AGGREGATE: &'static str = "Run";

    fn can_transition(self, to: Self) -> bool {
        use RunState::*;
        matches!(
            (self, to),
            (Pending, Running)
                | (Running, Suspended)
                | (Suspended, Running)
                | (Running, Completed)
                | (Pending | Running | Suspended, Failed | Cancelled)
        )
    }

    fn is_terminal(self) -> bool {
        matches!(
            self,
            RunState::Completed | RunState::Failed | RunState::Cancelled
        )
    }
}

/// Run projection state.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Run {
    /// Identity.
    pub run_id: RunId,
    /// Parent task.
    pub task_id: TaskId,
    /// Attempt ordinal within the task (1-based).
    pub attempt: u32,
    /// Where it executes.
    pub owner_location: OwnerLocation,
    /// Kernel lease generation that owns the run; stale generations are rejected.
    pub kernel_lease_generation: u64,
    /// Lifecycle state.
    pub state: RunState,
    /// Aggregate generation.
    pub generation: u64,
    /// Start time.
    pub started_at: Option<Timestamp>,
    /// End time.
    pub ended_at: Option<Timestamp>,
}

/// Run events.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "event_type")]
pub enum RunEvent {
    /// `RunCreated`.
    RunCreated {
        /// Parent task.
        task_id: TaskId,
        /// Attempt ordinal.
        attempt: u32,
        /// Location.
        owner_location: OwnerLocation,
        /// Owning lease generation.
        kernel_lease_generation: u64,
    },
    /// `RunStarted`.
    RunStarted,
    /// `RunSuspended`.
    RunSuspended,
    /// `RunResumed` with the lease generation that now owns it; must not go backwards.
    RunResumed {
        /// New owning lease generation.
        kernel_lease_generation: u64,
    },
    /// `RunCompleted`.
    RunCompleted,
    /// `RunFailed`.
    RunFailed {
        /// Failure code.
        failure_code: String,
    },
    /// `RunCancelled`.
    RunCancelled,
}

impl RunEvent {
    /// Canonical event type name.
    #[must_use]
    pub const fn event_type(&self) -> &'static str {
        match self {
            Self::RunCreated { .. } => "RunCreated",
            Self::RunStarted => "RunStarted",
            Self::RunSuspended => "RunSuspended",
            Self::RunResumed { .. } => "RunResumed",
            Self::RunCompleted => "RunCompleted",
            Self::RunFailed { .. } => "RunFailed",
            Self::RunCancelled => "RunCancelled",
        }
    }
}

impl Run {
    /// Build from `RunCreated`.
    pub fn create(
        run_id: RunId,
        event: &RunEvent,
        _at: Timestamp,
    ) -> Result<Self, crate::InvalidTransition> {
        match event {
            RunEvent::RunCreated {
                task_id,
                attempt,
                owner_location,
                kernel_lease_generation,
            } => Ok(Self {
                run_id,
                task_id: *task_id,
                attempt: *attempt,
                owner_location: *owner_location,
                kernel_lease_generation: *kernel_lease_generation,
                state: RunState::Pending,
                generation: 1,
                started_at: None,
                ended_at: None,
            }),
            other => Err(crate::InvalidTransition {
                aggregate: "Run",
                from: "<none>".into(),
                to: other.event_type().into(),
            }),
        }
    }

    /// Apply a subsequent event.
    pub fn apply(
        &mut self,
        event: &RunEvent,
        at: Timestamp,
    ) -> Result<(), crate::InvalidTransition> {
        use RunState::*;
        let to = match event {
            RunEvent::RunCreated { .. } => {
                return Err(crate::InvalidTransition {
                    aggregate: "Run",
                    from: format!("{:?}", self.state),
                    to: "RunCreated".into(),
                });
            }
            RunEvent::RunStarted => Running,
            RunEvent::RunSuspended => Suspended,
            RunEvent::RunResumed {
                kernel_lease_generation,
            } => {
                // Fencing (docs/13): a resume under an older lease generation is stale.
                if *kernel_lease_generation < self.kernel_lease_generation {
                    return Err(crate::InvalidTransition {
                        aggregate: "Run",
                        from: format!("lease generation {}", self.kernel_lease_generation),
                        to: format!("stale lease generation {kernel_lease_generation}"),
                    });
                }
                self.state.transition(Running)?;
                self.kernel_lease_generation = *kernel_lease_generation;
                Running
            }
            RunEvent::RunCompleted => Completed,
            RunEvent::RunFailed { .. } => Failed,
            RunEvent::RunCancelled => Cancelled,
        };
        self.state = self.state.transition(to)?;
        if to == Running && self.started_at.is_none() {
            self.started_at = Some(at);
        }
        if to.is_terminal() {
            self.ended_at = Some(at);
        }
        self.generation += 1;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stale_lease_generation_cannot_resume_a_run() {
        let mut r = Run::create(
            RunId::new(),
            &RunEvent::RunCreated {
                task_id: TaskId::new(),
                attempt: 1,
                owner_location: OwnerLocation::Local,
                kernel_lease_generation: 5,
            },
            Timestamp(1),
        )
        .unwrap();
        r.apply(&RunEvent::RunStarted, Timestamp(2)).unwrap();
        r.apply(&RunEvent::RunSuspended, Timestamp(3)).unwrap();
        let err = r
            .apply(
                &RunEvent::RunResumed {
                    kernel_lease_generation: 4,
                },
                Timestamp(4),
            )
            .unwrap_err();
        assert!(err.to_string().contains("stale lease generation 4"));
        assert_eq!(r.state, RunState::Suspended);
        r.apply(
            &RunEvent::RunResumed {
                kernel_lease_generation: 6,
            },
            Timestamp(5),
        )
        .unwrap();
        assert_eq!(r.kernel_lease_generation, 6);
        r.apply(&RunEvent::RunCompleted, Timestamp(6)).unwrap();
        assert!(r.apply(&RunEvent::RunStarted, Timestamp(7)).is_err());
    }
}
