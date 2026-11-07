# Task Card — PX-086 Automations surface and CLI

## Identity

- Task ID: PX-086 (REQ-PX-086; owner desktop); qualification QUAL-PX-086
- Evidence tier: release-critical
- Graph status: NOT_STARTED. graph.py refuses any advance: prerequisites PX-046, PX-058 and PX-084 are not COMPLETE.

## Existing-code audit

- classification: IMPLEMENTED-PARTIAL (surface, CLI and Core paths are wired end to end and run; some acceptance text and the task-card retry and cancel are not closed)
- production path: renderer (apps/desktop/src/renderer/automations) -> preload -> main (automation-ipc.ts) -> Core protocol (automations.proto) -> services/modbit-core/src/automation.rs; CLI apps/cli/src/automation_verbs.rs over the same messages.
- first missing link found: a test run's report said "Nothing it tried needed an approval" although the agent had asked for change.apply and shell.exec. The dry-run profile hides write tools, so the call is refused as TOOL_NOT_VISIBLE before any policy decision, and the report only read policy decisions. Closed in this change: the report lists those refused calls by the tool named, outcome DENIED.

## Verification

- tests: `services/modbit-core/tests/automations_px.rs::px_086_a_test_run_of_a_write_capable_definition_changes_nothing_and_leaves_no_receipt` (now asserts the listed denied effects, no file, no receipt); `apps/desktop/e2e/automations.spec.ts` (4 tests; the dry-run test now expects the denied change.apply entry in the dialog); `apps/cli/tests/automation.rs` (3 tests).
- runs on this change (macOS, local): automations_px 27 passed; cli automation 3 passed; desktop e2e computer-surface + automations 13 passed.
- mutation check: evidence/m10/PX-086/mutation-check.txt

## Limitations (not proven)

- Run on Electron from the build output, not a packaged app (packaging is M10.2).
- Task card scope "retry and cancel" of a single run has no protocol message (only Acknowledge, per-definition and global Kill exist); not built.
- Desktop history is not shown with a failed run (the CLI test covers failed).
- A repository definition shows its hash; the revision display is covered only through the hash/content_changed path.
- Prerequisites PX-046, PX-058, PX-084 are NOT_STARTED in the graph (code exists; closing them is separate work).
