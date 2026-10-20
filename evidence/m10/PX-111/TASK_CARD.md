# Task Card — PX-111 Persisted incremental indexes and an indexed grep path

## Identity

- Task ID: PX-111 (REQ-PX-111; owner context-engine); qualification QUAL-PX-111
- Evidence tier: release-critical (effect-bearing behavior; see the acceptance below)
- Closed from E2E_PROVEN to COMPLETE on 2026-10-20 after the re-run recorded in evidence.json.

## Existing-code audit

- adoption audit: DOCUMENTED-ONLY; no product caller present
- classification now: PRODUCTION-WORKING (landed on main; verified by the tests below through the real Core)

## Verification

- acceptance: Real Core and a large real repository fixture: after one full build, a restarted Core serves the first query without rebuilding (build counter zero) and the results equal those of a fresh build; a single-file edit refreshes below the recorded 103 ms baseline at the stated tolerance and a hundred-file change refreshes proportionally; a corrupted index file (a flipped byte) is detected, discarded and rebuilt with a reason and the answers stay correct; indexed exact search returns byte-identical results to the scan on a corpus of two hundred queries and is faster at the stated margin; a SIGKILL during an index write leaves either the previous whole index or a detected-corrupt one that is rebuilt.
- tests: `services/modbit-core/tests/memory_graph_index.rs::px_111_a_restarted_core_does_not_rebuild_and_damage_or_staleness_is_rebuilt_with_a_reason`, `services/modbit-core/tests/memory_graph_index.rs::px_111_a_core_killed_inside_an_index_write_recovers_to_a_whole_snapshot`, `services/modbit-core/tests/memory_graph_index.rs::px_111_indexed_search_returns_what_the_scan_returns_through_the_core`
- re-run: macOS local 2026-10-20 and CI run 39029466637 on main ac4aa804, all green.

## Limitations

- Windows and Linux legs are proven by CI only; no release gate is attested by this task.

## Evidence

- `evidence.json` and `ci-run-39029466637.json` in this directory
