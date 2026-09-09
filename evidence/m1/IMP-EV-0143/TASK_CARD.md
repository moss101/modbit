# Task Card — IMP-EV-0143 Agent Fleet / status center

## Identity

- Task ID: IMP-EV-0143
- Milestone: M1
- Canonical owner: Workspace UI / desktop
- Requirement IDs: REQ-EV-0143; qualification QUAL-EV-0143
- Risk class: high (protocol / persistence / security boundary as applicable)
- Evidence tier: release-critical
- Release membership: ALPHA, BETA, RELEASE_ZERO

## Goal

Attention buckets (Needs Attention, Ready for Review, Running, Waiting, Completed, Failed) project canonical task state; the reconnect path re-derives them from the Core.

## Non-goals

Behavior owned by later milestones (agent runtime, providers, checkpoints, cloud plane).

## Required reading

`AGENTS.md`, the `docs/40` row for REQ-EV-0143, `docs/13`, `docs/19`, `docs/30`, `docs/32`, `docs/33`, `docs/39`, `docs/81`.

## Existing-code audit

- classification: IMPLEMENTED-PARTIAL on the M1.1–M1.5 substrate before this task; this task adds the missing behavior and the named qualification test
- production entry point: `modbit-core` command handlers / desktop main IPC / domain reducers as applicable
- real effector/storage boundary: `core.db`, the real local socket, the real Electron app

## Verification

- qualification test IDs: QUAL-EV-0143 → fleet.spec.ts reconnect and restart assertions; renderer model tests
- E2E: hosted CI on macOS/Linux/Windows

## Completion evidence

See `evidence.json` (written at seal time).
