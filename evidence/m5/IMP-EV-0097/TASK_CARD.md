# Task Card — IMP-EV-0097 Minimal code-mode interface: exec / wait / request_user_input

## Identity

- Task ID: IMP-EV-0097
- Milestone: M5 Procedural runtime and skills (P0)
- Requirement: REQ-EV-0097; owner label: Procedural Tool Runtime; subsystem: procedural-runtime / core-runtime
- Qualification: `QUAL-EV-0097` — Real coding task completes through procedural mode and every nested effect is policy/evidence-tracked.
- Evidence tier: real-system (E2E-012 on the real Core)

## Goal

Expose the minimal composition primitives while governed `tools.*` remain programmatically callable.

## Existing-code audit

- classification: NOT-FOUND before M5.4; this task is the qualification of M5.2–M5.4 against its own acceptance.
- production entry points: `proc.exec`, `proc.wait` (M5.4), `user.ask` as `request_user_input`; `services/modbit-core/src/procedural.rs` (every nested call a tool-call aggregate with policy decision and receipt).
- proof: `qual_m5_e2e_012_a_program_composes_governed_tools_in_the_isolate`: the program's five calls are five tool-call aggregates under the exec id, each through the pipeline; the declared write lands; the task reaches ReadyForReview; `qual_m5_6_…` shows the same effects and policy outcomes as direct mode in three pairs.

## Limitations

- `request_user_input` is the model's `user.ask` between programs, not callable from inside one.

## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`):

- `qual_m5_e2e_012_a_program_composes_governed_tools_in_the_isolate`
- `qual_m5_6_direct_and_procedural_modes_yield_the_same_effects_at_different_token_costs`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json copy alongside
