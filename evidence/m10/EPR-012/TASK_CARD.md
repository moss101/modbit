# Task Card — EPR-012 Jointly evaluate conditional parameters and promote policy

## Identity

- Task ID: EPR-012 (REQ-EPR-012; related REQ-EV-0029, REQ-EV-0030; owner eval-bench)
- Milestone: M10, phase 5; prerequisites EPR-011, EPR-009, EPR-019, M10.1
- Qualification: QUAL-EPR-012 / EPR-E2E-012 / EPR-FI-012 (docs/61)
- Evidence tier: release-critical (policy activation, canonical registry state, evidence semantics)
- No second compiler, registry publisher or store: the Policy Lab evaluates configurations through the router's own compiler; promotion extends the one registry publisher (`model_registry::activate`) and the Gateway's registry slot with a canary beside it; evidence is content-addressed objects and the log (docs/81: eval-bench owns the offline Policy Lab).

## Existing-code audit

- classification: DOCUMENTED-ONLY. A signed registry activated unconditionally and replaced whatever was active; no previous-good, no rollback, no history (a restart lost the generation); nothing searched a policy offline, nothing held out a partition, nothing checked calibration, replay or propensity evidence before activation, and no canary stage existed. The EPR-011 replay, the EPR-015 statistics, the EPR-016 feasibility and the EPR-019 calibration existed as inputs.
- first missing link: activation was not a compare-and-swap and kept no previous-good generation to return to.
- production entry points: `benchmarks/policy-lab` (`search`, `stage_a`, `verify`); `services/modbit-core/src/promotion.rs` (`search` for `SearchPolicy`, `evaluate` through `modbit_providers::compiler::compile`, `gate`, `search_holds`, `replay_holds`, `canary_holds`, `record_evidence`); `model_registry.rs` (`activate`, `start_canary`, `promote_canary`, `rollback`, `restore`, `for_task`, `priced_by`, history); `crates/providers/src/gateway.rs` (`install_registry` CAS, `install_canary`, `promote_canary`, `clear_canary`, dispatch check); `crates/providers/src/registry.rs` (`Promotion`, `keep_revoked`, `auto_floor`); `routing.rs` (`for_task` at compile and admission; `evidence_of`, `thresholds_for`, `needs` shared with the lab); `runtime.rs`, `replay.rs`, `accounting.rs` (canary-aware routing, staleness and pricing); `server.rs` (`SearchPolicy`, `PromoteCanary`, `RollbackModelRegistry`, `RecordPromotionEvidence`; restore at boot).

## Verification

- `qual_epr_012_a_policy_is_searched_canaried_and_promoted_only_on_its_evidence_and_rolls_back_without_resurrecting_revocations` (real Core, host sandbox, signed registry, scripted OpenAI-compatible provider): 30 verified tasks in a train session and 30 in a separate holdout session (plus 2 in a thin one) are published as baselines and materialized as `train-1`, `holdout-1`, `holdout-thin`. A routed request is replayed isolated on its hard-eligible alternative (EPR-011) with its propensity on the log. The EPR-019 suite calibrates the gate over the holdout corpora. `SearchPolicy` over floors 0.72/0.8/0.95 at target 0.8 is `FEASIBLE` at 0.8 (0.95 `QUALITY_FLOOR_INFEASIBLE`, the tie broken to the safer floor), a search against the thin holdout is `HOLDOUT_REGRESSION`, the train session as its own holdout is `SEARCH_SPEC_INVALID`, and a submitted search report is refused.
- EPR-FI-012, each refused and leaving production and canary unchanged: a stale compare-and-swap and a promotion naming another previous-good (`REGISTRY_ACTIVATION_CONFLICT`); a lowered floor a search chose and a choice that regressed on the holdout (`POLICY_QUALITY_REGRESSION`); a floor the search did not choose, a canary of 0, no replay, an unknown replay, a candidate repriced after its search, an unapproved threshold profile and one the measured gate does not meet (`POLICY_EVIDENCE_MISSING`); exploration of critical requests and 5000 bp of exploration (`UNSAFE_EXPLORATION`); statistics other than the ones searched (`REGISTRY_INCOMPATIBLE`); a second searched and gated canary while one runs (`REGISTRY_ACTIVATION_CONFLICT`).
- EPR-E2E-012: the candidate starts as the canary (`CANARY`, previous-good `policy-a`), is ended (`CANARY_ENDED`, production untouched) and started again, and survives a Core restart. In one session a task in a plain repository compiles under `policy-a` and a task in a repository whose policy sets `routing.canary` ALLOW compiles under `policy-b` and reaches review. `PromoteCanary` is refused with no request, with the production-routed request and against a stale production generation, then `PROMOTED` on the canary request; a second promotion against `policy-a` (either command) conflicts. The first request's plan still pins `policy-a`. A plain activation naming a stale generation conflicts; `policy-c` revoking `gpt-5` activates; a rollback naming the replaced generation conflicts; the rollback naming `policy-c` returns `policy-b` with `gpt-5` still revoked; a restarted Core restores `policy-b` and the revocation.
- Mutations (each fails the qualification, sources restored): no install compare-and-swap; no previous-good check in the gate; revocations not kept; the floor-regression check off; replay evidence ignored; the search not reproduced; canary routing for every task; a canary request not required to be routed under the canary.
- Unit: `benchmarks/policy-lab/tests/search.rs` (Stage A then B then the holdout; holdout regression; thin evidence; untouched holdout and bounded spec; tampered report).

## Limitations

- The searched parameter is the auto quality floor; effort, Skills, evidence, acceptance, reviewer and continuation are fixed by the candidate document until the registry carries them as parameters.
- No production exploration is performed: a promotion's declared exploration is bounded by the gate, but routing never randomizes.
- Rollback trigger thresholds (docs/27 §13.5) are not automatic: an operator rolls back on what monitoring (docs/34, M10.1) shows.
- No threshold profile is owner-approved (docs/61): the qualification's profiles are test fixtures that say so, and gates B–G stay unattested.
- A project may opt its repository into canary routing; an administrator's DENY for `routing.canary` wins.
- Offline search runs in the Core on request; it is not scheduled.

## Evidence

- `evidence.json` in this directory
- docs/38 "PromoteRoutingPolicy" as built, docs/30
