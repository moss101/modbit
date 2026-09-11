# Task Card — M4.4 kernel lease/session fencing

## Identity

- Task ID: M4.4
- Milestone: M4 Durable recovery spine (P0)
- Requirements: docs/13 "Fencing and epochs" (session kernel lease generation: a result with an older generation is rejected, recorded and never applied silently), docs/33 "Session kernel lease" (one execution owner; a stale owner can append audit events but cannot advance state after lease loss), docs/19 resume step 1 (acquire the session kernel lease with a new fencing generation), REQ-EV-0054 / REQ-EV-0273 (M1: stale writers over the wire).
- Qualification: docs/54 fault 8 — "stale session/kernel lease attempts write".
- Evidence tier: real-system (real Core process, two clients over the real socket, a paced model)

## Goal

Make the execution owner itself fenced, not only the commands a client sends: the agent loop advances state only while it holds the session's current lease generation, stops at a safe boundary when it loses it, records the fence, and leaves the run for the new owner to resume.

## Existing-code audit

- classification: PARTIAL before this task. M1 fenced *commands* (`require_lease`, `STALE_LEASE`, generations forward-only on the session aggregate, `RunResumed` refusing an older generation). The agent loop — the execution owner — carried no generation: once started it kept appending turns, steps, dispatches and outcomes whatever lease the session was under, so a second owner taking the lease mid-run had two writers advancing the same task.
- production entry points:
  - `crates/event-store/src/store.rs` — `EventStore::append_fenced(req, lease_generation)`: reads the session's lease generation inside the append transaction and refuses with `Error::StaleLease { session, presented, current }` when it differs; nothing is written.
  - `crates/domain/src/run.rs` — `RunEvent::RunFenced { kernel_lease_generation, current_generation, owner }` (no state change; `RunSuspended` follows).
  - `services/modbit-core/src/runtime.rs` — `StartConfig.lease_generation` (set from the generation `StartTask` presented); `Lineage::fenced` / `unfenced` / `lease`; `append` uses `append_fenced` for a fenced lineage and maps the refusal to `STALE_LEASE`; the loop's lineage is fenced, `lease_lost` is checked before each turn, after each model response, before each tool call and at the loop's end; `LoopEnd::Fenced` records `RunFenced` + `RunSuspended` and `TaskWaiting`/`TaskNeedsAttention` naming the generations and the owner — unfenced, as the audit records a stale owner may write.
  - `services/modbit-core/src/tools.rs` — `InvokeRequest.lease_generation`: the write-ahead `ToolCallProposed` and the `DispatchLog` journal are fenced, so a superseded owner cannot start an effect even if a boundary check is raced (`JOURNAL_FAILED`, no effect); the outcome of an effect already running is recorded either way.
- `apps/cli/src/main.rs`, `SessionSnapshot.lease_generation/lease_owner` — the CLI used to mint a new lease generation on every mutating invocation, which under this task's fencing made a second shell's approval or answer fence out the run the first shell was waiting on (found by the headless CLI test). A shell now joins the session's current lease from the snapshot and acquires one only when the session has none: cooperating shells of the same owner act under one generation; a takeover is an explicit acquisition.
- proof: a paced run under generation 1 is under way when a second client acquires the session lease (generation 2, another owner); the Core refuses the old generation at once (`STALE_LEASE` on a fenced command); the loop stops at its next boundary with `RunFenced { 1, 2, owner-b }` and `RunSuspended`, the task waits with an attention line naming the superseded generation; every task event after the takeover is one of the four audit records — no turn, step, dispatch or outcome landed; the stale owner's `StartTask` is refused `STALE_LEASE`; the new owner's resumes the run (`RunResumed` under generation 2) and it reaches ReadyForReview; no step or dispatch lies between the fence and the resume.

## Limitations

- Local mode only: the lease is the session aggregate's generation in the local store; the cloud lease with expiry and heartbeat (docs/33) is M8.
- A model call already in flight when the lease is lost completes against the provider (its cost is spent); its result is not applied and no attempt is recorded for it under the stale generation.
- A subagent (Fast Context specialist) runs inside the parent's loop and is fenced by the parent's generation.

## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`):

- `qual_m4_4_a_stale_execution_owner_is_fenced_out_and_the_new_owner_resumes`
- Regression: `qual_ev_0054_0273_session_lease_fences_out_stale_writers_across_restart`, `m2_7_harness_refuses_unplanned_writes_exhausts_budgets_and_resumes_after_restart`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json copy alongside
