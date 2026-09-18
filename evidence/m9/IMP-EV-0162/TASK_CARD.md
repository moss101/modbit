# Task Card — IMP-EV-0162 Organizational engineering memory (M9.1)

## Identity

- Task ID: IMP-EV-0162 (the substance of milestone task M9.1 "Engineering Memory schemas/scopes/promotion")
- Milestone: M9 Engineering memory / effects / security hardening (P0/P1)
- Requirements: REQ-EV-0162 (validated facts with scope/provenance/TTL/edit/delete); QUAL-EV-0162 (memory conflict/supersession is inspectable and no raw transcript auto-promotes); docs/19 "Engineering Memory", docs/17 (memory.query/propose), docs/18 (the Skill-Evolution/Memory boundary).
- Qualification: a real Core over the real durable store — a scripted run proposes and reads memory, governed commands promote/forget, a second task reads the curated result.
- Evidence tier: real-system.

## Goal

Governed engineering memory: an agent proposes candidates and reads curated memory through tools, but a candidate never becomes durable memory on its own — promotion is a separate governed step with typed refusals, conflicts and supersession are inspectable, and memory is scoped, provenance-bound, TTL'd and editable.

## Existing-code audit

- classification: MISSING before this task: `crates/memory` was an empty stub; no memory schema, store, tools, promotion or scopes existed.
- production entry points: `crates/memory/src/lib.rs` (the pure domain: `Scope` (run<session<user<agent_profile<repository<space<organization), `RecordType`, `Source` with `promotable_unvalidated`, `Sensitivity`, `Status`, `MemoryItem` with content-addressed `propose`, `promotion_check`→`PromotionRefusal`, `MemoryStore` with query/promote/conflicts), `crates/event-store` (schema V16 `memory_items` — memory's own durable mutable store, not a projection; `MemoryRow` CRUD `memory_upsert`/`memory_get`/`memory_in_scopes`/`memory_all`), `crates/tools` (`MemoryPort` on InvokeContext; the `memory.query`/`memory.propose` tools), `crates/policy/src/kernel.rs` (`memory.query`/`memory.propose` in the profile lease defaults; excluded from `review_isolated` propose), `services/modbit-core/src/memory.rs` (`CoreMemory` over the store and the task's scope chain; `promote` applying the gate; `list_scoped`; `set_status`), `services/modbit-core/src/server.rs` (`ListMemory`/`PromoteMemory`/`ForgetMemory` commands), `crates/protocol` (`MemoryItemView`, `ListMemory`/`MemoryList`, `PromoteMemory`/`MemoryPromoted`, `ForgetMemory`/`MemoryForgotten`).
- proof: `services/modbit-core/tests/surface_protocol.rs::qual_ev_0162_memory_is_proposed_read_and_promoted_under_governance_no_transcript_auto_promotes` — a run proposes a user preference (user_stated), a fact (transcript_summary) and two conventions on one topic, and its `memory.query` reads none (proposals are not curated); `PromoteMemory` refuses the transcript fact (`TRANSCRIPT_NOT_VALIDATED`) and curates the rest; a later task's `memory.query` reads the curated preference and both conventions but never the still-proposed fact; the two conventions are an inspectable conflict; `ForgetMemory` supersedes one and the conflict clears. Domain unit tests cover the five promotion rules and the query/supersession/conflict semantics; the event-store round-trips memory rows across reopen; a tools crate test drives the tools against a fake port; the Core module tests the scope chain and row round-trip.

## Limitations

- Automatic injection of curated memory into the Context Pack (docs/18 "repository engineering memory with scope/confidence") is not wired here; memory reaches the model through the explicit `memory.query` tool. (A retrieval-side port is the natural follow-up.)
- Promotion authority is a session lease (`task.author`); a finer per-scope promotion policy (who may promote into org scope) and `scope_permits_sensitive` from policy are conservative defaults (sensitive promotion is refused pending that wiring).
- A blueprint-style bulk import and cross-tenant organization memory sharing are out of scope for M9.1.

## Verification

- `services/modbit-core/tests/surface_protocol.rs::qual_ev_0162_memory_is_proposed_read_and_promoted_under_governance_no_transcript_auto_promotes`
- `modbit_memory` unit tests; `modbit-event-store` `engineering_memory_rows_round_trip_query_by_scope_and_survive_reopen`; `modbit-tools` `memory_tools_delegate_to_the_host_port_and_never_promote`; `modbit-core` memory module unit tests

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json copy alongside
