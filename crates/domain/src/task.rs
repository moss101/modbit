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
    /// Canonical filesystem root of the user-approved workspace (`None` for a
    /// general Work space with no repository).
    pub workspace_root: Option<String>,
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

/// One typed alternative of a `UserQuestionAsked`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct QuestionOption {
    /// Stable id the answer names.
    pub id: String,
    /// Human label.
    pub label: String,
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
        /// Workspace root path (additive; absent in older events).
        #[serde(default)]
        workspace_root: Option<String>,
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
    /// `UserQuestionAsked` (REQ-EV-0222, docs/28 clarification policy): a typed
    /// question with concrete alternatives; the run suspends until answered.
    /// No state change (the suspension is `TaskWaiting(UserInput)`).
    UserQuestionAsked {
        /// Question id (stable; the answer names it).
        question_id: String,
        /// The model's tool call id the answer becomes the result of.
        call_id: String,
        /// Question text.
        question: String,
        /// Typed alternatives (empty = free text only).
        options: Vec<QuestionOption>,
        /// Whether free text is accepted.
        allow_free_text: bool,
        /// Why the answer is needed: `change_set` | `verification` | `protected_effect` | other.
        reason: String,
        /// Policy flags (`CONFIRMS_REPOSITORY_FACT`: the question asks what the repository already answers).
        flags: Vec<String>,
    },
    /// `AttachmentIngested` (REQ-EV-0190): a channel attachment (desktop, CLI,
    /// API) normalized through the media pipeline into the same canonical
    /// `MediaEnvelope` as a workspace read; bytes by digest only. No state change.
    AttachmentIngested {
        /// Attachment id (sha256 of the bytes).
        attachment_id: String,
        /// Client-supplied file name (label only).
        filename: String,
        /// Channel (`desktop` | `cli` | `api` | other).
        channel: String,
        /// The canonical envelope (boxed: it is the largest event payload).
        envelope: Box<crate::media::MediaEnvelope>,
    },
    /// `UserQuestionAnswered`: the user's typed answer; no state change.
    UserQuestionAnswered {
        /// Question id.
        question_id: String,
        /// Chosen option id, if any.
        option_id: Option<String>,
        /// Free text, if any.
        text: Option<String>,
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
    /// `PlanRecorded` (docs/28 PX-014): the plan artifact before the first write; no state change.
    PlanRecorded {
        /// Object hash of the plan JSON.
        plan_ref: String,
        /// Files the plan expects to change (original write set).
        expected_files: Vec<String>,
        /// Plan version (1 = original).
        version: u32,
    },
    /// `PlanRevised` with a scope delta; no state change.
    PlanRevised {
        /// Object hash of the plan JSON.
        plan_ref: String,
        /// Paths added.
        added: Vec<String>,
        /// Paths removed.
        removed: Vec<String>,
        /// Reason.
        reason: String,
        /// New version.
        version: u32,
    },
    /// `ToolsActivated` (REQ-EV-0134 deferred tool search): the model
    /// discovered deferred tools and they are projected from the next turn;
    /// discovery never authorizes (the Capability Kernel decides at invocation).
    /// No state change.
    ToolsActivated {
        /// The query.
        query: String,
        /// Tool names activated.
        tools: Vec<String>,
    },
    /// `ScopeExpansionRecorded` (docs/28 §3, PX-038): a write outside the
    /// original plan's write set reached a ScopePolicy bound or an
    /// always-ask path; carries the counters and how it was resolved.
    /// No state change.
    ScopeExpansionRecorded {
        /// The paths the expansion covers.
        paths: Vec<String>,
        /// Distinct files already written outside the original write set.
        out_of_plan_files: u32,
        /// Plan revisions so far.
        plan_revisions: u32,
        /// Why the expansion needs a decision.
        reason: String,
        /// `QUESTION_REQUIRED` | `CONTINUE` | `SPLIT` | `STOP` | `FAIL_CLOSED`.
        resolution: String,
        /// The user's answer, when there is one.
        answer: String,
    },
    /// `RetrievalRecorded` (docs/28 §2, PX-015): the task retrieved a file
    /// (read, language-service query, or its own write) at a revision and
    /// content hash — the record an edit of that file needs. No state change.
    RetrievalRecorded {
        /// Path.
        path: String,
        /// Content hash at the retrieval.
        content_hash: String,
        /// Workspace revision.
        workspace_revision: u64,
        /// Tool call.
        tool_call_id: String,
        /// Tool.
        tool_name: String,
    },
    /// `RepairAttemptRecorded` (docs/28 §5, PX-018): recorded before the
    /// attempt's change and verification run; no state change.
    RepairAttemptRecorded {
        /// 1-based ordinal within the task.
        attempt_ordinal: u32,
        /// The open failure signature the attempt targets.
        failure_signature: String,
        /// One-sentence hypothesis.
        hypothesis: String,
        /// Normalized fingerprint of the hypothesis (equivalence key).
        hypothesis_fingerprint: String,
        /// What the agent read or ran.
        evidence_refs: Vec<String>,
        /// Files/symbols and the change intent.
        intended_fix: String,
        /// Object hash of the attempt JSON.
        attempt_ref: String,
        /// Workspace revision when the attempt started.
        start_revision: u64,
    },
    /// `RepairAttemptConcluded` (docs/28 §5): the verification result and
    /// outcome of a recorded attempt; a WORSENED attempt is reverted through
    /// the Change Engine unless justified. No state change.
    RepairAttemptConcluded {
        /// Ordinal.
        attempt_ordinal: u32,
        /// Signature.
        failure_signature: String,
        /// `RESOLVED` | `PARTIAL` | `UNCHANGED` | `WORSENED`.
        outcome: String,
        /// Candidate revision the verification ran at.
        verification_revision: String,
        /// The change tool calls of the attempt.
        change_refs: Vec<String>,
        /// Normalized fingerprint of the attempt's change (paths and content hashes).
        change_fingerprint: String,
        /// Whether a WORSENED change was reverted.
        reverted: bool,
    },
    /// `RepairEscalated` (docs/28 §5): the loop refused to run an attempt
    /// (equivalent hypothesis, bound exhausted, oscillation) and the task
    /// needs attention with its attempt history. No state change.
    RepairEscalated {
        /// Signature.
        failure_signature: String,
        /// Why.
        reason: String,
        /// Attempts so far on that signature.
        attempts: u32,
        /// Object hash of the attempt history JSON.
        history_ref: String,
    },
    /// `SelfReviewRecorded` (docs/28 PX-019); no state change.
    SelfReviewRecorded {
        /// Object hash of the review JSON.
        review_ref: String,
        /// Unresolved findings (block completion).
        unresolved: u32,
    },
    /// `HarnessBudgetExhausted` (docs/14 harness contract 5); no state change.
    HarnessBudgetExhausted {
        /// Which budget.
        budget: String,
        /// Limit.
        limit: u64,
        /// Value reached.
        used: u64,
    },
    /// `NoProgressDetected` (docs/28 PX-039); no state change.
    NoProgressDetected {
        /// Consecutive turns without progress.
        turns: u32,
    },
    /// `ReviewDecisionRecorded` (docs/20 "Trusted Code Surface", REQ-EV-0036):
    /// the user's per-hunk decision on the candidate; no state change (the
    /// `TaskCompleted` / `TaskReturnedToWork` that follows carries the state).
    ReviewDecisionRecorded {
        /// `ACCEPT` or `RETURN`.
        decision: String,
        /// Candidate workspace revision reviewed.
        candidate_revision: u64,
        /// Hunks accepted, as `path#index`.
        accepted: Vec<String>,
        /// Hunks rejected, as `path#index`.
        rejected: Vec<String>,
        /// Commit created on accept.
        commit: Option<String>,
        /// Reviewer note.
        note: String,
        /// Provenance label (`user_review`).
        provenance: String,
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
            Self::UserQuestionAsked { .. } => "UserQuestionAsked",
            Self::UserQuestionAnswered { .. } => "UserQuestionAnswered",
            Self::AttachmentIngested { .. } => "AttachmentIngested",
            Self::PlanRecorded { .. } => "PlanRecorded",
            Self::PlanRevised { .. } => "PlanRevised",
            Self::SelfReviewRecorded { .. } => "SelfReviewRecorded",
            Self::ToolsActivated { .. } => "ToolsActivated",
            Self::ScopeExpansionRecorded { .. } => "ScopeExpansionRecorded",
            Self::RetrievalRecorded { .. } => "RetrievalRecorded",
            Self::RepairAttemptRecorded { .. } => "RepairAttemptRecorded",
            Self::RepairAttemptConcluded { .. } => "RepairAttemptConcluded",
            Self::RepairEscalated { .. } => "RepairEscalated",
            Self::HarnessBudgetExhausted { .. } => "HarnessBudgetExhausted",
            Self::NoProgressDetected { .. } => "NoProgressDetected",
            Self::ReviewDecisionRecorded { .. } => "ReviewDecisionRecorded",
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
                workspace_root,
                base_revision,
                execution_profile,
                policy_profile_id,
                origin,
            } => Ok(Self {
                task_id,
                session_id: *session_id,
                goal_text: goal_text.clone(),
                workspace_id: *workspace_id,
                workspace_root: workspace_root.clone(),
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
            | TaskEvent::TaskInputQueued { .. }
            | TaskEvent::UserQuestionAsked { .. }
            | TaskEvent::UserQuestionAnswered { .. }
            | TaskEvent::AttachmentIngested { .. }
            | TaskEvent::PlanRecorded { .. }
            | TaskEvent::PlanRevised { .. }
            | TaskEvent::SelfReviewRecorded { .. }
            | TaskEvent::ToolsActivated { .. }
            | TaskEvent::ScopeExpansionRecorded { .. }
            | TaskEvent::RetrievalRecorded { .. }
            | TaskEvent::RepairAttemptRecorded { .. }
            | TaskEvent::RepairAttemptConcluded { .. }
            | TaskEvent::RepairEscalated { .. }
            | TaskEvent::HarnessBudgetExhausted { .. }
            | TaskEvent::NoProgressDetected { .. }
            | TaskEvent::ReviewDecisionRecorded { .. } => {
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
                workspace_root: None,
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
