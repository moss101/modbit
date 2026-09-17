# Task Card — M7.3 State fingerprints + delta stream

## Identity

- Task ID: M7.3
- Milestone: M7 Live browser (P0)
- Requirements: docs/22 "Semantic Browser Compiler" (state fingerprint, delta/diff generation; full AX snapshots are not repeatedly dumped into context — after the initial state, incremental semantic patches are preferred), REQ-EV-0279 (after the baseline snapshot, bounded semantic deltas when safe; full rehydrate fallback), REQ-EV-0280 (observed transitions recorded as evidence; a changed page invalidates the fingerprint).
- Qualification: QUAL-EV-0279 (a long navigation flow shows token reduction and state equivalence), QUAL-EV-0280 (a changed page invalidates the fingerprint; the transition reuse half belongs to M7.4's actions).
- Evidence tier: real-system (the real Core with a fake host serving a long flow; the real app on a real page)

## Goal

Pay for what changed, not for the page again: every read after the first is a bounded delta between two content fingerprints, equivalent to re-reading the page; a page that changed most of itself comes back in full; every read is on the log.

## Existing-code audit

- classification: MISSING before this task: M7.2's `browser.snapshot` returned the full compiled page on every call; no content fingerprint, no delta, no record of reads.
- production entry points: `crates/browser/src/compiler.rs` (`state_fingerprint`, `EntityChange`, `PageDelta` with `is_empty` / `size`, `diff`, `apply`); `crates/browser/src/lib.rs` (`BrowserPort::last_page`); `services/modbit-core/src/browser.rs` (`SessionRecord.last_page`); `crates/tools/src/browser.rs` (`browser.snapshot` `mode: full | delta`, the delta answer, `DELTA_FALLBACK_SHARE`, `full_json` with `state_fingerprint`); `crates/domain/src/task.rs` (`BrowserPageObserved`); `services/modbit-core/src/tools.rs` (the event journaled beside every successful read).
- proof: `qual_m7_3_page_reads_after_the_first_are_bounded_deltas_between_fingerprints` (real Core, a fake host serving a forty-message inbox): read 1 is the full page (43 entities) with a fingerprint; read 2, a re-render with fresh DOM node ids and no change, is `unchanged` between the same fingerprints at under a fifth of the page's bytes; read 3, one message arrived and the search box typed in, is a delta naming exactly the added link and the changed field (before/after), the text lines added and removed, with a new fingerprint, at under a quarter of the page; read 4, a different screen, rehydrates in full with `delta_fallback` naming the version it left and a new fingerprint; read 5 with `mode: full` is full with no fallback and the same fingerprint as read 4; the task's log holds five `BrowserPageObserved` with the modes, fingerprints and counts. `apps/desktop/e2e/browser.spec.ts` (real app, real page): the read after navigating to the re-served page is a delta naming the renamed button (one added, one removed), `unchanged: false`, the new URL, smaller than the page. Unit: `a_delta_is_bounded_to_what_changed_and_applies_to_the_same_page` (equivalence: `apply(prev, diff(prev, next))` has `next`'s entity hash, fingerprint and text; no change → an empty delta between equal fingerprints).

## Limitations

- The host's state version counts navigations and loads; a DOM mutation between two reads is detected by the fingerprint at the next read, not pushed as it happens (a CDP mutation observer arrives with M7.4's postconditions).
- Transition reuse (QUAL-EV-0280's first half) needs actions (M7.4): the fingerprints and `BrowserPageObserved` are the evidence it will key on.
- The fallback threshold is a fixed share (half the page's entities).

## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`):

- `qual_m7_3_page_reads_after_the_first_are_bounded_deltas_between_fingerprints` (services/modbit-core, real Core)
- `apps/desktop/e2e/browser.spec.ts` (the M7.3 step)
- `a_delta_is_bounded_to_what_changed_and_applies_to_the_same_page` (crates/browser)
- Regression: `qual_m7_1_…`, `qual_m7_2_…`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json copy alongside
