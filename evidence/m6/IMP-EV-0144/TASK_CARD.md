# Task Card — IMP-EV-0144 Captain→Build separation

## Identity

- Task ID: IMP-EV-0144
- Milestone: M6 Subagents/fleet (P0→P1)
- Requirement: REQ-EV-0144; owner label: Agent Runtime; subsystem: core-runtime
- Qualification: QUAL-EV-0144 — Builder attempts out-of-scope file write and is denied.
- Evidence tier: real-system (the real Core against a scripted OpenAI-compatible server; real children in their own worktrees)

## Goal

Coordinator delegates typed bounded TaskContracts; builder cannot widen scope.

## Existing-code audit

- classification: PRESENT since M6.3 (docs/14 "Transactional subagent admission"): the coordinator is the primary, the contract is the `SubtaskSpec` (objective, artifacts, dependencies, scopes, tools, verification, budgets, work node), the builder is the child inside its `AgentExecutionCapsule`. No code change in this task: the audit names the production path and the qualification that already runs on every CI push.
- production entry points: `services/modbit-core/src/spawn.rs` (the spec becomes the capsule and the child's lease narrowed to its write scope), `crates/core-runtime/src/harness.rs::check_write` (`WriteScopeDenied` before any effector, even for a path the child's own plan names), `runtime.rs` (the child's projection narrowed to the capsule's tools).
- proof: builder `child-b` names `src/a/evil.txt` in its own plan and asks to write it: refused `WRITE_SCOPE_DENIED` before any effector (no `ToolCallProposed`), then writes `src/b/b.txt` inside its scope and completes; the explorer `child-e` sees no write tool at all. The coordinator collects typed envelopes and merges through the Change Engine — the builder never widened anything.

## Limitations

- A builder may propose a wider scope only through its result envelope; there is no in-flight scope negotiation.

## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`):

- `m6_3_subagents_are_admitted_transactionally_and_hand_typed_results_back`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json copy alongside
