# Task Card — PX-113 Engineering memory wired into the prompt and the event log, with Agent and Space scopes

## Identity

- Task ID: PX-113 (REQ-PX-113; owner memory); qualification QUAL-PX-113
- Evidence tier: release-critical (effect-bearing behavior; see the acceptance below)
- Closed from E2E_PROVEN to COMPLETE on 2026-10-20 after the re-run recorded in evidence.json.

## Existing-code audit

- adoption audit: DOCUMENTED-ONLY; no product caller present
- classification now: PRODUCTION-WORKING (landed on main; verified by the tests below through the real Core)

## Verification

- acceptance: Real Core and the scripted provider capturing requests: a memory promoted in session one appears in session two's request with its id and provenance and the Inspector lists it; forgetting it removes it from the next request and from the rebuilt store after a Core restart; promote and forget appear in the event log and replay to the same store; a memory with an expired time to live is not injected; Agent and Space scoped memories obey precedence; a hostile memory text asking for tool access changes no projection; the segment stays inside its budget with fifty memories.
- tests: `services/modbit-core/tests/memory_graph_index.rs::px_113_a_promoted_memory_reaches_a_later_sessions_prompt_and_forgetting_removes_it_after_a_restart`, `services/modbit-core/tests/memory_graph_index.rs::px_113_mutations_are_events_that_replay_to_the_same_store_and_a_direct_write_does_not_survive`, `services/modbit-core/tests/memory_graph_index.rs::px_113_ttl_scope_precedence_and_sensitivity_decide_what_is_injected`
- re-run: macOS local 2026-10-20 and CI run 39029466637 on main ac4aa804, all green.

## Limitations

- Windows and Linux legs are proven by CI only; no release gate is attested by this task.

## Evidence

- `evidence.json` and `ci-run-39029466637.json` in this directory
