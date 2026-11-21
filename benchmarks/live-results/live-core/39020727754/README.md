# live-core run 39020727754

- workflow run: https://github.com/moss101/modbit/actions/runs/39020727754 (`live-core`, which=all, cap 12 USD (4.80 to the Core legs, 7.20 to PX-114, whose job timed out at 355 min with no report), branch head b8c7f2cd of 2026-10-17, before main was merged in)
- model: glm-5.3-flash (a gateway model, not a first-party provider; DR-M9-002)
- endpoint: https://api.z.ai/api/coding/paas/v4
- spend of the whole dispatch, from the gateway's reported usage: 0.3771 USD

Retained here: the record of every leg that passed (every named check ok, live, not stopped by the cap).

| row | leg | passed | model calls | tokens in/out | cost USD | elapsed s |
|---|---|---|---|---|---|---|
| px041 | kill | True | 2 | 8962/9002 | 0.0058 | 294 |
| px041 | stream | True | 1 | 6587/1288 | 0.0016 | 34 |
| px050 | interrupt | True | 3 | 24186/9470 | 0.0084 | 251 |
| px050 | kill | False | 2 | 15208/17935 | 0.0112 | 344 |
| px050 | queue | True | 8 | 69628/7945 | 0.0144 | 188 |
| px051 | modes | False | 3 | 17727/8314 | 0.0068 | 241 |
| px057 | allowlist | True | 30 | 216639/29336 | 0.0472 | 663 |
| px059 | accounting | True | 9 | 74406/3567 | 0.0129 | 110 |
| px061 | checkpoints | True | 6 | 50659/11114 | 0.0132 | 226 |
| px084 | unattended | False | 0 | 0/0 | 0.0000 | 166 |
| px100 | cli | True | 10 | 57925/7973 | 0.0127 | 187 |
| px101 | pause | True | 8 | 75715/5953 | 0.0143 | 133 |
| px109 | compaction | False | 4 | 176586/4674 | 0.0288 | 93 |
| px109 | fallback | True | 3 | 175739/2365 | 0.0275 | 59 |
| px116 | budgets | True | 92 | 623354/43020 | 0.1150 | 466 |
| px118 | worktree | False | 25 | 281078/29924 | 0.0571 | 648 |

Not retained (did not pass in this run):

- px050/kill: failed run_ended_after_resume
- px051/modes: failed ask_run_ended
- px084/unattended: failed the_model_insisted_and_was_refused
- px109/compaction: failed model_summary_installed
- px118/worktree: failed second_run_ended

A passing live leg is one part of its QUAL; it does not by itself close the row (see PR #96).

Caveats: these records predate the merge of main into wave6/live-core (2026-11-07), so they prove the legs against the Core of 2026-10-17, not the current one; the px084 record shows 0 USD although its automation runs called the model (automation sessions were not yet summed into a leg's usage, fixed in f8f8c5b6), so the total above understates that dispatch's spend. A re-run on the merged head (42402648319, 2026-11-07) got HTTP 401 from the gateway on every call.
