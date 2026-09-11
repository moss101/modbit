# Task Card — IMP-EV-0055 Protocol-state persistence

## Identity

- Task ID: IMP-EV-0055
- Milestone: M4 Durable recovery spine (P0)
- Requirement: REQ-EV-0055; owner label: Reliability Layer; subsystem: durability
- Qualification: `QUAL-EV-0055` — Crash while tool awaits approval; restart reconstructs exact pending state.
- Evidence tier: real-system (the real Core hard-killed while a destructive call waits on its approval, and while a command runs)

## Goal

Persist the tool/approval/question/terminal lifecycle needed for exact resume, so a restarted Core reconstructs the exact pending state and the resumed run re-enters it by id.

## Existing-code audit

- classification: IMPLEMENTED by M4.1 (this card ladders the requirement on M4.1's evidence; M4.5 added the terminal cursors and M4.6 the user-facing reconciliation).
- production entry points: `crates/protocol-state` (typed reconstruction, `ResumeBoundary`, `find_call` by run + model call id + tool + intent hash, digest); event-store V10 `protocol_state` materialized in the append transaction; `services/modbit-core/src/tools.rs` write-ahead `ToolCallProposed` / `DispatchJournal`; `services/modbit-core/src/protocol.rs` and `runtime.rs` resume (`dangling_calls`, `ProtocolStateResumed`, `reconcile_after_restart`); `GetProtocolState` on the wire. Browser and subagent lifecycle are typed cursors without producers until M7/M9 (see M4.5 limitations).
- proof: see `evidence/m4/M4.1/TASK_CARD.md` — the same ApprovalId bound to the same intent hash on the same ToolCallId after a hard kill; one approval, one effect; an in-flight command becomes an unknown outcome that is reconciled, never re-dispatched.

## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`):

- `qual_ev_0055_e2e_004_core_crash_during_approval_restores_the_same_approval_and_one_effect`
- `qual_ev_0055_e2e_005_core_crash_after_dispatch_reconciles_the_unknown_outcome_without_replay`
- Unit: `an_approval_pending_call_resumes_at_the_same_approval_and_intent`, `in_flight_and_unknown_calls_reconcile_before_anything_else`, `a_question_without_an_answer_is_the_boundary_and_finished_calls_are_dropped`, `reconciliation_follows_the_effect_class`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names); the M4.1 run json copy alongside
