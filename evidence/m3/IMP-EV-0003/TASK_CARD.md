# Task Card — IMP-EV-0003 Signature-only retrieval stubs

## Identity

- Task ID: IMP-EV-0003
- Milestone: M3 (context batch)
- Requirement: REQ-EV-0003; owner label: Context Engine; subsystem: context-engine
- Qualification: `QUAL-EV-0003` — Context budget test proves hydration occurs only when requested and provenance survives.
- Evidence tier: real-system or production-equivalent

## Goal

Lower-ranked candidates use symbol/signature stubs with lazy hydration.

## Existing-code audit

- classification: IMPLEMENTED in this batch (cccb869; was NOT-FOUND)
- production entry point: crates/context/src/lib.rs (Stub, pack stub reserve); Core signatures from the symbol index
- proof: left-out candidates become signature-only stubs with provenance and a hydrate handle inside the budget; hydration happens only through a later fs.read, which the ledger records as the use, and the read's content hash equals the stub's provenance hash

## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`):

- `left_out_candidates_become_signature_stubs_inside_the_budget_with_hydration_handles`
- `m3_8_context_pack_packs_under_budget_with_provenance_and_the_ledger_records_use`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json/log copies alongside
