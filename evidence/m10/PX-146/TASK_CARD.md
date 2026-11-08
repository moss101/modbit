# PX-146 task card — Budget windows and crash resumption of Autonomous tasks

Record: docs/decisions/DR-PX-2026-10-25-017-autonomous-coding-engine.md. Rows, qualification and scenario: docs/62 (PX-146).

Audit classification at start: finding(s) confirmed in the record, section 1. First missing link: see the record.
Evidence: evidence/m10/PX-145/rust-tests.txt (real-Core tests in services/modbit-core/tests/run_control.rs, kernel tests in crates/policy/tests/run_modes.rs, classifier tests in crates/tools/src/shell_class.rs, desktop unit and e2e specs).
Not proven: the live desktop leg for this row on its own (the live runs used AUTONOMOUS and show no approval, but do not exercise the approval deck); Windows and Linux.
Status ceiling (as written at adoption): REAL_TESTING until the live and CI legs are retained. Superseded 2026-11-08: the live legs are not acceptance clauses of QUAL-PX-146 (real Core, scripted model); they stay named as limits below, and the CI leg is retained at closing.

## 2026-11-08: acceptance mapped clause by clause; two failing cases added; mutation check
Acceptance (real Core, scripted model) → test:
- Supervised window ends Waiting with the budget named; `StartTask` with the same limit continues to completion → `run_control.rs::qual_px_146_an_exhausted_budget_is_a_window_a_person_or_an_autonomous_run_can_continue` (first part; one `HarnessBudgetExhausted`).
- AUTONOMOUS run still making progress gets the next window (`BudgetWindowExtended`, reason `AUTONOMOUS_PROGRESS`) and completes with no approval → same test, second part.
- No progress is not extended and ends NO_PROGRESS → `qual_px_146_an_autonomous_run_that_makes_no_progress_is_not_extended`.
- Tool-call window ends the run at the turn boundary → first test, third part (`max_tool_calls`).
- Requests above the ceiling are clamped to 1000 turns and 5000 tool calls → first test, fourth part (the capsule the model sees).
- SIGKILL under AUTONOMOUS: the restarted Core names only that task for resumption, not a Supervised task nor one whose session was ended on purpose; the ordinary `StartTask` completes it with nothing asked → `qual_px_146_an_autonomous_task_a_crash_cut_short_is_offered_for_resumption_and_nothing_else_is`.
Failing cases the QUAL names:
- A window never bounded (more than four) → `core-runtime harness::tests::a_window_of_turns_and_tool_calls_can_be_reopened_a_bounded_number_of_times` (mutation B).
- A cost or wall-clock cap a new window resets → same unit test; cost was asserted, **wall clock added today** (mutation D).
- An extension of a run with no progress → the no-progress test; with a 40-turn window the no-progress bound always ended the run first, so removing the Core's guard went unnoticed (mutation C survived on the first pass). **Added today:** a second phase whose 2-turn window runs out while the run is already stalled, below the bound; it must end on `max_turns` with no `BudgetWindowExtended`.
- Resumption of a task the person ended → the crash test (`EndSession` with `resume_on_reopen: false`).
- A resumption that needs no client and lease → **added today** to the crash test: after the restart and before any `StartTask`, the offered task is neither running nor finished.
- A mutation that skips the window base fails the resume test → mutation A fails both the unit test and the Supervised resume (`run_control.rs:2903`).
Evidence: `mutation-check.txt` (baseline 4/4 and 1/1; A, B, C, D each killed; restored 4/4 and 1/1), `rust-tests-2026-11-08.txt` (run_control 27/27, core-runtime 50/50), `clippy-2026-11-08.txt` (clean).
Limits (not acceptance clauses): no live provider leg and no live desktop approval-deck leg for this row; Windows does not run `run_control.rs` (`cfg(unix)`: the SIGKILL leg is Unix), CI covers macOS and Linux.
