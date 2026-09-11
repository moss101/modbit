//! `modbit-protocol-state` — layer 2 of the seven-layer durability invariant
//! (docs/19): the typed reconstruction of everything a resumed run needs
//! beyond the canonical events and the transcript — outstanding tool calls
//! and their unknown-outcome reconciliation, pending approvals with the
//! intent hash they are bound to, the pending user question, the active
//! capability leases — and the exact boundary the run continues from.
//!
//! Canonical owner: core-runtime (`docs/12_REPOSITORY_AND_MODULE_LAYOUT.md`,
//! `docs/81_ARCHITECTURE_GUARDRAILS_AND_FORBIDDEN_DUPLICATION.md`).
//! Dependency direction is enforced by `tools/architecture-lint`.
//!
//! Nothing here reads a database: the Core hands the projections it holds
//! (`tool_calls`, `approvals`, `capability_leases`, the task's question
//! events) to [`ProtocolState::reconstruct`] and acts on the
//! [`ResumeBoundary`] it gets back. The state is deterministic in its input
//! and carries a digest so the resume is on the log as evidence
//! (`ProtocolStateResumed`).

use modbit_domain::approval::{Approval, ApprovalState};
use modbit_domain::lease::{CapabilityLease, LeaseState};
use modbit_domain::toolcall::{EffectClass, ToolCall, ToolCallState};
use modbit_domain::{ApprovalId, CapabilityLeaseId, RunId, TaskId, Timestamp, ToolCallId, TurnId};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Version of the reconstruction; bumped when the shape or the rules change.
pub const PROTOCOL_STATE_VERSION: &str = "protocol-state-1";

/// Where an outstanding call stands.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "phase", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CallPhase {
    /// Proposed or validated; nothing has been decided or dispatched.
    Proposed,
    /// Waiting on an approval bound to the call's intent.
    AwaitingApproval {
        /// The approval the call waits on.
        approval_id: ApprovalId,
    },
    /// Policy allowed the call; it was dispatched (or streaming) when the
    /// state was captured. Across a restart this is an unknown outcome.
    InFlight,
    /// The effect may or may not have happened; reconcile before any retry.
    UnknownOutcome {
        /// Why.
        reason: String,
    },
}

/// An outstanding tool call.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PendingCall {
    /// The call.
    pub tool_call_id: ToolCallId,
    /// Task.
    pub task_id: TaskId,
    /// Run the call belongs to (absent for direct client calls).
    pub run_id: Option<RunId>,
    /// Turn.
    pub turn_id: Option<TurnId>,
    /// The model's call id in the assistant message that proposed it.
    pub call_id: Option<String>,
    /// Tool.
    pub tool_name: String,
    /// Effect class.
    pub effect_class: EffectClass,
    /// sha256 of the normalized arguments (the intent).
    pub arguments_hash: String,
    /// Object hash of the raw arguments JSON.
    pub arguments_ref: Option<String>,
    /// Where it stands.
    pub phase: CallPhase,
    /// Aggregate generation at capture.
    pub generation: u64,
}

/// A pending approval and the intent it is bound to.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PendingApproval {
    /// Identity.
    pub approval_id: ApprovalId,
    /// The call it authorizes.
    pub tool_call_id: ToolCallId,
    /// Tool.
    pub tool_name: String,
    /// Effect class.
    pub effect_class: EffectClass,
    /// The intent the approval is bound to; a resolution authorizes exactly
    /// this hash and nothing else.
    pub intent_hash: String,
    /// Expiry.
    pub expires_at: Option<Timestamp>,
    /// Whether it had expired at capture.
    pub expired: bool,
}

/// The open typed question.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PendingQuestion {
    /// Question id.
    pub question_id: String,
    /// The model's call id that asked it.
    pub call_id: String,
}

/// An active capability lease.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActiveLease {
    /// Identity.
    pub lease_id: CapabilityLeaseId,
    /// Generation.
    pub generation: u64,
    /// Highest effect class it can authorize.
    pub effect_ceiling: EffectClass,
    /// Profile.
    pub execution_profile: String,
}

