# Task Card — IMP-EV-0093 Workspace trust + sandbox separation

## Identity

- Task ID: IMP-EV-0093
- Milestone: M2 (backlog batch 1: qualification of substrate delivered by M2.1–M2.10)
- Requirement: REQ-EV-0093 (ADOPT); owner label: Policy + Workspace Fabric; subsystem: effects-security
- Qualification: QUAL-EV-0093 — Trusted repo still cannot use denied network/secret capability.
- Evidence tier: real-system or production-equivalent

## Goal

Trusted repo still cannot use denied network/secret capability. The substrate for this requirement landed in the M2 milestone tasks; this card records the named qualification proof against the real substrate (real processes, real git, real SQLite, real Core over the socket) on hosted CI.

## Existing-code audit

- classification: FOUND (delivered by the M2 milestone tasks)
- production entry point: crates/policy/src/kernel.rs (workspace trust is separate from capability policy)
- proof: trust widens file scope only; network and secret capabilities stay denied

## Verification

Named tests (crate/test-binary :: test name), all run on macOS, Linux and Windows by `.github/workflows/ci.yml`:

- `modbit-policy/kernel` :: `qual_ev_0093_trusted_workspace_cannot_use_denied_network_or_secret`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- `ci-run-34409173853.json`, `ci-run-34409173853-tests.log`: hosted CI run 34409173853 on 7bdf76d, green on macOS, Linux and Windows
