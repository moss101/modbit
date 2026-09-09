# Task Card — IMP-EV-0108 Bounded IPC dispatch/chunking

## Identity

- Task ID: IMP-EV-0108
- Milestone: M1
- Canonical owner: Transport / modbit-protocol + modbit-core
- Requirement IDs: REQ-EV-0108; qualification QUAL-EV-0108
- Risk class: high (protocol / persistence / security boundary as applicable)
- Evidence tier: release-critical
- Release membership: ALPHA, BETA, RELEASE_ZERO

## Goal

Frames are capped at 4 MiB and rejected before allocation; subscriptions deliver bounded batches; multi-megabyte results are read by ReadObjectRange in ranges of at most 1 MiB from the content-addressed object store.

## Non-goals

Behavior owned by later milestones (agent runtime, providers, checkpoints, cloud plane).

## Required reading

`AGENTS.md`, the `docs/40` row for REQ-EV-0108, `docs/13`, `docs/19`, `docs/30`, `docs/32`, `docs/33`, `docs/39`, `docs/81`.

## Existing-code audit

- classification: IMPLEMENTED-PARTIAL on the M1.1–M1.5 substrate before this task; this task adds the missing behavior and the named qualification test
- production entry point: `modbit-core` command handlers / desktop main IPC / domain reducers as applicable
- real effector/storage boundary: `core.db`, the real local socket, the real Electron app

## Verification

- qualification test IDs: QUAL-EV-0108 → qual_ev_0108_multi_mb_object_is_read_in_bounded_ranges (10 MiB object, 10 ranges, over-large range refused); framing unit tests
- E2E: hosted CI on macOS/Linux/Windows

## Completion evidence

See `evidence.json` (written at seal time).
