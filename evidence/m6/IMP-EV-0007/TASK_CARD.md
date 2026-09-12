# Task Card — IMP-EV-0007 Idempotent subagent spawn

## Identity

- Task ID: IMP-EV-0007
- Milestone: M6 Subagents/fleet (P0→P1)
- Requirement: REQ-EV-0007; owner label: Agent Runtime; subsystem: core-runtime
- Qualification: QUAL-EV-0007 — Replay identical spawn call after transport retry; exactly one child exists.
- Evidence tier: real-system (the real Core against a scripted OpenAI-compatible server, real git worktrees, an injected admission fault)

## Goal

Spawn carries idempotency key; replay reattaches rather than duplicating work.

## Existing-code audit

- classification: MISSING before.
- production entry points: `spawn.rs` step 1 (the key names an admitted SUBAGENT node → `reattached: true` with the same child, capsule, ticket, worktree and branch); the `agent_nodes` projection's unique `(task_id, idempotency_key)` index.
- proof: the parent calls `agent.spawn` twice with `child-a`: the second reports `reattached: true`, one `AgentNodeCreated` for that key, one admission, one worktree.

## Limitations

- A key reused after the child ended reattaches to the ended child (REQ-EV-0050's resume-as-new-attempt is M6.5+).

## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`):

- `m6_3_subagents_are_admitted_transactionally_and_hand_typed_results_back`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json copy alongside
