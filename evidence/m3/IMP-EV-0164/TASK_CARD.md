# Task Card — IMP-EV-0164 Graph expansion

## Identity

- Task ID: IMP-EV-0164
- Milestone: M3 (context batch)
- Requirement: REQ-EV-0164; owner label: Context Engine; subsystem: context-engine
- Qualification: `QUAL-EV-0164` — Budget cap prevents runaway expansion.
- Evidence tier: real-system or production-equivalent

## Goal

Bounded expansion from symbol to callers/tests/config/evidence.

## Existing-code audit

- classification: IMPLEMENTED by M3.6/M3.7 (was NOT-FOUND)
- production entry point: EvidenceGraph::expand (depth clamped to 3, max), planner seeds (3) and max_hits
- proof: expansion from a seed to importers/tests/co-change/evidence is bounded by depth, seed count and the hit ceiling; the planner test caps results at max_hits

## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`):

- `structural_and_engineering_intents_expand_through_the_graph_with_distance_and_diagnostic_boosts`
- `graph_answers_imports_importers_history_tests_evidence_and_refreshes_per_path`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json/log copies alongside
