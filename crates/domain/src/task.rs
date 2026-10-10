//! Task aggregate and state machine (docs/13 "Task", docs/31 `tasks`).
//!
//! ```text
//! Created → Queued → Running ↔ Waiting(reason)
//!                     ├→ ReadyForReview → Completed
//!                     ├→ Failed
//!                     └→ Cancelled
//! ```

use serde::{Deserialize, Serialize};

use crate::ids::{RunId, SessionId, TaskId, WorkspaceId};
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
    /// Paused by a person at a turn boundary (REQ-PX-101, `PauseTask`): the
    /// run is suspended with its worktree and checkpoint held, and nothing
    /// resumes it but a `ResumeTask`. Not a fault: no attention is raised.
    Paused,
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
    /// Forked from another task at a checkpoint (REQ-EV-0077/0122).
    Fork,
    /// The Isolated Non-Committing Reviewer's task (EPR-018, docs/27 §9.5):
    /// a disposable review environment on a candidate revision.
    Review,
    /// A subagent's task, admitted by a parent agent (M6.3, docs/14
    /// "Transactional subagent admission").
    Subagent,
    /// An isolated counterfactual replay of another request (EPR-011,
    /// docs/38 "CounterfactualReplay"): the request's snapshot in a scratch
    /// repository, an alternative validated plan, the replay-only ceiling.
    Replay,
    /// Created by an automation trigger (PX-083, docs/68): an unattended run
    /// for a stable principal under a ceiling. Only the Core's own
    /// automation host creates one; its provenance (definition, version,
    /// event id) is the `TaskTriggeredByAutomation` event beside creation.
    Automation,
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
    /// `TaskForked` (REQ-EV-0077/0122, docs/19): this task began as a fork
    /// of another at one of its checkpoints. The capsule object names what
    /// was carried over and what was deliberately not; the worktree is the
    /// fork's own. No state change.
    TaskForked {
        /// The source task.
        from_task_id: String,
        /// The checkpoint the fork's worktree was materialized from.
        from_checkpoint_id: String,
        /// The checkpoint's epoch.
        from_epoch: u32,
        /// The source's latest run at the fork, when any.
        from_run_id: Option<String>,
        /// The runtime cursor the checkpoint recorded.
        from_event_offset: u64,
        /// The session branch generation the fork opened.
        branch_generation: u64,
        /// Object hash of the `BranchCarryoverCapsule`.
        capsule_ref: String,
        /// What was carried: `PLAN` | `DECISIONS` | `EVIDENCE` | `CONTEXT`.
        carried: Vec<String>,
        /// Answered questions carried.
        decisions_carried: u32,
        /// Retrieval records carried (those whose content the fork still has).
        evidence_carried: u32,
        /// Pending approvals of the source that were not carried.
        approvals_dropped: u32,
        /// The fork's worktree root.
        worktree: String,
        /// The fork's branch.
        branch: String,
    },
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
    /// `TaskPauseRequested` (M8.1, docs/30 `:pause`): the person asked the
    /// execution owner to pause at its next safe boundary; no state change
    /// (the owner records the wait it enters).
    TaskPauseRequested {
        /// Who asked.
        requested_by: String,
    },
    /// `TaskResumeRequested` (M8.1, docs/30 `:resume`): the person asked
    /// the execution owner to resume; no state change.
    TaskResumeRequested {
        /// Who asked.
        requested_by: String,
    },
    /// `TaskPaused` (REQ-PX-101): the run parked at a turn boundary because
    /// a person asked (`PauseTask`). Recorded with `TaskWaiting { Paused }`
    /// in one transaction; the typed record of why, who and where. No state
    /// change of its own.
    TaskPaused {
        /// Why, in the requester's words (may be empty).
        reason: String,
        /// Who asked (`Actor` label).
        paused_by: String,
        /// The `PauseTask` command that asked (UUID text).
        command_id: String,
        /// The checkpoint the pause holds (the latest committed one), when
        /// the task has any.
        checkpoint_id: String,
        /// Where the run stopped: `TURN_BOUNDARY`.
        boundary: String,
        /// The run's budgets, so the resume continues under them unless the
        /// resumer names others.
        #[serde(default)]
        max_turns: u32,
        /// Tool-call budget.
        #[serde(default)]
        max_tool_calls: u32,
        /// Consecutive turns without progress allowed.
        #[serde(default)]
        max_no_progress_turns: u32,
    },
    /// `TaskCancelRequested` (M8.1, docs/30 `:cancel`): the person asked
    /// the execution owner to cancel at its next safe boundary; no state
    /// change (`TaskCancelled` follows from the owner).
    TaskCancelRequested {
        /// Who asked.
        requested_by: String,
        /// Reason text.
        reason: String,
    },
    /// `TaskSteered`: durable steering input recorded; no state change.
    TaskSteered {
        /// Steering text.
        text: String,
        /// Where the steer came from when not the person at a client
        /// (`forge_review_comment` for PX-008); empty for the person.
        #[serde(default)]
        provenance: String,
        /// Whether the text is untrusted data the agent weighs rather than
        /// an instruction from the person (PX-008).
        #[serde(default)]
        untrusted: bool,
        /// The queued inputs this dispatch consumed (PX-050): the queue
        /// projection marks exactly these dispatched. A COLLECT dispatch
        /// names every input it coalesced. Empty on a record written before
        /// the queue was managed: such a record consumed the oldest queued
        /// input, as it always did.
        #[serde(default)]
        input_ids: Vec<String>,
    },
    /// `TaskNeedsAttention`: attention flag; no state change.
    TaskNeedsAttention {
        /// Reason text.
        reason: String,
        /// The typed diagnosis behind the reason (REQ-EV-0073): class,
        /// retryability, user action, recovery path and evidence.
        #[serde(default)]
        diagnostic: Option<crate::failure::FailureDiagnostic>,
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
        /// The protected paths (docs/64 DI-9) the question asks leave to
        /// change, as the Core recorded them; the Core set the options, and
        /// only the user's `continue` answer unlocks exactly these paths.
        /// Empty for every other question.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        protected_paths: Vec<String>,
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
        /// Where the input came from when not the person at a client
        /// (`forge_review_comment` for PX-008); empty for the person.
        #[serde(default)]
        provenance: String,
        /// Whether the text is untrusted data (PX-008): the agent reads it
        /// fenced as such, and it grants nothing.
        #[serde(default)]
        untrusted: bool,
    },
    /// `TaskInputEdited` (PX-050, docs/65 AFW-E02): the person changed a
    /// queued input before it was dispatched. Only what the record carries
    /// changes; a dispatched or deleted input is never edited. No state change.
    TaskInputEdited {
        /// The queued input.
        input_id: String,
        /// The new text (empty = unchanged).
        #[serde(default)]
        text: String,
        /// The new dispatch mode, when it changed.
        #[serde(default)]
        mode: Option<InputMode>,
        /// The per-item model selection the person recorded for the input
        /// (empty = unchanged). Recorded and shown; the router's pin policy
        /// (PX-053) decides whether a dispatch honours it.
        #[serde(default)]
        model: String,
    },
    /// `TaskInputRemoved` (PX-050): the person deleted a queued input; it is
    /// never dispatched. No state change.
    TaskInputRemoved {
        /// The queued input.
        input_id: String,
        /// The person's note (log text; never an instruction).
        #[serde(default)]
        reason: String,
    },
    /// `TaskInputReordered` (PX-050): a queued input moved before another
    /// queued input (or to the end). Only queued inputs move. No state change.
    TaskInputReordered {
        /// The input that moved.
        input_id: String,
        /// The queued input it now precedes; empty = the end of the queue.
        #[serde(default)]
        before_input_id: String,
    },
    /// `SendBehaviorSet` (PX-050, docs/65 AFW-E05): what plain Enter does while
    /// a turn runs, and what Send now does. The Core applies it when an input
    /// arrives with no mode of its own; a client only chooses it. No state
    /// change.
    SendBehaviorSet {
        /// `QUEUE` | `COLLECT` | `STEER` | `STOP_AND_SEND` (empty = unchanged).
        #[serde(default)]
        while_running: String,
        /// `INTERRUPT` | `STEER` (empty = unchanged).
        #[serde(default)]
        send_now: String,
    },
    /// `TaskInterruptRequested` (PX-050, docs/65 AFW-E06/E07): the person
    /// asked the run to stop what it is doing at the next safe point. `SEND_NOW`
    /// promotes the named queued input to run next as a typed
    /// interrupt-and-replace; `STOP` ends the turn and pauses the run. The
    /// record is the command's idempotency: one `interrupt_id` interrupts once.
    /// No state change.
    TaskInterruptRequested {
        /// The interrupt's id (the command's own).
        interrupt_id: String,
        /// `SEND_NOW` | `STOP`.
        kind: String,
        /// The queued input a `SEND_NOW` promotes (empty for `STOP`).
        #[serde(default)]
        input_id: String,
        /// The person's note (log text; never an instruction).
        #[serde(default)]
        reason: String,
    },
    /// `TaskInterruptApplied` (PX-050): the run reached the safe point an
    /// interrupt asked for and records what was in flight and how it ended.
    /// A user interrupt is a different typed outcome from a runtime or
    /// transport abort. No state change.
    TaskInterruptApplied {
        /// The interrupt.
        interrupt_id: String,
        /// `STREAM_ABORTED` | `TOOL_CANCELLED` | `IDLE` | `UNKNOWN_OUTCOME`.
        outcome: String,
        /// Whether a model stream was cut (it ended aborted with source
        /// `USER_INTERRUPT`, its partial text kept and marked partial).
        stream_aborted: bool,
        /// Tool calls in flight that were cancelled through the broker.
        #[serde(default)]
        cancelled_calls: Vec<String>,
        /// Tool calls whose outcome is unknown after the interrupt: the new
        /// turn does not start until they are reconciled.
        #[serde(default)]
        unknown_outcome_calls: Vec<String>,
        /// What the run does next: `DISPATCH` (the promoted input starts the
        /// next turn), `PAUSE` (a stop parked the run) or `HOLD` (the next
        /// turn waits for reconciliation).
        next: String,
    },
    /// `RunModeSet` (PX-057, docs/65 AFW-F06/F07): the person set who
    /// approves protected effects for the task. A mode that approves more
    /// carries the person's recorded acknowledgement of the risk.
    /// `RUN_EVERYTHING` is per Core process: after a restart the mode in force
    /// is the last durable one. No state change.
    RunModeSet {
        /// The mode.
        mode: crate::runmode::RunMode,
        /// The mode it replaced.
        #[serde(default)]
        previous: Option<crate::runmode::RunMode>,
        /// The person acknowledged the risk (prompt injection, exfiltration)
        /// of a mode that approves more.
        #[serde(default)]
        acknowledged: bool,
        /// The warning text the person acknowledged (log text).
        #[serde(default)]
        warning: String,
    },
    /// `AllowRuleAdded` (PX-057, docs/65 AFW-F10): a durable allowlist rule.
    /// A policy record, created by a person through the Core. No state change.
    AllowRuleAdded {
        /// The rule.
        rule: crate::runmode::AllowRule,
    },
    /// `AllowRuleRevoked` (PX-057): the rule stops applying. No state change.
    AllowRuleRevoked {
        /// The rule.
        rule_id: String,
        /// Who revoked it (`user:<id>`).
        #[serde(default)]
        revoked_by: String,
        /// The person's note (log text; never an instruction).
        #[serde(default)]
        reason: String,
    },
    /// `TaskModeSet` (PX-051, docs/65 AFW-D03): the user set the task's mode.
    /// The latest one is the task's mode; the posture it selects is enforced
    /// from the next round boundary (`TaskPostureApplied`). No state change.
    TaskModeSet {
        /// The mode.
        mode: crate::mode::TaskMode,
        /// The mode it replaced (`None` at creation).
        #[serde(default)]
        previous: Option<crate::mode::TaskMode>,
        /// `create` | `user`.
        source: String,
        /// The user's note (log text; never an instruction).
        #[serde(default)]
        reason: String,
        /// Leaving PLAN: the plan version the user accepted (0 = none).
        #[serde(default)]
        accepted_plan_version: u32,
    },
    /// `TaskPostureApplied` (PX-051): the run's round boundary adopted a mode;
    /// from here the Capability Kernel enforces that mode's posture. A call in
    /// flight keeps the posture it was decided under. No state change.
    TaskPostureApplied {
        /// The mode now in force.
        mode: crate::mode::TaskMode,
        /// The mode that was in force (`None` at run start).
        #[serde(default)]
        previous: Option<crate::mode::TaskMode>,
        /// `RUN_START` | `ROUND`.
        boundary: String,
        /// The `TaskModeSet` event offset this mode came from (0 = default).
        mode_offset: u64,
    },
    /// `ExecutionPreferenceSet` (PX-053, docs/65 AFW-D13): the user recorded an
    /// execution preference for the task. The router reads it at the next
    /// boundary (`ExecutionPreferenceApplied`). No state change.
    ExecutionPreferenceSet {
        /// The whole preference after this patch.
        preference: crate::mode::ExecutionPreference,
        /// What the command carried.
        patch: crate::mode::ExecutionPreference,
        /// `create` | `start_task` | `user`.
        source: String,
    },
    /// `ExecutionPreferenceApplied` (PX-053): routing read the recorded
    /// preference at a boundary, and what it did. With no signed registry the
    /// outcome is `DIRECT` with a typed reason. No state change.
    ExecutionPreferenceApplied {
        /// The preference read.
        preference: crate::mode::ExecutionPreference,
        /// The `ExecutionPreferenceSet` event offset it came from.
        preference_offset: u64,
        /// `RUN_START` | `ROUND`.
        boundary: String,
        /// `DIRECT` | `ROUTED` | `PINNED`.
        outcome: String,
        /// Typed reason (`NO_ACTIVE_REGISTRY`, `FLOOR_APPLIED`, `FLOOR_UNDEFINED`, `MANUAL_PIN`).
        reason_code: String,
        /// Detail.
        #[serde(default)]
        detail: String,
        /// The registry floor row the objective selected, when one compiled.
        #[serde(default)]
        floor_mode: String,
        /// The effort a dispatch carries (`None` = the catalog's own, or the model exposes none).
        #[serde(default)]
        effort_applied: Option<String>,
        /// The service tier a dispatch carries.
        #[serde(default)]
        service_tier_applied: Option<String>,
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
    /// `PlanAnnotated` (REQ-EV-0118): a person's review note on a plan
    /// version, outside the transcript; when it came with an edited plan
    /// the `PlanRevised` before it carries the new version. No state
    /// change.
    PlanAnnotated {
        /// The version annotated (the new one, when edited).
        version: u32,
        /// Object hash of that version's plan JSON.
        plan_ref: String,
        /// The note.
        note: String,
        /// `user_review` | `cli`.
        provenance: String,
    },
    /// `UserPatchApplied` (PX-005, docs/20 "Constrained inline patch",
    /// docs/29): a person applied a small direct edit through the canonical
    /// ChangeTransaction — revision precondition checked, path policy applied
    /// after symlink resolution, one `FileChanged` on the workspace log, one
    /// revision advance. No state change; thin clients consume it by cursor
    /// and mark code references bound to `before_hash` stale.
    UserPatchApplied {
        /// Root-relative path.
        path: String,
        /// Content hash before.
        before_hash: String,
        /// Content hash after (the new file revision).
        after_hash: String,
        /// Workspace revision after the change.
        workspace_revision: u64,
        /// Workspace revision before it (the precondition the client gave).
        previous_revision: u64,
        /// Always `user_direct_edit`.
        provenance: String,
        /// `review` | `cli` | `ide_adapter`.
        source: String,
        /// Ladder tier the edit target was located at (`exact` |
        /// `whitespace_remap`).
        match_tier: String,
        /// The command that made it (hex), the idempotency key.
        command_id: String,
    },
    /// `ExternalDiagnosticsRecorded` (PX-004, docs/29): an adapter's
    /// language-service diagnostics, normalized with provenance
    /// `external_ide` and bound to the workspace revision and the per-file
    /// revisions they were computed on; the batch is an object by hash.
    /// Context and verification-plan input only — never a verification
    /// result. No state change.
    ExternalDiagnosticsRecorded {
        /// Language service / adapter identity.
        source: String,
        /// Its version.
        source_version: String,
        /// Workspace revision the batch is bound to.
        workspace_revision: u64,
        /// Object hash of the normalized batch.
        batch_ref: String,
        /// Diagnostics recorded.
        recorded: u32,
        /// Diagnostics dropped because their file moved on.
        discarded: u32,
        /// Root-relative paths with recorded diagnostics.
        paths: Vec<String>,
        /// Always `external_ide`.
        provenance: String,
    },
    /// `ExternalDiagnosticsRejected` (PX-004): a batch refused before
    /// anything of it was persisted — `STALE_REVISION` (another workspace
    /// revision) or `MALFORMED`. No state change.
    ExternalDiagnosticsRejected {
        /// Language service / adapter identity.
        source: String,
        /// Its version.
        source_version: String,
        /// Workspace revision the batch named.
        workspace_revision: u64,
        /// `STALE_REVISION` | `MALFORMED`.
        code: String,
        /// Why.
        detail: String,
    },
    /// `ForgePullRequestOpened` (PX-006/007, docs/29): a pull request the
    /// forge adapter opened for this task under an idempotency key — the
    /// record a retry of the same key answers from. No state change.
    ForgePullRequestOpened {
        /// The key the create named.
        idempotency_key: String,
        /// Repository owner.
        owner: String,
        /// Repository.
        repo: String,
        /// Pull request number.
        number: u64,
        /// Web URL.
        url: String,
        /// Head branch.
        head: String,
        /// Head commit.
        head_sha: String,
        /// Base branch.
        base: String,
        /// The adapter's result as returned.
        result: serde_json::Value,
    },
    /// `ForgePullRequestUpdated` (PX-006/007): a pull request the adapter
    /// updated under an idempotency key. No state change.
    ForgePullRequestUpdated {
        /// The key the update named.
        idempotency_key: String,
        /// Repository owner.
        owner: String,
        /// Repository.
        repo: String,
        /// Pull request number.
        number: u64,
        /// Web URL.
        url: String,
        /// State after the update.
        state: String,
        /// The adapter's result as returned.
        result: serde_json::Value,
    },
    /// `ForgeCommentPosted` (PX-125): a status or progress comment the
    /// adapter posted on an issue or pull request under an idempotency key —
    /// the record a retry of the same key answers from. The body is not on
    /// the log, only its digest. No state change.
    ForgeCommentPosted {
        /// The key the post named.
        idempotency_key: String,
        /// `forge.issue.comment` | `forge.pr.comment`.
        tool: String,
        /// Repository owner.
        owner: String,
        /// Repository.
        repo: String,
        /// Issue or pull request number.
        number: u64,
        /// The forge's id for the comment.
        comment_id: u64,
        /// Web URL of the comment.
        url: String,
        /// sha256 of the body as sent (after redaction).
        body_sha256: String,
        /// Credentials replaced in the body before it was sent.
        redactions: u64,
        /// The adapter's result as returned.
        result: serde_json::Value,
    },
    /// `TaskCreatedFromIssue` (PX-010, docs/29): the task was made from a
    /// forge issue the Core read at creation; its text is the attached
    /// context document beside this event (untrusted). No state change.
    TaskCreatedFromIssue {
        /// The issue's web URL.
        url: String,
        /// Issue number.
        number: u64,
        /// Issue title.
        title: String,
        /// `forge_issue` (the Core read the issue, PX-010) or
        /// `forge_webhook` (the forge delivered it to the Cloud API, PX-011).
        provenance: String,
        /// sha256 of the attached text (the `ContextDocumentAttached` document id).
        document_id: String,
    },
    /// `TaskTriggeredByAutomation` (PX-083, docs/68 AUT-C01): the task is the
    /// run of an automation firing. Names the definition, its version, the
    /// event that fired it and the principal whose ceiling it runs under. No
    /// state change.
    TaskTriggeredByAutomation {
        /// The definition.
        automation_id: String,
        /// Its version when it fired.
        version: u32,
        /// The trigger that fired.
        trigger_id: String,
        /// `schedule`, `manual`, `event` or `webhook`.
        trigger_kind: String,
        /// The delivery id or schedule slot id.
        event_id: String,
        /// `sha256(definition | version | event id)`.
        dispatch_key: String,
        /// `user:<id>` or `service:<id>`.
        principal: String,
        /// The definition's hash the enable approval named.
        definition_hash: String,
        /// A dry-run: read-only, every protected effect denied.
        test: bool,
    },
    /// `BrowserSessionOpened` (M7.1, docs/22): the task has a browser
    /// session — one live Chromium session a host holds for it, in its own
    /// partition, with a control lease starting with the agent. No state change.
    BrowserSessionOpened {
        /// The session.
        browser_session_id: String,
        /// The session partition the host's view must use.
        partition: String,
    },
    /// `BrowserHostAttached` (M7.1): a host attached its view to the session
    /// over the authenticated surface socket (REQ-EV-0110), stating how the
    /// view is isolated. No state change.
    BrowserHostAttached {
        /// The session.
        browser_session_id: String,
        /// `electron-main`.
        host_kind: String,
        /// The partition the view was created in.
        partition: String,
        /// Renderer sandbox on.
        sandboxed: bool,
        /// Context isolation on.
        context_isolated: bool,
        /// Node integration (must be false).
        node_integration: bool,
    },
    /// `BrowserNavigated` (M7.1): the session's page state after a
    /// navigation the agent made (untrusted URL and title). No state change.
    BrowserNavigated {
        /// The session.
        browser_session_id: String,
        /// The tool call this record belongs to (IMP-EV-0147: browser
        /// evidence links to its run step through the call; empty before it).
        #[serde(default)]
        tool_call_id: String,
        /// URL.
        url: String,
        /// Title (page content).
        title: String,
        /// The host's state version.
        state_version: u64,
        /// Fingerprint of the state.
        fingerprint: String,
        /// The control lease generation the navigation ran under.
        lease_generation: u64,
    },
    /// `BrowserControlChanged` (M7.6, docs/22 "Live user takeover"): who
    /// holds the session's control lease and the generation it moved to;
    /// agent input stamped with an older generation is fenced. No state change.
    BrowserControlChanged {
        /// The session.
        browser_session_id: String,
        /// `AGENT` | `USER`.
        controller: String,
        /// The generation after the hand-over.
        lease_generation: u64,
    },
    /// `BrowserSessionClosed` (M7.1): the session's view is released. No state change.
    BrowserSessionClosed {
        /// The session.
        browser_session_id: String,
    },
    /// `BrowserActionPerformed` (M7.4, docs/22 "Action hierarchy",
    /// "Verification"; REQ-EV-0280): the agent acted on an entity — what it
    /// acted on (identity, never a node id), what the host did, the state
    /// fingerprints before and after, and whether the declared
    /// postcondition held. The observed transition, as evidence. No state
    /// change.
    BrowserActionPerformed {
        /// The session.
        browser_session_id: String,
        /// The tool call this record belongs to (IMP-EV-0147: browser
        /// evidence links to its run step through the call; empty before it).
        #[serde(default)]
        tool_call_id: String,
        /// The entity's reference.
        reference: String,
        /// `click` | `fill` | `select` | `check` | `uncheck` | `press`.
        action: String,
        /// Role of the target.
        role: String,
        /// Name of the target (untrusted).
        name: String,
        /// Effect class the call ran under (`REVERSIBLE_WRITE` | `EXTERNAL_SIDE_EFFECT`).
        effect_class: String,
        /// Fingerprint before.
        fingerprint_before: String,
        /// Fingerprint after.
        fingerprint_after: String,
        /// Whether the action navigated.
        navigated: bool,
        /// Whether the postcondition held (`None` when none was declared).
        postcondition_held: Option<bool>,
        /// The control lease generation the action ran under.
        lease_generation: u64,
        /// A click placed from a captured region (M7.5): the point inside
        /// the region's box and the reason for the fallback.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        visual_fallback: Option<serde_json::Value>,
        /// The compiled page after the action, as an object reference
        /// (PX-122): what a restart restores the known-state map from.
        #[serde(default, skip_serializing_if = "String::is_empty")]
        page_ref: String,
    },
    /// `BrowserRegionCaptured` (M7.5, docs/22 rung 4 and "V2 media
    /// interaction"): the agent fell back to vision on one region of the
    /// page — which region, why the semantic state was insufficient, the
    /// box captured (never the whole page) and the image's egress
    /// reference. No state change.
    BrowserRegionCaptured {
        /// The session.
        browser_session_id: String,
        /// The tool call this record belongs to (IMP-EV-0147: browser
        /// evidence links to its run step through the call; empty before it).
        #[serde(default)]
        tool_call_id: String,
        /// The region's reference.
        reference: String,
        /// Role (`canvas`, `image`, …).
        role: String,
        /// Why.
        reason: String,
        /// The box captured, `x,y,width,height` in CSS pixels.
        bounds: [u32; 4],
        /// The egress copy's object reference.
        egress_ref: String,
        /// The page's state version.
        state_version: u64,
        /// The page's fingerprint at capture.
        fingerprint: String,
    },
    /// `BrowserPageChanged` (PX-122): the page changed on its own — the
    /// host's mutation observer reported it and the Core read the page
    /// again without a model call. The delta stream of the page as it moved,
    /// by fingerprint, with counts; no state change.
    BrowserPageChanged {
        /// The session.
        browser_session_id: String,
        /// The observer's change counter this read covers.
        change_seq: u64,
        /// Fingerprint of the page the Core held before.
        from_fingerprint: String,
        /// Fingerprint of the page now.
        to_fingerprint: String,
        /// The host's state version.
        state_version: u64,
        /// Entities added.
        added: u64,
        /// Entities removed.
        removed: u64,
        /// Entities whose value or state changed.
        changed: u64,
        /// Text lines added.
        text_added: u64,
        /// Text lines removed.
        text_removed: u64,
        /// Mutations the host folded into the notices behind this read.
        #[serde(default)]
        coalesced: u64,
        /// Milliseconds from the host's first notice to this record.
        #[serde(default)]
        latency_ms: u64,
        /// URL (untrusted).
        url: String,
        /// The compiled page, as an object reference (what a restart restores).
        #[serde(default)]
        page_ref: String,
    },
    /// `BrowserOutcomeUnknown` (PX-121): an agent input to the session may
    /// have happened and nothing says whether it did (the host timed out or
    /// died after dispatch). The session is latched until a fresh
    /// observation reconciles it. No state change.
    BrowserOutcomeUnknown {
        /// The session.
        browser_session_id: String,
        /// The tool call whose outcome is unknown.
        tool_call_id: String,
        /// The tool.
        tool: String,
        /// The action.
        action: String,
        /// The entity acted on.
        reference: String,
        /// What was observed.
        reason: String,
    },
    /// `BrowserOutcomeReconciled` (PX-121): a fresh observation lifted the
    /// latch an unknown outcome set. No state change.
    BrowserOutcomeReconciled {
        /// The session.
        browser_session_id: String,
        /// The observing tool call.
        tool_call_id: String,
        /// The tool call whose outcome had been unknown.
        was_tool_call_id: String,
    },
    /// `ComputerSessionStarted` (PX-069, PX-070): a native-control session
    /// opened for the task after a person approved its observation grant
    /// (docs/66 CUC-C02). The application is named by the identity the
    /// operating system vouches for, never by a window title. No state change.
    ComputerSessionStarted {
        /// The control session.
        control_id: String,
        /// The `computer.start` call that opened it.
        tool_call_id: String,
        /// The approval that bound the grant.
        approval_id: String,
        /// The intent hash the approval bound (application identity, window, scope, expiry bounds).
        intent_hash: String,
        /// Bundle identifier of the application.
        bundle_id: String,
        /// Display name (an untrusted label).
        application: String,
        /// Executable path.
        executable_path: String,
        /// Code-signing identity.
        signing_identity: String,
        /// Process id.
        pid: u32,
        /// `APP` | `SCREEN`.
        scope: String,
        /// The run (the turn the grant ends with).
        run_id: String,
        /// The task mode the grant was given in; a switch ends it.
        mode: String,
        /// When the grant ends (ms since the epoch).
        grant_expires_ms: i64,
        /// Observation calls the grant allows.
        grant_call_cap: u32,
        /// The actuator's lease generation.
        lease_generation: u64,
        /// True when this session opened over an unknown-outcome latch and
        /// its first observation will reconcile it.
        #[serde(default)]
        reconciling: bool,
    },
    /// `ComputerObserved` (PX-069, PX-076): an observation of the target.
    /// Counts and artifact references only, never contents (CUC-F03). No
    /// state change.
    ComputerObserved {
        /// The control session.
        control_id: String,
        /// The observing call.
        tool_call_id: String,
        /// `apps` | `resolve` | `state` | `screenshot` | `wait`.
        kind: String,
        /// The snapshot id of the tree read, when one was.
        #[serde(default)]
        snapshot_id: String,
        /// Elements in the tree.
        #[serde(default)]
        elements: u32,
        /// Secure elements whose values were withheld.
        #[serde(default)]
        secure_withheld: u32,
        /// The stored frame (object reference), when one was kept.
        #[serde(default)]
        artifact_ref: String,
        /// The frame's content digest (after masking).
        #[serde(default)]
        artifact_digest: String,
        /// `lossless` or a ladder rung.
        #[serde(default)]
        codec: String,
        /// `flat` | `photographic`.
        #[serde(default)]
        content: String,
        /// The identical frame was not encoded again.
        #[serde(default)]
        reused: bool,
        /// Secure regions the actuator masked.
        #[serde(default)]
        masked_by_actuator: u32,
        /// Secure regions the Core had to mask.
        #[serde(default)]
        masked_by_core: u32,
    },
    /// `ComputerActionPerformed` (PX-069, PX-070): one approved input and
    /// how it ended. The kind, the rung and the outcome; never the text
    /// typed, never a pixel (CUC-F03). No state change.
    ComputerActionPerformed {
        /// The control session.
        control_id: String,
        /// The call.
        tool_call_id: String,
        /// The approval that bound it.
        approval_id: String,
        /// The intent hash it was approved under.
        intent_hash: String,
        /// `press` | `set_value` | `click` | `move` | `drag` | `type` | `key` | `scroll`.
        kind: String,
        /// `ax` | `coordinate` | `raw`.
        modality: String,
        /// Why that rung.
        #[serde(default)]
        modality_reason: String,
        /// `SUCCEEDED` | `FAILED` | `UNKNOWN`.
        outcome: String,
        /// The refusal code, when it did not succeed.
        #[serde(default)]
        code: String,
        /// Wall time in the actuator call.
        duration_ms: u64,
        /// Whether the expected effect was observed afterwards (`None`: nothing to check).
        #[serde(default)]
        verified: Option<bool>,
    },
    /// `ComputerLatched` (PX-069, CUC-D03): an input may have happened and
    /// nothing says whether it did; further input is refused until it is
    /// reconciled. No state change.
    ComputerLatched {
        /// The control session.
        control_id: String,
        /// The call whose outcome is unknown.
        tool_call_id: String,
        /// The tool.
        tool: String,
        /// The action kind.
        action: String,
        /// What was observed (`ACTUATOR_LOST`, `TIMEOUT`, ...).
        reason: String,
    },
    /// `ComputerLatchResolved` (PX-069): the latch ended, by a fresh
    /// observation in a new session or by the person. No state change.
    ComputerLatchResolved {
        /// The session that observed (empty when the person resolved it).
        control_id: String,
        /// The call whose outcome had been unknown.
        was_tool_call_id: String,
        /// `fresh_observation` | `person`.
        by: String,
    },
    /// `ComputerSessionClosed` (PX-069, PX-076, CUC-F03): a control session
    /// ended. The audit of a session: counts by kind, the screenshot count,
    /// the duration, the outcome, the application and the approvals - never
    /// contents. No state change.
    ComputerSessionClosed {
        /// The control session.
        control_id: String,
        /// `RELEASED` | `RUN_ENDED` | `USER_ABORTED` | `EMERGENCY_STOP` |
        /// `WATCHDOG` | `GRANT_EXPIRED` | `ACTUATOR_LOST` | `WINDOW_GONE` |
        /// `CORE_RESTART` | `MODE_CHANGED` | `LATCHED`.
        reason: String,
        /// Bundle identifier of the application.
        bundle_id: String,
        /// `APP` | `SCREEN`.
        scope: String,
        /// How long it was open.
        duration_ms: u64,
        /// Inputs by kind.
        actions_by_kind: std::collections::BTreeMap<String, u32>,
        /// Screenshots taken.
        screenshots: u32,
        /// Observations (state reads, waits, screenshots).
        observations: u32,
        /// `OK` | `LATCHED` | `STOPPED`.
        outcome: String,
        /// The approvals that bound its inputs.
        approval_ids: Vec<String>,
    },
    /// `ComputerArtifactExpired` (PX-076, CUC-F01): a screenshot the task's
    /// retention policy no longer keeps was removed from the object store.
    /// The record of the observation stays; the pixels are gone. No state
    /// change.
    ComputerArtifactExpired {
        /// The stored frame (object reference).
        artifact_ref: String,
        /// The retention that applied, in seconds.
        retention_secs: u64,
        /// When it was taken (ms since the epoch).
        taken_at_ms: i64,
    },
    /// `ComputerUserAborted` (PX-069, CUC-D05): the person stopped native
    /// control. Sticky for the rest of the run: no input, start or release
    /// is accepted. No state change.
    ComputerUserAborted {
        /// The run the stop applies to.
        run_id: String,
        /// Why (`on-screen Stop`, `emergency stop`).
        reason: String,
    },
    /// `BrowserCredentialFilled` (M7.8, docs/22 "Credentials"): a credential
    /// the person bound to an origin was filled by handle into a field of a
    /// page at that origin by the host, from its own custody. The handle,
    /// the origin and the field — never the value. No state change.
    BrowserCredentialFilled {
        /// The session.
        browser_session_id: String,
        /// The tool call this record belongs to (IMP-EV-0147: browser
        /// evidence links to its run step through the call; empty before it).
        #[serde(default)]
        tool_call_id: String,
        /// The credential's handle.
        credential: String,
        /// The origin it is bound to (the page's origin at the fill).
        origin: String,
        /// The field's reference, role and name.
        reference: String,
        /// The field's role.
        role: String,
        /// The field's accessible name.
        name: String,
        /// The page's state version at the fill.
        state_version: u64,
        /// The control lease generation the request carried.
        lease_generation: u64,
    },
    /// `SandboxLeaseAcquired` (M8.5, docs/21 "Sandbox substrate boundary",
    /// docs/30): the task's sandbox was provisioned through the Sandbox
    /// Gateway and admitted; its tools act inside it from here on. The
    /// identity of the sandbox — never a credential. No state change.
    SandboxLeaseAcquired {
        /// The gateway's sandbox id.
        sandbox_id: String,
        /// `microvm` | `reference`.
        backend: String,
        /// Whether the backend isolates.
        isolated: bool,
        /// The verified image's guest version (empty when unverified).
        #[serde(default)]
        image_version: String,
        /// The guest's boot id.
        boot_id: String,
        /// The workspace root inside the guest.
        workspace_root: String,
        /// The worker's cloud session lease generation it was issued under.
        lease_generation: u64,
        /// The egress the broker admits for it (`host:port`; M8.6).
        #[serde(default)]
        egress: Vec<String>,
        /// The credentialed virtual hosts the broker serves it (M8.6) —
        /// the hosts, never the handles' secrets.
        #[serde(default)]
        credentials: Vec<String>,
        /// Whether the sandbox may run the task's browser (M8.8): the
        /// lease grants `browser.control`.
        #[serde(default)]
        browser: bool,
    },
    /// `TaskHandedOff` (M8.7, docs/21 "Handoff local → cloud"): this Core
    /// exported the task — its log, its objects, an immutable workspace
    /// checkpoint over the repository's bundle — for a cloud continuation
    /// and parked it; the bundle carries no raw secret. No state change.
    TaskHandedOff {
        /// The bundle's manifest hash (content-addressed).
        bundle_hash: String,
        /// The workspace checkpoint the bundle carries.
        checkpoint_id: String,
        /// Git HEAD at export.
        git_head: String,
        /// The capabilities the continuation needs (what the run has used).
        capabilities: Vec<String>,
        /// The secret handles the continuation names (never their values).
        secret_handles: Vec<String>,
    },
    /// `TaskHandoffAdmitted` (M8.7): the cloud admitted the handoff — the
    /// continuation's capabilities are within what `cloud_isolated` serves
    /// — and holds the bundle; a worker resumes the task from it. No state
    /// change.
    TaskHandoffAdmitted {
        /// The bundle's manifest hash.
        bundle_hash: String,
        /// The tenant the task came from (as its envelopes say).
        from_tenant: String,
        /// The capabilities checked.
        capabilities: Vec<String>,
    },
    /// `TaskWorkspaceRebound` (M8.7): the task's workspace is now this root
    /// (a worker materialized the handoff bundle here). No state change;
    /// the projection's `workspace_root` follows.
    TaskWorkspaceRebound {
        /// The new root.
        workspace_root: String,
        /// Why (`handoff`).
        reason: String,
        /// The execution profile from here on (`cloud_isolated` for a
        /// continuation in the cloud); empty keeps the task's.
        #[serde(default)]
        execution_profile: String,
    },
    /// `SandboxReleased` (M8.5): the task ended and its sandbox was
    /// destroyed. No state change.
    SandboxReleased {
        /// The sandbox.
        sandbox_id: String,
        /// Why (`task_completed` | `task_cancelled` | `task_failed`).
        reason: String,
    },
    /// `SandboxLost` (M8.5, docs/21 "Sandbox recovery"): the sandbox stopped
    /// answering; in-flight calls are unknown until reconciled (M8.9 brings
    /// the recovery). No state change.
    SandboxLost {
        /// The sandbox.
        sandbox_id: String,
        /// What was observed.
        detail: String,
    },
    /// `EnvironmentPinned` (REQ-EV-0062/0146; docs/21 "Environment
    /// revisions"): the run pinned the environment revision it runs in —
    /// the definition files (by hash), the toolchain as observed, the
    /// `PATH` entries, the variables' names. No state change.
    EnvironmentPinned {
        /// The run.
        run_id: RunId,
        /// The revision's content digest.
        digest: String,
        /// The object holding the revision.
        revision_ref: String,
        /// The definition files: `kind`, `path`, `sha256`.
        sources: Vec<serde_json::Value>,
        /// The tools: `name`, `version`.
        toolchain: Vec<serde_json::Value>,
        /// `PATH` entries in front.
        path: Vec<String>,
        /// Variable names (never values).
        env_names: Vec<String>,
        /// Problems compiling the definition (part of the revision).
        problems: Vec<String>,
    },
    /// `EnvironmentStale` (REQ-EV-0021/0062): a resumed run found the
    /// environment no longer the revision it pinned; it waits for an
    /// explicit rebuild. No state change.
    EnvironmentStale {
        /// The run.
        run_id: RunId,
        /// What it pinned.
        pinned_digest: String,
        /// What is there now.
        current_digest: String,
        /// What differs.
        changes: Vec<String>,
    },
    /// `EnvironmentRebuilt` (REQ-EV-0021/0146): the task's environment was
    /// re-captured and pinned anew, by an explicit request. No state change.
    EnvironmentRebuilt {
        /// The digest before (empty when nothing was pinned).
        from_digest: String,
        /// The digest now.
        to_digest: String,
        /// The object holding the revision.
        revision_ref: String,
        /// What changed.
        changes: Vec<String>,
    },
    /// `SandboxRestored` (M8.9, docs/21 "Sandbox recovery"): a fresh sandbox
    /// took the lost one's place and the task's latest checkpoint was
    /// written into it — the worktree as it was at that checkpoint; what
    /// came after is the model's to redo. No state change.
    SandboxRestored {
        /// The sandbox the worktree was restored into.
        sandbox_id: String,
        /// The sandbox that was lost.
        replaced: String,
        /// The checkpoint restored (empty: none existed, the seed alone).
        checkpoint_id: String,
        /// Its epoch (0 with no checkpoint).
        epoch: u32,
        /// Files written from objects.
        files_written: u32,
        /// Files removed because the checkpoint did not have them.
        files_removed: u32,
    },
    /// M7.7 (docs/22 "Prompt-injection isolation", REQ-EV-0284): a
    /// security-relevant attempt the runtime saw and answered — content
    /// shaped like instructions to the agent in what a tool returned
    /// (`PROMPT_INJECTION_SUSPECTED`: marked, never obeyed), or a tool call
    /// whose arguments carried a credential in the Core's custody
    /// (`SECRET_EXFILTRATION_BLOCKED`: refused before any effect). The
    /// record names the shape, never the secret.
    SecurityEventRecorded {
        /// `PROMPT_INJECTION_SUSPECTED` | `SECRET_EXFILTRATION_BLOCKED`.
        kind: String,
        /// Where it came from: the tool whose observation or arguments carried it.
        tool_name: String,
        /// The call.
        tool_call_id: String,
        /// The shapes matched (`OVERRIDE_INSTRUCTIONS`, `EXFILTRATE_SECRET`, …) or the field.
        patterns: Vec<String>,
        /// A bounded, whitespace-normalized excerpt (untrusted) or the refusal.
        detail: String,
        /// What the runtime did: `MARKED` (the observation carries the finding) or `BLOCKED`.
        action: String,
    },
    /// `SloStageRecorded` (IMP-EV-0023, docs/34 "Metrics"): one rung of a
    /// cloud task's SLO ladder — `REQUESTED` (the run was asked for),
    /// `PREWARM` (what a warm pool offered: `NONE` today), `SANDBOX_REQUESTED`,
    /// `SANDBOX_READY` (`warm` when the task's sandbox was reused rather than
    /// provisioned), `FIRST_TOKEN` (the model's first output of the run) and
    /// `FIRST_TOOL` (the run's first tool dispatch). No state change.
    SloStageRecorded {
        /// The rung.
        stage: String,
        /// When (ms since the epoch, the Core's clock).
        at_ms: i64,
        /// The run it belongs to, once one exists.
        #[serde(default)]
        run_id: Option<String>,
        /// For `SANDBOX_READY` and `PREWARM`: whether the start was warm.
        #[serde(default)]
        warm: Option<bool>,
        /// What the rung saw (a sandbox id, a model, a tool).
        #[serde(default)]
        detail: String,
    },
    /// `BrowserPageObserved` (M7.3, docs/22 "state fingerprint / delta"):
    /// the agent read the page — in full or as the delta since its last
    /// read — and this is the state it saw, by fingerprint. The delta stream
    /// on the log: what changed between reads, as counts. No state change.
    BrowserPageObserved {
        /// The session.
        browser_session_id: String,
        /// The tool call this record belongs to (IMP-EV-0147: browser
        /// evidence links to its run step through the call; empty before it).
        #[serde(default)]
        tool_call_id: String,
        /// The host's state version.
        state_version: u64,
        /// Content fingerprint of the compiled page.
        state_fingerprint: String,
        /// Identity hash of the entity set.
        entity_hash: String,
        /// `full` | `delta`.
        mode: String,
        /// Entities in the page.
        entity_count: u64,
        /// Entities added since the previous read (delta).
        added: u64,
        /// Entities removed since the previous read (delta).
        removed: u64,
        /// Entities changed since the previous read (delta).
        changed: u64,
        /// URL (untrusted).
        url: String,
        /// The compiled page as an object reference (PX-122): the known-state
        /// map a restarted Core restores, so the same element keeps its
        /// reference and a delta against this fingerprint still works.
        #[serde(default, skip_serializing_if = "String::is_empty")]
        page_ref: String,
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
    /// `ToolProjectionConfigured` (PX-114): the task's tool projection mode
    /// and schema-bytes budget were set, by a person's command. It changes
    /// what the model is shown from the next round on, never what it may do
    /// (the Capability Kernel decides every call). No state change.
    ToolProjectionConfigured {
        /// `direct` or `exec_only`; empty keeps the model's or the Core's.
        mode: String,
        /// The most bytes of tool schemas one request may carry; 0 keeps
        /// the model's or the Core's budget.
        max_projection_bytes: u64,
    },
    /// `ProgramStarted` (docs/16 "Procedural Tool Runtime", M5.4): a
    /// `proc.exec` program began in the isolate with the bindings and budget
    /// named; every binding call is its own tool call on the log. No state
    /// change.
    ProgramStarted {
        /// Program handle (the exec call id).
        handle: String,
        /// Object hash of the program source.
        program_ref: String,
        /// Effects the program declared (tool names or toolsets).
        declared_effects: Vec<String>,
        /// Bindings the program was given.
        bindings: Vec<String>,
        /// Budget (JSON: cpu_time_ms, memory_bytes, max_tool_calls, …).
        budget: serde_json::Value,
    },
    /// `ProgramEnded`: how the program ended and what it produced. No state
    /// change.
    ProgramEnded {
        /// Program handle.
        handle: String,
        /// `COMPLETED` | `FAILED` | `BUDGET_EXHAUSTED` | `CANCELLED`.
        status: String,
        /// The budget that ended it, when one did.
        budget_exhausted: Option<String>,
        /// Object hash of the full outcome (value, error, log, calls).
        outcome_ref: Option<String>,
        /// Binding calls the program made.
        tool_calls: u32,
        /// Wall-clock milliseconds the program ran.
        elapsed_ms: u64,
        /// Interrupt polls the interpreter made.
        interrupt_polls: u64,
    },
    /// `SkillSelected` (docs/16 "Skills", M5.5; REQ-EV-0105's instruction
    /// manifest): a skill's compiled instructions entered the prompt, with
    /// its identity, lifecycle, why it was selected and what it compiled to.
    /// No state change.
    SkillSelected {
        /// Skill name.
        name: String,
        /// Version.
        version: String,
        /// Package content hash.
        content_hash: String,
        /// Lifecycle state at selection (`ENABLED`).
        lifecycle: String,
        /// Where the package was loaded from.
        source: String,
        /// Why (`EXPLICIT`, or `TRIGGER:<phrase>`).
        reason: String,
        /// Hash of the injected instructions.
        instructions_hash: String,
        /// Whether the instructions were cut at the budget.
        instructions_truncated: bool,
        /// Required tools the turn projects.
        tool_projection: Vec<String>,
        /// Required tools the turn does not project.
        tools_unavailable: Vec<String>,
        /// EPR-013: evaluation-qualified — a trusted signature over the
        /// content, a PROMOTE evaluation of that exact content, and a package
        /// outside any workspace an agent can write. Only runs whose skills
        /// are all qualified become outcome statistics.
        #[serde(default)]
        qualified: bool,
    },
    /// `RulesSelected` (REQ-EV-0059 / 0105 / 0129): the workspace and user
    /// rules in the prompt from this turn, each with its layer, source, hash
    /// and why it is active; the scoped rules still dormant; the expired;
    /// the conflicts with their winner; the files that were not rules. No
    /// state change.
    RulesSelected {
        /// Active rules (`{id, layer, source, hash, reason}`).
        active: Vec<serde_json::Value>,
        /// Dormant scoped rule ids.
        dormant: Vec<String>,
        /// Expired rules as `id:source`.
        expired: Vec<String>,
        /// Conflicts (`{id, winner, winner_layer, loser, loser_layer, decided_by}`).
        conflicts: Vec<serde_json::Value>,
        /// Invalid files as `source: why`.
        invalid: Vec<String>,
        /// The files that exist and are not in force, as `{source, reason}`
        /// (REQ-PX-107: an untrusted repository's AGENTS.md, a link out of
        /// the repository, a file past the cap).
        #[serde(default)]
        not_loaded: Vec<serde_json::Value>,
    },
    /// `AgentNodeCreated` (M6.1, docs/13 "Agent node", docs/14
    /// "AgentGraph"): a logical agent joined the task's AgentGraph — the
    /// primary at the first run, a specialist or subagent when admitted.
    /// No task state change.
    AgentNodeCreated {
        /// The node as created.
        node: crate::agent::AgentNode,
    },
    /// `AgentNodeTransitioned`: a node's status moved (docs/13 "Subagent").
    /// No task state change.
    AgentNodeTransitioned {
        /// Node.
        agent_id: crate::AgentId,
        /// From.
        from: crate::agent::AgentStatus,
        /// To.
        to: crate::agent::AgentStatus,
        /// The run it executes in, when it has one.
        run_id: Option<crate::RunId>,
        /// Why.
        reason: String,
    },
    /// `AgentBindingChanged` (REQ-EV-0256): the node runs on another
    /// binding under policy (a continuation on a stronger solver, a
    /// fallback) while its identity, lineage and run stay. No state change.
    AgentBindingChanged {
        /// Node.
        agent_id: crate::AgentId,
        /// Before.
        from: crate::agent::AgentBinding,
        /// After.
        to: crate::agent::AgentBinding,
        /// Why.
        reason: String,
    },
    /// `SubagentAdmitted` (M6.3, REQ-EV-0267; docs/14 "Transactional
    /// subagent admission"): the admission transaction committed whole —
    /// capacity ticket, parent generation, write-set check, worktree,
    /// capability lease, agent node and work ownership — and the child's
    /// task exists. Recorded on the parent task. No state change.
    SubagentAdmitted {
        /// The child node.
        agent_id: crate::AgentId,
        /// The child's task.
        child_task_id: crate::TaskId,
        /// Object hash of the `AgentExecutionCapsule` (REQ-EV-0048).
        capsule_ref: String,
        /// The capacity ticket the child holds.
        ticket_id: String,
        /// The child's worktree root.
        worktree: String,
        /// The child's branch.
        branch: String,
        /// The paths the child may write.
        write_scope: Vec<String>,
        /// The work node it owns.
        work_node: String,
        /// The idempotency key the spawn carried.
        idempotency_key: String,
        /// `FOREGROUND` | `BACKGROUND` — the mode in force.
        mode: String,
        /// REQ-EV-0180: why — `SEPARABLE` (the parent can go on without
        /// this result: background), `BLOCKING` (the parent's own pending
        /// work depends on the child's node: foreground whatever was
        /// asked) or `REQUESTED` (foreground by the spawn's choice).
        #[serde(default)]
        scheduling: String,
        /// REQ-EV-0115: the profile compiled into the capsule, if any.
        #[serde(default)]
        profile: String,
        /// The tools the profile asked for that were dropped, with why.
        #[serde(default)]
        narrowed_tools: Vec<String>,
        /// REQ-PX-116: what the admission reserved against the parent — the
        /// child's budget after it was clamped to the parent's remainder.
        /// Written in the same append as the node and the work ownership,
        /// so a reservation exists exactly when the child does.
        #[serde(default)]
        reserved_turns: u32,
        /// Tool calls reserved.
        #[serde(default)]
        reserved_tool_calls: u32,
        /// Cost reserved, minor units (0 = the parent had no cost cap).
        #[serde(default)]
        reserved_cost_minor: u64,
        /// Wall clock the child may use, milliseconds (0 = no deadline).
        #[serde(default)]
        reserved_wall_ms: u64,
        /// What the clamp changed, in words (`max_turns 20 -> 7`).
        #[serde(default)]
        clamped: Vec<String>,
    },
    /// `TaskBudgetsSet` (REQ-PX-116): the cost, wall-clock and delegation
    /// limits of the task. They bind every later run of it. No state
    /// change.
    TaskBudgetsSet {
        /// Cost cap in minor units; 0 = none.
        max_cost_minor: u64,
        /// Wall-clock cap across the task's runs, milliseconds; 0 = none.
        max_wall_ms: u64,
        /// Live children at once; 0 = the default.
        max_children: u32,
        /// No `agent.spawn` for this task.
        forbid_spawn: bool,
    },
    /// `SubagentAdmissionRefused`: an admission step failed and everything
    /// taken before it was returned; nothing started (REQ-EV-0267). No
    /// state change.
    SubagentAdmissionRefused {
        /// The key the spawn carried.
        idempotency_key: String,
        /// Refusal code.
        code: String,
        /// Why.
        detail: String,
        /// The step that refused: `IDEMPOTENCY` | `DEPTH` | `PARENT` |
        /// `WRITE_SET` | `CAPACITY` | `WORKTREE` | `LEASE` | `START`.
        stage: String,
        /// What was rolled back, in order.
        rolled_back: Vec<String>,
    },
    /// `SubagentCapsuleBound` (M6.3): on the child's own log, the capsule it
    /// runs inside and the parent it reports to; the child's harness reads
    /// its write scope and tool set from here. No state change.
    SubagentCapsuleBound {
        /// The child node.
        agent_id: crate::AgentId,
        /// The parent task.
        parent_task_id: crate::TaskId,
        /// Object hash of the `AgentExecutionCapsule`.
        capsule_ref: String,
    },
    /// `SubagentResultRecorded` (M6.5, docs/14 "Agent-to-agent
    /// communication"): the child's typed result envelope, on the parent
    /// task. No state change.
    SubagentResultRecorded {
        /// The child node.
        agent_id: crate::AgentId,
        /// The child's task.
        child_task_id: crate::TaskId,
        /// `COMPLETED` | `FAILED` | `CANCELLED` | `WAITING`.
        status: String,
        /// Object hash of the `SubagentResult`.
        result_ref: String,
        /// The summary the child gave.
        summary: String,
        /// Paths the child changed in its worktree.
        artifacts: Vec<String>,
        /// Evidence references (verification runs, objects, offsets).
        evidence_refs: Vec<String>,
        /// Unresolved risks (self-review findings left open, refusals).
        unresolved_risks: Vec<String>,
        /// The child's branch, for the parent's merge.
        branch: String,
    },
    /// `ReviewEnvironmentAdmitted` (EPR-018, docs/27 §9.5): a disposable
    /// review environment stands for this candidate task — a scratch
    /// worktree at the candidate revision, its own review task under the
    /// `review_isolated` profile, a lease confined to the scratch tree, the
    /// host's real sandbox. On the candidate task; no state change.
    ReviewEnvironmentAdmitted {
        /// Environment id.
        env_id: String,
        /// The reviewer's task.
        review_task_id: crate::TaskId,
        /// The scratch worktree.
        worktree: String,
        /// The branch the scratch worktree is on.
        branch: String,
        /// The candidate revision it holds.
        revision: String,
        /// The confined lease.
        lease_id: crate::CapabilityLeaseId,
        /// `seatbelt` | `seccomp-net`.
        sandbox: String,
    },
    /// `ReviewEnvironmentBound` (EPR-018): on the review task's own log,
    /// the environment it runs in and the candidate it reviews. No state
    /// change.
    ReviewEnvironmentBound {
        /// Environment id.
        env_id: String,
        /// The candidate task.
        candidate_task_id: crate::TaskId,
        /// The scratch worktree.
        worktree: String,
        /// The candidate revision it holds.
        revision: String,
    },
    /// `ReviewEnvironmentDisposed` (EPR-018): the environment's processes
    /// were ended, its lease revoked and its worktree removed. No state
    /// change.
    ReviewEnvironmentDisposed {
        /// Environment id.
        env_id: String,
        /// Processes ended.
        killed: u32,
        /// Whether the worktree is gone.
        worktree_removed: bool,
        /// Why.
        reason: String,
    },
    /// `ReviewerResultRecorded` (REQ-EPR-007, docs/27 §9.5): the Isolated
    /// Non-Committing Reviewer's structured result for the candidate at a
    /// revision — verdict, confidence, findings (each validated against the
    /// tree at that revision, or marked unsupported), unresolved questions —
    /// untrusted until validated, stale once the candidate moves. On the
    /// candidate task; no state change.
    ReviewerResultRecorded {
        /// The review task.
        review_task_id: crate::TaskId,
        /// The review environment.
        env_id: String,
        /// The revision reviewed.
        candidate_revision: u64,
        /// `PASS` | `REVISE` | `BLOCK` | `UNCERTAIN`.
        verdict: String,
        /// 0..=100.
        confidence: u32,
        /// Object hash of the `ReviewerResult`.
        result_ref: String,
        /// Findings that resolved against the tree.
        validated_findings: u32,
        /// Findings that did not (kept as unresolved assertions).
        unsupported_findings: u32,
        /// Highest validated severity (`CRITICAL` | `HIGH` | `MEDIUM` | `LOW` | `NONE`).
        highest_severity: String,
        /// One line.
        summary: String,
    },
    /// `RevisionActivated` (REQ-EPR-007): a bounded revision of the
    /// candidate after a `REVISE` verdict — the task returned to work with
    /// the validated findings as its typed input, a new attempt on the
    /// reviser (or solver) binding. No state change (the transitions follow).
    RevisionActivated {
        /// Ordinal of this revision, 1-based.
        revision: u32,
        /// The plan's bound.
        max_revisions: u32,
        /// The reviewer result the revision answers.
        result_ref: String,
        /// Binding the revision runs on.
        endpoint: String,
        /// Model.
        model: String,
    },
    /// `SubagentProtectedEffect` (REQ-EV-0046, docs/14 "Agent-to-agent
    /// communication"): a background child asked for an effect above its
    /// capsule's ceiling. The child is refused before any effector — a
    /// detached agent consumes grants but never opens an interactive
    /// privilege expansion of its own — and the parent transitions to its
    /// attention state (`TaskNeedsAttention` follows) to decide. On the
    /// parent task; no state change.
    SubagentProtectedEffect {
        /// The child node.
        agent_id: crate::AgentId,
        /// The child's task.
        child_task_id: crate::TaskId,
        /// The idempotency key the spawn carried.
        idempotency_key: String,
        /// The tool the child asked for.
        tool: String,
        /// Its effect class (`PROTECTED_WRITE` | `EXTERNAL_SIDE_EFFECT` |
        /// `SECRET_ACCESS` | `DESTRUCTIVE`).
        effect_class: String,
        /// The ceiling the capsule allows.
        ceiling: String,
        /// The model's call id, for the parent's reading of the child's log.
        call_id: String,
    },
    /// `WorkNodesChanged` (M6.1, REQ-EV-0052 / 0120): a plan version
    /// created or changed work nodes; the nodes as they stand after the
    /// change, so a reader needs no earlier record to know their state.
    /// No task state change.
    WorkNodesChanged {
        /// Plan version that carried the change.
        plan_version: u32,
        /// The nodes touched, as they now are.
        changed: Vec<crate::agent::WorkNode>,
        /// Nodes whose `PENDING`/`READY` followed from the change.
        ready: Vec<crate::agent::WorkNodeId>,
    },
    /// `CapacityTicketGranted` (M6.2, REQ-EV-0272, docs/14 "Capacity
    /// tickets"): a holder consumed a ticket for the resource vector it
    /// needs, leased until `expires_at_ms` under `generation`. No state
    /// change.
    CapacityTicketGranted {
        /// Ticket id.
        ticket_id: String,
        /// Holder (`run:<id>`, `agent:<id>`, …).
        holder: String,
        /// The vector held (JSON `ResourceVector`).
        holds: serde_json::Value,
        /// Lease expiry, ms.
        expires_at_ms: i64,
        /// Lease generation at grant.
        generation: u64,
    },
    /// `CapacityTicketReleased`: the ticket's capacity returned to the pool
    /// (released by its holder, or lapsed at expiry). No state change.
    CapacityTicketReleased {
        /// Ticket id.
        ticket_id: String,
        /// Holder.
        holder: String,
        /// `RELEASED` | `LAPSED`.
        reason: String,
    },
    /// `PausedCapacityReleased` (REQ-PX-101): a paused task kept its run's
    /// capacity ticket for the configured idle bound and gave it back when the
    /// bound passed. A resume after this re-enters through admission and may be
    /// refused `CAPACITY_EXHAUSTED`. No state change.
    PausedCapacityReleased {
        /// Ticket id.
        ticket_id: String,
        /// Holder.
        holder: String,
        /// How long the pause held it, ms.
        held_ms: u64,
        /// The idle bound in force, ms.
        idle_bound_ms: u64,
        /// `IDLE_BOUND` | `RESUMED_STALE` (a hold found lapsed at a resume).
        reason: String,
    },
    /// `CapacityDenied`: the pool could not cover the request; nothing was
    /// reserved and nothing started. No state change.
    CapacityDenied {
        /// Holder that asked.
        holder: String,
        /// The vector asked for (JSON `ResourceVector`).
        needs: serde_json::Value,
        /// Refusal code (`CAPACITY_EXHAUSTED` | `EXCEEDS_POOL` | …).
        code: String,
        /// The first dimension that did not fit.
        dimension: String,
        /// Units needed there.
        needed: u32,
        /// Units available there.
        available: u32,
        /// Holders of the live tickets at the time.
        live: Vec<String>,
    },
    /// `MediaBridged` (REQ-EV-0184 / 0185): the routed model takes no input
    /// of this media's modality, so a configured vision bridge described it
    /// (or failed to); the description is lossy, untrusted data. No state
    /// change.
    MediaBridged {
        /// Digest of the egress copy described.
        digest: String,
        /// MIME.
        mime: String,
        /// The routed model that could not take it (`endpoint/model`).
        routed_model: String,
        /// Bridge endpoint.
        bridge_endpoint: String,
        /// Bridge model.
        bridge_model: String,
        /// Object hash of the description, when one came back.
        description_ref: Option<String>,
        /// Bridge input tokens.
        input_tokens: u64,
        /// Bridge output tokens.
        output_tokens: u64,
        /// Why no description came back, when it did not.
        error: Option<String>,
    },
    /// `SkillIndexRecorded` (REQ-PX-105): the skill index the model was told
    /// of changed — the skills it may load, in the form each took under the
    /// aggregate budget. Recorded when it differs from the last. No state
    /// change.
    SkillIndexRecorded {
        /// Skills in the index, in order, with the form each took
        /// (`name:FULL|SHORT|NAME_ONLY|OMITTED`).
        entries: Vec<String>,
        /// Skills dropped for want of budget.
        omitted: u32,
        /// Tokens the index takes.
        tokens: u32,
        /// The aggregate budget of the skill segment.
        budget_tokens: u32,
        /// Tokens the injected bodies of selected skills take.
        body_tokens: u32,
        /// Hash of the index text.
        index_hash: String,
    },
    /// `SkillRejected`: a skill named for the task was not used, with why.
    /// No state change.
    SkillRejected {
        /// Skill name.
        name: String,
        /// Error code (`NOT_ENABLED`, `UNKNOWN`, …).
        code: String,
        /// Reason.
        reason: String,
    },
    /// `ContextEpochOpened` (docs/19 "Compaction epochs", REQ-EV-0056):
    /// the model-visible transcript before `source_head_offset` is replaced by
    /// the epoch's projection. The canonical log is untouched. No state change.
    ContextEpochOpened {
        /// Monotonic epoch number.
        epoch: u32,
        /// The previous epoch, when any.
        previous_epoch: Option<u32>,
        /// Store offset the compacted range ended at.
        source_head_offset: u64,
        /// Transcript entries the epoch summarised.
        source_entries: u32,
        /// Object hash of the manifest.
        manifest_ref: String,
        /// Hash of the manifest's fields, for a client that reads it back.
        manifest_hash: String,
        /// Estimated tokens of the projection.
        projection_tokens: u32,
        /// The compaction request this epoch answers (M4.2).
        #[serde(default)]
        compaction_id: String,
        /// Session branch generation the epoch was computed under.
        #[serde(default)]
        branch_generation: u64,
        /// `ASYNC` (worker result installed at a boundary) or
        /// `SYNC_FALLBACK` (bounded compaction under hard pressure).
        #[serde(default)]
        mode: String,
        /// sha256 of the source entries the epoch summarised.
        #[serde(default)]
        source_digest: String,
        /// `MODEL` (a summary a model wrote and the Core validated) or
        /// `EXTRACTIVE`; empty on a record from before REQ-PX-109, which
        /// was always extractive.
        #[serde(default)]
        summary_source: String,
        /// `endpoint/model` of the summarizer, when a model wrote it.
        #[serde(default)]
        summarizer: String,
        /// Why the summary is extractive when the model path was meant or
        /// tried (`SUMMARIZER_TIMEOUT`, `SUMMARY_INVALID`, ...).
        #[serde(default)]
        fallback_reason: String,
        /// Object hash of the pre-compaction transcript this epoch replaced:
        /// `artifact.range` reads the exact text back.
        #[serde(default)]
        transcript_ref: String,
    },
    /// `CompactionStarted` (docs/19 "Compaction epochs", docs/30
    /// "Durability", M4.2): a compaction was started, asynchronously by a
    /// worker or synchronously under hard pressure; what it captured is on
    /// the log before its result can be. No state change.
    CompactionStarted {
        /// Request identity.
        compaction_id: String,
        /// The epoch it would open.
        epoch: u32,
        /// The previous epoch, when any.
        previous_epoch: Option<u32>,
        /// Session branch generation captured.
        branch_generation: u64,
        /// Store offset the source range ends at.
        source_head_offset: u64,
        /// Transcript entries in the source range.
        source_entries: u32,
        /// sha256 of the source entries.
        source_digest: String,
        /// Compiler version the projection is written for.
        compiler_version: String,
        /// Target token budget.
        target_tokens: u32,
        /// `ASYNC` | `SYNC_FALLBACK`.
        mode: String,
        /// The routed model's context window the trigger derived from
        /// (0 = unknown to the catalog).
        #[serde(default)]
        window_tokens: u32,
        /// The transcript budget in force when this compaction started.
        #[serde(default)]
        budget_tokens: u32,
        /// `MODEL_WINDOW` | `ENV_OVERRIDE` | `FALLBACK`.
        #[serde(default)]
        budget_source: String,
    },
    /// `CompactionCommitted` (docs/30 "Durability", M4.2): the durability
    /// record of an installed epoch, beside the model-facing
    /// `ContextEpochOpened` that carries the same manifest. No state change.
    CompactionCommitted {
        /// The request.
        compaction_id: String,
        /// The epoch it opened.
        epoch: u32,
        /// Session branch generation it was computed under.
        branch_generation: u64,
        /// Object hash of the manifest.
        manifest_ref: String,
        /// Hash of the manifest's fields.
        manifest_hash: String,
        /// `ASYNC` | `SYNC_FALLBACK`.
        mode: String,
        /// `MODEL` | `EXTRACTIVE` (REQ-PX-109).
        #[serde(default)]
        summary_source: String,
        /// Why the summary is extractive when the model path was meant or
        /// tried.
        #[serde(default)]
        fallback_reason: String,
    },
    /// `CompactionRejectedStale` (docs/19: a result is accepted only while
    /// its source is still current; docs/30 "Durability"; docs/54 fault 10):
    /// a compaction result arrived for a history that moved on and was
    /// refused, never installed. No state change.
    CompactionRejectedStale {
        /// The request.
        compaction_id: String,
        /// The epoch it would have opened.
        epoch: u32,
        /// `BRANCH_CHANGED` | `SOURCE_REWRITTEN` | `NOT_SUCCESSOR` |
        /// `GENERATION_CHANGED` | `SOURCE_ADVANCED` | `SUPERSEDED` |
        /// `WORKER_FAILED`.
        reason: String,
        /// Detail, in words.
        detail: String,
        /// Hash of the refused manifest, when one was produced.
        manifest_hash: String,
    },
    /// `UnsupportedLanguageOptInRecorded` (REQ-PX-029, docs/76 "Degradation
    /// path"): the user allowed this task to edit files in languages the
    /// product makes no claim about. Without it those edits are refused; with
    /// it they carry `unsupported_language` provenance. No state change.
    UnsupportedLanguageOptInRecorded {
        /// Language labels the user allowed (`go`, `java`, `unknown`, …).
        languages: Vec<String>,
        /// Why, in the user's words.
        reason: String,
    },
    /// `ContextDocumentAttached` (REQ-EV-0161, docs/18 "Connected engineering
    /// context"): an approved spec, issue or design document the user brought
    /// into the task. It enters retrieval as labelled, provenance-carrying
    /// context and nothing more: external text is data, and no sentence inside
    /// it can grant a tool, widen a lease or change a policy. No state change.
    ContextDocumentAttached {
        /// sha256 of the text.
        document_id: String,
        /// Where it came from, as the user named it (`issue:PROJ-1`, a URL).
        source: String,
        /// Human title.
        title: String,
        /// Object hash of the text.
        content_ref: String,
        /// Size of the text in bytes.
        byte_length: u64,
        /// Always `UNTRUSTED_EXTERNAL_CONTENT`: the label travels with it.
        trust: String,
    },
    /// `SelectionRecorded` (REQ-EV-0141 / 0160, docs/18 "Workspace context
    /// bridge"): what the user currently has selected — files, a line range, a
    /// symbol, review hunks — so retrieval can prefer it and every client can
    /// show it. Selection is context, never authority: it grants no tool and
    /// no write. No state change.
    SelectionRecorded {
        /// Root-relative paths.
        paths: Vec<String>,
        /// Symbol name, when the selection is a symbol.
        symbol: Option<String>,
        /// 1-based inclusive line range inside the first path.
        lines: Option<(u32, u32)>,
        /// Review hunks as `path#index`.
        review_hunks: Vec<String>,
        /// `review` | `editor` | `cli` | `desktop`.
        source: String,
    },
    /// `ReproductionRecorded` (docs/28 §5, PX-039): what a verification run
    /// said about the failure the goal reports, or the plan's recorded
    /// limitation when it could not be reproduced. No state change.
    ReproductionRecorded {
        /// `REPRODUCED` | `UNREPRODUCED` | `WAIVED`.
        status: String,
        /// The failing checks that reproduce it, when any.
        failing_checks: Vec<String>,
        /// Why, for `WAIVED`.
        note: String,
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
    /// `ProtectedPathsUnlocked` (docs/64 DI-9, REQ-EPR-008): the user
    /// answered `continue` to a typed question naming these protected
    /// paths, so the change engine lets this task change them. Recorded
    /// only from an answer the user gave, never the agent; a restarted Core
    /// rebuilds the unlock from it. No state change.
    ProtectedPathsUnlocked {
        /// The question the user answered.
        question_id: String,
        /// The paths the question named, and nothing else.
        paths: Vec<String>,
        /// The user's answer (the option id).
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
    /// `ContextPackRecorded` (FIX-12, audit N8): the task compiled a Context
    /// Pack with `context.pack`. It names the pack and the Context Ledger
    /// snapshot taken right after the pack entered the ledger (both in the
    /// object store), so a restarted Core rebuilds the ledger — the pack the
    /// prompt injects, the Inspector's pack view and the usefulness marks —
    /// from the log. Additive; no state change.
    ContextPackRecorded {
        /// The pack's id.
        pack_id: String,
        /// Object hash of the pack JSON.
        pack_ref: String,
        /// Object hash of the ledger snapshot (every pack so far, the direct
        /// reads and the usefulness marks at that moment).
        ledger_ref: String,
        /// Workspace revision the pack was compiled at.
        workspace_revision: u64,
        /// The `context.pack` call.
        tool_call_id: String,
        /// Estimated tokens the pack uses.
        token_used: u32,
        /// Entries packed.
        entries: u32,
        /// Signature-only stubs packed.
        stubs: u32,
        /// What started the pack (REQ-PX-108): empty for a `context.pack`
        /// call; `TASK_START` | `GOAL_CHANGE` | `COMPACTION` for the Core's
        /// own pre-turn step.
        #[serde(default)]
        trigger: String,
        /// `PACKED` (also what an older record without one means) |
        /// `EMPTY` | `DEGRADED`.
        #[serde(default)]
        status: String,
        /// The typed reason of an `EMPTY` or `DEGRADED` pre-turn pack.
        #[serde(default)]
        reason: String,
        /// The token budget the pack was compiled under.
        #[serde(default)]
        token_budget: u32,
        /// sha256 of the text the planner was seeded with.
        #[serde(default)]
        seed_digest: String,
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
    /// `CheckpointStarted` (docs/19 "Checkpoint epochs", docs/30
    /// "Durability", M4.3): a checkpoint with this epoch began capturing.
    /// The epoch is claimed here, so a slower writer with an older epoch is
    /// known to be stale when it commits. No state change.
    CheckpointStarted {
        /// Identity, as UUID text.
        checkpoint_id: String,
        /// Monotonic epoch.
        epoch: u32,
        /// `BASELINE` | `DELTA`.
        kind: String,
        /// The base a delta is relative to.
        base_checkpoint_id: Option<String>,
        /// Why (`before_completion`, `before_revert`, `requested`).
        reason: String,
    },
    /// `CheckpointCommitted` (REQ-EV-0012/0013): the checkpoint became
    /// current. The projection refuses this event when a newer epoch is
    /// already current, so a stale writer can never overwrite newer state.
    /// No state change.
    CheckpointCommitted {
        /// Identity.
        checkpoint_id: String,
        /// Epoch.
        epoch: u32,
        /// `BASELINE` | `DELTA`.
        kind: String,
        /// The base.
        base_checkpoint_id: Option<String>,
        /// Object hash of the manifest.
        manifest_ref: String,
        /// Manifest integrity hash.
        integrity_hash: String,
        /// Workspace revision captured.
        workspace_revision: u64,
        /// Git HEAD captured.
        git_head: Option<String>,
        /// Files the manifest names (baseline: all dirty; delta: changed).
        files: u32,
        /// Paths a delta removed.
        removed: u32,
        /// Store offset the runtime cursor points at.
        event_offset: u64,
        /// Retrieval index generation captured.
        index_generation: u64,
        /// The turn whose boundary this checkpoint is (UUID text); empty
        /// for a checkpoint not taken at a turn boundary (REQ-PX-061).
        #[serde(default)]
        turn_id: String,
        /// That turn's ordinal in its run (1-based); 0 when not a turn's.
        #[serde(default)]
        turn_ordinal: u32,
        /// What the capture cost: wall-clock milliseconds.
        #[serde(default)]
        capture_ms: u64,
        /// Dirty files the capture read and hashed.
        #[serde(default)]
        hashed_files: u32,
        /// Dirty files whose content was known from an earlier capture
        /// (size and mtime unchanged, not racy) and never read.
        #[serde(default)]
        cache_hits: u32,
        /// Content blobs the capture wrote (new content only).
        #[serde(default)]
        blobs_written: u32,
        /// Bytes those blobs hold.
        #[serde(default)]
        bytes_written: u64,
    },
    /// `CheckpointRejectedStale` (docs/19: a stale epoch can never overwrite
    /// newer checkpoint state; docs/54 fault 9). No state change.
    CheckpointRejectedStale {
        /// Identity.
        checkpoint_id: String,
        /// Its epoch.
        epoch: u32,
        /// The epoch that was current when it tried to commit.
        current_epoch: u32,
        /// Why, in words.
        reason: String,
    },
    /// `CheckpointRestored` (REQ-EV-0013): the worktree and the runtime
    /// cursor were restored from a validated chain. No state change.
    CheckpointRestored {
        /// The checkpoint restored to.
        checkpoint_id: String,
        /// Its epoch.
        epoch: u32,
        /// Manifests walked, as UUID text, oldest first.
        chain: Vec<String>,
        /// Files written from objects.
        files_written: u32,
        /// Dirty paths returned to HEAD or removed because the checkpoint
        /// did not have them.
        files_reverted: u32,
        /// Workspace revision after the restore.
        workspace_revision_after: u64,
        /// Store offset the restored runtime cursor points at.
        event_offset: u64,
        /// Optimistic preconditions the caller supplied and the restore
        /// checked before writing (REQ-EV-0123): path and the content hash
        /// the caller last saw.
        #[serde(default)]
        preconditions_checked: u32,
        /// The checkpoint recorded just before the restore changed anything:
        /// restoring it is the exact inverse ("redo", REQ-PX-061). Empty for
        /// a restore recorded before that existed.
        #[serde(default)]
        pre_restore_checkpoint_id: String,
        /// The `RestoreCheckpoint` command (UUID text); a replay of it
        /// returns this record and writes nothing.
        #[serde(default)]
        command_id: String,
        /// This restore was a redo of an earlier one.
        #[serde(default)]
        redo: bool,
    },
    /// `CheckpointRestoreStarted` (REQ-PX-061): the intent journal of a
    /// restore. Written after the pre-restore checkpoint is committed and
    /// before the first byte of the worktree changes, so a Core that dies
    /// between the two finds an in-doubt restore and rolls it back to the
    /// pre-restore checkpoint. No state change.
    CheckpointRestoreStarted {
        /// The `RestoreCheckpoint` command (UUID text).
        command_id: String,
        /// The pre-restore checkpoint (UUID text).
        pre_restore_checkpoint_id: String,
        /// The checkpoint being restored to (UUID text).
        target_checkpoint_id: String,
    },
    /// `CheckpointRestoreRolledBack` (REQ-PX-061): an in-doubt restore was
    /// resolved by restoring the pre-restore checkpoint; the worktree is
    /// exactly what it was before the restore began. No state change.
    CheckpointRestoreRolledBack {
        /// The `RestoreCheckpoint` command (UUID text).
        command_id: String,
        /// The checkpoint that was restored to restore the old state.
        pre_restore_checkpoint_id: String,
        /// The target the abandoned restore was aiming at.
        target_checkpoint_id: String,
        /// Why (`CORE_RESTARTED_MID_RESTORE`).
        reason: String,
    },
    /// `CheckpointNamed` (REQ-PX-102): a person labelled a checkpoint. The
    /// name is unique within the task. No state change.
    CheckpointNamed {
        /// The checkpoint (UUID text).
        checkpoint_id: String,
        /// The label.
        name: String,
    },
    /// `CheckpointSkipped` (REQ-PX-061): a turn boundary left no checkpoint,
    /// and why — the worktree was beyond the capture bounds, or is not a
    /// repository. A fork at that turn is refused with the same reason. No
    /// state change.
    CheckpointSkipped {
        /// The turn (UUID text).
        turn_id: String,
        /// The turn's ordinal in its run.
        turn_ordinal: u32,
        /// Stable code: `OVER_BOUNDS` | `CAPTURE_FAILED`.
        code: String,
        /// Detail.
        detail: String,
    },
    /// `CheckpointGcStarted` (REQ-PX-102): the retention collector took its
    /// lease on this task and decided what to remove. The intent half of a
    /// two-phase effect. No state change.
    CheckpointGcStarted {
        /// The run of the collector (UUID text).
        gc_id: String,
        /// Who holds the lease.
        owner: String,
        /// Why it runs.
        reason: String,
        /// The policy in force, as JSON.
        policy_json: String,
        /// Checkpoints it will remove (UUID text).
        candidates: Vec<String>,
        /// Checkpoints kept, each with why: `<id>:<NAMED|FORK_PARENT|LATEST|PRE_RESTORE|RESTORE_TARGET|CHAIN|RECENT|LIVE>`.
        kept: Vec<String>,
    },
    /// `CheckpointCollected` (REQ-PX-102): these checkpoints are removed
    /// from the task's restorable set, atomically, before any blob goes.
    /// No state change.
    CheckpointCollected {
        /// The collector run (UUID text).
        gc_id: String,
        /// The checkpoints removed (UUID text).
        checkpoint_ids: Vec<String>,
    },
    /// `CheckpointGcCompleted` (REQ-PX-102): what a collector run did. The
    /// result half of the effect. No state change.
    CheckpointGcCompleted {
        /// The collector run (UUID text).
        gc_id: String,
        /// Checkpoints looked at.
        scanned: u32,
        /// Checkpoints removed.
        removed: u32,
        /// Content blobs deleted.
        blobs_removed: u32,
        /// Bytes those blobs held.
        bytes_freed: u64,
        /// Blobs left because a surviving checkpoint (of any task) names them.
        blobs_retained: u32,
    },
    /// `TerminalCreated` (docs/30 "Workspace/execution", docs/19 protocol
    /// state "terminal session ID + last acknowledged output cursor"; M4.5):
    /// a background command got its durable handle. No state change.
    TerminalCreated {
        /// The broker's session id.
        handle_id: String,
        /// The client-stable request id (a retry replays, never restarts).
        request_id: String,
        /// Command.
        argv: Vec<String>,
        /// The terminal replay generation the Core attaches under.
        replay_generation: u64,
        /// The tool call that started it (UUID text).
        tool_call_id: String,
    },
    /// `TerminalOutputAdvanced`: the run read the handle's output up to a
    /// cursor — the last acknowledged cursor a resume continues from. No
    /// state change.
    TerminalOutputAdvanced {
        /// Handle.
        handle_id: String,
        /// Byte cursor after the bytes the run has seen.
        cursor: u64,
        /// Whether the process was still running at the read.
        running: bool,
    },
    /// `ProcessExited`: the handle's process ended (or was found gone). No
    /// state change.
    ProcessExited {
        /// Handle.
        handle_id: String,
        /// Exit code, when known.
        exit_code: Option<i32>,
        /// Signal, when known.
        signal: Option<i32>,
        /// Content-addressed full output.
        output_ref: String,
        /// Total output bytes.
        total_bytes: u64,
        /// Cancelled by the user or the run.
        cancelled: bool,
        /// Timed out.
        timed_out: bool,
    },
    /// `TerminalControlRecorded` (PX-043, PX-099): what a person did to a
    /// terminal beyond reading it — resized it, took or gave back its input
    /// lease, typed into it. The typed bytes are never recorded, only how
    /// many. No state change.
    TerminalControlRecorded {
        /// Handle.
        handle_id: String,
        /// `RESIZED` | `INPUT_LEASE_TAKEN` | `INPUT_LEASE_RELEASED` |
        /// `INPUT_WRITTEN`.
        kind: String,
        /// Who held the lease (`user:<client>`), for the lease kinds and
        /// input; empty for a resize.
        holder: String,
        /// The size, for a resize.
        rows: u32,
        /// The size, for a resize.
        cols: u32,
        /// The input's length, for `INPUT_WRITTEN`.
        bytes: u64,
    },
    /// `BackgroundProcessEnded` (REQ-PX-043, docs/65 AFW-E08): a background
    /// terminal of this task ended — it ran out, or a client killed it
    /// (`KillTerminal`) — and the Core recorded how, and by whom. This is the
    /// typed wake-up of the agent: the runtime reads it from the log at its
    /// next round boundary and tells the model once
    /// ([`TaskEvent::BackgroundWakeDelivered`]); it is not an input and not a
    /// message from anyone. Lands in any task state. No state change.
    BackgroundProcessEnded {
        /// Handle (the broker's session id).
        handle_id: String,
        /// `EXITED` | `KILLED` | `LOST`.
        how: String,
        /// Exit code, when known.
        exit_code: Option<i32>,
        /// Signal, when known.
        signal: Option<i32>,
        /// Content-addressed full output, once sealed.
        output_ref: String,
        /// Total output bytes.
        total_bytes: u64,
        /// Whether the broker's deadline ended it.
        timed_out: bool,
        /// Who ended it: `user:<id>` for a `KillTerminal`, `core` for an
        /// exit the watcher observed.
        ended_by: String,
        /// The killer's stated reason (log text, never an instruction).
        reason: String,
        /// Where the fact came from: `KILL_COMMAND` | `WATCHER`.
        source: String,
        /// The Capability Kernel's decision a kill was made under (the rule
        /// that allowed it, or `user-command: <why>` when the person's own
        /// command stood in for the approval the policy asked for).
        #[serde(default)]
        decision: String,
    },
    /// `BackgroundWakeDelivered` (REQ-PX-043): the run was told, at a round
    /// boundary, that a background terminal ended — once. When the run had
    /// already observed the end through one of its own terminal tools
    /// (`observed_by_agent`), nothing is added to the conversation and the
    /// record only closes the wake. No state change.
    BackgroundWakeDelivered {
        /// Handle.
        handle_id: String,
        /// Log offset of the `BackgroundProcessEnded` this answers.
        ended_offset: u64,
        /// The run had seen the exit itself (`ProcessExited`).
        observed_by_agent: bool,
        /// The run it was delivered to.
        run_id: Option<RunId>,
        /// The words the model was shown (empty when `observed_by_agent`).
        /// Kept so a rebuilt transcript is the one the model last saw.
        #[serde(default)]
        notice: String,
    },
    /// `ProtocolStateResumed` (docs/19 layer 2, REQ-EV-0055): a restarted
    /// Core reconstructed the task's protocol state and continued the run
    /// from the boundary it names; the outstanding calls are re-entered by
    /// their existing ids, never re-proposed. No state change.
    ProtocolStateResumed {
        /// Run that continued.
        run_id: RunId,
        /// Boundary continued from (`EXECUTING`, `AWAITING_APPROVAL`,
        /// `RECONCILING`, `TURN_START`).
        boundary: String,
        /// Outstanding tool calls re-entered, as UUID text.
        tool_call_ids: Vec<String>,
        /// sha256 of the canonical protocol state that was reconstructed.
        digest: String,
    },
    /// `ToolCallReconciled` (docs/19 resume step 6, REQ-EV-0055): a call
    /// whose outcome became unknown across a restart was reconciled against
    /// the effect ledger and the target before the run went on; no effect
    /// was replayed. No state change.
    ToolCallReconciled {
        /// The call, as UUID text.
        tool_call_id: String,
        /// Tool.
        tool_name: String,
        /// Effect class of the call.
        effect_class: String,
        /// `RECEIPT_FOUND`, `TARGET_INSPECTED`, `HELD_FOR_RECEIPT` or
        /// `REPLAY_SAFE`.
        resolution: String,
        /// What was observed (receipt hash, target content hash, ...).
        observed: String,
    },
    /// `UsageReconciled` (REQ-EPR-010, docs/38
    /// "CompleteAccountingAndAttribution" step 3, EPR-FI-010): the
    /// provider's own figures for an attempt of this request whose usage was
    /// unknown when it ended arrived later (a late invoice). The attempt is
    /// charged these figures instead of its held reservation, exactly once.
    /// An audit record about the request, valid in every state.
    UsageReconciled {
        /// The run the attempt belongs to.
        run_id: RunId,
        /// Plan.
        plan_id: String,
        /// Slot.
        slot_id: String,
        /// Attempt ordinal within the slot.
        attempt: u32,
        /// The provider's request id, when the attempt recorded one.
        provider_request_id: Option<String>,
        /// Total input tokens.
        input_tokens: u64,
        /// Cached subset.
        cached_input_tokens: u64,
        /// Cache-write subset.
        cache_write_tokens: u64,
        /// Output tokens.
        output_tokens: u64,
        /// Priced at the registry in force at reconciliation; `None` when no
        /// price was in force.
        cost_minor: Option<u64>,
        /// Registry generation of that price; empty when unpriced.
        priced_under: String,
        /// Who delivered the figures (an importer's name).
        source: String,
        /// Object hash of the invoice line as delivered.
        invoice_ref: String,
    },
    /// `PolicyGenerationChanged` (REQ-EV-0041, docs/23 "Policy generations"):
    /// at a model-round boundary the configuration in force resolved to a new
    /// generation. The next round is decided under it — a tool whose
    /// capability it now denies is withheld — while a call already in flight
    /// finished under the snapshot it was decided with. No state change.
    PolicyGenerationChanged {
        /// The generation the previous round ran under.
        from: String,
        /// The generation from this round on.
        to: String,
        /// Capabilities made stricter (a new ASK or DENY).
        tightened: Vec<String>,
        /// Capabilities made looser.
        loosened: Vec<String>,
        /// Tools the new generation withholds, by name.
        withheld_tools: Vec<String>,
    },
    /// `ProcessServiceObserved` (REQ-PX-132, docs/21): the Core found, or lost,
    /// or re-judged a listening service in the process tree of one of the
    /// task's terminals. Recorded on a change of state only, never per probe.
    /// Observation changes no policy: a detected server is a fact the agent
    /// and the person are told, not a permission. No state change.
    ProcessServiceObserved {
        /// The terminal (broker session) whose process tree holds the socket.
        handle_id: String,
        /// The listening port.
        port: u32,
        /// The bound address.
        address: String,
        /// The listening process.
        pid: u32,
        /// Its command line: redacted and bounded.
        command: String,
        /// `dev_server` | `service`.
        kind: String,
        /// `STARTING` | `READY` | `UNHEALTHY` | `GONE`.
        state: String,
        /// The state it left (empty for the first observation).
        previous: String,
        /// The HTTP status the probe got; 0 = no HTTP answer.
        http_status: u32,
        /// How long the probe took.
        probe_ms: u64,
        /// Why the service is in this state.
        detail: String,
        /// True when the Core observed a service it had already recorded
        /// before it restarted.
        reobserved: bool,
    },
    /// `CapabilitySnapshotRecorded` (REQ-PX-131, docs/23 "Policy
    /// generations"): at a model-round boundary the Core froze the round's
    /// capability view — the projected tools, the policy generation, the
    /// lease and the mode posture — under the next `AuthorizationEpoch`.
    /// Every kernel decision and receipt of the round carries the epoch and
    /// is decided against this snapshot; a policy change, a revoked skill or
    /// a mode switch made during the round takes effect at the next one. A
    /// restarted Core finds the snapshot here and decides the rest of an
    /// interrupted round under it. No state change.
    CapabilitySnapshotRecorded {
        /// The frozen view.
        snapshot: crate::epoch::CapabilitySnapshot,
        /// SHA-256 (hex) of the canonical snapshot; what the round's
        /// decisions and receipts name.
        snapshot_hash: String,
    },
    /// `ReviewCommentsIngested` (PX-008, docs/29 "Review-comment
    /// steering"): the task's pull-request comments read from the forge —
    /// those from an identity the organization allows and addressed to
    /// Modbit queued as untrusted steering (`TaskInputQueued`, provenance
    /// `forge_review_comment`), every other one recorded with why it was
    /// ignored. A comment is taken once. An audit record about the task,
    /// valid in every state.
    ReviewCommentsIngested {
        /// Owner.
        owner: String,
        /// Repository.
        repo: String,
        /// Pull request number.
        pull_number: u64,
        /// Comments queued as steering.
        steered: Vec<ReviewCommentRecord>,
        /// Comments ignored, with the reason.
        ignored: Vec<ReviewCommentRecord>,
        /// The `forge.pr.comments.read` call that read them.
        tool_call_id: String,
    },
    /// `CiEvidenceRecorded` (PX-009, docs/29 "CI evidence", REQ-EV-0010):
    /// the forge's check runs for the commit the task's pull request carries,
    /// recorded as external evidence with provenance `ci` — never a
    /// verification result. Runs that named another commit are listed as
    /// refused. An audit record about the task, valid in every state.
    CiEvidenceRecorded {
        /// Forge (`github`).
        provider: String,
        /// Owner.
        owner: String,
        /// Repository.
        repo: String,
        /// Pull request number.
        pull_number: u64,
        /// The commit the Core pushed for the pull request.
        commit: String,
        /// Check runs accepted as evidence.
        checks: Vec<CiCheckRecord>,
        /// Check runs refused.
        rejected: Vec<CiRejectedRecord>,
        /// The `forge.ci.status` call that read them.
        tool_call_id: String,
        /// `ci`.
        provenance: String,
    },
    /// `RequestOutcomeRecorded` (REQ-EPR-010, docs/27 §11.2-11.3, docs/38
    /// "CompleteAccountingAndAttribution" step 4): the request's accounting
    /// and outcome record as of this point — every leg and attempt with its
    /// cost, the versions it ran under, request, leg and gate observations
    /// kept apart, raw signals by reference and the missing ones named. The
    /// record is the object; the event pins it and carries the headline. A
    /// later record supersedes an earlier one; records are never summed. An
    /// audit record about the request, valid in every state.
    RequestOutcomeRecorded {
        /// Object hash of the full record.
        record_ref: String,
        /// Accounting rules version.
        accounting_version: String,
        /// `RUN_END` | `REVIEW_CONCLUDED` | `REVIEW_DECIDED` |
        /// `USAGE_RECONCILED`.
        trigger: String,
        /// `pass` | `fail` | `partial` | `cancelled` | `open`.
        final_outcome: String,
        /// Whether the request is verified.
        verified_success: bool,
        /// Verified with no repair, escalation or revision.
        first_pass_success: bool,
        /// Whether the initial leg succeeded; `None` while undecided.
        initial_leg_success: Option<bool>,
        /// Everything charged, minor units (known spend plus held unknowns).
        total_minor: u64,
        /// Of which held for work whose usage is unknown.
        unknown_minor: u64,
        /// Currency.
        currency: String,
        /// Scale.
        scale: u8,
        /// Signals the record could not observe, by name.
        missing_signals: Vec<String>,
    },
    /// `RequestSnapshotRecorded` (EPR-011, docs/38 "CounterfactualReplay"
    /// step 1): the immutable repository revision the request started from
    /// — the worktree, dirty state included, as a commit under
    /// `refs/modbit/snapshots/<id>`; HEAD, the index and the files are
    /// untouched. No state change.
    RequestSnapshotRecorded {
        /// The ref.
        snapshot_ref: String,
        /// The snapshot commit.
        commit: String,
        /// Its tree.
        tree: String,
        /// HEAD it was taken over.
        parent: String,
        /// Paths that were dirty (tracked changes and untracked files).
        dirty_paths: Vec<String>,
    },
    /// `CounterfactualReplayStarted` (EPR-011): an alternative validated plan
    /// of this request is being replayed offline — its snapshot in a scratch
    /// repository, sanitized, under the replay-only ceiling, as its own task.
    /// Its outcome is the replay task's, read by the request's record as an
    /// observed counterfactual. An audit record, valid in every state.
    CounterfactualReplayStarted {
        /// Replay id.
        replay_id: String,
        /// The replay's own task.
        replay_task_id: TaskId,
        /// The alternative plan (from the request's decision record).
        plan_id: String,
        /// Its bindings, in order.
        bindings: Vec<String>,
        /// The snapshot commit replayed.
        snapshot_commit: String,
        /// The scratch repository.
        scratch: String,
        /// Credential-bearing files removed from the scratch copy.
        sanitized: Vec<String>,
        /// The registry generation the decision and the replay share.
        registry_generation: String,
        /// The statistics version the decision was compiled under.
        stats_version: String,
        /// The ceiling the replay ran under (execution profile).
        capability_ceiling: String,
    },
    /// `HooksResolved` (REQ-EV-0042, REQ-EV-0240): the hooks in force for a
    /// run as it starts — from the configuration layers and the session's
    /// extensions — and the declarations refused, with why. No state change.
    HooksResolved {
        /// Active registrations: `<source>/<name>@<point>`.
        active: Vec<String>,
        /// Declarations refused.
        refused: Vec<HookRefusal>,
    },
    /// `HookInvoked` (REQ-EV-0042, REQ-EV-0139): one handler invocation at a
    /// typed lifecycle point, what it answered and whether that changed what
    /// happened. An audit record, valid in every state (an `after_run` hook
    /// runs once the run has ended).
    HookInvoked {
        /// Registration id (`<source>/<name>`).
        hook: String,
        /// `config:<authority>` | `extension:<name>`.
        source: String,
        /// The point (`before_tool`, `after_run`, …).
        point: String,
        /// `observe` | `intercept`.
        mode: String,
        /// `closed` | `open`.
        fail_policy: String,
        /// `OK` | `DENIED` | `MUTATED` | `TIMEOUT` | `FAILED` | `MALFORMED` |
        /// `IGNORED` | `UNLOADED`.
        outcome: String,
        /// Whether it changed what happened (a denial, a fail-closed stop, a
        /// rewrite that was used).
        applied: bool,
        /// Wall time, milliseconds.
        duration_ms: u64,
        /// The handler's reason, or what went wrong.
        detail: String,
        /// The tool, at a tool or change point.
        tool: Option<String>,
        /// Hash of the rewritten arguments, when a rewrite was used.
        arguments_hash: Option<String>,
        /// Object holding the rewritten arguments, when a rewrite was used.
        arguments_ref: Option<String>,
        /// PX-117: what became of the context the handler offered: empty
        /// when none, `INJECTED`, or `DROPPED:<REASON>`.
        #[serde(default)]
        context_status: String,
        /// PX-117: bytes of context offered.
        #[serde(default)]
        context_bytes: u64,
        /// PX-117: `endpoint/model` a prompt hook ran on.
        #[serde(default)]
        model: Option<String>,
        /// PX-117: prompt tokens a prompt hook spent.
        #[serde(default)]
        input_tokens: u64,
        /// PX-117: completion tokens a prompt hook spent.
        #[serde(default)]
        output_tokens: u64,
    },
}

/// A hook declaration that is not in force, and why (REQ-EV-0042).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HookRefusal {
    /// The declaration's name, or its text when it has none.
    pub hook: String,
    /// Where it was declared.
    pub source: String,
    /// Why it is not in force.
    pub reason: String,
}

/// One pull-request comment as ingestion saw it (PX-008): who, where,
/// and what became of it — never its text, which is the input's.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReviewCommentRecord {
    /// The forge's comment id.
    pub comment_id: u64,
    /// `review` (on a line) | `issue` (on the conversation).
    pub kind: String,
    /// The forge login that wrote it.
    pub author: String,
    /// Where a person reads it.
    pub url: String,
    /// The queued input's id, when steered.
    pub input_id: String,
    /// `DISALLOWED_AUTHOR` | `NOT_ADDRESSED` | `EMPTY`, when ignored.
    pub reason: String,
    /// The file a line comment is on (PX-127; empty on the conversation and
    /// on records made before it).
    #[serde(default)]
    pub path: String,
    /// The line a line comment is on (0 when none).
    #[serde(default)]
    pub line: u64,
    /// The forge's timestamp of the comment (PX-127).
    #[serde(default)]
    pub created_at: String,
}

/// One check run recorded as CI evidence (PX-009).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CiCheckRecord {
    /// Check name.
    pub name: String,
    /// The forge's run id.
    pub run_id: u64,
    /// Status.
    pub status: String,
    /// Conclusion; empty while not completed.
    pub conclusion: String,
    /// Where a person reads it.
    pub url: String,
    /// When it completed.
    pub completed_at: String,
    /// Object hash of the run's own output (its log), readable by range;
    /// empty when it had none.
    pub log_ref: String,
    /// Whether the log was cut.
    pub log_truncated: bool,
}

/// One check run refused as CI evidence (PX-009).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CiRejectedRecord {
    /// Check name.
    pub name: String,
    /// The commit it named.
    pub head_sha: String,
    /// `MISMATCHED_COMMIT` | `MALFORMED`.
    pub reason: String,
}

