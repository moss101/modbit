# PX-135 live run (INCOMPLETE by one rule: five candidate runs have no priced cost)

- workflow run: https://github.com/moss101/modbit/actions/runs/38518402391 (`live-evals`, `which=px135`, cap 5 USD, branch wave6/live-evals-3, 1 repeat per lineage, wall-clock budget 335 min; it finished in 3.2 h, so the budget did not stop it)
- endpoint: the z.ai OpenAI-compatible endpoint (https://api.z.ai/api/coding/paas/v4), model `glm-5.3-flash` (0.15/0.50 USD per Mtok), through the real Core and modbit-cli. A compatible gateway (DR-M9-002), not a first-party provider.
- counts: 16 of 16 lineages (4 languages x 4) produced a candidate; 14 oracle-correct, 2 incorrect (javascript); 0 oracle leaks; report digest reproduces on rescoring
- cost: 0.3073 USD for the 11 runs with a priced Core economics view; five runs ended Cancelled (the 1200 s task timeout; their tree is the candidate) and the Core reports no price for a cancelled task, so their cost is listed as missing and the status is not complete. Their usage is in the retained events (`runs/*/events.jsonl` in the artifact), not summed here.
- earlier px135 attempts (38044422335, 38045866238, 38345341046: harness bugs and a gateway rate limit; 38470776165 and 38497257917: cancelled at the 240-minute job limit, nothing written) produced no result and are not counted as evidence; their spend is not recorded in any bundle.
