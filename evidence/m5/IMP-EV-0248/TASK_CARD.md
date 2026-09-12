# Task Card — IMP-EV-0248 Optimize reward/latency/cost jointly

## Identity

- Task ID: IMP-EV-0248
- Milestone: M5 Procedural runtime and skills (P0)
- Requirement: REQ-EV-0248 (disposition EXPERIMENT); owner label: Eval Harness; subsystem: eval-bench
- Qualification: `QUAL-EV-0248` — Cheap but lower-correctness profile cannot promote.
- Evidence tier: production-equivalent (the production lab and harness over real directories and fixture trials; see M5.7 for what the experiment does not establish)

## Goal

Optimize reward/latency/cost jointly, as an EXPERIMENT behind the Skill Registry and the Eval Harness (docs/26): isolated under the existing owner, never a second subsystem, promoted only through ADR and measurable benefit.

## Existing-code audit

- classification: NOT-FOUND before M5.7; built with M5.7's lab (`crates/skills/src/evolution.rs`) and harness (`benchmarks/skill-evolution`).
- proof: WSK-E2E-004 and 008: a candidate a sixth of the tokens and less correct in one class is REJECTED with "economics not considered: a hard gate failed"; economics are recorded only after the gates pass.

## Finding

EXPERIMENT: the mechanism behaves as its hypothesis requires on the fixture; no lift over simpler refinement is established there (M5.7 ablation), so nothing is promoted and no ADR is proposed; the exit criterion of docs/41 ("promote only through ADR + measurable benefit; otherwise remove cleanly") is not yet decided either way.

## Limitations

- Deterministic fixture trials and the template proposer stand in for live models (DR-M3-002); intervals have no width.

## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`):

- `wsk_e2e_004_promotion_needs_the_gates_and_rollback_restores_the_head`
- `wsk_e2e_008_the_paired_benchmark_reports_every_arm_with_intervals_and_never_promotes_over_a_failed_gate`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json copy alongside
