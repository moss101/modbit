# Task Card — IMP-EV-0054 SessionLease + fencing generation

## Identity

- Task ID: IMP-EV-0054
- Milestone: M1
- Canonical owner: Session Store / core-runtime
- Requirement IDs: REQ-EV-0054; qualification QUAL-EV-0054
- Risk class: high (protocol / persistence / security boundary as applicable)
- Evidence tier: release-critical
- Release membership: ALPHA, BETA, RELEASE_ZERO

## Goal

AcquireSessionLease mints a strictly greater lease generation persisted as a SessionLeaseAcquired event; every mutating command must present it in expected_generation; a stale writer gets STALE_LEASE and appends nothing, across desktop, CLI and Core restart.

## Non-goals

Behavior owned by later milestones (agent runtime, providers, checkpoints, cloud plane).

## Required reading

`AGENTS.md`, the `docs/40` row for REQ-EV-0054, `docs/13`, `docs/19`, `docs/30`, `docs/32`, `docs/33`, `docs/39`, `docs/81`.

## Existing-code audit

- classification: IMPLEMENTED-PARTIAL on the M1.1–M1.5 substrate before this task; this task adds the missing behavior and the named qualification test
- production entry point: `modbit-core` command handlers / desktop main IPC / domain reducers as applicable
- real effector/storage boundary: `core.db`, the real local socket, the real Electron app

## Verification

- qualification test IDs: QUAL-EV-0054 → qual_ev_0054_0273_session_lease_fences_out_stale_writers_across_restart (dual resume: only the current lease appends); CLI and desktop acquire leases
- E2E: hosted CI on macOS/Linux/Windows

## Completion evidence

See `evidence.json` (written at seal time).
