# Task Card — PX-214 Reviewer findings one by one in the review bundle, with a command to fix one

## Identity

- Task ID: PX-214 (REQ-PX-214; owner verification); qualification QUAL-PX-214 / PX-E2E-214; Decision Record DR-PX-2026-11-11-021 (MWS-18); AFW-A12, A13, A14 unchanged.
- Evidence tier: release-critical (the bundle's schema grows, a command queues work from a finding, and a dismissal is a new event on the canonical log).
- Prerequisites: PX-208 (COMPLETE), DOC-PX-020 (COMPLETE).
- Branch `wip/px-214-reviewer-findings`, from `main` at 48e0d597.

## Existing-code audit (before any edit)

Traced from the person's review to the reviewer's tool:

- The isolated reviewer answers through `review.report` (`services/modbit-core/src/critique.rs`, `handle_report`). The Core validates each finding against the tree at the candidate revision (`validated`, `unsupported_reason`), stores the whole `ReviewerResult` as a content-addressed object and appends `ReviewerResultRecorded` on the candidate task's log with the counts, the highest severity, the summary and `result_ref`. `latest_result` reads it back. PRODUCTION-WORKING (REQ-EPR-007).
- `ReviewBundle` (`surface.proto`, built in `services/modbit-core/src/review.rs`) carries the plan, the self-review, verification runs, attributions, quarantined checks, diff-invariant findings, CI evidence, PR comment threads and repair attempts. It carries nothing of the reviewer's result: no finding, no verdict, no digest. NOT-FOUND.
- The desktop's Review (`apps/desktop/src/renderer/review/review-model.ts`) says in its header that the reviewer's findings are not in the bundle one by one (PX-214). The findings it shows are the check, rule, self-review and forge ones. NOT-FOUND.
- A command that queues an input from a finding, and a dismissal record: NOT-FOUND in the protocol, the Core, the client, main, the preload and the renderer. The input queue (`SendInput` with a queued mode) is PRODUCTION-WORKING and is the way a fix is queued.
- "The next review re-evaluates it": `critique.rs` builds the reviewer's brief from the candidate; it does not name earlier findings. NOT-FOUND.

## First missing link

The bundle does not read the recorded result. Everything else (the finding's fields, its tree check, the object, the log event) already exists and is owned by the isolated-review path, so the work extends it and adds no second review store.

## Plan

1. Protocol (additive): `ReviewFindingView` and `reviewer_*` fields on `ReviewBundle`; `FixReviewFinding` and `DismissReviewFinding` with their views; fixtures on both sides.
2. Domain: `ReviewFindingDismissed` (who, why, finding id, result digest) on the candidate's log.
3. Core: the bundle reads the latest result and re-hashes the object (a mismatch shows nothing and says so); a finding that failed the tree check is shown as unverified; fix queues one input naming that finding and refuses an unknown or dismissed finding; dismiss records the actor and a reason; the next review's brief lists the open findings to re-evaluate.
4. Desktop: bridge, preload (one channel pair), Review: Walk through, Fix this one, Dismiss.
5. Proof: a real Core with a scripted reviewer that returns three findings; Playwright against the real app; retained mutation checks; the Core suite.

## What was built

- Protocol (`surface.proto`, additive): `ReviewFindingView` (finding_id, severity, category, path, line, claim, suggested_action, verified, unsupported_reason, dismissed, dismissed_by, dismissed_reason, dismissed_offset, fix_requests); `ReviewBundle.reviewer_state/verdict/confidence/summary/candidate_revision/result_digest/task_id/findings/questions/note` (fields 363..372); `FixReviewFinding` -> `ReviewFindingFixQueued`; `DismissReviewFinding` -> `ReviewFindingDismissal`. Fixtures on both sides (`review_bundle_reviewer_findings`, `fix_review_finding`, `review_finding_fix_queued`, `dismiss_review_finding`, `review_finding_dismissal`; Rust encodes, TypeScript decodes and re-encodes byte for byte).
- Domain: `TaskEvent::ReviewFindingDismissed { result_ref, candidate_revision, finding_id, dismissed_by, reason }` on the candidate's log; no state change.
- Core, `services/modbit-core/src/review_findings.rs`, the reading and acting side of the isolated-review path in `critique.rs` (no second store, no scheduler):
  - the bundle reads `latest_result`, fetches the object the log names, re-hashes it, and shows nothing of it when it is missing or does not hash (`reviewer_state` UNREADABLE with the digest the log names and a note); CURRENT when the result is for the candidate's present revision, STALE when the candidate moved on;
  - `verified` is the Core's tree check (`validated` set by `handle_report` for every finding, whatever the reviewer sent); an unverified finding carries its reason;
  - ids: the reviewer's own when a short plain token and unused, else `F<position>`;
  - `FixReviewFinding` (capability `task.author`, the session's lease): refuses BAD_PAYLOAD, UNKNOWN_TASK, TASK_ENDED, NO_REVIEWER_RESULT, RESULT_UNREADABLE, RESULT_CHANGED (the digest the person saw is not the latest), UNKNOWN_FINDING, FINDING_DISMISSED; otherwise appends one `TaskInputQueued` (provenance `review_finding`, FOLLOW_UP, text names that finding only and says so);
  - `DismissReviewFinding` (capability `review.decide`): needs a reason (at most 500 characters), refuses ALREADY_DISMISSED and the same codes; appends one `ReviewFindingDismissed` with the actor as `user:<id>`;
  - both are idempotent by command id (a replay returns the first answer from the log);
  - the next review's brief (`critique::materialize_and_brief`) lists the findings of the latest readable result that no one set aside, marked as an earlier reviewer's words to re-evaluate.
- Desktop: `CoreClient.fixReviewFinding` / `dismissReviewFinding`; `main/review-findings-ipc.ts` (validated ids, digest and reason; the lease; plain data back); one preload channel pair `review:fixFinding` / `review:dismissFinding` (neither is on a pane window's allow list) with `hardening-guard.test.ts`'s pin updated and noted; the reviewer's result rides the existing `review:bundle` channel; Review gets a "The reviewer's findings" block (`reviewer-findings.tsx`): the verdict in words, each finding with severity, place, claim, suggested action, whether the Core found it, Walk through, Fix this one, Dismiss (asks why).

## Verification

macOS 27.0.1 arm64, rustc 1.97.1, node 26.11.1, Electron 44.3.0, Playwright 1.63.0. The model is the repository's scripted stand-in; the Core, its SQLite store and socket, the isolated review environment, Electron main, the preload and the renderer are real.

- `cargo test -p modbit-core --test px_214_reviewer_findings` (`px214-core.log`): passed, 11 s. A real isolated review; the reviewer returns three findings (one real, one about a file that is not in the tree, one with no id). Bundle: ids `f1, f2, F3`, severity, path, line, claim, suggested action, `verified` true/false/true, the phantom one with its reason, the digest equal to the one on the log, the questions. Refusals that write nothing (unknown finding, another digest, no digest, no lease, blank reason, long reason, unknown finding on dismiss). Dismiss: one `ReviewFindingDismissed` on the candidate's aggregate with result_ref, finding id, reason and `user:` actor; a repeated command id replays; a second dismissal and a fix of it refused. The result object altered by one byte, then deleted: `UNREADABLE`, no findings, no verdict, fix refused `RESULT_UNREADABLE`; restored: readable again. Fix `f1`: exactly one `review_finding` input, naming `f1` and none of the others' claims, the candidate returned to work. Resumed: the old report is `STALE` with its findings still shown; the second review's brief lists `f1` and `f2` (the unverified one marked) and not `F3`; the first brief had none; the agent received the one finding; the second result (PASS) replaces the first and a command naming the first digest is refused.
- Playwright against the real app, `px-214-reviewer-findings.spec.ts` (`px214-e2e.log`): 3 of 3 with `--repeat-each 3`, and 6 of 6 earlier. A signed registry is activated in the data directory by the real CLI (the desktop has no bridge for it). Review shows the verdict in words and the three findings equal to the Core's bundle; the unconfirmed one is worded as not confirmed; Walk through; Dismiss asks why and the page and the Core's bundle agree; a dismissed finding cannot be fixed (the Core refuses); Fix this one queues one input visible in the Core's queue, naming `f1` only; the bridge refuses malformed arguments before the Core and relays the Core's own refusals; resuming shows the old report as stale, then the next review's report. axe clean on the page with findings; no raw codes in its text.
- Mutation checks, `mutation-check-2026-11-19.txt`: twelve mutations (a failed finding shown as verified; unknown finding not refused; dismissed finding fixable; another digest acted on; dismissal without a reason; fix input naming every finding; the next brief listing a dismissed finding; an unreadable result treated as none; a dismissal not recording who; the page wording an unconfirmed finding as found, in unit tests and in the real app; the bridge accepting a blank reason). Each made its test fail.
- Desktop unit: `pnpm --filter @modbit/desktop test` 472 passed. Neighbours on the same tree: `review.spec.ts`, `review-escape`, `px-208`, `px-104`, `forge-evidence`: 11 passed.
- Core suite, `cargo test -p modbit-core --locked --no-fail-fast` on 99b93b34 (no Rust or proto changed after it): 690 passed, 0 failed, 8 ignored, 48 test binaries (`core-suite.log`). In the first run 18 tests (px_115_mcp 8, px_130_broker 1, surface_protocol 9) failed with "No such file or directory" for `modbit-mcp-testserver`, a helper binary that `-p modbit-core` does not build; after `cargo build -p modbit-mcp-testserver` those three binaries passed (8, 3 and 208 passed). Not a change of this task.
- `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets --locked` (0 warnings), `tools/lint_policy.sh`, `architecture-lint` deps, modules, spawns and locked (0 violations), `protocol-fixtures --check`, `pnpm typecheck`: all exit 0. `cargo test -p protocol-fixtures -p modbit-domain`, `pnpm --filter @modbit/surface-protocol test` (58): passed.
- Real screenshots of the running app beside the Review page's own look (rows, labels, buttons, spacing): `after/review-reviewer-findings-*.png`, `review-walk-through-*`, `review-dismiss-reason-*`, `review-fix-queued-*`, `review-stale-report-*`, `review-second-report-*`, light and dark. They show a "Core recovered" banner: the data directory was prepared by the CLI before the app started, so the app's Core is boot 2.

## Limitations

- Fix on a task that is ReadyForReview also returns it to work (`TaskReturnedToWork`, `TaskWaiting`), as a review's RETURN does; without that the queued input would have no one to take it. On another state it queues only. Either way the person resumes the task; the page says so. A task whose Review is open stays on the Core's own state: the page does not start it.
- A finding the Core could not tie to the tree can still be fixed: the queued text says the Core could not confirm it and asks the agent to check first. A STALE result's findings can be fixed too; the text says which revision they were made at.
- The fix input is not marked untrusted (as the Core's own revision input is not); it carries the reviewer's claim, bounded to 1500 characters, with a Core-written frame naming one finding.
- The next brief reads only the latest result. A finding dismissed on a result stays dismissed only for that result's digest.
- The desktop's Windows build has no review sandbox, so the Core test and the spec are unix-only (`#![cfg(unix)]`, `test.skip` on win32).
- The spec needs `modbit-cli` (built with the workspace in CI's desktop job) to activate a registry before the app starts, and ends the CLI's own Core before launching.
- The page re-reads the review when the task's log moves; a pure state change with no new event on the task's log would not refresh it.
- macOS only so far. Linux and Windows are CI's.
- CI run 44629354029 on the PR's final head 979a85dc: all 19 checks green on Linux, macOS and Windows (`ci-run-44629354029.json`); merged as c73af335 (#236). The owner reviewed the screenshots and asked for the seal on 2026-11-20.

## Status

COMPLETE (sealed 2026-11-20).
