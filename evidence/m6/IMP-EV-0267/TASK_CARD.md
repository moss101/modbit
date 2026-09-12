# Task Card — IMP-EV-0267 Transactional subagent admission

## Identity

- Task ID: IMP-EV-0267
- Milestone: M6 Subagents/fleet (P0→P1)
- Requirement: REQ-EV-0267; owner label: Agent Runtime; subsystem: core-runtime
- Qualification: QUAL-EV-0267 — Injected failure during admission leaves no orphan child/worktree/capacity leak.
- Evidence tier: real-system (the real Core against a scripted OpenAI-compatible server, real git worktrees, an injected admission fault)

## Goal

Capacity, worktree/write-set, capability and child record admission commit atomically before child starts.

## Existing-code audit

- classification: MISSING before: no admission existed.
- production entry points: `services/modbit-core/src/spawn.rs` (`spawn`, `rollback_fork`, `rollback_ticket`, `fail_node`; `MODBIT_FAULT_SPAWN=<stage>` injects a failure after that stage's resource is taken).
- proof: with `MODBIT_FAULT_SPAWN=WORKTREE` the admission fails after the fork: the worktree is removed, the child task cancelled and the capacity ticket released (`SubagentAdmissionRefused` with `stage: WORKTREE` and the three rollback records); the worktrees directory is empty, the AgentGraph holds the primary only, the pool holds no ticket; the happy path admits three children whose tickets all return with their runs.

## Limitations

- One fault point is injected (after the worktree); the other rollback paths share the same functions.

## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`):

- `m6_3_subagents_are_admitted_transactionally_and_hand_typed_results_back`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json copy alongside
