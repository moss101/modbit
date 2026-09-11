# Task Card — EPR-016 Confidence-adjusted feasibility and cold start

## Identity

- Task ID: EPR-016
- Milestone: M3 (moved from M2 by DR-M2-002)
- Requirement: REQ-EPR-016; owner label: model-gateway; subsystem: model-gateway
- Qualification: `QUAL-EPR-016` / `EPR-E2E-016` — replay representative repository and request evidence through the real compiler quality component with pinned stats and threshold versions; a high mean with a weak bound fails cheap eligibility, the minimum-cost feasible plan wins, an empty feasible set emits exact infeasibility and a hard-empty set fails. Negative: `EPR-FI-016` — mutate mean-only selection, the confidence method, the sample threshold, missing data and switch-cost hysteresis; tests must expose unsafe cheap eligibility and no unbounded fallback.
- Evidence tier: real-system or production-equivalent

## Goal

Decide whether a whole conditional plan can be expected to meet the mode's quality floor on the lower confidence bound of what was observed, choose the cheapest plan that can, and when none can say `QUALITY_FLOOR_INFEASIBLE` exactly — never violating budget or policy and never claiming a target the evidence does not support.

## Existing-code audit

- classification: ABSENT before this task. Admission validated a plan's shape and budget, the registry carried the mode floor and EPR-015 carried the observations, but nothing joined them: no plan was ever measured against the floor, and nothing chose between plans at all.
- production entry points:
  - `crates/providers/src/feasibility.rs` — the component. `plan_quality` composes a plan's lower bound from its initial leg and, only when the joint escalation key has real support, its continuation; a thin or missing continuation contributes nothing and is named. `select` qualifies on the bound with the sample threshold, picks the minimum worst-case complete cost among feasible plans, keeps the plan in force unless a cheaper one saves at least the switch cost, and otherwise answers with the best hard-eligible plan as `QUALITY_FLOOR_INFEASIBLE` with `target_met: false` — never a plan outside the eligible set. Every exclusion carries its reason in words a reader can check.
  - `services/modbit-core/src/routing.rs` — `feasibility_of` measures a plan against the session's latest statistics snapshot (EPR-015) and the active registry's `auto` floor (EPR-002), and admission (EPR-014) records the verdict with the stats and threshold versions it was measured under. With no floor the record says `QUALITY_FLOOR_UNKNOWN`, not feasible.
  - `services/modbit-core/src/runtime.rs` — the direct plan every run admits is measured the same way, so the baseline is never described as meeting a target it was not measured against.
  - Migration V9 adds the feasibility columns to `routing_admissions`; `RoutingAdmissionView` carries them to clients.
- proof: through Core, a run with no registry records `QUALITY_FLOOR_UNKNOWN` with no versions to pin; after a signed registry with a 0.72 floor is activated and the session's single observation is materialized as `stats-1`, the next run's admission records `QUALITY_FLOOR_INFEASIBLE` under `stats-1` and `registry-feas-1` with `target_met: false` and a bound below the floor, and a two-slot conditional plan submitted through admission is measured the same way with its unobserved continuation named rather than assumed. The selection rules — a perfect mean over three observations losing to a well-observed dearer plan, the cheapest feasible plan winning, switch cost stopping a marginal flap, exact infeasibility, and the hard-empty set failing — are proven against synthetic candidates, and each EPR-FI-016 mutation (mean-only selection, a lax sample threshold, a zero switch cost, missing data) is shown to change the answer to the unsafe one.

## Limitations

- The cold-start case is the only case the product is in today: the recorded statistics carry one observation per session, so every plan is infeasible under the floor and the direct baseline runs as the best hard-eligible plan with the target not claimed. That is the correct answer for the evidence there is; feasible selections are exercised only against synthetic evidence in tests.
- `tau` is the registry's `min_quality` for the `auto` mode; `delta` and the switch cost are fixed in Core (0.05 and 0) until a mode policy carries them. Both are recorded with the verdict.
- The joint escalation key is composed with fixed `gate`, `repository` and `verification` components (`acceptance`, `workspace`, `configured`) until EPR-005/006 produce real escalation observations keyed by what actually ran.
- Selection across several candidate plans has no product caller until EPR-004 compiles them; admission measures the one plan it is given.

## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`):

- `qual_epr_016_feasibility_is_measured_at_admission_under_pinned_versions` — the end-to-end proof through Core.
- `a_high_mean_with_a_weak_bound_fails_cheap_eligibility` (EPR-FI-016: mean-only selection and the sample threshold)
- `the_minimum_cost_feasible_plan_wins_and_switch_cost_stops_flapping` (EPR-FI-016: switch cost)
- `an_empty_feasible_set_is_said_exactly_and_never_claims_the_target` (EPR-FI-016: missing data, hard-empty set)
- `a_continuation_adds_only_what_its_own_observations_support`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json copy alongside
