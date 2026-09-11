# Task Card — M4.1 Protocol State store

## Identity

- Task ID: M4.1
- Milestone: M4 Durable recovery spine (P0)
- Requirement: REQ-EV-0055 Protocol-state persistence (owner label Protocol Store; docs/19 layer 2 of the seven-layer durability invariant)
- Qualification: `QUAL-EV-0055` — crash while a tool awaits approval; restart reconstructs the exact pending state. Proof scenarios docs/51 `E2E-004` (same ApprovalId and intent hash restored; approving once causes exactly one effect) and the `E2E-005` shape (kill after the dispatch boundary; the tool becomes UnknownOutcome; the Core queries the ledger and the target; nothing is blindly replayed; the user sees the reconciliation state).
- Evidence tier: real-system (hard-killed `modbit-core` process, restarted against the same profile, driven over the real socket)

## Goal

Persist and reconstruct the protocol state a resumed run needs beyond the canonical events and the transcript — outstanding tool calls and their unknown-outcome reconciliation, pending approvals with the intent they bind, the pending question, active leases — and continue a restarted run from the exact boundary it stopped at, re-entering the calls it already proposed by their recorded ids.

## Existing-code audit

- classification: ABSENT before this task. `crates/protocol-state` was the empty M0.1 scaffold. Tool-call lifecycle events (`ToolCallProposed`, `Validated`, `PolicyDecision`, `Dispatched`) were appended after the effect completed, so a crash between dispatch and result left no record of the dispatch; `ToolCallProposed` carried no binding to the run, the turn or the model's call id and did not keep the arguments; `execute_tool` minted a fresh `ToolCallId` on every entry and the tool-call step was recorded after execution, so a crash while awaiting approval resumed by re-invoking the model with a dangling call, and any re-execution would have opened a second `ApprovalId`; `reconcile_after_restart` flagged every running task Waiting/External whatever it waited on and recorded nothing for calls in flight; no typed reconstruction of pending approval, question, unknown-outcome or lease state existed.
- production entry points:
  - `crates/protocol-state/src/lib.rs` — `ProtocolState::reconstruct` (calls with their phase, pending approvals with intent hash and expiry, the open question, active leases, reconciled unknowns), `boundary()` (`RECONCILING` before `AWAITING_APPROVAL` before `AWAITING_ANSWER` before `EXECUTING` before `TURN_START`), `find_call` (same run, same model call id, same tool, same intent), `digest()`, `Reconciliation::for_effect` (read-only replays, reversible writes inspect the target, protected/external/secret/destructive hold for the receipt), `restart_reason`.
  - `crates/domain/src/toolcall.rs` — `ToolCallProposed` and the `ToolCall` projection carry `run_id`, `turn_id`, the model's `call_id` and `arguments_ref` (serde defaults keep every recorded event readable).
  - `crates/event-store` — migration V10: the binding columns on `tool_calls`, and the docs/31 `protocol_state` table keyed by `session_id + protocol_key`; `task:<task_id>` is materialized in the transaction of every event that touches the task's calls, approvals, leases, question or reconciliations, and rebuilt from the log on `rebuild_projections`. Loaders `open_tool_calls`, `tool_calls_for_task`, `approvals_for_task`, `live_tasks`, `protocol_state`.
  - `crates/tools/src/pipeline.rs` — `DispatchJournal` write-ahead port: the pipeline hands the host a `DispatchRecord` right before the effector runs; a dispatch the journal refuses does not run (`JOURNAL_FAILED`).
  - `services/modbit-core/src/tools.rs` — `ToolCallProposed` is appended before validation, with the arguments stored as an object; `DispatchLog` appends `Validated`/`PolicyDecision`/`Dispatched` before the effect; the outcome events follow with the generation read back from the row.
  - `services/modbit-core/src/protocol.rs` — reconstruction from the materialized row (or the rows), `mark_orphaned_calls` (a call the dead Core had dispatched becomes `ToolCallUnknownOutcome` with the boot generation; a read-only one is cancelled), `attention_after_restart` (the boundary, in words, on the task), `resumed_call` / `unknown_call` (the model's call id names the recorded call), `reconcile_unknown` (receipt lookup or target inspection, recorded as `ToolCallReconciled`, handed to the model as the call's result; never a replay).
  - `services/modbit-core/src/runtime.rs` — `dangling_calls` finds the calls of the last assistant message without a result; the loop continues at that boundary without invoking the model (`ProtocolStateResumed` on the log with the digest), re-enters each call with its recorded `ToolCallId`, and reconciles calls of unknown outcome; `reconcile_after_restart` covers running and waiting tasks whose run the dead Core owned and keeps the wait reason the boundary names.
  - `services/modbit-core/src/server.rs`, `crates/protocol/proto/modbit/v1/surface.proto` — `GetProtocolState` → `ProtocolStateView` (boundary, calls with phase, pending approvals, question, leases, digest).
