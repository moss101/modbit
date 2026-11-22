# Task Card — PX-213 Merge when ready

## Identity

- Task ID: PX-213 (REQ-PX-213; owner external-tools); qualification QUAL-PX-213 / PX-E2E-213; Decision Record DR-PX-2026-11-11-021 (MWS-17)
- Evidence tier: release-critical (a new external effect that changes a remote repository; new events, a new command, protocol fields)
- Prerequisites: PX-211, DOC-PX-020 — both COMPLETE on `main`.
- Branch `px-213-merge-when-ready`.

## Existing-code audit (before any edit)

Traced from the production callers to the effect boundary:

- Forge writes: `crates/tools/src/forge.rs` — `forge.pr.create`, `forge.pr.update`, `forge.issue.comment`, `forge.pr.comment` are `ExternalSideEffect` tools needing `network.egress` + `secret.use`, idempotent through the host's `ForgeLedger`, the token only in the header. PRODUCTION-WORKING (PX-006, PX-125). **No merge verb** (`grep 'pr.merge\|/merge'` empty).
- Person-initiated protected forge effect: `OpenPullRequest` (`services/modbit-core/src/pull_request.rs`, PX-007) invokes the tool through the pipeline with a deterministic call id; the first call returns `APPROVAL_PENDING`, the call after the decision runs under the approval and the kernel appends the receipt. PRODUCTION-WORKING; the pattern to follow.
- Request state: `RefreshPullRequest` and the schedule -> `pull_request_state::refresh` -> `forge.pr.read` -> `ForgePullRequestStateRead` (state, draft, head sha, check counts). PRODUCTION-WORKING (PX-211); nothing acts on a passing read.
- Approval binding: the approval binds `arguments_hash(arguments)` (intent hash); `ResolveApproval` refuses a decision naming another hash. With the head sha in the merge arguments the approval binds that head.
- Voiding a pending ask: `approval_expiry::expire` writes `ApprovalExpired` and the waiting call's `ToolCallFailed` atomically (only when due today).
- Run modes and rules (`crates/policy/src/runmode.rs`): an `ExternalSideEffect` with `network.egress`/`secret.use` is in the always-ask NETWORK and SECRET classes; AUTONOMOUS passes NETWORK only for a dependency install; an allow rule is a literal argv prefix and a forge tool has no argv. So a forge write asks in every mode today. PRODUCTION-WORKING; proven again here for the merge.

Classification: merge when ready NOT-FOUND (documented only); every piece around it PRODUCTION-WORKING.

**First missing link:** the forge adapter has no merge verb, and nothing records a person's merge-when-ready request or turns a passing read into the approval-bound merge.

## Plan

1. Forge adapter: `forge.pr.merge` (`ExternalSideEffect`, `network.egress` + `secret.use`, idempotent by key) — reads the request and the head's check runs again and refuses `HEAD_MOVED`, `PR_DRAFT`, `PR_NOT_OPEN`, `CHECKS_FAILING`, `CHECKS_PENDING`, `CHECKS_UNAVAILABLE`, then `PUT .../merge` pinned to `sha` (`409` -> `HEAD_MOVED`, `405` -> `NOT_MERGEABLE`). The GitHub fake gains the documented merge route.
2. Domain: `MergeWhenReadySet`, `MergeAsked`, `MergeAskVoided`, `MergeCompleted`, `MergeRefused`, `MergeWhenReadyCleared` — audit records valid in every state.
3. Protocol (additive): `SetMergeWhenReady` -> `MergeWhenReadyState`; `TaskPullRequest` 25-32; `PullRequestRefreshed` 8-9.
4. Core `merge_when_ready.rs`: fold the request from the log; on every PX-211 read ask, void, merge or clear; `ResolveApproval` hands a decision on a merge approval to a worker (approve: the read that merges; deny: off). `approval_expiry::void` closes a pending ask whose intent no longer stands.
5. Desktop: client, bridge (validated method), preload pin, the Pull request pane's Merge when ready switch and method.
6. Proof: real Core + GitHub fake (Rust), real app + real Core + fake (Playwright), restart, mutations.

## Decisions taken inside the record (DR-PX-2026-11-11-021 MWS-17 leaves them open)

