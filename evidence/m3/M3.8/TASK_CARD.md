# Task Card — M3.8 Context Pack / token budget / provenance ledger

## Identity

- Task ID: M3.8
- Milestone: M3 (Context intelligence)
- Canonical owner: context-engine (`crates/context` — the canonical `context-retrieval-engine` crate: pack compiler and Context Ledger; `context.pack` / `context.ledger` tools in `crates/tools`; per-task ledgers, excerpting, durable pack objects and use-marking after every tool call in `services/modbit-core/src/tools.rs`)
- Requirement IDs: docs/18 "Context Pack" (pack_id, workspace_revision, query fingerprint, entries with source_ref/span/provenance/freshness/reason/score/token_cost, omitted_summary, token_budget, compiler_version; budget-aware packing — task constraints and critical diagnostics first, then highest marginal evidence utility; Context Ledger records every injected entry and later whether it was used); docs/28 §2 (retrieval record per file at the current workspace revision; ledger records retrieval and later use; a stale-revision record does not count); docs/43 M3.8; REQ-EV-0166 / REQ-EV-0169 / REQ-EV-0168 groundwork (QUAL-PX-015 builds on `ContextLedger::has_record`)
- Risk class: low (read-only compilation over M3.7 results; deterministic estimator, named as such)
- Evidence tier: release-critical
- Release membership: BETA, RELEASE_ZERO (M3)

## Goal

`context.pack` runs the M3.7 planner for a query, turns every fused hit into an excerpt (its line range, or a bounded file head) with provenance — path, workspace revision, content hash, retrieval sources and reasons, excerpt hash — and packs them under an explicit token budget with a named deterministic estimator (`bytes/4`; not a model tokenizer): the task's `required_paths` and diagnostic-linked hits are critical and go first, the rest by score per token, duplicates collapse on span identity or containment (a wider span absorbs the narrower packed entries), what did not fit is summarised (count, tokens, critical count, paths) and a pack with an omitted critical entry is marked incomplete. The pack is stored as a durable object (`pack_ref`) and every entry is recorded in the task's Context Ledger. After every successful tool call the Core marks the ledger entries of the paths it read or wrote (`path` argument, change targets) as used — only at the revision they were retrieved at — with the tool call id and tool name; `context.ledger` returns the ledger, the injected/used counts and a durable snapshot (`ledger_ref`).

## Non-goals

Injecting packs into the model prompt automatically (the prompt compiler's context segment is M4/Skills work; the runtime's `ContextPackCompiled` event still names the compiled prompt segment), compression with recoverable handles (REQ-EV-0167), enforcement of retrieve-before-edit (QUAL-PX-015 — the ledger now provides the record it needs), a model tokenizer.

## Existing-code audit

- classification: NOT-FOUND (`crates/context` carried no behavior; the runtime's context pack id was the hash of a prompt segment with no entries or ledger)
- production entry point: `modbit_context::{pack, estimate_tokens, ContextPack, Entry, Provenance, OmittedSummary, ContextLedger}`; `IndexPort` serves kinds `pack` and `ledger`; `ToolHost::ledger`; use-marking in `ToolHost::invoke`
- real effector/storage boundary: the real workspace files via the exact index; the event store's object store for packs and ledger snapshots

## Invariants

- `token_used <= token_budget` always; a critical entry that cannot fit is reported, never silently dropped.
- Every entry carries the revision and content hash it was read at; the ledger marks use only at that revision.
- Pack ids are deterministic for the same candidates, revision and fingerprint.

## Verification

- `modbit-context/pack` :: `budget_is_never_exceeded_critical_entries_go_first_and_omissions_are_summarised`, `duplicates_collapse_on_span_identity_or_containment`, `ledger_records_injections_and_marks_use_only_at_the_retrieved_revision`
- `modbit-core/surface_protocol` :: `m3_8_context_pack_packs_under_budget_with_provenance_and_the_ledger_records_use`
- `modbit-tools/pipeline_and_direct` :: `qual_ev_0217_compatibility_matrix_covers_every_registered_tool_with_owner_effect_and_test`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json/log copies alongside
