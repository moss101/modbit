# PX-059 — calibrated accounting estimate (live finding)

Row: docs/62 REQ-PX-059 / QUAL-PX-059. The graph node stays at REAL_TESTING: its live leg has to pass on main.

## Finding

Live run 45221342126 (main c5294eb0, gemini-3.8-flash) failed `estimator_within_declared_error`. The accounting estimate was 12245 tokens against Gemini's reported 9760: 2546 bp off, against the 2500 bp the view declares.

Cause: `inspector::compile_record` recorded the bare `tokens-v2` heuristic. The run already learns a calibration ratio from every reported count (`modbit_compaction::Calibration`, which compaction uses), but the accounting never applied it.

## Change

- `compile_record` takes the run's calibration, from requests 1..n-1 when compiling request n.
- Once the provider has counted at least one earlier request, every category estimate is the calibrated one. The record names its `estimator` (`tokens-v2` or `tokens-v2+calibration`) and keeps `request_estimate`, `calibration_ratio` and `calibration_samples`.
- The provider's count stays the total whenever it reports one. The first request of a run is still the bare heuristic.
- `Calibration::samples()` is new.

## Proof

- `services/modbit-core/tests/run_control.rs` `qual_px_059_after_the_first_count_the_estimate_follows_the_providers_tokenizer`: a real Core with a scripted provider whose tokenizer is about 6.5 bytes a token. After a five-turn run the estimate is under 1000 bp off the provider's count.
  - Mutation, calibration off: 9555 bp off (8747 vs 4473), and the test fails.
- The other `qual_px_059_*` tests, the whole `run_control` suite, `modbit-compaction` and `context_rules_pack` pass (`tests.log`). clippy is clean (`clippy.log`).

## Open

- The live leg on main. It needs the Gemini project's quota restored: run 45221342126 ended with HTTP 429 "exceeded your current quota".
