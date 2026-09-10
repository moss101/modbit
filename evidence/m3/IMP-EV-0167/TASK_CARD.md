# Task Card — IMP-EV-0167 Context compression

## Identity

- Task ID: IMP-EV-0167
- Milestone: M3 (context batch)
- Requirement: REQ-EV-0167; owner label: Context Engine; subsystem: context-engine
- Qualification: `QUAL-EV-0167` — Compression fidelity corpus and handle hydration pass.
- Evidence tier: real-system or production-equivalent

## Goal

Compress lower-priority data with recoverable handles/provenance.

## Existing-code audit

- classification: IMPLEMENTED in this batch as signature stubs (cccb869; was NOT-FOUND); summary compression of prose is not claimed
- production entry point: crates/context/src/lib.rs Stub; ledger stub entries
- proof: lower-priority candidates are compressed to signature stubs with provenance and a recoverable handle; hydrating the handle returns bytes whose hash equals the stub's provenance hash

## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`):

- `left_out_candidates_become_signature_stubs_inside_the_budget_with_hydration_handles`
- `m3_8_context_pack_packs_under_budget_with_provenance_and_the_ledger_records_use`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json/log copies alongside
