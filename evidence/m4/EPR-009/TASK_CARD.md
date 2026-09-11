# Task Card — EPR-009 Persist routing epochs and switch economics

## Identity

- Task ID: EPR-009
- Milestone: M4 Durable recovery spine (P0); phase 2-4; prerequisites EPR-005, M4.6
- Requirement: REQ-EPR-009 (related REQ-EV-0012, REQ-EV-0256); owner: core-runtime; subsystems: `crates/providers` (economics, compiler incumbent), `services/modbit-core` (boundaries, session state)
- Qualification: `QUAL-EPR-009` / EPR-E2E-009 — run live multi-turn provider sessions with cache metadata plus actual compaction/restart; compare stay/switch total economics and quality drift; record route and compaction epochs and switch reason. EPR-FI-009 — late old-model response after epoch change, cache expiry and process kill cannot overwrite newer state or reset spent/reserved budget; model identity change preserves Run/Agent identity.
- Evidence tier: real-system for the offline half (the real Core, a signed registry with cache prices, a provider that reports a warm prefix, a real compaction epoch, a hard kill and restart); production-equivalent for the switch itself (the production compiler with pinned confidence-feasible evidence). The live-provider half stays deferred by DR-M3-002.

## Goal

Persist active conditional plan/cache/profile/epoch; re-evaluate at task/compaction/demand/provider/quality/mode boundaries and switch only to confidence-feasible alternatives that remain better after lost cache, refill, cache-write, switch latency and hysteresis; inside a transaction activate only admitted slots; a different topology needs a reconciled new transaction with remaining budget.

## Existing-code audit

- classification: PARTIAL before this task. The compiler carried a `switch_cost_minor` threshold that the Core set to zero and a hysteresis rule that no caller reached (`select` was called with no incumbent); the registry priced input and output only; nothing re-evaluated a route after a run started; a resumed run took the last-inserted plan of its run; there was no session routing state and no record of why a route stayed or moved. First missing link: a constant zero where the switch cost should have been.
- production entry points:
  - `crates/providers/src/economics.rs` — `CacheState` (the provider's last `cached_tokens` with the binding and its cache key), `RemainingDemand`, `SwitchCost { lost_cache_value, re_prefill, cache_write, latency_penalty, hysteresis, total, prefix_warm }`, `compare` → `StaySwitch` on one accounting basis (the re-prefill counted once inside the alternative's price; the lost cache value reported, never added; latency and hysteresis explicit inputs); `registry::Economics` gains optional `cached_input_per_mtok_minor`, `cache_write_per_mtok_minor`, `cache_ttl_ms` (absent = no discount, free write, provider default; existing signed documents unchanged).
  - `crates/providers/src/compiler.rs` `CompileInput.current_binding` — the plan opening with the binding in force is the incumbent `select` keeps unless a cheaper feasible plan saves at least the switch cost (a fresh compile digests exactly as before: golden plan ids unchanged); `feasibility::select` keeps the incumbent when nothing is confidence-feasible.
  - `services/modbit-core/src/routing.rs` — `RouteContext`, `switch_economics` (the lowest switch cost over the alternatives becomes the threshold; the demand is the compiler's own basis: one leg at the expected prompt size and output ceiling), `session_route_context` and `session_state` (docs/27 §16.1 `RoutingSessionState` read off the log: active binding and plan from the latest activated slot, the warm prefix from the latest `ModelUsageRecorded`, executed path labels per run, route epoch, decisions), `reevaluated_event`, `decision_reason`; `GetRoutingSessionState`; the routing view shows the plan of the highest epoch.
  - `services/modbit-core/src/runtime.rs` — the TASK boundary in `route_new_run` (the session's route and warm prefix as the incumbent; `RouteReevaluated` INITIAL | STAY | SWITCH), the COMPACTION boundary (`reroute_at_boundary` after a new epoch installs: cold economics, and a SWITCH compiles, admits and activates a new transaction under the next routing epoch on the same run and moves the loop's binding), `recover_route` by highest epoch; `RunEvent::RouteReevaluated`; CLI `session route`.
- proof: (offline E2E) a signed registry with two solvers — `gpt-5` with a cached-input discount, `gpt-5-mini` cheaper uncached — and a provider reporting three quarters of every prompt as cached; run 1 pinned to `gpt-5` records `TASK / INITIAL` (manual pin) and leaves the session with the active binding, its compiled plan and a warm prefix with its cache key; run 2 in auto records `TASK / STAY` with the warm prefix consulted (`prefix_warm: true`, a positive re-prefill, both totals) because nothing is confidence-feasible at cold start, then crosses a real compaction epoch and records `COMPACTION / STAY` on the same run and epoch with the prefix gone (`prefix_warm: false`, re-prefill 0), the cheaper model cheaper cold, and the reason naming the missing feasibility; the executed path labels are one binding per run; after a hard kill the decisions, epoch, plan, attempts, slots, admission and cache state read back identical; a plan carrying a superseded epoch is refused admission. (Compiler) with both solvers confidence-feasible, a warm prefix keeps the dearer incumbent (the saving is below the switch cost, said in the exclusions) and a cold prefix switches to the cheaper one; at cold start the incumbent stays and the alternative is excluded for want of evidence; fresh compiles keep their ids. (Economics) a nominally cheaper model loses after a cache miss; hysteresis and latency are explicit and can flip a marginal switch; nothing is counted twice.

## Limitations

- The live half of EPR-E2E-009 (a real provider's cache metadata and quality drift over multi-turn sessions) is deferred by DR-M3-002 with the rest of the production-endpoint proofs; the offline half runs the same code against a provider that reports cache metadata in the provider's own shape.
- An end-to-end SWITCH needs a confidence-feasible alternative, which needs thirty attributable outcomes per binding (docs/27 §7.3); the switch is proven on the production compiler with pinned evidence, and every boundary the Core crosses records the same economics with STAY for want of feasibility.
- Provider-degradation, quality-rejection and mode boundaries wait for their producers (EPR-006/007); demand drift has no profiler signal yet (EPR-003 is in shadow).
- A late response of the old model after a switch cannot occur inside the loop (boundaries fall between turns and the model call is synchronous); the epoch fence is at admission.

## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`):

- `qual_epr_009_routes_reevaluate_at_boundaries_on_cache_economics_and_survive_restart` (services/modbit-core, real Core)
- `qual_epr_009_a_route_switches_only_to_a_feasible_alternative_that_beats_the_switch_cost` (crates/providers, compiler)
- Unit (crates/providers economics): `a_nominally_cheaper_model_loses_after_a_cache_miss`, `hysteresis_and_latency_are_explicit_and_can_flip_a_marginal_switch`, `nothing_is_counted_twice`
- Regression: `qual_epr_005_new_runs_go_through_the_compiled_initial_leg_and_keep_the_baseline` (plan/admission/activation/attempts across a kill), the routing golden corpus (`identical_inputs_produce_identical_plans_and_the_golden_cold_start` and siblings: plan ids unchanged)

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json copy alongside
