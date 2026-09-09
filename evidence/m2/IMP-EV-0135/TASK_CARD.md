# Task Card — IMP-EV-0135 Background shell handles + bounded output

## Identity

- Task ID: IMP-EV-0135
- Milestone: M2 (backlog batch 1: qualification of substrate delivered by M2.1–M2.10)
- Requirement: REQ-EV-0135 (ADOPT); owner label: Terminal Broker; subsystem: terminal
- Qualification: QUAL-EV-0135 — Background process survives UI restart and output cap.
- Evidence tier: real-system or production-equivalent

## Goal

Background process survives UI restart and output cap. The substrate for this requirement landed in the M2 milestone tasks; this card records the named qualification proof against the real substrate (real processes, real git, real SQLite, real Core over the socket) on hosted CI.

## Existing-code audit

- classification: FOUND (delivered by the M2 milestone tasks)
- production entry point: services/modbit-execd/src (background handles, bounded ring)
- proof: the real process keeps running across client detach; output beyond the cap is retained by digest, not dropped silently

## Verification

Named tests (crate/test-binary :: test name), all run on macOS, Linux and Windows by `.github/workflows/ci.yml`:

- `modbit-execd/broker` :: `qual_ev_0271_0027_0135_detach_reattach_from_cursor_exactly_then_cancel_kills_the_real_process`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- `ci-run-34409173853.json`, `ci-run-34409173853-tests.log`: hosted CI run 34409173853 on 7bdf76d, green on macOS, Linux and Windows
