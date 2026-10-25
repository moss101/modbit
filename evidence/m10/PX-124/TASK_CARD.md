# Task Card — PX-124 WebMCP rung

## Identity

- Task ID: PX-124 (REQ-PX-124; owner browser); qualification QUAL-PX-124
- Evidence tier: release-critical (a new tool source on a security boundary)

## Existing-code audit

- classification at start: NOT-FOUND (no WebMCP code). First missing link: discovery of page-declared tools in the host.

## What was built

- `crates/browser/src/page_tools.rs` (wire records, handle, annotation reading, reserved names), `HostRequest::PageTools` / `CallPageTool`
- `crates/tools/src/browser_page_tools.rs` (`browser.page_tools`, `browser.page_tool`), taxonomy codes, tool-matrix entries
- `services/modbit-core`: lease/generation fencing for the call, cloud answers UNSUPPORTED, `PAGE_TOOL_DECLARATION_REFUSED` security event, a page-tool call counts as progress
- `apps/desktop/src/main/browser-webmcp.ts` + `browser.ts`: the shim, per-frame discovery from the browser's frame tree, execution in the declaring frame, staleness refusal

## Verification

- `services/modbit-core/tests/px_124_page_tools.rs` (real Core, Kernel, approvals, log; stand-in host): 1 passed
- `apps/desktop/e2e/browser-webmcp.spec.ts` (real Chromium through the host, fixture page, cross-origin iframe, real Core, scripted model): 1 passed
- `apps/desktop/src/main/browser-webmcp.test.ts` (shim in a vm, binding, bounds): 6 passed; `pnpm test` 323 of 323
- `cargo test -p modbit-tools -p modbit-browser` all pass; clippy `-D warnings` on browser, tools, core all targets: exit 0
- mutation checks: `mutation-check.txt`

## Limitations

- A page's read-only annotation is its own claim (docs/22 records the accepted risk).
- Read-only calls leave no effect receipt (only effectful calls do); the read is on the log as `ToolCallProposed` with the result's provenance in the tool result.
- The shim is injected after the page's first script in an out-of-process frame that attaches late; a page that feature-detects once at the very start may miss it (the fixture polls). Native `navigator.modelContext`, if a future Chromium ships it enabled, is not read: the host's shim only defines the interface when the browser has none.
- Not built: cloud browser, the PX-115 lazy-discovery surface, the rung record on `browser.act`. Packaged (not dist) app not run. COMPLETE is gated on PX-120 (NOT_STARTED), PX-073 (REAL_TESTING) and DOC-PX-013 prerequisites in the graph.
