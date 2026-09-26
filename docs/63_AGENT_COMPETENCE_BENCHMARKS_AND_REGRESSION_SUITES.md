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

## Execution (PX-020)

The Eval Harness lives at `benchmarks/agent-engineering` (`12_REPOSITORY_AND_MODULE_LAYOUT.md`). The internal suite is the frozen task list `suites/internal/tasks.json` (id `internal-competence`): six tasks over the `rust-cli`, `ts-webapp` and `python-service` fixtures of doc 50, two per language at easy and medium difficulty, each with hidden acceptance the agent never sees and a DI-3-protected test file; the bundle's task-list digest covers the suite file and every hidden file. The `competence-baseline` binary runs every trial as a fresh Core profile on a fresh copy of the fixture, driven only through `modbit-cli` in the direct single-model configuration; a typed question gets the suite's one fixed answer, a protected-effect approval is denied, and hidden files are placed only after the task has ended. Scoring comes from the Core's own event log and `GetTaskEconomics`; the harness scores test integrity again on top of the engine's DI-3. The bundle is referenced by the SHA-256 of `baseline.json` as written, and its validation refuses gold-patch access, an image without a digest, and a missing or inconsistent trial count; a target record (PX-021) cannot exist without a valid baseline digest. `.github/workflows/live-competence.yml` runs the suite on hosted CI with the repository's provider secrets and gateway variables (`15_MODEL_ROUTER_AND_PROVIDER_GATEWAY.md` "Endpoint configuration"), and an accepted run is copied under `evidence/m3/PX-020/`.

Two rates are reported for the first fix, and a Decision Record chooses which carries a target: **first-pass success** exactly as defined above (zero `RepairAttemptRecorded`), and **first-candidate success** (at most one repair attempt and no `RepairEscalated`). Reproducing a reported failure with an ad-hoc command records `ReproductionRecorded` and opens no verification signature, so a direct fix after it is still a first pass; a repair attempt is required only after a failing planned verification (`28_AGENT_COMPETENCE_PLANNING_VERIFICATION_AND_REPAIR.md` §5). Test-selection quality is stated as not measured on these suites. The public benchmark runs under the same bundle rules with its container images pinned by digest; until its harness exists, the task card names it as open.

## Adaptive profile experiments (IMP-EV-0244, IMP-EV-0246; EXPERIMENT)

As built: a harness profile (`benchmarks/agent-engineering/src/profile.rs`) is declarative — the run's turn, tool-call and no-progress budgets and the skills a trial selects, validated against fixed bounds, 0 meaning the Core's default — and nothing the product's policy, tools or prompts treat as special. The pinned known-good profile is the frozen protocol (its flags are `--max-turns <n>` alone). `generate` derives a task-conditioned variant deterministically from the task's difficulty and tier (easy: half the turns and less patience; hard or tier C: half again the turns and more patience); `repair` moves one bound by a fixed step from what the failed trial showed (`budget:max_turns` → two more turns, `boundary:no_progress` → one more turn of patience), at most `MAX_PROFILE_REPAIRS` = 2 times, and refuses the third; the fallback is the known-good profile. `competence-baseline --profile <file>` runs a trial under a given profile and `--adaptive` runs the generate → repair → fallback loop; either is a shadow experiment written as `experiment.json` (configuration `shadow:<digest>` or `shadow:adaptive/<known-good digest>`, every attempt and refusal listed) and never as `baseline.json`, and a bundle labelled with anything but `direct` is refused as a baseline (`NotABaseline`). No workspace package reaches the harness through a normal or build dependency (checked on the resolved `cargo metadata` graph by the qualification), so a variant can shape an evaluation trial and nothing else. Proven by `qual_ev_0244_a_generated_profile_runs_only_in_shadow_and_never_as_the_baseline` and `qual_ev_0246_the_third_repair_is_refused_and_the_known_good_fallback_still_verifies` through the real CLI and Core. Finding: the mechanism behaves as the hypotheses require; no benefit is measured (the frozen protocol resumes a waiting run up to four times, so a tighter turn budget mostly moves turns across resumes), so nothing is promoted and no ADR is proposed; the exit decision stays open.
