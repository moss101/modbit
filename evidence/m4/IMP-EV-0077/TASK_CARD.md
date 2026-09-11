# Task Card — IMP-EV-0077 Session branching

## Identity

- Task ID: IMP-EV-0077
- Milestone: M4 Durable recovery spine (P0)
- Requirement: REQ-EV-0077; owner label: Execution Timeline; subsystem: durability
- Qualification: `QUAL-EV-0077` — Fork produces independent revision lineage without copying invalid pending effects.
- Evidence tier: real-system (the real Core, a real git repository, a real worktree created for the fork, a hard kill between the source's pending approval and the fork)

## Goal

Fork from a checkpoint with worktree, evidence and context carryover: the fork is a new task with its own git worktree at the checkpoint's content and its own revision lineage, and nothing pending on the source comes with it.

## Existing-code audit

- classification: NOT-FOUND before this batch (`SessionBranched` existed as the branch-generation event of M4.2 with no producer; checkpoints could be restored in place, not forked).
- production entry points:
  - `services/modbit-core/src/branch.rs` `fork` — validates the checkpoint chain object by object, creates the branch `modbit/fork-<id>` at the checkpoint's HEAD and a worktree under `<profile>/worktrees/<task id>` (never inside the source repository), materializes the checkpoint's dirty state through the workspace service (`FileChanged` under `fork:` on the fork's own workspace aggregate), writes the `BranchCarryoverCapsule` object, appends `SessionBranched(fork)` to the session and `TaskCreated(origin fork)` / `TaskQueued` / `TaskForked` plus the carried `PlanRecorded` and `RetrievalRecorded` records to the fork, and grants the fork its own capability lease on its own root.
  - `crates/domain` `TaskOrigin::Fork`, `TaskEvent::TaskForked`; `crates/checkpoint` `BranchCarryoverCapsule` and the carry kinds.
  - `ForkTask` / `TaskForked` on the wire (`services/modbit-core/src/server.rs`, lease-fenced, replayed by command id, refused while the source's loop runs); CLI `task fork`.
- proof: a source task with a plan, two reads, a user's answer, a write and a destructive call waiting on its approval is hard-killed and restarted (the approval still pending), checkpointed and forked. The fork's worktree holds the written `a.txt` and the untouched `b.txt`; only the one file that differed from HEAD was materialized; the fork is `Queued`, origin `fork`, with no pending call and no approval, while the source keeps its `REQUESTED` approval and its protocol-state digest is unchanged. The fork then runs to `ReadyForReview` writing `a.txt` again: the fork's worktree changed, the source's did not, the source's log gained nothing, and the source's destructive effect never ran. The session tree shows the fork edge and one `fork` branch event; a second `ForkTask` with the same command id replays the same fork.

## Limitations

- A fork carries the source's context by reference (Context Pack and compaction manifest in the capsule); its first turn compiles a fresh pack for its own worktree.
- The fork's worktree is a git worktree of the source repository: removing the source repository removes the fork's history too (git semantics).

## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`):

- `qual_ev_0077_0122_a_fork_carries_decisions_and_evidence_but_no_stale_pending_effect`
- Unit: `carry_labels_round_trip` (crates/checkpoint)

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json copy alongside
