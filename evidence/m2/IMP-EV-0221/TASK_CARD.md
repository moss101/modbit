# Task Card — IMP-EV-0221 Background task list/output/stop

## Identity

- Task ID: IMP-EV-0221
- Milestone: M2 (backlog batch 2c)
- Requirement: REQ-EV-0221 (ADOPT); owner label: Task Runtime; subsystem: core-runtime
- Qualification: QUAL-EV-0221 — Run long task, list/status/read full output/stop after UI restart.
- Evidence tier: real-system or production-equivalent

## Goal

Run long task, list/status/read full output/stop after UI restart.

## Existing-code audit

- classification: IMPLEMENTED in this batch (the broker had the sessions; no tool exposed them)
- production entry point: crates/tools/src/direct.rs shell.start / shell.read / shell.list / shell.cancel over the durable broker sessions (services/modbit-execd)
- proof: a ticker started in the background is listed, read from a byte cursor as a bounded preview that continues exactly, and cancelled from a fresh client connection; the exit and the full OutputRef are returned; unknown handles are typed

## Verification

Named tests (crate/test-binary :: test name), run on macOS, Linux and Windows by `.github/workflows/ci.yml`:

- `modbit-core/surface_protocol` :: `qual_ev_0221_background_handles_survive_client_restart_with_bounded_preview_and_cancel`
- `modbit-execd/broker` :: `qual_ev_0271_0027_0135_detach_reattach_from_cursor_exactly_then_cancel_kills_the_real_process`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- `ci-run-34437627213.json`, `ci-run-34437627213-tests.log`: hosted CI run 34437627213 on a1f7615, green on macOS, Linux and Windows
