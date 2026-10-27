# Task Card — PX-155 Context-overflow recovery: a typed provider error forces a synchronous compaction and one retry

## Identity

- Task ID: PX-155 (REQ-PX-155; owner durability); qualification QUAL-PX-155; prerequisite DOC-PX-017 (row registered on PR #148's branch)
- Evidence tier: release-critical (recovery and provider protocol behaviour)

## Existing-code audit (main 5b17f473)

- Classification: NOT-FOUND. An overflow answer (OpenAI `context_length_exceeded`, Anthropic "prompt is too long" / 413 `request_too_large`) fell into the gateway's generic `PROVIDER_REJECTED` and ended the run; nothing compacted in answer to it.
- Existing owners kept: the compaction ladder (`compaction_step`, `compaction_model::build`, `modbit_compaction::accept`) and the gateway's status mapping; no second compactor or retry loop.

## Built

- `crates/providers/src/openai.rs`, `anthropic.rs`: `is_context_overflow(status, body)` per wire family (unit tests).
- `crates/providers/src/gateway.rs`: such a response is `Attempt::Failed { code: CONTEXT_OVERFLOW }` (`gateway::CONTEXT_OVERFLOW`).
- `services/modbit-core/src/runtime.rs`: `CompactionCtx.force` takes the synchronous branch whatever the estimate; on the first `CONTEXT_OVERFLOW` of a request the turn loop forces it and, when an epoch installed, re-routes at the epoch boundary and sends the request again; a second overflow, or a compaction that installed nothing, ends the run with `CONTEXT_OVERFLOW` and a message saying why, without the fallback path.
- `services/modbit-core/tests/px_common/mod.rs`: the scripted server can answer with a real HTTP error response (`http_error`).

## Proof (local, macOS arm64)

- `services/modbit-core/tests/px_155_context_overflow.rs` (real Core, real gateway HTTP client, real ladder; scripted replies):
  - `qual_px_155_an_overflow_compacts_synchronously_and_the_retry_completes_the_turn`: one overflow, one `TurnFailed CONTEXT_OVERFLOW`, then `CompactionStarted` (SYNC_FALLBACK) and `ContextEpochOpened` after it, the retried request carries the epoch and the person's goal, the task ends ReadyForReview, the manifest reads back whole and by two ranges.
  - `qual_px_155_a_second_overflow_ends_with_the_typed_failure_and_never_loops`: exactly two overflowed requests, two typed failures, one compaction, no third attempt.
- Provider unit tests for both detectors; clippy `-D warnings`, the provider suite and the existing compaction tests of `surface_protocol.rs` re-run.
- Mutation check: `mutation-check.txt` — the one-retry latch removed, the forced compaction removed, the gateway typing removed; all KILLED.

## Limitations

- The transport is the local scripted server answering with the real response shape, not a live provider; the Anthropic path is proven at the detector (unit) level only. With no summarizer configured the epoch is the extractive fallback (validated by the same `accept`).
- Graph status cannot move until PR #148 (which registers PX-155) lands on main.
