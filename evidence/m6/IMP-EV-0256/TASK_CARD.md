# Task Card — IMP-EV-0256 Agent identity separate from persona/model

## Identity

- Task ID: IMP-EV-0256
- Milestone: M6 Subagents/fleet (P0→P1)
- Requirement: REQ-EV-0256; owner label: core-runtime; subsystem: core-runtime
- Qualification: QUAL-EV-0256 — Fallback model switch preserves agent/run identity and lineage.
- Evidence tier: real-system (the real Core against a scripted OpenAI-compatible server, a real compaction epoch, a hard kill and restart)

## Goal

AgentNode identity persists while model/profile may change under policy.

## Existing-code audit

- classification: MISSING before: the binding was the identity.
- production entry points: `AgentBinding` on the node; `services/modbit-core/src/agents.rs` `primary_rebound` (called by the EPR-006 quality boundary when the run continues on the stronger slot, and by `primary_running` when a resumed run's route differs); `AgentBindingChanged` on the log.
- proof: on the EPR-006 continuation the run moves from `openai/gpt-5-mini` to `openai/gpt-5`: one `AgentNodeCreated` for the task, one `AgentBindingChanged` (from mini to gpt-5, reason naming REQ-EPR-006), the same agent id and run before and after, `GetAgentGraph` showing the primary on the new binding.

## Limitations

- A provider fallback (EPR-007/route switch at a boundary) rebinds through the same function when it moves a live run; a run-start route change is recorded on resume.

## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`):

- `m6_1_work_graph_and_primary_agent_survive_compaction_and_restart_unchanged`
- `qual_epr_006_a_quality_rejection_continues_the_run_on_the_prevalidated_stronger_slot`
- `agent_status_transitions_are_the_documented_ones`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json copy alongside
