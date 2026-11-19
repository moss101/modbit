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

## Status

AUDITING. Nothing is built yet.
