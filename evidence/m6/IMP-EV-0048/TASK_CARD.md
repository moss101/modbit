# Task Card — IMP-EV-0048 AgentExecutionCapsule

## Identity

- Task ID: IMP-EV-0048
- Milestone: M6 Subagents/fleet (P0→P1)
- Requirement: REQ-EV-0048; owner label: Agent Runtime; subsystem: core-runtime
- Qualification: QUAL-EV-0048 — Child cannot access a parent-only secret/tool or hidden transcript.
- Evidence tier: real-system (the real Core against a scripted OpenAI-compatible server, real git worktrees, an injected admission fault)

## Goal

Per-agent context/tools/model policy/budgets/capability ceiling are explicit.

## Existing-code audit

- classification: MISSING before.
- production entry points: `crates/domain/src/agent.rs` `AgentExecutionCapsule` (spec, tools, binding, budgets, effect ceiling, worktree, branch, modalities, private context refs, mode) stored as an object and bound on the child's log (`SubagentCapsuleBound`); the harness rebuilds `write_scope` and `capsule` from it; the projection is narrowed to the capsule's tools; the fork's lease is narrowed and capped.
- proof: the child's first request carries its own objective and the capsule note and no parent goal; the explorer's capsule (`fs.read`, `fs.list`, `search.exact`) projects no write tool and no agent tool; the builder's lease writes only under its scope with `REVERSIBLE_WRITE`; a write outside the scope is `WRITE_SCOPE_DENIED`.

## Limitations

- Private context refs are empty until the parent can hand selected context to a child (M6.5 follow-on).

## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`):

- `m6_3_subagents_are_admitted_transactionally_and_hand_typed_results_back`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json copy alongside
