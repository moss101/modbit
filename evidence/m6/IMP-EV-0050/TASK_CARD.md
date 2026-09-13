# Task Card — IMP-EV-0050 Resume terminal child states

## Identity

- Task ID: IMP-EV-0050
- Milestone: M6 Subagents/fleet (P0→P1)
- Requirement: REQ-EV-0050; owner label: Agent Runtime; subsystem: core-runtime
- Qualification: QUAL-EV-0050 — Resume completed research child; new result is a new attempt with prior evidence linked.
- Evidence tier: real-system (a real Core killed and restarted between a child's completion and the parent's follow-up, against a scripted OpenAI-compatible server)

## Goal

Policy may continue failed/cancelled/completed child with new prompt while preserving lineage.

## Existing-code audit

- classification: MISSING before: a finished child could only be replaced by a new spawn under a new key.
- production entry points: `services/modbit-core/src/spawn.rs::follow_up_child` (terminal branch: `TaskReturnedToWork` / `TaskWaiting(UserInput)` / `TaskInputQueued(FOLLOW_UP)` on the child, node terminal → `ADMITTED`, a fresh run through `Runtime::start_boxed` with the capsule's budgets, node → `BACKGROUND` | `RUNNING`), `spawn::record_result` (`prior_result:<ref>` and `attempt:<n>` in the new envelope's evidence refs), `agent_tools::handle_steer` (`agent.steer`), `crates/tools/tool-matrix.json`.
- proof: a research child completes (envelope 1: "README says: research me"); the parent sends `agent.steer` "now create src/a/a.txt": the child's node goes COMPLETED → ADMITTED → BACKGROUND → COMPLETED with no second admission, its task has two runs and one `TaskReturnedToWork`, the second model call carries the earlier report and the follow-up, the write lands, and envelope 2 links envelope 1 (`prior_result:<ref>`, `attempt:2`) on the same agent and child task.

## Limitations

- A follow-up to a `FAILED` / `CANCELLED` child reopens its task the same way (`TaskWaiting` + input); the reviewer's pending review of attempt 1 is superseded by attempt 2's.

## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`):

- `qual_ev_0050_0179_a_follow_up_continues_a_finished_child_as_a_new_attempt_on_its_lineage`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json copy alongside
