# Task Card — IMP-EV-0199 Skill Proposer from wiki + traces

## Identity

- Task ID: IMP-EV-0199
- Milestone: M5 Procedural runtime and skills (P0)
- Requirement: REQ-EV-0199 (disposition EXPERIMENT); owner label: Skill Evolution Lab; subsystem: skills
- Qualification: `QUAL-EV-0199` — Candidate diff references motivating evidence IDs and changes one bounded behavior.
- Evidence tier: production-equivalent (the production lab and harness over real directories and fixture trials; see M5.7 for what the experiment does not establish)

## Goal

Skill Proposer from wiki + traces, as an EXPERIMENT behind the Skill Registry and the Eval Harness (docs/26): isolated under the existing owner, never a second subsystem, promoted only through ADR and measurable benefit.

## Existing-code audit

- classification: NOT-FOUND before M5.7; built with M5.7's lab (`crates/skills/src/evolution.rs`) and harness (`benchmarks/skill-evolution`).
- proof: WSK-E2E-003/006: the candidate carries the base content hash, one atomic patch to SKILL.md adding one instruction line (version bumped), PURPOSE naming the claim, the motivating pattern ids and the three trace ids, the declared tools and ceiling unchanged; a proposer without evidence proposes nothing.

## Finding

EXPERIMENT: the mechanism behaves as its hypothesis requires on the fixture; no lift over simpler refinement is established there (M5.7 ablation), so nothing is promoted and no ADR is proposed; the exit criterion of docs/41 ("promote only through ADR + measurable benefit; otherwise remove cleanly") is not yet decided either way.

## Limitations

- Deterministic fixture trials and the template proposer stand in for live models (DR-M3-002); intervals have no width.

## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`):

- `wsk_e2e_003_006_a_candidate_is_atomic_and_cannot_widen_authority`
- `a_proposer_model_is_a_trait_the_template_stands_in_for`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json copy alongside
