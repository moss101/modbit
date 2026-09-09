# Task Card — IMP-EV-0106 Typed file-change patch/diff events

## Identity

- Task ID: IMP-EV-0106
- Milestone: M2 (backlog batch 2)
- Requirement: REQ-EV-0106 (ADOPT); owner label: Change Engine; subsystem: workspace-git
- Qualification: QUAL-EV-0106 — Apply real patch and verify UI/evidence sees identical diff.
- Evidence tier: real-system or production-equivalent

## Goal

Apply real patch and verify UI/evidence sees identical diff.

## Existing-code audit

- classification: IMPLEMENTED in this batch (was NOT-FOUND: writes produced only a tool result)
- production entry point: services/modbit-core/src/tools.rs file_changed_events (FileChanged on the Workspace aggregate; before/after content and unified diff by object ref); crates/domain/src/workspace.rs
- proof: every successful change.apply/change.batch lands FileChanged with revision before/after and refs; the review bundle's content refs and hunk lines equal the evidence diff

## Verification

Named tests (crate/test-binary :: test name), run on macOS, Linux and Windows by `.github/workflows/ci.yml`:

- `modbit-core/surface_protocol` :: `qual_ev_0106_every_write_lands_a_revision_bound_file_changed_event_matching_the_review_bundle`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- `ci-run-34413048026.json`, `ci-run-34413048026-tests.log`: hosted CI run 34413048026 on a4a456d, green on macOS, Linux and Windows
