# Task Card — PX-027 Language tier conformance suites A, B and C

## Identity

- Task ID: PX-027
- Milestone: M3 (release BETA)
- Requirement: REQ-PX-027; owner label: verification; subsystem: verification
- Qualification: `QUAL-PX-027` — Tier A, B and C conformance suites exist and run on real fixture repositories; a language enters a tier only through a recorded pass.
- Evidence tier: real-system or production-equivalent

## Goal

Implement the tier suites in docs/76 over real fixture repositories and make tier entry a recorded pass; add the suites to docs/56.

## Existing-code audit

- classification: NOT-FOUND before this task. docs/76 defined the tiers and PX-026 shipped honest Alpha labels, but nothing ran a tier suite, and nothing stopped a label from claiming a tier no run had earned.
- production entry point: `crates/verification/src/tiers.rs` (the suites, `earned_tier`, `verify_record`, `tier_of`); `crates/verification/language-tiers.json` (the recorded passes, compiled into the build); `services/modbit-core/src/languages.rs` (every client's label carries the recorded line); docs/56 lists the checks.
- proof: the eight checks are the product doing the thing each tier claims, on the real fixture repositories — a CRLF file edited through the Change Engine and read back byte for byte, exact and BM25 hits bound to the index revision, a verification run on the task carrying checks with an environment digest, tree-sitter finding two definitions, a failing check located in a file with a line from the parser, an edit at a stale revision refused with the file untouched, the headless language service returning symbols and a known reference, and the same service reporting a seeded defect at its line and nothing once it is fixed. A tier is every check of it and of every tier below it: `earned_tier` gives nothing for a skipped check, `verify_record` refuses a record that claims more than its checks show, and the suite refuses a record that claims more than the run earned. On this run rust, python and typescript each earned Tier A on their fixtures, and that is what the record file says.

## Limitations

The record is what a claim may rest on; it is not the claim. The product's labels still read ALPHA_BASELINE, and promoting a language into a tier is PX-028. Tier A in docs/76 also names incremental index latency within a budget and the competence suite tasks at baseline: neither is measured here, and each record says so in `not_claimed`. A language service that does not answer on a machine records a skip, so that run earns no Tier A there; the recorded pass from the run that earned it stands, and the suite prints which checks did not run. JavaScript has no fixture of its own and therefore no record.

## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`):

- `qual_px_027_language_tier_suites_run_on_real_fixtures_and_a_tier_is_only_a_recorded_pass`
- `a_tier_is_every_check_of_it_and_of_every_tier_below_it`
- `a_record_that_claims_more_than_it_shows_is_refused`
- `every_shipped_record_stands_and_an_unrecorded_language_is_unsupported`
- `qual_px_026_language_labels_are_honest_and_served_to_every_client`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json/log copies alongside
