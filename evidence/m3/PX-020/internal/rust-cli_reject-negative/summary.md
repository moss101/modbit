# Competence baseline — internal-competence v1.0.0

- task list digest: `a6cdf39a8dac3294b97cc61d8497dbbe5923b2129e185a727d75892c2255ac0c`
- bundle digest: `2ebc671a7ee0e36eab770e0e7265c610b246a5b0696d2107d2c73eea54aef40d`
- protocol: 3 trials/task, max_turns 24, openai `glm-5.3-flash` at api.z.ai (direct)
- Modbit revision: `1507707cab9a328939bacb57b260f967184ab84c`; build digest `fbf23a98251f`; harness competence-baseline/1

| metric | value | 95% interval |
|---|---|---|
| verified success | 2/3 = 0.667 | 0.208–0.939 |
| first-pass success (zero RepairAttempts) | 2/3 = 0.667 | 0.208–0.939 |
| first-candidate success (at most one repair attempt, no escalation) | 2/3 = 0.667 | 0.208–0.939 |
| repair loops (attempts→trials) | {0: 3} | — |
| equivalent-hypothesis escalations | 0 (0.00/trial) | — |
| no-progress escalations | 0 (0.00/trial) | — |
| wrong-effect attempts blocked | 0 (0.00/trial) | — |
| evidence coverage | 2/2 = 1.000 | 0.342–1.000 |
| cost | total 0.0806 USD; per verified success Some(0.0402865) | — |
| edits without retrieval record | 0 | must be 0 |
| files outside original plan / expansions / questions per trial | 0.00 / 1.33 / 0.00 | — |
| regressions attributed | 0 | — |
| flaky quarantines | 3 (1.00/trial) | — |
| DI-3 violations | 0 | — |

| task | verified | first-pass | states |
|---|---|---|---|
| rust-cli/reject-negative | 2/3 | 2/3 | ReadyForReview, Waiting, ReadyForReview |
