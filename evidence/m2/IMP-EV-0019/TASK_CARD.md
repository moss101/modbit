# Task Card — IMP-EV-0019 Terminal scrollback as artifact

## Identity

- Task ID: IMP-EV-0019
- Milestone: M2 (backlog batch 1: qualification of substrate delivered by M2.1–M2.10)
- Requirement: REQ-EV-0019 (ADOPT); owner label: Terminal Broker; subsystem: terminal
- Qualification: QUAL-EV-0019 — Generate >10MB output; UI can replay, model receives bounded view, full artifact remains retrievable.
- Evidence tier: real-system or production-equivalent

## Goal

Generate >10MB output; UI can replay, model receives bounded view, full artifact remains retrievable. The substrate for this requirement landed in the M2 milestone tasks; this card records the named qualification proof against the real substrate (real processes, real git, real SQLite, real Core over the socket) on hosted CI.

## Existing-code audit

- classification: FOUND (delivered by the M2 milestone tasks)
- production entry point: services/modbit-execd streaming + crates/tools OutputRef bounded views
- proof: bounded chunks for replay, bounded model view, full artifact by digest

## Verification

Named tests (crate/test-binary :: test name), all run on macOS, Linux and Windows by `.github/workflows/ci.yml`:

- `modbit-execd/broker` :: `qual_ev_0019_0269_ten_megabytes_stream_in_bounded_chunks_and_output_ref_digest_matches`
- `modbit-tools/pipeline_and_direct` :: `shell_exec_and_test_run_go_through_the_real_broker`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- `ci-run-34409173853.json`, `ci-run-34409173853-tests.log`: hosted CI run 34409173853 on 7bdf76d, green on macOS, Linux and Windows