/// A durable terminal handle and the last output cursor the run acknowledged
/// (docs/19 "terminal session ID + last acknowledged output cursor"; M4.5).
/// A resume reads from `last_acknowledged_cursor`; earlier output stays
/// available by replay and, once exited, by `output_ref`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TerminalCursor {
    /// The broker's session id.
    pub handle_id: String,
    /// Client-stable request id: a retry replays the same session.
    pub request_id: String,
    /// Command.
    pub argv: Vec<String>,
    /// Terminal replay generation the handle was last attached under.
    pub replay_generation: u64,
    /// Byte cursor after the bytes the run has seen.
    pub last_acknowledged_cursor: u64,
    /// Whether the process was running at the last observation.
    pub running: bool,
    /// Content-addressed full output, once exited.
    pub output_ref: Option<String>,
    /// Exit code, once known.
    pub exit_code: Option<i32>,
    /// The tool call that started it.
    pub tool_call_id: String,
}

/// A browser session and its control lease (docs/19 "browser session/control
/// lease"; docs/13 "browser control lease generation"). The interface M7's
/// browser tools record into; nothing produces one yet.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BrowserCursor {
    /// Browser session id.
    pub session_id: String,
    /// Control lease generation (a human taking control moves it).
    pub control_lease_generation: u64,
    /// Last acknowledged state cursor.
    pub state_cursor: u64,
}

/// A sandbox lease and its generation (docs/19 "sandbox lease + generation";
/// docs/13 "sandbox lease generation"). The interface M8's isolated
/// execution records into; nothing produces one yet.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SandboxCursor {
    /// Lease id.
    pub lease_id: String,
    /// Lease generation.
    pub generation: u64,
}

/// One change to a task's terminal cursors, as the log records it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TerminalUpdate {
    /// `TerminalCreated`.
    Created {
        /// Handle.
        handle_id: String,
        /// Request id.
        request_id: String,
        /// Command.
        argv: Vec<String>,
        /// Replay generation.
        replay_generation: u64,
        /// Tool call.
        tool_call_id: String,
    },
    /// `TerminalOutputAdvanced`.
    Advanced {
        /// Handle.
        handle_id: String,
        /// Cursor.
        cursor: u64,
        /// Running.
        running: bool,
    },
    /// `ProcessExited`.
    Exited {
        /// Handle.
        handle_id: String,
        /// Output.
        output_ref: String,
        /// Exit code.
        exit_code: Option<i32>,
    },
}

/// Apply one terminal update to the cursors (idempotent for a replayed log).
pub fn apply_terminal(terminals: &mut Vec<TerminalCursor>, update: TerminalUpdate) {
    match update {
        TerminalUpdate::Created {
            handle_id,
            request_id,
            argv,
            replay_generation,
            tool_call_id,
        } => {
            if let Some(t) = terminals.iter_mut().find(|t| t.handle_id == handle_id) {
                t.replay_generation = t.replay_generation.max(replay_generation);
                return;
            }
            terminals.push(TerminalCursor {
                handle_id,
                request_id,
                argv,
                replay_generation,
                last_acknowledged_cursor: 0,
                running: true,
                output_ref: None,
                exit_code: None,
                tool_call_id,
            });
        }
        TerminalUpdate::Advanced {
            handle_id,
            cursor,
            running,
        } => {
            if let Some(t) = terminals.iter_mut().find(|t| t.handle_id == handle_id) {
                t.last_acknowledged_cursor = t.last_acknowledged_cursor.max(cursor);
                t.running = running && t.output_ref.is_none();
            }
        }
        TerminalUpdate::Exited {
            handle_id,
            output_ref,
            exit_code,
        } => {
            if let Some(t) = terminals.iter_mut().find(|t| t.handle_id == handle_id) {
                t.running = false;
                t.output_ref = Some(output_ref);
                t.exit_code = exit_code;
            }
        }
    }
}

