# Task Card — IMP-EV-0202 PURPOSE / motivating knowledge linkage

## Identity

- Task ID: IMP-EV-0202
- Milestone: M5 Procedural runtime and skills (P0)
- Requirement: REQ-EV-0202 (disposition EXPERIMENT); owner label: Skill Package; subsystem: skills
- Qualification: `QUAL-EV-0202` — Runtime loads purpose summary; detailed evolution wiki remains inaccessible by default.
- Evidence tier: production-equivalent (the production lab and harness over real directories and fixture trials; see M5.7 for what the experiment does not establish)

## Goal

PURPOSE / motivating knowledge linkage, as an EXPERIMENT behind the Skill Registry and the Eval Harness (docs/26): isolated under the existing owner, never a second subsystem, promoted only through ADR and measurable benefit.

## Existing-code audit

- classification: NOT-FOUND before M5.7; built with M5.7's lab (`crates/skills/src/evolution.rs`) and harness (`benchmarks/skill-evolution`).
- proof: `req_ev_0202_…`: the promoted head compiles to its instruction line with no pattern record, id or claim in the prompt; the PURPOSE and evidence are in the candidate and impact records under the lab; on the real Core, WSK-E2E-005 shows no wiki field in any request.

## Finding

EXPERIMENT: the mechanism behaves as its hypothesis requires on the fixture; no lift over simpler refinement is established there (M5.7 ablation), so nothing is promoted and no ADR is proposed; the exit criterion of docs/41 ("promote only through ADR + measurable benefit; otherwise remove cleanly") is not yet decided either way.

## Limitations

- Deterministic fixture trials and the template proposer stand in for live models (DR-M3-002); intervals have no width.

## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`):

- `req_ev_0202_a_promoted_skill_carries_its_purpose_not_the_wiki`
- `wsk_e2e_005_010_a_promoted_skill_reaches_the_model_without_the_wiki_and_recovery_needs_no_lab`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json copy alongside
