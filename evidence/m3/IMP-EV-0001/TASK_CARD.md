# Task Card — IMP-EV-0001 Dynamic context query planning

## Identity

- Task ID: IMP-EV-0001
- Milestone: M3 (context batch)
- Requirement: REQ-EV-0001; owner label: Context Engine; subsystem: context-engine
- Qualification: `QUAL-EV-0001` — Fixed-revision repository benchmark proves planner choice, recall, latency and token cost.
- Evidence tier: real-system or production-equivalent

## Goal

Classify query intent and escalate L0 exact → L1 hybrid → L2 structural → L3 engineering only when required.

## Existing-code audit

- classification: IMPLEMENTED by M3.7/M3.9 (was NOT-FOUND)
- production entry point: crates/retrieval/src/planner.rs (features, start_level, retrieve); crates/retrieval/src/bench.rs; search.retrieve
- proof: the planner classifies path/identifier/natural-language/structural/engineering wording, starts at the cheapest level and escalates only while coverage is short; the fixed-revision benchmark (evidence/m3/M3.9/report.json) records per case the level reached, recall@5, latency and the context-token estimate of the top K

## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`):

- `query_features_pick_the_cheapest_level`
- `l0_serves_a_known_identifier_without_escalating_and_boosts_the_symbol`
- `unknown_wording_starts_hybrid_and_a_miss_escalates_only_while_coverage_is_short`
- `harness_measures_the_three_profiles_on_a_real_corpus_and_reports_them`
- `m3_7_retrieval_planner_starts_cheap_escalates_on_short_coverage_and_fuses_with_boosts`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json/log copies alongside
