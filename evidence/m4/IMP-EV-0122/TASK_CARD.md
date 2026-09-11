# Task Card — IMP-EV-0122 Fork session

## Identity

- Task ID: IMP-EV-0122
- Milestone: M4 Durable recovery spine (P0)
- Requirement: REQ-EV-0122; owner label: Execution Timeline; subsystem: durability
- Qualification: `QUAL-EV-0122` — New branch gets selected decisions/evidence but no stale pending approval.
- Evidence tier: real-system (same run as IMP-EV-0077: the capsule is read back from the object store over the wire and checked field by field)

## Goal

Fork from a step/checkpoint with a `BranchCarryoverCapsule`: one content-addressed record of exactly what the fork took — the plan, the user's decisions, the retrieval evidence its worktree still matches, the source's context — and what it deliberately left: pending approvals and unfinished calls.

## Existing-code audit

- classification: NOT-FOUND before this batch.
- production entry points: `crates/checkpoint` `BranchCarryoverCapsule { schema_version, source_task_id, source_run_id, source_checkpoint_id, source_epoch, source_event_offset, fork_task_id, branch_generation, carried, plan, decisions, evidence, context, approvals_dropped, calls_dropped, worktree_files, git_head, created_at }` with `Carry` (`PLAN | DECISIONS | EVIDENCE | CONTEXT`, `ForkTask.carry`, empty = all); `services/modbit-core/src/branch.rs` `source_facts` (plan and revisions, answered questions, retrieval records, latest Context Pack) and the selection rule for evidence (a record is carried only when the fork's worktree has the bytes it names); `services/modbit-core/src/runtime.rs` `rebuild` reads `TaskForked` → the capsule → `harness_state.carried`, so the fork's model sees the decisions and is told the pending approvals were not carried; the plan and the valid evidence are re-recorded as the fork's own `PlanRecorded` / `RetrievalRecorded` (the retrieve-before-edit gate is satisfied by real records).
- proof: the capsule read back over `ReadObjectRange` names the source and fork task ids, the one decision (question and chosen option), exactly two evidence records — `b.txt` as read and `a.txt` as written — and not the read of `a.txt` the write made stale (the source holds three records), the plan's expected files, the dropped approval by id, and the worktree's file hashes; the fork's first model request carries the question, the answer and `approvals_dropped`; the fork's log carries `TaskForked`, the carried `PlanRecorded` and the carried `RetrievalRecorded` (`tool_name` `fork:carried`).

## Limitations

- Decisions are carried as facts in `harness_state`, not as transcript messages: a provider requires tool results to follow their calls, and the fork has no such calls.

## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`):

- `qual_ev_0077_0122_a_fork_carries_decisions_and_evidence_but_no_stale_pending_effect`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json copy alongside
