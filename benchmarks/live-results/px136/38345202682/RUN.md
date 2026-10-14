# PX-136 live run

- workflow run: https://github.com/moss101/modbit/actions/runs/38345202682 (`live-evals`, `which=px136`, cap 7 USD, run from main fb302c96)
- endpoints that answered: opener `openai/glm-5.3-flash` on the z.ai OpenAI-compatible endpoint (`MODBIT_OPENAI_BASE_URL`, https://api.z.ai/api/coding/paas/v4); second binding `anthropic/glm-5.3-flash` on the z.ai Anthropic-compatible endpoint (`MODBIT_ANTHROPIC_BASE_URL`, https://api.z.ai/api/anthropic). Both bindings are the SAME model (glm-5.3-flash, 0.15/0.50 USD per Mtok) on a compatible gateway (DR-M9-002), not a first-party provider.
- 30 pinned tasks x 3 arms = 90 trials, all complete; spend 1.7992 USD
- retained: result.json (every run record), status.json, summary.md, run.log; the per-trial CLI logs and event logs are in artifact `live-evals-px136` of that run
