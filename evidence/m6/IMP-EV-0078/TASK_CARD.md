# Task Card — IMP-EV-0078 Child-agent isolation

## Identity

- Task ID: IMP-EV-0078
- Milestone: M6 Subagents/fleet (P0→P1)
- Requirement: REQ-EV-0078; owner label: Agent Runtime; subsystem: core-runtime
- Qualification: QUAL-EV-0078 — Child prompt dump lacks parent-only memory and secrets.
- Evidence tier: real-system (the real Core against a scripted OpenAI-compatible server, real git worktrees, an injected admission fault)

## Goal

Children receive bounded capsule, not unrestricted parent transcript/authority.

## Existing-code audit

- classification: MISSING before.
- production entry points: `branch.rs` `SubagentFork` (carry nothing: no plan, decisions, evidence or context of the parent); the child's transcript starts from its own goal and capsule; provider credentials stay Gateway-owned as for every run.
- proof: every child request body lacks the parent's goal marker and the parent's tool results; the child's tools are its capsule's.

## Limitations

- Same as IMP-EV-0048.

## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`):

- `m6_3_subagents_are_admitted_transactionally_and_hand_typed_results_back`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json copy alongside
