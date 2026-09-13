# Task Card — IMP-EV-0008 Foreground ↔ background transition

## Identity

- Task ID: IMP-EV-0008
- Milestone: M6 Subagents/fleet (P0→P1)
- Requirement: REQ-EV-0008; owner label: Agent Runtime; subsystem: core-runtime
- Qualification: QUAL-EV-0008 — Move real running child background then foreground without restart or lost event offset.
- Evidence tier: real-system (the real Core against a scripted OpenAI-compatible server; real children in their own worktrees)

## Goal

Preserve agent identity while changing scheduling/attention mode.

## Existing-code audit

- classification: MISSING before: a child's attention mode was fixed at admission.
- production entry points: `services/modbit-core/src/agent_tools.rs::handle_attend` (`agent.attend`, projected to primaries; `AgentNodeTransitioned` BACKGROUND ↔ RUNNING with the run id and the child's last offset in the result; refused `NOT_RUNNING` for a child that is not running), `runtime.rs` dispatch, `crates/tools/tool-matrix.json`, docs/17.
- proof: a slow child is moved BACKGROUND → RUNNING → BACKGROUND by two `agent.attend` calls while mid-run, then ends COMPLETED: its node's transitions are exactly ADMITTED→BACKGROUND, BACKGROUND→RUNNING, RUNNING→BACKGROUND, BACKGROUND→COMPLETED; its own log has one run, no suspension, no resume, and the offsets reported by the two controls did not go backwards; its write landed and its envelope was collected.

## Limitations

- Attention mode is a node-level fact for the parent and the Fleet; it does not change the child's budgets or priority.

## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`):

- `qual_ev_0008_0180_scheduling_follows_the_work_graph_and_attention_moves_without_a_restart`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json copy alongside
