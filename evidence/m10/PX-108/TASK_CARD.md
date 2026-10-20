# Task Card — PX-108 Goal-seeded pre-turn context pack

## Identity

- Task ID: PX-108 (REQ-PX-108; owner context-engine); qualification QUAL-PX-108
- Evidence tier: release-critical (effect-bearing behavior; see the acceptance below)
- Closed from E2E_PROVEN to COMPLETE on 2026-10-20 after the re-run recorded in evidence.json.

## Existing-code audit

- adoption audit: DOCUMENTED-ONLY; no product caller present
- classification now: PRODUCTION-WORKING (landed on main; verified by the tests below through the real Core)

## Verification

- acceptance: Real Core, real repository fixtures (ts-webapp, python-service, rust-cli) and the scripted provider: the first request already carries a pack whose entries name the goal-relevant files with revision and reasons, without any model tool call; a write by the task invalidates the touched fragment and the next request drops or re-hydrates it; the pack stays inside the budget across fifty turns; after a Core restart the ledger rebuilds and the pack is consistent; a goal with no matches yields an empty pack with a recorded reason; measured on the existing corpus, tasks started with the pack reach the first correct file read no later than without it.
- tests: `services/modbit-core/tests/context_rules_pack.rs::px_108_the_first_request_carries_a_goal_seeded_pack_without_a_model_tool_call`, `services/modbit-core/tests/context_rules_pack.rs::px_108_a_write_invalidates_the_fragment_and_model_retrieval_still_works`, `services/modbit-core/tests/context_rules_pack.rs::px_108_the_pack_stays_inside_the_budget_across_fifty_turns`
- re-run: macOS local 2026-10-20 and CI run 39029466637 on main ac4aa804, all green.

## Limitations

- Windows and Linux legs are proven by CI only; no release gate is attested by this task.

## Evidence

- `evidence.json` and `ci-run-39029466637.json` in this directory
