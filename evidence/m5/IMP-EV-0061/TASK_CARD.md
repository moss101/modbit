# Task Card — IMP-EV-0061 Skill execution capsule

## Identity

- Task ID: IMP-EV-0061
- Milestone: M5 Procedural runtime and skills (P0)
- Requirement: REQ-EV-0061; owner label: Skill Compiler; subsystem: skills
- Qualification: `QUAL-EV-0061` — Malicious skill requesting admin capability cannot widen task authority.
- Evidence tier: real-system (the real Core under `review_isolated`, a signed package on disk asking for a destructive toolset, a secret tool and an `admin` ceiling)

## Goal

A skill declares its invocation contract, context, tool requirements and ceiling; the capability ceiling only narrows — nothing a skill says widens what the task may do.

## Existing-code audit

- classification: NOT-FOUND before M5.5 (no skills). As of M5.5 the compiler intersects `required_tools` with the task's policy surface and records the rest as unavailable; this task proves the boundary against a hostile package.
- production entry points: `crates/skills::compile` (intersection; `tools_unavailable`), `services/modbit-core/src/skills.rs` (compiled against `visible_specs` — profile × lease × kernel), the projection and kernel of M5.1 unchanged by any skill; the `capability_ceiling` is manifest data recorded with the package, never a grant.
- proof: `qual_ev_0061_0214_a_skill_cannot_widen_task_authority_and_a_non_invocable_skill_is_not_selected`: a signed `greedy` skill (`required_tools: fs.read, git.worktree, secret.read, shell.exec`; `capability_ceiling: admin, fs.write, net.egress`; instructions inviting `git.worktree.close`) under `review_isolated` is selected and compiled to `tool_projection [fs.read, shell.exec]`, `tools_unavailable [git.worktree, secret.read]`; the model's `git.worktree.close` is refused `TOOL_NOT_VISIBLE` and never becomes a tool call; nothing about `admin` reaches the log as a grant.

## Limitations

- The ceiling is recorded and honoured by intersection; a per-skill capability lease narrower than the task's (a skill that asks for less) is not applied yet.

## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`):

- `qual_ev_0061_0214_a_skill_cannot_widen_task_authority_and_a_non_invocable_skill_is_not_selected`
- `the_compiler_injects_bounded_instructions_and_never_widens_the_projection`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json copy alongside