/// The boundary a resumed run continues from (docs/19 resume step 9).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "boundary", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ResumeBoundary {
    /// Nothing outstanding: the next turn starts from the transcript.
    TurnStart,
    /// Calls the model asked for were not finished: re-enter them by id.
    Executing {
        /// Outstanding calls in order.
        tool_call_ids: Vec<ToolCallId>,
    },
    /// A call waits on an approval: the same approval, the same intent.
    AwaitingApproval {
        /// The approval.
        approval_id: ApprovalId,
        /// The call.
        tool_call_id: ToolCallId,
        /// The intent.
        intent_hash: String,
    },
    /// The model asked the user a question that has no answer yet.
    AwaitingAnswer {
        /// Question id.
        question_id: String,
    },
    /// A call was in flight or is of unknown outcome: reconcile before
    /// anything else runs.
    Reconciling {
        /// The calls to reconcile, in order.
        tool_call_ids: Vec<ToolCallId>,
    },
}

impl ResumeBoundary {
    /// Stable label for the log.
    #[must_use]
    pub const fn label(&self) -> &'static str {
        match self {
            Self::TurnStart => "TURN_START",
            Self::Executing { .. } => "EXECUTING",
            Self::AwaitingApproval { .. } => "AWAITING_APPROVAL",
            Self::AwaitingAnswer { .. } => "AWAITING_ANSWER",
            Self::Reconciling { .. } => "RECONCILING",
        }
    }
}

/// What a restarted Core may do with a call that was in flight when it died
/// (docs/19 resume step 6; docs/13: `UnknownOutcome` is never automatically
/// retried for effectful tools).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Reconciliation {
    /// Read-only: there is no effect to reconcile; the call is re-proposed.
    ReplaySafe,
    /// Reversible write inside the workspace: inspect the target and hand
    /// the observed state to the model; it decides, the Core never replays.
    InspectTarget,
    /// Protected, external, secret or destructive: hold for the effect
    /// receipt or the user; nothing is retried without a new approval.
    HoldForReceipt,
}

impl Reconciliation {
    /// The rule for an effect class.
    #[must_use]
    pub const fn for_effect(effect: EffectClass) -> Self {
        match effect {
            EffectClass::ReadOnly => Self::ReplaySafe,
            EffectClass::ReversibleWrite => Self::InspectTarget,
            EffectClass::ProtectedWrite
            | EffectClass::ExternalSideEffect
            | EffectClass::SecretAccess
            | EffectClass::Destructive => Self::HoldForReceipt,
        }
    }

    /// Stable label for the log.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::ReplaySafe => "REPLAY_SAFE",
            Self::InspectTarget => "TARGET_INSPECTED",
            Self::HoldForReceipt => "HELD_FOR_RECEIPT",
        }
    }
}

/// The reason recorded on a call that was dispatched by a Core that died
/// before the result was acknowledged.
#[must_use]
pub fn restart_reason(boot_generation: u64) -> String {
    format!(
        "core restarted (boot generation {boot_generation}) after the call was dispatched and before its result was acknowledged"
    )
}

/// What the Core hands over to reconstruct the state.
#[derive(Clone, Debug)]
pub struct Input<'a> {
    /// The task's tool calls (any state; finished ones are dropped here).
    pub tool_calls: &'a [ToolCall],
    /// The task's approvals (any state).
    pub approvals: &'a [Approval],
    /// The task's leases (any state).
    pub leases: &'a [CapabilityLease],
    /// The open question, if the task's log has one.
    pub question: Option<PendingQuestion>,
    /// Calls whose unknown outcome was already reconciled on the log.
    pub reconciled: &'a [ToolCallId],
    /// Terminal cursors, as the log built them.
    pub terminals: &'a [TerminalCursor],
    /// Now, for expiry.
    pub now: Timestamp,
}

impl Default for Input<'_> {
    fn default() -> Self {
        Self {
            tool_calls: &[],
            approvals: &[],
            leases: &[],
            question: None,
            reconciled: &[],
            terminals: &[],
            now: Timestamp(0),
        }
    }
}

/// The reconstructed protocol state of one task.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProtocolState {
    /// Reconstruction version.
    pub version: String,
    /// Task.
    pub task_id: TaskId,
    /// Outstanding calls, oldest first.
    pub calls: Vec<PendingCall>,
    /// Approvals still requested (unexpired or not), oldest first.
    pub approvals: Vec<PendingApproval>,
    /// The open question.
    pub question: Option<PendingQuestion>,
    /// Active leases.
    pub leases: Vec<ActiveLease>,
    /// Calls whose unknown outcome was reconciled on the log (they are no
    /// longer outstanding; kept so a later materialization knows).
    #[serde(default)]
    pub reconciled: Vec<ToolCallId>,
    /// Durable terminal handles with their last acknowledged cursors (M4.5).
    #[serde(default)]
    pub terminals: Vec<TerminalCursor>,
    /// Browser sessions and control leases (interface; M7 produces them).
    #[serde(default)]
    pub browsers: Vec<BrowserCursor>,
    /// Sandbox leases (interface; M8 produces them).
    #[serde(default)]
    pub sandboxes: Vec<SandboxCursor>,
}

