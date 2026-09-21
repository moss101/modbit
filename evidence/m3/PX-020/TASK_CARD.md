# Task Card — PX-020 Fixed M2 competence baseline on public and internal suites

## Identity

- Task ID: PX-020
- Milestone: M3; release: BETA
- Requirement: REQ-PX-020 (related: REQ-EV-0029); owner label: eval-bench; subsystem: eval-bench
- Qualification: `QUAL-PX-020` / `PX-E2E-020` — both suites run under the frozen protocol on the real product with the direct configuration; the immutable baseline bundle (digests, model metadata, per-task results, intervals) is recorded and referenced by digest. Negative: a baseline with gold-patch access, unpinned images or missing trial counts is rejected; no target may be recorded before this baseline exists.
- Evidence tier: real-system (a real model through the real product)
- Decision Records: `docs/decisions/DR-M3-003-benchmark-and-conformance-live-model-halves.md` (why the task was BLOCKED), `docs/decisions/DR-M9-002-live-proofs-on-compatible-gateway.md` (the gateway the live model is reached through), `docs/decisions/DR-M3-005-competence-baseline-public-suite-deferred.md` (the internal baseline seals the task; the public slice stays open)

## Goal

Record the competence baseline of docs/63 — the product as it stands with M2 and the M3 retrieval harness real, in the direct single-model configuration — so that PX-021 can set targets against it and the release gate can measure regression.

## Existing-code audit

- classification: NOT-FOUND before this task. `benchmarks/context-economics` published paired reports through the Core for a scripted model and nothing ran a suite of tasks with a real model, scored repository state, or produced a bundle referenced by digest; `docs/12` named `benchmarks/agent-engineering` and it did not exist.
- production entry points now:
  - `benchmarks/agent-engineering/src/{suite,events,bundle}.rs` — the frozen suite and its digest, scoring from the Core's own event log, doc 63 metrics with Wilson intervals, the bundle and its refusals, the target record that cannot exist before a valid baseline digest.
  - `benchmarks/agent-engineering/src/bin/competence_baseline.rs` — every trial a fresh Core profile on a fresh copy of the fixture, driven only through `modbit-cli`; nothing on the Core's side knows it is being measured.
  - `benchmarks/agent-engineering/suites/internal/` — six tasks with hidden acceptance and DI-3-protected tests.
  - `.github/workflows/live-competence.yml` — the suite on hosted CI with the repository's provider secrets and gateway variables.
  - `services/modbit-core/src/runtime.rs` — the product defect the baseline surfaced: a check the agent runs itself now records its residue instead of having it attributed at COMPLETION (`residue_of_a_check_the_agent_runs_is_recorded_not_attributed`).
- proof: the bundle below, produced on hosted CI by run 35560128454 against a real model through the real product.

## The baseline (internal suite)

- bundle: `internal/baseline.json`, digest `59301bb8f2699794b3e26d1699db94e5689aabd991c3b168f26801b7d1a7b786` (`internal/baseline.sha256`); summary `internal/summary.md`
- suite `internal-competence` v1.0.0, task-list digest `a6cdf39a8dac3294b97cc61d8497dbbe5923b2129e185a727d75892c2255ac0c`, 6 tasks
- protocol: 3 trials per task, max_turns 24, configuration `direct`, endpoint `openai` at `api.z.ai`, model `glm-5.3-flash`, catalog prices [0.15, 0.5] USD/Mtok, gold_patch_access false, images [], questions answered by protocol, approvals `deny`
- environment: Modbit revision `1507707cab9a328939bacb57b260f967184ab84c`, build digest `fbf23a98251f0939793f24c48f2c0de5a7a5183713ddcde2fabe0a6cddb47749`, harness `competence-baseline/1`, linux-x86_64; toolchains {"cargo": "cargo 1.97.1 (c980f4866 2026-06-30)", "git": "git version 2.55.0", "node": "v26.5.1", "python3": "Python 3.12.3", "rustc": "rustc 1.97.1 (8bab26f4f 2026-07-14)"}
- generated 2026-09-21T04:51:37Z

| metric | value |
|---|---|
| verified success | 12/18 = 0.667 (95% Wilson 0.437–0.837) |
| first-pass success (zero RepairAttempts) | 10/18 = 0.556 (95% Wilson 0.337–0.754) |
| first-candidate success (≤1 attempt, no escalation) | 12/18 = 0.667 (95% Wilson 0.437–0.837) |
| repair loops (attempts → trials) | {'0': 16, '1': 2} |
| equivalent-hypothesis escalations | 0 (0.00/trial) |
| no-progress escalations | 4 (0.22/trial) |
| wrong-effect attempts blocked | 0 (0.00/trial) |
| evidence coverage | 12/12 = 1.000 (95% Wilson 0.758–1.000) |
| cost and time | total 0.7040 USD; per verified success 0.0586645; wall 7721780 ms total, tool 18448 ms |
| retrieval discipline (edits without a retrieval record) | 0 |
| scope discipline (files outside plan v1 / expansions / questions per trial) | 0.06 / 2.11 / 0.00 |
| regression attribution | 0 |
| flaky-check rate | 12 quarantines (0.67/trial) |
| test-integrity violations (DI-3) | 1 |
| test-selection quality | not measured: the fixture suites are below the size at which TARGETED selection differs from the full suite (PX-035 reports it) |

