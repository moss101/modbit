# Task Card — IMP-EV-0218 Isolated coder/explore/plan subagents

## Identity

- Task ID: IMP-EV-0218
- Milestone: M6 Subagents/fleet (P0→P1)
- Requirement: REQ-EV-0218; owner label: Agent Runtime; subsystem: core-runtime
- Qualification: QUAL-EV-0218 — Explore child cannot mutate; coder child mutation requires worktree/capability.
- Evidence tier: real-system (the real Core against a scripted OpenAI-compatible server, real git worktrees, an injected admission fault)

## Goal

Coder, explore and plan subagents run isolated with their own tool ceilings.

## Existing-code audit

- classification: MISSING before.
- production entry points: the capsule's `required_tools` narrowing the child's projection; the builder's worktree, branch and narrowed lease; the harness write gate.
- proof: the explorer (read tools only) is refused when it tries `change.apply` and changes nothing; each builder mutates only its own worktree under its lease and scope.

## Limitations

- A dedicated plan-mode child profile is IMP-EV-0117.

## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`):

- `m6_3_subagents_are_admitted_transactionally_and_hand_typed_results_back`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json copy alongside
