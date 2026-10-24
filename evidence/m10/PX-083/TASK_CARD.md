# Task Card — PX-083 Trigger evaluation and dispatch inside the existing Scheduler: time source, idempotency, concurrency, missed runs and budgets

## Identity

- Task ID: PX-083 (REQ-PX-083; owner core-runtime); qualification QUAL-PX-083
- Evidence tier: release-critical
- Closed to COMPLETE on 2026-10-24 after the tests below ran in CI.

## Existing-code audit

- classification now: PRODUCTION-WORKING
- new in this closing: #130 added the worktree-isolation and gate-skip-budget tests and retained the idempotency mutation check (evidence/m10/PX-083/mutation-check.txt: removing the event-id dedupe fails three tests)

## Verification

- tests: `services/modbit-core/tests/automations_px.rs::px_083_a_schedule_fires_once_per_slot_on_a_controlled_clock_with_provenance`, `services/modbit-core/tests/automations_px.rs::px_083_missed_slots_follow_the_policy_and_are_never_replayed_in_bulk`, `services/modbit-core/tests/automations_px.rs::px_083_a_duplicate_firing_creates_one_task`, `services/modbit-core/tests/automations_px.rs::px_083_concurrency_policy_skips_queues_or_replaces_the_active_run`, `services/modbit-core/tests/automations_px.rs::px_083_the_rate_limit_and_the_daily_budget_stop_admission_with_a_typed_skip_and_attention`, `services/modbit-core/tests/automations_px.rs::px_083_a_gate_that_skips_spends_only_the_gates_own_budget`, `services/modbit-core/tests/automations_px.rs::px_083_a_gate_decides_whether_the_rest_of_the_run_happens`, `services/modbit-core/tests/automations_px.rs::px_083_a_scheduled_runs_task_works_in_an_isolated_worktree_of_its_own`, `services/modbit-core/tests/automations_px.rs::px_083_a_core_killed_in_the_middle_of_a_run_reconciles_to_one_typed_outcome`, `services/modbit-core/tests/automations_px.rs::px_083_a_core_killed_between_the_firing_and_the_dispatch_neither_loses_nor_repeats_the_run`, `services/modbit-core/tests/automations_px.rs::px_083_each_host_has_one_trigger_clock_and_no_second_scheduler_loop`, `services/modbit-core/tests/automations_px.rs::px_082_definitions_validate_version_and_need_the_owners_exact_approval`
- re-run: CI run 40042355021.

## Limitations

- Time-triggered paths are proven on a controlled clock against a real Core with real SIGKILL restarts; the forge-event trigger consumer has no clause-specific test here. The mutation touched only the Core pre-check, not the event-fold's own repeated-key check. The gate-skip test recomputes the gate session id derivation. No gate attested.

## Evidence

- `evidence.json` and `ci-run-40042355021.json` in this directory
