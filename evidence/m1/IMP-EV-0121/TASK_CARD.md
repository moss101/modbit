# Task Card — IMP-EV-0121 Resume/sessions

## Identity

- Task ID: IMP-EV-0121
- Milestone: M1
- Canonical owner: Session Store / core-runtime
- Requirement IDs: REQ-EV-0121; qualification QUAL-EV-0121
- Risk class: high (protocol / persistence / security boundary as applicable)
- Evidence tier: release-critical
- Release membership: ALPHA, BETA, RELEASE_ZERO

## Goal

Resume happens from canonical state (projections rebuilt from the log) and an event cursor; after a Core crash the pending state (queued tasks, queued inputs, lease generation) is reproduced exactly.

## Non-goals

Behavior owned by later milestones (agent runtime, providers, checkpoints, cloud plane).

## Required reading

`AGENTS.md`, the `docs/40` row for REQ-EV-0121, `docs/13`, `docs/19`, `docs/30`, `docs/32`, `docs/33`, `docs/39`, `docs/81`.

## Existing-code audit

- classification: IMPLEMENTED-PARTIAL on the M1.1–M1.5 substrate before this task; this task adds the missing behavior and the named qualification test
- production entry point: `modbit-core` command handlers / desktop main IPC / domain reducers as applicable
- real effector/storage boundary: `core.db`, the real local socket, the real Electron app

## Verification

- qualification test IDs: QUAL-EV-0121 → kill_points_during_a_command_stream_never_duplicate_or_tear_state; qual_ev_0262 (inputs after restart); qual_ev_0054_0273 (lease after restart); desktop E2E app restart
- E2E: hosted CI on macOS/Linux/Windows

## Completion evidence

See `evidence.json` (written at seal time).
