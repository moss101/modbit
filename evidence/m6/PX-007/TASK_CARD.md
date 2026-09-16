# Task Card — PX-007 Pull request create and update from a reviewed result

## Identity

- Task ID: PX-007
- Milestone: M6 Subagents/fleet (P0→P1)
- Requirement: REQ-PX-007; owner label: workspace-git; subsystem: `services/modbit-core/src/pull_request.rs`, `crates/git` (typed push), protocol
- Qualification: QUAL-PX-007 — a reviewed fixture result opens a PR on a dedicated branch after approval; the PR body carries the evidence summary; a later revision updates the same PR with a new receipt; a stale candidate revision cannot be pushed; a denied approval leaves no branch on the remote; a crash after push before receipt reconciles to exactly one PR.
- Evidence tier: real-system (the real Core, kernel and git; a real bare remote reached through git's `insteadOf` rewrite of the forge URL; the GitHub fake for the API — the real-repository run waits for a token, DR-M6-002)

## Goal

From Review or CLI, push the dedicated branch and open or update a PR as protected external effects with approval and receipts, bound to the exact candidate revision, with the evidence summary in the PR body (docs/20, docs/29).

## Existing-code audit

- classification: DOCUMENTED-ONLY before: the review decided and committed locally; no push, no pull request, no typed remote operation in `crates/git`.
- production entry points: `services/modbit-core/src/pull_request.rs` (`run`: candidate from the accepted commit or a per-revision snapshot, remote parsed on the forge host, the evidence summary, deterministic tool call id per attempt and idempotency key per revision, push only after the approval, `forge.pr.create` / `forge.pr.update` through the pipeline), `crates/git/src/lib.rs` (`remote_url`, `set_branch`, `push_branch`, `remote_branch_head`, `rev_parse`), `server.rs` (`OpenPullRequest`, `UpdatePullRequest`), `surface.proto` (`PullRequestAck`).
- proof: on a task under review at revision R1: the first `OpenPullRequest` is `APPROVAL_PENDING` with the intent hash and no branch reaches the remote; a denial leaves no branch and the next call is a new attempt; approved, the branch `modbit/pr-<task>` is on the bare remote at the candidate commit, PR #1 is open with a body carrying the evidence summary (revision, verification) on base `main`, one receipt; `expected_candidate_revision` R1+1 is `STALE_REVISION`; R1 again replays `OPENED` #1; after `RETURN` and a further run to R2, `UpdatePullRequest` is approved, pushes a new snapshot and PATCHes PR #1 with the R2 summary and a second receipt; the log holds one `ForgePullRequestOpened`, one `ForgePullRequestUpdated`, two receipts; every forge call carried the token.

## Limitations

- the real-repository run waits for a token (DR-M6-002); after acceptance the branch stays at the snapshot commit of the same tree until a later revision moves it; the review surface's button and `modbit-cli pr open|update` land with PX-023's screen work — the surface command is what they call.

## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`):

- `qual_px_007_a_reviewed_result_opens_and_updates_a_pull_request_as_approved_receipted_effects`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json copy alongside: `ci-run-34761496840.json` (main at 299f8be; the change commit c2441aa, the DR landing note 76256dd and its manifest reseal 299f8be)
