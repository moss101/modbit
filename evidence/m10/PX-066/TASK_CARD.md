# Task Card — PX-066 Apply-back of a worktree result to the user's checkout with conflict handling and exact undo (CLI leg)

## Identity

- Task ID: PX-066 (REQ-PX-066; owner workspace-git); qualification QUAL-PX-066 / PX-E2E-066
- Evidence tier: release-critical (protected effect on the user's own working tree with destructive options)
- This card records the CLI leg ("the same from the CLI"). Status is not COMPLETE: prerequisite PX-061 is REAL_TESTING.

## Existing-code audit

- adoption audit: DOCUMENTED-ONLY; no product caller present
- Core (`services/modbit-core/src/apply_back.rs`, `crates/git/src/apply.rs`, `worktrees.proto`): PRODUCTION-WORKING. `ApplyWorktree` / `UndoApply` / `ListWorktrees` are dispatched in `services/modbit-core/src/server.rs`, the effect is the governed tool `git.worktree.apply` / `git.apply.undo` (approval bound to the intent, receipts), the pre-apply checkpoint is in the object store before any write.
- Core proof that already existed: `services/modbit-core/tests/worktrees_px.rs::px_066_*` (clean apply and exact undo on hashes, every conflict option on hashes, stale revision and stale plan refused, forged plan refused, remembered choice only if non-destructive, protected paths, undo after a second edit reports UNDO_CONFLICT, three crash points) and `crates/git/tests/apply_back.rs`.
- Desktop: `packages/ide-adapter-core/src/projects.ts` (`applyWorktree`, `undoApply`), `apps/desktop/src/main/project-ipc.ts`, `apps/desktop/e2e/worktrees.spec.ts`; PX-068 owns the modal.
- CLI before this task: NOT-FOUND (only `task create --isolation worktree`); no way to list, apply or undo.
- First missing link: no `worktree` verb in `apps/cli`.

## Implementation

- `apps/cli/src/worktree_verbs.rs`: `worktree list`, `worktree apply`, `worktree undo`; thin client of the Core's commands under the session lease; prints the typed conflict view (paths, options, default, destructive, confirm paths) and every Core code; exit 0 applied/undone/nothing/cancelled, 2 approval pending, 3 conflict needing a decision, 1 refusal. The approval is the ordinary `approval list` / `approval resolve --intent`.
- No Git, plan, conflict or undo code in the CLI (the crate has no `modbit-git` dependency).

## Verification

- `apps/cli/tests/px_066_apply_back.rs` (real `modbit-cli`, real `modbit-core`, real Git): clean apply hash-equals the candidate, user edit untouched, approval required, stale review revision refused (STALE_REVISION), receipt events name the intent hash and the candidate tree and the checkout head before, replay is NOTHING_TO_APPLY, Undo restores every byte; with a conflicting user edit and a dirty and an untracked file: typed conflict view, Cancel, three unconfirmed overwrites refused (OVERWRITE_NOT_CONFIRMED) with no approval asked, MERGE_MANUALLY, OVERWRITE, STASH, FULL_OVERWRITE each compared by hashes and each undone to the exact pre-apply hashes; Undo after a second edit gives UNDO_CONFLICT and changes nothing.
- Mutation check: `mutation-check.txt` (the Core's typed-confirmation guard dropped makes the conflict-option test fail; restored passes).

## Limitations

- `SIGKILL` mid-apply is proven in the Core only (`px_066_a_core_killed_*`, three fault points that abort the Core process and recover on restart), not from the CLI and not by an external SIGKILL at arbitrary instants.
- The `APPLIED` ack does not repeat `intent_hash` (only `APPROVAL_PENDING` does); the receipt names it, and the CLI test checks that in the event log.
- Windows and Linux legs are not run by this task; no release gate is attested.
- The CLI cannot yet pass `--command-id` for apply replay; replay safety is the Core's NOTHING_TO_APPLY.
