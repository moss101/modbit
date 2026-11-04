# Task Card — PX-194 Agent workspace density

## Identity

- Task ID: PX-194 (REQ-PX-194; owner desktop); qualification QUAL-PX-194; Decision Record DR-PX-2026-11-03-019
- Evidence tier: iteration (presentation only)
- Prerequisites: PX-193, DOC-PX-018

## Existing-code audit (before any edit)

- classification: PRODUCTION-WORKING conversation, composer and apps panel (PX-047, PX-048, PX-054); first missing link: always-visible turn footers and wrapping status row.

## What was built

- User message as an editor-surface bubble; step groups one-line disclosure rows; completed turn footers at opacity 0 until hover or focus within (failed and interrupted always shown), on one line; the message's hover-only actions overlay its corner; centred 840 px column; composer radius 12 on the editor surface with the 3:1 edge; docked trays on the sidebar surface; one-line status row; information banners one caption line; apps-panel headings sentence-case captions and explanatory text caption-size. Before and after screenshots in light and dark are in `before/` and `after/` (supplemental record, not proof).

## Verification

- tests: `apps/desktop/e2e/awl.spec.ts`, `apps/desktop/e2e/conversation.spec.ts`, `apps/desktop/e2e/composer.spec.ts`, `apps/desktop/e2e/checkpoints.spec.ts`; the whole desktop e2e suite against a real Core (evidence.json). 7de50046 extends `awl.spec.ts` to a scripted task with an edit, a terminal and an inline approval, asserting each density clause with a retained mutation (`awl-gate-local.log`).

## Limitations

- CI run 41706960923 (PR #168 head 8d988dbe) is green on macOS, Linux and Windows for the rust, node and desktop E2E jobs, which closes the CI legs. That run predates the merges of main (c43db71f, Rust and dossier only, no renderer file) and the PX-156 test margin (d2c3b904). On those later heads the failures were outside this row's code: px_156 timing on Ubuntu (fixed), and on macOS fix_17 and the actuator ax_fixture (runner GUI session: human_active true, no key arrived) with macOS desktop E2E cancelled before its limit; main's own run showed the same E2E cancellation, on a Rust job that had not finished when this was written. Native window chrome and translucency stay with PX-094. Colours are Modbit's own (doc 65 section 2): they are near the reference product, not identical.
