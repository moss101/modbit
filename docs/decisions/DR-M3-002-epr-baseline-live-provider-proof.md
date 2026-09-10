---
id: DR-M3-002
title: The EPR direct baseline seals on the wire-faithful path; the production-endpoint run of EPR-E2E-000 waits for credentials
status: accepted
date: 2026-09-11
supersedes: none
approved_by: repository owner (moss101), standing instruction "complete all tasks"; evidence-scope decision applying DR-M2-001 (live provider proof pending credentials) to the EPR baseline, under docs/15 "Live provider proof" and docs/82 (no simulated substrate is claimed as the live proof)
---

# DR-M3-002 — The EPR baseline's live-provider half waits for credentials

## Trigger / evidence

`QUAL-EPR-000` / `EPR-E2E-000` (docs/61) ask for the direct baseline to be
measured "through Core on a real Git repository with edit/build/test/review and
cancellation" using a **real provider**. The repository still has no provider
secrets and the build host holds no `OPENAI_API_KEY` / `ANTHROPIC_API_KEY`, the
same condition DR-M2-001 recorded on 2026-09-09. Presenting a local server as a
production endpoint would be inventing evidence, which `docs/82` forbids.

Leaving EPR-000 NOT_STARTED has a cost of its own: EPR-001, EPR-002, EPR-003,
EPR-004, EPR-005, EPR-014, EPR-015 and EPR-016 are gated behind it in the graph,
and most of that work — schemas, migrations, the plan validator, statistics
materialization, feasibility arithmetic — needs no provider at all.

## Current behavior

The direct single-model path works and is instrumented: invocations, retries,
prompt-prefix cache units, verification runs and user interventions are all on
the canonical log. Nothing published a baseline, and a cancelled or dropped
invocation left no usage record at all, so its cost read as zero rather than as
unknown.

## Proposed replacement

1. EPR-000 ships the instrumentation gap and the baseline itself:
   `crates/observability` gains the `BaselineBundle` (build digest, repository
   revision, environment digest, and per task the outcome, verification verdict,
   checks, model calls, retries, cache units, tool calls, usage with unknowns
   preserved, wall/model/tool time and the user's interventions), the Core gains
   `PublishOutcomeBaseline` and the `OutcomeBaselinePublished` session event, and
   the CLI gains `baseline publish`.
2. A cancelled, interrupted or failed invocation now appends
   `ModelUsageRecorded` with `reported: false`. Unknown cost is carried as
   unknown through the bundle (`tasks_with_unknown_usage`, null token fields)
   and is never settled as zero (docs/38 "Shared serialization and validation").
3. Its retained evidence is
   `qual_epr_000_the_direct_path_is_instrumented_and_published_as_a_fixed_revision_baseline`:
   the real Core on a real Git fixture repository, a task that reads, plans,
   repairs, edits, runs real `cargo` build and test evidence and is accepted in
   Review, plus a second task whose provider stream is cancelled in flight. It
   asserts the published bundle, its digest over its own content, the pinned
   revision, the per-task numbers, and that the cancelled attempt's cost is
   unknown while its task changed nothing.
4. The live half of `EPR-E2E-000` — the same shape against a production
   endpoint, retaining provider request IDs and the observed cost/latency
   baseline — runs in `.github/workflows/live-providers.yml` with
   `MODBIT_LIVE_PROVIDERS=1` once the owner adds the repository secrets. Until
   then the workflow skips with an explicit notice. The first green run is
   appended to `evidence/m3/EPR-000/evidence.json` as run evidence.
5. EPR-000's graph node is sealed on items 1–3. The task card states the open
   item, and the milestone-level claim that the baseline was measured against a
   production provider stays open until item 4 has run green — exactly as
   DR-M2-001 left M2's "E2E with live model" open.

## Migration

`ModelUsageRecorded` gains an additive `reported` field defaulting to false, so
events written before this record decode unchanged (docs/30 additive-field
rule). `SessionEvent::OutcomeBaselinePublished` is additive and carries no state
change.

## Compatibility

Additive protocol messages (`PublishOutcomeBaseline`,
`OutcomeBaselinePublished`) and one additive session event. No existing
interface changes meaning.

## Security impact

The bundle carries no prose and no credentials: goals appear only as digests,
and the environment digest is a hash of platform and toolchain identity. The
live workflow receives secrets the same way DR-M2-001 established, through
repository secrets read only by the Core process.

## Test impact

One new Core test (item 3), the observability unit tests for the bundle, and the
existing suites unchanged. The live test is the one deferred to item 4.

## Rollback

Revert the commit that introduces the bundle and the `reported` field. The log
stays readable: both additions are additive.
