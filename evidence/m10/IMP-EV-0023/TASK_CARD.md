# Task Card — IMP-EV-0023 Cloud SLO event ladder

## Identity

- Task ID: IMP-EV-0023 (REQ-EV-0023, ADOPT; owner Observability, subsystem observability)
- Milestone: M10 (wave 1)
- Qualification: QUAL-EV-0023 — a staging run emits all timestamps and derived cold/warm latency metrics.
- Evidence tier: release-critical (evidence semantics of the cloud path's SLOs)
- No second metrics store: the rungs are task events on the canonical log (mirrored to the cloud log like every event); the derivation is a pure function in `crates/observability`.

## Existing-code audit

- classification: NOT-FOUND. Timestamps existed scattered (the cloud tables' `recorded_at_ms`, `claimed_at_ms`, `created_at_ms` — written after provisioning; the gateway's in-memory `last_first_token_ms` per endpoint), but no event recorded a sandbox being requested, none recorded a run's first token or first tool, `SandboxLeaseAcquired` carried no timing, no warm pool exists, and nothing derived cold/warm latencies.
- first missing link: no rung for "sandbox requested" and none for the first token of a run.
- production entry points: `crates/domain/src/task.rs` (`TaskEvent::SloStageRecorded`); `services/modbit-core/src/slo.rs` (`record`, `first`, `ladder`); `server.rs` (`REQUESTED`/`PREWARM` at `StartTask` for `cloud_isolated`; `GetSloLadder`); `sandboxes.rs` (`SANDBOX_REQUESTED`/`SANDBOX_READY`, warm on reuse, cold on provision); `runtime.rs` (`FIRST_TOKEN` on a run's first model output, `FIRST_TOOL` before its first tool dispatch); `crates/observability/src/slo.rs` (`ladder`, `summary`).

## Verification

- `qual_ev_0023_a_cloud_tasks_starts_emit_every_slo_timestamp_and_cold_and_warm_latencies` (apps/cloud-worker; the real Cloud API over Postgres, a worker, the Sandbox Gateway — the MicroVM backend in the hosted cloud job, the reference backend locally): a `cloud_isolated` task reaches review; its cloud log carries all six rungs in ladder order with non-decreasing timestamps, `PREWARM` `NONE`, `SANDBOX_READY` cold with the sandbox id, `FIRST_TOKEN` and `FIRST_TOOL` naming the same run; the shared derivation yields one cold start with provisioning, ready, first-token and first-tool latencies. Attached to the worker's Core, the task is returned from review and started again: `GetSloLadder` shows two starts, `COLD` then `WARM` (the sandbox reused), every timestamp present and ordered, two distinct runs, the warm provisioning no slower than the cold, one cold and one warm start in the figures.
- Mutation: recording a reused sandbox as cold fails the test on `COLD`/`WARM`.
- Unit: `modbit_observability::slo::tests::a_ladder_derives_each_starts_latencies_and_separates_cold_from_warm`.
- The whole cloud suite (API, worker, sandbox, gateway) passes with the new rungs on the log.

## Limitations

- No warm pool exists; `PREWARM` is recorded as `NONE`, and a warm pool is a product decision for the owner.
- Through the Cloud API every start is cold today (a task's sandbox lives for the task, and the API relays no second start); the warm path is a second run on the Core while the task holds its sandbox.
- `FIRST_TOKEN`/`FIRST_TOOL` are "first in this process": a Core restarted mid-run may record them again for the resumed run; the derivation takes the first per start.
- Latencies are per task; fleet-wide p95 dashboards over them are M10.1.

## Evidence

- `evidence.json` in this directory
- docs/34 and docs/30 as built
