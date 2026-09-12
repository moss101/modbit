# Task Card — IMP-EV-0052 Structured PlanGraph/TodoState

## Identity

- Task ID: IMP-EV-0052
- Milestone: M6 Subagents/fleet (P0→P1)
- Requirement: REQ-EV-0052; owner label: WorkGraph; subsystem: core-runtime
- Qualification: QUAL-EV-0052 — Compaction and model restart cannot alter canonical plan state.
- Evidence tier: real-system (the real Core against a scripted OpenAI-compatible server, a real compaction epoch, a hard kill and restart)

## Goal

Plan nodes exist outside transcript with dependencies/owner/status/evidence/blockers.

## Existing-code audit

- classification: MISSING before: the plan held no nodes; only the transcript carried what the model thought it was doing.
- production entry points: `crates/domain/src/agent.rs` `WorkGraph` / `WorkNode` / `WorkNodeChange`; `plan.update` `steps` in `services/modbit-core/src/runtime.rs` (`WorkNodesChanged` on the log, `harness_state.work` every turn, rebuild from docs/31 `work_nodes`); `GetWorkGraph`.
- proof: the plan's steps with dependencies, owner slot, status, evidence and blockers are recorded outside the transcript; a compaction epoch during the run and a Core kill/restart leave `GetWorkGraph` identical, and the resumed model reads the nodes from its harness state rather than the rewritten transcript.

## Limitations

- Owner assignment waits for admission (M6.3).

## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`):

- `m6_1_work_graph_and_primary_agent_survive_compaction_and_restart_unchanged`
- `nodes_follow_their_dependencies_and_done_needs_evidence`
- `unknown_dependencies_and_cycles_are_refused`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json copy alongside
