# PX-133 live run (INCOMPLETE)

- workflow run: https://github.com/moss101/modbit/actions/runs/38345205224 (`live-evals`, `which=px133`, cap 4 USD, from main fb302c96); the job FAILED by design: a required measurement is missing
- endpoints: as in the px136 run (same two z.ai endpoints, same model glm-5.3-flash on both)
- 60 trials (cascade + critique over the 30 pinned tasks); spend 1.1144 USD
- missing: zero Escalation samples (no cascade run ever activated its stronger-solver slot) and two cost reconciliations that failed on rounding. The reconciliation tolerance was too tight (the accounting rounds every attempt up); it was widened afterwards (harness bug, not a tuned result); the escalation gap is not a harness defect: see the report.
- reviewer samples: 32, all priced
