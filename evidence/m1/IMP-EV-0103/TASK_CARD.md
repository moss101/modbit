# Task Card — IMP-EV-0103 Renderer→bridge→privileged host boundaries

## Identity

- Task ID: IMP-EV-0103
- Milestone: M1
- Canonical owner: Desktop Security / desktop + modbit-core
- Requirement IDs: REQ-EV-0103; qualification QUAL-EV-0103
- Risk class: high (protocol / persistence / security boundary as applicable)
- Evidence tier: release-critical
- Release membership: ALPHA, BETA, RELEASE_ZERO

## Goal

The sandboxed renderer reaches the Core only through the preload bridge and main's validated IPC; malformed renderer arguments are rejected in main (BAD_ARGUMENT) and never forwarded; at the socket a command before the handshake, a bad secret, malformed bytes or an unknown command are rejected.

## Non-goals

Behavior owned by later milestones (agent runtime, providers, checkpoints, cloud plane).

## Required reading

`AGENTS.md`, the `docs/40` row for REQ-EV-0103, `docs/13`, `docs/19`, `docs/30`, `docs/32`, `docs/33`, `docs/39`, `docs/81`.

## Existing-code audit

- classification: IMPLEMENTED-PARTIAL on the M1.1–M1.5 substrate before this task; this task adds the missing behavior and the named qualification test
- production entry point: `modbit-core` command handlers / desktop main IPC / domain reducers as applicable
- real effector/storage boundary: `core.db`, the real local socket, the real Electron app

## Verification

- qualification test IDs: QUAL-EV-0103 → fleet.spec.ts 'renderer messages with invalid arguments are rejected by main'; wrong_secret_hostile_and_oversized_frames_are_rejected; CSP connect-src 'none', sandbox=true, contextIsolation=true
- E2E: hosted CI on macOS/Linux/Windows

## Completion evidence

See `evidence.json` (written at seal time).
