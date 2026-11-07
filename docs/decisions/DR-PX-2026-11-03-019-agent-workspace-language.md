---
id: DR-PX-2026-11-03-019
title: The desktop renderer adopts the agent workspace language: the reference product's layout, geometry and density as doc 65 records them, in Modbit's own neutral tokens, superseding the ZCode design language
status: accepted
date: 2026-11-03
supersedes: scoped — DR-PX-2026-10-24-016 sections 2 (goal ZDL), 5.1 rows ZDL-01 to ZDL-06, 5.3 and 6 (the fidelity gate); in doc 65 the numbers of AFW-A03, AFW-A04, AFW-A05 and AFW-A07 return to the values of section 5.3 below and the provenance clause of AFW-A11 applies again to every source; nothing else
approved_by: owner, chat instruction of 2026-11-03 ("can you redesign frontend of this application exactly exactly like cursor - first share an image before we approve", then, after the mockup and a side-by-side capture of the running app, "Yes, go ahead")
---

# DR-PX-2026-11-03-019 — Agent workspace language for the desktop renderer

> **Accepted 2026-11-03** on the owner's chat instruction, after the owner saw a mockup in light and dark and a capture of the running app beside it. The approval covers the layout, geometry, density and control recipes below. It does **not** cover any relaxation of AFW-A12 (WCAG 2.2 AA), AFW-A13 (themes) or AFW-A14 (reduced motion, forced colours), and it does **not** cover the clean-room posture of doc 65 section 2: "exactly like" the reference product is read as its layout, proportions and density as doc 65 records them, never its colours, theme file, icons, fonts, logo, brand colour or strings. A change that needs any of these stops for the owner.

## 1. Trigger and evidence

The owner asked for the frontend to be redesigned "exactly like" the Cursor reference product and asked to see an image first. A mockup of the agent window (agent list, conversation, apps panel with Changes) was rendered in light and dark at 1440 x 900; the running app on `origin/main` (`3207cb21`, ZCode design language of DR-PX-2026-10-24-016, real Core, scripted model) was captured beside it. The owner approved the mockup. The capture showed what makes the current renderer read as a console rather than a workspace, and almost none of it is the palette:

| Observed in the running app (1440 x 900, light and dark) | Cause in the renderer |
|---|---|
| Every action looks primary: Refresh, Accept, Reject, Copy answer, Fork from here, Queue | a bare `<button>` takes the solid primary fill (`legacyCss`, ZDL default button) |
| Three framed cards with gaps instead of one window | ZDL-06 independent framed panels, 12 px radius, 4 px gaps |
| The status row wraps into a column of text five lines tall | `.statusrow` items wrap; no single-line rule |
| Each turn shows timing, token counts, checkpoint, copy and fork controls permanently | turn footer always visible |
| The Changes app opens with three explanatory paragraphs before the diff | prose at body size and full contrast |
| Navigation is a row of outlined buttons (Fleet board, Worktrees, Automations, Customize, Dashboard) split over two bars | entries spread across the agent-region head and the top bar |
| The centre column keeps about 30 % of the window with the apps panel open | the panel takes its 0.5 ratio from the window, not the content width |

The reference product's layout facts used here are the ones doc 65 already records from the clean-room teardown (agent list 260, top bar 40, rows 30 with radius about 8, centre at least 424, panel ratio 0.5, composer cap 840, chrome type 13/18, the neutral alpha ladder). The doc 65 numbers that DR-PX-2026-10-24-016 replaced with ZCode's are restored; no new measurement of the reference product is made or needed.

## 2. Goal and exit criteria (the executable goal)

**Goal AWL.** The desktop renderer presents the agent workspace language of section 5 in light and dark, with Modbit's information architecture, behaviour, test ids, accessible names, key maps and accessibility floor unchanged. It is done when all of the following hold on one commit, each proven by a retained artifact:

1. **Tokens.** The emitted theme variables equal the tables of section 5.1 in light and dark, the high-contrast theme is unchanged, and the computed contrast test covers every pair in all three themes (PX-192).
2. **Shell and controls.** The shell is one window of unframed panes with the geometry of section 5.3; a bare button and every `Button` take the section 5.2 recipes; navigation sits in the agent list (PX-193).
3. **Density.** Conversation, composer, apps panel, banners and status row follow section 5.4 (PX-194).
4. **No regression.** The desktop unit suite and the desktop e2e suite pass against a real Core with the only changed assertions being those section 10 names; `axe` passes on every region in light, dark and high contrast.
5. **Governance.** PX-192..194 reach COMPLETE through `graph.py` with run and commit evidence; the manifests are resealed; no release gate is attested.

Executable form: `python3 tools/graph.py goal RELEASE_ZERO` lists PX-192..194 after DOC-PX-018; the specs named in QUAL-PX-192..194 are the proof.

## 3. Options

