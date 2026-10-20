# Task Card — PX-053 SetExecutionPreference command wired to the existing routing owner

## Identity

- Task ID: PX-053 (REQ-PX-053; owner model-gateway); qualification QUAL-PX-053
- Evidence tier: release-critical (effect-bearing behavior; see the acceptance below)
- Closed from E2E_PROVEN to COMPLETE on 2026-10-20 after the re-run recorded in evidence.json.

## Existing-code audit

- adoption audit: DOCUMENTED-ONLY; no product caller present
- classification now: PRODUCTION-WORKING (landed on main; verified by the tests below through the real Core)

## Verification

- acceptance: Real Core with a signed model registry bundle and an organisation policy: a task is created in Balance, the preference changes to Intelligence between turns and the next compile uses the new profile and records why in the routing decision; an allowed manual pin is honoured and a pin forbidden by policy is refused with the typed reason and no state change; the preference survives a Core restart; the CLI and the desktop client set it identically.
- tests: `services/modbit-core/tests/task_modes.rs::px_053_a_preference_is_recorded_survives_a_kill_reaches_the_wire_and_routing_says_direct`, `services/modbit-core/tests/task_modes.rs::px_053_invalid_preferences_and_forbidden_pins_are_refused_with_typed_errors_and_change_nothing`, `services/modbit-core/tests/task_modes.rs::px_053_with_a_signed_registry_the_router_reads_the_objective_and_says_which_floor`
- re-run: macOS local 2026-10-20 and CI run 39029466637 on main ac4aa804, all green.

## Limitations

- Windows and Linux legs are proven by CI only; no release gate is attested by this task.

## Evidence

- `evidence.json` and `ci-run-39029466637.json` in this directory