impl ProtocolState {
    /// Reconstruct the state for `task` from the projections.
    #[must_use]
    pub fn reconstruct(task: TaskId, input: Input<'_>) -> Self {
        let mut calls: Vec<PendingCall> = input
            .tool_calls
            .iter()
            .filter(|c| c.task_id == task)
            .filter(|c| !input.reconciled.contains(&c.tool_call_id))
            .filter_map(|c| {
                let phase = match c.state {
                    ToolCallState::Proposed
                    | ToolCallState::Validated
                    | ToolCallState::PolicyChecked => CallPhase::Proposed,
                    ToolCallState::ApprovalPending => CallPhase::AwaitingApproval {
                        approval_id: c.approval_id.or_else(|| {
                            input
                                .approvals
                                .iter()
                                .rev()
                                .find(|a| a.tool_call_id == c.tool_call_id)
                                .map(|a| a.approval_id)
                        })?,
                    },
                    ToolCallState::Dispatched | ToolCallState::Streaming => CallPhase::InFlight,
                    ToolCallState::UnknownOutcome => CallPhase::UnknownOutcome {
                        reason: c.unknown_outcome_reason.clone().unwrap_or_default(),
                    },
                    ToolCallState::Succeeded | ToolCallState::Failed | ToolCallState::Cancelled => {
                        return None;
                    }
                };
                Some(PendingCall {
                    tool_call_id: c.tool_call_id,
                    task_id: c.task_id,
                    run_id: c.run_id,
                    turn_id: c.turn_id,
                    call_id: c.call_id.clone(),
                    tool_name: c.tool_name.clone(),
                    effect_class: c.effect_class,
                    arguments_hash: c.arguments_hash.clone(),
                    arguments_ref: c.arguments_ref.clone(),
                    phase,
                    generation: c.generation,
                })
            })
            .collect();
        calls.sort_by(|a, b| {
            a.tool_call_id
                .as_bytes()
                .cmp(b.tool_call_id.as_bytes())
                .reverse()
        });
        // Keep the input order (the store orders by dispatch time then id);
        // the sort above is only a tie-break made stable below.
        let order: Vec<ToolCallId> = input.tool_calls.iter().map(|c| c.tool_call_id).collect();
        calls.sort_by_key(|c| order.iter().position(|id| *id == c.tool_call_id));
        let approvals = input
            .approvals
            .iter()
            .filter(|a| a.task_id == task && a.state == ApprovalState::Requested)
            .map(|a| PendingApproval {
                approval_id: a.approval_id,
                tool_call_id: a.tool_call_id,
                tool_name: a.tool_name.clone(),
                effect_class: a.effect_class,
                intent_hash: a.intent_hash.clone(),
                expires_at: a.expires_at,
                expired: a.expires_at.is_some_and(|e| input.now.0 >= e.0),
            })
            .collect();
        let leases = input
            .leases
            .iter()
            .filter(|l| {
                l.task_id == task
                    && l.state == LeaseState::Active
                    && l.expires_at.is_none_or(|e| input.now.0 < e.0)
            })
            .map(|l| ActiveLease {
                lease_id: l.lease_id,
                generation: l.generation,
                effect_ceiling: l.effect_ceiling,
                execution_profile: l.execution_profile.clone(),
            })
            .collect();
        Self {
            version: PROTOCOL_STATE_VERSION.into(),
            task_id: task,
            calls,
            approvals,
            question: input.question,
            leases,
            reconciled: input.reconciled.to_vec(),
            terminals: input.terminals.to_vec(),
            browsers: Vec::new(),
            sandboxes: Vec::new(),
        }
    }

    /// An empty state for a task nothing has happened to yet.
    #[must_use]
    pub fn empty(task: TaskId) -> Self {
        Self::reconstruct(task, Input::default())
    }

