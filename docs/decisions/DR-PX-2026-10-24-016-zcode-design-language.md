---
id: DR-PX-2026-10-24-016
title: The desktop renderer adopts the ZCode design language (Apache-2.0), measured against a retained reference capture, with Modbit's accessibility floor unchanged
status: accepted
date: 2026-10-24
supersedes: scoped — the provenance clause of AFW-A11 and the numbers of AFW-A03, AFW-A04, AFW-A05 and AFW-A07 in doc 65 (section 5.3); nothing else
approved_by: owner, chat instruction of 2026-10-24 ("investigate how we can use same frontend of zcode into modbit with same design aesthetics and high fidelity ... create a goal and complete that autonomously")
---

# DR-PX-2026-10-24-016 — ZCode design language for the desktop renderer

> **Accepted 2026-10-24** on the owner's chat instruction. The approval covers the adoption of the design language below. It does **not** cover any relaxation of AFW-A12 (WCAG 2.2 AA), AFW-A13 (themes) or AFW-A14 (reduced motion, forced colours): section 5.2 lists every place where ZCode's own values would breach them and the smallest deviation that keeps the floor. A change that needs one of those three relaxed is outside this record and stops for the owner.

## 1. Trigger and evidence

The owner asked how the ZCode frontend (https://github.com/zai-org/ZCode) can be used in Modbit with the same design aesthetics and high fidelity, and asked for a goal to be set and completed autonomously.

**What ZCode is (read from the checkout, not assumed).** Licence Apache-2.0 (`LICENSE`; `NOTICE.md` is a product data-flow disclosure, not a licence notice). Upstream revision studied: `3c5b227649371507ddc4eb46ac5133afe6db1cab` ("chore: sync ZCode source", 2026-10-20, README: v3.16.2). The user's installed ZCode.app is 3.16.3; it was not driven, because ZCode holds a single-instance lock and the owner's instance was running (no data of the owner's was read). The design system is written down in `DESIGN.md` (656 lines) and implemented in `packages/ui/src/styles.css` (2,279 lines): semantic tokens, a dedicated `text-ui-*` scale driven by one `--ui-font-size`, a radius hierarchy, and two active themes, Zai Light and Zai Dark. The UI package (`packages/ui`) is 1,963 files and about 406,000 lines of React, Zustand stores and Tailwind v4 classes, bound to `@zcode/services`, `@zcode/rpc`, `@zcode/shared` and `@zcode/provider` (its own session protocol, accounts, billing, marketing, remote control).

**Consequence for the approach (measured by reading the dependency list and the import graph).** The UI cannot be lifted into Modbit as a package: its components read ZCode's stores and services, and Modbit's renderer speaks SurfaceProtocol to a different Core. What is portable is the design system: the token layer, the type scale, the radius and sizing rules, the shell geometry, and the visual recipe of each primitive.

**Reference capture (the method that makes "high fidelity" measurable).** ZCode's real primitives (`button`, `input`, `textarea`, `badge`, `switch`, `checkbox`, `radio-group`, `tabs`, `card`, `alert`, `progress`, `separator`, `kbd`, `tooltip`, `popover`, `dropdown-menu`, `select`, `dialog`, `alert-dialog`, `toast`, `button-group`) and its real `styles.css` were rendered unmodified (stubs only for i18n and the key-cap badge) with React 19.2.7, Tailwind 4.2.2, radix-ui 1.4.3 and lucide-react 1.17.0 inside **Electron 44.3.0, the engine Modbit ships**, in both Zai themes, and every `data-slot` element was probed for computed style and geometry. 340 custom properties per theme were resolved to canonical values. The retained capture lives in `evidence/zcode-ui/reference/`; the harness recipe is `tools/zcode-reference/`.

**What ZCode's own values do against Modbit's gate (computed from the capture; WCAG relative luminance, translucent tokens composited over the surface they sit on).**

| ZCode value used verbatim | Measured | Modbit rule it breaks |
|---|---|---|
| `foreground-subtle` as text (light) | 3.94–4.13:1 on sidebar/background/panel; dark 4.46–5.10:1 | AFW-A12 4.5:1 text |
| `foreground-subtlest` | 2.2–2.4:1 | allowed for disabled and decorative use only |
| `success` as text, light | 3.87–4.41:1 | 4.5:1 |
| `warning` as text or icon, light | 2.63–3.00:1 | 4.5:1 text, 3:1 icon |
| `destructive` as text, light | 3.96–4.51:1 | 4.5:1 |
| `git-modified`, `idle-task`, `terminal-blue`, light | 2.82–3.82:1 | 4.5:1 |
| white label on `warning` button, light | 3.00:1 | 4.5:1 |
| white label on `destructive` button, dark | 3.03:1 | 4.5:1 |
| `border` / `border-hover` / `input-border-focused` as a control edge | 1.25 / 1.39 / 1.39:1 light; 1.36 / 1.60 / 1.60:1 dark | AFW-A12 3:1 control edge |
| global rule `*:focus, *:focus-visible { outline: none !important; box-shadow: none !important }` | no visible keyboard focus on most controls | AFW-A12 visible 2 px focus ring |

ZCode's own `remote-foreground-muted` (`#595959` light, `#b3b3b3` dark) and its `*-surface-foreground` tokens already pass 4.5:1 (6.15–8.63:1 and 5.6–10.8:1) and exist in its design system for readable secondary and status text, so most of the floor is kept by using ZCode's own tokens in the right role.

## 2. Goal and exit criteria (the executable goal)

**Goal ZDL.** Modbit's desktop renderer presents ZCode's design language (Zai Light as Modbit "light", Zai Dark as Modbit "dark") with measured fidelity to ZCode's real components, with Modbit's information architecture, behaviour, test ids and accessibility floor unchanged. It is done when all of the following hold on one commit, each proven by a retained artifact:

1. **Token parity.** Every ZCode colour token the renderer uses is emitted with a canonical value equal to the retained capture in both themes, except the declared exceptions of section 5.2 (PX-141).
2. **Primitive parity.** For every specimen of section 6, computed colour, border, radius, font, spacing, height and shadow equal the capture within the tolerances of section 6, and the per-specimen pixel difference stays under the recorded bound, in light and dark (PX-142).
3. **Shell parity.** The shell is independent framed panels with the geometry of section 5.3; Fleet, conversation, composer, Review, Settings, dialogs and the palette are in the same language (PX-143).
4. **No regression.** The desktop unit suite and the whole desktop e2e suite pass against a real Core with the only changed assertions being the geometry numbers of section 5.3; `axe` passes on every region in light, dark and high contrast; the computed contrast test covers every pair in all three themes.
5. **Governance.** Graph rows PX-141..143 reach COMPLETE through `graph.py` with run and commit evidence, the manifests are resealed, notices ship, no release gate is attested.

Executable form: `python3 tools/graph.py goal RELEASE_ZERO` lists PX-141..143 in dependency order (DOC-PX-015 → PX-141 → PX-142 → PX-143); `node apps/desktop/e2e/...` specs named in the qualifications are the proof; `python3 tools/check_dossier.py --manifest` and `python3 tools/test_dossier.py` keep the dossier sound.

## 3. Options

| Option | Verdict |
|---|---|
| A. Transplant `@zcode/ui` and re-wire its services to Modbit | **Rejected.** 406 k lines bound to a different protocol, account and billing model; every ZCode store, hook and service would need a Modbit twin; Modbit would own a 1,963-file fork. |
| B. Adopt the design system (tokens, scale, geometry, primitive recipes, icon paths) into Modbit's own renderer and primitives; keep DOM, behaviour and test ids | **Chosen.** Smallest surface that yields the look; preserves the 50-spec e2e suite and the Core contract. |
| C. Tokens only | **Rejected.** Colour alone does not read as ZCode: radii, control heights, panel framing, type scale and icons carry the aesthetic. |
| B1. Add Tailwind v4, radix-ui and class-variance-authority to Modbit and copy ZCode's class strings | **Rejected for now.** It would add a native-binary build toolchain and a large runtime dependency set to a repository with a supply-chain install-script policy (docs 36, 70), and change every primitive's DOM. The same visuals are reachable with the existing token-driven CSS, and the retained capture detects drift. Revisit by a new record if upstream tracking becomes a requirement. |

## 4. Licence, attribution and marks

- Apache-2.0 section 4: the licence text ships (`third-party/zcode-ui/LICENSE`), each file that carries copied token values or recipes states its origin and that it was changed, and `third-party/zcode-ui/NOTICE.md` records the upstream revision. Copied: design token values, Tailwind-palette values those tokens resolve to (Tailwind CSS, MIT), and class recipes re-expressed as CSS. Not copied: any ZCode source file, component, asset, string of copy or image.
- Icons: Lucide path data (ISC licence) inlined as a typed table in `@modbit/ui`, with the ISC text in the same notice directory; no icon package is added. Modbit's own icons stay where Lucide has no equivalent (the status glyph shapes of AFW-A14 stay).
- Apache-2.0 section 6: no ZCode, Z.ai or Zai name, logo, wordmark or product string in the product, the bundle or the notices except in the attribution lines. Theme names in the UI stay Modbit's (Light, Dark, Follow system, High contrast). The Modbit brand mark is unchanged (AFW-A11's "Modbit chooses its own brand mark" holds).
- The clean-room posture of doc 65 section 2 concerns the Cursor reference product and is unchanged: nothing from it is used here, and no research note is touched.

## 5. Specification

### 5.1 What is adopted

| ID | Adopted | Source of truth |
|---|---|---|
| ZDL-01 | The semantic colour tokens of Zai Light and Zai Dark (names kept: `--color-background`, `--color-panel`, `--color-sidebar`, `--color-card`, `--color-popover`, `--color-input`, `--color-border`, `--color-hover`, `--color-selected`, `--color-foreground*`, `--color-primary*`, the state, diff, git, tooltip, tag and node tokens), emitted by the one typed function of `packages/design-tokens`, alpha preserved, plus the Modbit role layer (`--mb-color-*`) re-pointed at them so every existing rule inherits the palette. | retained capture `zcode-tokens-resolved.json` and `zcode-colors-rgba.json` |
| ZDL-02 | The `text-ui-*` scale from one `--ui-font-size` (default 14 px): xl +4, lg +2, base 0, caption −1, sm −2, xs −4; the Text size setting of AFW-A11 (a helper in `packages/design-tokens` that no screen exposes yet) will drive `--ui-font-size` (80–140 %) when a screen exposes it, and never the root font size. System UI font stack of the capture; mono stack of the capture. | `DESIGN.md` "Highest-priority UI constraint", capture |
| ZDL-03 | The radius hierarchy: first rounded container `xl` (12 px), then `lg` (8), `md` (6), `sm` (4); basic controls `lg` stepping down inside rounded parents; menus and selection overlays `lg` shells with `md` items; dialogs `2xl` (16); the approved `2xl` exceptions (main composer shell, toast); `full` only for deliberate pills and circles. | `DESIGN.md` "Radius" |
| ZDL-04 | Control sizing: heights 20/24/28/32 (`xs/sm/default/lg`), icon sizes 10/12/14/16, regular inputs 36, 4 px spacing grid, `gap-0.5` between menu rows. | `DESIGN.md` "Sizing", capture |
| ZDL-05 | Elevation and motion: overlays `shadow-md`, toast `shadow-lg`, no shadow elsewhere; 100 ms colour transitions; overlay fade and zoom of the capture; reduced motion collapses every duration (kept from AFW-A14). | capture |
| ZDL-06 | The shell as independent framed panels over a window-root surface: sidebar, conversation (with its header), optional terminal and optional side pane each in its own `bg-background`, `xl`-radius, `border-border` frame, separated by 4 px resizable gaps; 2 px resize indicator at 50 % subtlest. | `DESIGN.md` "Workspace layout", `docs/design/workspace-independent-panels.md` |
| ZDL-07 | Lucide path data for the icon set the renderer uses; stroke 2, 24 viewBox, `size-4` default. | lucide-react 1.17.0 |

### 5.2 Declared exceptions to verbatim adoption (each keeps AFW-A12/13/14)

The fidelity gate holds this list closed: a deviation not listed fails it, and a listed exception that no longer deviates fails it (stale).

| ID | Exception | Reason (measured) |
|---|---|---|
| ZDX-01 | Secondary text uses ZCode's `remote-foreground-muted` where ZCode writes `foreground-subtle`; `foreground-subtle` stays for icons and chevrons (3:1), `foreground-subtlest` for disabled and decorative only. | 3.94–4.13:1 text; AFW-A12 |
| ZDX-02 | Status text and status icons use the `*-surface-foreground` tokens (`success-surface-foreground`, `warning-surface-foreground`, `destructive-surface-foreground`, `progress-surface-foreground`) where ZCode uses the solid state colour; the solid colours stay for fills, dots that carry a glyph, and diff and git bars. Mode and tag hues that fall under 4.5:1 as text (`idle-task`, `git-modified`, `terminal-blue`) use their `*-node-foreground` or `*-surface-foreground` sibling. | table in section 1 |
| ZDX-03 | Interactive control edges (input, textarea, select trigger, outline button, checkbox, radio, switch track) use the `borderControl` role at 3:1 against every surface it sits on. Decorative edges (cards, panels, popovers, menus, dialogs, tooltips, separators, tab rules) keep ZCode's `--color-border`. | 1.25:1 (light) and 1.36:1 (dark) control edges; AFW-A12 |
| ZDX-04 | Keyboard focus is Modbit's visible 2 px outline with 2 px offset in the `focus` role; ZCode's global focus suppression is not adopted. | AFW-A12 |
| ZDX-05 | The label on the warning button (light) and on the destructive button (dark) is the dark `foreground-inverse` pair that reaches 4.5:1. | 3.00:1 and 3.03:1 |
| ZDX-06 | The high-contrast theme remains Modbit's own palette; `prefers-contrast: more` and `forced-colors` behave as before. ZCode has neither. | AFW-A13, AFW-A14 |
| ZDX-07 | The window-root surface is the opaque `background-win-alt` on every platform (ZCode uses vibrancy on macOS); panel radius is 12 px on every platform (ZCode varies it by OS version). Native chrome, translucency and per-OS radius belong to PX-094. | doc 78 ownership |
| ZDX-08 | Modbit's composer-mode hues (ask, plan, debug, multitask) keep their meaning and map to the AA text variants of ZCode's success, warning, destructive and violet families. | AFW-D04 |
| ZDX-09 | Every transition and animation collapses under `prefers-reduced-motion` (ZCode has no global rule). | AFW-A14 |

### 5.3 Geometry supersession (doc 65, scoped; the only rows this record changes)

| AFW row | Before | After | ZCode source |
|---|---|---|---|
| AFW-A03 | agent list 260 pt default, 210–400 | 264 px default; 210–400 range kept | `WORKSPACE_SIDEBAR_DEFAULT_WIDTH_PX = 264` |
| AFW-A04 | top bar 40 pt | 48 px header with 8 px padding and a 1 px bottom rule at `border/50` | `WorkspaceHeader.tsx` `h-12 … p-2 … border-border/50` |
| AFW-A05 | navigation rows 30 pt, radius about 8 pt | row radius `md` (6 px); row height from the ZCode task row (measured in PX-143 and recorded in the card) | `TaskListItem.tsx` |
| AFW-A07 | composer radius about 17 pt | `rounded-2xl` (16 px), `p-3`, `gap-3`, `border-input-border`, `bg-input`; the 840 px cap of Modbit stays | `ChatPromptEditor.tsx:410` |
| AFW-A11 | "No colour, theme file, icon, font or string of the reference product is used" | applies to the Cursor reference product only; ZCode tokens, scale and icon paths are adopted under section 4 | this record |

AFW-A01 (three regions), A02, A06 (centre 424 pt, panel ratio), A08 (single pane below 448 pt), A09 and A10 (settings structure) keep their numbers; where the new frames add 4 px gaps the 424 pt centre minimum is measured on the conversation frame's content box, so the e2e numbers do not move.

### 5.4 Not adopted

ZCode's name, logo, "Zai" labels and brand art; login, Coding Plan, rewards, marketing and billing surfaces; Highspeed styling; Rive, Lottie and particle effects; the workflow timeline and graph; mobile remote control; CJK-specific type handling and the `pinyin` search; ZCode's information architecture (its task list grouping, bots, whiteboard, preview panes). Modbit keeps its own agent list, Fleet, approval deck, checkpoints and apps panel; they are restyled, not replaced.

## 6. The fidelity gate (what "high fidelity" means here)

- **Reference.** `evidence/zcode-ui/reference/`: `zcode-computed.json` (probe records of 16 sheet-theme pairs), `zcode-tokens-resolved.json`, `zcode-colors-rgba.json`, screenshots, and `PROVENANCE.md` (upstream revision, package versions, Electron version, capture date). Regenerated by `tools/zcode-reference/` against a fresh upstream checkout; a changed upstream is a new capture and a reviewed diff, never a silent update.
- **Specimens.** Modbit renders the same specimen sheets (button matrix, badge, fields, switch, checkbox, radio, tabs, card, alert, progress, separator, kbd, button group, tooltip, popover, menu, select, dialog, alert dialog, toast) in its model-free state gallery, each element carrying a `data-zdl` key equal to the reference record's key.
- **Compared properties.** colour, background, the four border colours and widths, the four radii, font family, size, weight, line height and letter spacing, padding, gap, height and width, box shadow, opacity, text decoration, outline. Tolerances: colour ±1 per 8-bit channel and ±0.01 alpha, lengths ±0.5 px, strings exact. Pixel bound: at most 3 % of a specimen's pixels differ by more than 8 per channel (recorded with the run, tightened when the first run shows the real figure).
- **Exceptions.** `ZDX-01..09` as a data file; the gate reports each deviation it finds with the exception that covers it.
- **It can fail.** The retained mutation run changes one reference-equal colour, one radius and one font size and shows the gate failing each, then restores them.
- **Supplemental.** Screenshots of each screen in light and dark are retained as a record; they are not proof (AFW-K02 and the gallery rule of check G11 stand: a gallery or a screenshot never closes a row).

## 7. Migration

None. No protocol, schema, event or persisted-format change. The theme preference values (`system`, `light`, `dark`, `high-contrast`) and the `data-theme` attribute are unchanged; "light" now resolves to the Zai Light values and "dark" to Zai Dark. Persisted panel widths keep their meaning; only the default of the agent list moves from 260 to 264 px.

## 8. Compatibility

No SurfaceProtocol change; the preload, the CSP and the IPC channel set are untouched. Test ids, roles, accessible names and keyboard maps are unchanged.

## 9. Security impact

None. Static CSS and inline SVG path data; no new runtime or build dependency, no network fetch, no font file, no remote asset. The CSP needs no relaxation.

## 10. Test impact

Added: the parity, specimen and shell specs and the contrast pairs that cover the new tokens; changed: the geometry numbers named in section 5.3 in `shell.spec.ts` and `gallery.spec.ts`, and nothing else. A changed assertion outside section 5.3 is a defect in the change, not a reconciliation.

## 11. Rollback

Revert the three row commits; the data and the protocol are untouched. The previous palette and primitives are one `git revert` away and the e2e suite verifies the result.

## 12. Risks and open items (not decided here)

- Windows and Linux rendering is CI_COMPATIBLE only; the capture and the gate run on macOS. ZCode's per-OS panel radii and caption controls are not reproduced (ZDX-07).
- Upstream drift: ZCode ships weekly; the capture pins one revision. Tracking upstream is a separate record (B1 above).
- If the owner prefers ZCode's faint control edges over AFW-A12's 3:1 (ZDX-03), that is a relaxation and needs its own record; it would remove the largest visible deviation from ZCode.
- The user's installed ZCode 3.16.3 was not measured against the 3.16.2 source; the specimens are the real components at the pinned revision, not a screenshot of the installed app.
