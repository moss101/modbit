# PX-137 live retrieval evaluation

Model `glm-5.3-flash`; repository https://github.com/BurntSushi/ripgrep.git @ `4649aa9700619f94cf9c66876e9549d83420e16c`; 77 of 77 cases scored under both profiles.

Spend: $0.2810 of the $4.00 cap over 662 model calls (1684145 input / 56819 output tokens).

Status: COMPLETE.

| profile | accuracy | input tokens / case | output tokens / case | tool calls / case | wall ms / case |
|---|---|---|---|---|---|
| baseline (rg + read_file) | 73/77 = 0.948 (Wilson 95% [0.874, 0.980]) | 10291 [7911, 13408] | 381 [316, 459] | 4.88 [4.38, 5.45] | 20461 [17827, 23368] |
| treatment (+ retrieve) | 73/77 = 0.948 (Wilson 95% [0.874, 0.980]) | 11581 [8602, 15100] | 357 [304, 419] | 4.86 [4.22, 5.56] | 19962 [17449, 22717] |

Treatment called `retrieve` in 65 of 77 cases (offered, never forced). Cases ended by a cap: baseline 0, treatment 2.

Treatment minus baseline, per case:

| measure | better | wins | losses | ties | mean delta | 95% bootstrap |
|---|---|---|---|---|---|---|
| accuracy | higher | 1 | 1 | 75 | 0.000 | [-0.039, 0.039] |
| input_tokens | lower | 30 | 47 | 0 | 1289.442 | [-508.260, 3258.169] |
| output_tokens | lower | 34 | 43 | 0 | -23.727 | [-78.831, 29.078] |
| tool_calls | lower | 27 | 29 | 21 | -0.026 | [-0.545, 0.519] |
| wall_ms | lower | 39 | 38 | 0 | -499.429 | [-2829.753, 1889.403] |

Index times at 101 MB: the pinned checkout (212 files, 3025946 bytes) plus 6285 deterministic synthetic padding files (102973440 bytes) to reach the target; the padding is not repository content. Cold start 439464 ms (exact 5338, BM25 3236, symbols 19647, semantic 393904, graph 17338); incremental refresh of one file 15.0 ms.

Report digest `f1ac05699374dcc6a5da03a39dc45571f89ef00c79b75be6034831f7e53af013` (reproduce with `retrieval-live --rescore <dir>`).

Same model, same questions, same pinned checkout for both profiles. BASELINE = the external baseline surface: a real ripgrep process behind `search` plus `read_file`. TREATMENT = those same two tools plus `retrieve` (the Modbit retrieval planner, structural profile, in-process); it is OFFERED, never forced: the transcripts record forced=false on every call and the validator rejects any other value, and the report counts the cases in which the model chose to call it. Correctness is the oracle's, never a model's: every labelled file must be named in the final JSON answer, with at most two extra paths. Accuracy intervals are Wilson; token, tool-call and time means carry deterministic bootstrap intervals; the paired rows are treatment minus baseline per case. Decoding is at temperature 0 but a hosted model is not bit-reproducible, so one run is one sample; the rescore reproduces the report from the retained transcripts, not from the model. Wall times include the gateway's latency and are not comparable across machines. Index times are for the corpus named in the index block, which says how much of it is synthetic padding.
