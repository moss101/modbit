# Task Card — IMP-EV-0100 Structured command contract

## Identity

- Task ID: IMP-EV-0100
- Milestone: M2 (backlog batch 1: qualification of substrate delivered by M2.1–M2.10)
- Requirement: REQ-EV-0100 (ADOPT); owner label: Terminal Broker; subsystem: terminal
- Qualification: QUAL-EV-0100 — Conformance suite exercises each field against real processes.
- Evidence tier: real-system or production-equivalent

## Goal

Conformance suite exercises each field against real processes. The substrate for this requirement landed in the M2 milestone tasks; this card records the named qualification proof against the real substrate (real processes, real git, real SQLite, real Core over the socket) on hosted CI.

## Existing-code audit

- classification: FOUND (delivered by the M2 milestone tasks)
- production entry point: services/modbit-execd/src (structured command contract: argv, cwd, env, stdin, streams, exit code, timeout)
- proof: every contract field asserted against a real child process

## Verification

Named tests (crate/test-binary :: test name), all run on macOS, Linux and Windows by `.github/workflows/ci.yml`:

- `modbit-execd/broker` :: `qual_ev_0100_argv_cwd_env_streams_exit_code_and_timeout_are_explicit`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- `ci-run-34409173853.json`, `ci-run-34409173853-tests.log`: hosted CI run 34409173853 on 7bdf76d, green on macOS, Linux and Windows
