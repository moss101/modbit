# Competence baseline — internal-competence v1.0.0

- task list digest: `a6cdf39a8dac3294b97cc61d8497dbbe5923b2129e185a727d75892c2255ac0c`
- bundle digest: `3b859ea509d7979127188d9185aa9b9658ed0fd3ad3ebbb22d254fde7e34db97`
- protocol: 3 trials/task, max_turns 24, openai `glm-5.3-flash` at api.z.ai (direct)
- Modbit revision: `1507707cab9a328939bacb57b260f967184ab84c`; build digest `fbf23a98251f`; harness competence-baseline/1

| metric | value | 95% interval |
|---|---|---|
| verified success | 1/3 = 0.333 | 0.061–0.792 |
| first-pass success (zero RepairAttempts) | 1/3 = 0.333 | 0.061–0.792 |
| first-candidate success (at most one repair attempt, no escalation) | 1/3 = 0.333 | 0.061–0.792 |
| repair loops (attempts→trials) | {0: 3} | — |
| equivalent-hypothesis escalations | 0 (0.00/trial) | — |
| no-progress escalations | 0 (0.00/trial) | — |
| wrong-effect attempts blocked | 0 (0.00/trial) | — |
| evidence coverage | 1/1 = 1.000 | 0.207–1.000 |
| cost | total 0.1347 USD; per verified success Some(0.134713) | — |
| edits without retrieval record | 0 | must be 0 |
| files outside original plan / expansions / questions per trial | 0.33 / 1.67 / 0.00 | — |
| regressions attributed | 0 | — |
| flaky quarantines | 3 (1.00/trial) | — |
| DI-3 violations | 1 | — |

| task | verified | first-pass | states |
|---|---|---|---|
| ts-webapp/parse-money | 1/3 | 1/3 | ReadyForReview, Waiting, Waiting |
