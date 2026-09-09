# Task Card — IMP-EV-0194 Multi-client permission mediation

## Identity

- Task ID: IMP-EV-0194
- Milestone: M2 (backlog batch 1: qualification of substrate delivered by M2.1–M2.10)
- Requirement: REQ-EV-0194 (ADAPT); owner label: Approval Service; subsystem: tool-runtime
- Qualification: QUAL-EV-0194 — Conflicting client approvals follow configured policy and are auditable.
- Evidence tier: real-system or production-equivalent

## Goal

Conflicting client approvals follow configured policy and are auditable. The substrate for this requirement landed in the M2 milestone tasks; this card records the named qualification proof against the real substrate (real processes, real git, real SQLite, real Core over the socket) on hosted CI.

## Existing-code audit

- classification: FOUND (delivered by the M2 milestone tasks)
- production entry point: services/modbit-core/src/server.rs ResolveApproval (canonical approval aggregate; idempotent replay)
- proof: first decision wins, the conflicting one replays the recorded outcome, one ApprovalResolved on the log

## Verification

Named tests (crate/test-binary :: test name), all run on macOS, Linux and Windows by `.github/workflows/ci.yml`:

- `modbit-core/surface_protocol` :: `qual_ev_0194_approvals_are_canonical_and_never_resolved_by_the_model`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- `ci-run-34409173853.json`, `ci-run-34409173853-tests.log`: hosted CI run 34409173853 on 7bdf76d, green on macOS, Linux and Windows
