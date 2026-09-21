# Competence baseline — internal-competence v1.0.0

- task list digest: `a6cdf39a8dac3294b97cc61d8497dbbe5923b2129e185a727d75892c2255ac0c`
- bundle digest: `603d2dccf201cf05e261fe739bab6abf695887160d3c9387260a5dbb1d15ab47`
- protocol: 3 trials/task, max_turns 24, openai `glm-5.3-flash` at api.z.ai (direct)
- Modbit revision: `1507707cab9a328939bacb57b260f967184ab84c`; build digest `fbf23a98251f`; harness competence-baseline/1

| metric | value | 95% interval |
|---|---|---|
| verified success | 3/3 = 1.000 | 0.439–1.000 |
| first-pass success (zero RepairAttempts) | 2/3 = 0.667 | 0.208–0.939 |
| first-candidate success (at most one repair attempt, no escalation) | 3/3 = 1.000 | 0.439–1.000 |
| repair loops (attempts→trials) | {0: 2, 1: 1} | — |
| equivalent-hypothesis escalations | 0 (0.00/trial) | — |
| no-progress escalations | 2 (0.67/trial) | — |
| wrong-effect attempts blocked | 0 (0.00/trial) | — |
| evidence coverage | 3/3 = 1.000 | 0.439–1.000 |
| cost | total 0.0928 USD; per verified success Some(0.030920333333333338) | — |
| edits without retrieval record | 0 | must be 0 |
| files outside original plan / expansions / questions per trial | 0.00 / 2.33 / 0.00 | — |
| regressions attributed | 0 | — |
| flaky quarantines | 3 (1.00/trial) | — |
| DI-3 violations | 0 | — |

| task | verified | first-pass | states |
|---|---|---|---|
| ts-webapp/reject-negative | 3/3 | 2/3 | ReadyForReview, ReadyForReview, ReadyForReview |
