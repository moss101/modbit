# Task Card — PX-195 Home is one composer: send creates, starts and opens; Alt+Enter leaves

## Identity

- Task ID: PX-195 (REQ-PX-195; owner desktop); qualification QUAL-PX-195; Decision Record DR-PX-2026-11-04-020
- Evidence tier: release-critical (send now starts a run and the approval's decision surface moved)
- Prerequisites: PX-194, DOC-PX-019

## Existing-code audit (before any edit)

- classification: BROKEN-DRIFTED against the approved mockup: the Fleet board was still the home under DR-019; first missing link: a home without the board and a send that starts the task.

## What was built

- Home is one composer: send creates, starts and opens; Alt+Enter leaves the task queued; no provider leaves it queued and says so. board.tsx, card.tsx, columns.ts, composer.tsx and fleet-view.tsx are deleted; welcome, settings and the status region moved; conversation/task-details.tsx and approvals/inbox.tsx give the board's facts a home.

## Verification

- tests: `apps/desktop/e2e/awl.spec.ts`, `apps/desktop/e2e/fleet.spec.ts`, `apps/desktop/e2e/onboarding.spec.ts`, `apps/desktop/e2e/states.spec.ts`; the whole desktop e2e suite against a real Core (evidence.json).

## Limitations

- Real proof so far is local, macOS arm64, against a real Core and scripted models: the whole desktop e2e suite on the working tree ran 186 passed, 2 skipped and 1 failed; the failure was gallery-projects.spec.ts counting the Projects heading, fixed in the spec and that spec then passed alone (the whole suite was not rerun after that one-line spec change). CI on macOS, Linux and Windows has not run yet. The owner's review of real screenshots beside the mockup is an exit criterion of the record and has not closed. So the rows are not COMPLETE.
- Checks deleted on purpose because their surface was removed (each named by the agent or spec that moved it): composer.spec suggest-plan chip (the mode menu replaces it); wave4.spec deciding an approval on the inline transcript card (the docked card decides it; the transcript's screen-reader row is asserted); projects.spec agents-section "projects" absent before a project exists (the heading is always there); qual-gaps.spec a no-op nav-fleet click that asserted nothing; keyboard.spec board column keys, focus surviving a column move and the type-ahead filter narrowing the board (no board; / opens the palette, which is asserted); fleet.spec column counts (asserted as data-state on rows). Moved, not deleted: every other fact, through task details, the Approvals page or Settings.
