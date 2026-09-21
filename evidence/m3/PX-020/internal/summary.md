# Competence baseline — internal-competence v1.0.0

- task list digest: `a6cdf39a8dac3294b97cc61d8497dbbe5923b2129e185a727d75892c2255ac0c`
- bundle digest: `59301bb8f2699794b3e26d1699db94e5689aabd991c3b168f26801b7d1a7b786`
- protocol: 3 trials/task, max_turns 24, openai `glm-5.3-flash` at api.z.ai (direct)
- Modbit revision: `1507707cab9a328939bacb57b260f967184ab84c`; build digest `fbf23a98251f`; harness competence-baseline/1

| metric | value | 95% interval |
|---|---|---|
| verified success | 12/18 = 0.667 | 0.437–0.837 |
| first-pass success (zero RepairAttempts) | 10/18 = 0.556 | 0.337–0.754 |
| first-candidate success (at most one repair attempt, no escalation) | 12/18 = 0.667 | 0.437–0.837 |
| repair loops (attempts→trials) | {0: 16, 1: 2} | — |
| equivalent-hypothesis escalations | 0 (0.00/trial) | — |
| no-progress escalations | 4 (0.22/trial) | — |
| wrong-effect attempts blocked | 0 (0.00/trial) | — |
| evidence coverage | 12/12 = 1.000 | 0.758–1.000 |
| cost | total 0.7040 USD; per verified success Some(0.0586645) | — |
| edits without retrieval record | 0 | must be 0 |
| files outside original plan / expansions / questions per trial | 0.06 / 2.11 / 0.00 | — |
| regressions attributed | 0 | — |
| flaky quarantines | 12 (0.67/trial) | — |
| DI-3 violations | 1 | — |

| task | verified | first-pass | states |
|---|---|---|---|
| python-service/discount | 3/3 | 2/3 | ReadyForReview, ReadyForReview, ReadyForReview |
| python-service/reject-zero | 3/3 | 3/3 | ReadyForReview, ReadyForReview, ReadyForReview |
| rust-cli/negative-amounts | 0/3 | 0/3 | Waiting, Waiting, Waiting |
| rust-cli/reject-negative | 2/3 | 2/3 | ReadyForReview, Waiting, ReadyForReview |
| ts-webapp/parse-money | 1/3 | 1/3 | ReadyForReview, Waiting, Waiting |
| ts-webapp/reject-negative | 3/3 | 2/3 | ReadyForReview, ReadyForReview, ReadyForReview |
