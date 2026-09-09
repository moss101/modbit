# Task Card — IMP-EV-0262 Queued prompts

## Identity

- Task ID: IMP-EV-0262
- Milestone: M1
- Canonical owner: Input Queue / core-runtime
- Requirement IDs: REQ-EV-0262; qualification QUAL-EV-0262
- Risk class: high (protocol / persistence / security boundary as applicable)
- Evidence tier: release-critical
- Release membership: ALPHA, BETA, RELEASE_ZERO

## Goal

QueueInput appends a typed, durable TaskInputQueued event (STEER | COLLECT | FOLLOW_UP) on the task aggregate under the session lease; ordering is the aggregate sequence and is preserved across reconnect and Core restart; retries replay.

## Non-goals

Behavior owned by later milestones (agent runtime, providers, checkpoints, cloud plane).

## Required reading

`AGENTS.md`, the `docs/40` row for REQ-EV-0262, `docs/13`, `docs/19`, `docs/30`, `docs/32`, `docs/33`, `docs/39`, `docs/81`.

## Existing-code audit

- classification: IMPLEMENTED-PARTIAL on the M1.1–M1.5 substrate before this task; this task adds the missing behavior and the named qualification test
- production entry point: `modbit-core` command handlers / desktop main IPC / domain reducers as applicable
- real effector/storage boundary: `core.db`, the real local socket, the real Electron app

## Verification

- qualification test IDs: QUAL-EV-0262 → qual_ev_0262_queued_inputs_keep_order_across_reconnect_and_restart
- E2E: hosted CI on macOS/Linux/Windows

## Completion evidence

See `evidence.json` (written at seal time).
