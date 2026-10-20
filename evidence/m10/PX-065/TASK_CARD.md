# Task Card — PX-065 Worktree lifecycle: creation policy, setup commands, scheduled cleanup, lease and retention

## Identity

- Task ID: PX-065 (REQ-PX-065; owner workspace-git); qualification QUAL-PX-065
- Evidence tier: release-critical (effect-bearing behavior; see the acceptance below)
- Closed from E2E_PROVEN to COMPLETE on 2026-10-20 after the re-run recorded in evidence.json.

## Existing-code audit

- adoption audit: DOCUMENTED-ONLY; no product caller present
- classification now: PRODUCTION-WORKING (landed on main; verified by the tests below through the real Core)

## Verification

- acceptance: Real Git and Core: create worktrees with dirty and untracked files and verify the copies and the typed branch-in-use error; a setup list that fails shows the failure and the task continues; a setup list that sleeps past 300 s is stopped and reported; with a shortened interval the cleanup runs on schedule, after a Core restart with an overdue last run the catch-up runs after 30 s, two concurrent requests give one lease holder and one refusal; with 30 worktrees (5 dirty, 3 with unapplied changes, 2 running, 1 under 10 minutes old) cleanup removes only eligible ones, in orphan-then-LRU order, and the report matches the filesystem; a skipped run reports skipped.
- tests: `services/modbit-core/tests/worktrees_px.rs::px_065_cleanup_removes_only_what_is_eligible_in_orphan_then_lru_order`, `services/modbit-core/tests/worktrees_px.rs::px_065_a_second_cleanup_while_the_lease_is_held_is_refused_naming_the_holder`, `services/modbit-core/tests/worktrees_px.rs::px_065_the_schedule_catches_up_after_a_restart_and_persists_the_last_run`
- re-run: macOS local 2026-10-20 and CI run 39029466637 on main ac4aa804, all green.

## Limitations

- Windows and Linux legs are proven by CI only; no release gate is attested by this task.

## Evidence

- `evidence.json` and `ci-run-39029466637.json` in this directory
