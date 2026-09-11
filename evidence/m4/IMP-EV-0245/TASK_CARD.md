# Task Card — IMP-EV-0245 Structured failure diagnostics

## Identity

- Task ID: IMP-EV-0245
- Milestone: M4 Durable recovery spine (P0)
- Requirement: REQ-EV-0245; owner label: Reliability Layer; subsystem: durability (core-runtime classifier)
- Qualification: `QUAL-EV-0245` — Fault corpus produces stable diagnostic features.
- Evidence tier: production-equivalent (the production classifier over a pinned corpus of every failure source kind; the same classifier the real Core runs in `QUAL-EV-0073`)

## Goal

An adaptive evaluator receives a typed failure taxonomy with evidence — class, code, retryability, source, and the source's own tags as stable feature strings — not a guess from prose.

## Existing-code audit

- classification: NOT-FOUND before this batch (no taxonomy; failures were prose).
- production entry points: `crates/core-runtime/src/diagnostics.rs` `classify` (features: `class:<c>`, `code:<code>`, `source:<tool|provider|store|harness|restart|lease>`, `retryable`/`not_retryable`, plus `tool:<name>`, `timed_out`, `corrupt_state`, `budget:<name>`, `boundary:<b>`; sorted, deduplicated); `FailureDiagnostic.features` travels in `TaskNeedsAttention` and `TaskStatus.diagnostic_features`.
- proof: the corpus `crates/core-runtime/tests/fixtures/diagnostics/corpus.json` (39 cases: tool timeouts, non-zero exits, cancellations, failed checks, corrupt and missing objects, broker unreachable, journal failure, stale lease, unknown outcome, approvals pending and denied, scope violations, invalid arguments, unknown tools; provider connect/rate-limit/timeout/auth/revoked/none/other; store lease/integrity/mismatch/conflict/sqlite; budgets; loop question/escalation/no-progress/scope; restart at every boundary; lost lease) is classified twice and compared field by field with the pinned `expected.json`; a change to any class, code, retryability or feature fails until the pin is regenerated on purpose.

## Limitations

- The corpus pins the classifier's output, not an evaluator's use of it; the adaptive evaluator that consumes the features is M9 work.

## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`):

- `qual_ev_0245_fault_corpus_produces_stable_diagnostic_features` (crates/core-runtime)

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json copy alongside
