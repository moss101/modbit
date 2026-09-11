# Task Card — M4.6 kill-point recovery suite

## Identity

- Task ID: M4.6
- Milestone: M4 Durable recovery spine (P0)
- Requirements: docs/19 "Resume algorithm" ("release-tested by process kill at every major state"), docs/54 faults 1, 2, 5, 6 and 7 (Core killed before/after commit; tool transport drops after dispatch; approval persisted then Core killed before dispatch; effect dispatched, response lost, retry arrives), docs/51 E2E-004/005/007 shapes; docs/13 "`UnknownOutcome` is never automatically retried for effectful tools"; REQ-EV-0055, REQ-EV-0073 (typed recovery diagnostics: the fault names the boundary and the log names what was reconciled).
- Qualification: docs/51 `E2E-005` in full (kill after the dispatch boundary, before the result acknowledgement: unknown outcome, receipt queried, not replayed, reconciliation visible to the user) and the kill-point suite across every M4 boundary.
- Evidence tier: real-system (the real Core aborts itself at the boundary; real restart, real resume, real worktree)

## Goal

Prove the whole recovery spine at once: whatever boundary the Core dies at, the restarted Core resumes the task to the same end state and worktree as an unkilled run, runs each effect once, invents nothing, and reconciles what it cannot know — with the user able to settle an unknown effect the Core holds.

## Existing-code audit

- classification: PARTIAL before this task. M1's kill-point test covered command streams; M4.1–M4.5 each proved one boundary. No suite killed the Core at every recovery boundary of a real task; two atomicity gaps and one replay gap were latent: a tool call's outcome and the file changes it made were separate transactions, a run's end and the task transition it implies were separate transactions, and a resumed run re-entering a call that had already finished ran it again.
- production entry points:
  - `crates/event-store/src/store.rs` — `FaultPlan` (`MODBIT_FAULT_KILL_BEFORE_EVENT` / `..._AFTER_EVENT`, `<EventType>:<n>`): the process aborts right before or right after committing the n-th event of a type; `append_all(reqs, lease)` commits requests on several aggregates in one transaction.
  - `services/modbit-core/src/tools.rs` — a finished call re-entered by a resumed run is replayed from its recorded result, never run again; the retrieval and terminal records, the call's outcome, the approval it opened and the files it changed land in one transaction.
  - `services/modbit-core/src/runtime.rs` — every loop end (fenced, ready for review, cancelled, budget, question, attention, no progress, provider failure) lands the run's end and the task transition in one transaction (`append_batch`); the user's verdict on an unknown outcome is honoured on resume (a confirmed effect is never repeated; the model is told).
  - `services/modbit-core/src/server.rs`, `surface.proto` — `ReconcileToolCall { task_id, tool_call_id, resolution: EFFECT_CONFIRMED | EFFECT_ABSENT, note }` (lease-fenced; refuses a call that is not of unknown outcome, a running task, and a second reconciliation).
  - forward fixes for run 34591118802 (M4.5): the broker no longer inherits any of the Core's pipes (its stderr goes to `execd.log`; on Windows the Core clears the inherit flag on its std handles first) — a broker outliving the Core kept the supervising desktop's pipe open and its close waited for the broker's orphan grace; the Tier A latency gate holds the p90 over forty refreshes (the p95 is reported), since three runner hiccups in forty samples were failing a path that is tens of milliseconds.
  - forward fixes for run 34593607441: the desktop's own pipes still reached the broker through descriptors and handles the browser process leaks into the Core, so before spawning the broker the Core now makes every handle it holds non-inheritable (Windows handle-table sweep; `FD_CLOEXEC` on every Unix descriptor above the stdio triple); the suite's liveness probe tolerates a connection the abort closes mid-request (Windows landed the abort inside `GetTaskStatus`).
- proof: the suite runs the reference task (plan, read, write, complete) on a real Core armed to abort at each of twenty-three boundaries — after TaskStarted, RunCreated, TurnPrepared (first and fourth), ContextPackCompiled, ModelInvocationStarted, ModelInvocationCompleted, StepSucceeded, PlanRecorded, RetrievalRecorded, before and after ToolCallProposed and ToolCallDispatched, after ToolCallSucceeded, FileChanged, SelfReviewRecorded, CheckpointStarted, before and after CheckpointCommitted, VerificationRunRecorded, before and after TaskReadyForReview — then restarts and resumes: every round reaches ReadyForReview with the note file exactly as the reference, one completion, one write that landed (the dispatch-boundary kill makes the write an unknown outcome, reconciled by target inspection — absent — and issued again by a model that read what its call reported), every unknown outcome reconciled, and a kill after the terminal event needs no resume. E2E-005 in full: a destructive effect is approved and dispatched, runs, and the Core dies before acknowledging it; the restarted Core makes it an unknown outcome, finds no receipt and holds it — the protocol state says RECONCILING and the task's attention line names the call; the approval is consumed; the user records that the effect happened; the resumed run is told (`RECONCILED / USER_CONFIRMED`) and does not repeat it; one proposal, one dispatch, one approval on the log.

## Limitations

- The suite kills at event boundaries of the Core's own log; kills inside the broker or a provider (faults 4, 11, 12) are covered by M4.5's broker test and M2's provider interruption tests, not repeated here. Faults 13–19 and 21–30 belong to the browser, sandbox, cloud, subagent and MCP milestones.
- The reactive scripted model retries an absent write because a real model would; the suite does not exercise a model that ignores an unknown outcome.
- `ReconcileToolCall` records the user's verdict; it does not itself inspect the target (the user did, and says so in the note).

## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`):

- `qual_m4_6_kill_point_suite_every_recovery_boundary_survives_a_process_abort`
- `qual_m4_6_e2e_005_a_protected_effect_of_unknown_outcome_is_held_for_the_user_and_never_replayed`
- Regression: `qual_ev_0055_e2e_004_core_crash_during_approval_restores_the_same_approval_and_one_effect`, `qual_ev_0055_e2e_005_core_crash_after_dispatch_reconciles_the_unknown_outcome_without_replay`, `kill_points_during_a_command_stream_never_duplicate_or_tear_state`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json copy alongside
