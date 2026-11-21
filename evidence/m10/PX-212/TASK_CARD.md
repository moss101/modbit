# Task Card — PX-212 Auto-fix

## Identity

- Task ID: PX-212 (REQ-PX-212; owner core-runtime); qualification QUAL-PX-212 / PX-E2E-212; Decision Record DR-PX-2026-11-11-021 (MWS-17)
- Evidence tier: release-critical (the Core queues work for a task and asks for its turn without the person typing it; new events, a new command, protocol fields)
- Prerequisites: PX-211, DOC-PX-020 — both COMPLETE on `main` at 011f237a.
- Branch `px-212-auto-fix`.

## Existing-code audit (before any edit)

Traced from the production callers to the effect boundary:

- Failing checks: `RefreshPullRequest` and the PX-211 schedule -> `pull_request_state::refresh` -> `forge.pr.read` under the task's lease -> `ForgePullRequestStateRead` with `checks` FAILING and the names that did not pass. PRODUCTION-WORKING (PX-211); nothing acts on a failing read.
- Comments: `IngestReviewComments` -> `review_comments::ingest` -> `forge.pr.comments.read` under the lease -> allowed authors (`review_comment_authors`), `@modbit` mention -> STEER `TaskInputQueued` (provenance `forge_review_comment`, untrusted). PRODUCTION-WORKING (PX-008); a steer waits for a person's next start; no schedule reads comments.
- Repair input path: `FixReviewFinding` (PX-214) returns a ReadyForReview candidate to work (`TaskReturnedToWork` + `TaskWaiting{USER_INPUT}`) and queues one FOLLOW_UP input; the person then sends `StartTask`, which resumes the waiting run with every check (lease, emergency stop, policy files, repository trust, route, budgets). PRODUCTION-WORKING.
- Run mode: `RunModeSet` on the task's log (PX-057); the kernel asks for every ExternalSideEffect under Ask/Supervised (`git push` through `shell.exec` asks, ALWAYS_ASK:NETWORK). PRODUCTION-WORKING.
- Internal starts: the automation host sends `StartTask` through `handle_command_as` with its own actor and the session's lease generation. PRODUCTION-WORKING pattern.

Classification: IMPLEMENTED-PARTIAL around the row (read, ingestion, input queue, start, approvals all work); auto-fix itself NOT-FOUND (`grep auto_fix` empty).

**First missing link:** nothing turns a failing read or an accepted comment into a queued repair and a turn; there is no per-task switch.

## Plan

1. Domain: `TaskEvent::AutoFixSet { enabled }`, `AutoFixQueued { cause, number, head_sha, failing_checks, comment_ids, input_id, attempt, cap, cause_offset }`, `AutoFixStopped { reason, attempts, cap, cause, head_sha, cause_offset }` — audit records valid in every state.
2. Protocol (additive): `SetAutoFix` -> `AutoFixState`; `TaskPullRequest` fields 19-24 (auto_fix, attempts, cap, stopped, last cause, last offset); `PullRequestRefreshed` 5-7 and `ReviewCommentsIngestedView` 8-10 (queued, input id, stopped).
3. Core `auto_fix.rs`: fold the switch and count from the log; on a cause queue through the existing input queue and ask for the turn through the ordinary `StartTask`; never set a run mode, rule or budget.
4. Hooks: `refresh` (OPEN + FAILING), `ingest` (steered comments), the schedule (also ingests comments while auto-fix is on and the request is open).
5. Desktop: client, bridge, preload pin, the Pull request pane's Auto-fix switch with the count and the stop.
6. Proof: real Core + GitHub fake (Rust), real app + real Core + fake (Playwright), restart, schedule, cap, busy task, disallowed author, mutations.

## Decisions taken inside the record (DR-PX-2026-11-11-021 MWS-17 leaves them open)

- **Cap:** 3 attempts from the latest time the switch is turned on (`MODBIT_AUTO_FIX_MAX_ATTEMPTS`, 1-20, for the Core's environment). Turning it off and on again starts a new count; that is the person's act.
- **One attempt per failing head:** a failing read of a head that already caused an attempt (or the stop) causes nothing again. A new head (a push by the agent or anyone) can.
- **Busy or held tasks are left alone:** only ReadyForReview or Waiting(USER_INPUT), not running, not archived, no emergency stop. A run waiting for an approval or a question, a paused task and an ended task get nothing; the head is not remembered, so a later read takes it.
- **Comments:** the existing ingestion's STEER inputs already name their comments, so auto-fix queues nothing more; it records `AutoFixQueued` (cause REVIEW_COMMENT, the comment ids) and asks for the turn. With the switch off, PX-008 is unchanged (the steer waits for a person's start). With it on, the schedule also takes the request's comments through the same ingestion (same allow-list and mention rule) while the request is open.
- **The turn:** `StartTask` through `handle_command_as` with actor `core:auto_fix` and the session's current lease generation (the automation host's pattern), command id derived from the input id so a retry replays. A refused start leaves the input queued for the person's next start and is said on stderr.

## What was built

