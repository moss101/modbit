# Task Card — IMP-EV-0246 Bounded repair of harness/profile

## Identity

- Task ID: IMP-EV-0246 (REQ-EV-0246, EXPERIMENT; owner Adaptive Profile Evaluator, subsystem eval-bench)
- Milestone: M10 (wave 1)
- Qualification: QUAL-EV-0246 — third repair attempt rejected; fallback run remains functional.
- Evidence tier: production-equivalent (the real CLI and Core with a scripted provider; an experiment report)
- Built on IMP-EV-0244's profile type; isolated in the Eval Harness. Not docs/28's `RepairPolicy`, which repairs the task's code, not the profile.

## Goal

At most two bounded repairs of a failing profile, with the static known-good fallback always available.

## Existing-code audit

- classification: NOT-FOUND: no profile variant existed to repair, and nothing consumed a trial's failure signals to change one.
- entry points: `benchmarks/agent-engineering/src/profile.rs` (`repair`, `MAX_PROFILE_REPAIRS`, `ProfileRefused::{RepairLimit, KnownGood}`); `src/bin/competence_baseline.rs` (`--adaptive`, `adaptive_trial`: generate → repair from the trial's features → refusal → known-good fallback, every attempt recorded).

## Finding

EXPERIMENT: repair is bounded as required and the fallback is always reachable; no benefit of repair over the fallback is measured. Nothing is promoted and no ADR is proposed; the exit decision stays open.

## Verification

- `qual_ev_0246_the_third_repair_is_refused_and_the_known_good_fallback_still_verifies` (real CLI and Core; a provider that fails the first three attempts whatever their profile, then behaves): the report lists the generated variant, `repaired/…/1` and `repaired/…/2`, all failing, then the third repair refused, then `direct/known-good` with the frozen 24 turns verified — the trial is a verified success.
- Unit: `profile::tests::a_variant_is_repaired_twice_and_the_third_repair_is_refused` (the known-good profile is never repaired).

- Finding on Windows CI (fixed here): the known-good fallback verified yet the gate rejected it with a DI-1 DENY on `__pycache__/*.pyc` outside the plan. Every attempt of a trial reused one scratch directory; on Windows the previous attempt's processes still held its files, `remove_dir_all` failed silently, and the next attempt committed the leftover `__pycache__` into its base, which its own pytest then rewrote. Each attempt now has its own scratch directory and a leftover that cannot be removed is an error; `copy_fixture` also leaves interpreter byproducts out (`python_byproducts_are_not_part_of_a_trial_workspace`); the qualification prints the gate's own events if it fails again.

## Limitations

- The failure is forced by the provider, not by the profile: the test proves the bound and the fallback, not that a repair helps.
- Repair uses two features (`budget:max_turns`, `boundary:no_progress`); the richer failure taxonomy (REQ-EV-0245) is not yet mapped.

## Evidence

- `evidence.json` in this directory
- docs/63 "Adaptive profile experiments"
