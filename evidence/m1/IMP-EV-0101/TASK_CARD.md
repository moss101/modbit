# Task Card — IMP-EV-0101 Separate durable stores / restart-resume

## Identity

- Task ID: IMP-EV-0101
- Milestone: M1
- Canonical owner: Persistence / core-runtime
- Requirement IDs: REQ-EV-0101; qualification QUAL-EV-0101
- Risk class: high (protocol / persistence / security boundary as applicable)
- Evidence tier: release-critical
- Release membership: ALPHA, BETA, RELEASE_ZERO

## Goal

Event log, projections, command ledger and the content-addressed object directory are separate concerns in core.db and objects/; a hard-killed Core resumes pending (Queued) tasks from canonical state with no transcript inference.

## Non-goals

Behavior owned by later milestones (agent runtime, providers, checkpoints, cloud plane).

## Required reading

`AGENTS.md`, the `docs/40` row for REQ-EV-0101, `docs/13`, `docs/19`, `docs/30`, `docs/32`, `docs/33`, `docs/39`, `docs/81`.

## Existing-code audit

- classification: IMPLEMENTED-PARTIAL on the M1.1–M1.5 substrate before this task; this task adds the missing behavior and the named qualification test
- production entry point: `modbit-core` command handlers / desktop main IPC / domain reducers as applicable
- real effector/storage boundary: `core.db`, the real local socket, the real Electron app

## Verification

- qualification test IDs: QUAL-EV-0101 → kill_points_during_a_command_stream_never_duplicate_or_tear_state; accepted_commands_survive_a_hard_kill_of_the_core_and_a_second_instance_is_refused; startup_recovery_verifies_chains_rebuilds_lagging_projections_and_bumps_boot_generation
- E2E: hosted CI on macOS/Linux/Windows

## Completion evidence

See `evidence.json` (written at seal time).
