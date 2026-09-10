# Task Card — IMP-EV-0070 Diagnostic Change Window

## Identity

- Task ID: IMP-EV-0070
- Milestone: M3 (context batch)
- Requirement: REQ-EV-0070; owner label: Diagnostics; subsystem: context-engine (headless language service bridge)
- Qualification: `QUAL-EV-0070` — High-noise repo proves only relevant post-change window is evaluated.
- Evidence tier: real-system or production-equivalent

## Goal

Capture baseline before mutation and bounded post-change diagnostics for changed regions.

## Existing-code audit

- classification: IMPLEMENTED in this batch (was NOT-FOUND: `lsp.diagnostics` returned the whole file every time)
- production entry point: services/modbit-core/src/tools.rs `LanguagePort::query` (per-task `DiagBaselines`; `window=changed` diffs the baseline text against the current text and keeps only diagnostics on the changed lines, marking `new_since_baseline`); crates/tools `lsp.diagnostics` `window` argument
- proof: on the python-service fixture with two pre-existing errors, the task's first look records the baseline; after an appended function with a new error, the changed window shows only the appended lines' diagnostics, names the new one, and counts the two noise errors outside the window instead of showing them; the baseline stays the first look

## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`; real pyright):

- `qual_ev_0070_diagnostic_change_window_evaluates_only_the_changed_region`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json/log copies alongside
