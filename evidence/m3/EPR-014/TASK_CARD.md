# Task Card — EPR-014 Conditional plan migration and slot admission

## Identity

- Task ID: EPR-014
- Milestone: M3 (moved from M2 by DR-M2-002)
- Requirement: REQ-EPR-014; owner label: core-runtime; subsystem: core-runtime
- Qualification: `QUAL-EPR-014` / `EPR-E2E-014` — migrate real pre-plan SQLite fixtures, send versioned plans through authenticated Core admission, validate every slot before the initial dispatch, persist the slot table and the reservation, and kill/restart during activation recovering one exact activation with the budget unchanged. Negative: `EPR-FI-014` — reject an unmappable legacy template, an unlisted runtime continuation, cyclic slots, a stale epoch and a duplicate activation, with no dispatch and no partial accepted patch.
- Evidence tier: real-system or production-equivalent

## Goal

Supply the one shared plan validator and admission interface EPR-004 and EPR-005 will use: stable slot ids, predecessors, triggers and activation counts; validation and reservation before dispatch; authenticated Core admission; and a restart that recovers an activation rather than repeating it.

## Existing-code audit

- classification: PARTIAL before this task. EPR-001 landed the contracts and the durable state — plans, slots and attempts are persisted and projected inside the append transaction — but nothing validated a plan before dispatch, nothing bounded a slot by its activations (the projection counted the first attempt instead), and no client could submit a plan at all.
- production entry points:
  - `crates/core-runtime/src/admission.rs` — the interface. `admit_plan` validates a plan whole, refuses a legacy template label that would authorize slots the legacy record never described, and returns the digest of what was validated with the money it reserves. `admit_activation` answers with an ordinal and an allowance, or refuses with `REQUIRED_CONTINUATION_UNAVAILABLE`, `ACTIVATIONS_EXHAUSTED`, `ATTEMPTS_EXHAUSTED` or `ALLOWANCE_EXCEEDED`. `check_not_replayed` is what makes a restart recover an activation instead of opening a second one.
  - `crates/event-store/src/schema.rs` — migration V8 adds `routing_admissions` and `routing_activations`.
  - `crates/event-store/src/projections.rs` — `RunEvent::RoutingPlanAdmitted` and `RunEvent::SlotActivated` project inside the append transaction; a slot's activation count is now the number of activation rows, so a replay cannot double it. Plans are ordered by routing epoch, which is what decides which plan is current.
  - `services/modbit-core/src/routing.rs` and `AdmitRoutingPlan` — authenticated admission. It is fenced by the session lease like any other write, refuses a plan that names a different run, and refuses a plan whose epoch is not newer than the admitted one.
  - `services/modbit-core/src/runtime.rs` — every run admits its own direct plan through `admit_plan` and activates its single slot through `admit_activation`, in the same transaction as `RunCreated`. Nothing dispatches from a plan that has not been admitted, including the direct one.
- proof: a real task runs through Core; its direct plan is admitted, its slot activates exactly once, and a hard kill while the second invocation is stalled recovers that one activation with its reservation unchanged. A two-slot conditional plan is then submitted over the surface protocol under the session lease: it is admitted whole, its reservation is the sum of its slots plus the verification reserve, and a client reads it back with both slots and no activation yet. The same plan resubmitted is refused as a stale generation, a cyclic plan is refused for its cycle, a foreign-tenant plan is refused as foreign, and an unfenced caller is refused the command outright — after all of which the admitted plan still stands.

## Limitations

- Nothing activates a continuation yet. `admit_activation` is exercised by the direct path's single slot and by unit tests over a two-slot plan; the code that answers a quality rejection by activating the stronger slot is EPR-005/006.
- Reservations are recorded and bound each activation, but no settlement path reconciles them when an attempt ends, because no attempt has a price yet: cost is unknown on the direct path and stays unknown.
- `AdmitRoutingPlan` carries the plan as the JSON of the versioned domain contract rather than a second protobuf mirror of it, so there is one definition of a plan rather than two that can drift.

## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`):

- `qual_epr_014_a_conditional_plan_is_admitted_whole_and_its_activation_is_exact` — the end-to-end proof through Core, including the kill during activation and every refusal over the wire.
- `a_plan_is_admitted_whole_with_the_digest_of_what_was_validated`
- `admission_refuses_before_anything_dispatches` — cyclic slots, stale epoch, foreign tenant, over-reservation (EPR-FI-014).
- `a_legacy_template_label_authorizes_no_slot` (EPR-FI-014)
- `the_runtime_activates_slots_and_never_adds_one` — unlisted continuation, trigger mismatch, activations exhausted (EPR-FI-014).
- `no_slot_receives_a_new_budget_and_a_restart_recovers_its_activation` — allowance, attempts, duplicate activation (EPR-FI-014).
- `migrates_the_committed_m1_1_fixture_and_derives_projections`, `a_database_from_before_the_routing_tables_upgrades_and_derives_them`, `a_crash_during_the_routing_migration_leaves_a_recoverable_database` — the migration path.

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json copy alongside
