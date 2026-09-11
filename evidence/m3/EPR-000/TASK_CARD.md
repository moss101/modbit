# Task Card — EPR-000 Preserve and measure the direct baseline

## Identity

- Task ID: EPR-000
- Milestone: M3 (moved from M2 by DR-M2-002)
- Requirement: REQ-EPR-000; owner label: model-gateway; subsystem: model-gateway
- Qualification: `QUAL-EPR-000` / `EPR-E2E-000` — a real provider through Core on a real Git repository with edit/build/test/review and cancellation; retain request/event/usage IDs, build and environment digests plus cost/latency/success baseline. Negative: `EPR-FI-000` — drop a provider stream and cancel an active attempt; incomplete usage stays unknown/estimated and no effect is repeated.
- Evidence tier: real-system or production-equivalent
- Decision Record: `docs/decisions/DR-M3-002-epr-baseline-live-provider-proof.md`

## Goal

Keep the existing single-model engineering path working; instrument every invocation, cache unit, retry, verification and intervention; publish a fixed-revision verified-outcome baseline before routing changes.

## Existing-code audit

- classification: PARTIAL before this task. Invocations, retries (in the route record), prompt-prefix cache units, verification runs and every intervention were already on the canonical log. Two things were missing: nothing published a baseline, and a cancelled or dropped invocation left no usage record at all, so its cost read as zero.
- production entry point: `crates/observability/src/baseline.rs` (`BaselineBundle`, `Usage`, `Interventions`, `publish`); `services/modbit-core/src/baseline.rs` (assembly from the log, build and environment digests); `PublishOutcomeBaseline` → `SessionEvent::OutcomeBaselinePublished`; CLI `baseline publish`; the unknown-usage records in `services/modbit-core/src/runtime.rs`.
- proof: the direct path runs a real coding task on a real Git fixture — read, plan, a recorded repair attempt, an edit, real `cargo` build and test evidence, and a Review the user accepts — and a second task's provider stream is cancelled while it is in flight. The published bundle is pinned to the build digest, the repository revision and the environment digest, and carries per task the state, the completion run's own verdict, the checks it passed and failed, the model calls, the retries, the cache units reused and rebuilt, the tool calls, the usage, the wall, model and tool time, and what the user had to do. Its digest covers its own content. The cancelled attempt is recorded as an invocation whose usage was never reported: its token fields are null, the task is counted in `tasks_with_unknown_usage`, and nothing it touched was repeated — unknown is never settled as zero.

## Limitations

The live half of `EPR-E2E-000` — the same shape against a production provider endpoint, retaining the provider's own request IDs and an observed cost and latency baseline — has not run: this repository has no provider credentials. DR-M3-002 records the boundary and defers that run to `.github/workflows/live-providers.yml`, which skips with an explicit notice until the owner adds `OPENAI_API_KEY` / `ANTHROPIC_API_KEY`; its first green run is appended to this task's `evidence.json`. The bundle covers one session's tasks; there is no cross-session or cross-revision rollup yet, and the baseline is assembled on request rather than on a schedule.

## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`):

- `qual_epr_000_the_direct_path_is_instrumented_and_published_as_a_fixed_revision_baseline`
- `unknown_cost_stays_unknown_and_is_counted`
- `a_bundle_is_pinned_sealed_and_honest_about_what_it_does_not_know`
- `qual_ev_0173_task_economics_report_quality_and_cost_from_the_log`
- `m2_8_verification_engine_gates_completion_on_real_cargo_fixture`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- `ci-run-34544470328.json`: hosted CI, green on macOS, Linux and Windows at `d21ee00`
- Commits: `994e37c` (the baseline and the unknown-usage records), `0d936a7` and `d21ee00` (two test-side fixes that CI found)

## What CI found before it went green

Run `34542192228` on `994e37c` failed for two things, neither of them the baseline:

- the commit that added `DR-M3-002` touched a locked path without a `Decision-Record:` trailer, which the locked-path lint refuses. Pushed history is never rewritten here, so the rule is carried by the commits that follow.
- two test assertions were wrong rather than the product. The fake provider advertised HTTP keep-alive while answering one request per connection, so a later request could be written into a socket it had already closed; and this task's own test required a tool call to take at least a millisecond, which a local file read need not. Both were fixed forward.
