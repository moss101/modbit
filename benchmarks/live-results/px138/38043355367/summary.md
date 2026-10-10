# PX-138 live compaction evaluation

Model `glm-5.3-flash`; 24 of 24 planned long runs scored; 240 probes per arm. Status: COMPLETE.

| arm | recall (95% Wilson) | unknown | unparsed | task success (95% Wilson) | context tokens saved (mean, 95% CI) | input tokens saved (reported) | cost per run USD |
|---|---|---|---|---|---|---|---|
| uncompacted | 205/240 = 0.854 [0.804, 0.893] | 0 | 0 | 24/24 = 1.000 [0.862, 1.000] | 0 [0, 0] | 0 [0, 0] | 0.00153 |
| extractive | 76/240 = 0.317 [0.261, 0.378] | 128 | 0 | 0/24 = 0.000 [0.000, 0.138] | 1851 [1763, 1946] | 4525 [4313, 4751] | 0.00102 |
| structured | 151/240 = 0.629 [0.566, 0.688] | 72 | 0 | 24/24 = 1.000 [0.862, 1.000] | 1444 [1356, 1538] | 3788 [3576, 4014] | 0.00085 |
| lossy_double | 97/240 = 0.404 [0.344, 0.467] | 120 | 0 | 24/24 = 1.000 [0.862, 1.000] | 1664 [1576, 1758] | 4216 [4004, 4442] | 0.00078 |

Paired differences against uncompacted (per run, mean and 95% bootstrap interval):

- extractive: recall -0.537 [-0.587, -0.475]; task success -1.000 [-1.000, -1.000]
- structured: recall -0.225 [-0.283, -0.158]; task success +0.000 [+0.000, +0.000]
- lossy_double: recall -0.450 [-0.504, -0.392]; task success +0.000 [+0.000, +0.000]

Lossy double recall 97/240 = 0.404 [0.344, 0.467] against structured 151/240 = 0.629 [0.566, 0.688]: lossy scored below structured on the point estimate and the intervals do not overlap; structured minus lossy per run 0.225 [0.196, 0.254].

Spend: $0.1004 of $1.00 cap over 192 calls.

Method: 24 synthetic long runs of 30 to 50 entries, each compacted once as a whole by the product's compaction code (extractive epoch; structured summary written by a scripted summarizer and validated by the product's validate; a lossy test double that drops files, failures and decisions). The same model (glm-5.3-flash) is called through the OpenAI-compatible gateway directly, NOT through the Core process, with a fixed system prompt and temperature 0, once per (run, arm) for the recall questions (all of a run's questions in one request, one scored answer line per question; the model is told to answer only from the context and to say unknown otherwise) and once per (run, arm) for the continuation plan. The continuation task is a plan scored by log-derived criteria, not a repository-level code task. Intervals: Wilson for proportions, deterministic bootstrap for means and paired differences. Cost is the gateway-reported usage at the list price. Tokens saved are reported both as the estimated retained-context tokens and as the gateway-reported input tokens of the two requests.

Not measured: Not measured: repository-level task completion by an agent running through the Core; behaviour of real sessions (the runs are synthetic); other models; a larger-window uncompacted arm differing from the full log; sampling variance at temperature above 0.

Digest `4bc7ea3d561b41ddc004abcc7d29e7f53a1e0f16b2fb48807fcec99aaf919d1c` (reproduce with `compaction-eval-live --rescore <dir>`).
