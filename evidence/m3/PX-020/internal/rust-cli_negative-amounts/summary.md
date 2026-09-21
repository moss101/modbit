# Competence baseline — internal-competence v1.0.0

- task list digest: `a6cdf39a8dac3294b97cc61d8497dbbe5923b2129e185a727d75892c2255ac0c`
- bundle digest: `b03fee5c86b9a489a7c80ca68ed4c9fabd6b215817148a97344b461b743c8dde`
- protocol: 3 trials/task, max_turns 24, openai `glm-5.3-flash` at api.z.ai (direct)
- Modbit revision: `1507707cab9a328939bacb57b260f967184ab84c`; build digest `fbf23a98251f`; harness competence-baseline/1

| metric | value | 95% interval |
|---|---|---|
| verified success | 0/3 = 0.000 | 0.000–0.561 |
| first-pass success (zero RepairAttempts) | 0/3 = 0.000 | 0.000–0.561 |
| first-candidate success (at most one repair attempt, no escalation) | 0/3 = 0.000 | 0.000–0.561 |
| repair loops (attempts→trials) | {0: 3} | — |
| equivalent-hypothesis escalations | 0 (0.00/trial) | — |
| no-progress escalations | 2 (0.67/trial) | — |
| wrong-effect attempts blocked | 0 (0.00/trial) | — |
| evidence coverage | 0/0 = 0.000 | 0.000–1.000 |
| cost | total 0.1933 USD; per verified success None | — |
| edits without retrieval record | 0 | must be 0 |
| files outside original plan / expansions / questions per trial | 0.00 / 2.00 / 0.00 | — |
| regressions attributed | 0 | — |
| flaky quarantines | 3 (1.00/trial) | — |
| DI-3 violations | 0 | — |

| task | verified | first-pass | states |
|---|---|---|---|
| rust-cli/negative-amounts | 0/3 | 0/3 | Waiting, Waiting, Waiting |
