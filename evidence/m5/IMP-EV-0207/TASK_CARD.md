# Task Card — IMP-EV-0207 Persistent knowledge is critical hypothesis

## Identity

- Task ID: IMP-EV-0207
- Milestone: M5 Procedural runtime and skills (P0)
- Requirement: REQ-EV-0207 (disposition EXPERIMENT); owner label: Eval Harness; subsystem: eval-bench
- Qualification: `QUAL-EV-0207` — Promotion of evolution-lab mechanism requires statistically/practically meaningful lift vs simpler skill refinement.
- Evidence tier: production-equivalent (the production lab and harness over real directories and fixture trials; see M5.7 for what the experiment does not establish)

## Goal

Persistent knowledge is critical hypothesis, as an EXPERIMENT behind the Skill Registry and the Eval Harness (docs/26): isolated under the existing owner, never a second subsystem, promoted only through ADR and measurable benefit.

## Existing-code audit

- classification: NOT-FOUND before M5.7; built with M5.7's lab (`crates/skills/src/evolution.rs`) and harness (`benchmarks/skill-evolution`).
- proof: `req_ev_0207_…`: the ablation pairs an evolved candidate against a manual refinement; on the fixture the lift is 0 bp and the finding says the lab is not justified there; a clear lift is recognised on both counts; no pairs establish nothing. Finding: no ADR proposed; the mechanism stays EXPERIMENT.

## Finding

EXPERIMENT: the mechanism behaves as its hypothesis requires on the fixture; no lift over simpler refinement is established there (M5.7 ablation), so nothing is promoted and no ADR is proposed; the exit criterion of docs/41 ("promote only through ADR + measurable benefit; otherwise remove cleanly") is not yet decided either way.

## Limitations

- Deterministic fixture trials and the template proposer stand in for live models (DR-M3-002); intervals have no width.

## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`):

- `req_ev_0207_the_ablation_says_whether_the_lab_earns_its_complexity`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json copy alongside
