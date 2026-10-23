# Task Card — PX-132 Process intelligence: port and dev-server detection, readiness and service health reported by the Core

## Identity

- Task ID: PX-132 (REQ-PX-132; owner terminal); qualification QUAL-PX-132
- Evidence tier: release-critical
- Closed to COMPLETE on 2026-10-23 after the tests below ran in CI.

## Existing-code audit

- classification now: PRODUCTION-WORKING
- new in this closing: none: the implementation and tests already existed on main; this closes the graph row after the audit

## Verification

- tests: `services/modbit-core/tests/px_132_services.rs::qual_px_132_three_dev_servers_are_reported_ready_and_the_agent_is_told_without_asking`, `services/modbit-core/tests/px_132_services.rs::qual_px_132_a_server_that_returns_500_is_unhealthy_and_one_that_dies_is_gone`, `services/modbit-core/tests/px_132_services.rs::qual_px_132_a_restarted_core_reattaches_and_reobserves_without_repeating_itself`, `services/modbit-core/tests/px_132_services.rs::qual_px_132_a_server_that_died_while_the_core_was_down_is_recorded_gone_on_restart`, `services/modbit-execd/tests/services.rs::qual_px_132_the_broker_reports_the_listeners_of_a_sessions_own_tree_and_nothing_else`
- re-run: CI run 39960289486.

## Limitations

- Unix-only suites; the Windows leg is not run; remote/cloud processes report through the guest RPC and are not exercised here; no gate attested.

## Evidence

- `evidence.json` and `ci-run-39960289486.json` in this directory
