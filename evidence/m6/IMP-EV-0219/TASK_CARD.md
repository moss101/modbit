# Task Card — IMP-EV-0219 No nested agent for certain profiles

## Identity

- Task ID: IMP-EV-0219
- Milestone: M6 Subagents/fleet (P0→P1)
- Requirement: REQ-EV-0219; owner label: Agent Runtime; subsystem: core-runtime
- Qualification: QUAL-EV-0219 — Leaf profile lacks spawn capability.
- Evidence tier: real-system (the real Core against a scripted OpenAI-compatible server, real git worktrees, an injected admission fault)

## Goal

Certain profiles cannot spawn nested agents.

## Existing-code audit

- classification: MISSING before.
- production entry points: `runtime.rs` projection: `agent.*` tools are projected only when the harness holds no capsule (a child is a leaf); `agent_tools::handle_spawn` refuses `NESTING_DISABLED` from a capsule.
- proof: the children's request bodies carry no `agent.spawn`, `agent.wait`, `agent.result` or `agent.cancel` tool.

## Limitations

- Profile-level (rather than depth-level) leaf declarations arrive with profile bundles (IMP-EV-0241).

## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`):

- `m6_3_subagents_are_admitted_transactionally_and_hand_typed_results_back`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json copy alongside
