# Task Card — IMP-EV-0244 Task-conditioned harness generation

## Identity

- Task ID: IMP-EV-0244 (REQ-EV-0244, EXPERIMENT; owner Adaptive Profile Evaluator, subsystem eval-bench)
- Milestone: M10 (wave 1)
- Qualification: QUAL-EV-0244 — shadow candidate never controls production run.
- Evidence tier: production-equivalent (the real CLI and Core with a scripted provider; an experiment report, docs/42)
- Isolated behind the existing owner (the Eval Harness, `benchmarks/agent-engineering`); no second subsystem.

## Goal

Generate declarative profile variants conditioned on the task, only in shadow/eval (docs/41: "promote only through ADR + measurable benefit; otherwise remove cleanly").

## Existing-code audit

- classification: NOT-FOUND. The harness ran every trial under the fixed string configuration `direct` with `--max-turns`; no declarative profile type existed and the CLI could not set a run's tool-call or no-progress budgets.
- first missing link: no profile type the harness could vary.
- entry points: `benchmarks/agent-engineering/src/profile.rs` (`HarnessProfile`, `known_good`, `validate`, `run_args`, `configuration`, `generate`); `src/bin/competence_baseline.rs` (`--profile`, `experiment.json`); `src/bundle.rs` (`NotABaseline`); `apps/cli/src/main.rs` (`task run --max-tool-calls`, `--max-no-progress-turns`); `benchmarks/agent-engineering/tests/harness.rs` `reaching` (no workspace package reaches the harness through a normal or build dependency, from `cargo metadata`).

## Finding

EXPERIMENT: a variant is generated deterministically from the task and the known-good profile, runs only as a shadow trial reported under its digest, and cannot become the baseline. No benefit is measured: under the frozen protocol a waiting run is resumed up to four times, so a tighter turn budget mostly moves turns across resumes. Nothing is promoted and no ADR is proposed; the exit decision stays open.

## Verification

- `qual_ev_0244_a_generated_profile_runs_only_in_shadow_and_never_as_the_baseline` (real CLI and Core, scripted provider, the internal suite's `python-service/reject-zero`): the known-good profile's flags are the frozen protocol's (`--max-turns 24`); the generated variant differs and names the known-good digest it came from; run with `--profile` it writes `experiment.json` (`profile-experiment`, `shadow:<digest>`, the variant's budgets, one trial) and no `baseline.json`; the frozen run of the same task writes the `direct` baseline with 24 turns, which validates, and the same bundle labelled with the variant is refused `NotABaseline`; no workspace package reaches the harness through a product dependency (the resolved `cargo metadata` graph).
- Unit: `profile::tests::a_variant_is_conditioned_on_the_task_deterministic_and_within_bounds`.
- `architecture-lint`: 0 violations; the confinement is checked on the resolved dependency graph rather than as lint rules, because `tools/architecture-lint/rules.toml` is a locked path (DR-M0-002).

## Limitations

- A scripted provider stands in for a live model; no live comparison of variant and known-good cost or success exists.
- Variants change budgets and skill selection only.

## Evidence

- `evidence.json` in this directory
- docs/63 "Adaptive profile experiments"
