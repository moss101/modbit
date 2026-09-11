# Task Card — EPR-001 Version routing contracts and durable Run state

## Identity

- Task ID: EPR-001
- Milestone: M3 (moved from M2 by DR-M2-002)
- Requirement: REQ-EPR-001; owner label: domain-events; subsystem: domain-events
- Qualification: `QUAL-EPR-001` / `EPR-E2E-001` — round-trip the generated Rust and TypeScript schemas, migrate an actual pre-plan SQLite fixture, kill Core between the plan append and the projection update and recover identical state, and verify the redacted API projection. Negative: `EPR-FI-001` — reject missing versions, invalid money and probabilities, foreign-tenant references and stale generations; a crash during migration must leave a recoverable database.
- Evidence tier: real-system or production-equivalent

## Goal

Version the routing contracts and persist the routing state of a Run — plan, slots, attempts, epoch and budgets — in the stores that already exist, preserving explicit provenance when a legacy plan shape is decoded.

## Existing-code audit

- classification: ABSENT before this task. No routing contract existed in any crate, and nothing recorded how a run was routed: the direct path dispatched one model per turn with no plan, slot, attempt, epoch or budget written anywhere. `RouteRecord` in the gateway described one HTTP attempt in flight and was serialised into a usage event, but it is not durable routing state and names no plan.
- production entry points:
  - `crates/domain/src/routing.rs` — the contracts. `ROUTING_SCHEMA_VERSION = 2`, `Money` (integer minor units, currency, scale, checked arithmetic), `Probability` (finite, in `[0, 1]`, constructed only through `new`), `Provenance` with the seven versions a plan was compiled from, `Budget`, `Trigger`, `Slot`, `ConditionalExecutionPlan` sealed by a digest over its own content, `PlanScope`, and `decode_legacy` with `LegacyDecode` provenance.
  - `ConditionalExecutionPlan::direct` — the direct path written down as the degenerate plan it already is: one initial solver slot, activated once, every model invocation of the run an attempt inside it. It changes no dispatch.
  - `crates/event-store/src/schema.rs` — migration V7 adds `routing_plans`, `routing_slots` and `routing_attempts`.
  - `crates/event-store/src/projections.rs` — `RunEvent::RoutingPlanCompiled` and `RunEvent::RoutingAttemptRecorded` project inside the append transaction; `load_routing_plans` / `load_routing_attempts` read them back; `rebuild` clears the three tables so a rebuild derives them from the log alone.
  - `services/modbit-core/src/runtime.rs` — the run's plan is appended in the same transaction as `RunCreated` / `RunStarted`, and every model invocation records an attempt with its outcome (`SUCCEEDED`, `FAILED`, `CANCELLED`, `INTERRUPTED`), the usage the provider reported or the fact that it reported none, and the provider's own request id.
  - `crates/providers/src/gateway.rs` — `RouteRecord::provider_request_id`, read from the response headers a provider actually returns.
  - `services/modbit-core/src/routing.rs` and `GetRoutingPlan` → `RoutingPlanView` — the redacted projection every client reads.
- proof: a real coding task runs through Core against a wire-faithful provider on a real Git repository, and its routing state is read back over the surface protocol: schema 2, epoch 0, a content digest that seals the plan, one solver slot activated once, and one attempt per model invocation carrying the provider's request id and the usage it reported. The Core is then killed hard and restarted twice — once after the run finished, once with a second run stalled mid-invocation — and the view is identical across both restarts, because the plan and its attempts are written in the append transaction and can never be half-written. A database that predates the routing tables is migrated and its routing state derived from the events it already held, and a process killed while the routing migration is in flight leaves a database the next open migrates cleanly.

## Limitations

- The only plan a run compiles today is the direct one. Conditional plans with real branches are validated, persisted, projected and read back (the store tests use a two-slot plan with a `QUALITY_REJECTED` continuation), but nothing compiles one yet: that is EPR-004, and activating a slot is EPR-005/006.
- `max_activations` is carried and projected, and the projection counts activations, but no code enforces the ceiling because no code activates a second slot yet.
- Money is carried in the contract and is zero on the direct path. Zero means unbudgeted, not free: the direct path has no cost model, and EPR-000's rule that unknown cost stays unknown is preserved end to end.
- `RealizedRisk`, `AcceptanceGateResult`, `ReviewerResult`, `OutcomeRecord` and `OutcomeStatistics` are named in the acceptance text as *refs* to version; they have no producer until EPR-003/EPR-006/EPR-007, so this task versions the contracts that exist and records provenance fields (`risk_version`, `gate_version`, `statistics_version`) for them rather than inventing records.

## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`):

- `qual_epr_001_the_routing_state_of_a_run_is_durable_versioned_and_redacted` — the end-to-end proof through Core, including both hard kills and the redaction check.
- `a_plan_its_slots_and_its_attempts_are_durable_and_rebuildable` — durable across reopen, and rebuilt from the log after the projection is wiped out of band.
- `a_database_from_before_the_routing_tables_upgrades_and_derives_them` — the pre-plan database migrates and derives its routing state.
- `a_crash_during_the_routing_migration_leaves_a_recoverable_database` — a killed migration commits nothing and the next open recovers (EPR-FI-001).
- `a_valid_plan_is_sealed_by_its_own_content`, `a_plan_that_cannot_be_executed_is_refused_before_anything_dispatches`, `money_is_integer_and_probability_is_a_probability`, `a_legacy_plan_decodes_with_its_provenance_and_authorizes_nothing`, `the_direct_baseline_is_a_valid_plan_that_claims_no_routing` — the contract and its refusals (EPR-FI-001).
- `round trip routing_plan_view` (TypeScript) and `typescript_encoded_fixtures_decode_and_reencode_identically` / `committed_rust_fixtures_are_fresh` (Rust) — the cross-language round trip of the generated schemas.

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json copy alongside
