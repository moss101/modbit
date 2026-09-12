# Task Card — EPR-006 Activate prevalidated escalation continuations

## Identity

- Task ID: EPR-006
- Milestone: M5 Procedural runtime and skills (P0); phase 3; prerequisites EPR-017, EPR-009, M3.9
- Requirement: REQ-EPR-006 (related REQ-EV-0014, REQ-EV-0029); owner: core-runtime; subsystems: `services/modbit-core` (quality boundary, run recovery, routing view), `crates/core-runtime` (harness leg), `crates/domain` (run event)
- Qualification: `QUAL-EPR-006` / EPR-E2E-006 — live floor/provider escalation on real repository defects and accepted edits through Core; holdout measures gate false accept/reject, quality floor, total expected cost including the failed floor and acceptable latency on a named slice. EPR-FI-006 — kill between floor result/gate/escalation; remove the mandatory gate, exhaust the budget or race user edits at apply; no floor auto-accept, duplicate effect or partial accepted patch.
- Evidence tier: real-system for the offline half (the real Core on a real repository with a real configured check, a signed registry, a conditional plan admitted through the production admission interface, a hard kill and restart, the user's accept through the Review surface); the live-provider half and the holdout measurement stay deferred by DR-M3-002 (no provider credentials in this repository).
- Decision Record: `docs/decisions/DR-M3-002-epr-baseline-live-provider-proof.md` (the live-provider half)

## Goal

Execute the economical initial candidate, collect static evidence, derive the factual assurance and the independent acceptance; on quality rejection activate only the plan's prevalidated stronger-solver slot, preserving the original request, the failed leg's evidence and the remaining budget; the exact-revision verified atomic apply yields a derived CASCADE path label.

## Existing-code audit

- classification: PARTIAL before this task. The compiler enumerated stronger-solver continuations (`Trigger::QualityRejected`) and admission could activate one exactly once (EPR-014), the gate rejected candidates at every COMPLETION run (EPR-017), and boundaries re-evaluated routes on economics (EPR-009) — but nothing consumed a rejection: a rejected leg that stopped went to attention, every compiled run was `DIRECT` in fact, a resumed run never activated anything, and the quality boundary docs/27 §9.2 names had no producer.
- production entry points:
  - `services/modbit-core/src/escalation.rs` — `at_quality_boundary`: the plan in force (highest routing epoch on the run), its `QUALITY_REJECTED` slot whose predecessor is the binding that failed, the Acceptance Gate at the exact candidate revision (a COMPLETION run made there when the leg's is not current), admission against a ledger priced from every recorded attempt at the registry's rates (unknown usage charged at the slot's worst case; legs counted as activations; the verification reserve held back), the events (`SlotActivated`, `RouteReevaluated` QUALITY SWITCH, `ContinuationActivated`), the switch of the run's binding, the note, and the harness's fresh leg. Every refusal is a `RouteReevaluated` QUALITY STAY with its reason.
  - `services/modbit-core/src/runtime.rs` — the boundary is consulted where the initial leg is over without acceptance: the repair loop's escalation, the no-progress ceiling, and the head of a resumed run whose last quality decision was a STAY (`HarnessState::quality_rejection`); `recover_route` resumes on the run's latest activation whichever transaction it is on; `rebuild` restores the gate verdict, the boundary's decision and the continuation's note from the log; `state.acceptance` carries the revision the gate judged.
  - `crates/core-runtime/src/harness.rs` — `HarnessState::begin_leg` (a continuation's repair loop and no-progress count start fresh; the failed leg's attempts stay on the log) and `quality_rejection`.
  - `crates/domain/src/run.rs` — `RunEvent::ContinuationActivated` (record-only).
  - `services/modbit-core/src/routing.rs` — the path label is derived from the `(role, trigger, binding)` of the slots that ran across the run's transactions: `DIRECT`, `CASCADE` for a solver continued by a stronger solver on quality rejection, a leg continued across a reconciled transaction on the same binding counted once.
- proof: through Core on a repository whose configured check passes at baseline, the cheaper solver's edit regresses it, the gate rejects at the COMPLETION run, the solver stops, and the boundary records a STAY (no prevalidated slot on the cold-start direct plan) with the run in attention and the rejected candidate in the worktree; a conditional plan admitted for that run whose transaction allows one leg is refused at the boundary (`ATTEMPTS_EXHAUSTED`), the run stops safely again and `DecideReview ACCEPT` is refused; a funded plan's stronger slot is activated at resume — the gate re-evaluated from a fresh COMPLETION run, the spend of the failed leg counted, the remaining budget exactly the total less the spend and the verification reserve — with the note on the log; the Core is killed after the activation and the restarted Core resumes on the stronger slot with exactly one activation, the continuation's first request carrying the original request, the failed leg's four tool results and its refused completion, then the note; the continuation's candidate is accepted by the gate and by the user, the file holds the continuation's edit, the attempts after the activation are the stronger slot's, `GetRoutingPlan` labels the run `CASCADE` and the session's executed path is `openai/gpt-5-mini -> openai/gpt-5`.

## Limitations

- The live-provider half of `EPR-E2E-006` and the holdout measurement (gate false accept/reject, quality floor, total expected cost, latency on a named slice) have not run: no provider credentials in this repository (DR-M3-002).
- At cold start the compiler opens direct — no continuation is confidence-feasible without statistics, and escalation statistics only exist once escalations have run (EPR-011 replays them offline) — so today a cascade is a plan an operator or the lab admits for a run through `AdmitRoutingPlan`; the boundary treats a compiled plan and an admitted one the same way.
- One continuation per rejection: the stronger slot's own rejection stops the run (its `max_activations` is one and no slot continues it); reviewer/reviser continuations are EPR-007.
- The transaction's attempt ceiling counts legs (activations); the per-invocation `RoutingAttemptRecorded` rows remain the accounting of what each leg cost.

## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`):

- `qual_epr_006_a_quality_rejection_continues_the_run_on_the_prevalidated_stronger_slot` — the end-to-end proof through Core: the rejected initial leg, the STAY without a slot, the refused continuation, the activated one, the kill and restart, the accept and the `CASCADE` label.
- `no_slot_receives_a_new_budget_and_a_restart_recovers_its_activation` (`crates/core-runtime/tests/admission.rs`) — the money allowance and the attempt ceiling at the same door.
- `qual_epr_017_acceptance_is_evidence_at_the_revision_and_never_erases_a_human_obligation` — the gate the boundary consumes.
- `qual_epr_009_routes_reevaluate_at_boundaries_on_cache_economics_and_survive_restart`, `qual_epr_005_new_runs_go_through_the_compiled_initial_leg_and_keep_the_baseline`, `qual_epr_014_a_conditional_plan_is_admitted_whole_and_its_activation_is_exact` — the boundaries, the compiled initial leg and the exact activation the continuation joins.

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json copy alongside
