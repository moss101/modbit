# Task Card — M6.3 transactional subagent admission

## Identity

- Task ID: M6.3
- Milestone: M6 Subagents/fleet (P0→P1)
- Requirement: docs/14 "Transactional subagent admission", "Decomposition"; docs/43 M6.3; owner label: core-runtime; subsystem: core-runtime (`services/modbit-core/src/spawn.rs`, `agent_tools.rs`, `branch.rs`, harness write gate)
- Qualification: M6.3 / E2E-009 — a task proposes two builders with overlapping write sets: the conflict check blocks unsafe concurrent admission; no two workers mutate the same paths concurrently; E2E-010 — two disjoint changes are admitted with separate worktrees and capabilities, results return to the parent, deterministic merge succeeds.
- Evidence tier: real-system (the real Core against a scripted OpenAI-compatible server, real git worktrees, an injected admission fault)

## Goal

Admission is one transaction — capacity ticket, parent active at the expected generation, no unsafe write-set conflict with admitted workers, worktree or snapshot allocated, least-privilege capability lease, resources within quota, AgentGraph node and WorkGraph ownership persisted — and if any step fails no worker starts and no partial reservation leaks.

## Existing-code audit

- classification: MISSING before: the Fast Context specialist was the only sub-run and it shared the parent's task; nothing admitted a child with its own task, worktree, lease and ticket, and `agent.*` were inventory rows without a tool.
- production entry points: `services/modbit-core/src/spawn.rs` (`spawn`: the eight-step transaction with typed refusals per stage and rollback of ticket, worktree and child task; `latest_admission`; `record_result`); `services/modbit-core/src/agent_tools.rs` (`agent.spawn` / `agent.wait` / `agent.result` / `agent.cancel` harness tools, projected to primaries only); `services/modbit-core/src/branch.rs` (`SubagentFork`: carry nothing, `TaskOrigin::Subagent`, lease narrowed to the write scope, no `git.worktree`, effect ceiling capped); `crates/domain/src/agent.rs` (`SubtaskSpec`, `SpawnMode`, `AgentExecutionCapsule`, `SubagentResult`, `write_scopes_overlap`, `AgentNode.child_task_id`); `crates/domain/src/task.rs` (`SubagentAdmitted`, `SubagentAdmissionRefused`, `SubagentCapsuleBound`, `SubagentResultRecorded`); `crates/core-runtime/src/harness.rs` (`WriteScopeDenied`, `HarnessState.write_scope` / `capsule` / `completion_summary`); `services/modbit-core/src/runtime.rs` (the capsule rebuilt from the child's log, projection narrowed to the capsule's tools, admission tickets honoured at start, the handoff at the loop's end); `crates/tools/tool-matrix.json` (the `agent` family served).
- proof: through Core, a primary delegates two builders with disjoint write scopes and an explorer with read tools only: each is admitted as one transaction (capacity ticket, parent liveness, write-set check, checkpoint + fork into its own worktree and branch, a lease narrowed to `fs.write:<worktree>/<scope>/**` with no `git.worktree` and the ceiling `REVERSIBLE_WRITE`, the SUBAGENT node under the primary owning its work node, `SubagentAdmitted`), runs inside its capsule — its first request carries its own objective and the capsule note and nothing of the parent's goal, and no `agent.*` tool — and hands a typed result back (`SubagentResultRecorded` with status, summary, artifacts, verification-run evidence, branch); a retried spawn with the same key reattaches (three nodes for four spawn calls); a builder whose scope overlaps is refused `WRITE_CONFLICT` at stage `WRITE_SET` with nothing taken; a child's write outside its scope is refused `WRITE_SCOPE_DENIED` before any effector even though its own plan names the path; the explorer's write is refused as not projected; the two builder branches merge cleanly into main with both files; every child ticket returns to the pool. With `MODBIT_FAULT_SPAWN=WORKTREE` the admission fails after the worktree exists and rolls back — worktree removed, child task cancelled, ticket released — leaving no node, no worktree directory and no ticket. With `MODBIT_AGENT_MAX_DEPTH=0` a spawn is refused `NESTING_DISABLED` at stage `DEPTH` with only the parent's own ticket ever granted.

## Limitations

- The write-set check is explicit path/glob overlap; symbol ownership, dependency hot spots, migrations, generated files and lockfiles are M6.4. A child that reaches a protected effect is refused by its lease ceiling rather than escalated to the parent's attention (REQ-EV-0046's transition arrives with M6.6). Durable continuation of a child across a Core restart (REQ-EV-0006 / 0220 / 0263) is M6.7: today a killed Core suspends the child like any run and the parent's `agent.wait` sees `WAITING`.

## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`):

- `m6_3_subagents_are_admitted_transactionally_and_hand_typed_results_back`
- `write_scopes_overlap_on_prefix_and_glob_but_not_on_disjoint_paths`
- `agent_status_transitions_are_the_documented_ones`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json copy alongside
