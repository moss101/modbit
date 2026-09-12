# Task Card — M5.6 Tool-schema / token-economics benchmark

## Identity

- Task ID: M5.6
- Milestone: M5 Procedural runtime and skills (P0)
- Requirements: docs/43 M5 ("tool-schema/token-economics benchmark"; proof: "direct and procedural mode yield equivalent receipts/policy behavior"), REQ-EV-0116 / QUAL-EV-0116 (tool-schema token benchmark vs eager all-tools baseline; the procedural half), docs/63 benchmark discipline (paired trials, stated method, no claims beyond the measurement), docs/72 (tool-schema proliferation risk: procedural runtime + dynamic projection + deferred tail as the mitigation).
- Qualification: a paired benchmark through the real Core with effect and policy parity asserted per pair and a published report.
- Evidence tier: real-system for the measurement (the real Core, the real pipeline, the canonical log, the requests the model server received); the model is the deterministic local server, so token counts are its accounting (request bytes / 4), not a live provider's.

## Goal

Measure what the procedural surface buys and what it costs — model calls, tool-schema bytes, prompt tokens — against direct mode on the same task, and prove the two modes are the same product at the policy boundary.

## Existing-code audit

- classification: PARTIAL before this task. QUAL-EV-0116's projected-vs-eager schema-bytes comparison existed in the tool-surface test; no benchmark compared the modes, and the paired benchmark crate measured neither model calls nor schema bytes.
- production entry points:
  - `benchmarks/context-economics/src/lib.rs` — `Trial.tool_schema_bytes`, `Metric::ModelCalls`, `Metric::ToolSchemaBytes`, both in every paired report.
  - `services/modbit-core/tests/surface_protocol.rs` — `qual_m5_6_direct_and_procedural_modes_yield_the_same_effects_at_different_token_costs`: three pairs; per pair the tool calls on the log (name, final state) and `b.txt` are equal across modes; the report is printed as `BENCH {json}` and asserted (pairs, verified, model calls 5 vs 3, schema bytes and input tokens lower in procedural mode, tool calls equal, the method's stated limits).
- proof (measured): `model_calls` 5 → 3; `tool_schema_bytes` 70,813 → 41,709 (−41 %); `input_tokens` 5,860 → 3,143 (−46 %); `tool_calls` 3 = 3 with identical outcomes (`fs.read`, `change.apply`, `shell.exec`, all `ToolCallSucceeded`); `b.txt` identical; both variants verified in all three pairs.

## Limitations

- The model is the deterministic local server: token counts are its accounting; the paired trials are identical, so the intervals have no width; whether a live model writes a correct program, or prefers procedural mode, is not established (DR-M3-002 keeps live-provider proofs deferred).
- One task shape; the savings scale with the number of turns a program replaces, which the report's method says.

## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`):

- `qual_m5_6_direct_and_procedural_modes_yield_the_same_effects_at_different_token_costs` (services/modbit-core, real Core)
- Regression: `qual_ev_0096_0116_0133_0044_0031_tool_surface_is_compiled_from_support_and_policy` (eager baseline), `qual_ev_0250_0252_0274_paired_context_economics_benchmark_publishes_savings_with_confidence`, the benchmark crate's `paired` tests

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names, the measured report)
- CI run json copy alongside
