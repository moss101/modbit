# Task Card — EPR-019 Independent gate calibration release suite

## Identity

- Task ID: EPR-019 (REQ-EPR-019; related REQ-EV-0029, REQ-EV-0112; owner eval-bench)
- Milestone: M9, phase 5; prerequisites EPR-007, EPR-017, EPR-010 (COMPLETE)
- Qualification: QUAL-EPR-019 / EPR-E2E-019 / EPR-FI-019 (docs/61)
- Evidence tier: release-critical (evidence semantics, the promotion gate)
- No duplicate gate, risk engine or verifier: the suite drives `modbit_verification` (engine, invariants, gate) and `modbit_policy::derive_realized_risk` as they are.

## Existing-code audit

- classification: NOT-FOUND. The EPR-008 corpus pins the risk rules' outputs (a tuning/regression set); nothing measured the gate or the rules against oracle labels on held-out data, and nothing could fail a release on an unsafe gate.
- first missing link: no held-out, oracle-labelled corpora.
- production entry points: `benchmarks/gate-calibration/src/lib.rs` (`calibrate`, `run_acceptance_case`, `run_risk_case`, `check_separation`, `tuning_lineages`, `metrics`/`wilson`, `ThresholdProfile`, `release_check`, `Bundle`); `src/bin/gate_calibration.rs`; `corpora/` (8 acceptance cases in two new defect lineages on `tests/fixtures/repos/rust-cli`, two hidden oracles, 18 risk cases).

## Verification

- `qual_epr_019_gates_are_measured_independently_on_the_holdout_and_an_unsafe_gate_fails_release` (real engine running `cargo test` on the fixture, real invariants, rules and gate, hidden oracles): every case's verdict and oracle pinned — the two candidates the visible tests cannot tell from correct (an overfitted zero check, an off-by-one maximum) are the gate's false accepts, the two that break formatting are rejected, the four correct ones accepted; the risk misses pinned (a dependency bump and a payments change not sent to review, a `.env.production` change not treated as a secret — a critical miss); rates FA 2/4, FR 0/4, FN 3/13, FP 0/5, critical miss 1/6 with Wilson intervals; per leg the initial leg's false accepts are 2/3 and the escalation's 0/1. Without a profile the release check fails `MISSING_THRESHOLD_PROFILE`; with a strict (test-only) profile and a router saving 5 000 minor units it fails `UNSAFE_GATE` on false accepts and critical misses and `INSUFFICIENT_SAMPLES`; a profile missing a rate is `MISSING_THRESHOLD`; a permissive (test-only) profile passes.
- `a_contaminated_holdout_is_refused`: a lineage from the competence suite is `Contaminated`; a lineage shared between the two holdouts is `SharedLineage`.
- `a_mislabeled_case_is_refused_by_its_oracle`: the overfitted candidate declared correct is `Mislabeled` (the oracle says incorrect).
- EPR-FI-019 mutation: suppressing critical misses in the risk comparison fails the pinned test.

## Limitations

- No approved threshold profile exists: the release check fails until the owner approves one (docs/61 — the dossier invents no numbers). EPR-GATE-D, E and G stay OPEN on that decision.
- The corpora are small (8 + 18): the intervals are wide and a real profile's sample minima will need more cases.
- The acceptance corpus uses one fixture language (Rust) so it runs on every CI platform; the risk corpus is facts, not repositories.
- A found rule defect (the `.env.*` secret surface) is raised as its own task, not fixed here.

## Evidence

- `evidence.json` in this directory
- docs/61 "Thresholds, priors and independent gate calibration" as built
