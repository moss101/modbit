# PX-137 live run

- workflow run: https://github.com/moss101/modbit/actions/runs/38044424486 (`live-evals`, `which=px137`, cap 4 USD)
- commit: 4f0491f6cf6754cdbaf392549ca741d947973304 (branch `wave6/live-evals`)
- endpoint that answered: the OpenAI-compatible family of the z.ai gateway, `https://api.z.ai/api/coding/paas/v4` (repository variable `MODBIT_OPENAI_BASE_URL`); model `glm-5.3-flash`, 0.15 / 0.50 USD per million tokens. A compatible gateway recorded as such (DR-M9-002), not a first-party provider. Note: this run used the first chat client (a direct HTTP client); commit d33b5ebb moved the clients onto the product's provider gateway, so a later re-run exercises that path.
- retained artifact: `live-evals-px137` (transcripts for 77 cases x 2 profiles, 2.9 MB, are in the artifact only; `retrieval-live --rescore <dir>` reproduces the digest from them)
- repository: BurntSushi/ripgrep 14.1.1, commit 4649aa9700619f94cf9c66876e9549d83420e16c; 77 hand-labelled cases
