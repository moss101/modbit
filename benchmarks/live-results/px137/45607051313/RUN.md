# PX-137 live run (after the fusion fix, on the provider gateway)

- Workflow run: https://github.com/moss101/modbit/actions/runs/45607051313 (`live-evals`, `which=px137`, cap 4 USD, status COMPLETE).
- Commit: fdb2c9ce on branch `fix/live-bench-provider-data`. That is `main` at c605c2e3, which includes the PX-137 fusion fix (#253), plus the bench chat client fix in this branch: it replays each tool call's `provider_data` (Gemini's `thought_signature`).
- Endpoint: Google's Gemini OpenAI-compatible endpoint (`https://generativelanguage.googleapis.com/v1beta/openai`, repository variable `MODBIT_OPENAI_BASE_URL`), model `gemini-3.8-flash`, 0.75 / 3.75 USD per million tokens. It is a compatible gateway and is recorded as one (DR-M9-002), not as a first-party provider. The harness reached it through the product's provider gateway, not through the direct HTTP client of the October run.
- Retained here: `result.json`, `status.json`, `summary.md` and `run.log`. The transcripts (77 cases x 2 profiles, 4.8 MB) are in the `live-evals-px137` artifact of the run only; `retrieval-live --rescore <dir>` reproduces digest `4ada0e96…f824` from them.
- Repository: BurntSushi/ripgrep 14.1.1 at commit 4649aa9700619f94cf9c66876e9549d83420e16c; 77 hand-labelled cases. These are the same cases as run 38044424486.

## Result (one sample; see summary.md)

| profile | accuracy | input tokens / case | tool calls / case |
|---|---|---|---|
| baseline (rg + read_file) | 75/77 = 0.974 [0.910, 0.993] | 35117 | 7.40 |
| treatment (+ retrieve) | 74/77 = 0.961 [0.892, 0.987] | 30203 | 6.94 |

Paired, treatment minus baseline:

| measure | mean delta | 95% CI | significant? |
|---|---|---|---|
| accuracy | -0.013 | [-0.052, 0.026] | no |
| input tokens per case | -4914 | [-8396, -1731] | yes (fewer) |
| tool calls per case | -0.47 | [-0.86, -0.10] | yes (fewer) |
| output tokens per case | -17 | [-30, -6] | yes (fewer) |
| wall time | -747 ms | [-3265, 1825] | no |

The model chose `retrieve` in only 10 of 77 cases. That makes this a weak test of the planner's ranking itself; most treatment cases ran the same tools as baseline.

Cases ended by a per-case cap: baseline 19, treatment 17. These are the harness's round and token limits per case, not the spend cap; the spend cap was never hit.

Spend: $3.91 of the $4.00 cap over 1258 calls (5.03M input / 36.7k output tokens). A rerun on this model needs a cap of at least $5.

Index at 101 MB (the checkout plus 6285 synthetic padding files): cold start 403 s, of which semantic took 369 s; an incremental refresh of one file took 13.2 ms.

## Not comparable with run 38044424486

- That run used a different model (`glm-5.3-flash` on z.ai) through a different client.
- That run reached the same accuracy in both profiles (73/77) and showed no significant token difference.
- Differences between the two runs are therefore not attributable to the fusion fix.

## Runs that did not score

- Run 45603660830 ran no cases. `MODBIT_LIVE_MODEL` still named `glm-5.3-flash`, which the Gemini catalog does not price, so the harness refused to start. It spent nothing.
- Run 45604451209 failed. Gemini returned HTTP 400 on the second turn of every case because the bench client dropped the `thought_signature`. That failure is the bug fixed by this branch. It spent $0.002.
