# Task Card — PX-000 Headless CLI thin client for the task lifecycle

## Identity

- Task ID: PX-000
- Milestone: M2 (backlog batch 2c)
- Requirement: REQ-PX-000 (ADOPT); owner label: desktop; subsystem: desktop
- Qualification: QUAL-PX-000 — Real Core plus the CLI process on a fixture repository: create a task, stream events by cursor as JSON lines, answer a question, approve one protected effect, kill and restart Core, resume by cursor; exit codes match the documented contract and exactly one effect receipt exists
- Evidence tier: real-system or production-equivalent

## Goal

Real Core plus the CLI process on a fixture repository: create a task, stream events by cursor as JSON lines, answer a question, approve one protected effect, kill and restart Core, resume by cursor; exit codes match the documented contract and exactly one effect receipt exists

## Existing-code audit

- classification: IMPLEMENTED in this batch on top of the M1.3/M2.x CLI
- production entry point: apps/cli (thin client over modbit-protocol only): task lifecycle, events tail --json by cursor, question answer, approval resolve, receipts, documented exit codes (apps/cli/README.md)
- proof: real Core + CLI on a fixture repo against a scripted model over HTTP: create, run to the question (exit 2), JSON event lines, answer from the shell, approve the protected worktree close from a second shell, ReadyForReview (exit 0), exactly one effect receipt, the stream resumed by cursor across Core restarts with identical offsets and no duplicate, exit 3 after cancel, exit 1 on rejection, exit 4 while queued; the CLI manifest carries no provider/workspace/git/policy/tools crate

## Verification

Named tests (crate/test-binary :: test name), run on macOS, Linux and Windows by `.github/workflows/ci.yml`:

- `modbit-cli/headless` :: `qual_px_000_headless_cli_task_lifecycle`
- `modbit-cli/smoke` :: `cli_drives_a_real_core_end_to_end`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- `ci-run-34437627213.json`, `ci-run-34437627213-tests.log`: hosted CI run 34437627213 on a1f7615, green on macOS, Linux and Windows

## 2026-11-09: macOS CI hang of `headless.rs` (root cause in the test harness; fixed)
- On macos-latest `qual_px_000_headless_cli_task_lifecycle` (run 42528104994, 2026-11-08) and `qual_ev_0126_…` (run 42483653881, 2026-11-08) hung until the job's 75-minute timeout, with nothing to diagnose.
- Every subprocess `headless.rs` starts (each CLI call, `git`, `pkill`) now runs through `tests/support/bounded.rs` `output_within`. The in-process desktop drive of `qual_ev_0126` is bounded the same way. A call past 180 s is killed, and the test panics with the command line, the CLI's stdout and stderr, the tail of `core.log`, the profile's Core processes, the session's last events with its attention list, and the task's status. `a_hung_cli_call_is_killed_and_fails_with_its_diagnostics` proves this against a real `task run --wait` whose model never answers.
- Reproduced locally under CPU load (1 hang in the first 8 full-file runs; 0 in 120 runs without load). The new report showed the cause: the approver thread's `approval list` exited 0 but its stdout never reached EOF, so the approval was never resolved, and the main thread's `question answer --wait` waited on it for good. On macOS std creates a child's pipes with `pipe()` and sets `FD_CLOEXEC` afterwards (no `pipe2`). The main thread's spawn, falling in that window, gave the long-lived `--wait` CLI the approver call's write end. Linux and Windows do not have this window, which is why only macOS hung.
- Fix: `bounded::SPAWN_LOCK` serialises every spawn of the test binary. `no_spawn_inherits_a_pipe_another_thread_is_creating` fails 20 of 20 runs with the lock removed and 0 of 20 with it. `headless-hang-2026-11-09.txt` holds the loops, the std source lines, the hang report and the mutation. No product code changed and no assertion was weakened.
