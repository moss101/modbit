# Task Card — PX-211 Task pull-request state

## Identity

- Task ID: PX-211 (REQ-PX-211; owner workspace-git); qualification QUAL-PX-211 / PX-E2E-211; Decision Record DR-PX-2026-11-11-021 (MWS-17)
- Evidence tier: release-critical (the Core records and refreshes a pull request's state, a projection gains fields, a new command and a new preference)
- Prerequisites: PX-125, PX-202, DOC-PX-020 — all COMPLETE on `main` at 7701741a.
- Branch `px-211-pull-request-state`.

## Existing-code audit (before any edit)

Traced from the production callers to the forge boundary:

- Open: Review / Pull request pane -> `usePullRequest` -> `window.modbit.openPullRequest` -> `OpenPullRequest` (`services/modbit-core/src/pull_request.rs`) -> `forge.pr.create` through the tool pipeline under the task's lease and an approval -> the adapter (`crates/tools/src/forge.rs`) -> `ForgePullRequestOpened` on the task's log (`services/modbit-core/src/forge.rs`). PRODUCTION-WORKING (PX-007, PX-125).
- Read: `forge.pr.read` returns `state`, `merged`, `draft`, `head.sha` and a check-run summary (`checks_summary`). PRODUCTION-WORKING as a tool, proven on real GitHub by PX-125; **no Core caller** — only the model can call it.
- CI evidence: `IngestCiResults` (`ci_evidence.rs`) reads `forge.ci.status` for the pushed commit and records `CiEvidenceRecorded`. PRODUCTION-WORKING (PX-009); it is evidence, not the request's state.
- Header: `GetAgentHeaders` -> `transcript::headers` -> `EventStore::task_digests` (`crates/event-store/src/digest.rs`, grouped SQL, loads no object) -> `AgentHeader`. PRODUCTION-WORKING; carries nothing of a pull request.
- Archive and its undo: `ArchiveTask` -> `ConversationArchived` / `ConversationUnarchived` on the Conversation aggregate. PRODUCTION-WORKING.
- Desktop: the Pull request pane (PX-202) says in its own header comment that "the request's state (open, merged, closed) is not shown anywhere: the Core has no read of it (PX-211)"; the request it shows is the open/update answer held in renderer memory and gone on reload. The preview card says "the pull request waits for the row that delivers it (MWS-17)".

Classification: IMPLEMENTED-PARTIAL around the row (open, read tool, CI ingestion, archive/undo all work); the row's own behaviour is NOT-FOUND.

**First missing link:** nothing in the Core reads a task's pull request after it was opened, so no state is on the log for a header, a pane or a schedule to show.

## Plan

1. Domain: `TaskEvent::ForgePullRequestStateRead` (number, url, state, draft, merged, head as read, the pushed commit, check counts and the names not passing, trigger, tool call). `SessionEvent::PullRequestPreferencesSet { archive_after_merge }`.
2. Protocol (additive, `stream.proto`): `TaskPullRequest` view; `AgentHeader.pull_request = 470`; `RefreshPullRequest` -> `PullRequestRefreshed`; `GetPullRequestPreferences` / `SetPullRequestPreferences` -> `PullRequestPreferences`.
3. Core: `pull_request_state.rs` — refresh reads `forge.pr.read` through the tool pipeline under the task's lease (never around it), records the read, and, on the read that first sees the merge with the session's preference on, archives the task with the existing `ConversationArchived` (undo is the existing `ArchiveTask{archived:false}`). A watcher refreshes open requests on a schedule (`MODBIT_PR_REFRESH_SECS`).
4. Store digest: latest opened request and latest read per task, into `AgentHeader.pull_request`.
5. Desktop: bridge, header row badge, preview card line, Pull request pane state with "read at", Settings switch for archive after merge.
6. Proof: real Core + GitHub fake (Rust), real app + real Core + fake (Playwright), restart, preference on/off, undo, policy refusal, stale read.

## What was built

- **Domain** (`crates/domain`): `TaskEvent::ForgePullRequestStateRead` (owner, repo, number, url, state OPEN/CLOSED/MERGED, draft, the head the forge reported and the commit this Core pushed, checks NONE/PENDING/FAILING/PASSING/UNAVAILABLE with counts and up to 20 names that did not pass, trigger REQUEST/SCHEDULE, the `forge.pr.read` call id) — an audit record valid in every state; the request's body and title are not recorded. `SessionEvent::PullRequestPreferencesSet { archive_after_merge }`.
- **Store** (`crates/event-store`): the task digest gains `pull_request` — the latest `ForgePullRequestOpened` and the latest `ForgePullRequestStateRead` of that same request, by grouped SQL (no event or object loaded). `sessions_with_pull_requests()` for the schedule.
- **Protocol** (additive, `stream.proto`): `TaskPullRequest`; `AgentHeader.pull_request = 470` (block 470-479); `RefreshPullRequest` -> `PullRequestRefreshed`; `GetPullRequestPreferences` / `SetPullRequestPreferences` -> `PullRequestPreferences`. Fixtures on both sides (`agent_headers` gains the field; `refresh_pull_request`, `pull_request_refreshed`, `get_pull_request_preferences`, `set_pull_request_preferences`, `pull_request_preferences`).
- **Core** (`services/modbit-core/src/pull_request_state.rs`): `refresh` reads `forge.pr.read` through the tool pipeline under the task's lease and the Capability Kernel (token in the Core's custody), refuses an answer about another request or with an unknown state, and records the read; a refused or failed read records nothing. On the read that first finds the request merged, with the session's preference on and the task not running, it appends the ordinary `ConversationArchived` (actor `core:pull_request.archive_after_merge`), so `ArchiveTask{archived:false}` is the undo; it never archives twice. The schedule (`MODBIT_PR_REFRESH_SECS`, default 300 s, 0 = off) reads every request not yet read as merged or closed, of tasks not archived. Commands: `RefreshPullRequest` (`review.decide`, fenced by the session lease), `GetPullRequestPreferences` (`events.subscribe`), `SetPullRequestPreferences` (`session.control`, idempotent by command id; the same value records nothing). The transcript draws no tool card for the schedule's reads (they stay on the log); a person's Refresh does, like CI ingestion.
- **Client and bridge**: `CoreClient.refreshPullRequest`, `pullRequestPreferences`, `setPullRequestPreferences`; channels `review:refreshPullRequest`, `prefs:pullRequests`, `prefs:setPullRequests` (validated in main, not on any pane window's allow list; preload re-pinned with its review note); `AgentHeaderView.pullRequest`.
- **Renderer**: the Pull request pane shows the Core's record — number, state, checks, the names that failed, and when and how it was read ("Read just now, when asked." / "…, by the Core's schedule.") — or that it was not read yet; its Refresh reads the state, then CI, then comments, each refusal said apart; an archive it caused offers Undo; a request on record is updated, not opened again; when the list no longer holds the header (archived) the pane reads it itself. The task row carries `#N` with a mark for the state and checks (its words in the row's label and title), the preview card a line, the details rail a row. Settings > Automations and worktrees: "Archive after merge" (the Core's preference; off until set). The list shows the same Archived notice with Undo when the Core archives after a merge.

## Verification (macOS 27.0.1 arm64, rustc 1.97.1, node 26.11.1)

- `cargo test -p modbit-core --test px_211_pull_request_state` — 1 passed (`px211-core.log`). Real Core, real socket, real store and kernel, real git with a bare remote, scripted model, the GitHub REST fake (`tests/support/github_fake.rs`). Two tasks: refresh before a request is NO_PULL_REQUEST; opened through its approval, the header names #1 with state "" and no read time; refresh reads OPEN/PENDING, OPEN/FAILING (lint), OPEN/PASSING, each equal to the server's, with the forge read under the Core's token; a 500 records nothing and the header keeps the last read with its time; MERGED with the preference off archives nothing; the log holds exactly the four reads and no body or title; the preference set twice records once; the Core is killed and started on the same store and the header is identical; the schedule (1 s) reads b's merge (trigger SCHEDULE), archives it once, does not read the settled a; Undo stands for 3 s and a person's refresh of the same merge does not archive again; the transcript shows one card per person's read and none for the schedule's; after EmergencyStop revokes the leases the kernel refuses the read, the forge is not asked and nothing is recorded.
- `services/modbit-core/src/pull_request_state.rs` unit tests: 2 passed (state mapping; check counting).
- Mutations (`mutation-checks-2026-11-21.txt`): 5 of 5 caught — archive ignoring the preference, archive on every merged read, schedule reading settled requests, a state shown before any read, scheduled reads drawing tool cards. Restored sources pass.
- Desktop: `apps/desktop/e2e/px-211-pull-request-state.spec.ts` (PX-E2E-211), real app and Core with a fake forge in the test process: opened through the docked approval card; pane, row and header say "not read yet"; Refresh reads failing then passing checks, shown in the pane, the row, the details rail and the preview card with when they were read; the app and Core restart and the header is the same; Settings turns archive after merge on (off before, the Core's record after); the Core's schedule reads the merge, archives the task, the list shows the notice with Undo and the pane still shows "merged, by the Core's schedule"; Undo brings it back for good and the settled request is not read again. Axe on the pane, the preview card, the setting and the notice. Screenshots in `after/` (light and dark).
- Related specs re-run green with the change (`px211-e2e-related.log`, 10 tests): px-202 pane dock (its "no unread state" checker now accepts state words only inside values of a state read — `agentHeaders` or `refreshPullRequest` — and still catches an injected "Merged"), forge-evidence, px-218 last line, agents, px-201 details rail, px-208 review settings.
- Protocol fixtures: `cargo run -p protocol-fixtures -- --check` exit 0; `pnpm --filter @modbit/surface-protocol test` 68 passed; desktop unit 485 passed; adapter tests pass; clippy `-D warnings` and fmt clean on the touched crates.

## Limitations

- Proven against a wire-faithful fake, not real GitHub; the read path itself (`forge.pr.read`) is PX-125's, proven live there.
- The preference is per session (the desktop's session); a second session has its own.
- The schedule reads each open request every interval in one pass, one at a time; there is no backoff beyond the next interval when the forge refuses (rate limits are typed by the adapter and logged).
- The Core's archive is recognised by the desktop as the archive right after the read that found the merge (pushed events carry no actor); it changes only the notice's words.
- macOS only locally; Linux and Windows are CI's.
