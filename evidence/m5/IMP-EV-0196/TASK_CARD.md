# Task Card — IMP-EV-0196 Separate raw experience / persistent wiki / executable skills

## Identity

- Task ID: IMP-EV-0196
- Milestone: M5 Procedural runtime and skills (P0)
- Requirement: REQ-EV-0196 (disposition EXPERIMENT); owner label: Skill Evolution Lab; subsystem: skills
- Qualification: `QUAL-EV-0196` — Delete/reject candidate skill; raw traces and wiki knowledge remain intact.
- Evidence tier: production-equivalent (the production lab and harness over real directories and fixture trials; see M5.7 for what the experiment does not establish)

## Goal

Separate raw experience / persistent wiki / executable skills, as an EXPERIMENT behind the Skill Registry and the Eval Harness (docs/26): isolated under the existing owner, never a second subsystem, promoted only through ADR and measurable benefit.

## Existing-code audit

- classification: NOT-FOUND before M5.7; built with M5.7's lab (`crates/skills/src/evolution.rs`) and harness (`benchmarks/skill-evolution`).
- proof: WSK-E2E-004: the rejected candidate and its qualification stay under `candidates/` and `impact/`, the traces under `traces/` and the patterns under `patterns/` are untouched (count asserted), the registry head byte-identical.

## Finding

EXPERIMENT: the mechanism behaves as its hypothesis requires on the fixture; no lift over simpler refinement is established there (M5.7 ablation), so nothing is promoted and no ADR is proposed; the exit criterion of docs/41 ("promote only through ADR + measurable benefit; otherwise remove cleanly") is not yet decided either way.

## Limitations

- Deterministic fixture trials and the template proposer stand in for live models (DR-M3-002); intervals have no width.

## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`):

- `wsk_e2e_004_promotion_needs_the_gates_and_rollback_restores_the_head`
- `wsk_e2e_001_a_sealed_trace_is_immutable_and_corrections_reference_it`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json copy alongside
