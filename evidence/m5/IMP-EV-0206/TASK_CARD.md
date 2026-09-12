# Task Card — IMP-EV-0206 Skill evolution complements model scaling hypothesis

## Identity

- Task ID: IMP-EV-0206
- Milestone: M5 Procedural runtime and skills (P0)
- Requirement: REQ-EV-0206 (disposition EXPERIMENT); owner label: Eval Harness; subsystem: eval-bench
- Qualification: `QUAL-EV-0206` — A/B benchmark uses same tasks/environment and records confidence intervals.
- Evidence tier: production-equivalent (the production lab and harness over real directories and fixture trials; see M5.7 for what the experiment does not establish)

## Goal

Skill evolution complements model scaling hypothesis, as an EXPERIMENT behind the Skill Registry and the Eval Harness (docs/26): isolated under the existing owner, never a second subsystem, promoted only through ADR and measurable benefit.

## Existing-code audit

- classification: NOT-FOUND before M5.7; built with M5.7's lab (`crates/skills/src/evolution.rs`) and harness (`benchmarks/skill-evolution`).
- proof: WSK-E2E-008: paired arms over the same tasks, models and environment with bootstrap intervals (18 pairs; the fixture is deterministic, so the intervals have no width and the report says so); a larger aggregate saving never overrides a failed hard gate. The hypothesis itself is not tested on engineering tasks with live models.

## Finding

EXPERIMENT: the mechanism behaves as its hypothesis requires on the fixture; no lift over simpler refinement is established there (M5.7 ablation), so nothing is promoted and no ADR is proposed; the exit criterion of docs/41 ("promote only through ADR + measurable benefit; otherwise remove cleanly") is not yet decided either way.

## Limitations

- Deterministic fixture trials and the template proposer stand in for live models (DR-M3-002); intervals have no width.

## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`):

- `wsk_e2e_008_the_paired_benchmark_reports_every_arm_with_intervals_and_never_promotes_over_a_failed_gate`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json copy alongside
