---
id: DR-M3-003
title: Benchmark method and conformance tasks seal on their harness halves; the live-model halves wait for credentials
status: accepted
date: 2026-09-11
supersedes: none
approved_by: repository owner (moss101), standing instruction "complete all tasks"; evidence-scope decision applying DR-M2-001 and DR-M3-002 (live provider proof pending credentials) to the benchmark-method and language-conformance tasks, under docs/15 "Live provider proof" and docs/82 (no simulated substrate is claimed as the live proof)
---

# DR-M3-003 — The benchmark and conformance live-model halves wait for credentials

## Trigger / evidence

Four M3 tasks have qualifications with two halves: a harness or product half
that is provable on the wire-faithful local provider, and a half whose only
subject is a real model's own behaviour.

- `IMP-EV-0251` (REQ-EV-0251, "count normalized tool calls per task; same
  model, task and environment across variants") — the normalization and the
  paired count are harness machinery; the *reduction* a model achieves when it
  chooses its own tools is a live-model result.
- `IMP-EV-0253` (REQ-EV-0253, "the agent chooses tools naturally so the
  treatment is not biased; benchmark prompts are identical except the
  available capability profile") — prompt parity and the absence of forcing
  instructions are properties of the harness and provable; whether a model
  "chooses naturally" is only observable with a model that chooses.
- `PX-028` (REQ-PX-028) — the Tier A suite with real headless language
  services and the incremental index latency are provable here; "competence
  suite tasks for each language pass at baseline" needs the competence
  baseline of `PX-020`, which needs a live model.
- `PX-022` (REQ-PX-022) — onboarding to a first useful task on the packaged
  app is measurable against the wire-faithful provider; the qualification
  names "a live provider test model" for the median it reports.

`PX-020` (REQ-PX-020, the fixed competence baseline on public and internal
suites) has no offline half: its whole value is the real model's result, and
running the suites on a scripted server would be inventing a baseline
(`docs/82`). It stays open.

The repository still has no provider secrets and the build host holds no
`OPENAI_API_KEY` / `ANTHROPIC_API_KEY`, the condition DR-M2-001 recorded on
2026-09-09 and DR-M3-002 confirmed on 2026-09-11.

## Current behavior

`benchmarks/context-economics` publishes paired reports with a bootstrap
interval (IMP-EV-0250/0252/0274, sealed), counting tool calls as the log
counts them and saying nothing about whether the variants were asked the same
thing. `crates/verification/language-tiers.json` records Tier A passes for
Rust, Python and TypeScript from PX-027's suites with real language services.
Nothing measures incremental index latency against a stated budget. Nothing
drives onboarding on the packaged app.

## Proposed replacement

1. `IMP-EV-0251` ships `normalized_tool_calls`: protocol calls (the plan
   gate, the completion handshake, repair bookkeeping, questions) are left
   out, a batch counts as the operations it carried, and every other call is
   one unit of work whatever it is named, so counts compare across variants
   with different tool families. The paired report carries it as a metric,
   and the real benchmark through Core records it from the calls each
   variant actually made.
2. `IMP-EV-0253` ships `prompt_parity`: the first prompt each variant saw is
   compared with the workspace location scrubbed; the system prompt and the
   request must be byte-identical, the only permitted difference is the tool
   list, and any phrase that tells the agent which tools to use invalidates
   the comparison. The report carries the verdict, and the real benchmark
   through Core asserts it.
3. `PX-028` ships the incremental index latency measurement against a
   declared budget on the three fixtures, and the explicit-degradation proof
   for a dead language service, on top of PX-027's recorded Tier A passes.
4. `PX-022` ships the onboarding flow driven on the packaged app against the
   wire-faithful provider, reporting the median and p90 it measured.
5. The live-model halves — the tool-call reduction a model achieves, whether
   it reaches for retrieval unprompted, the per-language competence baseline,
   and onboarding with a live test model — run in
   `.github/workflows/live-providers.yml` once the owner adds the repository
   secrets, exactly as DR-M2-001 and DR-M3-002 defer theirs. Their first green
   run is appended to each task's `evidence.json`.
6. `IMP-EV-0251`, `IMP-EV-0253`, `PX-028` and `PX-022` seal on items 1–4 with
   their task cards stating the open item. `PX-020` is set `BLOCKED` in the
   graph with this record as the reason, and the milestone-level claim that
   the competence baseline was measured stays open until item 5 has run.

## Migration

None: additive fields on the benchmark report (`normalized_tool_calls`,
`prompt_parity`, both defaulted) and additive tests.

## Compatibility

No interface changes meaning. Reports written before this record deserialize
with the new fields absent.

## Security impact

None. The live workflow receives secrets the same way DR-M2-001 established,
through repository secrets read only by the Core process.

## Test impact

New unit tests in `benchmarks/context-economics`, the real benchmark through
Core extended with the normalized count and the parity verdict, PX-028's
latency and degradation tests, and PX-022's packaged-app flow. The live halves
are the ones deferred to item 5.

## Rollback

Revert the commits that introduce the fields and the tests. Nothing on the log
changes shape.
