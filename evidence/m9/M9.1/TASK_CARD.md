# Task Card — M9.1 Engineering Memory schemas/scopes/promotion

## Identity

- Task ID: M9.1 (milestone task; the substance is IMP-EV-0162)
- Milestone: M9 Engineering memory / effects / security hardening (P0/P1)
- Requirements: REQ-EV-0162; QUAL-EV-0162; docs/19 "Engineering Memory".
- Qualification: a real Core over the durable memory store — propose, query, the governed promotion commands, a second task reading the curated result.
- Evidence tier: real-system.

## Goal

Engineering Memory as a governed subsystem: schemas (scope, record type, provenance, confidence, TTL, sensitivity, supersedes/conflicts, last validation revision), scopes (run < session < user < agent profile < repository < space < organization), and promotion — memory cannot be created from a transcript without an explicit governed promotion.

## Summary

Delivered as IMP-EV-0162 (see `evidence/m9/IMP-EV-0162/TASK_CARD.md` for the full audit and entry points): the `crates/memory` domain, the durable `memory_items` store (event-store V16), the `memory.query`/`memory.propose` tools with `MemoryPort`, the `CoreMemory` port over the task's scope chain, and the governed `PromoteMemory`/`ListMemory`/`ForgetMemory` commands. Proven by `qual_ev_0162_…`: a proposal is never read by a query, a transcript summary never auto-promotes, promotion is a separate governed step with typed refusals, curated memory is read by a later task, and conflict/supersession are inspectable.

## Limitations

- Automatic Context-Pack injection of curated memory (docs/18) is a follow-up; memory reaches the model through the explicit `memory.query` tool.
- Finer promotion authority and sensitive-scope policy are conservative defaults (sensitive promotion refused pending that wiring).

## Verification

- `services/modbit-core/tests/surface_protocol.rs::qual_ev_0162_memory_is_proposed_read_and_promoted_under_governance_no_transcript_auto_promotes`
- `modbit_memory` unit tests; `modbit-event-store` memory round-trip; `modbit-tools` memory-tool test; `modbit-core` memory module tests

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- `evidence/m9/IMP-EV-0162/` for the full task card
