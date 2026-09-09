# Task Card — IMP-EV-0102 Thread state distinct from turn state

## Identity

- Task ID: IMP-EV-0102
- Milestone: M1
- Canonical owner: Domain Model / modbit-domain
- Requirement IDs: REQ-EV-0102; qualification QUAL-EV-0102
- Risk class: high (protocol / persistence / security boundary as applicable)
- Evidence tier: release-critical
- Release membership: ALPHA, BETA, RELEASE_ZERO

## Goal

Session, Task, Run, Turn and RunStep each have their own explicit state machine; a failed command step never fails the turn, run, task or session, and conflated transitions are rejected by the reducers.

## Non-goals

Behavior owned by later milestones (agent runtime, providers, checkpoints, cloud plane).

## Required reading

`AGENTS.md`, the `docs/40` row for REQ-EV-0102, `docs/13`, `docs/19`, `docs/30`, `docs/32`, `docs/33`, `docs/39`, `docs/81`.

## Existing-code audit

- classification: IMPLEMENTED-PARTIAL on the M1.1–M1.5 substrate before this task; this task adds the missing behavior and the named qualification test
- production entry point: `modbit-core` command handlers / desktop main IPC / domain reducers as applicable
- real effector/storage boundary: `core.db`, the real local socket, the real Electron app

## Verification

- qualification test IDs: QUAL-EV-0102 → modbit-domain tests/state_separation.rs: a_failed_command_step_does_not_fail_the_turn_the_run_the_task_or_the_session; every_machine_has_exactly_its_own_terminal_states
- E2E: hosted CI on macOS/Linux/Windows

## Completion evidence

See `evidence.json` (written at seal time).
