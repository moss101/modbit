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
