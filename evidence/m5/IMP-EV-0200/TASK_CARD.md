# Task Card — IMP-EV-0200 Validation gating and rollback

## Identity

- Task ID: IMP-EV-0200
- Milestone: M5 Procedural runtime and skills (P0)
- Requirement: REQ-EV-0200 (disposition EXPERIMENT); owner label: Skill Registry + Eval; subsystem: skills
- Qualification: `QUAL-EV-0200` — Candidate regressing safety/quality is rejected and previous active skill remains byte-identical.
- Evidence tier: production-equivalent (the production lab and harness over real directories and fixture trials; see M5.7 for what the experiment does not establish)

## Goal

Validation gating and rollback, as an EXPERIMENT behind the Skill Registry and the Eval Harness (docs/26): isolated under the existing owner, never a second subsystem, promoted only through ADR and measurable benefit.

## Existing-code audit

- classification: NOT-FOUND before M5.7; built with M5.7's lab (`crates/skills/src/evolution.rs`) and harness (`benchmarks/skill-evolution`).
- proof: WSK-E2E-004: a class regression, a cheaper-but-less-correct candidate and a safety failure each REJECT; the head's SKILL.md bytes are unchanged after the refused promotion; rollback restores 1.0.0 byte for byte.

## Finding

EXPERIMENT: the mechanism behaves as its hypothesis requires on the fixture; no lift over simpler refinement is established there (M5.7 ablation), so nothing is promoted and no ADR is proposed; the exit criterion of docs/41 ("promote only through ADR + measurable benefit; otherwise remove cleanly") is not yet decided either way.

## Limitations

- Deterministic fixture trials and the template proposer stand in for live models (DR-M3-002); intervals have no width.

## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`):

- `wsk_e2e_004_promotion_needs_the_gates_and_rollback_restores_the_head`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json copy alongside
