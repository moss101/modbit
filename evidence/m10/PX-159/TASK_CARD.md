# Task Card — PX-159 Compaction telemetry promised by doc 34: tokens before and after, duration, mode, source, rejection and cost

## Identity

- Task ID: PX-159 (REQ-PX-159; owner observability); qualification QUAL-PX-159; prerequisite DOC-PX-017 (row registered on PR #148's successor, #156)
- Evidence tier: release-critical (two compaction events gain fields; what leaves the machine)

## Existing-code audit (main 5b17f473)

- Classification: IMPLEMENTED-PARTIAL. The PX-139 exporter (`services/modbit-core/src/telemetry.rs`, off unless `MODBIT_OTLP_ENDPOINT`, redacted, never blocking) exported a zero-length `modbit.compaction_epoch` span with the epoch, source entries and projection tokens only. Nothing recorded the tokens before or after, the duration, the summarizer's calls, or a rejected compaction.
- Owners kept: the one exporter and the compaction ladder; no second telemetry path.

## Built

- `crates/domain`: `CompactionStarted.tokens_before`; `CompactionCommitted.tokens_after` (projection plus kept tail) and `summarizer_usage` (`{endpoint, requested_model, input, output, cached, reported}` per call); all serde-default.
- `services/modbit-core/src/runtime.rs`: fills them on both the synchronous and the asynchronous path.
- `services/modbit-core/src/telemetry.rs`: one `modbit.compaction` span per compaction id, from its request to its commit or stale rejection, with `id, epoch, mode, outcome, tokens_before, duration_ms` and, committed, `tokens_after, summary_source, fallback_reason, summarizer_calls, summarizer_cost_minor` (priced by `usage::call_cost` like every request) and `summarizer_unmetered`, or, rejected, `rejection`. No summary text, path or prompt is an attribute.

## Proof (local, macOS arm64)

- `services/modbit-core/tests/context_rules_pack.rs::px_159_compactions_export_their_tokens_duration_mode_source_and_cost` (real Core, a real OTLP/HTTP collector on a socket): an extractive-fallback compaction (`SUMMARIZER_NOT_CONFIGURED`, tokens before > after > 0), a committed model summary (one summarizer call, cost and unmetered flag present, no summary text in any export), and a stale result held by the docs/54 fault-10 hook and refused (`REJECTED`, its code equal to the event's reason, mode ASYNC) — one span per compaction.
- Re-run: `px_139_otel` (export off by default; a hung, refusing or vanished collector costs the run nothing), the px_109 compaction tests, the compaction tests of `surface_protocol`; workspace `cargo check`; clippy.
- Mutation check: `mutation-check.txt`.

## Limitations

- The exporter sends a run's spans when it completes, fails or is cancelled; a run that suspends and is never resumed exports nothing (pre-existing PX-139 behaviour; the test ends its run with `CancelTask`). Counters for compactions are not added (spans only). Graph status cannot move until #156 lands on main.
