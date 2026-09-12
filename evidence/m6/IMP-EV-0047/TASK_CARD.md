# Task Card — IMP-EV-0047 Persisted AgentNode

## Identity

- Task ID: IMP-EV-0047
- Milestone: M6 Subagents/fleet (P0→P1)
- Requirement: REQ-EV-0047; owner label: Agent Runtime; subsystem: core-runtime
- Qualification: QUAL-EV-0047 — Core restart preserves child status, task, tool cursor and private context refs.
- Evidence tier: real-system (a real Core process killed with SIGKILL mid-child and a second process on the same data directory, against a scripted OpenAI-compatible server)

## Goal

Agent state and lineage are durable independent of process.

## Existing-code audit

- classification: PARTIAL before: nodes were projected from the log (M6.1) and survived restart unchanged, but a child's status after restart was stale (RUNNING/BACKGROUND with no process) and its tool cursor was not re-entered by anything.
- production entry points: `crates/event-store` `agent_nodes` projection (M6.1); `runtime.rs::agent_nodes_suspended` (status follows the suspension); `spawn::resume_child` (status returns with the run); the harness re-enters the child's dangling calls by id (docs/19 layer 2).
- proof: after the kill every node is WAITING (parent primary, child node on the parent, child primary); after the resume the child's node runs BACKGROUND then COMPLETED with the same `capsule_ref`; the child's write ran exactly once (the re-entered cursor), its plan and capsule (private context refs, tools, budgets) rebuilt from its own log; M6.1 pins nodes and work graph across compaction and restart.

## Limitations

- Private context refs are carried in the capsule object; none are exercised beyond the empty set today.

## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`):

- `m6_7_a_background_child_survives_a_core_restart_and_hands_its_result_back`
- `m6_1_work_graph_and_primary_agent_survive_compaction_and_restart_unchanged`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json copy alongside
