# Task Card — IMP-EV-0177 Lazy tool/schema context

## Identity

- Task ID: IMP-EV-0177
- Milestone: M3 (context batch)
- Requirement: REQ-EV-0177; owner label: Tool Runtime; subsystem: core-runtime
- Qualification: `QUAL-EV-0177` — Large MCP catalog token benchmark proves lazy behavior.
- Evidence tier: real-system or production-equivalent

## Goal

Hydrate schemas only when relevant after discovery; authorization separate.

## Existing-code audit

- classification: IMPLEMENTED in this batch for the Core's own catalog (was NOT-FOUND); there is no MCP catalog yet — the benchmark is the Core's 28 host tools, and the measured saving is stated, not the docs target
- production entry point: services/modbit-core/src/runtime.rs projection(): deferred tools cost one name in the tool.search description until activated, then carry their JSON schema
- proof: measured on the 28-tool host catalog under review_isolated: the lazy first-turn projection is 10678 bytes (18 tools incl. harness) against 14253 bytes eager (28 tools) in the provider wire shape — about 25% smaller; after discovery lsp.symbols is projected with its parameters schema (the test asserts the schema is present only after activation)

## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`; scripted wire-faithful model server, real Core):

- `qual_ev_0134_deferred_tool_search_activates_without_authorizing_and_hydrates_schemas_lazily`
- `qual_ev_0096_0116_0133_0044_0031_tool_surface_is_compiled_from_support_and_policy` (the projection stays support × policy)

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json/log copies alongside
