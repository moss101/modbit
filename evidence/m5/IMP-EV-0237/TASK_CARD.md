# Task Card — IMP-EV-0237 Self-improving skills

## Identity

- Task ID: IMP-EV-0237
- Milestone: M5 Procedural runtime and skills (P0)
- Requirement: REQ-EV-0237 (disposition EXPERIMENT); owner label: Skill Evolution Lab; subsystem: skills
- Qualification: `QUAL-EV-0237` — Skill cannot self-promote without eval/promotion transaction.
- Evidence tier: production-equivalent (the production lab and harness over real directories and fixture trials; see M5.7 for what the experiment does not establish)

## Goal

Self-improving skills, as an EXPERIMENT behind the Skill Registry and the Eval Harness (docs/26): isolated under the existing owner, never a second subsystem, promoted only through ADR and measurable benefit.

## Existing-code audit

- classification: NOT-FOUND before M5.7; built with M5.7's lab (`crates/skills/src/evolution.rs`) and harness (`benchmarks/skill-evolution`).
- proof: WSK-E2E-004: promotion is refused under a REJECT qualification and under another candidate's qualification; the `ProposerModel` trait and `Proposer` hold no head write — only `Promotion::promote` with a PROMOTE qualification and the lab's signing key moves the head; on the real Core the production run selects the head only (WSK-E2E-005).

## Finding

EXPERIMENT: the mechanism behaves as its hypothesis requires on the fixture; no lift over simpler refinement is established there (M5.7 ablation), so nothing is promoted and no ADR is proposed; the exit criterion of docs/41 ("promote only through ADR + measurable benefit; otherwise remove cleanly") is not yet decided either way.

## Limitations

- Deterministic fixture trials and the template proposer stand in for live models (DR-M3-002); intervals have no width.

## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`):

- `wsk_e2e_004_promotion_needs_the_gates_and_rollback_restores_the_head`
- `wsk_e2e_005_010_a_promoted_skill_reaches_the_model_without_the_wiki_and_recovery_needs_no_lab`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json copy alongside
