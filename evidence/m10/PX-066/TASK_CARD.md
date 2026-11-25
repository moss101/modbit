# Task Card — PX-066 Apply-back of a worktree result to the user's checkout with conflict handling and exact undo (CLI leg)

## Identity

- Task ID: PX-066 (REQ-PX-066; owner workspace-git); qualification QUAL-PX-066 / PX-E2E-066
- Evidence tier: release-critical (protected effect on the user's own working tree with destructive options)
- This card records the CLI leg ("the same from the CLI") and, from 2026-11-25, the close-out below. The earlier hold (prerequisite PX-061 REAL_TESTING) ended when PX-061 was sealed COMPLETE on 2026-11-24 (main 39068949).

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

## Close-out (2026-11-25, branch px-066-close-out from main 6c9c1c79)

The graph still showed PX-066 NOT_STARTED although the Core, Git, CLI and protocol work had landed with PR e6221766 and the CLI leg after it. This close-out audits the row against QUAL-PX-066 clause by clause, closes the two gaps the Limitations above named, writes the docs the card requires, and walks the graph.

### Acceptance, clause by clause

| QUAL-PX-066 | Proof |
|---|---|
| clean reviewed result: checkout hash equals the candidate | `worktrees_px.rs::px_066_apply_is_a_protected_effect_bound_to_its_intent_and_undo_is_exact`; CLI `qual_px_066_cli_clean_apply_is_hash_exact_receipted_and_undo_restores_the_users_bytes` |
| conflicting user edit, each option, hashes after each and after Undo | `px_066_conflicts_are_typed_never_overwritten_and_each_option_is_exact`; CLI `qual_px_066_cli_every_conflict_option_is_exact_and_undo_after_a_second_edit_destroys_nothing` |
| a stale review revision is refused | `px_066_a_stale_review_or_plan_is_refused_and_a_forged_plan_cannot_ride_an_approval` (STALE_REVISION, STALE_PLAN) |
| `SIGKILL` mid-apply recovers to a whole state verified by hashes | **new** `qual_px_066_an_external_sigkill_while_the_apply_is_writing_recovers_whole`: the Core is held between two writes (`MODBIT_FAULT_WORKTREE_STALL=WORKTREE_APPLY_MID:120000`), the test sees the half-written checkout on disk and SIGKILLs the Core from outside; restart restores the exact pre-apply hashes, logs one `WorktreeApplyRolledBack`, leaves no temp file, and the same apply then completes. The three abort-point tests `px_066_a_core_killed_*` still run. |
| the receipt names the intent hash and both revisions | `px_066_apply_is_…` (receipt `intent_hash`; `WorktreeApplied.candidate_tree` and `checkout_head_before`) |
| the same from the CLI | `apps/cli/tests/px_066_apply_back.rs` (2 tests) |
| negative: apply without approval refused | `px_066_apply_is_…` (denied → nothing changes) |
| negative: stale revision refused | as above |
| negative: overwrite without typed confirmation refused | `px_066_conflicts_…`, `px_066_protected_paths_need_their_names_typed` |
| negative: Undo after a second user edit reports the conflict | `px_066_undo_after_a_second_user_edit_reports_the_conflict_and_destroys_nothing` |
| negative: a remembered destructive choice is not honoured | `px_066_a_remembered_choice_is_applied_only_when_it_is_non_destructive` |
| negative: a replay of Apply with the same command id applies once | **new** `qual_px_066_a_replayed_apply_with_the_same_command_id_applies_once`: the identical envelope is sent after the person edits an applied file (twice), after a Core restart, and after an Undo. Answers are `CONFLICT, CONFLICT, CONFLICT, APPROVAL_PENDING`; the checkout is never written; one `WorktreeApplied` and one SUCCESS receipt for the intent. |

### What the replay proof shows

A resend of `ApplyWorktree` is not answered from a command-id record. It is planned again against the checkout as it is now. That is why it applies once: over a later edit it is a conflict with Cancel as the default, and after an Undo it is a new request that waits for a new approval. A replay of the approved effect after a crash between the record and the answer is a separate path: it returns the recorded `APPLIED` with `replayed` (`apply_back.rs`, the "Replayed after a crash" branch).

### Change

- `services/modbit-core/src/apply_back.rs`: `fault_point` also honours `MODBIT_FAULT_WORKTREE_STALL="<point>:<ms>"`, the same shape as the checkpoint stall hook. It is only read from the environment the test sets.
- `services/modbit-core/tests/worktrees_px.rs`: the two tests above and a `replay_apply` helper.
- `docs/20` "Applying a worktree result to the checkout" and `docs/30` "As built (PX-066)": the card's "docs 20 and 30 updated by the implementing task".

### Verification (local, macOS, 2026-11-25)

- `tests-2026-11-25.log`: `worktrees_px` 41 passed (every PX-118/065/066/119 test), `modbit-git --test apply_back` 9 passed, `modbit-cli --test px_066_apply_back` 2 passed.
- `clippy-2026-11-25.log`: `cargo clippy -p modbit-core --tests -D warnings`, exit 0.
- `mutation-check-sigkill.txt`: with the startup recovery call removed, the external SIGKILL test fails ("after recovery the checkout is exactly the pre-apply state"; `a.txt` left half-applied); restored, it passes.
- Linux and Windows run in CI on the PR.

### Remaining

- The desktop modal and its packaged E2E are PX-068, not this row.
- No release gate is attested.
