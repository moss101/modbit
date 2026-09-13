# Task Card — IMP-EV-0178 Specialized subagents with tool/model profiles

## Identity

- Task ID: IMP-EV-0178
- Milestone: M6 Subagents/fleet (P0→P1)
- Requirement: REQ-EV-0178; owner label: Agent Runtime; subsystem: core-runtime
- Qualification: QUAL-EV-0178 — Launch two real specialized children and verify independent state/tool ceilings.
- Evidence tier: real-system (the real Core against a scripted OpenAI-compatible server; real children)

## Goal

Agent profile selects bounded tools/model policy/domain context inside AgentExecutionCapsule.

## Existing-code audit

- classification: PRESENT since M6.3: `SubtaskSpec.required_tools` narrows the child's projection inside its capsule; the binding (endpoint/model) is the capsule's; state is the child's own task log. Declarative named profiles (IMP-EV-0115/0182/0241) are not part of this task. No code change in this task.
- production entry points: `spawn.rs` (capsule `tools`, `binding`, budgets), `runtime.rs` (projection narrowed to the capsule's tools when named; harness state carries the capsule), `crates/domain/src/agent.rs::AgentExecutionCapsule`.
- proof: an explorer child admitted with `required_tools: [fs.read, fs.list, search.exact]` is offered `fs.read` and neither `change.apply` nor `agent.spawn`, and its write is refused as not projected; two builders admitted at the same time each run on their own task, worktree, branch and lease with `fs.write` narrowed to their scope; the three capsules are distinct objects on the parent's AgentGraph.

## Limitations

- Per-child model selection is the parent's binding today; a profile-driven model policy arrives with IMP-EV-0115.

## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`):

- `m6_3_subagents_are_admitted_transactionally_and_hand_typed_results_back`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json copy alongside
