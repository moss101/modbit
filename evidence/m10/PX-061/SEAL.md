# PX-061 seal (2026-11-24)

The graph row was REAL_TESTING since 2026-10-17 with one open item: the live-gateway leg. Everything else was proven on the real Core then.

## Live-gateway leg

- Run: live-core 45221342126 on main c5294eb0 (PR 96), gemini-3.8-flash, spend cap 15 USD (owner allows capped live runs).
- Leg record: `benchmarks/live-results/live-core/45221342126/px061-checkpoints.json`, merged to main in PR 251 (c45c62e5). passed=true, 17 of 17 checks ok.
- The run's "Core legs on the live gateway" job concluded failure, but on other rows: PX-059 (fixed in PR 251), PX-109 compaction (Gemini quota, HTTP 429) and PX-118 (model variance, failed closed as designed). This row's leg passed on its own record. Details are in that run's README.
- `live-core-run-45221342126.json` is the run's job list.

## Main CI

- ci run 45265480804 on main ecad24ee (which contains c5294eb0): conclusion success, 19 of 19 jobs green. Captured as `ci-run-45265480804.json`.

## Status

REAL_TESTING → E2E_PROVEN → COMPLETE through `tools/graph.py`, with evidence run:gha-45221342126, run:gha-45265480804, commit:c45c62e5 and the leg record artifact.
