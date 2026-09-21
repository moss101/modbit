# Competence baseline — internal-competence v1.0.0

- task list digest: `a6cdf39a8dac3294b97cc61d8497dbbe5923b2129e185a727d75892c2255ac0c`
- bundle digest: `53960653b9203952b514d09b965dab0a1c9db97df34ab6c974639b8b50273f12`
- protocol: 3 trials/task, max_turns 24, openai `glm-5.3-flash` at api.z.ai (direct)
- Modbit revision: `1507707cab9a328939bacb57b260f967184ab84c`; build digest `fbf23a98251f`; harness competence-baseline/1

| metric | value | 95% interval |
|---|---|---|
| verified success | 3/3 = 1.000 | 0.439–1.000 |
| first-pass success (zero RepairAttempts) | 3/3 = 1.000 | 0.439–1.000 |
| first-candidate success (at most one repair attempt, no escalation) | 3/3 = 1.000 | 0.439–1.000 |
| repair loops (attempts→trials) | {0: 3} | — |
| equivalent-hypothesis escalations | 0 (0.00/trial) | — |
| no-progress escalations | 0 (0.00/trial) | — |
| wrong-effect attempts blocked | 0 (0.00/trial) | — |
| evidence coverage | 3/3 = 1.000 | 0.439–1.000 |
| cost | total 0.0907 USD; per verified success Some(0.030223333333333335) | — |
| edits without retrieval record | 0 | must be 0 |
| files outside original plan / expansions / questions per trial | 0.00 / 2.67 / 0.00 | — |
| regressions attributed | 0 | — |
| flaky quarantines | 0 (0.00/trial) | — |
| DI-3 violations | 0 | — |

| task | verified | first-pass | states |
|---|---|---|---|
| python-service/reject-zero | 3/3 | 3/3 | ReadyForReview, ReadyForReview, ReadyForReview |
