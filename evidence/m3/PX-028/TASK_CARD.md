# Task Card — PX-028 Tier A conformance for TypeScript/JavaScript, Python and Rust

## Identity

- Task ID: PX-028
- Milestone: M3; release: BETA
- Requirement: REQ-PX-028; owner label: context-engine; subsystem: context-engine
- Qualification: `QUAL-PX-028` / `PX-E2E-028` — TypeScript/JavaScript, Python and Rust pass the Tier A suite with real headless language services on the fixtures; incremental index latency within budget; competence suite tasks for each language pass at baseline. Negative: a diagnostics parity failure or missing references blocks the tier; a dead language service degrades explicitly rather than faking results.
- Evidence tier: real-system or production-equivalent
- Decision Record: `docs/decisions/DR-M3-003-benchmark-and-conformance-live-model-halves.md` (the competence clause)

## Goal

Earn Tier A for the three languages on evidence: the suites with real language services, an incremental index that stays within a stated latency budget, and a product that says so when a service is not there.

## Existing-code audit

- classification: PARTIAL before this task. PX-027 recorded Tier A passes for `rust`, `python` and `typescript` in `crates/verification/language-tiers.json` from suites that ran `a1_language_service_symbols_and_references` and `a2_diagnostics_parity` against rust-analyzer, pyright and typescript-language-server on the fixture repositories, with a dead service producing a `skip` and no claim. Nothing measured incremental index latency against a budget, and the explicit-degradation rule was proven at the suite level but not for the tools an agent calls.
- production entry points:
  - `crates/verification/language-tiers.json` — the recorded Tier A passes (PX-027), regenerated only by the suite test.
  - `crates/retrieval` — the exact, lexical, symbol and semantic indexes, each refreshed incrementally after a change; the budget is declared in `crates/retrieval/tests/tier_a_latency.rs` because docs/76 asks for "within budget" without a number: one file change is current in all four indexes within 250 ms, p95, on the fixtures.
  - `services/modbit-core/src/tools.rs` — the `lsp.*` tools answer `LANGUAGE_SERVICE_UNAVAILABLE` naming the missing server when no service can be resolved; never a success with an empty answer. The text-level tools keep working on the same file, which is the Tier C behaviour the degradation path lands on (PX-029).
- proof: with rust-analyzer unreachable (a PATH without it and a HOME without `~/.cargo/bin`), `lsp.symbols`, `lsp.references` and `lsp.diagnostics` on the Rust fixture each fail with `LANGUAGE_SERVICE_UNAVAILABLE` naming rust-analyzer, while `search.symbols` and `fs.read` succeed on the same file and the catalog still reports the recorded tier. Twelve single-file changes on each of the three fixtures refresh all four indexes with p50 8 ms and p95 8 ms (rust-cli), p50 7 ms and p95 8 ms (python-service), p50 8 ms and p95 8 ms (ts-webapp) on an Apple M5 Pro, against the 250 ms budget, with every index verified current after each refresh.

## Limitations

- The competence clause — "competence suite tasks for each language pass at baseline" — is the competence baseline of PX-020, which needs a live model. DR-M3-003 records it as the deferred half; PX-020 is BLOCKED on the same record.
- The latency budget is a number this task declares, not one the dossier fixed; it is stated with the measurement so a reader can judge it, and it is generous by design so the assertion measures a regression rather than the machine.
- The Tier A suites themselves are PX-027's evidence and are not re-run here; this task adds what PX-027 did not measure.

## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`):

- `qual_px_028_a_dead_language_service_is_a_typed_failure_not_an_empty_answer`
- `incremental_index_latency_is_within_budget_on_the_tier_a_fixtures`
- `qual_px_027_language_tier_suites_run_on_real_fixtures_and_a_tier_is_only_a_recorded_pass` (PX-027; the Tier A suites and the recorded passes)

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- `ci-run-34565153777.json`: hosted CI, green on macOS, Linux and Windows at `213a89f`
- Commit: `213a89f`
