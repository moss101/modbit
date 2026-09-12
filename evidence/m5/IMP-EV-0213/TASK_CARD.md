# Task Card — IMP-EV-0213 Compact skill mode / selective context

## Identity

- Task ID: IMP-EV-0213
- Milestone: M5 Procedural runtime and skills (P0)
- Requirement: REQ-EV-0213; owner label: Skill Compiler; subsystem: skills
- Qualification: `QUAL-EV-0213` — Token benchmark compares eager package vs compiled skill projection.
- Evidence tier: production-equivalent (the production compiler over a realistic package; bytes, not a provider's tokens)

## Goal

Skills compile into minimal instructions with large content by reference, and the saving against injecting the whole package is measured, not assumed.

## Existing-code audit

- classification: NOT-FOUND before M5.5; M5.5's `compile` bounds the instructions (8 KiB in the Core) and names procedures and resources by reference.
- production entry points: `crates/skills::compile` (budgeted instructions with the cut declared; `resources`/`procedures` by reference), `services/modbit-core/src/skills.rs::INSTRUCTION_BUDGET_BYTES`.
- proof: `qual_ev_0213_compiled_projection_is_a_fraction_of_the_eager_package` prints a `BENCH` report: a package with a 25 KB reference document and two procedures — eager injection would send every byte; the compiled projection is under a tenth of it and names the two resources and two procedures by reference; the reference text never enters the instructions.

## Limitations

- Bytes on the crate's fixture, not tokens of a live provider; the benchmark's method says so.

## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`):

- `qual_ev_0213_compiled_projection_is_a_fraction_of_the_eager_package`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json copy alongside
