# Task Card — IMP-EV-0220 Resumable per-agent state

## Identity

- Task ID: IMP-EV-0220
- Milestone: M6 Subagents/fleet (P0→P1)
- Requirement: REQ-EV-0220; owner label: Agent Runtime; subsystem: core-runtime
- Qualification: QUAL-EV-0220 — Restart and resume child with same lineage.
- Evidence tier: real-system (a real Core process killed with SIGKILL mid-child and a second process on the same data directory, against a scripted OpenAI-compatible server)

## Goal

Persist child state independent of parent transcript.

## Existing-code audit

- classification: PARTIAL before: the child ran on its own task log (M6.3, carry-nothing fork) but was not resumable after a restart.
- production entry points: `services/modbit-core/src/spawn.rs` (`resume_child`, `resume_suspended_children`, step 1 reattach-resume); `runtime.rs` (`Runtime::start` for a resumed parent).
- proof: the child's state (plan, transcript, capsule, node) lives on the child's log, not the parent's: the parent resumes with its earlier tool results only (no second spawn) while the child resumes separately with `TaskResumed` / `RunResumed` on its own log and the same lineage (`parent_agent_id`, `root_agent_id`, `child_task_id`).

## Limitations

- See M6.7.

## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`):

- `m6_7_a_background_child_survives_a_core_restart_and_hands_its_result_back`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json copy alongside