    /// The boundary the run continues from. Reconciliation comes first (an
    /// unknown effect blocks everything after it), then an open approval,
    /// then an open question, then the calls that never finished.
    #[must_use]
    pub fn boundary(&self, run: Option<RunId>) -> ResumeBoundary {
        let mine = |c: &&PendingCall| run.is_none() || c.run_id.is_none() || c.run_id == run;
        let reconcile: Vec<ToolCallId> = self
            .calls
            .iter()
            .filter(mine)
            .filter(|c| {
                matches!(
                    c.phase,
                    CallPhase::InFlight | CallPhase::UnknownOutcome { .. }
                )
            })
            .map(|c| c.tool_call_id)
            .collect();
        if !reconcile.is_empty() {
            return ResumeBoundary::Reconciling {
                tool_call_ids: reconcile,
            };
        }
        if let Some(c) = self.calls.iter().filter(mine).find_map(|c| match &c.phase {
            CallPhase::AwaitingApproval { approval_id } => Some((c, *approval_id)),
            _ => None,
        }) {
            return ResumeBoundary::AwaitingApproval {
                approval_id: c.1,
                tool_call_id: c.0.tool_call_id,
                intent_hash: c.0.arguments_hash.clone(),
            };
        }
        if let Some(q) = &self.question {
            return ResumeBoundary::AwaitingAnswer {
                question_id: q.question_id.clone(),
            };
        }
        let executing: Vec<ToolCallId> = self
            .calls
            .iter()
            .filter(mine)
            .map(|c| c.tool_call_id)
            .collect();
        if executing.is_empty() {
            ResumeBoundary::TurnStart
        } else {
            ResumeBoundary::Executing {
                tool_call_ids: executing,
            }
        }
    }

    /// The outstanding call the model's `call_id` names in `run`, if its
    /// intent is the one recorded: the same call id with different arguments
    /// is a different call and gets a fresh id.
    #[must_use]
    pub fn find_call(
        &self,
        run: RunId,
        call_id: &str,
        tool_name: &str,
        arguments_hash: &str,
    ) -> Option<&PendingCall> {
        self.calls.iter().rev().find(|c| {
            c.run_id == Some(run)
                && c.call_id.as_deref() == Some(call_id)
                && c.tool_name == tool_name
                && c.arguments_hash == arguments_hash
        })
    }

    /// The call by id.
    #[must_use]
    pub fn call(&self, id: &ToolCallId) -> Option<&PendingCall> {
        self.calls.iter().find(|c| c.tool_call_id == *id)
    }

    /// Whether anything is outstanding.
    #[must_use]
    pub fn is_quiescent(&self) -> bool {
        self.calls.is_empty() && self.approvals.is_empty() && self.question.is_none()
    }

    /// sha256 of the canonical JSON of the state (keys sorted), as hex.
    #[must_use]
    pub fn digest(&self) -> String {
        let v = serde_json::to_value(self).unwrap_or_default();
        hex::encode(Sha256::digest(canonical(&v).as_bytes()))
    }
}

