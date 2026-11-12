import { test } from "node:test";
import assert from "node:assert/strict";
import { AWL_DARK, AWL_LIGHT, COLOR_ROLES, COMPACT, DENSITIES, GEOMETRY, TYPE, ZDL_TOKENS, PALETTES, PAIRS, checkPairs, conversationType, contrastRatio, densityMetrics, densityVariables, failingPairs, isDensity, resolveTheme, sharedVariables, themeVariables, tokensCss, type ColorRoles, type ThemeName } from "./index.ts";

const THEMES: ThemeName[] = ["light", "dark", "high-contrast"];

test("REQ-PX-044: every theme has exactly the same colour role set (a missing or extra role fails)", () => {
  for (const t of THEMES) {
    assert.deepEqual(Object.keys(PALETTES[t]).sort(), [...COLOR_ROLES].sort(), t);
    for (const [role, value] of Object.entries(PALETTES[t])) assert.match(value, /^#[0-9a-f]{6}$/, `${t}.${role} is an opaque hex colour`);
  }
  const keys = (t: ThemeName) => Object.keys(themeVariables(t)).sort();
  assert.deepEqual(keys("light"), keys("dark"));
  assert.deepEqual(keys("light"), keys("high-contrast"));
});

test("AFW-A12: every text/background and control pair meets WCAG AA in every theme", () => {
  assert.ok(PAIRS.length >= 70, "the pair list covers the primitives");
  for (const t of THEMES) {
    const bad = failingPairs(t).map((r) => `${t}: ${r.fg} on ${r.bg} (${r.use}) ${r.ratio.toFixed(2)} < ${r.min}`);
    assert.deepEqual(bad, []);
  }
});

test("AFW-A12: a token lowered below its ratio fails the contrast test", () => {
  const lowered: ColorRoles = { ...PALETTES.light, textMuted: "#9aa1ab" };
  const bad = failingPairs("light", lowered);
  assert.ok(bad.some((r) => r.fg === "textMuted"));
  // The old UI's primary button (white on #7aa2ff in the dark scheme) measured 2.49:1: it must fail.
  assert.ok(Math.abs(contrastRatio("#ffffff", "#7aa2ff") - 2.49) < 0.05);
  const old: ColorRoles = { ...PALETTES.dark, textOnAccent: "#ffffff", accent: "#7aa2ff" };
  assert.ok(failingPairs("dark", old).some((r) => r.fg === "textOnAccent" && r.bg === "accent"));
  // And Modbit's own primary button passes with room to spare.
  const primary = checkPairs("dark").find((r) => r.fg === "textOnAccent" && r.bg === "accent");
  assert.ok(primary && primary.ratio >= 7);
});

test("AFW-A12: the disabled text level is never used by a pair", () => {
  assert.ok(PAIRS.every((p) => p.fg !== "textDisabled" && p.bg !== "textDisabled"));
});

test("AFW-A11: the scale tokens follow the spec's grid", () => {
  const shared = sharedVariables();
  assert.equal(shared["--mb-space-1"], "4px");
  assert.equal(shared["--mb-space-0_5"], "2px");
  assert.equal(shared["--mb-space-1_5"], "6px");
  assert.equal(shared["--mb-radius-full"], "9999px");
  assert.equal(shared["--mb-control-md"], "28px");
  // DR-PX-2026-11-03-019 AWL-04 with the sizes of DR-PX-2026-11-11-021 MWS-03: chrome text is `text-ui-base` (14 px on 20 in
  // the default density), driven by the one --ui-font-size; the conversation body is 15 on 21.
  assert.equal(shared["--mb-type-chrome-size"], "var(--text-ui-base)");
  assert.equal(shared["--mb-type-chrome-line"], "calc(var(--text-ui-base) * 1.4286)");
  assert.equal(shared["--mb-type-body-size"], "var(--text-ui-lg)");
  assert.equal(shared["--mb-type-body-line"], "calc(var(--text-ui-lg) * 1.4)");
  assert.equal(shared["--mb-motion-fast"], "100ms");
  assert.equal(shared["--mb-press-scale"], "0.98");
  assert.equal(shared["--ui-font-size"], "14px");
  assert.equal(shared["--text-ui-base"], "var(--ui-font-size)");
  assert.equal(shared["--text-ui-xl"], "calc(var(--ui-font-size) + 4px)");
  assert.equal(shared["--text-ui-lg"], "calc(var(--ui-font-size) + 1px)");
  assert.equal(shared["--text-ui-caption"], "calc(var(--ui-font-size) - 1px)");
  assert.equal(shared["--text-ui-xs"], "calc(var(--ui-font-size) - 2px)");
  assert.equal(shared["--radius-xl"], "12px");
  assert.equal(shared["--radius-2xl"], "14px");
  assert.equal(shared["--mb-top-bar"], "45px");
  assert.match(shared["--mb-font-sans"]!, /system-ui/);
  assert.equal(conversationType(100).size, 15);
  assert.equal(conversationType(100).line, 21);
  assert.deepEqual(conversationType(100, "compact"), { size: 14, line: 20 });
  assert.equal(conversationType(1000).size, conversationType(140).size, "the Text size setting is clamped");
});

test("AFW-A14: the stylesheet collapses motion under prefers-reduced-motion and defines every theme", () => {
  const css = tokensCss();
  assert.match(css, /@media \(prefers-reduced-motion: reduce\)[\s\S]*animation: none !important[\s\S]*transition: none !important/);
  assert.match(css, /@media \(prefers-color-scheme: dark\)/);
  assert.match(css, /@media \(prefers-contrast: more\)/);
  assert.match(css, /@media \(forced-colors: active\)/);
  for (const t of THEMES) assert.ok(css.includes(`:root[data-theme="${t}"]`), t);
});

test("AFW-A13: Follow system resolves from what the OS reports; an explicit choice wins", () => {
  assert.equal(resolveTheme("system", { dark: true, moreContrast: false }), "dark");
  assert.equal(resolveTheme("system", { dark: false, moreContrast: false }), "light");
  assert.equal(resolveTheme("system", { dark: false, moreContrast: true }), "high-contrast");
  assert.equal(resolveTheme("light", { dark: true, moreContrast: true }), "light");
});

test("DR-PX-2026-11-03-019 AWL-01..03: the light and dark inputs are the record's section 5.1 values", () => {
  const pick = (i: typeof AWL_LIGHT) => ({ chrome: i.chrome, sidebar: i.sidebar, editor: i.editor, base: i.base, accent: i.accent, onAccent: i.onAccent });
  assert.deepEqual(pick(AWL_LIGHT), { chrome: "#f6f6f6", sidebar: "#f0f0f0", editor: "#fdfdfd", base: "#171717", accent: "#145fb4", onAccent: "#ffffff" });
  assert.deepEqual(pick(AWL_DARK), { chrome: "#131313", sidebar: "#181818", editor: "#1b1b1b", base: "#ececec", accent: "#64a8f0", onAccent: "#10151c" });
});

test("AWL-01..03: the role map and the --color-* names resolve to the AWL surfaces, ladder and accent", () => {
  for (const [theme, i] of [["light", AWL_LIGHT], ["dark", AWL_DARK]] as const) {
    const v = themeVariables(theme);
    // three achromatic surfaces, their values unchanged by DR-PX-2026-11-11-021: chrome, sidebar and editor
    assert.equal(v["--color-background-win-alt"], i.chrome, theme);
    assert.equal(v["--color-background"], i.chrome, theme);
    assert.equal(v["--color-sidebar"], i.sidebar, theme);
    assert.equal(v["--color-panel"], i.editor, theme);
    assert.equal(v["--color-card"], i.editor, theme);
    assert.equal(v["--mb-color-raised"], i.editor, theme);
    // one accent for the primary action, never black or white
    assert.equal(v["--color-primary"], i.accent, theme);
    assert.equal(v["--mb-color-accent"], i.accent, theme);
    assert.equal(v["--mb-color-focus"], i.accent, theme);
    // the AFW-A11 ladder from the base: stroke 12 %, hover 6 %, selected 8 %
    const base = parseInt(i.base.slice(1, 3), 16);
    assert.equal(v["--color-border"], `rgba(${base}, ${base}, ${base}, 0.12)`, theme);
    assert.equal(v["--color-hover"], `rgba(${base}, ${base}, ${base}, 0.06)`, theme);
    assert.equal(v["--color-selected"], `rgba(${base}, ${base}, ${base}, 0.08)`, theme);
    // achromatic: every surface has r = g = b
    for (const s of [i.chrome, i.sidebar, i.editor, i.base]) assert.ok(s.slice(1, 3) === s.slice(3, 5) && s.slice(3, 5) === s.slice(5, 7), `${theme} ${s}`);
    for (const t of ZDL_TOKENS) assert.ok(v[`--color-${t}`], `${theme} --color-${t}`);
  }
  assert.equal(GEOMETRY.topBar, 45);
  assert.equal(GEOMETRY.listDefault, 288);
});

test("MWS-02: one surface mapping: the conversation reads editor, the task list sidebar, and chrome is the fill; no role value moved", () => {
  const shared = sharedVariables();
  assert.equal(shared["--mb-surface-conversation"], "var(--color-panel)");
  assert.equal(shared["--mb-surface-list"], "var(--color-sidebar)");
  assert.equal(shared["--mb-surface-fill"], "var(--color-background)");
  for (const [theme, i] of [["light", AWL_LIGHT], ["dark", AWL_DARK]] as const) {
    const v = themeVariables(theme);
    assert.equal(v["--color-panel"], i.editor, `${theme}: the conversation surface is the editor input`);
    assert.equal(v["--color-sidebar"], i.sidebar, `${theme}: the list surface is the sidebar input`);
    assert.equal(v["--color-background"], i.chrome, `${theme}: the fill is the chrome input`);
    // The role map's values are the ones DR-PX-2026-11-03-019 gave them.
    assert.equal(v["--mb-color-bg"], i.chrome, theme);
    assert.equal(v["--mb-color-panel"], i.chrome, theme);
    assert.equal(v["--mb-color-raised"], i.editor, theme);
  }
});

test("DR-PX-2026-11-11-021 MWS-01: the measured geometry is section 5.1's, and what the record keeps is kept", () => {
  assert.deepEqual([GEOMETRY.listDefault, GEOMETRY.listMin, GEOMETRY.listMax], [288, 200, 420]);
  assert.equal(GEOMETRY.topBar, 45);
  assert.deepEqual([GEOMETRY.row, GEOMETRY.rowGap, GEOMETRY.rowRadius, GEOMETRY.rowInset], [32, 2, 8, 8]);
  assert.deepEqual([GEOMETRY.column, GEOMETRY.columnText, GEOMETRY.composerRadius], [768, 736, 14]);
  // The centre's minimum, the single-pane threshold, the rail and the window's minimum keep their numbers.
  assert.deepEqual([GEOMETRY.centerMin, GEOMETRY.singlePaneBelow, GEOMETRY.rail, GEOMETRY.panelMin], [424, 448, 40, 384]);
  assert.deepEqual(GEOMETRY.windowMin, { width: 900, height: 600 });
  const shared = sharedVariables();
  assert.equal(shared["--mb-row"], "32px");
  assert.equal(shared["--mb-row-gap"], "2px");
  assert.equal(shared["--mb-row-radius"], "8px");
  assert.equal(shared["--mb-row-inset"], "8px");
  assert.equal(shared["--mb-column"], "768px");
  assert.equal(shared["--mb-column-text"], "736px");
  assert.equal(shared["--mb-composer-radius"], "14px");
  // The superseded numbers of DR-PX-2026-11-03-019 are gone: a list at 260, a bar at 40, a column at 640.
  assert.notEqual(GEOMETRY.listDefault, 260);
  assert.notEqual(GEOMETRY.topBar, 40);
  assert.notEqual(GEOMETRY.column, 640);
});

test("MWS-03: Comfortable is the default and Compact is token overrides only: 13 on 18 chrome, rows 30, bars 40, the transcript 14 on 20", () => {
  assert.deepEqual([...DENSITIES], ["comfortable", "compact"]);
  assert.ok(isDensity("compact") && !isDensity("cosy") && !isDensity(null));
  // Comfortable: chrome 14, the transcript 15 on 21, the sizes of section 5.1.
  assert.deepEqual([TYPE.chrome.size, TYPE.body.size, TYPE.body.line], [14, 15, 21]);
  assert.deepEqual(densityVariables("comfortable"), {}, "the default density overrides nothing");
  assert.deepEqual(densityMetrics("comfortable"), { topBar: 45, row: 32, rowGap: 2, rowTwoLine: 46, groupHead: 38 });
  const c = densityVariables("compact");
  assert.equal(c["--ui-font-size"], "13px");
  assert.equal(c["--mb-type-chrome-line"], "calc(var(--text-ui-base) * 1.3846)");
  assert.equal(c["--mb-type-body-line"], "calc(var(--text-ui-lg) * 1.4286)");
  assert.equal(c["--mb-row"], "30px");
  assert.equal(c["--mb-top-bar"], "40px");
  assert.deepEqual([COMPACT.type.chrome.size, COMPACT.type.chrome.line, COMPACT.type.body.size, COMPACT.type.body.line], [13, 18, 14, 20]);
  assert.deepEqual(densityMetrics("compact"), { topBar: 40, row: 30, rowGap: 2, rowTwoLine: 44, groupHead: 32 });
  // Every role's size is its step from the one --ui-font-size in both densities, so a density never sets a size of its own.
  for (const [role, v] of Object.entries(COMPACT.type)) assert.equal(v.ui, TYPE[role as keyof typeof TYPE].ui, role);
  // A density overrides variables the shared block defines, never a new name and never a colour.
  const shared = sharedVariables();
  for (const name of Object.keys(c)) {
    assert.ok(name in shared, `${name} is a shared variable`);
    assert.ok(!name.includes("color"), name);
  }
  const css = tokensCss();
  assert.match(css, /:root\[data-density="compact"\] \{[^}]*--ui-font-size: 13px;[^}]*--mb-top-bar: 40px;[^}]*--mb-row: 30px;/);
  assert.equal(css.split("data-density").length - 1, 1, "one block selects on the density");
});

test("AWL gate: one ZDL value restored is detected (the gate can fail)", () => {
  // The retained mutation of the record's section 6: ZCode's black primary and 264 px list are not AWL.
  const v = themeVariables("light");
  assert.notEqual(v["--color-primary"], "#000000");
  assert.notEqual(GEOMETRY.listDefault, 264);
  const mutated = { ...v, "--color-primary": "#000000" };
  assert.notEqual(mutated["--color-primary"], AWL_LIGHT.accent);
});
