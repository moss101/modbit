# Task Card — PX-156 Time parked on an approval is excluded from tool timeouts and wall-clock accounting

## Identity

- Task ID: PX-156 (REQ-PX-156; owner core-runtime); qualification QUAL-PX-156; prerequisite DOC-PX-017 (row registered on PR #148's branch)
- Evidence tier: release-critical (execution budgets)

## Existing-code audit (main 5b17f473)

- Tool timeouts: PRODUCTION-WORKING already — a call that needs approval returns `ApprovalPending` before anything runs and is re-entered by its id after the decision, so its own timeout starts when it executes (proven below, not changed).
- Wall clock: BROKEN for parked approvals. The budget was each run's first-to-last event span (`usage::SpendScan`) and, inside a run, `run_clock.elapsed()`; the inline approval wait (`TaskWaiting{APPROVAL}` … `TaskResumed`) was charged, so a person taking longer than the cap ended the task at the next boundary. A question suspends the run, so its wait was already outside every span.
- Owners kept: `services/modbit-core/src/usage.rs` (the one spend scanner), the run loop's boundary check, the inspector's budget view.

## Built

- `usage.rs`: `SpendScan` pairs each approval `TaskWaiting` of a run with the `TaskResumed` after it; `Spent.wall_ms` is the spans less that time and `Spent.parked_ms` names it. `run_parked_ms` reads the same from the task aggregate's own events (an open wait counts to now).
- `runtime.rs`: the resumed-run prior and the per-boundary wall clock both take the parked time off; `HarnessState.parked_ms` (`crates/core-runtime`, serde default).
- `run_control.proto` `BudgetAccountingView.wall_ms_parked = 9` (additive; TS regenerated); the inspector fills it.

## Proof (local, macOS arm64)

- `services/modbit-core/tests/px_156_parked_time.rs` (real Core, real Kernel under an `ASK` policy for `shell.exec`, real broker processes):
  - `qual_px_156_a_call_parked_on_an_approval_is_neither_timed_out_nor_charged_to_the_wall_clock`: a 3 s wall cap, a 1.5 s call timeout, the approval left 3.5 s; the call runs after the approval with `timed_out: false`, the task reaches ReadyForReview, the accounting reports `wall_ms_parked >= 3500` and `wall_ms_used < 3000`.
  - `qual_px_156_a_call_that_is_not_parked_still_times_out_on_schedule`: `sleep 5` under a 400 ms timeout times out and nothing is reported parked.
- Existing budget suites re-run: `px_060_budget_policy`, `task_modes`, the budget tests of `surface_protocol` and `automations_px`; clippy `-D warnings`.
- Mutation check: `mutation-check.txt` — the boundary charging the parked time, the scanner charging it, and the approval wait not recognised; all KILLED.

## Limitations

- The suite is `cfg(unix)` (it runs `sh`); Windows runs the scanner through the other budget suites only. The model is the scripted stand-in.
- An unbounded wait is still bounded by the existing approval expiry (PX-058), not by this row.
- Graph status cannot move until PR #148 (which registers PX-156) lands on main.

## 2026-11-08: wall-clock margin widened (CI flake)
- On macos-latest (moss101/modbit#188, run 42483653881) the parked-call test failed at `px_156_parked_time.rs:170`: `wall_ms_used: 3109` against a 3000 ms cap, with `wall_ms_parked: 3580`. The parked time was excluded as designed; the run's own unparked time (start, three scripted turns, one command) exceeded 3 s on a loaded hosted runner.
- `WALL_CAP_MS` 3000 → 8000 and `PARKED_MS` 3500 → 9000: the parked time stays longer than the cap and than the call's 1.5 s timeout, so charging it still ends the task and fails the check. Locally the unparked time is about 1.4 s.
- `margin-2026-11-08.txt`: baseline 2 passed; the mutation that stops subtracting the parked time fails it (`wall_ms_used: 10497`, cap 8000); restored with `git checkout` and rebuilt, 2 passed. No product code changed.