fn canonical(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::Object(m) => {
            let mut keys: Vec<&String> = m.keys().collect();
            keys.sort();
            let parts: Vec<String> = keys
                .into_iter()
                .map(|k| {
                    format!(
                        "{}:{}",
                        serde_json::to_string(k).unwrap_or_default(),
                        canonical(&m[k])
                    )
                })
                .collect();
            format!("{{{}}}", parts.join(","))
        }
        serde_json::Value::Array(a) => {
            let parts: Vec<String> = a.iter().map(canonical).collect();
            format!("[{}]", parts.join(","))
        }
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use modbit_domain::toolcall::ToolCallEvent;

    fn call(
        task: TaskId,
        run: RunId,
        call_id: &str,
        name: &str,
        effect: EffectClass,
        hash: &str,
    ) -> ToolCall {
        ToolCall::create(
            ToolCallId::new(),
            &ToolCallEvent::ToolCallProposed {
                task_id: task,
                step_id: None,
                tool_name: name.into(),
                tool_version: "1".into(),
                effect_class: effect,
                capability_lease_id: None,
                arguments_hash: hash.into(),
                run_id: Some(run),
                turn_id: None,
                call_id: Some(call_id.into()),
                arguments_ref: Some("ref".into()),
            },
            Timestamp(1),
        )
        .unwrap()
    }

    #[test]
    fn reconciliation_follows_the_effect_class() {
        assert_eq!(
            Reconciliation::for_effect(EffectClass::ReadOnly),
            Reconciliation::ReplaySafe
        );
        assert_eq!(
            Reconciliation::for_effect(EffectClass::ReversibleWrite),
            Reconciliation::InspectTarget
        );
        for e in [
            EffectClass::ProtectedWrite,
            EffectClass::ExternalSideEffect,
            EffectClass::SecretAccess,
            EffectClass::Destructive,
        ] {
            assert_eq!(
                Reconciliation::for_effect(e),
                Reconciliation::HoldForReceipt
            );
        }
    }

    #[test]
    fn an_approval_pending_call_resumes_at_the_same_approval_and_intent() {
        let task = TaskId::new();
        let run = RunId::new();
        let mut c = call(
            task,
            run,
            "call_1",
            "git.worktree.close",
            EffectClass::Destructive,
            "h1",
        );
        let approval = ApprovalId::new();
        c.apply(&ToolCallEvent::ToolCallValidated, Timestamp(2))
            .unwrap();
        c.apply(
            &ToolCallEvent::ToolCallApprovalRequested {
                approval_id: approval,
                decision: "APPROVAL_REQUIRED".into(),
            },
            Timestamp(3),
        )
        .unwrap();
        let a = Approval::create(
            approval,
            &modbit_domain::approval::ApprovalEvent::ApprovalRequested {
                task_id: task,
                tool_call_id: c.tool_call_id,
                tool_name: "git.worktree.close".into(),
                effect_class: EffectClass::Destructive,
                intent_hash: "h1".into(),
                scope_json: "{}".into(),
                expires_at: Some(Timestamp(1_000)),
            },
            Timestamp(3),
        )
        .unwrap();
        let calls = vec![c.clone()];
        let approvals = vec![a];
        let state = ProtocolState::reconstruct(
            task,
            Input {
                tool_calls: &calls,
                approvals: &approvals,
                leases: &[],
                question: None,
                reconciled: &[],
                terminals: &[],
                now: Timestamp(10),
            },
        );
        assert_eq!(state.calls.len(), 1);
        assert_eq!(
            state.calls[0].phase,
            CallPhase::AwaitingApproval {
                approval_id: approval
            }
        );
        assert_eq!(state.approvals.len(), 1);
        assert!(!state.approvals[0].expired);
        assert_eq!(
            state.boundary(Some(run)),
            ResumeBoundary::AwaitingApproval {
                approval_id: approval,
                tool_call_id: c.tool_call_id,
                intent_hash: "h1".into()
            }
        );
        // The same call id with the same intent is the same call; with a
        // different intent it is not.
        assert_eq!(
            state
                .find_call(run, "call_1", "git.worktree.close", "h1")
                .map(|c| c.tool_call_id),
            Some(c.tool_call_id)
        );
        assert!(
            state
                .find_call(run, "call_1", "git.worktree.close", "h2")
                .is_none()
        );
        assert!(
            state
                .find_call(RunId::new(), "call_1", "git.worktree.close", "h1")
                .is_none()
        );
        // Deterministic digest.
        let again = ProtocolState::reconstruct(
            task,
            Input {
                tool_calls: &calls,
                approvals: &approvals,
                leases: &[],
                question: None,
                reconciled: &[],
                terminals: &[],
                now: Timestamp(10),
            },
        );
        assert_eq!(state.digest(), again.digest());
        assert_eq!(state.digest().len(), 64);
    }

    #[test]
    fn in_flight_and_unknown_calls_reconcile_before_anything_else() {
        let task = TaskId::new();
        let run = RunId::new();
        let mut a = call(
            task,
            run,
            "call_1",
            "change.apply",
            EffectClass::ReversibleWrite,
            "h1",
        );
        a.apply(&ToolCallEvent::ToolCallValidated, Timestamp(2))
            .unwrap();
        a.apply(
            &ToolCallEvent::ToolCallPolicyDecision {
                allowed: true,
                decision: "ALLOW".into(),
                approval_required: false,
            },
            Timestamp(3),
        )
        .unwrap();
        a.apply(&ToolCallEvent::ToolCallDispatched, Timestamp(4))
            .unwrap();
        let b = call(task, run, "call_2", "fs.read", EffectClass::ReadOnly, "h2");
        let mut u = call(
            task,
            run,
            "call_0",
            "shell.exec",
            EffectClass::ReversibleWrite,
            "h0",
        );
        u.apply(&ToolCallEvent::ToolCallValidated, Timestamp(2))
            .unwrap();
        u.apply(
            &ToolCallEvent::ToolCallPolicyDecision {
                allowed: true,
                decision: "ALLOW".into(),
                approval_required: false,
            },
            Timestamp(3),
        )
        .unwrap();
        u.apply(&ToolCallEvent::ToolCallDispatched, Timestamp(4))
            .unwrap();
        u.apply(
            &ToolCallEvent::ToolCallUnknownOutcome {
                reason: restart_reason(7),
            },
            Timestamp(5),
        )
        .unwrap();
        let calls = vec![u.clone(), a.clone(), b.clone()];
        let state = ProtocolState::reconstruct(
            task,
            Input {
                tool_calls: &calls,
                approvals: &[],
                leases: &[],
                question: None,
                reconciled: &[],
                terminals: &[],
                now: Timestamp(10),
            },
        );
        assert_eq!(state.calls.len(), 3);
        assert_eq!(state.calls[0].tool_call_id, u.tool_call_id);
        assert_eq!(state.calls[1].phase, CallPhase::InFlight);
        assert_eq!(state.calls[2].phase, CallPhase::Proposed);
        assert_eq!(
            state.boundary(Some(run)),
            ResumeBoundary::Reconciling {
                tool_call_ids: vec![u.tool_call_id, a.tool_call_id]
            }
        );
        // Once reconciled on the log, the unknown call is gone and the
        // in-flight one still blocks; with both gone the executing boundary
        // names what never finished.
        let state = ProtocolState::reconstruct(
            task,
            Input {
                tool_calls: &calls,
                approvals: &[],
                leases: &[],
                question: None,
                reconciled: &[u.tool_call_id, a.tool_call_id],
                terminals: &[],
                now: Timestamp(10),
            },
        );
        assert_eq!(
            state.boundary(Some(run)),
            ResumeBoundary::Executing {
                tool_call_ids: vec![b.tool_call_id]
            }
        );
        assert!(!state.is_quiescent());
    }

    #[test]
    fn a_question_without_an_answer_is_the_boundary_and_finished_calls_are_dropped() {
        let task = TaskId::new();
        let run = RunId::new();
        let mut done = call(task, run, "call_1", "fs.read", EffectClass::ReadOnly, "h1");
        done.apply(&ToolCallEvent::ToolCallValidated, Timestamp(2))
            .unwrap();
        done.apply(
            &ToolCallEvent::ToolCallPolicyDecision {
                allowed: true,
                decision: "ALLOW".into(),
                approval_required: false,
            },
            Timestamp(3),
        )
        .unwrap();
        done.apply(&ToolCallEvent::ToolCallDispatched, Timestamp(4))
            .unwrap();
        done.apply(
            &ToolCallEvent::ToolCallSucceeded {
                result_ref: "r".into(),
            },
            Timestamp(5),
        )
        .unwrap();
        let calls = vec![done];
        let state = ProtocolState::reconstruct(
            task,
            Input {
                tool_calls: &calls,
                approvals: &[],
                leases: &[],
                question: Some(PendingQuestion {
                    question_id: "q1".into(),
                    call_id: "call_2".into(),
                }),
                reconciled: &[],
                terminals: &[],
                now: Timestamp(10),
            },
        );
        assert!(state.calls.is_empty());
        assert_eq!(
            state.boundary(Some(run)),
            ResumeBoundary::AwaitingAnswer {
                question_id: "q1".into()
            }
        );
        let quiet = ProtocolState::reconstruct(task, Input::default());
        assert!(quiet.is_quiescent());
        assert_eq!(quiet.boundary(None), ResumeBoundary::TurnStart);
    }
}
