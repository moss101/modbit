# Task Card — IMP-EV-0123 Rewind / session tree

## Identity

- Task ID: IMP-EV-0123
- Milestone: M4 Durable recovery spine (P0)
- Requirement: REQ-EV-0123; owner label: Execution Timeline; subsystem: durability
- Qualification: `QUAL-EV-0123` — Preview is non-mutating; revert honors optimistic hash checks.
- Evidence tier: real-system (the real Core, a real coding task's completion checkpoint, hand edits to the worktree between preview and revert)

## Goal

Preview, revert and fork over the run DAG are explicit and auditable: a preview changes nothing and names exactly what a revert would do; a revert refuses when the worktree moved since the caller looked; the session tree shows every task, fork edge, run, checkpoint and restore.

## Existing-code audit

- classification: PARTIAL before this batch (`RestoreCheckpoint` restored a validated chain with no preconditions and no preview; no session tree).
- production entry points:
  - `crates/checkpoint` `plan_rewind` (pure: `WRITE | DELETE | REVERT_TO_HEAD | REMOVE_UNTRACKED | UNCHANGED` with the hash now and after) and `RewindEntry`.
  - `services/modbit-core/src/branch.rs` `preview_rewind` (validates the chain and its objects, plans, applies nothing: no lease, no event, no write) and `session_tree` (off the log: `TaskCreated`, `TaskForked`, `CheckpointRestored`, `SessionBranched`, plus runs and checkpoints per task).
  - `services/modbit-core/src/checkpoint.rs` `restore` — `expected` preconditions checked before anything is planned (one mismatch refuses the whole restore, `HASH_MISMATCH`, naming the path; an absent path is a hash too), every op of the restore transaction carrying the content it was planned against so a concurrent write refuses the transaction; `CheckpointRestored.preconditions_checked`.
  - `PreviewRewind` / `RewindPreview`, `RestoreCheckpoint.expected` / `CheckpointRestoreResult.preconditions_checked`, `GetSessionTree` / `SessionTreeView` on the wire; CLI `task rewind [--apply]` (the preview's hashes become the restore's preconditions) and `session tree`; `session_tree_view` protocol fixture (Rust ↔ TS).
- proof: after a task reaches review with its completion checkpoint, the worktree is edited by hand and a stray file added; the preview names `qty.txt WRITE` (hand hash → checkpoint hash) and `stray.txt REMOVE_UNTRACKED`, twice identically, and leaves the files, the revision and the log offset unchanged; a restore expecting a hash the caller never saw is refused `HASH_MISMATCH` naming the path with nothing written and no event; expecting a file to be absent that exists is refused too; the restore with the preview's hashes writes one file, removes one, checks two preconditions, lands `CheckpointRestored { preconditions_checked: 2 }`, and a following preview finds everything unchanged; the session tree records the restore and opens no branch.

## Limitations

- A revert of the same task does not move the session branch generation (only a fork does); it records `CheckpointRestored` and is visible in the tree.
- The tree is read from the log on demand; it is not a projection table.

## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`):

- `qual_ev_0123_rewind_preview_is_non_mutating_and_revert_honours_optimistic_hashes`
- Unit: `a_rewind_plan_names_every_action_and_nothing_else` (crates/checkpoint)
- Regression: `qual_ev_0012_0013_e2e_007_checkpoint_epochs_are_fenced_and_restore_validates_every_object` (restore without preconditions)

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json copy alongside