- **Ready** means open, not a draft, a 40-character head, and every check run of that head completed without a failing conclusion — or no check runs at all (nothing to wait for). `UNAVAILABLE` checks wait.
- **Method:** `merge` (GitHub's default) unless the person picks `squash` or `rebase`; the approval binds it.
- **One ask per head at a time.** A head that moves, checks that stop passing, a draft, or the person withdrawing voids the ask (`MergeAskVoided`); a still-pending approval is closed with the expiry's records (`ApprovalExpired` + `ToolCallFailed APPROVAL_VOIDED`), so the card leaves and a late decision is refused `APPROVAL_EXPIRED`. An already-approved ask for a head that moved is never spent.
- **A denial turns merge when ready off** (`MergeWhenReadyCleared DENIED`); asking again is the person's act and asks again at once.
- **Retryable refusals** (`HEAD_MOVED`, `CHECKS_*`, rate limit, unreachable) leave it on and the next ready read asks again; any other refusal (`NOT_MERGEABLE`, `PR_NOT_OPEN`, `PR_DRAFT`, forbidden) turns it off with the reason. A request closed or merged by someone else turns it off (`CLOSED`, `MERGED_ELSEWHERE`).
- **The merge's turn:** a decision on a `forge.pr.merge` approval is queued by `ResolveApproval` to a worker started with the Core (the handler never waits on a forge read); the worker runs the read, which dispatches the approved call once. A Core restarted with an approved ask merges on its next read (request or schedule).
- **No allow rule, preset or run mode covers it:** the kernel's existing always-ask classes (NETWORK, SECRET for an `ExternalSideEffect` with egress and a secret) and argv-only rules already guarantee it; the qualification proves it under AUTONOMOUS with a rule naming the tool.

## What was built

- **Forge adapter** (`crates/tools/src/forge.rs`): `forge.pr.merge` as planned; `MERGE_RETRYABLE` names the refusals a later read may clear. No compensation is declared (a merge is undone only by a revert a person decides on). GitHub fake (`tests/support/github_fake.rs`): `PUT /repos/{o}/{r}/pulls/{n}/merge` as documented.
- **Domain** (`crates/domain/src/task.rs`): the six records, listed with the audit records valid in every state.
- **Protocol**: `SetMergeWhenReady` / `MergeWhenReadyState`; `TaskPullRequest` 25-32; `PullRequestRefreshed` 8-9; the transcript approval state gains `WITHDRAWN` (comment only). Fixtures on both sides (`set_merge_when_ready`, `merge_when_ready_state`; `agent_headers`, `pull_request_refreshed` carry the fields). TypeScript regenerated.
- **Core**: `services/modbit-core/src/merge_when_ready.rs` (fold, phase words, `on_read`, `dispatch`, the decision worker); `pull_request_state::refresh` calls it on every read and reads once more after a merge (so the header says MERGED and archive-after-merge applies); `SetMergeWhenReady` (`task.author`, fenced by the session lease, idempotent by command id; reads at once when turned on); `ResolveApproval` queues decisions on `forge.pr.merge` approvals; `approval_expiry::void`; a withdrawn approval's refusal says it was withdrawn, and the transcript labels it `WITHDRAWN`.
- **Client and bridge**: `CoreClient.setMergeWhenReady`; channel `review:setMergeWhenReady` (main validates the ids, the boolean and the method; preload re-pinned with its review note; on no pane window's allow list; the renderer-bridge allow-list in `workers.spec.ts` names it).
- **Renderer**: the Pull request pane's Merge when ready switch and method under Auto-fix (words, not colour alone: off says nothing merges without approval; waiting says for what; asked points to the approvals tray; merged names the head and merge commit; cleared says why). The conversation's approval card and the dock name a withdrawn ask as withdrawn, never as one the person let expire.

## Verification (macOS 27.0.1 arm64, rustc 1.97.1, node 26.11.1)

- `cargo test -p modbit-core --test px_213_merge_when_ready` — 1 passed (`px213-core.log`). Real Core over its socket, real kernel, approvals, effect ledger and store, real git with a bare remote, scripted model, the GitHub REST fake. Before a request exists: `NO_PULL_REQUEST`. A bad method: `BAD_PAYLOAD`. Asked for (squash) while a check runs: WAITING, "1 check(s)", no approval; the same request again records nothing. Run mode AUTONOMOUS and a USER rule naming `forge.pr.merge` with `covers_always_ask`: the checks pass and the merge still asks (`REQUESTED`, ExternalSideEffect, decided under AUTONOMOUS); `MergeAsked` names head, method, number and the approval's intent hash; the forge received no merge. The same read again asks nothing more. The head moves before approval: `MergeAskVoided HEAD_MOVED`, the old approval `EXPIRED`, approving it refused `APPROVAL_EXPIRED` with "withdrawn", a new ask for the new head. The checks stop passing: voided `CHECKS_NOT_PASSING`, WAITING naming `lint`. The tool itself, under an approval, refuses `CHECKS_FAILING` and `CHECKS_PENDING`; the forge received no merge. Denied: `CLEARED`, "denied"; a read asks nothing; asking again reads at once and asks. Killed and restarted on the same store: the header's merge fields identical. Approved while the forge's head had already moved: the approved head is not merged (voided `HEAD_MOVED`) and the new head is asked. Approved at its head: exactly one `PUT .../merge` with that sha and `squash`; `MergeCompleted` names the approval and receipts that are on the effect ledger; the header reads MERGED; asking again `PR_NOT_OPEN`; no second merge.
- `merge_when_ready.rs` unit tests: 3 passed (phase words, readiness, call ids).
- Mutations (`mutation-checks-2026-11-22.txt`): 5 of 5 caught — approved ask dispatched without comparing heads, the tool not checking the head's runs, asking while checks are pending, a moved head leaving the pending approval open, a denial not turning it off. Restored sources pass.
- Desktop: `apps/desktop/e2e/px-213-merge-when-ready.spec.ts` (PX-E2E-213) — 1 passed (`px213-e2e.log`): off by default; asked for with squash while a check runs, it waits and says so and no card asks; the checks pass and the docked card asks for `forge.pr.merge` naming that head's approval; a push moves the head before the decision and the card asks for the new head's approval while the old one is named withdrawn (dock and conversation); allowed on the card, the forge receives exactly one merge of that head, squashed, and the pane says merged. Axe on the pane and the card. Screenshots in `after/` (light and dark), checked by eye.
- Related specs (`px213-e2e-related.log`): px-211, px-212, px-202 pane dock, forge-evidence, workers (bridge allow-list and isolation), agents, browser, qual-gaps (expired-approval wording).
- Protocol fixtures: `cargo run -p protocol-fixtures -- --check` exit 0; `@modbit/surface-protocol` 72 passed; desktop typecheck and unit 487 passed; clippy `-D warnings` and fmt clean (`other-suites.log`).
- Full Core, tools, domain and protocol suites, no fail-fast (`core-suite.log`): every binary passed but the tool compatibility matrix, which had no row for the new tool; the `forge.pr.merge` row was added to `crates/tools/tool-matrix.json` and `modbit-tools` re-run green (appended). The M5.1 projection test now lists `forge.pr.merge` among the protected effects withheld until declared.

## Limitations

- Proven against a wire-faithful GitHub fake; no live merge against real GitHub (PX-125's live leg covers the read paths, not a merge).
- Without webhooks (PX-126) the checks passing is seen only when the Core reads: on Refresh, on the schedule (default every 300 s) or when the request is made.
- A repository whose branch rules require reviews or other conditions is refused by the forge (`NOT_MERGEABLE`, merge when ready turned off with the reason); the Core does not read branch protection beforehand.
- An approved ask whose head moved is never spent but stays `APPROVED` on the approval's record (only a pending approval can be closed); the ledger shows no effect for it.
- macOS only locally; Linux and Windows are CI's.

## CI and merge

- PR 246 first head f76f9e29: run 44989931790 failed in the three Rust jobs (Linux, macOS, Windows), all at the same assertion in `px_213_merge_when_ready.rs` step 11. The header showed merge_phase MERGED while state was still OPEN, because `MergeCompleted` is recorded before the Core's follow-up read. The test checked between the two. The Core behaved as designed; the fix (b550ad49) makes the test wait for the follow-up read to record the request merged. It passed three times in a row locally.
- PR 246 head b550ad49: GitHub Actions run 44994944832 — 19 of 19 jobs success (Rust, node, desktop E2E and packaged desktop on macOS, Ubuntu and Windows; cloud; protocol round trip; dossier integrity; supply chain). Saved as `ci-run-44994944832.json`.
- Squash-merged into `main` as df72d23a on 2026-11-22 (owner: "Yes, merge when green").

## Status

COMPLETE (sealed 2026-11-22).
