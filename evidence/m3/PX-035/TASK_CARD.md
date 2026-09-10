# Task Card — PX-035 Impact-based test selection from the evidence graph

## Identity

- Task ID: PX-035
- Milestone: M3
- Requirement: REQ-PX-035; owner: context-engine; docs/64 §6
- Qualification: QUAL-PX-035 — on the fixtures the impact selector chooses tests from dependency, symbol-reference, test-link and Git co-change evidence; precision and recall against the full-suite ground truth are recorded with the retrieval benchmarks; until the task is COMPLETE the plan states that targeting is heuristic. Negative proof: a selector that omits a test failing in the full suite for a changed symbol within the depth bound fails; a selection result without recorded precision and recall is rejected
- Evidence tier: real-system or production-equivalent

## Goal

Select the checks a change could break from the evidence graph within a bounded depth, and measure the selection instead of trusting it.

## Existing-code audit

- classification: IMPLEMENTED in this batch (M3.6 built the evidence graph and M3.9 the benchmark harness; nothing selected tests from them)
- production entry point: `crates/retrieval/src/impact.rs` — `select_impacted` (test links, import dependencies within the depth bound, symbol references restricted to definitions the changed file alone owns, Git co-change, and the changed file itself when it is a test), `precision_recall`, `HEURISTIC_LIMITATION`; `search.impact` tool in `crates/tools`; the Core's `IndexPort` serves kind `impact` and every derived verification plan carries the heuristic-targeting limitation
- proof: on the real rust-cli fixture a change to `src/lib.rs` selects `tests/quantities.rs` with the evidence that chose it and the symbols it searched, at precision 1.00 and recall 1.00 against the fixture's full suite; a change to the test file selects that file with reason `changed` at distance 0; on the repository corpus the benchmark harness records the selection for every case that carries ground truth (mean precision 0.27, recall 1.00 at the sealed commit — recall is the property the qualification protects: a covering test is never omitted); the selection always carries its limitation and the verification plan repeats it

## Measured at this commit

| measure | rust-cli fixture | repository corpus (benchmark) |
|---|---|---|
| precision | 1.00 | 0.27 |
| recall | 1.00 | 1.00 |

Precision on the wider corpus is low because a symbol reference in any test file is evidence; the selector prefers recall, and the mandatory COMPLETION run is unaffected.

## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`):

- `qual_px_035_impact_selection_chooses_tests_from_graph_evidence_with_measured_precision_and_recall`
- `selection_uses_test_links_dependencies_symbol_references_and_cochange`
- `precision_and_recall_are_measured_against_the_ground_truth`
- `harness_measures_the_three_profiles_on_a_real_corpus_and_reports_them` (the report carries the impact section)

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- `../M3.9/report.json` — the benchmark report with the impact measurements
- CI run json/log copies alongside
