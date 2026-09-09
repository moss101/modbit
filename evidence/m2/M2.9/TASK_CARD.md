# Task Card — M2.9 Trusted Code Review Surface

## Identity

- Task ID: M2.9
- Milestone: M2
- Canonical owner: workspace-git (`crates/git` diff parsing and selective hunk application; the Core's `review` module applies decisions through the Workspace File Service) with desktop (thin Review screen)
- Requirement IDs: REQ-EV-0036 (per-hunk diff review with provenance and evidence; the resulting Git diff matches the user's choices exactly), REQ-EV-0119 (completion is host-owned: the agent proposes, the user's review decides), REQ-EV-0103 (validated renderer IPC); docs/20 "Trusted Code Surface", "Stale reference handling", "Direct user edits"; docs/29 "display and review results"; docs/83 "Agent coding loop" (show diff/evidence, survive renderer restart); docs/51 E2E-001 (user merges and Git history reflects the change).
- Risk class: high (accepts real changes into Git history)
- Evidence tier: release-critical
- Release membership: ALPHA, BETA, RELEASE_ZERO

## Goal

The Core serves an immutable, revision-bound review bundle for a `ReadyForReview` task: the candidate diff as files and hunks against the base commit, content refs, plan and self-review, verification runs with check statuses, regression attribution, quarantines, invariant findings, receipts and evidence links; a `CodeViewModel` per file with workspace/file revision, changed ranges and stale detection. `DecideReview` applies the user's per-hunk decision: rejected hunks are reverted by rebuilding the file from the base text plus the accepted hunks through the Workspace File Service (revision-bound, provenance `user_review`), the accepted result is committed with the note and evidence summary, and the task completes; `RETURN` sends the task back to work with the feedback queued, and `StartTask` resumes it as a fresh attempt. The desktop Review screen renders only Core payloads and drives the decision through validated IPC; the CLI exposes the same commands.

## Non-goals

- Inline direct edits from the review surface (PX-005, Beta) and pull-request creation (PX-007).
- Syntax highlighting, symbol outlines and diagnostics in the code view (Tier A language services, M3/Beta); the view-model carries the language label, changed ranges and evidence links.
- Conflict handling for candidates that diverged from `HEAD` beyond the worktree (Change Engine merge transactions, M4).

## Required reading

`AGENTS.md`, `docs/20`, `docs/29`, `docs/32`, `docs/51`, `docs/83`.

## Existing-code audit

- classification: NOT-FOUND (no review commands, no hunk model, no review screen)
- production entry point: Core `GetReviewBundle`, `GetCodeView`, `DecideReview`; `modbit_git::{parse_unified, apply_selected}`; desktop `Review` screen + `task:start`, `review:bundle`, `review:codeView`, `review:decide` IPC; CLI `review show`, `review decide`
- real effector/storage boundary: `git diff HEAD` (with intent-to-add for new files) and `git commit` on the task worktree; Workspace File Service writes; `ReviewDecisionRecorded` + `TaskCompleted` / `TaskReturnedToWork` + queued follow-up on the log; object store for content refs

## Invariants

- Only `ReadyForReview` tasks take a decision; a decision built at another workspace revision is refused (`STALE_REVIEW`); unknown hunks are refused.
- Rejected hunks never reach the commit; accepted hunks are committed exactly; the worktree is clean after an accept.
- Every review fact is re-read from the Core (bundle, code view); a restarted app shows the same review.
- A `CodeReference` whose file revision moved on is marked stale, never silently remapped.
- `RETURN` records the decision, returns the task to work and queues the note as a follow-up; the next `StartTask` creates a new attempt.

## Verification

- `crates/git` unit tests: unified-diff parsing; selective application equals the candidate with all hunks, the base with none, and the exact mix otherwise
- Core `m2_9_review_surface_applies_per_hunk_decisions_and_commits`: bundle content and evidence, code view changed ranges and staleness, `NOT_REVIEWABLE` / `STALE_REVIEW` / `UNKNOWN_HUNK`, reject-one-accept-rest → file content, two-file commit, clean worktree, commit message; `RETURN` → Waiting with follow-up queued → resumed attempt reaches review again
- Desktop Playwright `review.spec.ts`: real app, real Core, real Git checkout, scripted wire-faithful model: Start from the card, Review screen from Core payloads, app restart mid-review, reject one hunk, accept → Completed column, Git history and worktree match the choice
- E2E: hosted CI on macOS/Linux/Windows

## Completion evidence

See `evidence.json`.
