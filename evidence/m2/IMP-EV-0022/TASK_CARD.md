# Task Card — IMP-EV-0022 Git snapshot local→cloud handoff

## Identity

- Task ID: IMP-EV-0022
- Milestone: M2 (backlog batch 1: qualification of substrate delivered by M2.1–M2.10)
- Requirement: REQ-EV-0022 (ADAPT); owner label: Workspace Fabric; subsystem: workspace-git
- Qualification: QUAL-EV-0022 — Cloud run reconstructs dirty state exactly and cleanup removes temporary refs safely.
- Evidence tier: real-system or production-equivalent

## Goal

Cloud run reconstructs dirty state exactly and cleanup removes temporary refs safely. The substrate for this requirement landed in the M2 milestone tasks; this card records the named qualification proof against the real substrate (real processes, real git, real SQLite, real Core over the socket) on hosted CI.

## Existing-code audit

- classification: FOUND (delivered by the M2 milestone tasks)
- production entry point: crates/git/src (dirty snapshot ref for handoff)
- proof: the snapshot reproduces the dirty tree byte-for-byte and cleanup leaves no temporary ref

## Verification

Named tests (crate/test-binary :: test name), all run on macOS, Linux and Windows by `.github/workflows/ci.yml`:

- `modbit-git/real_git` :: `qual_ev_0022_dirty_snapshot_reconstructs_exactly_and_cleanup_removes_the_ref`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- `ci-run-34409173853.json`, `ci-run-34409173853-tests.log`: hosted CI run 34409173853 on 7bdf76d, green on macOS, Linux and Windows