| Option | Verdict |
|---|---|
| A. Copy the reference product's palette, icon font and theme values to get pixel identity | **Rejected.** Doc 65 section 2 and AFW-A11 forbid it and the owner accepted that posture on 2026-10-03; the EULA of the reference product bars reverse engineering. |
| B. Keep ZCode's tokens and only move the layout | **Rejected.** ZCode's primary is solid black and its surfaces are framed panels; the approved mockup is an achromatic three-surface window with a blue accent and hairline panes. |
| C. Modbit's own neutral tokens per AFW-A11, the doc 65 geometry, and density rules for the screens; same DOM, behaviour and test ids | **Chosen.** Smallest change that yields the approved image; keeps the e2e suite and the Core contract. |
| D. Rewrite the renderer's components | **Rejected.** The structure is sound; the gap is presentation. |

## 4. Provenance and licence

- Every colour in section 5.1 is Modbit's own: chosen for this record from the AFW-A11 constraints (one base, the alpha ladder, three achromatic surfaces) and verified by the computed contrast test. None is a value of the reference product or of ZCode.
- The `--color-*` custom-property names that DR-PX-2026-10-24-016 introduced are kept as internal variable names (they are generic words: background, panel, foreground, border); their values are this record's. The ZCode notices in `third-party/zcode-ui` stay while any recipe of PX-142 remains in `packages/ui`; the Lucide path table (ISC) stays (ZDL-07 is not superseded).
- The fidelity gate of DR-PX-2026-10-24-016 section 6 is retired: it measures the renderer against ZCode, which is no longer the target. Its capture stays retained under `evidence/zcode-ui/reference/` as the record of PX-141..143.

## 5. Specification

### 5.1 Tokens (AWL-01 to AWL-04)

| ID | Rule | Light | Dark |
|---|---|---|---|
| AWL-01 | Three achromatic surfaces and one base. `chrome` is the window root, the top bars and the conversation; `sidebar` is the agent list; `editor` is the apps panel, cards, the composer, menus, popovers and dialogs. | chrome `#f6f6f6`, sidebar `#f0f0f0`, editor `#fdfdfd`, base `#171717` | chrome `#131313`, sidebar `#181818`, editor `#1b1b1b`, base `#ececec` |
| AWL-02 | The neutral ladder of AFW-A11 from the base, composited over the surface it sits on: text 100 / 74 / 60 / 36 %, icon 100 / 66 / 52 / 28 %, fill 20 / 14 / 8 / 6 / 4 %, stroke 20 / 12 / 8 / 4 %. Muted text is 60 % only where it reaches 4.5:1 on its surface, else the darkest step that does; 36 % is disabled and decorative only. Hover is fill 6 %, selected and pressed fill 8 %; decorative edges are stroke 12 % (cards, dialogs, menus) and 8 % (pane dividers, rules); interactive control edges stay at 3:1 (ZDX-03 carried over). | | |
| AWL-03 | One blue accent for the primary action, selection edges, links and the focus ring; status hues for ok, warn, danger and info as text at 4.5:1 on every surface and as tints at 12 %; mode hues keep AFW-D04 meanings. | accent `#145fb4`, on-accent `#ffffff` | accent `#64a8f0`, on-accent `#10151c` |
| AWL-04 | Type: chrome 13/18 (`--ui-font-size` 13 px), caption 12/16, conversation body 14/22, title 14/20 at 600, heading 17/24, mono 12/20; system font stacks unchanged; radii 2 / 4 / 6 / 8 / 12 / 14 / full; control heights 20 / 24 / 28 / 32; motion and z-layers unchanged; press scale 0.98 on buttons. | | |

The high-contrast theme and the ZDX-04 to ZDX-06 and ZDX-09 rules of DR-PX-2026-10-24-016 carry over unchanged (focus ring, inverse destructive label, high contrast, reduced motion).

### 5.2 Controls (AWL-05)

| Recipe | Rule |
|---|---|
| Default (a bare `<button>`, `Button` with `secondary`) | fill 8 % over its surface, text 100 %, no border; hover fill 14 %; height 28 (24 when `sm`), radius 6, padding 0 10 |
| Primary | accent fill, on-accent label at 500; hover 90 % accent toward the base; used for the one main action of a surface (Run, Commit, Approve, Send) |
| Outline | transparent, 1 px control edge (3:1), text 100 %; hover fill 6 % |
| Ghost and icon buttons | transparent, text and icon 74 %; hover fill 6 % and 100 % |
| Tabs | pill tabs: text 74 %, selected fill 8 % and text 100 %, radius 8, height 28; no underline |
| Chips and badges | radius full or 4, fill 8 %, text 74 %, caption size |

### 5.3 Shell geometry (AWL-06; doc 65 numbers restored)

