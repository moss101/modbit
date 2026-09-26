# Task Card — EPR-013 Qualify model × Skill outcome statistics

## Identity

- Task ID: EPR-013 (REQ-EPR-013; owner skills)
- Milestone: M10, phase 6; prerequisites EPR-012, M5.7
- Qualification: QUAL-EPR-013 / EPR-E2E-013 / EPR-FI-013 (docs/61)
- Evidence tier: release-critical (statistics and evidence semantics; a Skill's reach under a reviewer's ceiling)
- No second store, compiler or policy service: the key lives with Outcome Statistics (eval-bench), the selection with the skills owner, the compile with the router; nothing a Skill carries grants a capability.

## Existing-code audit

- classification: SCAFFOLDED. `StatKey::Solver { model, skill, harness }` existed but `samples_from_baseline` and the compiler both hard-coded `skill = "none"`; plan provenance pinned no skill or content version; the harness identity ignored the context configuration; a signed skill was enabled without any evaluation and nothing distinguished qualified from unqualified outcomes; there was no revocation; a repository's own skill could be "evaluated" by an `EVALUATION.json` any agent could write beside it; a reviewer's trigger-selected skill asking for more than the reviewer's ceiling was trimmed, not refused.
- first missing link: the Skill never reached the statistics key or the plan.
- production entry points: `benchmarks/outcome-statistics/src/lib.rs` (`skill_set`, `model_config`, `samples_from_baseline`); `crates/observability/src/baseline.rs` (`SkillUse`, `TaskOutcome.skills/effort`, `BaselineBundle.harness_version`, `with_harness`); `crates/domain` (`SkillSelected.qualified`, `Provenance.skill_set`); `crates/providers/src/compiler.rs` (`CompileInput.skill_set`, the solver key, provenance); `services/modbit-core/src/skills.rs` (`choose`, `qualified`, revocations, the reviewer ceiling, `preview_set`); `baseline.rs` (`harness_version`, skills and effort per outcome); `routing.rs` (`compile_for_run` keyed on the run's skills; `feasibility_of` on the pinned set); `promotion.rs` and `benchmarks/policy-lab` (the request shape's skill set); `apps/cli` (`skill revoke`).

## Verification

- `qual_epr_013_statistics_are_keyed_on_qualified_skill_combinations_and_plans_pin_the_skill_set` (real Core, signed registry, signed skills, a scripted provider that answers with the working script only when the skill's instructions are in the request): 30 train runs with the operator's qualified `fixer` skill verify and 3 without it fail; a run with a signed but unevaluated skill is selected `qualified: false` and left out — the train snapshot has 33 samples, `fixer@<hash>` 30/30, `none` 0/3 and low-confidence; 30 holdout runs with the skill. A routed request with the skill compiles `FEASIBLE` and its plan pins `fixer@<hash>`; without it `QUALITY_FLOOR_INFEASIBLE` with `none`. The Policy Lab searching the combination is `FEASIBLE` (held out), the bare model `NO_FEASIBLE_CONFIGURATION`.
- EPR-FI-013: a repository's own signed skill with a PROMOTE evaluation beside it is selected `qualified: false`; the skill changed (drift) — the plan pins the new set and is `QUALITY_FLOOR_INFEASIBLE`; revoked — `SKILL_REVOKED`, no selection, the task's granted lease operations equal a control task's; in an isolated replay a skill asking for `git.commit`/`git.push` is `SKILL_EXCEEDS_REVIEWER_CEILING` while one asking for `fs.read`/`fs.write` is selected.
- Mutations (each fails the qualification; sources restored): the compiler ignoring the skill set; unqualified outcomes pooled; workspace skills qualifying; revocation ignored; the reviewer ceiling off; the plan not pinning the set; the set without content hashes.

## Limitations

- Effort is in the key, but no run chooses an effort yet (every invocation asks for the provider's default), so every key's model configuration is the bare model.
- The context tier enters through the harness version (the compaction budget); per-request context tiers are not a routing parameter.
- The escalation key does not yet carry the skill set (no continuation statistics are materialized in the product).
- Qualification reads the operator's evaluation record; producing it on a real benchmark belongs to the skill-evolution lab (M5.7), and no threshold profile is owner-approved (docs/61).

## Evidence

- `evidence.json` in this directory (after CI)
- docs/38 "MaterializeOutcomeStatistics" as built
