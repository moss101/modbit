# Task Card — IMP-EV-0025 Stateless process / stateful shell execution model

## Identity

- Task ID: IMP-EV-0025
- Milestone: M2 (backlog batch 1: qualification of substrate delivered by M2.1–M2.10)
- Requirement: REQ-EV-0025 (ADAPT); owner label: Terminal Broker; subsystem: terminal
- Qualification: QUAL-EV-0025 — Two concurrent workspaces cannot leak cwd/env or aliases.
- Evidence tier: real-system or production-equivalent

## Goal

Two concurrent workspaces cannot leak cwd/env or aliases. The substrate for this requirement landed in the M2 milestone tasks; this card records the named qualification proof against the real substrate (real processes, real git, real SQLite, real Core over the socket) on hosted CI.

## Existing-code audit

- classification: FOUND (delivered by the M2 milestone tasks)
- production entry point: services/modbit-execd/src (stateless process execution; explicit stdin)
- proof: two requests through one broker share no cwd, env or shell state

## Verification

Named tests (crate/test-binary :: test name), all run on macOS, Linux and Windows by `.github/workflows/ci.yml`:

- `modbit-execd/broker` :: `qual_ev_0025_stdin_is_explicit_and_two_requests_do_not_share_environment`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- `ci-run-34409173853.json`, `ci-run-34409173853-tests.log`: hosted CI run 34409173853 on 7bdf76d, green on macOS, Linux and Windows
