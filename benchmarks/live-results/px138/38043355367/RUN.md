# PX-138 live run

- workflow run: https://github.com/moss101/modbit/actions/runs/38043355367 (`live-evals`, `which=px138`, cap 1 USD)
- commit: 4f0491f6cf6754cdbaf392549ca741d947973304 (branch `wave6/live-evals`)
- endpoint that answered: the OpenAI-compatible family of the z.ai gateway, `https://api.z.ai/api/coding/paas/v4` (repository variable `MODBIT_OPENAI_BASE_URL`); model `glm-5.3-flash`, list price 0.15 / 0.50 USD per million tokens (`MODBIT_OPENAI_MODELS`). This is a compatible gateway recorded as such under DR-M9-002, not a first-party provider.
- retained artifact: `live-evals-px138` of that run (the raw request/reply exchanges, `exchanges/`, are in the artifact only; `compaction-eval-live --rescore <dir>` reproduces the digest from them)
- result: see `summary.md`; `status.json`: complete, 192 calls, 0.1004 USD spent
