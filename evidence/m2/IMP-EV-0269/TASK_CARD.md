# Task Card — IMP-EV-0269 OutputRef tool pagination

## Identity

- Task ID: IMP-EV-0269
- Milestone: M2 (backlog batch 1: qualification of substrate delivered by M2.1–M2.10)
- Requirement: REQ-EV-0269 (ADOPT); owner label: Artifact Store; subsystem: domain-events
- Qualification: QUAL-EV-0269 — 10MB+ tool result is paged without context overflow and digest matches raw output.
- Evidence tier: real-system or production-equivalent

## Goal

10MB+ tool result is paged without context overflow and digest matches raw output. The substrate for this requirement landed in the M2 milestone tasks; this card records the named qualification proof against the real substrate (real processes, real git, real SQLite, real Core over the socket) on hosted CI.

## Existing-code audit

- classification: FOUND (delivered by the M2 milestone tasks)
- production entry point: crates/tools/src/pipeline.rs OutputRef; services/modbit-execd streaming
- proof: 10 MB of real process output paged by OutputRef; SHA-256 of the pages equals the raw output

## Verification

Named tests (crate/test-binary :: test name), all run on macOS, Linux and Windows by `.github/workflows/ci.yml`:

- `modbit-tools/pipeline_and_direct` :: `qual_ev_0269_large_results_are_paged_by_output_ref_with_matching_digest`
- `modbit-execd/broker` :: `qual_ev_0019_0269_ten_megabytes_stream_in_bounded_chunks_and_output_ref_digest_matches`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- `ci-run-34409173853.json`, `ci-run-34409173853-tests.log`: hosted CI run 34409173853 on 7bdf76d, green on macOS, Linux and Windows
