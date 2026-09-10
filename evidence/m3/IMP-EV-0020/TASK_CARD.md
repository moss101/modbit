# Task Card — IMP-EV-0020 Pull-based diagnostics

## Identity

- Task ID: IMP-EV-0020
- Milestone: M3 (context batch)
- Requirement: REQ-EV-0020; owner label: Context Engine; subsystem: context-engine
- Qualification: `QUAL-EV-0020` — High-churn editor/test fixture proves no unsolicited diagnostic prompt traffic.
- Evidence tier: real-system or production-equivalent

## Goal

Diagnostics are fetched after settle/on demand, not injected continuously.

## Existing-code audit

- classification: IMPLEMENTED by M3.4 (was NOT-FOUND)
- production entry point: crates/diagnostics/src/lsp.rs diagnostics() (settle on progress, then pull textDocument/diagnostic); lsp.diagnostics tool
- proof: diagnostics are fetched on demand after the server's analysis settles (pull when the server offers it); nothing in the runtime injects diagnostics into prompts — the only path is an explicit tool call, so a high-churn session produces no unsolicited diagnostic traffic

## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`):

- `m3_4_headless_language_service_bridge_serves_diagnostics_symbols_references_and_definitions`
- `rust_analyzer_reports_a_seeded_type_mismatch_symbols_references_and_definitions`
- `pyright_reports_a_seeded_error_and_resolves_references`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json/log copies alongside
