# Task Card — IMP-EV-0011 Blob-addressed large run state

## Identity

- Task ID: IMP-EV-0011
- Milestone: M2 (backlog batch 1: qualification of substrate delivered by M2.1–M2.10)
- Requirement: REQ-EV-0011 (ADAPT); owner label: Artifact Store; subsystem: domain-events
- Qualification: QUAL-EV-0011 — Restart and retrieve multi-MB artifact by digest; digest mismatch fails closed.
- Evidence tier: real-system or production-equivalent

## Goal

Restart and retrieve multi-MB artifact by digest; digest mismatch fails closed. The substrate for this requirement landed in the M2 milestone tasks; this card records the named qualification proof against the real substrate (real processes, real git, real SQLite, real Core over the socket) on hosted CI.

## Existing-code audit

- classification: FOUND (delivered by the M2 milestone tasks)
- production entry point: crates/event-store/src (content-addressed object store; PayloadRef::Object)
- proof: a 3 MB payload is stored by reference, readable after reopen, and a tampered object is refused

## Verification

Named tests (crate/test-binary :: test name), all run on macOS, Linux and Windows by `.github/workflows/ci.yml`:

- `modbit-event-store/real_sqlite` :: `qual_ev_0011_blob_addressed_payloads_survive_reopen_and_digest_mismatch_fails_closed`
- `modbit-core/surface_protocol` :: `qual_ev_0108_multi_mb_object_is_read_in_bounded_ranges`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- `ci-run-34409173853.json`, `ci-run-34409173853-tests.log`: hosted CI run 34409173853 on 7bdf76d, green on macOS, Linux and Windows
