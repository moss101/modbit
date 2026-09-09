//! Task aggregate and state machine (docs/13 "Task", docs/31 `tasks`).
//!
//! ```text
//! Created → Queued → Running ↔ Waiting(reason)
//!                     ├→ ReadyForReview → Completed
//!                     ├→ Failed
//!                     └→ Cancelled
//! ```

use serde::{Deserialize, Serialize};

use crate::ids::{SessionId, TaskId, WorkspaceId};
use crate::state::StateMachine;
use crate::time::Timestamp;

/// Why a running task is waiting.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum WaitReason {
    /// Needs a user answer.
    UserInput,
    /// Needs a protected-effect approval.
    Approval,
    /// Waiting for a capacity ticket.
    Capacity,
    /// Waiting on an external system.
    External,
    /// Waiting on a model provider.
    Provider,
}

/// Where a task came from (docs/30 `origin` on `CreateTask`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskOrigin {
    /// Desktop app.
    Desktop,
    /// Headless CLI.
    Cli,
    /// IDE adapter.
    IdeAdapter,
    /// Forge issue.
    ForgeIssue,
    /// Forge webhook.
    ForgeWebhook,
}

/// Typed input dispatch mode (MOD-INPUT-001, docs/14): concurrency semantics
/// live in Core. `Steer` interrupts and replaces at the next safe boundary,
/// `Collect` coalesces after the current step, `FollowUp` is an ordered
/// separate turn.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum InputMode {
    /// Interrupt and replace.
    Steer,
    /// Coalesce after the current step.
    Collect,
    /// Ordered separate turn.
    FollowUp,
}

/// Task lifecycle.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum TaskState {
    /// Accepted, not scheduled.
    Created,
    /// Scheduled, no run started.
    Queued,
    /// A run is executing.
    Running,
    /// A run is blocked on the given reason.
    Waiting(WaitReason),
    /// Candidate ready for human review.
    ReadyForReview,
    /// Accepted.
    Completed,
    /// Failed with a failure code.
    Failed,
    /// Cancelled by user or policy.
    Cancelled,
}

impl StateMachine for TaskState {
    const AGGREGATE: &'static str = "Task";

    fn can_transition(self, to: Self) -> bool {
        use TaskState::*;
        match (self, to) {
            (Created, Queued) | (Queued, Running) => true,
            (Running, Waiting(_)) | (Waiting(_), Running) => true,
            (Running, ReadyForReview) | (ReadyForReview, Completed) => true,
            // Review can send the task back for more work.
            (ReadyForReview, Running) => true,
            (Created | Queued | Running | Waiting(_) | ReadyForReview, Failed | Cancelled) => true,
            _ => false,
        }
    }

    fn is_terminal(self) -> bool {
        matches!(
            self,
            TaskState::Completed | TaskState::Failed | TaskState::Cancelled
        )
    }
}

/// Task projection state.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Task {
    /// Identity.
    pub task_id: TaskId,
    /// Owning session.
    pub session_id: SessionId,
    /// User goal.
    pub goal_text: String,
    /// Workspace the task operates in.
    pub workspace_id: WorkspaceId,
    /// Workspace revision the task started from.
    pub base_revision: Option<String>,
    /// Execution profile name (e.g. `local_trusted`).
    pub execution_profile: String,
    /// Policy profile id.
    pub policy_profile_id: Option<String>,
    /// Origin surface.
    pub origin: TaskOrigin,
    /// Lifecycle state.
    pub state: TaskState,
    /// Aggregate generation.
    pub generation: u64,
    /// Creation time.
    pub created_at: Timestamp,
    /// First run start.
    pub started_at: Option<Timestamp>,
    /// Terminal time.
    pub completed_at: Option<Timestamp>,
    /// Failure code when failed.
    pub failure_code: Option<String>,
}

