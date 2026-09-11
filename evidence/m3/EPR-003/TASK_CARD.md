# Task Card — EPR-003 Bootstrap and calibrate the Request Profiler

## Identity

- Task ID: EPR-003
- Milestone: M3 (moved from M2 by DR-M2-002)
- Requirement: REQ-EPR-003; owner label: model-gateway; subsystem: model-gateway
- Qualification: `QUAL-EPR-003` / `EPR-E2E-003` — benchmark a fixed real repository and request corpus plus an untouched holdout; publish Brier, ECE, slice and OOD metrics and extraction-inclusive p50/p95 on named hardware; price, provider, cache and allowlist changes do not change intrinsic features. Negative: `EPR-FI-003` — disable the profiler and supply stale or missing repository metadata; use a conservative eligible initial-leg plan or fail; contaminated features and poor high-risk calibration block promotion.
- Evidence tier: real-system or production-equivalent

## Goal

Extract bounded, deterministic, catalog-independent features of a request and its repository, and answer `p_floor_success`, confidence and out-of-distribution from a versioned calibration cohort — with no model call in the hot path, a conservative fallback when it cannot know, and shadow-only operation until its calibration earns promotion.

## Existing-code audit

- classification: ABSENT before this task. Nothing extracted features from a request: the runtime read the goal text only for the reproduction gate, and no probability, confidence or OOD signal existed anywhere in the product.
- production entry points:
  - `crates/providers/src/profiler.rs` — the extractor and the cohort. `RequestFacts` is the whole input, and it has no field for a price, a provider, a cache or an allowlist, so a configuration change cannot reach a feature. Extraction reads at most 8 KiB of goal text and 2000 paths, calls no model, and is deterministic.
  - `profile` — `p_floor_success` is the slice's recorded rate shrunk toward a conservative prior by how little support it has; a slice below the support floor is out of distribution and says so; a disabled profiler or stale repository metadata answers with the conservative profile rather than a guess. `Profile::may_economise` refuses to let a fallback or an OOD profile justify a cheaper plan.
  - `crates/providers/calibration-cohort.json` — the recorded cohort, shipped with the build. It is empty, because nothing has been recorded yet, and the file says so: an entry in it is a claim that the product observed that slice.
  - `calibration` — Brier, ECE, slice coverage and OOD over the untouched holdout, with the blockers that stop promotion.
  - `services/modbit-core/src/runtime.rs` — every run profiles its request at start and records `RunEvent::RequestProfiled`. Shadow only: nothing reads it to decide anything, and the plan the run executes carries `profiler_version: "none"` in its provenance, which is the truthful statement that no profiler informed it.
- proof: a real task runs through Core against a real repository, and the recorded profile reads the request for what it intrinsically is — a cross-file bug fix in a web repository that wants verification — with a feature digest, an out-of-distribution flag, a zero probability and zero confidence, because the cohort behind it is empty. The run still dispatches on the direct path. Over a fixed corpus of eight requests against four real fixture repositories, extraction classifies every request's task and domain correctly, is deterministic to the digest, and costs microseconds.

## Published metrics

Measured on the corpus in `crates/providers/tests/profiler.rs`, on an Apple M5 Pro (Darwin arm64):

- extraction, inclusive of reading the repository facts: p50 = 22 µs, p95 = 261 µs over 8 requests.
- calibration against the untouched holdout: slices covered 0, observations 0, Brier 0.000, ECE 0.000, OOD slices 0, promotable **false**.
- blockers: the holdout has 0 observations (promotion needs at least 200); no holdout slice is covered by the calibration set.

The Brier and ECE numbers are what an empty cohort produces, and they are published as such rather than read as a flattering zero: with no observations there is nothing to be calibrated against, which is precisely why the profiler stays in shadow.

## Limitations

- The calibration cohort is empty. Recording one needs observed outcomes at a volume this repository has not produced, and the promotion gate refuses at 0 observations, so the profiler is shadow-only in fact and not just by intent. When EPR-015 snapshots accumulate, the cohort is regenerated from them; it is never hand-written.
- Because the cohort is empty, every request is out of distribution today, and `p_floor_success` is the conservative prior for all of them. The shrinkage rule, the support floor and the promotion gate are exercised against synthetic cohorts in tests, which is honest for a rule but is not a measurement of the product.
- `has_configured_verification` is not yet read from the workspace at run time; the runtime passes `false` and lets the request text decide the verification modifier. Reading `.modbit/verification.json` at profile time is a small follow-on.
- The profiler feeds nothing. Using it to choose a plan is EPR-016 and EPR-004, and both are gated on a promotable cohort.

## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`):

- `qual_epr_003_the_profiler_records_intrinsic_demand_in_shadow_and_claims_nothing` — the end-to-end proof through Core.
- `extraction_is_bounded_deterministic_and_reads_the_corpus_correctly` — the corpus, the determinism and the latency.
- `features_are_intrinsic_and_no_catalog_change_can_move_them`
- `a_disabled_profiler_or_stale_metadata_answers_conservatively` (EPR-FI-003)
- `a_thin_slice_is_shrunk_toward_the_prior_and_a_thick_one_is_not`
- `calibration_is_published_and_an_unrecorded_cohort_blocks_promotion` (EPR-FI-003)

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- `ci-run-34554982015.json`: hosted CI, green on macOS, Linux and Windows at `0540d62`
- Commit: `0540d62`
