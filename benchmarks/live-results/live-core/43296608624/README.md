# live-core run 43296608624

- workflow run: https://github.com/moss101/modbit/actions/runs/43296608624 (`live-core`, which=px050,px051,px057,px084,px118, cap 4 USD to the Core legs, branch `wave6/live-core` head 67cfa8fe with main 0b2ffb02 merged in)
- model: gemini-3.8-flash (Google, OpenAI-compatible endpoint; tool calls carry Gemini thought signatures, PR #203)
- endpoint: https://generativelanguage.googleapis.com/v1beta/openai
- price used for the cap: 0.75/3.75 USD per million input/output tokens (Google's introductory Gemini 3.8 Flash rate through 2026-12-31)
- spend of the legs retained here: 1.4933 USD

Every record below passed: every named check ok, live, not stopped by the cap.

| row | leg | passed | model calls | tokens in/out | cost USD | elapsed s |
|---|---|---|---|---|---|---|
| px050 | interrupt | True | 3 | 26998/7171 | 0.0471 | 201 |
| px050 | kill | True | 9 | 74799/13410 | 0.1064 | 254 |
| px050 | queue | True | 8 | 73478/3058 | 0.0666 | 114 |
| px051 | modes | True | 34 | 365026/1861 | 0.2807 | 254 |
| px057 | allowlist | True | 31 | 234614/2413 | 0.1850 | 204 |
| px084 | unattended | True | 38 | 197825/1760 | 0.1550 | 300 |
| px118 | worktree | True | 49 | 853152/3368 | 0.6525 | 376 |

A passing live leg is one part of its QUAL; it does not by itself close the row (see PR #96).