- proof: with the real Core hard-killed while a destructive `git.worktree.close` waits on its approval, the restarted Core lists the same `ApprovalId` bound to the same intent hash on the same `ToolCallId`, the task waits on the approval with the attention line naming it, and `GetProtocolState` returns the identical pending call and digest; resuming re-enters the call by id (the model is not invoked), opens no second approval, and approving once removes the worktree exactly once — one `ToolCallProposed`, one `ApprovalRequested`, one `ToolCallDispatched`, one receipt carrying the approval, one `ToolCallSucceeded`. With the Core hard-killed while a `shell.exec` runs, the restart finds the journaled dispatch, records `ToolCallUnknownOutcome` naming the boot generation, shows `RECONCILING` with the reason over the wire and on the task's attention line; the resumed run reconciles it (`ToolCallReconciled`, `TARGET_INSPECTED`), hands the model `UNKNOWN_OUTCOME` with the observation as the command's result, never dispatches the command again, and completes with a quiet protocol state.

## Limitations

- Reconciliation of a reversible write inspects the workspace targets the call names (`change.apply` / `change.batch` paths); for a command it reports that there is no workspace target to inspect and leaves the decision to the model. Process-table reconciliation of a command the broker may still be running is M4.5 (terminal cursor metadata) and M4.6.
- A protected effect with no receipt is held (`HELD_FOR_RECEIPT`) and surfaced; a user-facing command to record an external reconciliation (E2E-005 in full, with a simulated staging target) arrives with M4.6's kill-point suite.
- The `protocol_state` table holds the `task:` key only; terminal, browser and sandbox keys arrive with M4.5.
- An approval that expires while the Core is down is not re-requested automatically: the resumed run waits on it as before and the user resolves or the loop's existing expiry handling applies.

## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`):

- `qual_ev_0055_e2e_004_core_crash_during_approval_restores_the_same_approval_and_one_effect`
- `qual_ev_0055_e2e_005_core_crash_after_dispatch_reconciles_the_unknown_outcome_without_replay`
- `modbit-protocol-state` unit tests: `an_approval_pending_call_resumes_at_the_same_approval_and_intent`, `in_flight_and_unknown_calls_reconcile_before_anything_else`, `a_question_without_an_answer_is_the_boundary_and_finished_calls_are_dropped`, `reconciliation_follows_the_effect_class`
- `modbit-event-store`: `migrates_the_committed_m1_1_fixture_and_derives_projections` (V10), `a_pre_routing_database_upgrades_and_derives_routing_state_from_its_log` (V10 applied on an older database)
- Regression: `m2_5_capability_kernel_gates_destructive_tools_behind_intent_bound_approvals`, `qual_ev_0194_approvals_are_canonical_and_never_resolved_by_the_model`, `m2_7_harness_refuses_unplanned_writes_exhausts_budgets_and_resumes_after_restart`, `qual_ev_0222_px_014_typed_question_suspends_the_run_and_the_answer_resumes_it`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json copy alongside
