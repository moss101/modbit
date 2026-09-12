# Task Card — M6.7 Durable subagent continuation (background child survives restart)

## Identity

- Task ID: M6.7
- Milestone: M6 Subagents/fleet (P0→P1)
- Requirement: docs/25 "Subagent continuation"; docs/14 "Transactional subagent admission" / "Compound execution and interruption"; docs/43 M6.7; owner label: core; subsystem: `services/modbit-core/src/runtime.rs` (`reconcile_after_restart`, `agent_nodes_suspended`, `Runtime::start`), `services/modbit-core/src/spawn.rs` (`resume_child`, `resume_suspended_children`, step 1 reattach)
- Qualification: M6.7 — kill the Core mid-child run; the child's identity, lineage, event offsets and result envelope survive.
- Evidence tier: real-system (a real Core process killed with `SIGKILL` while a real child agent's model request is open; a second Core process on the same data directory; the scripted OpenAI-compatible model server)

## Goal

A background child outlives the Core that started it: on restart both the parent and the child suspend at their boundaries and their agent nodes wait; resuming the parent resumes the child on its own log — the same agent id, child task, capsule and run, a fresh capacity ticket — and the child's result envelope reaches the parent, whose re-entered `agent.wait` collects it. Nothing runs twice.

## Existing-code audit

- classification: PARTIAL before this task: startup reconciliation (M4.x) suspended every task left `Running` and marked orphaned calls, the parent's and the child's alike, but the AgentGraph stayed `BACKGROUND` / `RUNNING` and nothing brought a suspended child back — a resumed parent's `agent.wait` timed out against a child no process owned; a child admitted without a tool budget was refused its first kernel call (`max_tool_calls:0/0`).
- production entry points: `runtime.rs::reconcile_after_restart` → `agent_nodes_suspended` (the task's `PRIMARY` and, for a subagent's task, its `SUBAGENT` node on the parent's graph → `WAITING`, reason "Core restarted; the run is suspended at its boundary"); `Runtime::start` (a resumed non-subagent task spawns `spawn::resume_suspended_children`); `spawn::resume_child` (a `Waiting` child with a `Suspended` run restarts through `Runtime::start_boxed` under the resuming lease with the capsule's budgets, then `AgentNodeTransitioned WAITING → BACKGROUND | RUNNING`); `spawn` step 1 (the same idempotency key on a suspended child reattaches and resumes it); the child's default tool budget.
- proof: `m6_7_a_background_child_survives_a_core_restart_and_hands_its_result_back` (`services/modbit-core/tests/surface_protocol.rs`): parent plans, spawns `child-a` (`BACKGROUND`), waits; the child's second model request is held open and the Core is killed; the second Core reports parent and child `Waiting / External / Suspended`, every node `WAITING`; `StartTask` on the parent (a new lease generation) resumes it and the child: one `SubagentAdmitted`, one `SubagentResultRecorded` with the same `agent_id` and `child_task_id`, `COMPLETED`, summary and `artifacts: [src/a/a.txt]`, the file in the child's worktree; the node's transitions `BACKGROUND → WAITING → BACKGROUND → COMPLETED` with the same `capsule_ref`; the child's log has one `RunCreated`, one `RunSuspended`, one `RunResumed`, a `CapacityTicketGranted` before the resume, and exactly one `change.apply` proposed; the parent's resumed requests carry the earlier tool results (no second spawn). The desktop reducer renders the transitions through M6.6's `AgentNodeTransitioned` arm (parent card agents count, nested child state).

## Limitations

- Children resume with their parent, or on a repeated `agent.spawn` with the same key; a child is not resumed on its own by `StartTask` from a client (its origin is `subagent`; the parent owns it).
- The WorkGraph dependency analysis that decides whether a child may run in the background at all (docs/25) is not enforced at admission: `BACKGROUND` is the spawn's default and the parent's `agent.wait` bounds the dependency.
- A child that finds no capacity on resume stays `WAITING` (its task `Waiting(Capacity)`); the parent's `agent.wait` reports it and the next parent resume retries.

## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`):

- `m6_7_a_background_child_survives_a_core_restart_and_hands_its_result_back`
- `m6_3_subagents_are_admitted_transactionally_and_hand_typed_results_back` (the explorer's read now runs on the default tool budget)

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json copy alongside
