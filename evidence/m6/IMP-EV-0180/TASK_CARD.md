# Task Card — IMP-EV-0180 Default background delegation policy

## Identity

- Task ID: IMP-EV-0180
- Milestone: M6 Subagents/fleet (P0→P1)
- Requirement: REQ-EV-0180; owner label: Agent Runtime; subsystem: core-runtime
- Qualification: QUAL-EV-0180 — Dependency-sensitive scheduler keeps blocking child foreground and separable child background.
- Evidence tier: real-system (the real Core against a scripted OpenAI-compatible server; real children in their own worktrees)

## Goal

Background only when dependency graph says parent can progress independently; never blind default.

## Existing-code audit

- classification: PARTIAL before: `mode` was the spawn's word with BACKGROUND as the default, whatever the work graph said.
- production entry points: `crates/domain/src/agent.rs::WorkGraph::blocks_parent`; `services/modbit-core/src/spawn.rs` (the mode in force from `(requested, blocking)`, recorded in the capsule, the node transition and `SubagentAdmitted.scheduling`); `agent_tools::handle_spawn` (the foreground wait follows the mode in force; the result names the scheduling).
- proof: the parent's plan has b depending on a and c standing alone; `child-a` (asked BACKGROUND) owns a: admitted FOREGROUND / BLOCKING and the spawn's own tool result carries the finished envelope; `child-c` owns c: admitted BACKGROUND / SEPARABLE and collected later with `agent.wait`.

## Limitations

- The dependency read is the parent's own unowned pending work; a child blocked by another child's node is separable from the parent's point of view.

## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`):

- `qual_ev_0008_0180_scheduling_follows_the_work_graph_and_attention_moves_without_a_restart`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json copy alongside
