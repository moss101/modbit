# Competence baseline — internal-competence v1.0.0

- task list digest: `a6cdf39a8dac3294b97cc61d8497dbbe5923b2129e185a727d75892c2255ac0c`
- bundle digest: `ab250217ec1954ff19b5239fd39487ac9419a37298bc02ff20db4c619ca3e395`
- protocol: 3 trials/task, max_turns 24, openai `glm-5.3-flash` at api.z.ai (direct)
- Modbit revision: `1507707cab9a328939bacb57b260f967184ab84c`; build digest `fbf23a98251f`; harness competence-baseline/1

| metric | value | 95% interval |
|---|---|---|
| verified success | 3/3 = 1.000 | 0.439–1.000 |
| first-pass success (zero RepairAttempts) | 2/3 = 0.667 | 0.208–0.939 |
| first-candidate success (at most one repair attempt, no escalation) | 3/3 = 1.000 | 0.439–1.000 |
| repair loops (attempts→trials) | {0: 2, 1: 1} | — |
| equivalent-hypothesis escalations | 0 (0.00/trial) | — |
| no-progress escalations | 0 (0.00/trial) | — |
| wrong-effect attempts blocked | 0 (0.00/trial) | — |
| evidence coverage | 3/3 = 1.000 | 0.439–1.000 |
| cost | total 0.1120 USD; per verified success Some(0.03732133333333334) | — |
| edits without retrieval record | 0 | must be 0 |
| files outside original plan / expansions / questions per trial | 0.00 / 2.67 / 0.00 | — |
| regressions attributed | 0 | — |
| flaky quarantines | 0 (0.00/trial) | — |
| DI-3 violations | 0 | — |

| task | verified | first-pass | states |
|---|---|---|---|
| python-service/discount | 3/3 | 2/3 | ReadyForReview, ReadyForReview, ReadyForReview |
