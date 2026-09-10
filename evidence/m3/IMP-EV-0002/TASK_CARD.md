# Task Card — IMP-EV-0002 Read-through freshness hydration

## Identity

- Task ID: IMP-EV-0002
- Milestone: M3 (context batch)
- Requirement: REQ-EV-0002; owner label: Context Engine; subsystem: context-engine
- Qualification: `QUAL-EV-0002` — Mutate indexed file after indexing; stale bytes must never reach model context.
- Evidence tier: real-system or production-equivalent

## Goal

Indexes rank candidates, but source bytes are re-read from the active revision before prompt inclusion.

## Existing-code audit

- classification: IMPLEMENTED in this batch (cccb869; was PARTIAL: excerpts came from the index text)
- production entry point: services/modbit-core/src/tools.rs IndexPort kind `pack` (`hydrated` closure)
- proof: context.pack re-reads every excerpt from disk at the active revision and hashes it; the m3_8 test edits a file behind the index and the pack carries the new bytes labelled rehydrated_from_active_revision with the current hash (fs.read agrees)

## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`):

- `m3_8_context_pack_packs_under_budget_with_provenance_and_the_ledger_records_use`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json/log copies alongside
