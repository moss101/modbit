# Task Card — IMP-EV-0120 Todo/tasklist

## Identity

- Task ID: IMP-EV-0120
- Milestone: M6 Subagents/fleet (P0→P1)
- Requirement: REQ-EV-0120; owner label: WorkGraph; subsystem: core-runtime
- Qualification: QUAL-EV-0120 — Restart preserves task statuses independent of chat compaction.
- Evidence tier: real-system (the real Core against a scripted OpenAI-compatible server, a real compaction epoch, a hard kill and restart)

## Goal

Durable dependency task graph with evidence and attempts.

## Existing-code audit

- classification: MISSING before: no task list existed beyond the plan's outcome sentence.
- production entry points: `WorkGraph::apply` (readiness from dependencies, `DONE` needs evidence and done dependencies, attempts counted on a return to `ACTIVE`), the `work_nodes` projection, `GetWorkGraph`, CLI `task work`.
- proof: statuses, evidence references and attempts survive a hard kill and restart unchanged while the transcript compacted; a refused change (done ahead of a dependency) records nothing.

## Limitations

- Insertion, deletion and reordering are changes to `steps`; there is no separate todo tool — the plan owns the list (docs/17).

## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`):

- `m6_1_work_graph_and_primary_agent_survive_compaction_and_restart_unchanged`
- `nodes_follow_their_dependencies_and_done_needs_evidence`
- `unknown_dependencies_and_cycles_are_refused`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json copy alongside
