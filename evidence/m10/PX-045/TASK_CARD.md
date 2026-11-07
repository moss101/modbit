# Task Card — PX-045 Agent-first shell: three regions, top bar, apps-panel host, status row, palette and shortcut registry

## Identity

- Task ID: PX-045 (REQ-PX-045; owner desktop); qualification QUAL-PX-045
- Evidence tier: release-critical
- Closed to COMPLETE on 2026-10-24 after the tests below ran in CI.

## Existing-code audit

- classification now: PRODUCTION-WORKING
- new in this closing: #126 added the 1000 x 700 / 900 x 600 geometry test and the Settings-with-a-running-task test; the full desktop suite then ran green on main

## Verification

- tests: `apps/desktop/e2e/shell.spec.ts::PX-045: geometry at 1280 x 800, keyboard-resizable splits inside 210..400, persisted across a restart`, `apps/desktop/e2e/shell.spec.ts::PX-045: below a 448 centre the shell is single-pane with the 40 px rail; the list can also be collapsed`, `apps/desktop/e2e/shell.spec.ts::PX-045: at 1000 x 700 and 900 x 600 the list keeps 260, the top bar 40 and the centre stays above the single-pane threshold with nothing overflowing`, `apps/desktop/e2e/shell.spec.ts::PX-045: opening Settings with a running task neither stops nor alters the task`, `apps/desktop/e2e/shell.spec.ts::PX-045: the command palette and the shortcut registry by keyboard alone; Escape closes the innermost layer; focus returns`, `apps/desktop/e2e/shell.spec.ts::PX-045: the apps panel appears for a task with an artifact, within the geometry; per-task state and width persist; it stacks when it cannot fit`, `apps/desktop/e2e/states.spec.ts (PX-023 screen states, same suite and run)`, `apps/desktop/src/renderer/shell/shell.test.ts (layout clamp, registry conflicts)`
- re-run: CI run 40018683027.

## Limitations

- The suite drives the built Electron app against a real Core with a scripted model, not a signed packaged installer (M10.2 owns signing). The QUAL mutation checks (double-registered shortcut, unclamped panel ratio) rely on shell.test.ts and the existing conflict test; no new mutation run was retained. No gate attested.

## Evidence

- `evidence.json` and `ci-run-40018683027.json` in this directory
