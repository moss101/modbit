# Task Card — IMP-EV-0134 Deferred tool search

## Identity

- Task ID: IMP-EV-0134
- Milestone: M3 (context batch)
- Requirement: REQ-EV-0134; owner label: Tool Runtime; subsystem: core-runtime
- Qualification: `QUAL-EV-0134` — Search tool catalog then activate; permission still enforced.
- Evidence tier: real-system or production-equivalent

## Goal

Stable core + searchable deferred metadata; discovery does not authorize.

## Existing-code audit

- classification: IMPLEMENTED in this batch (was NOT-FOUND: every visible tool was projected with its schema on every turn)
- production entry point: crates/core-runtime/src/harness.rs (TOOL_SEARCH, CORE_TOOLS/CORE_NAMESPACES, is_deferred, HarnessState.activated_tools); services/modbit-core/src/runtime.rs projection() and handle_tool_search(); domain TaskEvent::ToolsActivated
- proof: the model sees the stable core with schemas plus `tool.search`, whose description names the deferred tools by toolset; a search activates matches and records ToolsActivated (rebuilt after restart); `visible_specs` (profile × lease × kernel) bounds what is discoverable, so a tool the profile denies is neither named nor activatable, and every invocation still goes through the Capability Kernel; a direct call to a visible deferred tool activates it (discovery by use, recorded the same way) so a model that already knows a tool name is not refused

## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`; scripted wire-faithful model server, real Core):

- `qual_ev_0134_deferred_tool_search_activates_without_authorizing_and_hydrates_schemas_lazily`
- `qual_ev_0096_0116_0133_0044_0031_tool_surface_is_compiled_from_support_and_policy` (the projection stays support × policy)

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json/log copies alongside
