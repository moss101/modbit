# Task Card — IMP-EV-0145 Task→branch/environment isolation

## Identity

- Task ID: IMP-EV-0145
- Milestone: M6 Subagents/fleet (P0→P1)
- Requirement: REQ-EV-0145; owner label: Agent Runtime; subsystem: core-runtime
- Qualification: QUAL-EV-0145 — Parallel tasks prove isolation across every bound resource.
- Evidence tier: real-system (the real Core with real git worktrees and branches, capability leases and per-task logs)

## Goal

Bind worktree, sandbox, context, lease, credentials, capability snapshot and evidence namespace.

## Existing-code audit

- classification: PRESENT since M2.5 / M4.3 / M6.3: a task binds its worktree and branch (a fork or a child gets its own), its capability lease (resources, operations, effect ceiling snapshotted at grant), its context (a fork carries only what it is told; a child carries nothing), its credentials (the binding endpoint/model on its agent node; the provider key stays in the Core's memory), and its evidence namespace (its own task log and object refs). No code change in this task.
- production entry points: `services/modbit-core/src/branch.rs` (fork: worktree, branch, lease minted for the profile at the root, carry set), `spawn.rs` (children: worktree, branch, lease narrowed to the write scope, capsule), `crates/policy` (lease snapshot), `crates/event-store` (per-task aggregates and projections).
- proof: two builders admitted at the same time write in distinct worktrees on distinct branches under distinct leases (each `fs.write` narrowed to its scope, no `git.worktree`), neither sees the other's file, and both branches merge into main; a fork carries decisions and evidence but no stale pending effect and runs on its own worktree and lease; a child's evidence refs and result envelope live on its own log and are handed to the parent by reference.

## Limitations

- Sandbox isolation is the worktree and the lease (processes run in the task's worktree under the same OS user); credential isolation is per binding, not per OS principal.

## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`):

- `m6_3_subagents_are_admitted_transactionally_and_hand_typed_results_back`
- `qual_ev_0077_0122_a_fork_carries_decisions_and_evidence_but_no_stale_pending_effect`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json copy alongside
