# Task Card — IMP-EV-0006 Persisted child conversations

## Identity

- Task ID: IMP-EV-0006
- Milestone: M6 Subagents/fleet (P0→P1)
- Requirement: REQ-EV-0006; owner label: Agent Runtime; subsystem: core-runtime
- Qualification: QUAL-EV-0006 — Kill Core mid-child run; restart resumes exact child identity and parent linkage.
- Evidence tier: real-system (a real Core process killed with SIGKILL mid-child and a second process on the same data directory, against a scripted OpenAI-compatible server)

## Goal

Child AgentNode state is durable with parent/root lineage and its own execution capsule.

## Existing-code audit

- classification: PARTIAL before: the child's task, node and capsule were durable (M6.1/M6.3) but a restart left the node RUNNING/BACKGROUND with no owner and nothing resumed it.
- production entry points: `services/modbit-core/src/runtime.rs` (`reconcile_after_restart` → `agent_nodes_suspended`; `Runtime::start` → `spawn::resume_suspended_children`), `services/modbit-core/src/spawn.rs` (`resume_child`).
- proof: the Core is killed while the child's model request is open; the second Core moves the child's node (on the parent) and its primary to WAITING; resuming the parent resumes the child: one admission, one result envelope with the same `agent_id` and `child_task_id`, the same `capsule_ref` on the node, the child's conversation continuing on its own log (one run: suspended then resumed).

## Limitations

- A child is resumed with its parent or on a repeated spawn key, not on its own by a client.

## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`):

- `m6_7_a_background_child_survives_a_core_restart_and_hands_its_result_back`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json copy alongside
