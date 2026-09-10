# Task Card — IMP-EV-0169 Context provenance

## Identity

- Task ID: IMP-EV-0169
- Milestone: M3 (context batch)
- Requirement: REQ-EV-0169; owner label: Context Engine; subsystem: context-engine + core-runtime
- Qualification: `QUAL-EV-0169` — Prompt envelope validates provenance on every non-ephemeral fragment.
- Evidence tier: real-system or production-equivalent

## Goal

Fragments carry source, repository path, revision, hash and retrieval reason; the prompt envelope validates that provenance and injects nothing without it.

## Existing-code audit

- classification: IMPLEMENTED in this batch (M3.8 gave pack entries their provenance, but nothing consumed a pack and the envelope had no context segment)
- production entry point: `crates/prompt-compiler/src/lib.rs` — `ContextFragment` with `provenance_complete` and `render`, the context segment, and `CompiledPrompt.injected_fragments` / `rejected_fragments`; `crates/context` keeps the task's latest pack on the ledger; `services/modbit-core/src/runtime.rs` turns that pack's entries into fragments for every turn and records what was injected and refused on the ContextCompile step object
- proof: a fragment missing its content hash, its retrieval reason or its revision is refused by the compiler and never appears in the request, while a complete one is rendered with `--- workspace:<path> L<a>-<b> @revision <n> hash <12 hex> (<reason>)`; the context is part of the pack segment, so a different context is a different pack id; on a real run the pack the task compiled reaches the next prompt with provenance on every line, labelled as data and never instructions, and the step object lists the injected fragments with an empty rejection list

## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`):

- `tests::context_fragments_without_provenance_are_refused_not_injected`
- `qual_ev_0169_context_pack_reaches_the_prompt_with_provenance_or_not_at_all`
- `m3_8_context_pack_packs_under_budget_with_provenance_and_the_ledger_records_use` (the provenance the fragments carry)

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json/log copies alongside
