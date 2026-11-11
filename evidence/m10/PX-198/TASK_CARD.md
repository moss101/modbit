# Task Card — PX-198 Measured workspace shell: the list, bars, rows, column and rail at the measured sizes, the conversation on the lightest surface, and a density setting

## Identity

- Task ID: PX-198 (REQ-PX-198; owner desktop); qualification QUAL-PX-198 / PX-E2E-198; Decision Record DR-PX-2026-11-11-021 (MWS-01, MWS-02, MWS-03; AFW-A12, A13, A14 unchanged)
- Evidence tier: iteration (sizes and one surface mapping move; the layout preference gains one field read with a default; no effect, canonical persistence, policy, protocol, preload or security boundary changes)
- Prerequisites: PX-197, DOC-PX-020. The record and the row were not on main when this was built (PR #196); nothing here sets a graph status.
- Branch `wip/desktop-2-shell-list`, one change with PX-199.

## Existing-code audit (before any edit)

- classification: IMPLEMENTED-PARTIAL. Traced from the production caller to the pixels: `build.mjs` concatenates `tokensCss()` (packages/design-tokens) with the component stylesheets into `app.css`; `Shell` reads `loadUiPrefs()` and hands `ShellFrame` the widths `computeLayout` derives from `GEOMETRY`. The token pipeline, the role map, the WCAG pairs test and the AWL gate (`e2e/awl.spec.ts`) all worked.
- first missing link: `GEOMETRY` held only the top bar, the list's three widths, the rail, the centre, the panel and the window. Every other size the record names was a literal inside a component: navigation rows 28 (`styles.ts`), the panel's tab bar and the Changes head 40, the conversation column 640 in four places (`workspace-css.ts` twice, `composer-css.ts`, `controls-css.ts`), the composer's radius 12, and the list's row heights 50, 28 and 30 as constants in `agents/list-model.ts`. `--ui-font-size` was fixed at 13 and nothing selected a density. `UiPrefs` had no density field and Settings, Appearance had Theme and Zoom only.
- surfaces: `.shell-frame` read `--color-background` (chrome) and `.conv-user` read `--color-panel` (editor), the mapping MWS-02 turns round. The role map itself needed no change.

## What was built

- **Tokens (packages/design-tokens).** `GEOMETRY`: list 288 (200 to 420), bar 45, row 32, row gap 2, row radius 8, row inset 8, two-line row 46, group heading 38, list foot 48, column 768, column text 736, composer radius 14. The centre minimum 424, the single-pane threshold 448, the rail 40, the panel minimum 384 and the window minimum 900 by 600 keep their numbers. All of it is emitted as `--mb-*` variables by `sharedVariables()`.
- **Density (MWS-03).** `DENSITIES`, `COMPACT`, `densityVariables()` and `densityMetrics()`. Comfortable is the shared block: `--ui-font-size` 14, the transcript 15 on 21. Compact is one block, `:root[data-density="compact"]`, that overrides `--ui-font-size` (13), each type role's line (chrome 13 on 18, the transcript 14 on 20), `--mb-top-bar` (40), `--mb-row` (30), `--mb-row-two-line` (44) and `--mb-group-head` (32). No component stylesheet selects on the density. The conversation's own density (compact, balanced, detailed) is untouched.
- **One surface mapping (MWS-02).** Three variables, `--mb-surface-conversation` (editor), `--mb-surface-list` (sidebar) and `--mb-surface-fill` (chrome). The centre frame and the apps panel read the first, the list the second; the person's message, code blocks, the approval's command, attachment tokens, the terminal well and the conversation's select read the third. No colour input and no role value changed.
- **Shell.** Top bar, the list's head, the panel's tab bar and the Changes head at `--mb-top-bar`, with a 1 px rule at 8 % under the top bar; navigation rows at `--mb-row`, `--mb-row-gap` apart, radius and inset from tokens; the conversation's text at `--mb-column-text` and the notice, approval tray and composer at `--mb-column`; the composer's radius from `--mb-composer-radius`. The composer's structure is unchanged (PX-200).
- **Text sizes.** Every `font-size`, `line-height` and `font` in the component stylesheets is a token (the Changes pane's 11, 11.5 and 12 px literals, the avatar's 11, and six literal line heights were moved).
- **Preference and control.** `UiPrefs.density` (`modbit.ui.v1`, default Comfortable, validated on read); `Shell` sets or removes `data-density` on the root; Settings, Appearance has one Density control (`settings-density-comfortable`, `settings-density-compact`).
- **Pins moved, none deleted:** `packages/design-tokens/src/index.test.ts`, `renderer/shell/shell.test.ts`, `e2e/shell.spec.ts`, `e2e/awl.spec.ts`, `e2e/gallery.spec.ts`, `e2e/composer.spec.ts` (list below).

## Verification

- Unit: `@modbit/design-tokens` 13 of 13, `@modbit/ui` 11 of 11, `@modbit/desktop` 363 of 363; `pnpm typecheck` clean. New: the measured geometry and what the record keeps; Compact as overrides of shared names only; the surface mapping with the role map's values unchanged; a stored list width inside 200 to 420 kept (210, 226, 260, 333, 400); density stored, defaulted and separate from zoom; `renderer/measured-css.test.ts` on the stylesheet text.
- Real app, real Core (`e2e/awl.spec.ts`, two tests): at 1280, 1000 and 900 wide, in light, dark and high contrast, in both densities: the list 288, bars 45 or 40, navigation rows 32 or 30 and 2 apart with radius 8 and inset 8, chrome text 14 or 13 on 18, the emitted size variables equal to the token source, the surfaces on their roles, axe clean. With a scripted task: the transcript 15 on 21 or 14 on 20, pane heads at the bar's height, the column 768 with its text at 736 and inset 16.
- `e2e/shell.spec.ts`: the splitter's 288, 200 and 420; Compact chosen with the real control, the density and a list width of 216 surviving an app restart, the zoom untouched, and Comfortable restored.
- Retained mutations, each asserted to fail: a list at 260, a bar at 40 (top bar and the list's head), a column at 640, the old 840 column, navigation rows at 28, rows 1 apart, 13 px chrome text in Comfortable, a 12 px composer, the conversation back on chrome, the message back on the editor surface, the old 14 on 22 transcript, a 40 px pane head, text that is not inset, a 48 px bar, a ZDL primary, a black bare button. In `measured-css.test.ts`: a size written inside a component (four ways of writing it), a rule selecting on the density, and each measured rule with its number or surface written in place. The contrast test still fails on a lowered token (`index.test.ts`, unchanged).
- Whole desktop suite, local, macOS arm64, real Core, scripted models, on the final code: 198 passed, 0 failed, 2 skipped (the live-provider specs). Two earlier runs and the run on untouched main are in evidence.json; results by test in `desktop-e2e-full-results.txt`.
- Screenshots of the running app at 1440 by 900 (`after/`), taken with a throwaway Playwright script on `e2e/support/ui-harness.ts`: home and an open task in light and dark and in both densities, home in high contrast, the approval docked, the Changes pane in both densities, the Density control. `boards/` holds the approved boards rendered offline at the same size (their icon font and webfont are not loaded, so icons show as clipped ligature text).

## Pins moved with their fact

- `index.test.ts`: `--ui-font-size` 13 to 14; chrome line ratio 1.3846 to 1.4286; body line 1.5714 to 1.4; `--mb-top-bar` 40 to 45; `conversationType(100)` 14 to 15 on 21; `GEOMETRY.topBar` 40 to 45; `GEOMETRY.listDefault` 260 to 288.
- `shell.test.ts`: list 260 to 288 and centre 1020 to 992; panel 596 to 568; clamp 210 and 400 to 200 and 420; the rail and stacking edges recomputed from 288; stored keys gain `density`; hostile width clamps to 420.
- `shell.spec.ts`: list 260 to 288, bar 40 to 45, splitter 260, 210, 400, 308, 292, 226 to 288, 200, 420, 336, 320, 216.
- `gallery.spec.ts`: list 260 to 288, centre 521 to 493 and 1020 to 992, limits 400 and 210 to 420 and 200.
- `composer.spec.ts`: composer never wider than 640 to 768, wider than 560 to 688; its height check is unchanged.
- `awl.spec.ts`: bar 40 to the density's bar; navigation rows 28 to the density's row; composer radius 12 to 14; column 640 to 768 with text at 736; panel tabs 40 to the bar; the body-text check `13px` moved into the per-density check; the user message's surface from editor to chrome.
- Moved, not deleted: the 840 px column mutation now runs with the panel closed (`columnDrift`), where the centre is wide enough for the cap to be what holds the column.

## Differences from the boards (what I can see in `after/` beside `boards/`)

- Home (board Main): the board's home has no top bar, a product name, a location switch, a composer in a tray with the environment strip under it, Waiting on you and Get started. The app keeps the 45 px bar with "New task" and PX-195's centred composer in a 640 column. Home is MWS-12, a later row.
- Task view (board Task-Working): the board's bar holds the title's menu and one toggle per pane, and a details rail is at the right; the app's bar holds the title, the state pill, the branch, the details overflow and the panel toggle. Those are MWS-05 and MWS-09. The board's message is a right-aligned bubble at most 85 % wide with no edge; the app's is the full text width with a hairline. The board's composer is one line with the model, run mode and ring under it; the app's is still two rows (PX-200), now at 768 with radius 14.
- The fill of the person's message is lighter in the app (AWL's chrome, #f6f6f6 on #fdfdfd) than on the board (#F0F0F0 on #FCFCFC), because the record keeps AWL's inputs. In dark the app's fill is darker than its surface (#131313 on #1b1b1b); the board's dark puts the lightest grey on the fill.
- Icons are Lucide at 16 px; the board's stand-in icons are drawn at 20. Control heights stay 20, 24, 28 and 32; the board's stylesheet draws a large button at 40 (32 in Compact).
- Type is the system stack, not the board's webfont, so line widths differ.
- Compact (board Split): bars 40, rows 30, chrome 13 and transcript 14 on 20 match. The board draws Compact task rows 1 apart; the app keeps 2 in both densities (the record names no Compact gap).

## Differences from the Decision Record

- AFW-A07's "one line that grows to 40 % of the window's height" is not built here: the brief leaves the composer's structure to PX-200. Radius 14 and the 768 cap are.
- The details rail's 320 and 280 and the pane's 280 by 100 minimum (section 5.1) are not built: PX-201 and later rows.
- QUAL-PX-198's mutation "a size written inside a component" is a unit test on the stylesheet text (`measured-css.test.ts`), not an e2e mutation: a computed style cannot tell a literal from a token.
- "Every size comes from a token" holds for every text size, the bars, rows, columns and the composer's radius. Paddings, icon sizes, the 22 px avatar and the preview card's 320 width are still literals.
- Home's 640 column is unchanged (MWS-12).

## Limitations

- The proof is local (macOS arm64) until CI has run on the pull request; the suite's result and CI are recorded in evidence.json.
- The owner has not yet seen these screenshots beside the boards, which the record makes an exit criterion. The row is not COMPLETE and no graph status was set.
- Linux and Windows draw classic scrollbars; nothing in this row was looked at on them by eye.
