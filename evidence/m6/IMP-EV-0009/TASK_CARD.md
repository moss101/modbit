# Task Card — IMP-EV-0009 Typed live steering

## Identity

- Task ID: IMP-EV-0009
- Milestone: M6 Subagents/fleet (P0→P1)
- Requirement: REQ-EV-0009; owner label: Agent Runtime; subsystem: core-runtime
- Qualification: QUAL-EV-0009 — Steer during live model/tool cycle and verify deterministic cancellation boundary and replay.
- Evidence tier: real-system (the real Core against a scripted OpenAI-compatible server with a stalled stream)

## Goal

Steer/cancel/follow-up are typed control events, not chat conventions.

## Existing-code audit

- classification: PRESENT since M2.7 / QUAL-EV-0191: `QueueInput` (STEER | FOLLOW_UP | COLLECT), `CancelTask` and `TaskSteered` / `TaskInputQueued` are typed commands and events on the log; the loop applies them at safe boundaries and a STEER interrupts the in-flight stream. No code change in this task.
- production entry points: `services/modbit-core/src/server.rs` (`QueueInput`, `CancelTask`), `runtime.rs` (steer applied between steps; cancellation at the next safe boundary; the interrupted response is never applied), `crates/domain/src/task.rs` (`TaskSteered`, `TaskInputQueued`, `TaskCancelRequested`).
- proof: steering lands between steps as a durable `TaskSteered` and reaches the model as a user message; cancellation interrupts the in-flight stream at a deterministic boundary; COLLECT inputs coalesce after the current turn, FOLLOW_UP inputs become ordered separate turns, a STEER interrupts the stream and nothing from the interrupted response is applied; the next turn replays from the steer.

## Limitations

- Steering a child agent goes through its parent (`agent.cancel`; follow-ups to a terminal child are IMP-EV-0050).

## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`):

- `m2_7_steering_and_cancellation_apply_at_safe_boundaries`
- `qual_ev_0191_steering_policy_interrupts_replaces_coalesces_and_orders`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json copy alongside
