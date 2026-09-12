# Task Card — IMP-EV-0051 Bounded recursive delegation

## Identity

- Task ID: IMP-EV-0051
- Milestone: M6 Subagents/fleet (P0→P1)
- Requirement: REQ-EV-0051; owner label: Agent Runtime; subsystem: core-runtime
- Qualification: QUAL-EV-0051 — Depth N+1 launch rejected with typed admission failure.
- Evidence tier: real-system (the real Core against a scripted OpenAI-compatible server, real git worktrees, an injected admission fault)

## Goal

Nested delegation disabled by default; explicit max depth and capacity enforced transactionally.

## Existing-code audit

- classification: MISSING before.
- production entry points: `spawn.rs` step 2 (`MODBIT_AGENT_MAX_DEPTH`, default 1: `NESTING_DISABLED` / `DEPTH_EXCEEDED` at stage `DEPTH`); children see no `agent.*` tool and `agent.spawn` from a capsule is refused `NESTING_DISABLED`.
- proof: with `MODBIT_AGENT_MAX_DEPTH=0` the primary's spawn is refused `NESTING_DISABLED` at stage `DEPTH` with nothing taken; children's projections carry no agent tool.

## Limitations

- Depth is a global setting; per-profile depth arrives with profile bundles (IMP-EV-0241).

## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`):

- `m6_3_subagents_are_admitted_transactionally_and_hand_typed_results_back`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json copy alongside
