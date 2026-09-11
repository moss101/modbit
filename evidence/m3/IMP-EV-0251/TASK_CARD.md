# Task Card — IMP-EV-0251 Measure tool-call reduction

## Identity

- Task ID: IMP-EV-0251
- Milestone: M3
- Requirement: REQ-EV-0251; owner label: Benchmark Harness; subsystem: eval-bench
- Qualification: `QUAL-EV-0251` — same model, task and environment across variants.
- Evidence tier: real-system or production-equivalent
- Decision Record: `docs/decisions/DR-M3-003-benchmark-and-conformance-live-model-halves.md` (the live-model half)

## Goal

Count normalized tool calls per task, so the count compares across variants that expose different tool families and says how much work the agent had to do rather than how many protocol messages it sent.

## Existing-code audit

- classification: PARTIAL before this task. The paired benchmark (IMP-EV-0250/0252/0274) counted tool calls as the log counts them: a `change.batch` carrying five edits counted as one call, and every variant's count meant something slightly different depending on which tools it had.
- production entry points:
  - `benchmarks/context-economics::normalized_tool_calls` — protocol calls (`plan.update`, `task.complete`, `repair.attempt`, `user.ask`, `tool.search`) are left out, a `change.batch` counts as the operations it carried, and every other call — a read, a search, a pack, a shell command, an edit — is one unit of work whatever it is named.
  - `Trial::normalized_tool_calls` and `Metric::NormalizedToolCalls` — carried through the paired report with the same bootstrap interval as every other metric.
  - The real benchmark through Core (`qual_ev_0250_0252_0274_…`) records the calls each variant actually made from the model server's request bodies and normalizes them; the model, the task and the environment are held constant across the variants, as the report's `held_constant` says.
- proof: on the real benchmark, the baseline's four reads and the treatment's one pack normalize to 4 and 1 with a paired delta of −3 over three pairs; the unit tests show a batch of three edits counting as three, the protocol calls counting as nothing, and a raw count that would have told a different story (7 vs 6) once a batch hides work.

## Limitations

- The reduction a model achieves when it chooses its own tools is the live-model half: on the wire-faithful local provider each variant does what its script does, so the count measures the machinery under its intended use, not a model's spontaneous behaviour. DR-M3-003 defers that run to `.github/workflows/live-providers.yml`.
- The log's own `tool_calls` already leaves harness tools out, so on this benchmark the raw and the normalized counts agree; they part company when a batch is used.

## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`):

- `qual_ev_0250_0252_0274_paired_context_economics_benchmark_publishes_savings_with_confidence` — the real benchmark, now with the normalized count.
- `tool_calls_are_normalized_across_tool_families`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json copy alongside
