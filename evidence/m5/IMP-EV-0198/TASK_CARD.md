# Task Card — IMP-EV-0198 Wiki Maintainer consolidation

## Identity

- Task ID: IMP-EV-0198
- Milestone: M5 Procedural runtime and skills (P0)
- Requirement: REQ-EV-0198 (disposition EXPERIMENT); owner label: Skill Evolution Lab; subsystem: skills
- Qualification: `QUAL-EV-0198` — Seed contradictory traces; maintainer records both with provenance/confidence rather than overwriting.
- Evidence tier: production-equivalent (the production lab and harness over real directories and fixture trials; see M5.7 for what the experiment does not establish)

## Goal

Wiki Maintainer consolidation, as an EXPERIMENT behind the Skill Registry and the Eval Harness (docs/26): isolated under the existing owner, never a second subsystem, promoted only through ADR and measurable benefit.

## Existing-code audit

- classification: NOT-FOUND before M5.7; built with M5.7's lab (`crates/skills/src/evolution.rs`) and harness (`benchmarks/skill-evolution`).
- proof: WSK-E2E-002: two verified and one failed trace with the same observation become one pattern with two supporting and one contradicting trace id and 6666 bp confidence; a later failing trace yields a superseding revision at 5000 bp with the earlier one persisting; the skill files and no memory store are touched.

## Finding

EXPERIMENT: the mechanism behaves as its hypothesis requires on the fixture; no lift over simpler refinement is established there (M5.7 ablation), so nothing is promoted and no ADR is proposed; the exit criterion of docs/41 ("promote only through ADR + measurable benefit; otherwise remove cleanly") is not yet decided either way.

## Limitations

- Deterministic fixture trials and the template proposer stand in for live models (DR-M3-002); intervals have no width.

## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`):

- `wsk_e2e_002_the_maintainer_consolidates_contradictory_traces_without_overwriting`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json copy alongside
