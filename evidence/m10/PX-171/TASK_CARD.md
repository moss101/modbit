# Task Card — PX-171 Fault-injecting scripted provider server for the provider and Core suites

## Identity

- Task ID: PX-171 (REQ-PX-171; owner eval-bench); qualification QUAL-PX-171; prerequisite DOC-PX-017 (row registered on PR #148's branch)
- Evidence tier: iteration (test support and tests only; no product behaviour changed)

## Existing-code audit (main 5b17f473)

- Classification: IMPLEMENTED-PARTIAL. Fault cases existed, each in its own suite and server: `crates/providers/tests/conformance.rs` (OpenAI and Anthropic fakes: status, stall, drop), `rate_limits.rs` (429 with `Retry-After`, OpenAI only), the Core's scripted OpenAI server (success only). Missing: one shared server both the provider and Core suites use, `retry-after-ms`, a cut in the middle of an event, a malformed event, and the Core-level assertions on the log for both wires.

## Built

- `tests/support/fault_provider.rs` (std only, included by `#[path]` like `scripted_openai.rs`): `fault_provider(Wire, Vec<Fault>)`, request `n` answered by fault `n` (the last repeats); `Fault::{Ok, Status{status, headers, body}, CutMidEvent{keep}, EndEarly{keep}, Stall{keep, ms}, Malformed{keep}}`; `Answer::{Text, Tool}` rendered as real OpenAI chunks or Anthropic named events; every request recorded with its arrival time and body.
- `crates/providers/tests/faults.rs`: the real gateway, each case on both wires.
- `services/modbit-core/tests/px_171_faults.rs`: the real Core, each case on both wires, asserting the log.

## Proof (local, macOS arm64)

- Provider suite, both wires (6/6): a 429 with `retry-after: 1` and with `retry-after-ms: 350` waits at least that long, retries once and completes with one tool call; a cut after the first token is `STREAM_INTERRUPTED`, never retried, with no tool call; a cut before any event is retried and completes; a body that ends cleanly part-way is `STREAM_INTERRUPTED` and unretried after the first token, retried before it; an idle stall under an 800 ms timeout ends `TIMEOUT` between 0.7 and 4 s; a malformed event ends `MALFORMED_STREAM` without completing.
- Core suite, both wires (2/2): a cut inside a tool call leaves `TurnFailed STREAM_INTERRUPTED`, no `ToolCallProposed`, one request; a 429 with `retry-after-ms` is retried once and the first `ModelUsageRecorded` route says `retries: 1`.
- Mutation check: `mutation-check.txt` — a retry after the first token on the body-error branch and on the clean-end branch, the provider's retry header ignored, a malformed OpenAI event dropped, and a server that only ever succeeds; all KILLED. (A first run with only the clean-end mutation SURVIVED because the cut fault reaches the body-error branch; the `EndEarly` fault and test were added for it.)

## Limitations

- The docs/54 list is covered for the provider transport cases above; the existing per-suite fakes were left in place rather than migrated (they assert other behaviour). No live provider. Graph status cannot move until PR #148 (which registers PX-171) lands on main.
