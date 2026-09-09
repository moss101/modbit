# Task Card — IMP-EV-0091 Admin policy precedence

## Identity

- Task ID: IMP-EV-0091
- Milestone: M2 (backlog batch 1: qualification of substrate delivered by M2.1–M2.10)
- Requirement: REQ-EV-0091 (ADOPT); owner label: Policy Kernel; subsystem: effects-security
- Qualification: QUAL-EV-0091 — Project instruction cannot weaken org deny rule.
- Evidence tier: real-system or production-equivalent

## Goal

Project instruction cannot weaken org deny rule. The substrate for this requirement landed in the M2 milestone tasks; this card records the named qualification proof against the real substrate (real processes, real git, real SQLite, real Core over the socket) on hosted CI.

## Existing-code audit

- classification: FOUND (delivered by the M2 milestone tasks)
- production entry point: crates/policy/src/kernel.rs (admin deny precedence)
- proof: a project allow never overrides an admin deny

## Verification

Named tests (crate/test-binary :: test name), all run on macOS, Linux and Windows by `.github/workflows/ci.yml`:

- `modbit-policy/kernel` :: `qual_ev_0091_project_allow_cannot_weaken_admin_deny`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- `ci-run-34409173853.json`, `ci-run-34409173853-tests.log`: hosted CI run 34409173853 on 7bdf76d, green on macOS, Linux and Windows
