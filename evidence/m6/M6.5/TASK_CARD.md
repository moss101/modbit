# Task Card — M6.5 subagent result/evidence handoff

## Identity

- Task ID: M6.5
- Milestone: M6 Subagents/fleet (P0→P1)
- Requirement: docs/14 "Agent-to-agent communication"; docs/43 M6.5; owner label: core-runtime; subsystem: core-runtime (`services/modbit-core/src/spawn.rs` `record_result`, `agent_tools.rs`)
- Qualification: M6.5 / E2E-010 — results return to the parent as a typed `SubagentResult`; the parent's merge/verification proceeds from them.
- Evidence tier: real-system (the real Core; three children of one parent)

## Goal

Subagents return a structured `SubagentResult` (summary, evidence refs, artifacts, unresolved risks, proposed follow-ups) that is untrusted context until the parent selects it; a peer message never silently becomes durable memory.

## Existing-code audit

- classification: MISSING before.
- production entry points: `crates/domain/src/agent.rs` `SubagentResult`; `services/modbit-core/src/spawn.rs` `record_result` (called by the child's loop as it ends, whatever way: status, the completion summary, the paths changed in the child's worktree, its verification runs and gate, unresolved risks, branch and worktree, candidate revision; `SubagentResultRecorded` on the parent and the child's node transition); `agent_tools.rs` `agent.wait` (bounded) / `agent.result` (the envelope, labelled untrusted context) / `agent.cancel`.
- proof: three children end and each lands one `SubagentResultRecorded` on the parent with `COMPLETED`, the child's own summary, its artifacts (only what it actually wrote; the refused write is absent; the explorer has none), `verification:COMPLETION:` evidence and its branch; the parent's `agent.wait` returns each envelope with the untrusted-context note; the branches merge cleanly and the merged tree holds both files.

## Limitations

- Proposed follow-ups are empty until the child can name them in `task.complete` (a follow-on of the completion handshake).
- The parent merges through the Change Engine's git tools; the runtime does not merge on the child's behalf.

## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`):

- `m6_3_subagents_are_admitted_transactionally_and_hand_typed_results_back`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json copy alongside
