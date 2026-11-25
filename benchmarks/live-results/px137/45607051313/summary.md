# PX-137 live retrieval evaluation

Model `gemini-3.8-flash`; repository https://github.com/BurntSushi/ripgrep.git @ `4649aa9700619f94cf9c66876e9549d83420e16c`; 77 of 77 cases scored under both profiles.

Spend: $3.9097 of the $4.00 cap over 1258 model calls (5029640 input / 36660 output tokens).

Status: COMPLETE.

| profile | accuracy | input tokens / case | output tokens / case | tool calls / case | wall ms / case |
|---|---|---|---|---|---|
| baseline (rg + read_file) | 75/77 = 0.974 (Wilson 95% [0.910, 0.993]) | 35117 [29321, 41005] | 247 [224, 270] | 7.40 [6.65, 8.14] | 25245 [22164, 28395] |
| treatment (+ retrieve) | 74/77 = 0.961 (Wilson 95% [0.892, 0.987]) | 30203 [25289, 35117] | 229 [207, 253] | 6.94 [6.17, 7.66] | 24498 [21236, 28208] |

Treatment called `retrieve` in 10 of 77 cases (offered, never forced). Cases ended by a cap: baseline 19, treatment 17.

Treatment minus baseline, per case:

| measure | better | wins | losses | ties | mean delta | 95% bootstrap |
|---|---|---|---|---|---|---|
| accuracy | higher | 1 | 2 | 74 | -0.013 | [-0.052, 0.026] |
| input_tokens | lower | 43 | 34 | 0 | -4913.740 | [-8395.779, -1730.857] |
| output_tokens | lower | 46 | 29 | 2 | -17.377 | [-29.948, -5.857] |
| tool_calls | lower | 28 | 18 | 31 | -0.468 | [-0.857, -0.104] |
| wall_ms | lower | 46 | 31 | 0 | -746.779 | [-3265.169, 1825.325] |

Index times at 101 MB: the pinned checkout (212 files, 3025946 bytes) plus 6285 deterministic synthetic padding files (102973440 bytes) to reach the target; the padding is not repository content. Cold start 403095 ms (exact 3788, BM25 2605, symbols 14202, semantic 369481, graph 13018); incremental refresh of one file 13.2 ms.

Report digest `4ada0e9693ef5d3b593e4fa58cbd91481a124035c28aeea4964d5907bf78f824` (reproduce with `retrieval-live --rescore <dir>`).

Same model, same questions, same pinned checkout for both profiles. BASELINE = the external baseline surface: a real ripgrep process behind `search` plus `read_file`. TREATMENT = those same two tools plus `retrieve` (the Modbit retrieval planner, structural profile, in-process); it is OFFERED, never forced: the transcripts record forced=false on every call and the validator rejects any other value, and the report counts the cases in which the model chose to call it. Correctness is the oracle's, never a model's: every labelled file must be named in the final JSON answer, with at most two extra paths. Accuracy intervals are Wilson; token, tool-call and time means carry deterministic bootstrap intervals; the paired rows are treatment minus baseline per case. Decoding is at temperature 0 but a hosted model is not bit-reproducible, so one run is one sample; the rescore reproduces the report from the retained transcripts, not from the model. Wall times include the gateway's latency and are not comparable across machines. Index times are for the corpus named in the index block, which says how much of it is synthetic padding.
