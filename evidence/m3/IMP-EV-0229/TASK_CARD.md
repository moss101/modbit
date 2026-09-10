# Task Card — IMP-EV-0229 Toolsets / capability grouping

## Identity

- Task ID: IMP-EV-0229
- Milestone: M3 (context batch)
- Requirement: REQ-EV-0229; owner label: Tool Runtime; subsystem: core-runtime
- Qualification: `QUAL-EV-0229` — Toolset enablement cannot expose denied tool.
- Evidence tier: real-system or production-equivalent

## Goal

Group discoverable tools for projection but resolve authority in Capability Kernel.

## Existing-code audit

- classification: IMPLEMENTED in this batch (was PARTIAL: tool families existed in the matrix but not in projection)
- production entry point: crates/core-runtime/src/harness.rs toolset_of (namespace grouping used by tool.search's catalog and result lines); services/modbit-core/src/tools.rs visible_specs (profile, lease, kernel)
- proof: toolsets group the deferred catalog for projection only; enabling by discovery cannot expose a denied tool because discovery searches `visible_specs`, which already applies the execution profile, the lease and the kernel — under review_isolated an explicit activate of git.worktree.create returns no match and the git toolset never appears in a projection

## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`; scripted wire-faithful model server, real Core):

- `qual_ev_0134_deferred_tool_search_activates_without_authorizing_and_hydrates_schemas_lazily`
- `qual_ev_0096_0116_0133_0044_0031_tool_surface_is_compiled_from_support_and_policy` (the projection stays support × policy)

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json/log copies alongside