- **Domain** (`crates/domain/src/task.rs`): the three events above, listed with the audit records valid in every state.
- **Protocol** (`stream.proto`, `surface.proto`): as planned; fixtures on both sides (`set_auto_fix`, `auto_fix_state`; `agent_headers` and `pull_request_refreshed` carry the new fields). TypeScript regenerated.
- **Core** (`services/modbit-core/src/auto_fix.rs`): `status` folds the task's records; `on_cause` serialises causes, applies the rules above, appends `TaskReturnedToWork` + `TaskWaiting{USER_INPUT}` for a ReadyForReview candidate, `TaskInputQueued` (FOLLOW_UP, provenance `auto_fix`, untrusted: the check names are the forge's labels) naming only the checks that did not pass, and `AutoFixQueued`; then `StartTask`. Past the cap, `AutoFixStopped` once. `SetAutoFix` (`task.author`, fenced by the session lease, idempotent by command id; the same value records nothing). `refresh`, `ingest` and the schedule call it; the header's `TaskPullRequest` carries the switch and count.
- **Client and bridge**: `CoreClient.setAutoFix`; channel `review:setAutoFix` (validated in main; not on any pane window's allow list; preload re-pinned with its review note; the renderer-bridge allow-list in `workers.spec.ts` names it).
- **Renderer**: the Pull request pane's Auto-fix switch under the request's state (open requests only): off says what it does and that pushes still ask; on says the count against the cap and the latest cause; past the cap says it stopped and how to allow more (words, not colour alone).

## Verification (macOS 27.0.1 arm64, rustc 1.97.1, node 26.11.1)

- `cargo test -p modbit-core --test px_212_auto_fix` — 1 passed (`px212-core.log`). Real Core over its socket, real kernel, approvals, input queue and store, real git with a bare remote, scripted model, the GitHub REST fake. Off: a failing read queues nothing, the task stays ReadyForReview. On (once; the same value again records nothing): a failing read of a new head returns the candidate to work and queues exactly one FOLLOW_UP untrusted input naming `lint` (not `build`), `AutoFixQueued` attempt 1; the run starts and applies it (`TaskSteered`, provenance auto_fix); the agent's `git push` asks (ExternalSideEffect) and the remote has no branch until the person approves; no `RunModeSet` is written. While the run waits for that approval, another failing head is left alone. A stranger's `@modbit` comment is ignored and causes nothing; an allowed reviewer's is the next cause (REVIEW_COMMENT, its id, attempt 2, the steer is its input). Past the cap (2 in the test), the head left alone earlier is taken and stopped once (`AutoFixStopped`, ATTEMPT_CAP); a further head adds nothing. Off then a failing read: nothing; on again: a new count, attempt 1; the same head read again while idle: nothing. Killed and started on the same store with the schedule at 1 s: the header's auto-fix fields are identical, and the schedule's read of a new failing head is a cause (trigger SCHEDULE, attempt 2).
- `auto_fix.rs` unit tests: 2 passed (the input names the failing checks only; a long name is cut).
- Mutations (`mutation-checks-2026-11-21.txt`): 6 of 6 caught — switch ignored, no per-head dedupe, no cap, busy task not left alone, stop said every time, turn never asked for. Restored sources pass.
- Related Core qualifications re-run green (`related-core.log`): PX-211, PX-214, PX-008 (`qual_px_008_allowed_review_comments_steer_as_untrusted_input_and_grant_nothing`).
- Desktop: `apps/desktop/e2e/px-212-auto-fix.spec.ts` (PX-E2E-212) — 1 passed (`px212-e2e.log`): the switch off by default and a failing Refresh starts nothing; turned on (the Core's answer); a new failing head starts the repair turn, whose `git push` asks on the docked approval card and lands on the remote only once allowed; the pane says "1 of 2 repair turns used; the latest for failing checks."; a second head is the second turn; a third is stopped and the pane says so; off says Off. Axe on the pane with the switch, the approval card and the stop. Screenshots in `after/` (light and dark).
- Related specs re-run green (`px212-e2e-related.log`, 7 tests): px-211 pull request state, px-202 pane dock (both), forge-evidence, workers (the bridge allow-list and renderer isolation).
- Protocol fixtures: `cargo run -p protocol-fixtures -- --check` exit 0; `@modbit/surface-protocol` 70 passed; desktop unit 486 passed (`other-suites.log`); clippy `-D warnings` and fmt clean on the touched crates.
- Full Core, domain and protocol suites: `core-suite.log`.

## Limitations

- Proven against a wire-faithful GitHub fake; the read paths are PX-125's (live) and PX-008's.
- Without webhooks (PX-126) a cause is seen only when the Core reads: on Refresh or on the schedule (default every 300 s).
- A refused start (capacity, policy file, trust) leaves the repair input queued and is said on stderr only; the pane shows the attempt as used.
- The Pull request pane's Checks list repeats rows on each Refresh (pre-existing CI-evidence behaviour of PX-009/PX-211, not changed here).
- macOS only locally; Linux and Windows are CI's.
