# Task Card — PX-193 Agent workspace shell and controls

## Identity

- Task ID: PX-193 (REQ-PX-193; owner desktop); qualification QUAL-PX-193; Decision Record DR-PX-2026-11-03-019
- Evidence tier: iteration (presentation only)
- Prerequisites: PX-192, DOC-PX-018

## Existing-code audit (before any edit)

- classification: PRODUCTION-WORKING shell (PX-045) in the framed-panel language; first missing link: navigation split across the head and the top bar, bare buttons primary-filled.

## What was built

- Unframed panes (sidebar, chrome and editor surfaces, 1 px dividers, no radius or gap, 4 px splitter), 40 px top bars, agent list 260 px; the navigation entries (New task, Search, Fleet board, Worktrees, Automations, Customize, Dashboard) are 30 px rows in `agent-region.tsx` with their test ids (Search opens the existing palette); the top bar keeps title, location, overflow and panel toggle; a bare button and `Button` secondary/subtle are the 8 % fill, primary the accent, new `outline`, ghost muted, pill tabs, 12 px menus with 30 px rows; six Lucide shapes added through the generator. Geometry assertions changed only as the record's section 10 names (264 -> 260 and the derived 1016/592/636/312/296; 48 -> 40).

## Verification

- tests: `apps/desktop/e2e/awl.spec.ts`, `apps/desktop/e2e/shell.spec.ts`, `apps/desktop/e2e/gallery.spec.ts`, `apps/desktop/e2e/agents.spec.ts`, `apps/desktop/src/renderer/shell/shell.test.ts`; the whole desktop e2e suite against a real Core (evidence.json). 7de50046 extends `awl.spec.ts` to every size and theme the acceptance names and the primary, outline and ghost fills.

## Limitations

- macOS arm64 only so far; Windows and Linux are CI legs not yet run. Native window chrome and translucency stay with PX-094. Colours are Modbit's own (doc 65 section 2): they are near the reference product, not identical.
