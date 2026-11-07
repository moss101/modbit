# Task Card — PX-141 ZCode design tokens (Zai Light and Zai Dark, type scale, radius, elevation, motion) with the retained reference capture and the declared accessibility exceptions

## Identity

- Task ID: PX-141 (REQ-PX-141; owner desktop); qualification QUAL-PX-141; Decision Record DR-PX-2026-10-24-016
- Evidence tier: iteration (presentation only; the packaged UI suite on a real Core, axe and the computed contrast test are the proof of record, doc 83)
- Prerequisites: PX-044 COMPLETE, PX-045 COMPLETE, DOC-PX-015 COMPLETE (sealed 22ecca83)

## Existing-code audit (2026-10-24, before any product edit)

- Production caller to effector: `apps/desktop/build.mjs` -> `tokensCss()` (`packages/design-tokens/src/css.ts`) -> `dist/renderer/app.css` -> every rule of `packages/ui/src/styles.ts` and the renderer CSS modules reads `--mb-*` custom properties; `index.tsx` sets `data-theme` from the persisted preference.
- classification now: PRODUCTION-WORKING for Modbit's own palette (PX-044, COMPLETE); NOT-FOUND for a ZCode token layer.
- First missing link: no `--color-*` (ZCode-named) tokens, no `text-ui-*` scale, no radius/elevation/font scale of the record are emitted; the role palette (`palette.ts`) holds Modbit's own blue-accent values and the contrast test (`contrast.ts`, `pairs.ts`) accepts opaque `#rrggbb` only, so a translucent token cannot be tested.
- Existing owners kept: `packages/design-tokens` is the one typed function (AFW-A11); no second token system is added.
- Not touched: main process, preload, CSP, protocol, the high-contrast palette, the theme preference values.

## Plan (vertical slice)

1. `zai.ts`: generated typed table of ZCode's colour tokens as canonical literals, from `evidence/zcode-ui/reference` (generator and exclusions recorded in `tools/zcode-reference/scripts/gen-design-language.mjs`).
2. `contrast.ts`: alpha-aware colour parsing and compositing; `palette.ts`: light and dark roles derived from the table through one explicit role map, the declared exceptions as explicit values; high contrast unchanged.
3. `scale.ts` / `css.ts`: `--ui-font-size`, `--text-ui-*`, radius, shadow, font and `--color-*` emitted by the one function.
4. Tests: unit parity against the capture, contrast pairs in three themes, the exceptions list closed; an Electron spec resolving the emitted tokens in the real app against the capture; the packaged axe suite in three themes; notices and a vendor-mark search of the bundle.
