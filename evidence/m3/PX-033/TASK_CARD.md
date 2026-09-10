# Task Card — PX-033 Normalized test reports and failing-check identity

## Identity

- Task ID: PX-033
- Milestone: M3 (moved from M2 by DR-M2-002: the real pytest proof needed the python-service fixture)
- Requirement: REQ-PX-033 (related REQ-EV-0107, REQ-EV-0068, REQ-EV-0017); owner: verification
- Qualification: QUAL-PX-033 — real vitest, pytest and cargo runs on the Alpha fixtures produce TestReports with STRUCTURED parser confidence, stable check_ids, per-check status, location, error class and message fingerprint, and a raw OutputRef; the failure_signature of a seeded failure is identical across two runs at the same revision; the model receives failing CheckResults first with declared truncation and the raw log retained; a configured command with no structured report is HEURISTIC
- Evidence tier: real-system or production-equivalent

## Goal

`test.run`/verification stages return `TestReport`s with `CheckResult`s from the runner adapters with STRUCTURED or HEURISTIC confidence; `failure_signature` derived only from normalized results; bounded failure evidence to the model with the raw log retained (docs/64 §2).

## Existing-code audit

- classification: IMPLEMENTED by M2.8 for cargo and vitest and the junit/configured-command parsers; COMPLETED in this batch with the real pytest run (was PARTIAL: the pytest adapter was proven only on a stored junit file)
- production entry point: crates/verification/src/adapters.rs (cargo libtest, vitest JSON, pytest junit, configured command), plan.rs `derive` (pytest from pytest.ini/pyproject; `python` on Windows), report.rs `failure_signature`; the runtime's failing-first observation (M2.8)
- proof: on the python-service fixture (now carrying `pytest.ini`) a real `pytest --junitxml` run yields a STRUCTURED report whose checks carry `pytest:` ids, path, symbol, status, error class and fingerprint, with the raw log stored as an artifact; two runs at the same revision produce identical failure signatures for the seeded failure; cargo (rust-cli) and vitest (ts-webapp) are proven by the M2.8 tests; a configured command without a structured report is HEURISTIC

## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`; the rust job installs pytest 8.4.2):

- `pytest_fixture_produces_structured_reports_with_stable_failure_signatures_when_pytest_is_installed`
- `vitest_fixture_parses_structured_reports_when_vitest_is_installed`
- `pytest_junit_and_configured_command_adapters`
- `m2_8_verification_engine_gates_completion_on_real_cargo_fixture` (cargo; failing CheckResults first with the raw ref)

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json/log copies alongside
