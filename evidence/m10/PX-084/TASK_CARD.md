# Task Card — PX-084 Unattended run policy: principal ceiling, parked approvals with expiry, injection handling and kill switches

## Identity

- Task ID: PX-084 (REQ-PX-084; owner effects-security); qualification QUAL-PX-084 / PX-E2E-084; Decision Record DR-PX-2026-10-03-010
- Evidence tier: release-critical (permissions and policy for runs with no person present)
- Prerequisites: M2.5, M6.7, PX-040, PX-082, DOC-PX-010, all COMPLETE

## Existing-code audit (2026-10-27)

- Graph said NOT_STARTED with "DOCUMENTED-ONLY" at adoption; the code was built with PX-082/083 (graph drift, as recorded for PX-045..128).
- Production path: `RunAutomation` / trigger tick -> `services/modbit-core/src/automation.rs` (`ceiling_of`, `narrow_lease`: the profile's default lease intersected with the owner-approved operations, path resources and effect ceiling) -> the canonical `CreateTask` / `StartTask` under the narrowed `CapabilityLeaseGranted` -> Capability Kernel (`crates/policy`) refuses anything outside it; parked approvals expire in the tick (`expire_approval`), pause / kill / emergency stop in the same module; the trigger payload is wrapped as labelled untrusted data and scanned by `modbit_browser::injection::scan`; `crates/automation/src/definition.rs` refuses `computer.*` and `secret.use` capabilities and secret-shaped text.
- Classification: PRODUCTION-WORKING. First missing link: none in code; the graph row and its evidence bundle were missing. No product code changed in this closing.

## Acceptance -> proof (services/modbit-core/tests/automations_px.rs, real Core binary, real SQLite store, real Kernel, real Git worktree, scripted OpenAI-compatible model)

- protected write parks with the exact intent and a later approval by the person continues it exactly once: `px_084_a_parked_approval_the_person_grants_later_lets_the_run_continue_exactly_once`
- an unanswered request cancels the run at expiry with a receipt, never auto-approved: `px_084_a_parked_approval_expires_into_a_typed_cancellation_and_is_never_auto_approved`
- a hostile PR/trigger body changes neither tools, approvals nor the definition: `px_084_a_hostile_trigger_payload_changes_neither_tools_nor_approvals_nor_the_definition`
- no computer capability, no Always rule, no person's mode, no approving tool, no secret in the lease: `px_084_a_persons_rules_and_modes_computer_tools_and_approving_tools_do_not_reach_a_run`
- paused definition fires nothing; global pause, kill and emergency stop cancel runs: `px_084_kill_switches_stop_queued_and_running_runs_and_hold_new_firings`
- five consecutive failures disable the definition with an attention item: `px_084_budgets_stop_a_runaway_and_five_failures_disable_the_definition`
- a read-only / narrowed definition cannot write outside its ceiling even when the model insists; no run mode widens it: `px_084_an_unattended_run_is_held_to_its_ceiling_by_the_kernel`, `px_084_the_reference_definition_reports_drift_and_can_never_change_the_tree`
- an unpriced cost limit stops instead of spending unmeasured: `px_084_without_a_price_a_cost_limit_stops_the_run_instead_of_spending_unmeasured`
- a secret value in a definition is refused (credentials only as broker handles): `px_082_a_missing_budget_a_secret_value_and_a_client_without_the_right_fail_closed`

## Runs

- Local macOS arm64, 2026-10-27: `cargo test -p modbit-core --test automations_px` 27 passed, 0 failed.
- CI run 40239702943 (PR #147 head 40ef7536, landed on main as 84eecda2): rust jobs green on macOS, Linux and Windows, 9 of 9 `px_084_*` tests ok on each.
- Mutation check (QUAL: "removing the ceiling intersection lets a write through"): `mutation-check.txt` — the resource intersection, the operation intersection and the effect-ceiling minimum of `narrow_lease` each removed; all three KILLED.

## Limitations (not proven)

- The model is the repository's scripted OpenAI-compatible stand-in, not a live gateway (same as PX-082/083).
- The later approval is given over the Surface Protocol from the person's client (`ResolveApproval`, the action the attention item names), not by clicking the desktop card; the desktop automation surface is PX-086.
- No gate attested.
