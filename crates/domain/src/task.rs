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
    /// The Isolated Non-Committing Reviewer's task (EPR-018, docs/27 §9.5):
    /// a disposable review environment on a candidate revision.
    Review,
    /// A subagent's task, admitted by a parent agent (M6.3, docs/14
    /// "Transactional subagent admission").
    Subagent,
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
            Self::TaskCancelRequested { .. } => "TaskCancelRequested",
            Self::TaskNeedsAttention { .. } => "TaskNeedsAttention",
            Self::TaskInputQueued { .. } => "TaskInputQueued",
            Self::UserQuestionAsked { .. } => "UserQuestionAsked",
            Self::UserQuestionAnswered { .. } => "UserQuestionAnswered",
            Self::AttachmentIngested { .. } => "AttachmentIngested",
            Self::PlanRecorded { .. } => "PlanRecorded",
            Self::PlanRevised { .. } => "PlanRevised",
            Self::PlanAnnotated { .. } => "PlanAnnotated",
            Self::UserPatchApplied { .. } => "UserPatchApplied",
            Self::ExternalDiagnosticsRecorded { .. } => "ExternalDiagnosticsRecorded",
            Self::ExternalDiagnosticsRejected { .. } => "ExternalDiagnosticsRejected",
            Self::ForgePullRequestOpened { .. } => "ForgePullRequestOpened",
            Self::ForgePullRequestUpdated { .. } => "ForgePullRequestUpdated",
            Self::TaskCreatedFromIssue { .. } => "TaskCreatedFromIssue",
            Self::BrowserSessionOpened { .. } => "BrowserSessionOpened",
            Self::BrowserHostAttached { .. } => "BrowserHostAttached",
            Self::BrowserNavigated { .. } => "BrowserNavigated",
            Self::BrowserSessionClosed { .. } => "BrowserSessionClosed",
            Self::BrowserControlChanged { .. } => "BrowserControlChanged",
            Self::BrowserPageObserved { .. } => "BrowserPageObserved",
            Self::BrowserActionPerformed { .. } => "BrowserActionPerformed",
            Self::BrowserRegionCaptured { .. } => "BrowserRegionCaptured",
            Self::SecurityEventRecorded { .. } => "SecurityEventRecorded",
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
            Self::ProgramStarted { .. } => "ProgramStarted",
            Self::ProgramEnded { .. } => "ProgramEnded",
            Self::SkillSelected { .. } => "SkillSelected",
            Self::SkillRejected { .. } => "SkillRejected",
            Self::RulesSelected { .. } => "RulesSelected",
            Self::AgentNodeCreated { .. } => "AgentNodeCreated",
            Self::AgentNodeTransitioned { .. } => "AgentNodeTransitioned",
            Self::AgentBindingChanged { .. } => "AgentBindingChanged",
            Self::SubagentAdmitted { .. } => "SubagentAdmitted",
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
            Self::UsageReconciled { .. } => "UsageReconciled",
            Self::CiEvidenceRecorded { .. } => "CiEvidenceRecorded",
            Self::RequestOutcomeRecorded { .. } => "RequestOutcomeRecorded",
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
            | TaskEvent::PlanAnnotated { .. }
            | TaskEvent::UserPatchApplied { .. }
            | TaskEvent::ExternalDiagnosticsRecorded { .. }
            | TaskEvent::ExternalDiagnosticsRejected { .. }
            | TaskEvent::ForgePullRequestOpened { .. }
            | TaskEvent::ForgePullRequestUpdated { .. }
            | TaskEvent::TaskCreatedFromIssue { .. }
            | TaskEvent::BrowserSessionOpened { .. }
            | TaskEvent::BrowserHostAttached { .. }
            | TaskEvent::BrowserNavigated { .. }
            | TaskEvent::BrowserSessionClosed { .. }
            | TaskEvent::BrowserControlChanged { .. }
            | TaskEvent::BrowserPageObserved { .. }
            | TaskEvent::BrowserActionPerformed { .. }
            | TaskEvent::BrowserRegionCaptured { .. }
            | TaskEvent::SecurityEventRecorded { .. }
            | TaskEvent::TaskPauseRequested { .. }
            | TaskEvent::TaskResumeRequested { .. }
            | TaskEvent::TaskCancelRequested { .. }
            | TaskEvent::BrowserCredentialFilled { .. }
            | TaskEvent::SandboxLeaseAcquired { .. }
            | TaskEvent::TaskHandedOff { .. }
            | TaskEvent::TaskHandoffAdmitted { .. }
            | TaskEvent::SelfReviewRecorded { .. }
            | TaskEvent::ToolsActivated { .. }
            | TaskEvent::ProgramStarted { .. }
            | TaskEvent::ProgramEnded { .. }
            | TaskEvent::SkillSelected { .. }
            | TaskEvent::SkillRejected { .. }
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
            // A sandbox is given back after the task ended (M8.5), and one
            // may be lost at any time: the records of the substrate's
            // lifecycle land whatever the task's state. So do the request's
            // accounting records (REQ-EPR-010): a review concludes and a
            // late invoice arrives after the request's own run is over; and
            // CI results (PX-009), which finish on the forge's schedule.
            TaskEvent::SandboxReleased { .. }
            | TaskEvent::CiEvidenceRecorded { .. }
            | TaskEvent::UsageReconciled { .. }
            | TaskEvent::RequestOutcomeRecorded { .. }
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
