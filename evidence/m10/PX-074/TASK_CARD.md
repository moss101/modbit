# Task Card — PX-074 Computer-control desktop surface

## Identity

- Task ID: PX-074 (REQ-PX-074; owner desktop); qualification QUAL-PX-074
- Evidence tier: release-critical
- Graph status: NOT_STARTED. graph.py refuses any advance: prerequisites PX-058, PX-070 and PX-071 are not COMPLETE.

## Existing-code audit

- classification: IMPLEMENTED-PARTIAL (the surface is wired through preload, main and the Core to the fixture actuator process; the real macOS helper path is not exercised by the desktop specs)
- production path: renderer (apps/desktop/src/renderer/computer) -> preload -> main (computer-ipc.ts) -> Core ComputerStop and presence -> actuator RPC.
- first missing link found: no test of the QUAL "renderer-kill case". Closed in this change: a spec kills the renderer while an input hangs inside the actuator; the replacement window shows CONTROLLING from the Core with the same Core pid, Stop from it ends the hang and shows USER_ABORTED.

## Verification

- tests: `apps/desktop/e2e/computer-surface.spec.ts` (9 tests, one new); run on this change: 13 passed together with automations.spec.ts (macOS, local, fixture actuator).
- mutation check: evidence/m10/PX-074/mutation-check.txt

## Limitations (not proven)

- Real macOS machine with the signed helper and real System Settings permission grants: not run (permissions are read from a file by the fixture actuator).
- Packaged app: not run.
- Presence in a second window: the app has a single window, so there is no product path to a second window; only the replacement window after a renderer kill is proven.
- Prerequisites PX-058, PX-070, PX-071 are NOT_STARTED in the graph.
