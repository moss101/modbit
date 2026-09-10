# Task Card — M3.4 headless LSP diagnostics/symbol bridge

## Identity

- Task ID: M3.4
- Milestone: M3 (Context intelligence)
- Canonical owner: workspace-git / context-engine (`crates/diagnostics` LSP client and server registry; `lsp.*` tools and `LanguageServicePort` in `crates/tools`; per-workspace servers in `services/modbit-core/src/tools.rs`)
- Requirement IDs: docs/18 "Index inputs" (headless LSP symbols/references, diagnostics), "Semantic language services: headless LSP processes normalized into Modbit diagnostics/symbol records"; docs/76 Tier A (language-service symbols, references and diagnostics); docs/43 M3.4; REQ-EV-0020 / REQ-EV-0070 groundwork
- Risk class: high (real language-server processes; untrusted repository content)
- Evidence tier: release-critical
- Release membership: BETA, RELEASE_ZERO (M3)

## Goal

Real language servers driven headlessly over JSON-RPC/stdio — rust-analyzer (rustup component), typescript-language-server with a TypeScript 5 tsserver, pyright — one process per workspace and language, started at first use and killed with the Core; every request re-syncs the file from the policy-checked workspace so results carry the file's content hash and workspace revision; diagnostics wait for the server's analysis to settle (progress tokens, quiet period, pull diagnostics where offered); symbols, references and definitions normalized to root-relative paths. Exposed as `lsp.diagnostics`, `lsp.symbols`, `lsp.references`, `lsp.definition`. A language without a reachable server is `LANGUAGE_SERVICE_UNAVAILABLE`, never a fabricated result.

## Non-goals

Diagnostics in the Context Pack and verification plan inputs (M3.8, PX-004), Tier A promotion itself (PX-027/028 conformance suites), incremental didChange deltas (full re-sync per request), languages beyond the Alpha baseline.

## Existing-code audit

- classification: NOT-FOUND (the diagnostics crate was empty)
- production entry point: `modbit_diagnostics::LanguageServer` (spawn/open/diagnostics/document_symbols/references/definition), `resolve_server`; `LanguagePort` in the Core tool host
- real effector/storage boundary: real rust-analyzer / node processes over stdio on real fixture repositories

## Invariants

- No structural claim without a running server for the language; unavailability is typed.
- Results are bound to the file content hash and workspace revision they were computed from.
- Servers die with the Core (killed on drop) and answer their own requests so they never stall.

## Verification

- `modbit-diagnostics/servers` :: `rust_analyzer_reports_a_seeded_type_mismatch_symbols_references_and_definitions`, `typescript_language_server_reports_a_seeded_type_error_and_resolves_definitions`, `pyright_reports_a_seeded_error_and_resolves_references`, `unsupported_language_is_reported_unavailable_not_faked`
- `modbit-core/surface_protocol` :: `m3_4_headless_language_service_bridge_serves_diagnostics_symbols_references_and_definitions`
- `modbit-tools/pipeline_and_direct` :: `qual_ev_0217_compatibility_matrix_covers_every_registered_tool_with_owner_effect_and_test`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json/log copies alongside
