# Agent competence benchmarks and regression suites

> **Authority date:** 2026-09-05  
> **Decision:** DR-PX-2026-09-05 item 6. Owner: eval-bench, with verification for the internal suite fixtures. Metric refinements and the two-baseline rule: DR-PX-2026-09-05-006.  
> **Rule:** targets are established only after a fixed competence baseline. Until then every number here is a metric definition, not a claim or a goal.

## Two suites, kept separately

**Public benchmark.** SWE-bench Verified or its maintained successor, run under a frozen protocol: pinned model and provider configuration, pinned Modbit revision and harness version, pinned container images, fixed trial count with confidence intervals, no test-time access to gold patches. Results are reported per model configuration and never aggregated across configurations. This suite exists for external comparability, including against products like Cursor's agents; it cannot be tuned to. The product's own contracts apply unchanged inside the benchmark harness: no benchmark-specific prompts, tools or policy relaxations, and the test-integrity invariant DI-3 of `64_VERIFICATION_EXECUTION_CONTRACTS.md` stays enforced. A trial in which the candidate modifies, deletes, skips or weakens any test named by the task's visible or hidden acceptance is scored as failed regardless of the runner's verdict.

**Internal competence regression suite.** Tasks drawn from the fixture repositories of `50_TEST_STRATEGY_REAL_SYSTEM_GATES.md` (`ts-webapp`, `rust-cli`, `python-service`, `multi-package`, `conflict-repo`) plus held-out tasks distilled from real anonymized runs where policy permits. Each task has an observable acceptance in repository state and tests, a seeded difficulty label, and a language tier. This suite exercises the whole product path: real Core, real tools, real verification, real repair loop.

## Metrics

| Metric | Definition |
|---|---|
| Verified success | tasks whose derived verification plan and acceptance gate pass, across N independent trials |
| First-pass success | verified success with zero RepairAttempts |
| Repair loops | RepairAttempts per task; distribution, not mean only |
| Equivalent-hypothesis escalations | count and rate, from `RepairEscalated` events |
| No-progress escalations | count and rate, from `NoProgressDetected` events |
| Wrong-effect attempts blocked | tool calls denied by policy per task |
| Evidence coverage | share of completions whose self-review and plan are complete |
| Cost and time | total inference cost, tool time, wall-clock per verified success (ADR-R-045 view) |
| Retrieval discipline | edits without retrieval record (must be zero), Context Pack precision proxy |
| Scope discipline | files changed outside the **original** plan (`plan_v1`) per task, scope expansions per task, and scope questions asked; never measured against the last plan revision |
| Regression attribution | checks that passed at BASELINE and failed at the candidate revision after the flake protocol; must be zero at acceptance unless declared in the plan |
| Flaky-check rate | `FLAKY` labels per run and quarantines per task, from `FlakyCheckQuarantined` events |
| Test-integrity violations | DI-3 violations attempted and denied per task; an accepted result must have zero |
| Test-selection quality | precision and recall of TARGETED selection against the full suite on fixtures (PX-035 from M3) |

## Two baselines, kept distinct

- **Routing direct baseline (EPR-000, M2).** The raw single-model initial-leg baseline of `49_EXECUTION_POLICY_REQUIREMENTS_AND_TASKS.md`, captured as soon as the real local loop exists. It is the reference for EPR-005 and the execution-policy gates of doc 61, not for competence targets.
- **Competence baseline (PX-020, M3).** Both suites run under the frozen protocol on the product as it stands when M2 and the M3 retrieval harness are real, in the direct single-model configuration. It is the reference for competence targets and the regression gate below. Its bundle names the exact product revision so nobody has to infer whether context intelligence was present: it was.

The two bundles are immutable, referenced by digest and never merged.

## Baseline-then-target rule

1. **Fixed competence baseline (PX-020).** Run both suites with the direct single-model configuration under the frozen protocol and record the baseline bundle: build digest, model and provider metadata, seeds where available, per-task results, event log checksums. The baseline is immutable and referenced by digest.
2. **Targets (PX-021).** Only after the baseline exists may targets be set, by Decision Record, per metric and per language tier, with the statistical method and trial count stated. Targets are versioned and stored with the profile that owns the release gate.
3. **Regression gate.** A release candidate fails the competence gate if verified success, first-pass success or repair-loop distribution regress beyond the approved threshold against the current baseline with confidence intervals; routing changes that improve cost while regressing competence fail (consistent with ADR-R-056). Public-suite regressions are reported and block promotion of the model configuration they measure.

## Reporting

Every run emits a bundle: suite version, task list digest, Modbit revision, harness version, model configuration, trials, per-task outcomes, metrics with intervals, RepairAttempt histories, flake quarantines, regression attributions, diff-invariant findings and cost. Reports state what was measured and never generalize across suites or model configurations. Numbers from other products' published results are context, not comparison, unless reproduced under this protocol.

## Relationship to other gates

Competence gates are additional to the real-effect qualifications in docs 42/61/62 and to the EPR gates. A competence improvement cannot excuse a security, recovery or gate-calibration regression, and none of those can excuse a competence regression.