/// Task events (docs/30 "Session/task").
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "event_type")]
pub enum TaskEvent {
    /// `TaskCreated`.
    TaskCreated {
        /// Owning session.
        session_id: SessionId,
        /// Goal.
        goal_text: String,
        /// Workspace.
        workspace_id: WorkspaceId,
        /// Base revision.
        base_revision: Option<String>,
        /// Execution profile.
        execution_profile: String,
        /// Policy profile.
        policy_profile_id: Option<String>,
        /// Origin surface.
        origin: TaskOrigin,
    },
    /// `TaskQueued`.
    TaskQueued,
    /// `TaskStarted`.
    TaskStarted,
    /// `TaskWaiting`.
    TaskWaiting {
        /// Reason.
        reason: WaitReason,
    },
    /// `TaskResumed` (Waiting → Running).
    TaskResumed,
    /// `TaskReadyForReview`.
    TaskReadyForReview,
    /// `TaskReturnedToWork` (ReadyForReview → Running).
    TaskReturnedToWork,
    /// `TaskCompleted`.
    TaskCompleted,
    /// `TaskFailed`.
    TaskFailed {
        /// Failure code.
        failure_code: String,
    },
    /// `TaskCancelled`.
    TaskCancelled,
    /// `TaskSteered`: durable steering input recorded; no state change.
    TaskSteered {
        /// Steering text.
        text: String,
    },
    /// `TaskNeedsAttention`: attention flag; no state change.
    TaskNeedsAttention {
        /// Reason text.
        reason: String,
    },
    /// `TaskInputQueued`: a durable, ordered user input (REQ-EV-0262); no
    /// state change. Ordering is the aggregate sequence.
    TaskInputQueued {
        /// Client-chosen input id (stable across retries).
        input_id: String,
        /// Dispatch mode.
        mode: InputMode,
        /// Text.
        text: String,
    },
}

impl TaskEvent {
    /// Canonical event type name.
    #[must_use]
    pub const fn event_type(&self) -> &'static str {
        match self {
            Self::TaskCreated { .. } => "TaskCreated",
            Self::TaskQueued => "TaskQueued",
            Self::TaskStarted => "TaskStarted",
            Self::TaskWaiting { .. } => "TaskWaiting",
            Self::TaskResumed => "TaskResumed",
            Self::TaskReadyForReview => "TaskReadyForReview",
            Self::TaskReturnedToWork => "TaskReturnedToWork",
            Self::TaskCompleted => "TaskCompleted",
            Self::TaskFailed { .. } => "TaskFailed",
            Self::TaskCancelled => "TaskCancelled",
            Self::TaskSteered { .. } => "TaskSteered",
            Self::TaskNeedsAttention { .. } => "TaskNeedsAttention",
            Self::TaskInputQueued { .. } => "TaskInputQueued",
        }
    }
}

fn invalid(from: &TaskState, to: &str) -> crate::InvalidTransition {
    crate::InvalidTransition {
        aggregate: "Task",
        from: format!("{from:?}"),
        to: to.into(),
    }
}

impl Task {
    /// Build the initial projection from `TaskCreated`.
    pub fn create(
        task_id: TaskId,
        event: &TaskEvent,
        at: Timestamp,
    ) -> Result<Self, crate::InvalidTransition> {
        match event {
            TaskEvent::TaskCreated {
                session_id,
                goal_text,
                workspace_id,
                base_revision,
                execution_profile,
                policy_profile_id,
                origin,
            } => Ok(Self {
                task_id,
                session_id: *session_id,
                goal_text: goal_text.clone(),
                workspace_id: *workspace_id,
                base_revision: base_revision.clone(),
                execution_profile: execution_profile.clone(),
                policy_profile_id: policy_profile_id.clone(),
                origin: *origin,
                state: TaskState::Created,
                generation: 1,
                created_at: at,
                started_at: None,
                completed_at: None,
                failure_code: None,
            }),
            other => Err(crate::InvalidTransition {
                aggregate: "Task",
                from: "<none>".into(),
                to: other.event_type().into(),
            }),
        }
    }

