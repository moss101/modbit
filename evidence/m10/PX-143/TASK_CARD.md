# Task Card — PX-143 Shell, conversation, composer and remaining screens in the ZCode panel language

## Identity

- Task ID: PX-143 (REQ-PX-143; owner desktop); qualification QUAL-PX-143; Decision Record DR-PX-2026-10-24-016
- Evidence tier: iteration
- Prerequisite: PX-142 (see its card): the graph refuses any move of this row until PX-142 is COMPLETE.

## Existing-code audit (before any edit)

- Callers: `shell-frame.tsx` lays out the agent list, the centre region and the apps panel from `computeLayout`; screens carry their CSS in `styles.ts`, `workspace-css.ts`, `controls-css.ts`, `composer-css.ts` and the project, automation and computer modules, all already written against `--mb-*` roles.
- classification: PRODUCTION-WORKING (AFW-A01..A10 geometry 40/260); NOT-FOUND for the framed-panel language.
- First missing link: the regions were flat columns separated by rules; no region was a framed panel.

## What was built

- Shell: independent framed panels (12 px radius, hairline, page surface) over the window-root surface, 4 px apart; header 48 px with a half-strength rule; agent list default 264 px (210 to 400 kept); the resize indicator is ZCode's 2 px line. A wrapper element `shell-frame` inside `region-conversation`; test ids, roles and layout maths unchanged.
- Screens: literal sizes and radii moved to the `text-ui-*` scale and the radius hierarchy; legacy cards read the raised surface; the user message is ZCode's secondary-fill bubble; the composer shell is the `2xl` input shell; list rows `lg` with `surface-hover` and `selected`; the renderer's own icons are the Lucide table.
- Geometry assertions replaced (DR section 5.3): `shell.spec.ts` (264, 48 and the nudge numbers derived from them), `gallery.spec.ts` (264, 592, 1016), `shell.test.ts` (264 and derived). Test support: the shared `accessible()` waits for chained colour transitions and reports measured colours.
- Accessibility findings the new look produced, fixed (not excepted): danger chip 4.47:1 on a tinted card (chips composite over the raised surface), danger button hover 4.03:1 (hover moves toward the text colour, ZDX-05), off-screen gallery rows stuck mid-transition (the existing QA-only `content-visibility` rule now covers the conversation gallery).

## Verification

- tests: the whole desktop e2e suite against a real Core (evidence.json), `shell.spec.ts` geometry, axe in light, dark and high contrast on every gallery state and region.
- supplemental record (not proof): `before/` and `after/` screenshots of the shell, conversation rows, controls and primitives in light and dark, from the same gallery sections of `origin/main` and of this tree.

## Limitations

- Window chrome, translucency and per-OS radii belong to PX-094 (ZDX-07). Windows and Linux are CI-only. Screens whose only change is inherited from the tokens (Review, Settings, dialogs, automations, worktrees) are covered by the axe and geometry specs and the gallery screenshots, not by per-screen ZCode comparison: ZCode has no counterpart for them.
