# Task Card — IMP-EV-0271 Durable terminal + replay window

## Identity

- Task ID: IMP-EV-0271
- Milestone: M2 (backlog batch 1: qualification of substrate delivered by M2.1–M2.10)
- Requirement: REQ-EV-0271 (ADOPT); owner label: Terminal Broker; subsystem: terminal
- Qualification: QUAL-EV-0271 — Restart desktop, replay exact terminal tail and continue input.
- Evidence tier: real-system or production-equivalent

## Goal

Restart desktop, replay exact terminal tail and continue input. The substrate for this requirement landed in the M2 milestone tasks; this card records the named qualification proof against the real substrate (real processes, real git, real SQLite, real Core over the socket) on hosted CI.

## Existing-code audit

- classification: FOUND (delivered by the M2 milestone tasks)
- production entry point: services/modbit-execd/src (durable session ring, cursor-exact reattach)
- proof: detach, reattach from a cursor and receive the exact tail; the process outlives the client

## Verification

Named tests (crate/test-binary :: test name), all run on macOS, Linux and Windows by `.github/workflows/ci.yml`:

- `modbit-execd/broker` :: `qual_ev_0271_0027_0135_detach_reattach_from_cursor_exactly_then_cancel_kills_the_real_process`
- `modbit-execd/broker` :: `pty_mode_runs_a_real_terminal_session`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- `ci-run-34409173853.json`, `ci-run-34409173853-tests.log`: hosted CI run 34409173853 on 7bdf76d, green on macOS, Linux and Windows
