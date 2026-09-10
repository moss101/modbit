# Task Card — IMP-EV-0157 Dependency/call graph

## Identity

- Task ID: IMP-EV-0157
- Milestone: M3 (context batch)
- Requirement: REQ-EV-0157; owner label: Context Engine; subsystem: context-engine
- Qualification: `QUAL-EV-0157` — Impact benchmark checks affected file/test recall.
- Evidence tier: real-system or production-equivalent

## Goal

Traverse callers/callees/contracts/ownership/impact.

## Existing-code audit

- classification: IMPLEMENTED by M3.6/M3.7/M3.9 (was NOT-FOUND)
- production entry point: graph relations imports/importers/tests/cochange/owners/evidence; planner L2/L3 expansion
- proof: the graph answers importers, tests reaching a path, co-change and ownership; the benchmark's impact cases measure affected-test recall (impact accuracy 1.0 for profile C at the sealed commit)

## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`):

- `harness_measures_the_three_profiles_on_a_real_corpus_and_reports_them`
- `m3_6_evidence_graph_serves_imports_history_changed_lines_tests_and_verification_evidence`
- `structural_and_engineering_intents_expand_through_the_graph_with_distance_and_diagnostic_boosts`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json/log copies alongside
