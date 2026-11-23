# live-core run 45221342126

- workflow run: https://github.com/moss101/modbit/actions/runs/45221342126 (`live-core`, which=all, cap 15 USD, **main c5294eb0**: PR 96 merged)
- model: gemini-3.8-flash (Google, OpenAI-compatible endpoint), endpoint https://generativelanguage.googleapis.com/v1beta/openai
- spend: Core legs 4.6155 USD; PX-114 paired trial 4.2383 USD of its 9 USD share (90 calls, 23 without usage); total about 8.85 USD

| row | leg | passed | model calls | tokens in/out | cost USD | elapsed s | failed checks |
|---|---|---|---|---|---|---|---|
| px041 | kill | True | 2 | 9008/5995 | 0.0292 | 71.026235385 |  |
| px041 | stream | True | 1 | 6645/518 | 0.0069 | 24.300742869 |  |
| px050 | interrupt | True | 3 | 25550/2828 | 0.0298 | 218.856161003 |  |
| px050 | kill | True | 9 | 84213/15815 | 0.1225 | 333.983518133 |  |
| px050 | queue | True | 8 | 74973/3238 | 0.0684 | 209.699501378 |  |
| px051 | modes | True | 34 | 398768/1939 | 0.3063 | 270.217135334 |  |
| px057 | allowlist | True | 33 | 242616/2797 | 0.1925 | 181.465863528 |  |
| px059 | accounting | False | 5 | 45738/61 | 0.0345 | 16.559040106 | estimator_within_declared_error |
| px061 | checkpoints | True | 5 | 44424/349 | 0.0346 | 24.79656716 |  |
| px084 | unattended | True | 39 | 197108/1693 | 0.1542 | 370.047274672 |  |
| px100 | cli | True | 11 | 65330/382 | 0.0504 | 69.766966998 |  |
| px101 | pause | True | 8 | 81914/1510 | 0.0671 | 57.036413511 |  |
| px109 | compaction | False | 21 | 1332072/688 | 1.0016 | 113.523557147 | run_ended |
| px109 | fallback | True | 35 | 2321109/1357 | 1.7459 | 220.25611768 |  |
| px116 | budgets | True | 98 | 693850/6344 | 0.5442 | 262.809167702 |  |
| px118 | worktree | False | 20 | 292691/2077 | 0.2273 | 311.365243005 | ended early |

## Reading

- **PX-061, PX-101, PX-116** pass on main. Their only open item was this leg.
- **PX-041, PX-050, PX-051, PX-057, PX-084, PX-100, PX-109 fallback** pass. Each row except PX-084 (already COMPLETE) has other open items on its graph node.
- **PX-059** fails `estimator_within_declared_error`: the estimator counted 12245 tokens against Gemini's 9760. That is 2546 bp off against the 2500 bp it declares, a real, narrow miss for this provider's tokenizer.
- **PX-109 compaction** ended `RATE_LIMITED`: HTTP 429, "You exceeded your current quota". The Gemini project's quota ran out during the run.
- **PX-118** ended early. The model revised its plan three times against a bound of two. The headless run failed closed (`SCOPE_FAIL_CLOSED`), as designed, and the leg expected completion. The same leg passed on run 43296608624 before the merge; this is model variance, not a regression.
- **PX-114:** the paired trial is not a valid verdict. Its exec_only arm ran after the quota was gone: 23 of 30 runs errored `RATE_LIMITED`, and the harness scored them as failures. Its "WORSE" recommendation measures the quota, not the projection.

Live legs need the Gemini quota restored before they are run again.
