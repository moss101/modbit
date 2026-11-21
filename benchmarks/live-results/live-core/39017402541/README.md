# live-core run 39017402541

- workflow run: https://github.com/moss101/modbit/actions/runs/39017402541 (`live-core`, which=px041,px050,px059,px061, cap 2 USD, branch head 51ef8521 of 2026-10-17, before main was merged in)
- model: glm-5.3-flash (a gateway model, not a first-party provider; DR-M9-002)
- endpoint: https://api.z.ai/api/coding/paas/v4
- spend of the whole dispatch, from the gateway's reported usage: 0.0208 USD

Retained here: the record of every leg that passed (every named check ok, live, not stopped by the cap).

| row | leg | passed | model calls | tokens in/out | cost USD | elapsed s |
|---|---|---|---|---|---|---|
| px041 | kill | False | 0 | 0/0 | 0.0000 | 120 |
| px041 | stream | True | 1 | 6587/1496 | 0.0017 | 40 |
| px050 | interrupt | True | 4 | 19811/3246 | 0.0046 | 193 |
| px050 | kill | False | 0 | 0/0 | 0.0000 | 321 |
| px050 | queue | False | 0 | 0/0 | 0.0000 | 120 |
| px059 | accounting | False | 0 | 0/0 | 0.0000 | 57 |
| px061 | checkpoints | True | 7 | 59699/11056 | 0.0145 | 234 |

Not retained (did not pass in this run):

- px041/kill: failed the leg ended before it finished (a panic or an early return); see the job log
- px050/kill: failed run_ended_after_resume
- px050/queue: failed the leg ended before it finished (a panic or an early return); see the job log
- px059/accounting: failed window_from_the_catalog

A passing live leg is one part of its QUAL; it does not by itself close the row (see PR #96).

Caveat: these records predate the merge of main into wave6/live-core (2026-11-07) and were superseded by run 39020727754 the same day.
