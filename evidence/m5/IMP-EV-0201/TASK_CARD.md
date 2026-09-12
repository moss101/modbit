# Task Card — IMP-EV-0201 Skill impact log

## Identity

- Task ID: IMP-EV-0201
- Milestone: M5 Procedural runtime and skills (P0)
- Requirement: REQ-EV-0201 (disposition EXPERIMENT); owner label: Skill Registry; subsystem: skills
- Qualification: `QUAL-EV-0201` — Audit can reconstruct why each skill version was accepted/rejected.
- Evidence tier: production-equivalent (the production lab and harness over real directories and fixture trials; see M5.7 for what the experiment does not establish)

## Goal

Skill impact log, as an EXPERIMENT behind the Skill Registry and the Eval Harness (docs/26): isolated under the existing owner, never a second subsystem, promoted only through ADR and measurable benefit.

## Existing-code audit

- classification: NOT-FOUND before M5.7; built with M5.7's lab (`crates/skills/src/evolution.rs`) and harness (`benchmarks/skill-evolution`).
- proof: WSK-E2E-004: `Promotion::impact` returns one record per decision — the REJECTED one with its gate reasons, the PROMOTED one with the version, the patch, the source patterns, the proposer configuration and the environment — and the rollback record.

## Finding

EXPERIMENT: the mechanism behaves as its hypothesis requires on the fixture; no lift over simpler refinement is established there (M5.7 ablation), so nothing is promoted and no ADR is proposed; the exit criterion of docs/41 ("promote only through ADR + measurable benefit; otherwise remove cleanly") is not yet decided either way.

## Limitations

- Deterministic fixture trials and the template proposer stand in for live models (DR-M3-002); intervals have no width.

## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`):

- `wsk_e2e_004_promotion_needs_the_gates_and_rollback_restores_the_head`
- `req_ev_0202_a_promoted_skill_carries_its_purpose_not_the_wiki`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json copy alongside
