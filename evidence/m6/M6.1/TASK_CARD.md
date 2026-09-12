# Task Card — M6.1 WorkGraph/AgentGraph projections

## Identity

- Task ID: M6.1
- Milestone: M6 Subagents/fleet (P0→P1)
- Requirement: docs/14 "One runtime, three explicit graphs"; docs/13 "Agent node"; owner label: core-runtime; subsystem: core-runtime (services/modbit-core agents, crates/domain agent, crates/event-store projections)
- Qualification: M6.1 — the WorkGraph and the AgentGraph exist as projections of the task's log, served to clients, kept outside the transcript, unchanged by compaction and restart.
- Evidence tier: real-system (the real Core against a scripted OpenAI-compatible server, a real compaction epoch, a hard kill and restart)

## Goal

WorkGraph (tasks/subtasks, dependencies, artifacts, verification gates) and AgentGraph (which logical agents own which work) as projections coordinated by core-runtime, with no second orchestration service.

## Existing-code audit

- classification: MISSING before: the plan was a flat artifact (outcome, expected files, verification) inside the harness state; nothing represented subtasks with dependencies or status, and no logical agent identity existed — the Fast Context specialist ran as an actor string, and a run's model binding was the only notion of who was working.
- production entry points: `crates/domain/src/agent.rs` (`AgentNode`, `AgentKind`, `AgentStatus` with the documented transitions, `AgentBinding`, `WorkNode`, `WorkStatus`, `WorkNodeChange`, `WorkGraph::apply` validating a change whole); `crates/domain/src/task.rs` (`AgentNodeCreated`, `AgentNodeTransitioned`, `AgentBindingChanged`, `WorkNodesChanged`); `crates/event-store` V14 `agent_nodes` / `work_nodes` projections and loaders; `services/modbit-core/src/agents.rs` (the primary agent's lifecycle at run start, resume and end; child nodes for the specialist; rebinding on a continuation); `services/modbit-core/src/runtime.rs` (`plan.update` `steps` through the WorkGraph, `harness_state.work`, rebuild from the projection); `services/modbit-core/src/subagent.rs` (the specialist as a node); `services/modbit-core/src/server.rs` (`GetWorkGraph`, `GetAgentGraph`); `apps/cli` (`task work`, `task agents`); TS protocol regenerated.
- proof: through Core, a plan with three dependent steps is recorded; a step marked done ahead of its dependency is refused `WORK_GRAPH_INVALID` and records no plan version; a step done with evidence unlocks its dependent; the transcript compacts during the run and the graph is unchanged; a hard kill and restart rebuild `GetWorkGraph` and `GetAgentGraph` byte-identical; the resumed model's first request carries the graph in its harness state; the task has exactly one primary agent, created once on its first run with the run and the binding, `WAITING`/`COMPLETED` when the run ends, `RUNNING` again on resume with the same identity; on the EPR-006 continuation the primary keeps its identity and `AgentBindingChanged` records the move to the stronger model.

## Limitations

- Ownership of work nodes by agents (`owner`, `owns`) is recorded but nothing assigns it until admission (M6.3); the WorkGraph has no scheduler — readiness is derived, execution is still the primary's loop; the specialist is the only child node until M6.3.

## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`):

- `m6_1_work_graph_and_primary_agent_survive_compaction_and_restart_unchanged`
- `nodes_follow_their_dependencies_and_done_needs_evidence`
- `unknown_dependencies_and_cycles_are_refused`
- `qual_epr_006_a_quality_rejection_continues_the_run_on_the_prevalidated_stronger_slot`
- `agent_status_transitions_are_the_documented_ones`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json copy alongside