| task | verified | first-pass | first-candidate | states | repair attempts |
|---|---|---|---|---|---|
| python-service/discount | 3/3 | 2/3 | 3/3 | ReadyForReview, ReadyForReview, ReadyForReview | [0, 0, 1] |
| python-service/reject-zero | 3/3 | 3/3 | 3/3 | ReadyForReview, ReadyForReview, ReadyForReview | [0, 0, 0] |
| rust-cli/negative-amounts | 0/3 | 0/3 | 0/3 | Waiting, Waiting, Waiting | [0, 0, 0] |
| rust-cli/reject-negative | 2/3 | 2/3 | 2/3 | ReadyForReview, Waiting, ReadyForReview | [0, 0, 0] |
| ts-webapp/parse-money | 1/3 | 1/3 | 1/3 | ReadyForReview, Waiting, Waiting | [0, 0, 0] |
| ts-webapp/reject-negative | 3/3 | 2/3 | 3/3 | ReadyForReview, ReadyForReview, ReadyForReview | [0, 1, 0] |

Trial end states: {'ReadyForReview': 12, 'Waiting': 6}. Harness notes: none.

## What the baseline says (for PX-021's targets)

- Every fix the model produced passed hidden acceptance in every trial (18/18 `acceptance_passed`); the six non-verified trials were completion failures, not wrong code: on `rust-cli/negative-amounts` (3/3) and `rust-cli/reject-negative` (1/3) the model treated the fixture's seeded pre-existing failure (`preexisting_failing_unrelated`, recorded `KNOWN_FAILING` at BASELINE) or the other seeded defect as something to resolve and exhausted its turns hunting evidence instead of completing; on `ts-webapp/parse-money` (2/3) it looped on a targeted verification and once created a test under `src/` (a DI-3 flag). Recognising a known-failing check as out of scope is therefore the behaviour that separates this model's first-pass rate from its acceptance rate.
- Retrieval discipline held (0 edits without a retrieval record), regressions were 0, every completed trial recorded a plan and a self-review (evidence coverage 12/12), and the flake protocol quarantined the seeded flaky test 12 times without attributing it.
- Cost: 0.70 USD for the suite at catalog list prices; 0.059 USD per verified success; the median trial took about 3 minutes, the turn-exhausted ones 10–16.

## Limitations

- This is the internal suite only. The public benchmark (SWE-bench Verified under the frozen protocol with images pinned by digest) has not run; DR-M3-005 states what it needs and keeps it an open item here. No public-suite target may be recorded until its bundle exists.
- The live model is `glm-5.3-flash` through a compatible gateway at `api.z.ai` (DR-M9-002), not a provider's own production endpoint. The bundle names it; the numbers are for this model configuration and are never aggregated with another.
- The suite has six tasks on three small fixtures; intervals are correspondingly wide. It measures the product path — real Core, real tools, real verification and repair loop, real hidden acceptance — not model quality in general.
- Test-selection quality is stated as not measured on these suites.
- Environment hazard, stated rather than hidden: CPython trusts a bytecode cache whose recorded source size and mtime-second still match, so a same-size edit within a second of a reproduction import can be verified against stale bytecode. The suite's seeded edits change the file size; the harness does not alter the interpreter's environment.

## Verification

- `the_internal_suite_is_frozen_and_its_digest_covers_hidden_acceptance`, `bundle_validation_refuses_what_the_frozen_protocol_forbids_and_no_target_precedes_a_baseline`, `event_scoring_counts_the_doc_63_signals_from_the_log`, `a_bundle_is_referenced_by_the_digest_of_its_file_bytes`, `the_binary_drives_a_real_task_through_the_real_cli_and_core_and_scores_it` (benchmarks/agent-engineering/tests/harness.rs; macOS, Linux, Windows)
- `residue_of_a_check_the_agent_runs_is_recorded_not_attributed` (services/modbit-core/tests/surface_protocol.rs)
- live: `.github/workflows/live-competence.yml` run 35560128454

## Evidence

- `evidence.json` in this directory; `internal/` holds the bundle, its digest and summary; `live-run-35560128454.json` the workflow run record
- commits: `3d91938` (PR #20 squash: the harness, the suite, the Core residue fix), `e3c105c` (one job per task, merge, teardown, diagnostics), `a37afa6` (regression labels, protected-test rule, report retention), `1507707cab9a328939bacb57b260f967184ab84c` (absolute module link); hosted CI: PR #20 run 35538461759 on c5ecfbd (14/14 on macOS, Linux and Windows), main run 35543942063 on 3d91938 green; the main runs on a37afa6/1507707 passed every Rust job and failed only the documented desktop-E2E timing flake, rerun
- earlier live runs and why they are not baselines: `live-runs.json`
