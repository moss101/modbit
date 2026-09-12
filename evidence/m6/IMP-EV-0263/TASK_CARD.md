# Task Card — IMP-EV-0263 Background tasks

## Identity

- Task ID: IMP-EV-0263
- Milestone: M6 Subagents/fleet (P0→P1)
- Requirement: REQ-EV-0263; owner label: Agent Runtime; subsystem: core-runtime
- Qualification: QUAL-EV-0263 — Client disconnect/restart does not lose background task.
- Evidence tier: real-system (a real Core process killed with SIGKILL mid-child and a second process on the same data directory, against a scripted OpenAI-compatible server)

## Goal

Detached work uses durable task handles, permission ceiling, checkpoint/replay.

## Existing-code audit

- classification: PARTIAL before: a background child had a durable task handle, a capped effect ceiling (REVERSIBLE_WRITE) and a checkpoint before its admission (M6.3), but was lost to a Core restart.
- production entry points: `spawn.rs` (checkpoint `before_spawn`, effect ceiling cap, durable child task), `runtime.rs` (restart reconciliation, resume; dangling calls replayed by id), the desktop supervisor restarting the Core and rebuilding the board from the Core (M6.6 E2E).
- proof: the client disconnects and the Core is killed mid-child; the child is suspended, not lost: the second Core lists it Waiting/External/Suspended with its node WAITING, and the parent's resume brings it back to completion under the same ceiling; the packaged app restarts and rebuilds the parent card with the child nested from the Core.

## Limitations

- A client disconnect alone does not suspend a headless Core's work; the desktop-supervised Core exits with its client and resumes on the next start.

## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`):

- `m6_7_a_background_child_survives_a_core_restart_and_hands_its_result_back`
- `fleet: a delegating parent shows its phase, agents and the nested child; the child is never a top-level card (apps/desktop/e2e/fleet-agents.spec.ts)`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json copy alongside
