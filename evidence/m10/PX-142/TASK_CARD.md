# Task Card — PX-142 Primitive restyle to the ZCode recipes with a fidelity gate

## Identity

- Task ID: PX-142 (REQ-PX-142; owner desktop); qualification QUAL-PX-142; Decision Record DR-PX-2026-10-24-016
- Evidence tier: iteration
- Prerequisite: PX-141 (REAL_TESTING; COMPLETE needs the Linux and Windows CI legs). The graph tool refuses any move of this row until PX-141 is COMPLETE, so the row is NOT_STARTED in the graph while the work below is on the branch.

## Existing-code audit (before any edit)

- Callers: every screen renders `@modbit/ui` primitives (`.mb-btn`, menus, dialogs, tabs, tooltip, badge, kbd, rows, trays) and native `input`, `textarea`, `select` and checkbox styled by `legacyCss`; `build.mjs` concatenates `uiCss` and the screens' CSS into `app.css`.
- classification: PRODUCTION-WORKING for Modbit's old recipes; NOT-FOUND for any measurement against another product.
- First missing link: no way to prove a primitive looks like ZCode's. Built: the capture comparator (`packages/design-tokens/src/fidelity.ts`), the declared exceptions (`fidelity-exceptions.ts`), the closed coverage table (`e2e/support/zdl-coverage.ts`), the specimen sheets (`#fidelity/<sheet>/<theme>`, gallery build only) and `e2e/fidelity.spec.ts`.

## What was built

- Primitives and native controls restyled to the measured recipes; Button gained `xs` and `subtle`, Badge `outline`, Menu `tone` and `triggerVariant`, Tooltip `shortcut`, Dialog `description`; an `Icon` over the generated Lucide table. All additive: roles, names, test ids and keys unchanged.
- Gate result on macOS arm64, Electron 44.3.0: every compared property equals ZCode's except the declared exceptions ZDX-01, 02, 03 and 05 (all still used); 128 elements pixel-compared, 78 identical, worst interior mismatch 4.9 percent; the coverage table is closed over the 9 reference sheets.
- Measured, not read: ZCode's real input is 28 px tall; its buttons inherit line height except `sm`; its reference page surface is `--color-background`; its alert dialog is a different recipe from its dialog.

## Verification

- tests: `apps/desktop/e2e/fidelity.spec.ts`, `packages/design-tokens/src/fidelity.test.ts`, the gallery, shell, keyboard, checkpoints and conversation specs (axe in three themes), the full desktop suite (see evidence.json).
- negative proof: `mutation-check.txt` (radius, font size, colour, undeclared deviation, an exception that covers nothing, a stale exception) each fail the gate.

## Limitations

- Linux and Windows legs run in CI only; the pixel comparison runs only at device scale 2 (macOS) and is skipped, with the style comparison still enforced, elsewhere.
- Not E2E_PROVEN; blocked from moving by PX-141 not being COMPLETE.
