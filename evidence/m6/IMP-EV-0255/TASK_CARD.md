# Task Card — IMP-EV-0255 One primary responsible agent

## Identity

- Task ID: IMP-EV-0255
- Milestone: M6 Subagents/fleet (P0→P1)
- Requirement: REQ-EV-0255; owner label: core-runtime; subsystem: core-runtime
- Qualification: QUAL-EV-0255 — Typical task creates one primary; unnecessary swarm is not spawned.
- Evidence tier: real-system (the real Core against a scripted OpenAI-compatible server, a real compaction epoch, a hard kill and restart)

## Goal

Default one primary agent owns task outcome; delegate only separable bounded work.

## Existing-code audit

- classification: MISSING before: no agent identity existed to be one of.
- production entry points: `services/modbit-core/src/agents.rs` `primary_running` (created once per task, keyed by the task id, on the first run; moved through `RUNNING`/`WAITING`/`COMPLETED`/`ADMITTED` as runs end and resume) and `primary_transition`; `GetAgentGraph`.
- proof: a typical task through Core creates exactly one `PRIMARY` node and no other agent; the same node is the one running after a restart and a resume; the EPR-006 continuation spawns no second agent.

## Limitations

- Delegation of separable work (subagents) arrives with M6.3–M6.5; until then the primary is the only worker by construction.

## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`):

- `m6_1_work_graph_and_primary_agent_survive_compaction_and_restart_unchanged`
- `qual_epr_006_a_quality_rejection_continues_the_run_on_the_prevalidated_stronger_slot`
- `agent_status_transitions_are_the_documented_ones`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json copy alongside
