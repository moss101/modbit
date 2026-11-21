# live-core run 43302343430

- workflow run: https://github.com/moss101/modbit/actions/runs/43302343430 (`live-core`, which=px109,px041,px059,px061,px100,px101,px116, cap 6 USD to the Core legs, branch `wave6/live-core` head 67cfa8fe with main 0b2ffb02 merged in)
- model: gemini-3.8-flash (Google, OpenAI-compatible endpoint; tool calls carry Gemini thought signatures, PR #203)
- endpoint: https://generativelanguage.googleapis.com/v1beta/openai
- price used for the cap: 0.75/3.75 USD per million input/output tokens (Google's introductory Gemini 3.8 Flash rate through 2026-12-31)
- spend of the legs retained here: 0.1643 USD

Every record below passed: every named check ok, live, not stopped by the cap.

| row | leg | passed | model calls | tokens in/out | cost USD | elapsed s |
|---|---|---|---|---|---|---|
| px041 | kill | True | 2 | 8944/6345 | 0.0305 | 68 |
| px041 | stream | True | 1 | 6581/492 | 0.0068 | 31 |
| px061 | checkpoints | True | 5 | 41787/289 | 0.0324 | 21 |
| px100 | cli | True | 11 | 65402/431 | 0.0507 | 61 |
| px101 | pause | True | 6 | 54659/772 | 0.0439 | 26 |

A passing live leg is one part of its QUAL; it does not by itself close the row (see PR #96).

Not retained (did not pass in this run; the dispatch spent 4.8624 USD of a 6 USD cap, 4.62 of it on the two px109 legs):

- px059/accounting: failed estimator_within_declared_error. The tokens-v2 estimator counted 12043 against Gemini's 9578, 2573 bp off against the 2500 bp the Core declares. A Core finding, not a test defect; the bound is unchanged.
- px109/compaction and px109/fallback: failed the_run_compacted. The chain of parts peaks near 189k tokens and Gemini's catalog window is 1M, so the run never reached its compaction budget. The legs now declare a window of at most 200k for the live model.
- px116/budgets: failed three_children_admitted. The cost figures were sized for a 0.15/0.50 model; at Gemini's price the parent's own calls used its 400 cap before the second spawn. The figures now scale by the model's price ratio.
