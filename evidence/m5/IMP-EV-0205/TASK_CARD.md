# Task Card — IMP-EV-0205 Cross-model skill transfer evaluation

## Identity

- Task ID: IMP-EV-0205
- Milestone: M5 Procedural runtime and skills (P0)
- Requirement: REQ-EV-0205 (disposition EXPERIMENT); owner label: Eval Harness; subsystem: eval-bench
- Qualification: `QUAL-EV-0205` — Nightly matrix reports baseline vs skill deltas per model and rejects hidden regression.
- Evidence tier: production-equivalent (the production lab and harness over real directories and fixture trials; see M5.7 for what the experiment does not establish)

## Goal

Cross-model skill transfer evaluation, as an EXPERIMENT behind the Skill Registry and the Eval Harness (docs/26): isolated under the existing owner, never a second subsystem, promoted only through ADR and measurable benefit.

## Existing-code audit

- classification: NOT-FOUND before M5.7; built with M5.7's lab (`crates/skills/src/evolution.rs`) and harness (`benchmarks/skill-evolution`).
- proof: WSK-E2E-009: two families reported separately; a regression in one withholds the model-neutral label and REJECTs on gate 9 while the other family's line stands; one family promotes model-specific. The nightly schedule against live families is not set up (EXPERIMENT; DR-M3-002).

## Finding

EXPERIMENT: the mechanism behaves as its hypothesis requires on the fixture; no lift over simpler refinement is established there (M5.7 ablation), so nothing is promoted and no ADR is proposed; the exit criterion of docs/41 ("promote only through ADR + measurable benefit; otherwise remove cleanly") is not yet decided either way.

## Limitations

- Deterministic fixture trials and the template proposer stand in for live models (DR-M3-002); intervals have no width.

## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`):

- `wsk_e2e_009_transfer_is_reported_per_model_and_a_regressing_family_blocks_the_neutral_label`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json copy alongside
