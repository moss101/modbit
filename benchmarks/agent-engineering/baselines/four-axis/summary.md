# Competence baseline — four-axis v1.0.0

- task list digest: `d129a6a0cb84d4a5e32456fd0ba4fcafcbc506037484d017e9bc9878e1da6a3f`
- bundle digest: `e4e380fac17dc75ee444cc5f02fa7ea7c6aa14b9d1c46afcfdd68a70700b5e3b`
- protocol: 1 trials/task, max_turns 24, openai `gpt-5-mini` at 127.0.0.1:54067 (direct)
- Modbit revision: `9915c0749ec5085811f54247c2af92ae09cec06e`; build digest `a86a34ba904e`; harness competence-baseline/1

| metric | value | 95% interval |
|---|---|---|
| verified success | 40/40 = 1.000 | 0.912–1.000 |
| first-pass success (zero RepairAttempts) | 40/40 = 1.000 | 0.912–1.000 |
| first-candidate success (at most one repair attempt, no escalation) | 40/40 = 1.000 | 0.912–1.000 |
| repair loops (attempts→trials) | {0: 40} | — |
| equivalent-hypothesis escalations | 0 (0.00/trial) | — |
| no-progress escalations | 0 (0.00/trial) | — |
| wrong-effect attempts blocked | 0 (0.00/trial) | — |
| evidence coverage | 40/40 = 1.000 | 0.912–1.000 |
| cost | total 0.1503 USD; per verified success Some(0.0037581249999999997) | — |
| edits without retrieval record | 0 | must be 0 |
| files outside original plan / expansions / questions per trial | 0.00 / 0.00 / 0.00 | — |
| regressions attributed | 0 | — |
| flaky quarantines | 0 (0.00/trial) | — |
| DI-3 violations | 0 | — |

| task | verified | first-pass | states |
|---|---|---|---|
| memory/changelog-prefix | 1/1 | 1/1 | ReadyForReview |
| memory/current-api-version | 1/1 | 1/1 | ReadyForReview |
| memory/database-name | 1/1 | 1/1 | ReadyForReview |
| memory/deploy-procedure | 1/1 | 1/1 | ReadyForReview |
| memory/flaky-upload | 1/1 | 1/1 | ReadyForReview |
| memory/propose-lint-pitfall | 1/1 | 1/1 | ReadyForReview |
| memory/propose-port-convention | 1/1 | 1/1 | ReadyForReview |
| memory/release-owner | 1/1 | 1/1 | ReadyForReview |
| memory/staging-port | 1/1 | 1/1 | ReadyForReview |
| memory/timeout-unit | 1/1 | 1/1 | ReadyForReview |
| skill_use/adr | 1/1 | 1/1 | ReadyForReview |
| skill_use/branch-name | 1/1 | 1/1 | ReadyForReview |
| skill_use/changelog-section | 1/1 | 1/1 | ReadyForReview |
| skill_use/ini-order | 1/1 | 1/1 | ReadyForReview |
| skill_use/license-header | 1/1 | 1/1 | ReadyForReview |
| skill_use/metric-name | 1/1 | 1/1 | ReadyForReview |
| skill_use/release-note | 1/1 | 1/1 | ReadyForReview |
| skill_use/status-code | 1/1 | 1/1 | ReadyForReview |
| skill_use/todo-format | 1/1 | 1/1 | ReadyForReview |
| skill_use/version-bump | 1/1 | 1/1 | ReadyForReview |
| subagent/api-doc | 1/1 | 1/1 | ReadyForReview |
| subagent/feature-flags | 1/1 | 1/1 | ReadyForReview |
| subagent/fixture-json | 1/1 | 1/1 | ReadyForReview |
| subagent/modules | 1/1 | 1/1 | ReadyForReview |
| subagent/monthly-notes | 1/1 | 1/1 | ReadyForReview |
| subagent/package-versions | 1/1 | 1/1 | ReadyForReview |
| subagent/report | 1/1 | 1/1 | ReadyForReview |
| subagent/setup-notes | 1/1 | 1/1 | ReadyForReview |
| subagent/translations | 1/1 | 1/1 | ReadyForReview |
| subagent/two-parts | 1/1 | 1/1 | ReadyForReview |
| tool_use/american-spelling | 1/1 | 1/1 | ReadyForReview |
| tool_use/batch-create | 1/1 | 1/1 | ReadyForReview |
| tool_use/bump-retries | 1/1 | 1/1 | ReadyForReview |
| tool_use/count-assets | 1/1 | 1/1 | ReadyForReview |
| tool_use/find-channel | 1/1 | 1/1 | ReadyForReview |
| tool_use/fix-typo | 1/1 | 1/1 | ReadyForReview |
| tool_use/last-commit-subject | 1/1 | 1/1 | ReadyForReview |
| tool_use/list-cfg-files | 1/1 | 1/1 | ReadyForReview |
| tool_use/sorted-names | 1/1 | 1/1 | ReadyForReview |
| tool_use/sum-numbers | 1/1 | 1/1 | ReadyForReview |

## Four axes (PX-184)

- bundle digest: `e4e380fac17dc75ee444cc5f02fa7ea7c6aa14b9d1c46afcfdd68a70700b5e3b`
- gate: an axis fails when its verified-success rate is more than 0.05 below the baseline's
- output tokens per thousand tasks: 218400 (8736 output tokens over 40 trials, 273 model calls); 4.2673 USD per thousand tasks at catalog list prices
- source: per trial: GetTaskEconomics of the task and every child it admitted, equal to the ModelUsageRecorded events of the trial's log

| axis | tasks | verified | 95% interval | output tokens / 1000 tasks |
|---|---|---|---|---|
| tool_use | 10 | 10/10 = 1.000 | 0.722–1.000 | 124800 |
| memory | 10 | 10/10 = 1.000 | 0.722–1.000 | 128000 |
| skill_use | 10 | 10/10 = 1.000 | 0.722–1.000 | 140800 |
| subagent | 10 | 10/10 = 1.000 | 0.722–1.000 | 480000 |