impl TaskEvent {
    /// Canonical event type name.
    #[must_use]
    pub const fn event_type(&self) -> &'static str {
        match self {
            Self::TaskCreated { .. } => "TaskCreated",
            Self::TaskQueued => "TaskQueued",
            Self::TaskForked { .. } => "TaskForked",
            Self::TaskStarted => "TaskStarted",
            Self::TaskWaiting { .. } => "TaskWaiting",
            Self::TaskResumed => "TaskResumed",
            Self::TaskReadyForReview => "TaskReadyForReview",
            Self::TaskReturnedToWork => "TaskReturnedToWork",
            Self::TaskCompleted => "TaskCompleted",
            Self::TaskFailed { .. } => "TaskFailed",
            Self::TaskCancelled => "TaskCancelled",
            Self::TaskSteered { .. } => "TaskSteered",
            Self::TaskPauseRequested { .. } => "TaskPauseRequested",
            Self::TaskResumeRequested { .. } => "TaskResumeRequested",
            Self::TaskPaused { .. } => "TaskPaused",
            Self::TaskCancelRequested { .. } => "TaskCancelRequested",
            Self::TaskNeedsAttention { .. } => "TaskNeedsAttention",
            Self::TaskInputQueued { .. } => "TaskInputQueued",
            Self::SendBehaviorSet { .. } => "SendBehaviorSet",
            Self::TaskInputEdited { .. } => "TaskInputEdited",
            Self::TaskInputRemoved { .. } => "TaskInputRemoved",
            Self::TaskInputReordered { .. } => "TaskInputReordered",
            Self::TaskInterruptRequested { .. } => "TaskInterruptRequested",
            Self::TaskInterruptApplied { .. } => "TaskInterruptApplied",
            Self::RunModeSet { .. } => "RunModeSet",
            Self::AllowRuleAdded { .. } => "AllowRuleAdded",
            Self::AllowRuleRevoked { .. } => "AllowRuleRevoked",
            Self::UserQuestionAsked { .. } => "UserQuestionAsked",
            Self::UserQuestionAnswered { .. } => "UserQuestionAnswered",
            Self::AttachmentIngested { .. } => "AttachmentIngested",
            Self::TaskModeSet { .. } => "TaskModeSet",
            Self::TaskPostureApplied { .. } => "TaskPostureApplied",
            Self::ExecutionPreferenceSet { .. } => "ExecutionPreferenceSet",
            Self::ExecutionPreferenceApplied { .. } => "ExecutionPreferenceApplied",
            Self::PlanRecorded { .. } => "PlanRecorded",
            Self::PlanRevised { .. } => "PlanRevised",
            Self::PlanAnnotated { .. } => "PlanAnnotated",
            Self::UserPatchApplied { .. } => "UserPatchApplied",
            Self::ExternalDiagnosticsRecorded { .. } => "ExternalDiagnosticsRecorded",
            Self::ExternalDiagnosticsRejected { .. } => "ExternalDiagnosticsRejected",
            Self::ForgePullRequestOpened { .. } => "ForgePullRequestOpened",
            Self::ForgePullRequestUpdated { .. } => "ForgePullRequestUpdated",
            Self::ForgeCommentPosted { .. } => "ForgeCommentPosted",
            Self::TaskCreatedFromIssue { .. } => "TaskCreatedFromIssue",
            Self::TaskTriggeredByAutomation { .. } => "TaskTriggeredByAutomation",
            Self::BrowserSessionOpened { .. } => "BrowserSessionOpened",
            Self::BrowserHostAttached { .. } => "BrowserHostAttached",
            Self::BrowserNavigated { .. } => "BrowserNavigated",
            Self::BrowserSessionClosed { .. } => "BrowserSessionClosed",
            Self::BrowserControlChanged { .. } => "BrowserControlChanged",
            Self::BrowserPageObserved { .. } => "BrowserPageObserved",
            Self::BrowserActionPerformed { .. } => "BrowserActionPerformed",
            Self::BrowserRegionCaptured { .. } => "BrowserRegionCaptured",
            Self::BrowserPageChanged { .. } => "BrowserPageChanged",
            Self::BrowserOutcomeUnknown { .. } => "BrowserOutcomeUnknown",
            Self::BrowserOutcomeReconciled { .. } => "BrowserOutcomeReconciled",
            Self::ComputerSessionStarted { .. } => "ComputerSessionStarted",
            Self::ComputerObserved { .. } => "ComputerObserved",
            Self::ComputerActionPerformed { .. } => "ComputerActionPerformed",
            Self::ComputerLatched { .. } => "ComputerLatched",
            Self::ComputerLatchResolved { .. } => "ComputerLatchResolved",
            Self::ComputerSessionClosed { .. } => "ComputerSessionClosed",
            Self::ComputerUserAborted { .. } => "ComputerUserAborted",
            Self::ComputerArtifactExpired { .. } => "ComputerArtifactExpired",
            Self::SecurityEventRecorded { .. } => "SecurityEventRecorded",
            Self::SloStageRecorded { .. } => "SloStageRecorded",
            Self::BrowserCredentialFilled { .. } => "BrowserCredentialFilled",
            Self::SandboxLeaseAcquired { .. } => "SandboxLeaseAcquired",
            Self::TaskHandedOff { .. } => "TaskHandedOff",
            Self::TaskHandoffAdmitted { .. } => "TaskHandoffAdmitted",
            Self::TaskWorkspaceRebound { .. } => "TaskWorkspaceRebound",
            Self::SandboxReleased { .. } => "SandboxReleased",
            Self::SandboxLost { .. } => "SandboxLost",
            Self::SandboxRestored { .. } => "SandboxRestored",
            Self::EnvironmentPinned { .. } => "EnvironmentPinned",
            Self::EnvironmentStale { .. } => "EnvironmentStale",
            Self::EnvironmentRebuilt { .. } => "EnvironmentRebuilt",
            Self::UnsupportedLanguageOptInRecorded { .. } => "UnsupportedLanguageOptInRecorded",
            Self::ContextDocumentAttached { .. } => "ContextDocumentAttached",
            Self::SelectionRecorded { .. } => "SelectionRecorded",
            Self::SelfReviewRecorded { .. } => "SelfReviewRecorded",
            Self::ToolsActivated { .. } => "ToolsActivated",
            Self::ToolProjectionConfigured { .. } => "ToolProjectionConfigured",
            Self::ProgramStarted { .. } => "ProgramStarted",
            Self::ProgramEnded { .. } => "ProgramEnded",
            Self::SkillSelected { .. } => "SkillSelected",
            Self::SkillRejected { .. } => "SkillRejected",
            Self::SkillIndexRecorded { .. } => "SkillIndexRecorded",
            Self::RulesSelected { .. } => "RulesSelected",
            Self::AgentNodeCreated { .. } => "AgentNodeCreated",
            Self::AgentNodeTransitioned { .. } => "AgentNodeTransitioned",
            Self::AgentBindingChanged { .. } => "AgentBindingChanged",
            Self::SubagentAdmitted { .. } => "SubagentAdmitted",
            Self::TaskBudgetsSet { .. } => "TaskBudgetsSet",
            Self::SubagentAdmissionRefused { .. } => "SubagentAdmissionRefused",
            Self::SubagentCapsuleBound { .. } => "SubagentCapsuleBound",
            Self::SubagentResultRecorded { .. } => "SubagentResultRecorded",
            Self::SubagentProtectedEffect { .. } => "SubagentProtectedEffect",
            Self::ReviewEnvironmentAdmitted { .. } => "ReviewEnvironmentAdmitted",
            Self::ReviewEnvironmentBound { .. } => "ReviewEnvironmentBound",
            Self::ReviewerResultRecorded { .. } => "ReviewerResultRecorded",
            Self::RevisionActivated { .. } => "RevisionActivated",
            Self::ReviewEnvironmentDisposed { .. } => "ReviewEnvironmentDisposed",
            Self::WorkNodesChanged { .. } => "WorkNodesChanged",
            Self::CapacityTicketGranted { .. } => "CapacityTicketGranted",
            Self::CapacityTicketReleased { .. } => "CapacityTicketReleased",
            Self::CapacityDenied { .. } => "CapacityDenied",
            Self::MediaBridged { .. } => "MediaBridged",
            Self::ContextEpochOpened { .. } => "ContextEpochOpened",
            Self::CompactionStarted { .. } => "CompactionStarted",
            Self::CompactionCommitted { .. } => "CompactionCommitted",
            Self::CompactionRejectedStale { .. } => "CompactionRejectedStale",
            Self::ReproductionRecorded { .. } => "ReproductionRecorded",
            Self::ScopeExpansionRecorded { .. } => "ScopeExpansionRecorded",
            Self::ProtectedPathsUnlocked { .. } => "ProtectedPathsUnlocked",
            Self::RetrievalRecorded { .. } => "RetrievalRecorded",
            Self::ContextPackRecorded { .. } => "ContextPackRecorded",
            Self::RepairAttemptRecorded { .. } => "RepairAttemptRecorded",
            Self::RepairAttemptConcluded { .. } => "RepairAttemptConcluded",
            Self::RepairEscalated { .. } => "RepairEscalated",
            Self::HarnessBudgetExhausted { .. } => "HarnessBudgetExhausted",
            Self::NoProgressDetected { .. } => "NoProgressDetected",
            Self::ReviewDecisionRecorded { .. } => "ReviewDecisionRecorded",
            Self::CheckpointStarted { .. } => "CheckpointStarted",
            Self::CheckpointCommitted { .. } => "CheckpointCommitted",
            Self::CheckpointRejectedStale { .. } => "CheckpointRejectedStale",
            Self::CheckpointRestored { .. } => "CheckpointRestored",
            Self::CheckpointRestoreStarted { .. } => "CheckpointRestoreStarted",
            Self::CheckpointRestoreRolledBack { .. } => "CheckpointRestoreRolledBack",
            Self::CheckpointNamed { .. } => "CheckpointNamed",
            Self::CheckpointSkipped { .. } => "CheckpointSkipped",
            Self::CheckpointGcStarted { .. } => "CheckpointGcStarted",
            Self::CheckpointCollected { .. } => "CheckpointCollected",
            Self::CheckpointGcCompleted { .. } => "CheckpointGcCompleted",
            Self::TerminalCreated { .. } => "TerminalCreated",
            Self::TerminalOutputAdvanced { .. } => "TerminalOutputAdvanced",
            Self::ProcessExited { .. } => "ProcessExited",
            Self::TerminalControlRecorded { .. } => "TerminalControlRecorded",
            Self::BackgroundProcessEnded { .. } => "BackgroundProcessEnded",
            Self::BackgroundWakeDelivered { .. } => "BackgroundWakeDelivered",
            Self::PausedCapacityReleased { .. } => "PausedCapacityReleased",
            Self::ProtocolStateResumed { .. } => "ProtocolStateResumed",
            Self::ToolCallReconciled { .. } => "ToolCallReconciled",
            Self::UsageReconciled { .. } => "UsageReconciled",
            Self::CiEvidenceRecorded { .. } => "CiEvidenceRecorded",
            Self::ReviewCommentsIngested { .. } => "ReviewCommentsIngested",
            Self::PolicyGenerationChanged { .. } => "PolicyGenerationChanged",
            Self::CapabilitySnapshotRecorded { .. } => "CapabilitySnapshotRecorded",
            Self::ProcessServiceObserved { .. } => "ProcessServiceObserved",
            Self::RequestOutcomeRecorded { .. } => "RequestOutcomeRecorded",
            Self::HooksResolved { .. } => "HooksResolved",
            Self::HookInvoked { .. } => "HookInvoked",
            Self::RequestSnapshotRecorded { .. } => "RequestSnapshotRecorded",
            Self::CounterfactualReplayStarted { .. } => "CounterfactualReplayStarted",
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
            | TaskEvent::SendBehaviorSet { .. }
            | TaskEvent::TaskInputEdited { .. }
            | TaskEvent::TaskInputRemoved { .. }
            | TaskEvent::TaskInputReordered { .. }
            | TaskEvent::TaskInterruptRequested { .. }
            | TaskEvent::TaskInterruptApplied { .. }
            | TaskEvent::UserQuestionAsked { .. }
            | TaskEvent::UserQuestionAnswered { .. }
            | TaskEvent::AttachmentIngested { .. }
            | TaskEvent::TaskModeSet { .. }
            | TaskEvent::TaskPostureApplied { .. }
            | TaskEvent::ExecutionPreferenceSet { .. }
            | TaskEvent::ExecutionPreferenceApplied { .. }
            | TaskEvent::PlanRecorded { .. }
            | TaskEvent::PlanRevised { .. }
            | TaskEvent::PlanAnnotated { .. }
            | TaskEvent::UserPatchApplied { .. }
            | TaskEvent::ExternalDiagnosticsRecorded { .. }
            | TaskEvent::ExternalDiagnosticsRejected { .. }
            | TaskEvent::ForgePullRequestOpened { .. }
            | TaskEvent::ForgePullRequestUpdated { .. }
            | TaskEvent::ForgeCommentPosted { .. }
            | TaskEvent::TaskCreatedFromIssue { .. }
            | TaskEvent::TaskTriggeredByAutomation { .. }
            | TaskEvent::BrowserSessionOpened { .. }
            | TaskEvent::BrowserHostAttached { .. }
            | TaskEvent::BrowserNavigated { .. }
            | TaskEvent::BrowserSessionClosed { .. }
            | TaskEvent::BrowserControlChanged { .. }
            | TaskEvent::BrowserPageObserved { .. }
            | TaskEvent::BrowserActionPerformed { .. }
            | TaskEvent::BrowserRegionCaptured { .. }
            | TaskEvent::BrowserPageChanged { .. }
            | TaskEvent::BrowserOutcomeUnknown { .. }
            | TaskEvent::BrowserOutcomeReconciled { .. }
            | TaskEvent::SecurityEventRecorded { .. }
            | TaskEvent::SloStageRecorded { .. }
            | TaskEvent::TaskPauseRequested { .. }
            | TaskEvent::TaskResumeRequested { .. }
            | TaskEvent::TaskPaused { .. }
            | TaskEvent::TaskCancelRequested { .. }
            | TaskEvent::BrowserCredentialFilled { .. }
            | TaskEvent::SandboxLeaseAcquired { .. }
            | TaskEvent::TaskHandedOff { .. }
            | TaskEvent::TaskHandoffAdmitted { .. }
            | TaskEvent::SelfReviewRecorded { .. }
            | TaskEvent::ToolsActivated { .. }
            | TaskEvent::ToolProjectionConfigured { .. }
            | TaskEvent::ProgramStarted { .. }
            | TaskEvent::ProgramEnded { .. }
            | TaskEvent::SkillSelected { .. }
            | TaskEvent::SkillRejected { .. }
            | TaskEvent::SkillIndexRecorded { .. }
            | TaskEvent::RulesSelected { .. }
            | TaskEvent::MediaBridged { .. }
            | TaskEvent::CapacityTicketGranted { .. }
            | TaskEvent::CapacityTicketReleased { .. }
            | TaskEvent::CapacityDenied { .. }
            | TaskEvent::AgentNodeCreated { .. }
            | TaskEvent::AgentNodeTransitioned { .. }
            | TaskEvent::AgentBindingChanged { .. }
            | TaskEvent::WorkNodesChanged { .. }
            | TaskEvent::SubagentAdmitted { .. }
            | TaskEvent::TaskBudgetsSet { .. }
            | TaskEvent::SubagentAdmissionRefused { .. }
            | TaskEvent::SubagentCapsuleBound { .. }
            | TaskEvent::SubagentResultRecorded { .. }
            | TaskEvent::SubagentProtectedEffect { .. }
            | TaskEvent::ReviewEnvironmentAdmitted { .. }
            | TaskEvent::ReviewEnvironmentBound { .. }
            | TaskEvent::ReviewerResultRecorded { .. }
            | TaskEvent::RevisionActivated { .. }
            | TaskEvent::ReviewEnvironmentDisposed { .. }
            | TaskEvent::ContextEpochOpened { .. }
            | TaskEvent::CompactionStarted { .. }
            | TaskEvent::CompactionCommitted { .. }
            | TaskEvent::CompactionRejectedStale { .. }
            | TaskEvent::ReproductionRecorded { .. }
            | TaskEvent::SelectionRecorded { .. }
            | TaskEvent::ContextDocumentAttached { .. }
            | TaskEvent::UnsupportedLanguageOptInRecorded { .. }
            | TaskEvent::ScopeExpansionRecorded { .. }
            | TaskEvent::ProtectedPathsUnlocked { .. }
            | TaskEvent::RetrievalRecorded { .. }
            | TaskEvent::ContextPackRecorded { .. }
            | TaskEvent::RepairAttemptRecorded { .. }
            | TaskEvent::RepairAttemptConcluded { .. }
            | TaskEvent::RepairEscalated { .. }
            | TaskEvent::HarnessBudgetExhausted { .. }
            | TaskEvent::NoProgressDetected { .. }
            | TaskEvent::ReviewDecisionRecorded { .. }
            | TaskEvent::TaskForked { .. }
            | TaskEvent::TerminalCreated { .. }
            | TaskEvent::TerminalOutputAdvanced { .. }
            | TaskEvent::ProcessExited { .. }
            | TaskEvent::TerminalControlRecorded { .. }
            | TaskEvent::ProtocolStateResumed { .. }
            | TaskEvent::ToolCallReconciled { .. }
            | TaskEvent::PolicyGenerationChanged { .. }
            | TaskEvent::CapabilitySnapshotRecorded { .. } => {
                if self.state.is_terminal() {
                    return Err(invalid(&self.state, event.event_type()));
                }
                None
            }
            // The checkpoint ledger (REQ-PX-061/102): a finished task is
            // exactly the one whose checkpoints are restored, forked from,
            // named and collected, so these records land in any state.
            TaskEvent::CheckpointStarted { .. }
            | TaskEvent::CheckpointCommitted { .. }
            | TaskEvent::CheckpointRejectedStale { .. }
            | TaskEvent::CheckpointRestored { .. }
            | TaskEvent::CheckpointRestoreStarted { .. }
            | TaskEvent::CheckpointRestoreRolledBack { .. }
            | TaskEvent::CheckpointNamed { .. }
            | TaskEvent::CheckpointSkipped { .. }
            | TaskEvent::CheckpointGcStarted { .. }
            | TaskEvent::CheckpointCollected { .. }
            | TaskEvent::CheckpointGcCompleted { .. }
            // A background process ends, and a paused task's held capacity
            // lapses, whatever state the task is in by then.
            | TaskEvent::BackgroundProcessEnded { .. }
            | TaskEvent::BackgroundWakeDelivered { .. }
            | TaskEvent::PausedCapacityReleased { .. } => None,
            // A policy record (PX-057): the run mode and the allowlist rules
            // are the person's, and a rule is revoked whatever state the task
            // that holds its record is in.
            TaskEvent::RunModeSet { .. }
            | TaskEvent::AllowRuleAdded { .. }
            | TaskEvent::AllowRuleRevoked { .. } => None,
            // Native control (PX-069): a session closes, a latch is reconciled
            // and a stop is recorded whatever state the task is in by then
            // (a run ends, and its control sessions with it).
            TaskEvent::ComputerSessionStarted { .. }
            | TaskEvent::ComputerObserved { .. }
            | TaskEvent::ComputerActionPerformed { .. }
            | TaskEvent::ComputerLatched { .. }
            | TaskEvent::ComputerLatchResolved { .. }
            | TaskEvent::ComputerSessionClosed { .. }
            | TaskEvent::ComputerUserAborted { .. }
            | TaskEvent::ComputerArtifactExpired { .. } => None,
            // A service the task's terminal started is lost when the task
            // ends (its terminals are killed): the record says so whatever
            // state the task is in (REQ-PX-132).
            TaskEvent::ProcessServiceObserved { .. } => None,
            // A sandbox is given back after the task ended (M8.5), and one
            // may be lost at any time: the records of the substrate's
            // lifecycle land whatever the task's state. So do the request's
            // accounting records (REQ-EPR-010): a review concludes and a
            // late invoice arrives after the request's own run is over; and
            // CI results (PX-009), which finish on the forge's schedule.
            TaskEvent::SandboxReleased { .. }
            | TaskEvent::CiEvidenceRecorded { .. }
            | TaskEvent::ReviewCommentsIngested { .. }
            | TaskEvent::UsageReconciled { .. }
            | TaskEvent::RequestOutcomeRecorded { .. }
            | TaskEvent::HooksResolved { .. }
            | TaskEvent::HookInvoked { .. }
            | TaskEvent::RequestSnapshotRecorded { .. }
            | TaskEvent::CounterfactualReplayStarted { .. }
            | TaskEvent::SandboxLost { .. }
            | TaskEvent::SandboxRestored { .. }
            | TaskEvent::EnvironmentPinned { .. }
            | TaskEvent::EnvironmentStale { .. }
            | TaskEvent::EnvironmentRebuilt { .. } => None,
            // The workspace moved (M8.7): the projection follows.
            TaskEvent::TaskWorkspaceRebound {
                workspace_root,
                execution_profile,
                ..
            } => {
                if self.state.is_terminal() {
                    return Err(invalid(&self.state, event.event_type()));
                }
                self.workspace_root = Some(workspace_root.clone());
                if !execution_profile.is_empty() {
                    self.execution_profile = execution_profile.clone();
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

/// A forge issue as the intake sees it (PX-010: read by the Core; PX-011:
/// delivered by the forge's webhook to the Cloud API). Both paths make the
/// same canonical task from it: the goal it names and the untrusted
/// context document it renders to are one rendering, here.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ForgeIssueIntake {
    /// The issue's web URL.
    pub url: String,
    /// Issue number.
    pub number: u64,
    /// Title.
    pub title: String,
    /// Author login.
    #[serde(default)]
    pub author: String,
    /// `open` | `closed`.
    #[serde(default)]
    pub state: String,
    /// Label names.
    #[serde(default)]
    pub labels: Vec<String>,
    /// Body text (data, never instructions).
    #[serde(default)]
    pub body: String,
}

impl ForgeIssueIntake {
    /// The task's goal when none was given: the title and the number.
    #[must_use]
    pub fn goal(&self) -> String {
        let title = if self.title.trim().is_empty() {
            "issue"
        } else {
            self.title.trim()
        };
        format!("{title} (#{})", self.number)
    }

    /// The attached context document's text.
    #[must_use]
    pub fn document_text(&self) -> String {
        format!(
            "# {}\n\nissue #{} by {} ({}) — {}\nlabels: {}\n\n{}\n",
            self.title,
            self.number,
            self.author,
            self.state,
            self.url,
            self.labels.join(", "),
            self.body
        )
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
            t.apply(
                &TaskEvent::TaskSteered {
                    text: "x".into(),
                    provenance: String::new(),
                    untrusted: false,
                    input_ids: vec![],
                },
                Timestamp(4),
            )
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
