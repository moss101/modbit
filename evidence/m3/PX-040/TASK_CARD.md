# Task Card — PX-040 Agent harness contracts

## Identity

- Task ID: PX-040
- Milestone: M3 (moved from M2 by DR-M2-002)
- Requirement: REQ-PX-040; owner: core-runtime; docs/14 the eleven harness contracts
- Qualification: QUAL-PX-040 — real fixture task with a large-output command, a failing test, a missing runner and a headless CLI run: every tool result reaches the model within the inline ceiling with declared omitted ranges and a pageable OutputRef; failing CheckResults arrive first; the Context Pack carries harness_state with plan, open failure signatures, quarantines, scope counters, remaining budgets and candidate revision; the missing runner is recorded in the plan with a question or compensation; the headless question fails closed to Needs Attention after the configured wait; a completion proposal while a verification run is in flight or stale is refused; budget exhaustion emits HarnessBudgetExhausted and moves the task to Waiting or Needs Attention with partial evidence. Negative proof: silent truncation, a turn aborted by a command failure, a task blocked forever on a headless question, a completion accepted without the COMPLETION run at the final revision, or a budget exhausted without an event or attention state
- Evidence tier: real-system or production-equivalent

## Goal

The eleven harness contracts of docs/14 hold together on a real task.

## Existing-code audit

- classification: COMPLETED in this batch (M2.7–M2.10 built the turn loop, bounded observations, budgets, steering and the completion handshake; M3's PX-015/016/018/038/039 added the gates; this task closed the one real gap — the observation promised paging through a tool that did not exist)
- production entry point: `crates/core-runtime/src/harness.rs` `observe` (inline ceiling, declared omitted range, `result_ref`), `HarnessState` in the Context Pack, budgets and `Exhausted`; `crates/tools` `artifact.range` (new) reading a stored result back by range through the Core's `ArtifactSource` over the event store's object directory; `services/modbit-core/src/runtime.rs` turn loop, headless question suspension with `TaskNeedsAttention`, completion handshake through the COMPLETION run
- proof: on a real task a command emitting ~47 KiB is observed within the 16 KiB ceiling with `omitted: 16384..` and a `result_ref`, and `artifact.range` reads the rest back to EOF; the Context Pack's `harness_state` carries the plan, budgets, open failures, scope counters, quarantines and both policies; the derived plan records `no configured runner detected` for the missing runner; the headless question suspends the run (Waiting / UserInput / Suspended, loop not alive) with `TaskNeedsAttention`, the agent never answers it and completion never runs

## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`):

- `qual_px_040_harness_contracts_bound_observations_page_results_and_fail_closed_headless`
- `m2_7_one_agent_runtime_drives_a_coding_task_to_ready_for_review` and `m2_7_harness_refuses_unplanned_writes_exhausts_budgets_and_resumes_after_restart` (turn shape, command failure as evidence, `HarnessBudgetExhausted` into Waiting with attention, resume after restart)
- `m2_8_verification_engine_gates_completion_on_real_cargo_fixture` (failing CheckResults first with raw refs; completion refused until the COMPLETION run at the final revision passes)
- `m2_7_steering_and_cancellation_apply_at_safe_boundaries` (steering at safe boundaries)
- `qual_ev_0222_px_014_typed_question_suspends_the_run_and_the_answer_resumes_it` (question service; resume needs a real answer)
- `harness::tests::observations_declare_truncation`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json/log copies alongside
