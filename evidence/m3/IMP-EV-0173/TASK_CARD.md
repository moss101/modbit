# Task Card — IMP-EV-0173 Context efficiency metrics

## Identity

- Task ID: IMP-EV-0173
- Milestone: M3
- Requirement: REQ-EV-0173; owner label: Context Economy; subsystem: context-engine
- Qualification: `QUAL-EV-0173` — Benchmark dashboard reports quality and economics together.
- Evidence tier: real-system or production-equivalent

## Goal

Measure verified outcome per token/latency/cost.

## Existing-code audit

- classification: PARTIAL before this task. Usage, routes, verification runs and tool calls were all on the canonical log, and the Context Inspector had just started reporting the prompt-cache prefix; nothing put quality and cost in one place.
- production entry point: `crates/protocol` `GetTaskEconomics` → `TaskEconomicsView`; `services/modbit-core/src/economics.rs`; CLI `task economics --task <id>`; desktop economics banner (`task-economics`).
- proof: the view is counted from the log and nothing else — model calls from ModelInvocationStarted, tokens from ModelUsageRecorded (including the cached share the provider reported), tool calls from ToolCallProposed, model and tool time from the interval between each start and its completion, wall time from the first and last event of the task, prefix cache hits and misses from the routed cache key, epochs and summarised entries from ContextEpochOpened, injected tokens from the task's Context Ledger. Quality sits in the same view, and never overstates: the verdict is the COMPLETION run's own (PASSED, NO_CHECKS when it had nothing to run, FAILED, or NOT_RUN), `verified` is true only when a run passed with at least one check, and the regressions counter separates a suite that was already failing from one this change broke. Cost is the catalog list price of the model the calls actually routed to, and the view says when no catalog priced it.

## Limitations

Cost is a list price, not a bill: no cache discount, batch rate or negotiated price is applied, and the view says so. Latency is measured from event timestamps on the Core's clock. There is no cross-task or cross-run rollup yet, so "verified outcome per token" is reported for one task at a time; the paired baseline comparison belongs to the benchmark family (REQ-EV-0250 to 0254, REQ-EV-0274).

## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`):

- `qual_ev_0173_task_economics_report_quality_and_cost_from_the_log`
- `qual_px_016_change_strategy_tests_first_one_concern_per_transaction_and_no_silent_scope_widening`
- `qual_ev_0056_0092_0130_compaction_epoch_preserves_facts_survives_restart_and_moves_the_cache_prefix`
- `qual_ev_0111_0268_context_show_reports_compaction_epochs_and_the_cached_prefix`

The CLI and desktop surfaces build and typecheck in the same CI run.

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json/log copies alongside
