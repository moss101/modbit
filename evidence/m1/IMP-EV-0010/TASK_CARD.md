# Task Card — IMP-EV-0010 Offset-key event resume

## Identity

- Task ID: IMP-EV-0010
- Milestone: M1
- Canonical owner: Event Protocol / core-runtime (modbit-core, modbit-protocol)
- Requirement IDs: REQ-EV-0010; qualification QUAL-EV-0010
- Risk class: high (protocol / persistence / security boundary as applicable)
- Evidence tier: release-critical
- Release membership: ALPHA, BETA, RELEASE_ZERO

## Goal

Every stored event carries a store-wide monotonic offset; SubscribeEvents(after_offset) replays exactly the events after it then streams live; a cursor beyond the log is refused with INVALID_CURSOR so clients rehydrate from GetSessionSnapshot (full-rehydrate fallback) instead of skipping.

## Non-goals

Behavior owned by later milestones (agent runtime, providers, checkpoints, cloud plane).

## Required reading

`AGENTS.md`, the `docs/40` row for REQ-EV-0010, `docs/13`, `docs/19`, `docs/30`, `docs/32`, `docs/33`, `docs/39`, `docs/81`.

## Existing-code audit

- classification: IMPLEMENTED-PARTIAL on the M1.1–M1.5 substrate before this task; this task adds the missing behavior and the named qualification test
- production entry point: `modbit-core` command handlers / desktop main IPC / domain reducers as applicable
- real effector/storage boundary: `core.db`, the real local socket, the real Electron app

## Verification

- qualification test IDs: QUAL-EV-0010 → qual_ev_0010_offset_resume_is_exact_and_invalid_cursors_force_rehydrate (real Core, real socket); commands_subscription_resume_and_idempotent_replay_over_the_real_socket; desktop main reconnects and re-reads the snapshot on INVALID_CURSOR
- E2E: hosted CI on macOS/Linux/Windows

## Completion evidence

See `evidence.json` (written at seal time).
