# Task Card — IMP-EV-0204 On-demand proposer retrieval from wiki index

## Identity

- Task ID: IMP-EV-0204
- Milestone: M5 Procedural runtime and skills (P0)
- Requirement: REQ-EV-0204 (disposition EXPERIMENT); owner label: Skill Evolution Lab; subsystem: skills
- Qualification: `QUAL-EV-0204` — Large evolution corpus stays within token budget and provenance remains complete.
- Evidence tier: production-equivalent (the production lab and harness over real directories and fixture trials; see M5.7 for what the experiment does not establish)

## Goal

On-demand proposer retrieval from wiki index, as an EXPERIMENT behind the Skill Registry and the Eval Harness (docs/26): isolated under the existing owner, never a second subsystem, promoted only through ADR and measurable benefit.

## Existing-code audit

- classification: NOT-FOUND before M5.7; built with M5.7's lab (`crates/skills/src/evolution.rs`) and harness (`benchmarks/skill-evolution`).
- proof: WSK-E2E-007: 2,000 traces → 120 patterns; the compact index is under a quarter of the full records; hydration of ten heads under a 1,200-token budget takes what fits, refuses the rest by name, counts only what was hydrated, and the candidate names exactly the hydrated sources.

## Finding

EXPERIMENT: the mechanism behaves as its hypothesis requires on the fixture; no lift over simpler refinement is established there (M5.7 ablation), so nothing is promoted and no ADR is proposed; the exit criterion of docs/41 ("promote only through ADR + measurable benefit; otherwise remove cleanly") is not yet decided either way.

## Limitations

- Deterministic fixture trials and the template proposer stand in for live models (DR-M3-002); intervals have no width.

## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`):

- `wsk_e2e_007_selective_hydration_respects_the_budget_and_names_its_sources`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json copy alongside
