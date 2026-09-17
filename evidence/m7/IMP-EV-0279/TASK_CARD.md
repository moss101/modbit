# Task Card — IMP-EV-0279 Incremental page-state deltas

## Identity

- Task ID: IMP-EV-0279
- Milestone: M7 Live browser (P0)
- Requirement: REQ-EV-0279; disposition ADOPT
- Mandatory behavior: After baseline snapshot, send bounded semantic deltas when safe; full rehydrate fallback.
- Qualification: `QUAL-EV-0279` — Long navigation flow shows token reduction and state equivalence.
- Evidence tier: real-system (the real Core with a fake host; the real app on real pages)

## Goal

After baseline snapshot, send bounded semantic deltas when safe; full rehydrate fallback.

## Existing-code audit

- classification: IMPLEMENTED by M7.3.
- production entry points: `crates/browser/src/compiler.rs` (`diff`, `apply`, `state_fingerprint`), `crates/tools/src/browser.rs` (`browser.snapshot` delta mode with the full-rehydrate fallback).
- proof: `qual_m7_3_…`: a long flow's deltas are a fraction of the page and applying them reproduces the page (state equivalence); `a_delta_is_bounded_to_what_changed_and_applies_to_the_same_page`.

## Limitations

- Token reduction is measured in bytes of the observation.

## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`):

- `qual_m7_3_page_reads_after_the_first_are_bounded_deltas_between_fingerprints`
- `a_delta_is_bounded_to_what_changed_and_applies_to_the_same_page`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json copy alongside
