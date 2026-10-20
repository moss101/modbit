# Task Card — PX-110 Code graph: symbol reference and call edges, implementors and non-test impact selection

## Identity

- Task ID: PX-110 (REQ-PX-110; owner context-engine); qualification QUAL-PX-110
- Evidence tier: release-critical (effect-bearing behavior; see the acceptance below)
- Closed from E2E_PROVEN to COMPLETE on 2026-10-20 after the re-run recorded in evidence.json.

## Existing-code audit

- adoption audit: DOCUMENTED-ONLY; no product caller present
- classification now: PRODUCTION-WORKING (landed on main; verified by the tests below through the real Core)

## Verification

- acceptance: Real Core and fixture repositories (ts-webapp, python-service, rust-cli) with a hand-labelled set of callers, implementors and impacted files: for each of at least thirty labelled queries (including an interface change and a renamed function) the returned non-test dependents match the labels with recall and precision at or above the thresholds recorded in the qualification, ambiguous and unresolved edges are labelled as such, impact feeds test selection and the selected tests include the dependent's tests; an edit invalidates the changed file's edges incrementally and the result is correct at the new revision; a result that hits the expansion cap says it is partial.
- tests: `crates/retrieval/tests/code_graph.rs::labelled_queries_meet_the_recall_and_precision_floors`, `crates/retrieval/tests/code_graph.rs::an_interface_change_reaches_its_implementors_and_dropping_implementor_edges_fails_it`, `services/modbit-core/tests/memory_graph_index.rs::px_110_impact_callers_and_implementors_are_served_by_the_core_and_follow_an_edit`
- re-run: macOS local 2026-10-20 and CI run 39029466637 on main ac4aa804, all green.

## Limitations

- Windows and Linux legs are proven by CI only; no release gate is attested by this task.

## Evidence

- `evidence.json` and `ci-run-39029466637.json` in this directory
