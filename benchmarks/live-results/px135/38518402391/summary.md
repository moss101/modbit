# Gate calibration, live candidates (px135) - INCOMPLETE

- model `glm-5.3-flash` via `openai` (api.z.ai); suite `gate-calibration-live/1`
- runs scored: 16 of 16 required (1 repeats per lineage)
- spend: $0.3073 of $5.00 cap
- report digest: `aef6cf6e036605f80783cbbe4e82ae2c62183fe200a1f40e688ceaac127871d8` (reproduced on rescoring: true)
- corpus digest: `d5ebc80989bcfbb5e608943ecc54a1fc7d82005cd419cde45e1b77a86180a23b`
- oracle leaks: 0

Rates are on the live candidates, Wilson 95% intervals, `count/n = rate [lower, upper]`.

| language | candidates | false accept: previous | false accept: widened | false reject: previous | false reject: widened | risk false negative |
|---|---|---|---|---|---|---|
| rust | 4 (4 correct, 0 incorrect) | 0/0 = 0.000 [0.000, 1.000] | 0/0 = 0.000 [0.000, 1.000] | 0/4 = 0.000 [0.000, 0.490] | 1/4 = 0.250 [0.046, 0.699] | 0/0 = 0.000 [0.000, 1.000] |
| python | 4 (4 correct, 0 incorrect) | 0/0 = 0.000 [0.000, 1.000] | 0/0 = 0.000 [0.000, 1.000] | 0/4 = 0.000 [0.000, 0.490] | 1/4 = 0.250 [0.046, 0.699] | 0/0 = 0.000 [0.000, 1.000] |
| javascript | 4 (2 correct, 2 incorrect) | 2/2 = 1.000 [0.342, 1.000] | 2/2 = 1.000 [0.342, 1.000] | 0/2 = 0.000 [0.000, 0.658] | 1/2 = 0.500 [0.095, 0.905] | 0/0 = 0.000 [0.000, 1.000] |
| typescript | 4 (4 correct, 0 incorrect) | 0/0 = 0.000 [0.000, 1.000] | 0/0 = 0.000 [0.000, 1.000] | 0/4 = 0.000 [0.000, 0.490] | 1/4 = 0.250 [0.046, 0.699] | 0/0 = 0.000 [0.000, 1.000] |
| all | 16 (14 correct, 2 incorrect) | 2/2 = 1.000 [0.342, 1.000] | 2/2 = 1.000 [0.342, 1.000] | 0/14 = 0.000 [0.000, 0.215] | 4/14 = 0.286 [0.117, 0.546] | 0/0 = 0.000 [0.000, 1.000] |

Previous gate against widened gate:

| language | incorrect | accepted previous | accepted widened | newly rejected | correct | not accepted previous | not accepted widened | newly rejected (correct) |
|---|---|---|---|---|---|---|---|---|
| rust | 0 | 0 | 0 | 0 | 4 | 0 | 1 | 1 |
| python | 0 | 0 | 0 | 0 | 4 | 0 | 1 | 1 |
| javascript | 2 | 2 | 2 | 0 | 2 | 0 | 1 | 1 |
| typescript | 0 | 0 | 0 | 0 | 4 | 0 | 1 | 1 |
| all | 2 | 2 | 2 | 0 | 14 | 0 | 4 | 4 |

Runs:

| run | task state | oracle | previous | widened | cost USD | model calls |
|---|---|---|---|---|---|---|
| rust-quantity-0 | Waiting | correct | ACCEPT | REJECT | 0.036634 | 12 |
| rust-discount-0 | Cancelled | correct | ACCEPT | ACCEPT | unknown | 18 |
| rust-sku-0 | ReadyForReview | correct | ACCEPT | ACCEPT | 0.009046 | 4 |
| rust-shipping-0 | Cancelled | correct | ACCEPT | ACCEPT | unknown | 17 |
| python-quantity-0 | ReadyForReview | correct | ACCEPT | ACCEPT | 0.038448 | 12 |
| python-discount-0 | Waiting | correct | ACCEPT | REJECT | 0.043998 | 13 |
| python-sku-0 | ReadyForReview | correct | ACCEPT | ACCEPT | 0.026879 | 10 |
| python-shipping-0 | ReadyForReview | correct | ACCEPT | ACCEPT | 0.027332 | 12 |
| javascript-quantity-0 | Cancelled | incorrect | ACCEPT | ACCEPT | unknown | 14 |
| javascript-discount-0 | Cancelled | correct | ACCEPT | REJECT | unknown | 15 |
| javascript-sku-0 | ReadyForReview | correct | ACCEPT | ACCEPT | 0.046148 | 14 |
| javascript-shipping-0 | Cancelled | incorrect | ACCEPT | ACCEPT | unknown | 13 |
| typescript-quantity-0 | ReadyForReview | correct | ACCEPT | ACCEPT | 0.012717 | 6 |
| typescript-discount-0 | Waiting | correct | ACCEPT | REJECT | 0.016201 | 6 |
| typescript-sku-0 | ReadyForReview | correct | ACCEPT | ACCEPT | 0.011986 | 5 |
| typescript-shipping-0 | ReadyForReview | correct | ACCEPT | ACCEPT | 0.037887 | 13 |

MISSING (this result is not a pass):

- run rust-discount-0: cost unknown (the model is not in the Core's price catalog; an unknown price is not free)
- run rust-shipping-0: cost unknown (the model is not in the Core's price catalog; an unknown price is not free)
- run javascript-quantity-0: cost unknown (the model is not in the Core's price catalog; an unknown price is not free)
- run javascript-discount-0: cost unknown (the model is not in the Core's price catalog; an unknown price is not free)
- run javascript-shipping-0: cost unknown (the model is not in the Core's price catalog; an unknown price is not free)
