# live-core run 45132159870

- workflow run: https://github.com/moss101/modbit/actions/runs/45132159870 (`live-core`, which=px116, cap 1 USD, branch `wave6/live-core` head b40464c7: main 98180a4c merged in, plus the decoder fix for parallel tool calls without distinct indexes)
- model: gemini-3.8-flash (Google, OpenAI-compatible endpoint)
- endpoint: https://generativelanguage.googleapis.com/v1beta/openai
- spend: 0.6309 USD

The earlier dispatch 43304710511 failed this leg at `extra_spawn_refused_with_a_typed_reason` (admitted 0, no refusal): the model's two parallel `agent.spawn` calls arrived without distinct indexes, the decoder merged them into one invalid call (INVALID_ARGUMENTS), and the next request was a 400. With the fix every check passed.

| row | leg | passed | model calls | tokens in/out | cost USD | elapsed s |
|---|---|---|---|---|---|---|
| px116 | budgets | True | 108 | 803824/7468 | 0.6309 | 206 |

This is a pre-merge head; a row's closing evidence is its leg passing on main after this branch merges.