| AFW row | After this record | Before (DR-PX-2026-10-24-016) |
|---|---|---|
| AFW-A03 | agent list 260 px default, 210 to 400 | 264 |
| AFW-A04 | top bars 40 px, no bottom rule in the centre; the apps panel's tab row carries a stroke 8 % rule | 48 with a rule |
| AFW-A05 | navigation rows 30 px, radius 8, inset 8; agent rows two lines with a status glyph and a relative age | radius 6 |
| AFW-A07 | composer radius 12, editor surface, the 3:1 control edge (ZDX-03) and a 1 px shadow, single-line about 42 px, cap 840 | radius 16, input fill |

Panes are unframed: the agent list on `sidebar` with a 1 px stroke 8 % right edge, the conversation on `chrome`, the apps panel on `editor` with a 1 px stroke 8 % left edge; no radius, no gaps; the splitter is 4 px with a 2 px accent-free line (stroke 20 %) on hover and focus. The agent list carries the navigation entries in one column under its top bar: New task, Search, Fleet board, Worktrees, Automations, Customize and Dashboard, each a 30 px row with an icon and a label (the same commands, test ids and states; only their place changes). AFW-A01, A02, A06, A08, A09 and A10 keep their numbers.

### 5.4 Density (AWL-07 to AWL-09)

| ID | Rule |
|---|---|
| AWL-07 | Conversation: the user message is a bubble on `editor` with a stroke 8 % edge and radius 12; assistant text has no frame; a step group is a one-line disclosure row (icon, verb, count) without a box; a turn's footer (timing, tokens, checkpoint, copy, fork) is shown at opacity 0 and revealed on hover or keyboard focus within the turn (it stays in the accessibility tree and in the tab order); the column is centred with the 840 px cap. |
| AWL-08 | Composer: trays dock above it with a stroke 8 % edge and the sidebar surface; mode and model chips sit in its bottom row; the status row under it is one line (no wrap, ellipsis, the full text in its accessible name and title). |
| AWL-09 | Apps panel and screens: explanatory paragraphs are caption size at muted text; headings are sentence case caption at 600, never uppercase; the support-claim and language banners are one line at caption size on the surface (no tint), with the full text kept; Fleet columns and cards take stroke 8 % edges on `editor`. |

### 5.5 Not adopted

Any colour, icon font, icon name, logo, brand colour, theme file or string of the reference product; its vibrancy and window translucency (PX-094 owns native chrome); its information architecture beyond doc 65 (Modbit keeps Fleet, the approval deck, checkpoints, Evidence and its own names: "New task", not the reference product's label).

## 6. The gate

- **Tokens.** `packages/design-tokens` pins the section 5.1 tables in a unit test (exact values, both themes) and `apps/desktop/e2e/awl.spec.ts` reads the emitted variables from the real Electron build and compares them with the same tables.
- **Geometry and recipes.** `awl.spec.ts` measures, on the real app against a real Core at 1280 x 800: the agent list width (260 within 2), the top bar height (40), no pane radius and no gap between panes, the navigation rows (30), the computed recipe of a bare button and of each `Button` variant, the pill tabs, the composer radius, and a one-line status row; it fails on a framed pane, a primary-filled bare button or a wrapped status row.
- **Accessibility.** `accessible()` (axe, WCAG 2.x A/AA) on every region in the three themes, and the computed contrast test over every pair.
- **It can fail.** The retained mutation run sets one token, the top bar height and the bare button fill to their ZDL values and shows the gate failing each.
- **Supplemental.** Screenshots of each screen in light and dark are retained as a record; they are not proof.

## 7. Migration

None. No protocol, schema, event or persisted-format change. Theme preference values and `data-theme` are unchanged. A persisted agent-list width keeps its meaning; only the default moves from 264 to 260 px.

## 8. Compatibility

No SurfaceProtocol change; the preload, the CSP and the IPC channel set are untouched. Test ids, roles, accessible names and key maps are unchanged; the navigation entries keep their test ids in their new place.

## 9. Security impact

None. Static CSS and markup placement; no new dependency, no network fetch, no font file, no remote asset.

## 10. Test impact

Removed: `apps/desktop/e2e/fidelity.spec.ts`, `apps/desktop/e2e/zdl-tokens.spec.ts` and their support (the retired ZCode gate) and the ZDL parity unit tests in `packages/design-tokens`. Added: `awl.spec.ts` and the token pins. Changed: the geometry numbers of section 5.3 in `shell.spec.ts`, `shell.test.ts` and `gallery.spec.ts` and the widths derived from them; the window-root colour assertions that read the token source. A changed assertion outside these is a defect in the change, not a reconciliation.

## 11. Rollback

Revert the three row commits; data and protocol are untouched.

## 12. Risks and open items (not decided here)

- Windows and Linux rendering is CI_COMPATIBLE only.
- Native window chrome and translucency remain with PX-094.
- If the owner wants the reference product's exact colours, that is a change to the clean-room posture of doc 65 section 2 and needs its own record.
