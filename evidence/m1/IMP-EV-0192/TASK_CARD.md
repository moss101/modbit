# Task Card — IMP-EV-0192 Daemon multi-client HTTP+SSE event model (ADAPT: local socket)

## Identity

- Task ID: IMP-EV-0192
- Milestone: M1
- Canonical owner: Core API / modbit-core
- Requirement IDs: REQ-EV-0192; qualification QUAL-EV-0192
- Risk class: high (protocol / persistence / security boundary as applicable)
- Evidence tier: release-critical
- Release membership: ALPHA, BETA, RELEASE_ZERO

## Goal

Any number of clients attach to one session over the authenticated local SurfaceProtocol and observe the same replayable event stream from a cursor; the desktop and the CLI are interchangeable clients; cloud HTTP+SSE arrives with M8 on the same envelope.

## Non-goals

Behavior owned by later milestones (agent runtime, providers, checkpoints, cloud plane).

## Required reading

`AGENTS.md`, the `docs/40` row for REQ-EV-0192, `docs/13`, `docs/19`, `docs/30`, `docs/32`, `docs/33`, `docs/39`, `docs/81`.

## Existing-code audit

- classification: IMPLEMENTED-PARTIAL on the M1.1–M1.5 substrate before this task; this task adds the missing behavior and the named qualification test
- production entry point: `modbit-core` command handlers / desktop main IPC / domain reducers as applicable
- real effector/storage boundary: `core.db`, the real local socket, the real Electron app

## Verification

- qualification test IDs: QUAL-EV-0192 → commands_subscription_resume_and_idempotent_replay_over_the_real_socket (two clients, live delivery, lossless resume); CLI smoke tails the stream the desktop writes
- E2E: hosted CI on macOS/Linux/Windows

## Completion evidence

See `evidence.json` (written at seal time).
