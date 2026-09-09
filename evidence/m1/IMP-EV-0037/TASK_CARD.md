# Task Card — IMP-EV-0037 Attention/fleet supervision

## Identity

- Task ID: IMP-EV-0037
- Milestone: M1
- Canonical owner: Workspace UI / desktop
- Requirement IDs: REQ-EV-0037; qualification QUAL-EV-0037
- Risk class: high (protocol / persistence / security boundary as applicable)
- Evidence tier: release-critical
- Release membership: ALPHA, BETA, RELEASE_ZERO

## Goal

The Fleet's six attention buckets are a pure projection of Core task state (snapshot + ordered events); after a Core restart or an app reload the buckets are re-derived from the Core and no client-local truth survives.

## Non-goals

Behavior owned by later milestones (agent runtime, providers, checkpoints, cloud plane).

## Required reading

`AGENTS.md`, the `docs/40` row for REQ-EV-0037, `docs/13`, `docs/19`, `docs/30`, `docs/32`, `docs/33`, `docs/39`, `docs/81`.

## Existing-code audit

- classification: IMPLEMENTED-PARTIAL on the M1.1–M1.5 substrate before this task; this task adds the missing behavior and the named qualification test
- production entry point: `modbit-core` command handlers / desktop main IPC / domain reducers as applicable
- real effector/storage boundary: `core.db`, the real local socket, the real Electron app

## Verification

- qualification test IDs: QUAL-EV-0037 → Playwright E2E fleet.spec.ts: after Core SIGKILL and after full app restart the cards and their Waiting bucket come back from the Core projection; renderer model unit tests (Completed only via TaskCompleted)
- E2E: hosted CI on macOS/Linux/Windows

## Completion evidence

See `evidence.json` (written at seal time).
