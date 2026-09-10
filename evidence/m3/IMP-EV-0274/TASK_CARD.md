# Task Card — IMP-EV-0274 Measured context/tool-result savings

## Identity

- Task ID: IMP-EV-0274
- Milestone: M3
- Requirement: REQ-EV-0274; owner label: Benchmark Harness / Context Economy; subsystem: eval-bench
- Qualification: `QUAL-EV-0274` — Paired benchmark publishes verified-outcome economics.
- Evidence tier: real-system or production-equivalent

## Goal

Measure tokens/tool calls/latency saved by compaction, OutputRef and retrieval choices against baseline.

## Existing-code audit

- classification: NOT-FOUND before this task. The retrieval benchmark measured retrieval quality (M3.9) and the task economics view measured one task (IMP-EV-0173); nothing compared two configurations of the product on the same work.
- production entry point: `benchmarks/context-economics` (`Trial`, `Metric`, `paired_report`); the runner is `qual_ev_0250_0252_0274_paired_context_economics_benchmark_publishes_savings_with_confidence`, which drives the real Core and reads every number back through `GetTaskEconomics`.
- proof: The benchmark publishes the economics next to the outcome: input tokens, tool calls, agent time and cold time for each variant, with the paired deltas and their interval, and the count of verified outcomes in each. The saving is attributed to the machinery under test because everything else is held constant and the report says what varied. On this run: 54% fewer input tokens, three fewer tool calls, and the same verified outcome in both variants.

## Limitations

The model is a deterministic local server, so the paired trials are identical and the bootstrap interval has no width: the report says so rather than implying variance a real provider would have. The variants differ by what each script does with the tools it has, which measures the machinery under its intended use rather than a model's spontaneous behaviour; whether a model left to itself would reach for retrieval, and how many calls it would make, is REQ-EV-0251 and REQ-EV-0253 and needs a live provider.

## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`):

- `qual_ev_0250_0252_0274_paired_context_economics_benchmark_publishes_savings_with_confidence`
- `a_paired_report_measures_the_difference_and_says_what_it_cannot`
- `a_difference_that_is_not_there_is_not_claimed`
- `an_unpaired_trial_says_nothing_about_a_difference`
- `qual_ev_0173_task_economics_report_quality_and_cost_from_the_log`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json/log copies alongside