    /// Apply a subsequent event, enforcing the state machine.
    pub fn apply(
        &mut self,
        event: &TaskEvent,
        at: Timestamp,
    ) -> Result<(), crate::InvalidTransition> {
        use TaskState::*;
        let next = match event {
            TaskEvent::TaskCreated { .. } => return Err(invalid(&self.state, "TaskCreated")),
            TaskEvent::TaskQueued => Some(Queued),
            TaskEvent::TaskStarted => Some(Running),
            TaskEvent::TaskWaiting { reason } => Some(Waiting(*reason)),
            TaskEvent::TaskResumed => {
                if !matches!(self.state, Waiting(_)) {
                    return Err(invalid(&self.state, "TaskResumed"));
                }
                Some(Running)
            }
            TaskEvent::TaskReadyForReview => Some(ReadyForReview),
            TaskEvent::TaskReturnedToWork => {
                if self.state != ReadyForReview {
                    return Err(invalid(&self.state, "TaskReturnedToWork"));
                }
                Some(Running)
            }
            TaskEvent::TaskCompleted => Some(Completed),
            TaskEvent::TaskFailed { failure_code } => {
                self.state.transition(Failed)?;
                self.failure_code = Some(failure_code.clone());
                Some(Failed)
            }
            TaskEvent::TaskCancelled => Some(Cancelled),
            TaskEvent::TaskSteered { .. }
            | TaskEvent::TaskNeedsAttention { .. }
            | TaskEvent::TaskInputQueued { .. } => {
                if self.state.is_terminal() {
                    return Err(invalid(&self.state, event.event_type()));
                }
                None
            }
        };
        if let Some(to) = next {
            self.state = self.state.transition(to)?;
            if to == Running && self.started_at.is_none() {
                self.started_at = Some(at);
            }
            if to.is_terminal() {
                self.completed_at = Some(at);
            }
        }
        self.generation += 1;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn created() -> Task {
        Task::create(
            TaskId::new(),
            &TaskEvent::TaskCreated {
                session_id: SessionId::new(),
                goal_text: "fix the build".into(),
                workspace_id: WorkspaceId::new(),
                base_revision: None,
                execution_profile: "local_trusted".into(),
                policy_profile_id: None,
                origin: TaskOrigin::Cli,
            },
            Timestamp(1),
        )
        .unwrap()
    }

    #[test]
    fn happy_path_reaches_completed_with_generation_per_event() {
        let mut t = created();
        for (i, e) in [
            TaskEvent::TaskQueued,
            TaskEvent::TaskStarted,
            TaskEvent::TaskWaiting {
                reason: WaitReason::Approval,
            },
            TaskEvent::TaskResumed,
            TaskEvent::TaskReadyForReview,
            TaskEvent::TaskReturnedToWork,
            TaskEvent::TaskReadyForReview,
            TaskEvent::TaskCompleted,
        ]
        .iter()
        .enumerate()
        {
            t.apply(e, Timestamp(i as i64 + 2)).unwrap();
        }
        assert_eq!(t.state, TaskState::Completed);
        assert_eq!(t.generation, 9);
        assert_eq!(t.started_at, Some(Timestamp(3)));
        assert_eq!(t.completed_at, Some(Timestamp(9)));
    }

    #[test]
    fn illegal_transitions_are_rejected_and_leave_state_untouched() {
        let mut t = created();
        let err = t.apply(&TaskEvent::TaskStarted, Timestamp(2)).unwrap_err();
        assert_eq!(
            err.to_string(),
            "invalid Task transition Created -> Running"
        );
        assert_eq!(t.state, TaskState::Created);
        assert_eq!(t.generation, 1);
        t.apply(&TaskEvent::TaskCancelled, Timestamp(3)).unwrap();
        assert!(t.apply(&TaskEvent::TaskQueued, Timestamp(4)).is_err());
        assert!(
            t.apply(&TaskEvent::TaskSteered { text: "x".into() }, Timestamp(4))
                .is_err()
        );
        assert!(Task::create(TaskId::new(), &TaskEvent::TaskQueued, Timestamp(1)).is_err());
    }

    #[test]
    fn failure_code_is_recorded_only_on_a_legal_failure() {
        let mut t = created();
        t.apply(&TaskEvent::TaskCompleted, Timestamp(2))
            .unwrap_err();
        t.apply(
            &TaskEvent::TaskFailed {
                failure_code: "PROVIDER_DOWN".into(),
            },
            Timestamp(2),
        )
        .unwrap();
        assert_eq!(t.failure_code.as_deref(), Some("PROVIDER_DOWN"));
        assert!(
            t.apply(
                &TaskEvent::TaskFailed {
                    failure_code: "AGAIN".into()
                },
                Timestamp(3)
            )
            .is_err()
        );
        assert_eq!(t.failure_code.as_deref(), Some("PROVIDER_DOWN"));
    }
}
