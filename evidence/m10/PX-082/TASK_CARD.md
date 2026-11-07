# Task Card — PX-082 Automation definitions: versions, principal, validation, enable approval and repository-supplied definitions

## Identity

- Task ID: PX-082 (REQ-PX-082; owner automation); qualification QUAL-PX-082
- Evidence tier: release-critical
- Closed to COMPLETE on 2026-10-23 after the tests below ran in CI.

## Existing-code audit

- classification now: PRODUCTION-WORKING
- new in this closing: the Core suite now runs on Windows (#121, rust windows-latest green in this run) and the QUAL mutation check was run and retained: evidence/m10/PX-082/mutation-check.txt

## Verification

- tests: `services/modbit-core/tests/automations_px.rs::px_082_definitions_validate_version_and_need_the_owners_exact_approval`, `services/modbit-core/tests/automations_px.rs::px_082_a_client_cannot_create_an_automation_task_and_a_tool_cannot_reach_the_ledger`, `services/modbit-core/tests/automations_px.rs::px_082_a_repository_definition_stays_disabled_until_its_hash_is_approved_and_a_changed_byte_disables_it`, `services/modbit-core/tests/automations_px.rs::px_082_three_versions_enable_the_third_and_a_killed_core_replays_them_all`, `services/modbit-core/tests/automations_px.rs::px_082_a_missing_budget_a_secret_value_and_a_client_without_the_right_fail_closed`, `services/modbit-core/tests/automations_px.rs::px_082_a_model_that_calls_for_the_ledger_changes_nothing`
- re-run: CI run 39993797935.

## Limitations

- Mutation A (the approval's hash binding dropped) fails the suite; mutation B (the run gate's duplicate hash comparison dropped) survives because the version comparison beside it still refuses an edited definition, so that line is defence in depth, not the binding the QUAL names. The reference drift definition is exercised with a scripted model; no live provider. No gate attested.

## Evidence

- `evidence.json` and `ci-run-39993797935.json` in this directory
