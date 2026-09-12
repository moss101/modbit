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
    /// `TaskSteered`: durable steering input recorded; no state change.
    TaskSteered {
        /// Steering text.
        text: String,
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
            Self::TaskNeedsAttention { .. } => "TaskNeedsAttention",
            Self::TaskInputQueued { .. } => "TaskInputQueued",
            Self::UserQuestionAsked { .. } => "UserQuestionAsked",
            Self::UserQuestionAnswered { .. } => "UserQuestionAnswered",
            Self::AttachmentIngested { .. } => "AttachmentIngested",
            Self::PlanRecorded { .. } => "PlanRecorded",
            Self::PlanRevised { .. } => "PlanRevised",
            Self::UnsupportedLanguageOptInRecorded { .. } => "UnsupportedLanguageOptInRecorded",
            Self::ContextDocumentAttached { .. } => "ContextDocumentAttached",
            Self::SelectionRecorded { .. } => "SelectionRecorded",
            Self::SelfReviewRecorded { .. } => "SelfReviewRecorded",
            Self::ToolsActivated { .. } => "ToolsActivated",
            Self::ProgramStarted { .. } => "ProgramStarted",
            Self::ProgramEnded { .. } => "ProgramEnded",
            Self::SkillSelected { .. } => "SkillSelected",
            Self::SkillRejected { .. } => "SkillRejected",
            Self::ContextEpochOpened { .. } => "ContextEpochOpened",
            Self::CompactionStarted { .. } => "CompactionStarted",
            Self::CompactionCommitted { .. } => "CompactionCommitted",
            Self::CompactionRejectedStale { .. } => "CompactionRejectedStale",
            Self::ReproductionRecorded { .. } => "ReproductionRecorded",
            Self::ScopeExpansionRecorded { .. } => "ScopeExpansionRecorded",
            Self::RetrievalRecorded { .. } => "RetrievalRecorded",
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
            Self::TerminalCreated { .. } => "TerminalCreated",
            Self::TerminalOutputAdvanced { .. } => "TerminalOutputAdvanced",
            Self::ProcessExited { .. } => "ProcessExited",
            Self::ProtocolStateResumed { .. } => "ProtocolStateResumed",
            Self::ToolCallReconciled { .. } => "ToolCallReconciled",
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
            | TaskEvent::ProgramStarted { .. }
            | TaskEvent::ProgramEnded { .. }
            | TaskEvent::SkillSelected { .. }
            | TaskEvent::SkillRejected { .. }
            | TaskEvent::ContextEpochOpened { .. }
            | TaskEvent::CompactionStarted { .. }
            | TaskEvent::CompactionCommitted { .. }
            | TaskEvent::CompactionRejectedStale { .. }
            | TaskEvent::ReproductionRecorded { .. }
            | TaskEvent::SelectionRecorded { .. }
            | TaskEvent::ContextDocumentAttached { .. }
            | TaskEvent::UnsupportedLanguageOptInRecorded { .. }
            | TaskEvent::ScopeExpansionRecorded { .. }
            | TaskEvent::RetrievalRecorded { .. }
            | TaskEvent::RepairAttemptRecorded { .. }
            | TaskEvent::RepairAttemptConcluded { .. }
            | TaskEvent::RepairEscalated { .. }
            | TaskEvent::HarnessBudgetExhausted { .. }
            | TaskEvent::NoProgressDetected { .. }
            | TaskEvent::ReviewDecisionRecorded { .. }
            | TaskEvent::CheckpointStarted { .. }
            | TaskEvent::CheckpointCommitted { .. }
            | TaskEvent::CheckpointRejectedStale { .. }
            | TaskEvent::CheckpointRestored { .. }
            | TaskEvent::TaskForked { .. }
            | TaskEvent::TerminalCreated { .. }
            | TaskEvent::TerminalOutputAdvanced { .. }
            | TaskEvent::ProcessExited { .. }
            | TaskEvent::ProtocolStateResumed { .. }
            | TaskEvent::ToolCallReconciled { .. } => {
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
